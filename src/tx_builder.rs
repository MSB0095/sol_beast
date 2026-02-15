use borsh::BorshSerialize;
use solana_program::{instruction::{Instruction, AccountMeta}, pubkey::Pubkey};
use spl_associated_token_account::get_associated_token_address_with_program_id;
use std::str::FromStr;
use crate::settings::Settings;
use crate::idl::{load_all_idls, SimpleIdl};
use crate::onchain_idl::{get_instruction_discriminator, compute_anchor_discriminator};
use std::collections::HashMap;
use log::{debug, warn};

const SYSTEM_PROGRAM_PUBKEY: &str = "11111111111111111111111111111111";
const TOKEN_PROGRAM_PUBKEY: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022_PROGRAM_PUBKEY: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
// fee_program address from IDL
const FEE_PROGRAM_PUBKEY: &str = "pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ";

#[derive(BorshSerialize)]
pub struct BuyArgs {
    pub amount: u64,
    pub max_sol_cost: u64,
    // Option<bool> serializes as 1 byte (0 = None, 1 = Some) then bool
    pub track_volume: Option<bool>,
}

#[derive(BorshSerialize)]
pub struct SellArgs {
    pub amount: u64,
    pub min_sol_output: u64,
}

// Discriminators from pumpfun IDL (8 bytes) - DEPRECATED: Use IDL-derived discriminators instead
// Kept as fallback for backward compatibility when IDL is not available
// Using plain buy instruction (not buy_exact_sol_in)
pub const BUY_DISCRIMINATOR_FALLBACK: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
pub const SELL_DISCRIMINATOR_FALLBACK: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];

// For backward compatibility exports
pub const BUY_DISCRIMINATOR: [u8; 8] = BUY_DISCRIMINATOR_FALLBACK;
pub const SELL_DISCRIMINATOR: [u8; 8] = SELL_DISCRIMINATOR_FALLBACK;

/// Get discriminator for buy instruction, preferring IDL over computed value
fn get_buy_discriminator(idl_opt: Option<&SimpleIdl>) -> [u8; 8] {
    if let Some(idl) = idl_opt {
        match get_instruction_discriminator(idl, "buy") {
            Ok(disc) => {
                debug!("Using IDL-derived buy discriminator: {:?}", disc);
                return disc;
            }
            Err(e) => {
                warn!("Failed to get buy discriminator from IDL: {}, computing from name", e);
            }
        }
    }
    
    // Compute discriminator from instruction name
    let computed = compute_anchor_discriminator("buy");
    debug!("Using computed buy discriminator: {:?}", computed);
    computed
}

/// Get discriminator for sell instruction, preferring IDL over computed value
fn get_sell_discriminator(idl_opt: Option<&SimpleIdl>) -> [u8; 8] {
    if let Some(idl) = idl_opt {
        match get_instruction_discriminator(idl, "sell") {
            Ok(disc) => {
                debug!("Using IDL-derived sell discriminator: {:?}", disc);
                return disc;
            }
            Err(e) => {
                warn!("Failed to get sell discriminator from IDL: {}, computing from name", e);
            }
        }
    }
    
    // Compute discriminator from instruction name
    let computed = compute_anchor_discriminator("sell");
    debug!("Using computed sell discriminator: {:?}", computed);
    computed
}



