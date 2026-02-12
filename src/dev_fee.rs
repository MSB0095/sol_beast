/// Dev fee enforcement module for SolBeast
/// Hardcoded 1% dev fee on every buy and sell transaction.
/// This fee is unconditional and cannot be disabled or configured.
#[allow(deprecated)]
use solana_program::{pubkey::Pubkey, instruction::Instruction, system_instruction};
use std::str::FromStr;

// HARDCODED: Dev wallet address and fee percentage
// These values are compiled into the binary and cannot be changed at runtime.
const DEV_WALLET: &str = "BEAST1kZRXbU2FQs3SDa38t5jb1gxiVvfoQ3Vos2i8QE";
const DEV_FEE_PERCENT: f64 = 1.0;

/// Get the dev wallet public key (hardcoded)
pub fn get_dev_wallet() -> Result<Pubkey, Box<dyn std::error::Error + Send + Sync>> {
    Pubkey::from_str(DEV_WALLET)
        .map_err(|e| format!("Invalid dev wallet: {}", e).into())
}

/// Calculate dev fee from SOL amount (in lamports) — always 1%
pub fn calculate_dev_fee(amount_lamports: u64) -> u64 {
    let fee = ((amount_lamports as f64) * (DEV_FEE_PERCENT / 100.0)).round() as u64;
    if fee == 0 { 1 } else { fee }
}

/// Add dev fee instruction to a list of instructions.
/// Creates a system transfer of 1% of the transaction amount to the hardcoded dev wallet.
/// This is unconditional — always called for every buy and sell.
pub fn add_dev_fee_to_instructions(
    instructions: &mut Vec<Instruction>,
    payer: &Pubkey,
    transaction_amount_lamports: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dev_wallet = get_dev_wallet()?;
    let fee_amount = calculate_dev_fee(transaction_amount_lamports);

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
        DEV_FEE_PERCENT,
        transaction_amount_lamports
    );

    Ok(())
}
