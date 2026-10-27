//! Local Raydium CPMM swap construction.
//!
//! Jupiter is convenient, but it cannot be told to use one specific pool, and
//! its public endpoint is heavily rate-limited. A cross-pool arbitrage must
//! trade the *detected* pools: quoting both legs through Jupiter can route both
//! of them back into the same pool and turn a modelled profit into a loss.
//!
//! This module builds Raydium CPMM `swap_base_input` instructions directly
//! from the on-chain account layout. Every offset and discriminator below was
//! checked against the CPMM program source and live accounts:
//!
//! * pool account: 637 bytes, discriminator `f7ede3f5d7c3de46`;
//! * authority seed is `vault_and_lp_mint_auth_seed` (the client constant);
//! * mints at 168/200, vaults at 72/104, token programs at 232/264,
//!   observation at 296, authority bump at 328, status at 329, decimals at
//!   331/332, creator-fee flag at 390;
//! * config account: trade fee (1e-6 units) at offset 12;
//! * instruction discriminator = `sha256("global:swap_base_input")[..8]`;
//! * accounts are exactly the `Swap` struct in `swap_base_input.rs`.
//!
//! The swap output uses Raydium's fee rule: the **trade fee only** is charged on
//! the input, rounded up. Protocol and fund rates are shares of that fee, not
//! additional charges. Creator fees are rejected instead of guessed: a pool
//! with `enable_creator_fee` set is not supported by this builder.
//!
//! Output must be quoted against *effective* reserves: the program subtracts
//! already-booked protocol/fund/creator fees from both vault balances before
//! running the curve (`vault_amount_without_fee`). Quoting gross vault amounts
//! overstates the output and produces on-chain `ExceededSlippage`.

use base64::Engine as _;
use beast_dex::jupiter::{RawAccountMeta, RawInstruction, SwapInstructions};
use beast_dex::Address;
use serde_json::Value;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address_with_program_id;
use std::str::FromStr;
use thiserror::Error;

/// Raydium CPMM program id.
pub const CPMM_PROGRAM_ID: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
/// Verified fixed size of a CPMM pool account.
pub const CPMM_POOL_LEN: usize = 637;
/// Verified fixed size of a CPMM config account (current program version).
pub const CPMM_CONFIG_LEN: usize = 236;
/// Seed used by the program for the pool vault authority.
pub const AUTH_SEED: &[u8] = b"vault_and_lp_mint_auth_seed";
/// Anchor account discriminator for `PoolState`.
pub const POOL_DISCRIMINATOR: [u8; 8] = [0xf7, 0xed, 0xe3, 0xf5, 0xd7, 0xc3, 0xde, 0x46];
/// Anchor account discriminator for `AmmConfig`.
pub const CONFIG_DISCRIMINATOR: [u8; 8] = [0xda, 0xf4, 0x21, 0x68, 0xcb, 0xcb, 0x2b, 0x6f];
/// `sha256("global:swap_base_input")[..8]`.
pub const SWAP_BASE_INPUT_DISCRIMINATOR: [u8; 8] = [0x8f, 0xbe, 0x5a, 0xda, 0xc4, 0x1e, 0x33, 0xde];
/// Status bit 2 disables swapping.
pub const STATUS_SWAP_DISABLED: u8 = 4;
/// SPL token account `amount` offset.
pub const TOKEN_AMOUNT_OFFSET: usize = 64;
/// SPL token account `mint` offset.
pub const TOKEN_MINT_OFFSET: usize = 0;

#[derive(Debug, Error)]
pub enum CpmmError {
    #[error("{account} account is {got} bytes, need {need}")]
    AccountTooShort {
        account: &'static str,
        need: usize,
        got: usize,
    },
    #[error("{account} has discriminator {got}, expected {expected}")]
    BadDiscriminator {
        account: &'static str,
        got: String,
        expected: String,
    },
    #[error("invalid CPMM pubkey: {0}")]
    Pubkey(String),
    #[error("pool is disabled: {0}")]
    Disabled(String),
    #[error("CPMM pool has creator fees enabled; local execution is not verified for this pool")]
    CreatorFeeEnabled,
    #[error("mint {mint} is not part of pool {pool}")]
    MintNotInPool { pool: Pubkey, mint: Pubkey },
    #[error("CPMM math: {0}")]
    Math(String),
    #[error("token account {account}: {reason}")]
    TokenAccount { account: Pubkey, reason: String },
    #[error("instruction serialization: {0}")]
    Encode(String),
}

