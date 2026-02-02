/// Dev fee enforcement module for SolBeast
/// This module handles dev fee collection from transactions
///
/// NOTE: Magic code validation functions are currently disabled pending
/// deployment of the smart contract-based anti-copycat protection system.
/// These functions will be re-enabled once the smart contract is deployed:
///   - generate_magic_codes()
///   - validate_magic_codes()
///   - get_dev_fee_program_id()
///   - build_dev_fee_instruction_data()
///   - verify_dev_fee_in_instructions()
#[allow(deprecated)]
use solana_program::{pubkey::Pubkey, instruction::Instruction, system_instruction};
use crate::settings::Settings;
use std::str::FromStr;

// Dev wallet address (will be replaced with actual address before production)
const DEV_WALLET: &str = "11111111111111111111111111111112";

/// Get the dev wallet public key
pub fn get_dev_wallet() -> Result<Pubkey, Box<dyn std::error::Error + Send + Sync>> {
    Pubkey::from_str(DEV_WALLET)
        .map_err(|e| format!("Invalid dev wallet: {}", e).into())
}

/// Calculate dev fee from SOL amount (in lamports) using percent (e.g., 2.0 for 2%)
pub fn calculate_dev_fee(amount_lamports: u64, percent: f64) -> u64 {
    if percent <= 0.0 {
        return 0;
    }
    let fee = ((amount_lamports as f64) * (percent / 100.0)).round() as u64;
    if fee == 0 { 1 } else { fee }
}

/// Add dev fee instruction to a list of instructions
/// This creates a system transfer instruction for the configured percentage
/// and adds it to the instruction list
pub fn add_dev_fee_to_instructions(
    instructions: &mut Vec<Instruction>,
    payer: &Pubkey,
    transaction_amount_lamports: u64,
    _op_type: u8, // 0 = buy, 1 = sell (reserved for future use)
    settings: &Settings,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Determine dev wallet: prefer configured address, otherwise fallback
    let dev_wallet = if let Some(addr) = &settings.dev_wallet_address {
        Pubkey::from_str(addr)?
    } else {
        get_dev_wallet()? 
    };
    let fee_amount = calculate_dev_fee(transaction_amount_lamports, settings.dev_fee_percent);

    // Create system transfer for dev fee
    let transfer_instruction = system_instruction::transfer(
        payer,
        &dev_wallet,
        fee_amount,
    );

    // Add transfer instruction at the beginning (before main operations)
    instructions.insert(0, transfer_instruction);

    log::info!("Added dev fee instruction: {} lamports ({} SOL) - {}% of {} lamports", 
        fee_amount, 
        fee_amount as f64 / 1_000_000_000.0,
        settings.dev_fee_percent,
        transaction_amount_lamports
    );

    Ok(())
}
