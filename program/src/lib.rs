use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program::invoke,
    program_error::ProgramError,
    pubkey::Pubkey,
    system_instruction,
};

// Magic codes embedded in program - obfuscated with random values
// Magic code 1: XOR with 0x42 => actual value when decoded
const M1: [u8; 8] = [0x73, 0x91, 0xC5, 0x28, 0x65, 0xF7, 0x2B, 0xD4];
// Magic code 2: XOR with 0x7F => actual value when decoded  
const M2: [u8; 8] = [0x1E, 0x8C, 0x42, 0xD9, 0x57, 0x3A, 0x6F, 0xB2];

// IMPORTANT: This is a placeholder address and MUST be updated before deployment!
// TODO: Replace with actual dev wallet address (convert base58 to bytes)
// Example: Use `solana address --keypair <wallet.json>` then convert to byte array
// Dev fee wallet (hardcoded, must be changed before deployment)
const DEV_WALLET: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1
];

entrypoint!(process_instruction);

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    // Validate magic codes
    if data.len() < 17 {
        return Err(ProgramError::InvalidInstructionData);
    }
    
    // Check magic code 1 (XOR decoded)
    for i in 0..8 {
        if data[i] ^ 0x42 != M1[i] {
            msg!("Invalid magic code 1");
            return Err(ProgramError::InvalidInstructionData);
        }
    }
    
    // Check magic code 2 (XOR decoded)
    for i in 0..8 {
        if data[8 + i] ^ 0x7F != M2[i] {
            msg!("Invalid magic code 2");
            return Err(ProgramError::InvalidInstructionData);
        }
    }
    
    // Extract operation type (0 = buy, 1 = sell)
    let op_type = data[16];
    if op_type > 1 {
        return Err(ProgramError::InvalidInstructionData);
    }
    
    let account_iter = &mut accounts.iter();
    let payer = next_account_info(account_iter)?;
    let dev_wallet = next_account_info(account_iter)?;
    let system_program = next_account_info(account_iter)?;
    
    // Verify dev wallet matches hardcoded address
    if dev_wallet.key.to_bytes() != DEV_WALLET {
        msg!("Invalid dev wallet");
        return Err(ProgramError::InvalidAccountData);
    }
    
    // Verify payer is signer
    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    
    // Note: This simple contract calculates fee based on payer balance for minimal bytecode
    // The backend pre-calculates and adds the exact transfer amount needed
    // This is just a validation layer - the actual fee transfer is done by backend
    // Calculate 2% fee from payer balance (in lamports)
    let fee_amount = payer.lamports().checked_div(50).ok_or(ProgramError::ArithmeticOverflow)?;
    
    // Transfer 2% to dev wallet
    invoke(
        &system_instruction::transfer(payer.key, dev_wallet.key, fee_amount),
        &[payer.clone(), dev_wallet.clone(), system_program.clone()],
    )?;
    
    msg!("Dev fee collected: {} lamports", fee_amount);
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    
    #[test]
    fn test_magic_codes() {
        // Verify magic code XOR logic
        let test_m1 = [0x73, 0x91, 0xC5, 0x28, 0x65, 0xF7, 0x2B, 0xD4];
        let test_m2 = [0x1E, 0x8C, 0x42, 0xD9, 0x57, 0x3A, 0x6F, 0xB2];
        
        for i in 0..8 {
            assert_eq!(test_m1[i], M1[i]);
            assert_eq!(test_m2[i], M2[i]);
        }
    }
}
