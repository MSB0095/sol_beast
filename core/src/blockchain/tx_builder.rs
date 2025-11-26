use borsh::BorshSerialize;
use solana_program::{instruction::{Instruction, AccountMeta}, pubkey::Pubkey};
use spl_associated_token_account::get_associated_token_address;
use std::str::FromStr;

const SYSTEM_PROGRAM_PUBKEY: &str = "11111111111111111111111111111111";
const TOKEN_PROGRAM_PUBKEY: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const FEE_PROGRAM_PUBKEY: &str = "pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ";

#[derive(BorshSerialize)]
pub struct BuyArgs {
    pub amount: u64,
    pub max_sol_cost: u64,
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

pub fn build_buy_instruction(
    program_id: &Pubkey,
    mint: &str,
    amount: u64,
    max_sol_cost: u64,
    track_volume: Option<bool>,
    user: &Pubkey,
    fee_recipient: &Pubkey,
    creator_pubkey: Option<Pubkey>,
) -> Result<Instruction, Box<dyn std::error::Error + Send + Sync>> {
    let args = BuyArgs {
        amount,
        max_sol_cost,
        track_volume,
    };
    let mut data = BUY_DISCRIMINATOR.to_vec();
    data.extend(borsh::to_vec(&args)?);

    let mint_pk = Pubkey::from_str(mint)?;
    let bonding_curve_pda = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], program_id).0;
    let associated_user = get_associated_token_address(user, &mint_pk);
    let event_authority = Pubkey::find_program_address(&[b"__event_authority"], program_id).0;
    let global_vol_acc = Pubkey::find_program_address(&[b"global_volume_accumulator"], program_id).0;
    let user_vol_acc = Pubkey::find_program_address(&[b"user_volume_accumulator", user.as_ref()], program_id).0;

    let global_pda = Pubkey::find_program_address(&[b"global"], program_id).0;

    let mut accounts = vec![
        AccountMeta::new_readonly(global_pda, false),
        AccountMeta::new_readonly(*fee_recipient, false),
        AccountMeta::new(bonding_curve_pda, false),
        AccountMeta::new(associated_user, false),
        AccountMeta::new_readonly(*user, true),
        AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_PUBKEY)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_PUBKEY)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(FEE_PROGRAM_PUBKEY)?, false),
        AccountMeta::new_readonly(mint_pk, false),
        AccountMeta::new(event_authority, false),
        AccountMeta::new(global_vol_acc, false),
        AccountMeta::new(user_vol_acc, false),
    ];

    // Add creator account if provided
    if let Some(creator) = creator_pubkey {
        accounts.push(AccountMeta::new_readonly(creator, false));
    }

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

pub fn build_sell_instruction(
    program_id: &Pubkey,
    mint: &str,
    amount: u64,
    min_sol_output: u64,
    user: &Pubkey,
    fee_recipient: &Pubkey,
    creator_pubkey: Option<Pubkey>,
) -> Result<Instruction, Box<dyn std::error::Error + Send + Sync>> {
    let args = SellArgs {
        amount,
        min_sol_output,
    };
    let mut data = SELL_DISCRIMINATOR.to_vec();
    data.extend(borsh::to_vec(&args)?);

    let mint_pk = Pubkey::from_str(mint)?;
    let bonding_curve_pda = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], program_id).0;
    let associated_user = get_associated_token_address(user, &mint_pk);
    let event_authority = Pubkey::find_program_address(&[b"__event_authority"], program_id).0;
    let global_vol_acc = Pubkey::find_program_address(&[b"global_volume_accumulator"], program_id).0;
    let user_vol_acc = Pubkey::find_program_address(&[b"user_volume_accumulator", user.as_ref()], program_id).0;

    let global_pda = Pubkey::find_program_address(&[b"global"], program_id).0;

    let mut accounts = vec![
        AccountMeta::new_readonly(global_pda, false),
        AccountMeta::new_readonly(*fee_recipient, false),
        AccountMeta::new(bonding_curve_pda, false),
        AccountMeta::new(associated_user, false),
        AccountMeta::new_readonly(*user, true),
        AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_PUBKEY)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_PUBKEY)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(FEE_PROGRAM_PUBKEY)?, false),
        AccountMeta::new_readonly(mint_pk, false),
        AccountMeta::new(event_authority, false),
        AccountMeta::new(global_vol_acc, false),
        AccountMeta::new(user_vol_acc, false),
    ];

    // Add creator account if provided
    if let Some(creator) = creator_pubkey {
        accounts.push(AccountMeta::new_readonly(creator, false));
    }

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}