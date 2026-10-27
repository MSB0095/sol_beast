//! End-to-end simulation of the Marginfi v2 flash adapter on mainnet.
//!
//! Marginfi has no dedicated flash-loan instruction. A flash loan is a flag on
//! a temporary Marginfi account:
//!
//! ```text
//! create ATA -> fund ATA -> sync native
//!     -> initialize_account_pda -> start_flashloan
//!     -> borrow -> repay_all -> end_flashloan -> close_account
//!     -> close ATA
//! ```
//!
//! The account PDA, group, bank, vault, oracle and fee data all come from live
//! accounts, so a pass proves the adapter is dynamic. Simulation is read-only:
//! no transaction is ever submitted.
//!
//! ```text
//! cargo run -p beast-flash --example marginfi_roundtrip
//! RPC_URL=https://... RESERVE=<bank> FLASH_AMOUNT=... FLASH_FUND=... \
//!     cargo run -p beast-flash --example marginfi_roundtrip
//! ```

use base64::Engine as _;
use beast_flash::{
    marginfi::MarginfiFlashProvider, FlashLifecycle, FlashProvider, MARGINFI_WSOL_BANK,
};
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
#[allow(deprecated)]
use solana_sdk::system_instruction;
use solana_sdk::transaction::Transaction;
use std::str::FromStr;