/// Decoded `PoolState` fields needed to build a swap.
#[derive(Debug, Clone)]
pub struct CpmmPool {
    pub address: Pubkey,
    pub amm_config: Pubkey,
    /// Token 0 and token 1 mints, in account order.
    pub mints: [Pubkey; 2],
    pub vaults: [Pubkey; 2],
    pub token_programs: [Pubkey; 2],
    pub observation: Pubkey,
    pub auth_bump: u8,
    pub status: u8,
    pub decimals: [u8; 2],
    /// Fees the program has booked but not yet collected, per side. The swap
    /// curve runs on vault balances minus these amounts.
    pub protocol_fees: [u64; 2],
    pub fund_fees: [u64; 2],
    pub creator_fees: [u64; 2],
    pub open_time: u64,
    pub creator_fee_enabled: bool,
}

impl CpmmPool {
    /// Decode the fixed CPMM `PoolState` layout.
    pub fn decode(address: Pubkey, data: &[u8]) -> Result<Self, CpmmError> {
        if data.len() < CPMM_POOL_LEN {
            return Err(CpmmError::AccountTooShort {
                account: "CPMM pool",
                need: CPMM_POOL_LEN,
                got: data.len(),
            });
        }
        if data[..8] != POOL_DISCRIMINATOR {
            return Err(CpmmError::BadDiscriminator {
                account: "CPMM pool",
                got: hex(&data[..8]),
                expected: hex(&POOL_DISCRIMINATOR),
            });
        }
        Ok(Self {
            address,
            amm_config: read_pubkey(data, 8)?,
            vaults: [read_pubkey(data, 72)?, read_pubkey(data, 104)?],
            mints: [read_pubkey(data, 168)?, read_pubkey(data, 200)?],
            token_programs: [read_pubkey(data, 232)?, read_pubkey(data, 264)?],
            observation: read_pubkey(data, 296)?,
            auth_bump: data[328],
            status: data[329],
            decimals: [data[331], data[332]],
            protocol_fees: [read_u64(data, 341)?, read_u64(data, 349)?],
            fund_fees: [read_u64(data, 357)?, read_u64(data, 365)?],
            creator_fees: [read_u64(data, 397)?, read_u64(data, 405)?],
            open_time: read_u64(data, 373)?,
            creator_fee_enabled: data[390] != 0,
        })
    }

    /// Return the side index of `mint`, if it is in this pool.
    pub fn mint_index(&self, mint: &Pubkey) -> Option<usize> {
        self.mints.iter().position(|candidate| candidate == mint)
    }

    /// Derive and verify the vault authority PDA.
    pub fn authority(&self) -> Result<Pubkey, CpmmError> {
        let program = Pubkey::from_str(CPMM_PROGRAM_ID)
            .map_err(|error| CpmmError::Pubkey(error.to_string()))?;
        let (authority, bump) = Pubkey::find_program_address(&[AUTH_SEED], &program);
        if bump != self.auth_bump {
            return Err(CpmmError::Math(format!(
                "pool {} stores auth_bump {}, canonical PDA bump is {}",
                self.address, self.auth_bump, bump
            )));
        }
        Ok(authority)
    }

    /// Reject pools the program would reject anyway, with a clearer reason.
    pub fn ensure_swappable(&self) -> Result<(), CpmmError> {
        if self.status & STATUS_SWAP_DISABLED != 0 {
            return Err(CpmmError::Disabled(format!(
                "pool {} status {} disables swaps",
                self.address, self.status
            )));
        }
        if self.creator_fee_enabled {
            return Err(CpmmError::CreatorFeeEnabled);
        }
        Ok(())
    }

    /// Fees booked against one side of the pool and not yet collected.
    pub fn pending_fees(&self, side: usize) -> Result<u64, CpmmError> {
        let (protocol, fund, creator) = self
            .protocol_fees
            .get(side)
            .zip(self.fund_fees.get(side))
            .zip(self.creator_fees.get(side))
            .map(|((protocol, fund), creator)| (*protocol, *fund, *creator))
            .ok_or_else(|| CpmmError::Math(format!("pool side {side} is out of range")))?;
        protocol
            .checked_add(fund)
            .and_then(|value| value.checked_add(creator))
            .ok_or_else(|| CpmmError::Math("pending fee overflow".into()))
    }

