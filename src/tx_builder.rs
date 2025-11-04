use borsh::BorshSerialize;
use solana_program::{instruction::{Instruction, AccountMeta}, pubkey::Pubkey};
use spl_associated_token_account::get_associated_token_address;
use std::str::FromStr;
use crate::settings::Settings;
use crate::idl::load_all_idls;
use std::collections::HashMap;

const SYSTEM_PROGRAM_PUBKEY: &str = "11111111111111111111111111111111";
const TOKEN_PROGRAM_PUBKEY: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
// associated token program constant removed (unused)
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

// Discriminators from pumpfun IDL (8 bytes)
pub const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
pub const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];

// Helper to add or merge an AccountMeta (merge writable/signer flags if pubkey already present)
fn add_or_merge(accounts: &mut Vec<AccountMeta>, am: AccountMeta) {
    for existing in accounts.iter_mut() {
        if existing.pubkey == am.pubkey {
            // merge flags
            let writable = existing.is_writable || am.is_writable;
            let signer = existing.is_signer || am.is_signer;
            *existing = if signer {
                AccountMeta::new(am.pubkey, true)
            } else if writable {
                AccountMeta::new(am.pubkey, false)
            } else {
                AccountMeta::new_readonly(am.pubkey, false)
            };
            return;
        }
    }
    accounts.push(am);
}

pub fn build_buy_instruction(
    program_id: &Pubkey,
    mint: &str,
    amount: u64,
    max_sol_cost: u64,
    track_volume: Option<bool>,
    user: &Pubkey,
    creator_pubkey: Option<Pubkey>,
    _settings: &Settings,
) -> Result<Instruction, Box<dyn std::error::Error + Send + Sync>> {
    
    // For now we only build instruction.data (discriminator + borsh args).
    // Account list is intentionally left empty as PDAs/account derivation
    // is environment-specific and handled elsewhere. Keeping a single
    // builder centralizes encoding so dry-mode simulate and real-mode send
    // use identical instruction bytes.
    let args = BuyArgs {
        amount,
        max_sol_cost,
        track_volume,
    };
    let mut data = BUY_DISCRIMINATOR.to_vec();
    data.extend(borsh::to_vec(&args)?);
    // Try to load IDLs and build accounts from IDL if possible for exactness.
    // Preferred order: pumpfun, pumpfunamm, pumpfunfees
    let idls = load_all_idls();
    let mut context: HashMap<String, Pubkey> = HashMap::new();
    let mint_pk = Pubkey::from_str(mint)?;
    context.insert("mint".to_string(), mint_pk);
    context.insert("user".to_string(), *user);
    if let Some(creator) = creator_pubkey {
        context.insert("bonding_curve.creator".to_string(), creator);
    }

    let pref = ["pumpfun", "pumpfunamm", "pumpfunfees"];
    for key in pref.iter() {
        if let Some(idl) = idls.get(*key) {
            match idl.build_accounts_for("buy", &context) {
                Ok(metas) => return Ok(Instruction { program_id: idl.address, accounts: metas, data }),
                Err(e) => log::debug!("IDL {} build_accounts_for(buy) failed: {}", key, e),
            }
        }
    }

    // fallback: construct best-effort like before
    let pump_program = *program_id;
    // global PDA
    let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &pump_program);
    // bonding_curve PDA
    let (bonding_curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
    // associated_user (ATA)
    let associated_user = get_associated_token_address(user, &mint_pk);
    // event authority PDA
    let (event_authority, _) = Pubkey::find_program_address(&[b"__event_authority"], &pump_program);
    // volume accumulators
    let (global_vol_acc, _) = Pubkey::find_program_address(&[b"global_volume_accumulator"], &pump_program);
    let (user_vol_acc, _) = Pubkey::find_program_address(&[b"user_volume_accumulator", user.as_ref()], &pump_program);

    let mut accounts: Vec<AccountMeta> = Vec::new();
    add_or_merge(&mut accounts, AccountMeta::new_readonly(global_pda, false));
    add_or_merge(&mut accounts, AccountMeta::new(*user, false));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(mint_pk, false));
    add_or_merge(&mut accounts, AccountMeta::new(bonding_curve_pda, false));
    let (assoc_bonding, _) = Pubkey::find_program_address(&[bonding_curve_pda.as_ref(), mint_pk.as_ref()], &pump_program);
    add_or_merge(&mut accounts, AccountMeta::new(assoc_bonding, false));
    add_or_merge(&mut accounts, AccountMeta::new(associated_user, false));
    add_or_merge(&mut accounts, AccountMeta::new(*user, true));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_PUBKEY)?, false));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_PUBKEY)?, false));
    if let Some(creator) = creator_pubkey {
        let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program);
        add_or_merge(&mut accounts, AccountMeta::new(creator_vault, false));
    } else {
        return Err("creator_pubkey required to compute creator_vault; IDL build failed".into());
    }
    add_or_merge(&mut accounts, AccountMeta::new_readonly(event_authority, false));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(*program_id, false));
    add_or_merge(&mut accounts, AccountMeta::new(global_vol_acc, false));
    add_or_merge(&mut accounts, AccountMeta::new(user_vol_acc, false));
    let fee_program_pk = Pubkey::from_str(FEE_PROGRAM_PUBKEY)?;
    let (fee_config_pda, _) = Pubkey::find_program_address(&[b"fee_config", &[1,86,224,246,147,102,90,207,68,219,21,104,191,23,91,170,81,137,203,151,245,210,255,59,101,93,43,182,253,109,24,176]], &fee_program_pk);
    add_or_merge(&mut accounts, AccountMeta::new_readonly(fee_config_pda, false));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(Pubkey::from_str(FEE_PROGRAM_PUBKEY)?, false));

    Ok(Instruction { program_id: *program_id, accounts, data })
}

