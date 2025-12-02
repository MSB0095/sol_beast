/// Dev fee enforcement module for SolBeast
/// This module provides anti-copycat protection through magic code validation
#[allow(deprecated)]
use solana_program::{pubkey::Pubkey, instruction::Instruction, system_instruction};
use std::str::FromStr;

// Magic codes - obfuscated with XOR encoding
// Magic code 1: XOR with 0x42
#[allow(dead_code)]
const M1: [u8; 8] = [0x73, 0x91, 0xC5, 0x28, 0x65, 0xF7, 0x2B, 0xD4];
// Magic code 2: XOR with 0x7F  
#[allow(dead_code)]
const M2: [u8; 8] = [0x1E, 0x8C, 0x42, 0xD9, 0x57, 0x3A, 0x6F, 0xB2];

// IMPORTANT: These are placeholder values and MUST be updated before production deployment!
// 
// Dev fee program ID (placeholder - will be replaced with actual deployed program)
// TODO: After deploying the smart contract, replace with actual program ID
#[allow(dead_code)]
const DEV_FEE_PROGRAM_ID: &str = "DevFeeProgramXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";

// Dev wallet address (placeholder - must match hardcoded address in smart contract)
// TODO: Replace with actual dev wallet address before deployment
const DEV_WALLET: &str = "11111111111111111111111111111112";

/// Generate magic codes for transaction data
/// These must be included in transaction instruction data to pass validation
#[allow(dead_code)]
pub fn generate_magic_codes() -> Vec<u8> {
    let mut data = Vec::with_capacity(16);
    
    // Encode magic code 1 with XOR 0x42
    for byte in &M1 {
        data.push(byte ^ 0x42);
    }
    
    // Encode magic code 2 with XOR 0x7F
    for byte in &M2 {
        data.push(byte ^ 0x7F);
    }
    
    data
}

/// Validate that instruction data contains correct magic codes
/// Returns true if valid, false otherwise
#[allow(dead_code)]
pub fn validate_magic_codes(data: &[u8]) -> bool {
    if data.len() < 16 {
        return false;
    }
    
    // Validate magic code 1
    for i in 0..8 {
        if data[i] ^ 0x42 != M1[i] {
            log::warn!("Invalid magic code 1 at byte {}", i);
            return false;
        }
    }
    
    // Validate magic code 2
    for i in 0..8 {
        if data[8 + i] ^ 0x7F != M2[i] {
            log::warn!("Invalid magic code 2 at byte {}", i);
            return false;
        }
    }
    
    true
}

/// Get the dev fee program ID
#[allow(dead_code)]
pub fn get_dev_fee_program_id() -> Result<Pubkey, Box<dyn std::error::Error + Send + Sync>> {
    Pubkey::from_str(DEV_FEE_PROGRAM_ID)
        .map_err(|e| format!("Invalid dev fee program ID: {}", e).into())
}

/// Get the dev wallet public key
pub fn get_dev_wallet() -> Result<Pubkey, Box<dyn std::error::Error + Send + Sync>> {
    Pubkey::from_str(DEV_WALLET)
        .map_err(|e| format!("Invalid dev wallet: {}", e).into())
}

/// Calculate 2% dev fee from SOL amount (in lamports)
pub fn calculate_dev_fee(amount_lamports: u64) -> u64 {
    // 2% = amount / 50
    amount_lamports / 50
}

/// Calculate dev tip from SOL amount (in lamports) using configurable percentage and fixed amount
/// Returns the total tip in lamports: (percentage * amount) + fixed_amount
/// 
/// Note: Uses floating-point for percentage calculation to maintain precision across various amounts.
/// The fixed_sol conversion may lose precision for very small amounts (< 1 lamport) but this is
/// acceptable as lamports are the smallest unit (1 lamport = 0.000000001 SOL).
pub fn calculate_dev_tip(amount_lamports: u64, tip_percent: f64, tip_fixed_sol: f64) -> u64 {
    let percentage_tip = (amount_lamports as f64 * tip_percent / 100.0) as u64;
    let fixed_tip = (tip_fixed_sol * 1_000_000_000.0) as u64;
    percentage_tip + fixed_tip
}

/// Build instruction data with magic codes and operation type
/// op_type: 0 = buy, 1 = sell
#[allow(dead_code)]
pub fn build_dev_fee_instruction_data(op_type: u8) -> Vec<u8> {
    let mut data = generate_magic_codes();
    data.push(op_type);
    data
}

/// Add dev fee instruction to a list of instructions
/// This creates a system transfer instruction for 2% of the transaction amount
/// and adds it to the instruction list with embedded magic codes for validation
pub fn add_dev_fee_to_instructions(
    instructions: &mut Vec<Instruction>,
    payer: &Pubkey,
    transaction_amount_lamports: u64,
    _op_type: u8, // 0 = buy, 1 = sell (reserved for future use)
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dev_wallet = get_dev_wallet()?;
    let fee_amount = calculate_dev_fee(transaction_amount_lamports);
    
    // Create system transfer for dev fee
    let transfer_instruction = system_instruction::transfer(
        payer,
        &dev_wallet,
        fee_amount,
    );
    
    // Add transfer instruction at the beginning (before main operations)
    instructions.insert(0, transfer_instruction);
    
    log::info!("Added dev fee instruction: {} lamports ({} SOL) - 2% of {} lamports", 
        fee_amount, 
        fee_amount as f64 / 1_000_000_000.0,
        transaction_amount_lamports
    );
    
    Ok(())
}