    /// Vault balance minus booked-but-uncollected fees: the reserve the swap
    /// curve actually uses.
    pub fn effective_reserve(&self, side: usize, vault_amount: u64) -> Result<u64, CpmmError> {
        let pending = self.pending_fees(side)?;
        vault_amount.checked_sub(pending).ok_or_else(|| {
            CpmmError::Math(format!(
                "pool {} side {side} vault holds {vault_amount}, below its {pending} pending fees",
                self.address
            ))
        })
    }
}

/// Decode the trade fee from a CPMM config account.
///
/// The value is in hundredths of a bip (1e-6). Only `trade_fee_rate` is a
/// charge to the swapper; the protocol/fund/creator rates are shares of it.
pub fn decode_trade_fee_rate(data: &[u8]) -> Result<u64, CpmmError> {
    if data.len() < 20 {
        return Err(CpmmError::AccountTooShort {
            account: "CPMM config",
            need: 20,
            got: data.len(),
        });
    }
    if data[..8] != CONFIG_DISCRIMINATOR {
        return Err(CpmmError::BadDiscriminator {
            account: "CPMM config",
            got: hex(&data[..8]),
            expected: hex(&CONFIG_DISCRIMINATOR),
        });
    }
    read_u64(data, 12)
}

/// Exact Raydium `swap_base_input` output for a constant-product CPMM pool.
///
/// Matches `Fees::trading_fee` (ceiling) followed by
/// `ConstantProductCurve::swap_base_input_without_fees` (floor). No creator fee
/// is applied; callers must reject pools where it is enabled.
pub fn quote_base_input(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    trade_fee_rate: u64,
) -> Result<u64, CpmmError> {
    if amount_in == 0 {
        return Err(CpmmError::Math("amount_in is zero".into()));
    }
    if reserve_in == 0 || reserve_out == 0 {
        return Err(CpmmError::Math("pool side has zero reserves".into()));
    }
    if trade_fee_rate >= 1_000_000 {
        return Err(CpmmError::Math(format!(
            "trade fee {trade_fee_rate} is not below 1e6"
        )));
    }
    let numerator = u128::from(amount_in)
        .checked_mul(u128::from(trade_fee_rate))
        .and_then(|value| value.checked_add(999_999))
        .ok_or_else(|| CpmmError::Math("fee overflow".into()))?;
    let fee = numerator / 1_000_000;
    let net = u128::from(amount_in)
        .checked_sub(fee)
        .ok_or_else(|| CpmmError::Math("fee exceeds input".into()))?;
    if net == 0 {
        return Err(CpmmError::Math("fee consumes the whole input".into()));
    }
    let denominator = u128::from(reserve_in)
        .checked_add(net)
        .ok_or_else(|| CpmmError::Math("reserve overflow".into()))?;
    let out = net
        .checked_mul(u128::from(reserve_out))
        .ok_or_else(|| CpmmError::Math("output overflow".into()))?
        / denominator;
    u64::try_from(out).map_err(|_| CpmmError::Math("output exceeds u64".into()))
}

/// Build a CPMM `swap_base_input` instruction for one pool and direction.
///
/// The user's source and destination token accounts are the associated token
/// accounts derived from `payer`. Callers must ensure both exist before the
/// swap (the flash cycle preamble creates them idempotently).
pub fn swap_base_input_instruction(
    pool: &CpmmPool,
    payer: Pubkey,
    input_mint: Pubkey,
    output_mint: Pubkey,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<Instruction, CpmmError> {
    if amount_in == 0 {
        return Err(CpmmError::Math("swap amount is zero".into()));
    }
    let input_index = pool
        .mint_index(&input_mint)
        .ok_or(CpmmError::MintNotInPool {
            pool: pool.address,
            mint: input_mint,
        })?;
    let output_index = 1 - input_index;
    if pool.mints[output_index] != output_mint {
        return Err(CpmmError::MintNotInPool {
            pool: pool.address,
            mint: output_mint,
        });
    }
    let authority = pool.authority()?;
    let input_token_program = pool.token_programs[input_index];
    let output_token_program = pool.token_programs[output_index];
    let input_token_account =
        get_associated_token_address_with_program_id(&payer, &input_mint, &input_token_program);
    let output_token_account =
        get_associated_token_address_with_program_id(&payer, &output_mint, &output_token_program);

    let accounts = vec![
        // The wallet is the transaction fee payer and therefore already a
        // message-level signer. Marking it again here would trip the composed
        // executor's "no additional signers" check without changing the
        // runtime's signer resolution.
        AccountMeta::new(payer, false),
        AccountMeta::new_readonly(authority, false),
        AccountMeta::new_readonly(pool.amm_config, false),
        AccountMeta::new(pool.address, false),
        AccountMeta::new(input_token_account, false),
        AccountMeta::new(output_token_account, false),
        AccountMeta::new(pool.vaults[input_index], false),
        AccountMeta::new(pool.vaults[output_index], false),
        AccountMeta::new_readonly(input_token_program, false),
        AccountMeta::new_readonly(output_token_program, false),
        AccountMeta::new_readonly(input_mint, false),
        AccountMeta::new_readonly(output_mint, false),
        AccountMeta::new(pool.observation, false),
    ];

    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&SWAP_BASE_INPUT_DISCRIMINATOR);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&minimum_amount_out.to_le_bytes());

    Ok(Instruction {
        program_id: Pubkey::from_str(CPMM_PROGRAM_ID)
            .map_err(|error| CpmmError::Pubkey(error.to_string()))?,
        accounts,
        data,
    })
}

