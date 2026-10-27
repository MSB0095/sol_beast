//! End-to-end simulation of the Kamino flash adapter on mainnet.
//!
//! The reference transaction only proves the *borrow* (it failed in a swap
//! before repaying). This example proves the full `borrow -> repay` round trip
//! by funding the destination token account first, because Kamino charges its
//! flash fee from that account *in addition to* the principal repayment.
//!
//! It deliberately resolves the lending market, market authority, vaults and
//! token program from the live reserve account, so a pass proves the adapter is
//! dynamic and not pinned to one hardcoded market.
//!
//! Simulation is read-only: no transaction is ever submitted.
//!
//! ```text
//! cargo run -p beast-flash --example roundtrip
//! RESERVE=<reserve> FLASH_AMOUNT=... FLASH_FUND=... cargo run -p beast-flash --example roundtrip
//! ```

use base64::Engine as _;
use beast_flash::{kamilend::KaminoFlashProvider, FlashProvider};
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
const REFERENCE: &str =
    "3ofZkLX2sXZgXujH5UPDMRa9h5JTQcmK7U9LKENFDTYhECW92hWBT6igpqCZuF2vaMsqbinpUnApzE4rsqcQxKor";
const WALLET: &str = "EtW8SkG2fFiLWzLpuFawvaESLTtn4qkQKvGa8cyBfV6x";
/// The deepest WSOL reserve on Kamino's main market (`7u3...`).
const DEFAULT_RESERVE: &str = "d4A2prbA2whesmvHaL88BH6Ewn5N4bTSU2Ze8P6Bc4Q";

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
    let reserve_argument = env_string("RESERVE", DEFAULT_RESERVE);
    let amount = env_u64("FLASH_AMOUNT", 1_000_000_000);
    let fund = env_u64("FLASH_FUND", 20_000_000);

    let rpc = beast_rpc::RpcClient::new(vec![RPC.to_string()], "confirmed".to_string(), 20_000, 1)?;
    let wallet_lamports = rpc
        .get_balance(beast_dex::Address::from_str(WALLET)?)
        .await?;
    println!(
        "wallet     : {wallet} ({:.9} SOL)",
        wallet_lamports as f64 / 1e9
    );
    println!("reference  : {REFERENCE} (borrow succeeded, swap failed)");
    if wallet_lamports < fund.saturating_add(20_000_000) {
        return Err(format!(
            "wallet needs about {} lamports to fund the destination plus fees; it has {wallet_lamports}",
            fund.saturating_add(20_000_000)
        )
        .into());
    }

    // Resolve the live reserve through RPC so the adapter decodes real state.
    let reserve_key = beast_dex::Address::from_str(&reserve_argument)?;
    let account = rpc
        .get_account_info(reserve_key)
        .await?
        .ok_or("reserve account not found")?;
    let reserve = KaminoFlashProvider::decode_reserve(&reserve_key, &account.account.data)?;
    let provider = KaminoFlashProvider::mainnet();
    let expected_authority =
        provider.lending_market_authority(&beast_flash::pubkey(&reserve.lending_market));
    if expected_authority.to_bytes() != *reserve.lending_market_authority.as_bytes() {
        return Err("derived market authority does not match the decoded reserve".into());
    }

    println!(
        "\nreserve    : {reserve_key}\n  mint     : {}\n  market   : {}\n  authority: {}\n  vault    : {}\n  available: {} ({:.4} tokens)\n  decimals : {}\n  token prg: {}\n  fee      : {} bits ({:.6} ppm) -> {} for {amount}",
        reserve.mint,
        reserve.lending_market,
        reserve.lending_market_authority,
        reserve.supply_vault,
        reserve.available,
        reserve.available as f64 / 10f64.powi(reserve.decimals as i32),
        reserve.decimals,
        reserve.token_program,
        reserve.flash_fee_bits,
        reserve.fee_ppm().unwrap_or(f64::NAN),
        reserve.flash_fee(amount)?,
    );

    // The destination is the wallet's associated token account for the reserve
    // mint. Deriving it is what lets the same cycle work for every asset.
    let token_program = key(&reserve.token_program.to_string());
    let mint = key(&reserve.mint.to_string());
    let destination = spl_associated_token_account::get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &token_program,
    );

    // Fund the destination so the fee has something to draw from. A freshly
    // created ATA has zero tokens, and Kamino debits principal *and* fee from
    // it, so an unfunded ATA can never complete a round trip. Only wrapped SOL
    // (native mint) can be funded from lamports and synced this way; for every
    // other mint the account must already hold tokens, which is true for an
    // arbitrage cycle because the final swap returns them.
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

    let borrow_index = u8::try_from(instructions.len())?;
    let borrow = provider.borrow(&reserve, wallet, destination, amount, borrow_index)?;
    let repay = provider.repay(&reserve, wallet, destination, amount, borrow.index)?;
    println!(
        "\nsequence: {} preamble instructions, borrow {amount} at ix {borrow_index}, repay principal at ix {}",
        borrow_index,
        borrow_index + 1
    );
    if std::env::var("FLASH_VERBOSE").is_ok() {
        for (label, instruction) in [
            ("borrow", &borrow.instruction),
            ("repay", &repay.instruction),
        ] {
            println!("{label} accounts:");
            for (index, meta) in instruction.accounts.iter().enumerate() {
                println!(
                    "  [{index}] {} signer={} writable={}",
                    meta.pubkey, meta.is_signer, meta.is_writable
                );
            }
        }
    }
    instructions.push(borrow.instruction);
    instructions.push(repay.instruction);

    let transaction = Transaction::new_with_payer(&instructions, Some(&wallet));
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(bincode::serialize(&transaction)?);
    println!("simulating {} bytes ...", encoded.len());

    let accounts = vec![
        destination.to_string(),
        reserve.supply_vault.to_string(),
        reserve.fee_vault.to_string(),
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
        println!(
            "\nRe-run with FLASH_VERBOSE=1 for the program's own error text. A bare \
             \"insufficient funds\" means the funded buffer was below the fee."
        );
        return Err(format!("round trip rejected: {error}").into());
    }

    println!("\n>>> ROUND TRIP ACCEPTED ON MAINNET.");
    println!("err: none");
    let mut final_destination = None;
    for (index, entry) in outcome.post_accounts.iter().enumerate() {
        match entry {
            Some(value) => {
                if let Some(amount) = post_account_amount(value) {
                    println!("  account {index} ({}) amount = {amount}", accounts[index]);
                    if index == 0 {
                        final_destination = Some(amount);
                    }
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
        if let Some(final_amount) = final_destination {
            let expected = fund.saturating_add(amount).saturating_sub(amount);
            let fee = expected.saturating_sub(final_amount);
            println!(
                "\nfee actually charged: {fee} lamports ({:.6}% of {amount}); expected {}",
                fee as f64 * 100.0 / amount as f64,
                reserve.flash_fee(amount)?
            );
            if fee != reserve.flash_fee(amount)? {
                return Err("charged fee does not match the decoded 2^60 fraction".into());
            }
        }
    }
    println!(
        "To make this round trip profitable, the swap inside it must return principal + fee = {}.",
        reserve.repayment_amount(amount)?
    );
    Ok(())
}
