// Transaction parsing - platform agnostic
// Extracts pump.fun token creation data from Solana transactions

use crate::error::CoreError;
use serde_json::Value;
use solana_program::pubkey::Pubkey;
use std::str::FromStr;
use log::{info, debug};

/// Pump.fun `create` instruction discriminator: [24, 30, 200, 40, 5, 28, 7, 119]
/// This is the 8-byte Anchor discriminator for the `create` instruction that creates new tokens.
pub const PUMP_CREATE_DISCRIMINATOR: [u8; 8] = [24, 30, 200, 40, 5, 28, 7, 119];

/// Extracted data from a pump.fun create instruction
#[derive(Debug, Clone)]
pub struct PumpCreateData {
    pub mint: String,
    pub creator: String,
    pub curve: Option<String>,
}

/// Result of transaction parsing
#[derive(Debug, Clone)]
pub struct ParsedTransaction {
    pub creator: String,
    pub mint: String,
    pub bonding_curve: String,
    pub holder_address: String,
    pub is_creation: bool,
}

/// Attempts to extract pump.fun create instruction data from an instruction object.
/// 
/// Returns Some(PumpCreateData) if the instruction is a valid pump.fun create instruction,
/// None otherwise.
fn try_extract_pump_create_data(
    instr: &Value,
    account_keys: &[String],
    pump_fun_program_id: &str,
) -> Option<PumpCreateData> {
    // Get program ID
    let program_id = instr
        .get("programId")
        .and_then(|p| p.as_str())
        .or_else(|| {
            instr
                .get("programIdIndex")
                .and_then(|idx| idx.as_u64())
                .and_then(|i| account_keys.get(i as usize).map(|s| s.as_str()))
        })?;
    
    if program_id != pump_fun_program_id {
        return None;
    }
    
    // Check instruction data for create discriminator
    let data_str = instr.get("data").and_then(|d| d.as_str())?;
    let data_bytes = bs58::decode(data_str).into_vec().ok()?;
    
    if data_bytes.len() < 8 || data_bytes[..8] != PUMP_CREATE_DISCRIMINATOR {
        return None;
    }
    
    // Extract accounts - per pump.fun IDL:
    // [0] mint, [1] mint_authority, [2] bonding_curve, [3] associated_bonding_curve,
    // [4] global, [5] mpl_token_metadata, [6] metadata, [7] user (creator/signer)
    let accounts = instr.get("accounts").and_then(|a| a.as_array())?;
    
    if accounts.len() < 8 {
        return None;
    }
    
    // Helper to extract account at index
    let get_account = |idx: usize| -> Option<String> {
        let account_val = accounts.get(idx)?;
        if let Some(i) = account_val.as_u64() {
            account_keys.get(i as usize).cloned()
        } else {
            account_val.as_str().map(|s| s.to_string())
        }
    };
    
    let mint = get_account(0)?;
    let creator = get_account(7)?;
    let curve = get_account(2);
    
    Some(PumpCreateData { mint, creator, curve })
}

/// Computes the holder address (associated token account) for a bonding curve and mint.
/// 
/// # Parameters
/// - `owner`: The owner of the token account (bonding curve PDA)
/// - `mint`: The mint address
fn compute_holder_address(owner: &str, mint: &str) -> Result<String, CoreError> {
    let owner_pk = Pubkey::from_str(owner)
        .map_err(|e| CoreError::ParseError(format!("Invalid owner pubkey: {}", e)))?;
    let mint_pk = Pubkey::from_str(mint)
        .map_err(|e| CoreError::ParseError(format!("Invalid mint pubkey: {}", e)))?;
    
    // Use SPL token's get_associated_token_address to compute the ATA
    let holder_addr = spl_associated_token_account::get_associated_token_address(&owner_pk, &mint_pk);
    Ok(holder_addr.to_string())
}