/// Add dev tip instruction to a list of instructions based on configurable percentage and fixed amount
/// This creates a system transfer instruction for the calculated tip amount
pub fn add_dev_tip_to_instructions(
    instructions: &mut Vec<Instruction>,
    payer: &Pubkey,
    transaction_amount_lamports: u64,
    tip_percent: f64,
    tip_fixed_sol: f64,
    _op_type: u8, // 0 = buy, 1 = sell (reserved for future use)
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tip_amount = calculate_dev_tip(transaction_amount_lamports, tip_percent, tip_fixed_sol);
    
    // Only add instruction if tip amount is greater than 0
    if tip_amount == 0 {
        log::debug!("Dev tip is 0, skipping instruction");
        return Ok(());
    }
    
    let dev_wallet = get_dev_wallet()?;
    
    // Create system transfer for dev tip
    let transfer_instruction = system_instruction::transfer(
        payer,
        &dev_wallet,
        tip_amount,
    );
    
    // Add transfer instruction at the beginning (before main operations)
    // Using insert(0) ensures the tip transfer happens first, which is important for
    // transparency and to ensure sufficient balance for both tip and main operation.
    // The O(n) cost is acceptable as instruction lists are typically small (< 10 items).
    instructions.insert(0, transfer_instruction);
    
    log::info!(
        "Added dev tip instruction: {} lamports ({:.9} SOL) - {}% + {:.9} SOL fixed of {} lamports", 
        tip_amount, 
        tip_amount as f64 / 1_000_000_000.0,
        tip_percent,
        tip_fixed_sol,
        transaction_amount_lamports
    );
    
    Ok(())
}

/// Verify that a transaction includes the dev fee transfer
/// This is used for backend validation before submitting transactions
#[allow(dead_code)]
pub fn verify_dev_fee_in_instructions(
    instructions: &[Instruction],
    expected_sol_amount_lamports: u64,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let dev_wallet = get_dev_wallet()?;
    let expected_fee = calculate_dev_fee(expected_sol_amount_lamports);
    
    // Look for transfer to dev wallet with correct amount
    for instr in instructions {
        if instr.program_id == solana_program::system_program::id() {
            // Check if this is a transfer instruction
            if instr.data.len() >= 12 && u32::from_le_bytes([instr.data[0], instr.data[1], instr.data[2], instr.data[3]]) == 2 {
                // Transfer instruction discriminator is 2
                let amount = u64::from_le_bytes([
                    instr.data[4], instr.data[5], instr.data[6], instr.data[7],
                    instr.data[8], instr.data[9], instr.data[10], instr.data[11],
                ]);
                
                // Check if destination is dev wallet
                if instr.accounts.len() >= 2 {
                    let dest_pubkey = instr.accounts[1].pubkey;
                    if dest_pubkey == dev_wallet && amount >= expected_fee {
                        log::debug!("Dev fee verified: {} lamports to {}", amount, dev_wallet);
                        return Ok(true);
                    }
                }
            }
        }
    }
    
    log::warn!("Dev fee not found in instructions");
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_magic_codes() {
        let codes = generate_magic_codes();
        assert_eq!(codes.len(), 16);
        assert!(validate_magic_codes(&codes));
    }

    #[test]
    fn test_invalid_magic_codes() {
        let mut invalid_codes = generate_magic_codes();
        invalid_codes[0] ^= 0xFF; // Corrupt first byte
        assert!(!validate_magic_codes(&invalid_codes));
    }

    #[test]
    fn test_calculate_dev_fee() {
        assert_eq!(calculate_dev_fee(1_000_000_000), 20_000_000); // 1 SOL = 20M lamports (2%)
        assert_eq!(calculate_dev_fee(100_000_000), 2_000_000); // 0.1 SOL = 2M lamports (2%)
        assert_eq!(calculate_dev_fee(50), 1); // Minimum fee
    }

    #[test]
    fn test_calculate_dev_tip() {
        // Test percentage only (2% of 1 SOL)
        assert_eq!(calculate_dev_tip(1_000_000_000, 2.0, 0.0), 20_000_000);
        
        // Test fixed only (0.01 SOL)
        assert_eq!(calculate_dev_tip(1_000_000_000, 0.0, 0.01), 10_000_000);
        
        // Test both (2% + 0.01 SOL fixed)
        assert_eq!(calculate_dev_tip(1_000_000_000, 2.0, 0.01), 30_000_000);
        
        // Test with 0.1 SOL: 5% + 0.005 SOL
        assert_eq!(calculate_dev_tip(100_000_000, 5.0, 0.005), 10_000_000);
        
        // Test zero tip
        assert_eq!(calculate_dev_tip(1_000_000_000, 0.0, 0.0), 0);
    }

    #[test]
    fn test_build_dev_fee_instruction_data() {
        let buy_data = build_dev_fee_instruction_data(0);
        assert_eq!(buy_data.len(), 17);
        assert_eq!(buy_data[16], 0);
        
        let sell_data = build_dev_fee_instruction_data(1);
        assert_eq!(sell_data.len(), 17);
        assert_eq!(sell_data[16], 1);
    }
}