pub fn build_sell_instruction(
    program_id: &Pubkey,
    mint: &str,
    amount: u64,
    min_sol_output: u64,
    user: &Pubkey,
    creator_pubkey: Option<Pubkey>,
    _settings: &Settings,
) -> Result<Instruction, Box<dyn std::error::Error + Send + Sync>> {
    let args = SellArgs {
        amount,
        min_sol_output,
    };
    let mut data = SELL_DISCRIMINATOR.to_vec();
    data.extend(borsh::to_vec(&args)?);
    // Try to build using IDL if available. Preferred order: pumpfun, pumpfunamm, pumpfunfees
    let idls = load_all_idls();
    let mint_pk = Pubkey::from_str(mint)?;
    let mut context: HashMap<String, Pubkey> = HashMap::new();
    context.insert("mint".to_string(), mint_pk);
    context.insert("user".to_string(), *user);
    if let Some(creator) = creator_pubkey {
        context.insert("bonding_curve.creator".to_string(), creator);
    }
    let pref = ["pumpfun", "pumpfunamm", "pumpfunfees"];
    for key in pref.iter() {
        if let Some(idl) = idls.get(*key) {
            match idl.build_accounts_for("sell", &context) {
                Ok(metas) => return Ok(Instruction { program_id: idl.address, accounts: metas, data }),
                Err(e) => log::debug!("IDL {} build_accounts_for(sell) failed: {}", key, e),
            }
        }
    }

    // fallback best-effort behavior (requires creator)
    let pump_program = *program_id;
    let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &pump_program);
    let (bonding_curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
    let associated_user = get_associated_token_address(user, &mint_pk);
    let (event_authority, _) = Pubkey::find_program_address(&[b"__event_authority"], &pump_program);
    let (global_vol_acc, _) = Pubkey::find_program_address(&[b"global_volume_accumulator"], &pump_program);
    let (user_vol_acc, _) = Pubkey::find_program_address(&[b"user_volume_accumulator", user.as_ref()], &pump_program);
    let mut accounts: Vec<AccountMeta> = Vec::new();
    add_or_merge(&mut accounts, AccountMeta::new_readonly(global_pda, false));
    add_or_merge(&mut accounts, AccountMeta::new(*user, false));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(mint_pk, false));
    add_or_merge(&mut accounts, AccountMeta::new(bonding_curve_pda, false));
    let (assoc_bonding, _) = Pubkey::find_program_address(&[bonding_curve_pda.as_ref(), mint_pk.as_ref()], &pump_program);
    add_or_merge(&mut accounts, AccountMeta::new(assoc_bonding, false));
    add_or_merge(&mut accounts, AccountMeta::new(associated_user, false));
    add_or_merge(&mut accounts, AccountMeta::new(*user, true));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_PUBKEY)?, false));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_PUBKEY)?, false));
    if let Some(creator) = creator_pubkey {
        let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program);
        add_or_merge(&mut accounts, AccountMeta::new(creator_vault, false));
    } else {
        return Err("creator_pubkey required to compute creator_vault; IDL build failed".into());
    }
    add_or_merge(&mut accounts, AccountMeta::new_readonly(event_authority, false));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(*program_id, false));
    add_or_merge(&mut accounts, AccountMeta::new(global_vol_acc, false));
    add_or_merge(&mut accounts, AccountMeta::new(user_vol_acc, false));
    let fee_program_pk = Pubkey::from_str(FEE_PROGRAM_PUBKEY)?;
    let (fee_config_pda, _) = Pubkey::find_program_address(&[b"fee_config", &[1,86,224,246,147,102,90,207,68,219,21,104,191,23,91,170,81,137,203,151,245,210,255,59,101,93,43,182,253,109,24,176]], &fee_program_pk);
    add_or_merge(&mut accounts, AccountMeta::new_readonly(fee_config_pda, false));
    add_or_merge(&mut accounts, AccountMeta::new_readonly(fee_program_pk, false));

    Ok(Instruction { program_id: *program_id, accounts, data })
}
