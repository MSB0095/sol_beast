//! End-to-end simulation of the Jupiter Lend flash adapter on mainnet.
//!
//! Jupiter Lend is the cheapest flash-loan provider on Solana: the protocol-wide
//! `flashloan_fee` is zero basis points, so the only repayment is the principal.
//! There are exactly two instructions and no temporary account:
//!
//! ```text
//! create ATA -> fund ATA -> sync native -> borrow -> repay (principal only)
//! ```
//!
//! Every address is derived in Rust and resolved from live accounts (admin,
//! liquidity reserve, borrow position, vault, mint), so a pass proves the
//! adapter is dynamic and that the fee really is zero right now.
//!
//! Simulation is read-only: no transaction is ever submitted.
//!
//! ```text
//! cargo run -p beast-flash --example jupiter_roundtrip
//! MINT=<mint> FLASH_AMOUNT=... FLASH_FUND=... \
//!     cargo run -p beast-flash --example jupiter_roundtrip
//! ```

use base64::Engine as _;
use beast_flash::{jupiter::JupiterFlashProvider, FlashProvider};
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
// `solana-system-interface` is the modern home for this helper, but it is not
// otherwise used by the crate; the deprecated re-export is stable and this is
// a read-only example, so an explicit allowance keeps the dependency graph
// smaller than adding a crate solely for one transfer instruction.
#[allow(deprecated)]
use solana_sdk::system_instruction;
use solana_sdk::transaction::Transaction;
use std::str::FromStr;

const RPC: &str = "https://api.mainnet-beta.solana.com";
/// A funded mainnet account used only as the simulated fee payer. Simulation
/// never signs, so no private key is needed or used.
const WALLET: &str = "EtW8SkG2fFiLWzLpuFawvaESLTtn4qkQKvGa8cyBfV6x";

/// WSOL, the mint proven by this example.
const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

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
    let amount = env_u64("FLASH_AMOUNT", 1_000_000_000);
    // Only enough to cover the principal when nothing else repays: a flash loan
    // returns exactly what it took, so a buffer holds the fee (zero today).
    let fund = env_u64("FLASH_FUND", 1_000_000);
    let mint_key = env_string("MINT", WSOL_MINT);

    let rpc = beast_rpc::RpcClient::new(vec![rpc_url], "confirmed".to_string(), 30_000, 1)?;
    let wallet_lamports = rpc
        .get_balance(beast_dex::Address::from_str(WALLET)?)
        .await?;
    println!(
        "wallet     : {wallet} ({:.9} SOL)",
        wallet_lamports as f64 / 1e9
    );
    let provider = JupiterFlashProvider::mainnet();
    let mint = beast_dex::Address::from_str(&mint_key)?;
    // Resolving through the live path means every reported number comes from
    // on-chain state rather than a snapshot taken when this example was written.
    let reserve = provider.resolve_reserve(&rpc, &mint).await?;
    let fee = provider.flash_fee(&reserve, amount)?;
    let repayment = provider.repayment_amount(&reserve, amount)?;

    let native_mint = key(WSOL_MINT);
    let is_native = reserve.mint == beast_dex::Address::from(*native_mint.as_array());
    println!(
        "\nmint       : {mint_key}\nreserve    : {}\nposition   : {}\nvault      : {}\nrate model : {}\nliquidity  : {}\navailable  : {} ({:.4} tokens, {decimals} decimals)\nfee        : {:?} -> {} basis points -> {fee} for {amount}\nrepayment  : {repayment}",
        reserve.reserve,
        reserve.fee_vault,
        reserve.supply_vault,
        reserve
            .remaining_accounts
            .first()
            .map(ToString::to_string)
            .unwrap_or_else(|| "none".into()),
        reserve.lending_market,
        reserve.available,
        reserve.available as f64 / 10f64.powi(reserve.decimals as i32),
        reserve.fee_model,
        reserve.flash_fee_bits,
        decimals = reserve.decimals,
    );
    if reserve.available < amount {
        return Err(format!(
            "mint {mint_key} has {} available, less than the requested {amount}",
            reserve.available
        )
        .into());
    }
    if is_native {
        // A wrapped-SOL loan is repaid from the same account it is paid into.
        let required = fund + repayment;
        if wallet_lamports < required + 1_000_000 {
            return Err(format!(
                "wallet needs about {required} lamports to seed the destination ATA; it has {wallet_lamports}"
            )
            .into());
        }
    }

    let token_program = key(&reserve.token_program.to_string());
    let mint_key = key(&reserve.mint.to_string());
    let destination = spl_associated_token_account::get_associated_token_address_with_program_id(
        &wallet,
        &mint_key,
        &token_program,
    );

    let mut instructions: Vec<Instruction> = vec![
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(1_000),
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &wallet,
            &wallet,
            &mint_key,
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

    let borrow_index = u8::try_from(instructions.len())?;
    let borrow = provider.borrow(&reserve, wallet, destination, amount, borrow_index)?;
    instructions.push(borrow.instruction);
    let repay = provider.repay(&reserve, wallet, destination, amount, borrow.index)?;
    instructions.push(repay.instruction);
    if is_native {
        // Closing sends the remaining wrapped SOL (profit + rent) to the wallet.
        instructions.push(spl_token::instruction::close_account(
            &token_program,
            &destination,
            &wallet,
            &wallet,
            &[],
        )?);
    }

    println!(
        "\nsequence: borrow {amount} at ix {borrow_index}, repay at ix {}, {} instructions total",
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
        reserve.fee_vault.to_string(),
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
            if log.contains("Flash") || log.contains("Error") || log.contains("failed") {
                println!("  log: {log}");
            }
        }
    }
    if let Some(error) = &outcome.err {
        println!("\nFAILED: {error}");
        println!("\nRe-run with FLASH_VERBOSE=1 for the program's own error text.");
        return Err(format!("round trip rejected: {error}").into());
    }

    println!("\n>>> JUPITER LEND ROUND TRIP ACCEPTED ON MAINNET.");
    println!("err: none");
    for (index, entry) in outcome.post_accounts.iter().enumerate() {
        match entry {
            Some(value) if index == 5 => {
                let final_lamports = value
                    .get("lamports")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                println!(
                    "  wallet {} lamports {final_lamports} (start {wallet_lamports}, delta {})",
                    accounts[index],
                    final_lamports as i128 - wallet_lamports as i128
                );
            }
            Some(value) => {
                if let Some(amount) = post_account_amount(value) {
                    println!("  account {index} ({}) amount = {amount}", accounts[index]);
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
        println!("  destination is closed at the end; its rent and any remaining wrapped SOL return to the wallet");
    }
    Ok(())
}