pub fn build_buy_instruction(
    program_id: &Pubkey,
    mint: &str,
    amount: u64,
    max_sol_cost: u64,
    track_volume: Option<bool>,
    user: &Pubkey,
    fee_recipient: &Pubkey,
    creator_pubkey: Option<Pubkey>,
    _settings: &Settings,
) -> Result<Instruction, Box<dyn std::error::Error + Send + Sync>> {
    
    let args = BuyArgs {
        amount,
        max_sol_cost,
        track_volume,
    };
    
    // Try to load IDLs and build accounts from IDL if possible for exactness.
    // Preferred order: pumpfun, pumpfunamm, pumpfunfees
    let idls = load_all_idls();
    let mut context: HashMap<String, Pubkey> = HashMap::new();
    let mint_pk = Pubkey::from_str(mint)?;
    context.insert("mint".to_string(), mint_pk);
    context.insert("user".to_string(), *user);
    
    // Creator is REQUIRED for buy instruction - do not use placeholder
    let creator = creator_pubkey.ok_or("creator_pubkey is required for buy instruction")?;
    // Support both snake_case and camelCase for compatibility
    context.insert("bonding_curve.creator".to_string(), creator);
    context.insert("bondingCurve.creator".to_string(), creator);
    
    // Use provided fee_recipient from bonding curve
    context.insert("fee_recipient".to_string(), *fee_recipient);
    context.insert("feeRecipient".to_string(), *fee_recipient);
    // NOTE: The pump.fun IDL lists `feeProgram` as a required account for `buy`,
    // but its address is supplied directly by the IDL, so it should not be added to `context`.

    let pref = ["pumpfun", "pumpfunamm", "pumpfunfees"];
    let mut last_error: Option<String> = None;
    
    for key in pref.iter() {
        if let Some(idl) = idls.get(*key) {
            match idl.build_accounts_for("buy", &context) {
                Ok(metas) => {
                    // Get discriminator from IDL
                    let discriminator = get_buy_discriminator(Some(idl));
                    let mut data = discriminator.to_vec();
                    data.extend(borsh::to_vec(&args)?);
                    
                    debug!("Built buy instruction with IDL {} ({} accounts)", key, metas.len());
                    return Ok(Instruction { program_id: idl.address, accounts: metas, data });
                }
                Err(e) => {
                    let err_msg = format!("IDL {} build_accounts_for(buy) failed: {}", key, e);
                    debug!("{}", err_msg);
                    last_error = Some(err_msg);
                }
            }
        }
    }

    // Log detailed error before falling back
    if let Some(err) = last_error {
        warn!("All IDL-based builds failed, using fallback. Last error: {}", err);
        warn!("Context provided: {:?}", context.keys().collect::<Vec<_>>());
    }
    
    // fallback: construct best-effort like before
    // Default to Token-2022 for all pump.fun tokens
    let token_program_pk = Pubkey::from_str(TOKEN_2022_PROGRAM_PUBKEY)?;
    let pump_program = *program_id;
    // global PDA
    let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &pump_program);
    // bonding_curve PDA
    let (bonding_curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
    // associated_user (ATA) - use Token-2022 for correct derivation
    let associated_user = get_associated_token_address_with_program_id(user, &mint_pk, &token_program_pk);
    // event authority PDA
    let (event_authority, _) = Pubkey::find_program_address(&[b"__event_authority"], &pump_program);
    // volume accumulators
    let (global_vol_acc, _) = Pubkey::find_program_address(&[b"global_volume_accumulator"], &pump_program);
    let (user_vol_acc, _) = Pubkey::find_program_address(&[b"user_volume_accumulator", user.as_ref()], &pump_program);

    // Build accounts in exact order expected by pump.fun (don't use add_or_merge to avoid deduplication)
    let mut accounts: Vec<AccountMeta> = vec![
        AccountMeta::new_readonly(global_pda, false),           // 0: global
        AccountMeta::new(*fee_recipient, false),                // 1: fee_recipient (from bonding curve, non-signer)
        AccountMeta::new_readonly(mint_pk, false),              // 2: mint
        AccountMeta::new(bonding_curve_pda, false),             // 3: bonding_curve
    ];    // Associated bonding curve ATA - use Token-2022 for correct derivation
    let assoc_bonding = get_associated_token_address_with_program_id(&bonding_curve_pda, &mint_pk, &token_program_pk);
    accounts.push(AccountMeta::new(assoc_bonding, false));                 // 4: assoc_bonding
    accounts.push(AccountMeta::new(associated_user, false));               // 5: assoc_user
    accounts.push(AccountMeta::new(*user, true));                          // 6: user (signer)
    accounts.push(AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_PUBKEY)?, false)); // 7: system_program
    accounts.push(AccountMeta::new_readonly(token_program_pk, false)); // 8: token_program (Token-2022)
    
    // Creator vault is REQUIRED - we already validated creator exists above
    let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program);
    accounts.push(AccountMeta::new(creator_vault, false));              // 9: creator_vault
    accounts.push(AccountMeta::new_readonly(event_authority, false));        // 10: event_authority
    accounts.push(AccountMeta::new_readonly(*program_id, false));            // 11: program
    accounts.push(AccountMeta::new(global_vol_acc, false));                  // 12: global_vol_acc
    accounts.push(AccountMeta::new(user_vol_acc, false));                    // 13: user_vol_acc
    let fee_program_pk = Pubkey::from_str(FEE_PROGRAM_PUBKEY)?;
    let (fee_config_pda, _) = Pubkey::find_program_address(&[b"fee_config", &[1,86,224,246,147,102,90,207,68,219,21,104,191,23,91,170,81,137,203,151,245,210,255,59,101,93,43,182,253,109,24,176]], &fee_program_pk);
    accounts.push(AccountMeta::new_readonly(fee_config_pda, false));         // 14: fee_config
    accounts.push(AccountMeta::new_readonly(Pubkey::from_str(FEE_PROGRAM_PUBKEY)?, false)); // 15: fee_program

    // Use fallback discriminator
    let discriminator = get_buy_discriminator(None);
    let mut data = discriminator.to_vec();
    data.extend(borsh::to_vec(&args)?);
    
    warn!("Using fallback buy instruction builder (no IDL available)");
    Ok(Instruction { program_id: *program_id, accounts, data })
}