/// Wrap one native swap instruction in the Jupiter instruction envelope so the
/// existing composed-cycle assembler can place it exactly where a Jupiter leg
/// would have gone.
pub fn as_swap_instructions(instruction: Instruction) -> Result<SwapInstructions, CpmmError> {
    let accounts = instruction
        .accounts
        .iter()
        .map(|meta| RawAccountMeta {
            pubkey: Address::new(meta.pubkey.to_bytes()),
            is_signer: meta.is_signer,
            is_writable: meta.is_writable,
        })
        .collect();
    Ok(SwapInstructions {
        token_ledger_instruction: None,
        compute_budget_instructions: Vec::new(),
        setup_instructions: Vec::new(),
        swap_instruction: RawInstruction {
            program_id: Address::new(instruction.program_id.to_bytes()),
            accounts,
            data: base64::engine::general_purpose::STANDARD.encode(instruction.data),
        },
        cleanup_instruction: None,
        other_instructions: Vec::new(),
        address_lookup_table_addresses: Vec::new(),
        prioritization_fee_lamports: Value::from(0),
        compute_unit_limit: None,
        transaction_version: Value::from("legacy"),
        simulation_slot: None,
        dynamic_slippage_report: None,
        simulation_error: Value::Null,
        blockhash_with_metadata: None,
    })
}

/// Read an SPL token account's mint and amount.
pub fn token_account_mint_and_amount(data: &[u8]) -> Result<(Pubkey, u64), CpmmError> {
    if data.len() < TOKEN_AMOUNT_OFFSET + 8 {
        return Err(CpmmError::AccountTooShort {
            account: "SPL token account",
            need: TOKEN_AMOUNT_OFFSET + 8,
            got: data.len(),
        });
    }
    let mint = read_pubkey(data, TOKEN_MINT_OFFSET)?;
    let amount = read_u64(data, TOKEN_AMOUNT_OFFSET)?;
    Ok((mint, amount))
}