const RPC: &str = "https://api.mainnet-beta.solana.com";
/// A funded mainnet account used only as the simulated fee payer. Simulation
/// never signs, so no private key is needed or used.
const WALLET: &str = "EtW8SkG2fFiLWzLpuFawvaESLTtn4qkQKvGa8cyBfV6x";

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_string(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn key(value: &str) -> Pubkey {
    Pubkey::from_str(value).expect("valid base58")
}

fn token_amount(data: &[u8]) -> u64 {
    // SPL token account: mint(32) owner(32) amount(u64) ...
    data.get(64..72)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
        .unwrap_or(0)
}

fn post_account_amount(value: &serde_json::Value) -> Option<u64> {
    let data = value.get("data")?.get(0)?.as_str()?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    Some(token_amount(&raw))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wallet = key(WALLET);
    let rpc_url = env_string("RPC_URL", RPC);
    let reserve_argument = env_string("RESERVE", MARGINFI_WSOL_BANK);
    let amount = env_u64("FLASH_AMOUNT", 100_000_000);
    let fund = env_u64("FLASH_FUND", 2_000_000);

    let rpc = beast_rpc::RpcClient::new(vec![rpc_url], "confirmed".to_string(), 30_000, 1)?;
    let wallet_lamports = rpc
        .get_balance(beast_dex::Address::from_str(WALLET)?)
        .await?;
    println!(
        "wallet     : {wallet} ({:.9} SOL)",
        wallet_lamports as f64 / 1e9
    );
    let provider = MarginfiFlashProvider::mainnet();
    let rent = provider.temporary_rent_lamports();
    if wallet_lamports < fund.saturating_add(rent).saturating_add(1_000_000) {
        return Err(format!(
            "wallet needs about {} lamports for the account rent float, destination funding and fees; it has {wallet_lamports}",
            fund.saturating_add(rent).saturating_add(1_000_000)
        )
        .into());
    }

    let reserve_key = beast_dex::Address::from_str(&reserve_argument)?;
    let account = rpc
        .get_account_info(reserve_key)
        .await?
        .ok_or("bank account not found")?;
    let reserve = MarginfiFlashProvider::decode_reserve(&reserve_key, &account.account.data)?;

    println!(
        "\nbank       : {reserve_key}\n  mint     : {}\n  group    : {}\n  vault    : {}\n  authority: {}\n  oracle   : {:?}\n  available: {} ({:.4} tokens)\n  decimals : {}\n  fee      : {:?} -> {} for {amount}\n  rent     : {rent}",
        reserve.mint,
        reserve.lending_market,
        reserve.supply_vault,
        reserve.lending_market_authority,
        reserve.remaining_accounts.get(1),
        reserve.available,
        reserve.available as f64 / 10f64.powi(reserve.decimals as i32),
        reserve.decimals,
        reserve.fee_model,
        reserve.flash_fee(amount)?,
    );

    // Probe the account PDA exactly like the executor: index 0 first, then
    // later indexes if a stale account already exists.
    let group = beast_flash::pubkey(&reserve.lending_market);
    let mut lifecycle = None;
    for index in 0..32u16 {
        let Some(address) = provider.lifecycle_account(&reserve, wallet, index) else {
            break;
        };
        if rpc.get_account_info(address).await?.is_none() {
            lifecycle = Some(FlashLifecycle {
                account: Some(address),
                account_index: index,
                third_party_id: 0,
            });
            break;
        }
    }
    let lifecycle = lifecycle.ok_or("no unused Marginfi account index")?;
    println!(
        "\nlifecycle  : {} (index {}, group {group})",
        lifecycle.account.expect("set above"),
        lifecycle.account_index
    );

    let token_program = key(&reserve.token_program.to_string());
    let mint = key(&reserve.mint.to_string());
    let destination = spl_associated_token_account::get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &token_program,
    );

    let native_mint = key("So11111111111111111111111111111111111111112");
    let is_native = mint == native_mint;
    let mut instructions: Vec<Instruction> = vec![
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(1_000),
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &wallet,
            &wallet,
            &mint,
            &token_program,
        ),
    ];
    if fund > 0 && is_native {
        instructions.push(system_instruction::transfer(&wallet, &destination, fund));
        instructions.push(spl_token::instruction::sync_native(
            &token_program,
            &destination,
        )?);
    }

    let mut pre = provider.pre_borrow(&reserve, wallet, destination, amount, &lifecycle)?;
    instructions.append(&mut pre);
    let borrow_index = u8::try_from(instructions.len())?;
    let borrow = provider.borrow_with_lifecycle(
        &reserve,
        wallet,
        destination,
        amount,
        borrow_index,
        &lifecycle,
    )?;
    instructions.push(borrow.instruction);
    let repay = provider.repay_with_lifecycle(
        &reserve,
        wallet,
        destination,
        amount,
        borrow.index,
        &lifecycle,
    )?;
    instructions.push(repay.instruction);
    let mut post = provider.post_repay(&reserve, wallet, destination, amount, &lifecycle)?;
    instructions.append(&mut post);
    instructions.push(spl_token::instruction::close_account(
        &token_program,
        &destination,
        &wallet,
        &wallet,
        &[],
    )?);
    provider.finalize(&mut instructions, &lifecycle)?;

    println!(
        "\nsequence: borrow {amount} at ix {borrow_index}, repay_all at ix {}, {} instructions total",
        borrow_index + 1,
        instructions.len()
    );
    if std::env::var("FLASH_VERBOSE").is_ok() {
        for (index, instruction) in instructions.iter().enumerate() {
            println!(
                "  [{index}] program {} data {} bytes, {} accounts",
                instruction.program_id,
                instruction.data.len(),
                instruction.accounts.len()
            );
        }
    }

    let transaction = Transaction::new_with_payer(&instructions, Some(&wallet));
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(bincode::serialize(&transaction)?);
    println!("simulating {} bytes ...", encoded.len());

    let accounts = vec![
        destination.to_string(),
        reserve.reserve.to_string(),
        reserve.supply_vault.to_string(),
        reserve.lending_market.to_string(),
        lifecycle.account.expect("set above").to_string(),
        wallet.to_string(),
    ];
    let outcome = rpc
        .simulate_transaction_with_accounts(&encoded, &accounts, true)
        .await?;
    if std::env::var("FLASH_VERBOSE").is_ok() {
        for log in &outcome.logs {
            println!("  log: {log}");
        }
    } else {
        for log in &outcome.logs {
            if log.contains("Flash")
                || log.contains("Error")
                || log.contains("failed")
                || log.contains("insufficient")
            {
                println!("  log: {log}");
            }
        }
    }
    if let Some(error) = &outcome.err {
        println!("\nFAILED: {error}");
        println!("\nRe-run with FLASH_VERBOSE=1 for the program's own error text.");
        return Err(format!("round trip rejected: {error}").into());
    }

    println!("\n>>> MARGINFI ROUND TRIP ACCEPTED ON MAINNET.");
    println!("err: none");
    for (index, entry) in outcome.post_accounts.iter().enumerate() {
        match entry {
            Some(value) => {
                if index == 5 {
                    let final_lamports = value
                        .get("lamports")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    println!(
                        "  wallet {} lamports {final_lamports} (start {wallet_lamports}, delta {})",
                        accounts[index],
                        final_lamports as i128 - wallet_lamports as i128
                    );
                } else if let Some(token_amount) = post_account_amount(value) {
                    println!(
                        "  account {index} ({}) amount = {token_amount}",
                        accounts[index]
                    );
                } else if index == 4 {
                    println!("  account {index} ({}) closed", accounts[index]);
                } else {
                    println!(
                        "  account {index} ({}) is not a token account",
                        accounts[index]
                    );
                }
            }
            None => println!(
                "  account {index} ({}) missing from simulation",
                accounts[index]
            ),
        }
    }
    if is_native {
        println!(
            "  destination is closed at the end; its rent and any remaining wrapped SOL return to the wallet"
        );
    }
    Ok(())
}