pub fn build_sell_instruction(
    program_id: &Pubkey,
    mint: &str,
    amount: u64,
    min_sol_output: u64,
    user: &Pubkey,
    fee_recipient: &Pubkey,
    creator_pubkey: Option<Pubkey>,
    _settings: &Settings,
) -> Result<Instruction, Box<dyn std::error::Error + Send + Sync>> {
    let args = SellArgs {
        amount,
        min_sol_output,
    };
    
    // Try to build using IDL if available. Preferred order: pumpfun, pumpfunamm, pumpfunfees
    let idls = load_all_idls();
    let mint_pk = Pubkey::from_str(mint)?;
    let mut context: HashMap<String, Pubkey> = HashMap::new();
    context.insert("mint".to_string(), mint_pk);
    context.insert("user".to_string(), *user);
    
    // Creator is REQUIRED for sell instruction - do not use placeholder
    let creator = creator_pubkey.ok_or("creator_pubkey is required for sell instruction")?;
    // Support both snake_case and camelCase for compatibility
    context.insert("bonding_curve.creator".to_string(), creator);
    context.insert("bondingCurve.creator".to_string(), creator);
    
    // Use provided fee_recipient from bonding curve
    context.insert("fee_recipient".to_string(), *fee_recipient);
    context.insert("feeRecipient".to_string(), *fee_recipient);
    // NOTE: The pump.fun IDL includes `feeProgram` in both buy and sell instructions.
    // For sell, we add it to the context here so the IDL resolver can use it.
    let fee_program_pubkey = Pubkey::from_str(FEE_PROGRAM_PUBKEY)
        .map_err(|e| format!("Invalid fee_program pubkey: {}", e))?;
    context.insert("fee_program".to_string(), fee_program_pubkey);
    context.insert("feeProgram".to_string(), fee_program_pubkey);
    
    let pref = ["pumpfun", "pumpfunamm", "pumpfunfees"];
    let mut last_error: Option<String> = None;
    
    for key in pref.iter() {
        if let Some(idl) = idls.get(*key) {
            match idl.build_accounts_for("sell", &context) {
                Ok(metas) => {
                    // Get discriminator from IDL
                    let discriminator = get_sell_discriminator(Some(idl));
                    let mut data = discriminator.to_vec();
                    data.extend(borsh::to_vec(&args)?);
                    
                    debug!("Built sell instruction with IDL {} ({} accounts)", key, metas.len());
                    return Ok(Instruction { program_id: idl.address, accounts: metas, data });
                }
                Err(e) => {
                    let err_msg = format!("IDL {} build_accounts_for(sell) failed: {}", key, e);
                    debug!("{}", err_msg);
                    last_error = Some(err_msg);
                }
            }
        }
    }

    // Log detailed error before falling back
    if let Some(err) = last_error {
        warn!("All IDL-based builds failed, using fallback. Last error: {}", err);
        warn!("Context provided: {:?}", context.keys().collect::<Vec<_>>());
    }
    
    // fallback best-effort behavior (requires creator)
    // Default to Token-2022 for all pump.fun tokens
    let token_program_pk = Pubkey::from_str(TOKEN_2022_PROGRAM_PUBKEY)?;
    let pump_program = *program_id;
    let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &pump_program);
    let (bonding_curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
    let associated_user = get_associated_token_address_with_program_id(user, &mint_pk, &token_program_pk);
    let (event_authority, _) = Pubkey::find_program_address(&[b"__event_authority"], &pump_program);
    
    // Build accounts in exact order expected by pump.fun (don't use add_or_merge to avoid deduplication)
    let mut accounts: Vec<AccountMeta> = vec![
        AccountMeta::new_readonly(global_pda, false),             // 0: global
        AccountMeta::new(*fee_recipient, false),                  // 1: fee_recipient (from bonding curve)
        AccountMeta::new_readonly(mint_pk, false),                // 2: mint
        AccountMeta::new(bonding_curve_pda, false),               // 3: bonding_curve
    ];    // Associated bonding curve ATA - use Token-2022 for correct derivation
    let assoc_bonding = get_associated_token_address_with_program_id(&bonding_curve_pda, &mint_pk, &token_program_pk);
    accounts.push(AccountMeta::new(assoc_bonding, false));                   // 4: assoc_bonding
    accounts.push(AccountMeta::new(associated_user, false));                 // 5: assoc_user
    accounts.push(AccountMeta::new(*user, true));                            // 6: user (signer)
    accounts.push(AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_PUBKEY)?, false)); // 7: system_program
    
    // Creator vault is REQUIRED - we already validated creator exists above
    let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program);
    accounts.push(AccountMeta::new(creator_vault, false));               // 8: creator_vault
    accounts.push(AccountMeta::new_readonly(token_program_pk, false)); // 9: token_program (Token-2022)
    accounts.push(AccountMeta::new_readonly(event_authority, false));        // 10: event_authority
    accounts.push(AccountMeta::new_readonly(*program_id, false));            // 11: program
    let fee_program_pk = Pubkey::from_str(FEE_PROGRAM_PUBKEY)?;
    let (fee_config_pda, _) = Pubkey::find_program_address(&[b"fee_config", &[1,86,224,246,147,102,90,207,68,219,21,104,191,23,91,170,81,137,203,151,245,210,255,59,101,93,43,182,253,109,24,176]], &fee_program_pk);
    accounts.push(AccountMeta::new_readonly(fee_config_pda, false));         // 12: fee_config
    accounts.push(AccountMeta::new_readonly(fee_program_pk, false)); // 13: fee_program

    // Use fallback discriminator
    let discriminator = get_sell_discriminator(None);
    let mut data = discriminator.to_vec();
    data.extend(borsh::to_vec(&args)?);
    
    warn!("Using fallback sell instruction builder (no IDL available)");
    Ok(Instruction { program_id: *program_id, accounts, data })
}