/// Processes extracted pump.fun create data and computes derived addresses.
/// Returns ParsedTransaction with all computed fields.
fn process_pump_create_data(
    data: PumpCreateData,
    pump_fun_program: &str,
    location: &str,
) -> Result<ParsedTransaction, CoreError> {
    // Log info if mint doesn't end with "pump" (unusual but possible)
    if !data.mint.ends_with("pump") {
        info!("{} mint {} does not end with 'pump' (unusual for pump.fun, but accepting)", location, data.mint);
    }
    
    // Compute bonding curve PDA if not already extracted
    let curve = match data.curve {
        Some(c) => c,
        None => {
            let pump_program = Pubkey::from_str(pump_fun_program)
                .map_err(|e| CoreError::ParseError(format!("Failed to parse pump.fun program ID: {}", e)))?;
            let mint_pk = Pubkey::from_str(&data.mint)
                .map_err(|e| CoreError::ParseError(format!("Failed to parse mint address {}: {}", data.mint, e)))?;
            let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
            curve_pda.to_string()
        }
    };
    
    // Compute the holder address (ATA for the bonding curve)
    let holder_addr = compute_holder_address(&curve, &data.mint)?;
    
    debug!("Pump.fun CREATE in {}: mint={} creator={} curve={} holder_addr={}", 
           location, data.mint, data.creator, curve, holder_addr);
    
    Ok(ParsedTransaction {
        creator: data.creator,
        mint: data.mint,
        bonding_curve: curve,
        holder_address: holder_addr,
        is_creation: true,
    })
}

/// Normalize account keys from transaction JSON
fn normalize_account_keys(data_value: &Value) -> Result<Vec<String>, CoreError> {
    let account_keys_arr = data_value
        .get("transaction")
        .and_then(|t| t.get("message"))
        .and_then(|m| m.get("accountKeys"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| CoreError::ParseError("Missing accountKeys in transaction".to_string()))?;

    let mut account_keys: Vec<String> = Vec::with_capacity(account_keys_arr.len());
    for key in account_keys_arr {
        if let Some(s) = key.as_str() {
            account_keys.push(s.to_string());
        } else if let Some(obj) = key.as_object() {
            if let Some(pubkey) = obj.get("pubkey").and_then(|p| p.as_str()) {
                account_keys.push(pubkey.to_string());
            } else {
                account_keys.push(serde_json::to_string(obj)
                    .map_err(|e| CoreError::ParseError(format!("Failed to serialize account key: {}", e)))?);
            }
        } else {
            account_keys.push(key.to_string());
        }
    }
    
    Ok(account_keys)
}

/// Parse transaction JSON to extract pump.fun create instruction data
/// 
/// This function searches for pump.fun create instructions in both main instructions
/// and inner instructions (for CPI cases).
/// 
/// # Arguments
/// * `transaction_json` - The parsed transaction JSON from RPC
/// * `pump_fun_program_id` - The pump.fun program ID to look for
/// 
/// # Returns
/// * `Ok(ParsedTransaction)` if a create instruction is found
/// * `Err(CoreError)` if no create instruction is found or parsing fails
pub fn parse_transaction(
    transaction_json: &Value,
    pump_fun_program_id: &str,
) -> Result<ParsedTransaction, CoreError> {
    debug!("Parsing transaction for pump.fun create instruction");
    
    // Normalize account keys
    let account_keys = normalize_account_keys(transaction_json)?;
    
    // STEP 1: Check for pump.fun `create` instruction in main instructions
    if let Some(instructions) = transaction_json
        .get("transaction")
        .and_then(|t| t.get("message"))
        .and_then(|m| m.get("instructions"))
        .and_then(|i| i.as_array())
    {
        for instr in instructions {
            if let Some(data) = try_extract_pump_create_data(instr, &account_keys, pump_fun_program_id) {
                return process_pump_create_data(data, pump_fun_program_id, "main instructions");
            }
        }
    }
    
    // STEP 2: Fallback - check inner instructions for pump.fun create
    // This handles cases where the create is wrapped in a CPI call
    if let Some(meta) = transaction_json.get("meta") {
        if let Some(inner_instructions) = meta.get("innerInstructions").and_then(|v| v.as_array()) {
            for inner_instruction in inner_instructions {
                if let Some(instructions) = inner_instruction.get("instructions").and_then(|v| v.as_array()) {
                    for instr in instructions {
                        if let Some(data) = try_extract_pump_create_data(instr, &account_keys, pump_fun_program_id) {
                            return process_pump_create_data(data, pump_fun_program_id, "inner instructions");
                        }
                    }
                }
            }
        }
    }
    
    // No pump.fun create instruction found
    debug!("No pump.fun CREATE instruction found in transaction");
    debug!("Account keys (len={}): {:?}", account_keys.len(), account_keys);
    Err(CoreError::NotFound("Could not find pump.fun create instruction".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pump_create_discriminator() {
        // Ensure discriminator constant is correct
        assert_eq!(PUMP_CREATE_DISCRIMINATOR.len(), 8);
        assert_eq!(PUMP_CREATE_DISCRIMINATOR, [24, 30, 200, 40, 5, 28, 7, 119]);
    }
}