/// Derive the associated token account for a wallet, mint, and token program.
pub fn associated_token_account(wallet: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    get_associated_token_address_with_program_id(wallet, mint, token_program)
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, CpmmError> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or(CpmmError::AccountTooShort {
            account: "CPMM account",
            need: offset + 8,
            got: data.len(),
        })?;
    Ok(u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
}

fn read_pubkey(data: &[u8], offset: usize) -> Result<Pubkey, CpmmError> {
    let bytes = data
        .get(offset..offset + 32)
        .ok_or(CpmmError::AccountTooShort {
            account: "CPMM account",
            need: offset + 32,
            got: data.len(),
        })?;
    Ok(Pubkey::new_from_array(bytes.try_into().expect("32 bytes")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WSOL: &str = "So11111111111111111111111111111111111111112";
    const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    const WALLET: &str = "BEAST1kZRXbU2FQs3SDa38t5jb1gxiVvfoQ3Vos2i8QE";

    fn write_key(data: &mut [u8], offset: usize, key: &str) {
        let key: Pubkey = key.parse().unwrap();
        data[offset..offset + 32].copy_from_slice(key.as_ref());
    }

    fn pretty() -> CpmmPool {
        let owner: Pubkey = "v3YN4d7JRhKFcwtex7qNtyg2r5hKYW96Cayb6GivSvY"
            .parse()
            .unwrap();
        CpmmPool {
            address: "fAjTnZ9QqJkUmrr8cXutkYhpVge2qqtSZNt9qKn7YC2"
                .parse()
                .unwrap(),
            amm_config: "FMmwToGqT9iwDgsqvzLg65vcKSzKme4szbWqX1kyCBCJ"
                .parse()
                .unwrap(),
            mints: [WSOL.parse().unwrap(), USDC.parse().unwrap()],
            vaults: [
                owner,
                "HG2WgeYsjqQhnjhwHj7QdTVvjyWpFRk6VREAQ8MURv2u"
                    .parse()
                    .unwrap(),
            ],
            token_programs: [
                "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
                    .parse()
                    .unwrap(),
                "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
                    .parse()
                    .unwrap(),
            ],
            observation: "BeMRiScNtZrU1Ce4D6Kv5FYKLyhGTRbmMq2KPVxupuDv"
                .parse()
                .unwrap(),
            auth_bump: {
                let program: Pubkey = CPMM_PROGRAM_ID.parse().unwrap();
                Pubkey::find_program_address(&[AUTH_SEED], &program).1
            },
            status: 0,
            decimals: [9, 6],
            protocol_fees: [0, 0],
            fund_fees: [0, 0],
            creator_fees: [0, 0],
            open_time: 1_700_000_000,
            creator_fee_enabled: false,
        }
    }

    #[test]
    fn decodes_the_verified_pool_layout() {
        let mut data = vec![0u8; CPMM_POOL_LEN];
        data[..8].copy_from_slice(&POOL_DISCRIMINATOR);
        write_key(&mut data, 8, "FMmwToGqT9iwDgsqvzLg65vcKSzKme4szbWqX1kyCBCJ");
        write_key(&mut data, 72, "v3YN4d7JRhKFcwtex7qNtyg2r5hKYW96Cayb6GivSvY");
        write_key(
            &mut data,
            104,
            "HG2WgeYsjqQhnjhwHj7QdTVvjyWpFRk6VREAQ8MURv2u",
        );
        write_key(&mut data, 168, WSOL);
        write_key(&mut data, 200, USDC);
        write_key(
            &mut data,
            232,
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        );
        write_key(
            &mut data,
            264,
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        );
        write_key(
            &mut data,
            296,
            "BeMRiScNtZrU1Ce4D6Kv5FYKLyhGTRbmMq2KPVxupuDv",
        );
        let program: Pubkey = CPMM_PROGRAM_ID.parse().unwrap();
        data[328] = Pubkey::find_program_address(&[AUTH_SEED], &program).1;
        data[329] = 0;
        data[331] = 9;
        data[332] = 6;
        data[373..381].copy_from_slice(&1_700_000_000u64.to_le_bytes());
        data[341..349].copy_from_slice(&70u64.to_le_bytes());
        data[349..357].copy_from_slice(&7u64.to_le_bytes());
        data[357..365].copy_from_slice(&5u64.to_le_bytes());
        data[365..373].copy_from_slice(&1u64.to_le_bytes());
        data[397..405].copy_from_slice(&2u64.to_le_bytes());
        data[405..413].copy_from_slice(&3u64.to_le_bytes());

        let pool = CpmmPool::decode(
            "fAjTnZ9QqJkUmrr8cXutkYhpVge2qqtSZNt9qKn7YC2"
                .parse()
                .unwrap(),
            &data,
        )
        .unwrap();
        assert_eq!(pool.mints[0].to_string(), WSOL);
        assert_eq!(pool.mints[1].to_string(), USDC);
        assert_eq!(
            pool.vaults[0].to_string(),
            "v3YN4d7JRhKFcwtex7qNtyg2r5hKYW96Cayb6GivSvY"
        );
        assert_eq!(pool.decimals, [9, 6]);
        assert_eq!(pool.protocol_fees, [70, 7]);
        assert_eq!(pool.fund_fees, [5, 1]);
        assert_eq!(pool.creator_fees, [2, 3]);
        assert_eq!(pool.pending_fees(0).unwrap(), 77);
        assert_eq!(pool.pending_fees(1).unwrap(), 11);
        assert_eq!(pool.effective_reserve(0, 1_000).unwrap(), 923);
        assert!(pool.effective_reserve(1, 10).is_err());
        assert!(pool.authority().is_ok());
        pool.ensure_swappable().unwrap();
    }

    #[test]
    fn rejects_creator_fee_pools() {
        let mut data = vec![0u8; CPMM_POOL_LEN];
        data[..8].copy_from_slice(&POOL_DISCRIMINATOR);
        data[390] = 1;
        let pool = CpmmPool::decode(Pubkey::new_unique(), &data).unwrap();
        assert!(matches!(
            pool.ensure_swappable(),
            Err(CpmmError::CreatorFeeEnabled)
        ));
    }

    #[test]
    fn quote_matches_raydium_ceiling_fee_and_floor_output() {
        // 1 SOL into a 100 SOL / 10,000 USDC pool at 25 bps.
        let out = quote_base_input(1_000_000_000, 100_000_000_000, 10_000_000_000, 2_500).unwrap();
        let net = 1_000_000_000u128 - 2_500_000; // fee = ceil(1e9 * 2500 / 1e6) = 2_500_000
        let expected = net * 10_000_000_000u128 / (100_000_000_000u128 + net);
        assert_eq!(out, expected as u64);
        // 50 ppm on 100 units rounds the fee up to 1; the output is 98.
        assert_eq!(quote_base_input(100, 1_000_000, 1_000_000, 50).unwrap(), 98);
        // A one-unit trade is entirely consumed by the one-unit fee.
        assert!(quote_base_input(1, 1_000_000, 1_000_000, 50).is_err());
        assert!(quote_base_input(0, 1, 1, 0).is_err());
        assert!(quote_base_input(1, 1, 1, 1_000_000).is_err());
    }

    #[test]
    fn builds_the_verified_account_order_and_arguments() {
        let pool = pretty();
        let wallet: Pubkey = WALLET.parse().unwrap();
        let input: Pubkey = WSOL.parse().unwrap();
        let output: Pubkey = USDC.parse().unwrap();
        let ix =
            swap_base_input_instruction(&pool, wallet, input, output, 1_000_000, 900_000).unwrap();
        assert_eq!(ix.program_id.to_string(), CPMM_PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 13);
        assert!(!ix.accounts[0].is_signer);
        assert!(ix.accounts[0].is_writable);
        assert_eq!(ix.accounts[1].pubkey, pool.authority().unwrap());
        assert_eq!(ix.accounts[2].pubkey, pool.amm_config);
        assert_eq!(ix.accounts[3].pubkey, pool.address);
        assert_eq!(ix.accounts[6].pubkey, pool.vaults[0]);
        assert_eq!(ix.accounts[7].pubkey, pool.vaults[1]);
        assert_eq!(ix.accounts[10].pubkey, input);
        assert_eq!(ix.accounts[11].pubkey, output);
        assert_eq!(ix.accounts[12].pubkey, pool.observation);
        assert_eq!(&ix.data[..8], &SWAP_BASE_INPUT_DISCRIMINATOR);
        assert_eq!(&ix.data[8..16], &1_000_000u64.to_le_bytes());
        assert_eq!(&ix.data[16..24], &900_000u64.to_le_bytes());

        let reversed = swap_base_input_instruction(&pool, wallet, output, input, 1, 0).unwrap();
        assert_eq!(reversed.accounts[6].pubkey, pool.vaults[1]);
        assert_eq!(reversed.accounts[7].pubkey, pool.vaults[0]);

        assert!(matches!(
            swap_base_input_instruction(&pool, wallet, output, output, 1, 0),
            Err(CpmmError::MintNotInPool { .. })
        ));
    }

    #[test]
    fn wrapper_round_trips_the_native_instruction() {
        let pool = pretty();
        let wallet: Pubkey = WALLET.parse().unwrap();
        let ix = swap_base_input_instruction(
            &pool,
            wallet,
            WSOL.parse().unwrap(),
            USDC.parse().unwrap(),
            10,
            9,
        )
        .unwrap();
        let envelope = as_swap_instructions(ix.clone()).unwrap();
        let raw = envelope.swap_instruction;
        assert_eq!(
            raw.program_id.to_string(),
            "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C"
        );
        assert_eq!(raw.accounts.len(), 13);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(raw.data)
                .unwrap(),
            ix.data
        );
    }

    #[test]
    fn reads_token_account_amount_and_rejects_short_data() {
        let mut data = vec![0u8; 165];
        write_key(&mut data, 0, USDC);
        data[64..72].copy_from_slice(&123_456_789u64.to_le_bytes());
        let (mint, amount) = token_account_mint_and_amount(&data).unwrap();
        assert_eq!(mint.to_string(), USDC);
        assert_eq!(amount, 123_456_789);
        assert!(token_account_mint_and_amount(&data[..70]).is_err());
    }
}
