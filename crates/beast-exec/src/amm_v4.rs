//! Local Raydium AMM v4 swap construction.
//!
//! AMM v4 is the deepest constant-product venue on Solana for the majors: the
//! SOL/USDC pool alone holds roughly 94,000 SOL of base liquidity, against a
//! few SOL in the CPMM pools the driver could previously trade. It was left
//! out until now because every documented swap instruction wanted the bound
//! OpenBook market's account list (bids, asks, event queue, vault signer), and
//! guessing those addresses is exactly the kind of guess that turns a modelled
//! profit into a reverted transaction.
//!
//! Raydium has since removed the OpenBook integration from AMM v4 — it is now a
//! pure constant-product AMM — and the program exposes a swap that takes no
//! market accounts at all. That is instruction 16, `swap_base_in` v2:
//!
//! ```text
//! accounts: token_program, pool, authority, vault_a, vault_b,
//!           user_token_in, user_token_out, owner
//! data:     u8 instruction_id = 16, u64 amount_in, u64 minimum_amount_out
//! ```
//!
//! Nothing in that list has to be guessed: the pool, the vaults and the
//! authority all come out of the pool account, and the two token accounts are
//! the wallet's associated token accounts. The instruction id is part of the
//! program's ABI, so a wrong id fails simulation rather than doing something
//! wrong, and simulation is how this module is proven before it is trusted.
//!
//! Layout notes, all checked against live accounts:
//!
//! * the pool account is 752 bytes and owned by the AMM v4 program;
//! * the swap fee is `swap_fee_numerator / swap_fee_denominator` read from the
//!   account at 176/184 — it is never hardcoded, because a pool can carry a
//!   different fee from the program default;
//! * `base_vault` is at 336, `quote_vault` at 368, `base_mint` at 400,
//!   `quote_mint` at 432, `open_orders` at 496;
//! * the authority is the PDA over `[pool, open_orders]` under the program id,
//!   which is also the owner of both vaults.
//!
//! The `status` word at offset 0 is deliberately not interpreted. Its bit
//! meanings have changed across program revisions, and the values observed on
//! live pools do not match any published enum, so reading it as a permission
//! would be a guess. The program validates its own state; a disabled pool
//! fails simulation and is rejected there instead.

use beast_dex::jupiter::{RawAccountMeta, RawInstruction, SwapInstructions};
use beast_dex::Address;
use serde_json::Value;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address_with_program_id;
use std::str::FromStr;
use thiserror::Error;

/// Raydium AMM v4 program id.
pub const AMM_V4_PROGRAM_ID: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
/// Verified fixed size of an AMM v4 pool account.
pub const AMM_V4_POOL_LEN: usize = 752;
/// Seed of the single program-wide vault authority PDA.
pub const AUTHORITY_SEED: &[u8] = b"amm authority";
/// Instruction id of the v2 base-in swap, which needs no OpenBook accounts.
pub const SWAP_BASE_IN_V2: u8 = 16;

/// `swap_fee_numerator` offset.
pub const SWAP_FEE_NUMERATOR_OFFSET: usize = 176;
/// `swap_fee_denominator` offset.
pub const SWAP_FEE_DENOMINATOR_OFFSET: usize = 184;
/// `pool_open_time` offset.
pub const POOL_OPEN_TIME_OFFSET: usize = 224;
/// Unclaimed protocol PnL on the base side, excluded from pricing.
pub const NEED_TAKE_PNL_COIN_OFFSET: usize = 192;
/// Unclaimed protocol PnL on the quote side, excluded from pricing.
pub const NEED_TAKE_PNL_PC_OFFSET: usize = 200;
/// `base_vault` offset.
pub const BASE_VAULT_OFFSET: usize = 336;
/// `quote_vault` offset.
pub const QUOTE_VAULT_OFFSET: usize = 368;
/// `base_mint` offset.
pub const BASE_MINT_OFFSET: usize = 400;
/// `quote_mint` offset.
pub const QUOTE_MINT_OFFSET: usize = 432;
/// `open_orders` offset.
pub const OPEN_ORDERS_OFFSET: usize = 496;

/// SPL token account `amount` offset.
const TOKEN_AMOUNT_OFFSET: usize = 64;

#[derive(Debug, Error)]
pub enum AmmV4Error {
    #[error("{account} account is {got} bytes, need {need}")]
    AccountTooShort {
        account: &'static str,
        need: usize,
        got: usize,
    },
    #[error("invalid AMM v4 pubkey: {0}")]
    Pubkey(String),
    #[error("pool {pool} does not contain mint {mint}")]
    MintNotInPool { pool: Pubkey, mint: Pubkey },
    #[error("AMM v4 math: {0}")]
    Math(String),
    #[error("pool {pool} has a zero {side} fee denominator")]
    ZeroFeeDenominator { pool: Pubkey, side: &'static str },
    #[error("instruction serialization: {0}")]
    Encode(String),
}

/// Decoded AMM v4 fields needed to build a swap.
#[derive(Debug, Clone)]
pub struct AmmV4Pool {
    pub address: Pubkey,
    /// Token 0 (base) and token 1 (quote) mints, in account order.
    pub mints: [Pubkey; 2],
    pub vaults: [Pubkey; 2],
    pub open_orders: Pubkey,
    pub fee_numerator: u64,
    pub fee_denominator: u64,
    pub open_time: u64,
    /// Protocol PnL that has been booked but not yet withdrawn. Raydium prices
    /// against the vault balance minus these amounts.
    pub need_take_pnl_coin: u64,
    pub need_take_pnl_pc: u64,
}

impl AmmV4Pool {
    /// Decode the fixed AMM v4 pool layout.
    pub fn decode(address: Pubkey, data: &[u8]) -> Result<Self, AmmV4Error> {
        if data.len() < AMM_V4_POOL_LEN {
            return Err(AmmV4Error::AccountTooShort {
                account: "AMM v4 pool",
                need: AMM_V4_POOL_LEN,
                got: data.len(),
            });
        }
        Ok(Self {
            address,
            mints: [
                read_pubkey(data, BASE_MINT_OFFSET)?,
                read_pubkey(data, QUOTE_MINT_OFFSET)?,
            ],
            vaults: [
                read_pubkey(data, BASE_VAULT_OFFSET)?,
                read_pubkey(data, QUOTE_VAULT_OFFSET)?,
            ],
            open_orders: read_pubkey(data, OPEN_ORDERS_OFFSET)?,
            fee_numerator: read_u64(data, SWAP_FEE_NUMERATOR_OFFSET)?,
            fee_denominator: read_u64(data, SWAP_FEE_DENOMINATOR_OFFSET)?,
            open_time: read_u64(data, POOL_OPEN_TIME_OFFSET)?,
            need_take_pnl_coin: read_u64(data, NEED_TAKE_PNL_COIN_OFFSET)?,
            need_take_pnl_pc: read_u64(data, NEED_TAKE_PNL_PC_OFFSET)?,
        })
    }

    /// Index of `mint` in the pool's `[base, quote]` order.
    pub fn mint_index(&self, mint: &Pubkey) -> Option<usize> {
        self.mints.iter().position(|candidate| candidate == mint)
    }

    /// The reserve the program actually prices against.
    ///
    /// Raydium books its protocol share of every swap as `need_take_pnl_*` and
    /// excludes it from the curve until the creator withdraws it. On a deep
    /// pool that is a rounding error; on a thin one it dominates. This pool's
    /// vault holds 12,720,754 lamports while the program prices against
    /// 9,169,922 — pricing the raw balance overstates the output by 12% and
    /// turns every detected profit into a reverted swap.
    pub fn effective_reserve(&self, side: usize, raw: u64) -> u64 {
        let booked = if side == 0 {
            self.need_take_pnl_coin
        } else {
            self.need_take_pnl_pc
        };
        raw.saturating_sub(booked)
    }

    /// The vault authority, which owns both vaults.
    ///
    /// This is one PDA shared by every AMM v4 pool — seeds `[b"amm authority"]`
    /// under the program id — not a per-pool derivation. The golden test below
    /// pins it against the account that actually owns the live SOL/USDC
    /// vaults, which is how the wrong per-pool seed order was caught.
    pub fn authority(&self) -> Result<Pubkey, AmmV4Error> {
        let program = Pubkey::from_str(AMM_V4_PROGRAM_ID)
            .map_err(|error| AmmV4Error::Pubkey(error.to_string()))?;
        let (authority, _bump) = Pubkey::find_program_address(&[AUTHORITY_SEED], &program);
        Ok(authority)
    }

    /// Fee charged on the input side, rounded up, matching the program.
    pub fn fee_on(&self, amount_in: u64) -> Result<u64, AmmV4Error> {
        if self.fee_denominator == 0 {
            return Err(AmmV4Error::ZeroFeeDenominator {
                pool: self.address,
                side: "swap",
            });
        }
        let numerator = u128::from(amount_in)
            .checked_mul(u128::from(self.fee_numerator))
            .and_then(|value| value.checked_add(u128::from(self.fee_denominator) - 1))
            .ok_or_else(|| AmmV4Error::Math("fee overflow".into()))?;
        let fee = numerator / u128::from(self.fee_denominator);
        u64::try_from(fee).map_err(|_| AmmV4Error::Math("fee exceeds u64".into()))
    }
}

/// Exact AMM v4 `swap_base_in` output for a constant-product pool.
///
/// The program takes the swap fee off the input (rounded up), then runs the
/// constant-product curve on the net input and floors the output:
///
/// ```text
/// fee = ceil(amount_in * num / den)
/// out = reserve_out * (amount_in - fee) / (reserve_in + amount_in - fee)
/// ```
pub fn quote_base_in(
    pool: &AmmV4Pool,
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
) -> Result<u64, AmmV4Error> {
    if amount_in == 0 {
        return Err(AmmV4Error::Math("amount_in is zero".into()));
    }
    if reserve_in == 0 || reserve_out == 0 {
        return Err(AmmV4Error::Math("pool side has zero reserves".into()));
    }
    let fee = pool.fee_on(amount_in)?;
    let net = amount_in
        .checked_sub(fee)
        .ok_or_else(|| AmmV4Error::Math("fee exceeds input".into()))?;
    if net == 0 {
        return Err(AmmV4Error::Math("fee consumes the whole input".into()));
    }
    let denominator = u128::from(reserve_in)
        .checked_add(u128::from(net))
        .ok_or_else(|| AmmV4Error::Math("reserve overflow".into()))?;
    let out = u128::from(net)
        .checked_mul(u128::from(reserve_out))
        .ok_or_else(|| AmmV4Error::Math("output overflow".into()))?
        / denominator;
    u64::try_from(out).map_err(|_| AmmV4Error::Math("output exceeds u64".into()))
}

/// Build an AMM v4 `swap_base_in` v2 instruction for one pool and direction.
///
/// `payer` is the wallet: it must be the transaction fee payer, because the
/// composed executor holds exactly one keypair and rejects any instruction
/// that asks for a second signer. Marking the owner here as a signer is
/// unnecessary — the wallet is already a message-level signer — and it would
/// be rejected, so it is marked read-only and non-signing.
pub fn swap_base_in_instruction(
    pool: &AmmV4Pool,
    payer: Pubkey,
    input_mint: Pubkey,
    output_mint: Pubkey,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<Instruction, AmmV4Error> {
    if amount_in == 0 {
        return Err(AmmV4Error::Math("swap amount is zero".into()));
    }
    let input_index = pool
        .mint_index(&input_mint)
        .ok_or(AmmV4Error::MintNotInPool {
            pool: pool.address,
            mint: input_mint,
        })?;
    let output_index = 1 - input_index;
    if pool.mints[output_index] != output_mint {
        return Err(AmmV4Error::MintNotInPool {
            pool: pool.address,
            mint: output_mint,
        });
    }

    let token_program = spl_token::id();
    let authority = pool.authority()?;
    let input_token_account =
        get_associated_token_address_with_program_id(&payer, &input_mint, &token_program);
    let output_token_account =
        get_associated_token_address_with_program_id(&payer, &output_mint, &token_program);

    let accounts = vec![
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new(pool.address, false),
        AccountMeta::new_readonly(authority, false),
        // The program determines direction from the user's source account, so
        // the vaults are always passed in the pool's own base/quote order.
        AccountMeta::new(pool.vaults[0], false),
        AccountMeta::new(pool.vaults[1], false),
        AccountMeta::new(input_token_account, false),
        AccountMeta::new(output_token_account, false),
        AccountMeta::new_readonly(payer, false),
    ];

    let mut data = Vec::with_capacity(17);
    data.push(SWAP_BASE_IN_V2);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&minimum_amount_out.to_le_bytes());

    Ok(Instruction {
        program_id: Pubkey::from_str(AMM_V4_PROGRAM_ID)
            .map_err(|error| AmmV4Error::Pubkey(error.to_string()))?,
        accounts,
        data,
    })
}

/// Wrap one native swap instruction in the Jupiter instruction envelope so the
/// composed-cycle assembler can place it where a Jupiter leg would have gone.
pub fn as_swap_instructions(instruction: Instruction) -> Result<SwapInstructions, AmmV4Error> {
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
            data: base64_encode(&instruction.data),
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
pub fn token_account_mint_and_amount(data: &[u8]) -> Result<(Pubkey, u64), AmmV4Error> {
    if data.len() < TOKEN_AMOUNT_OFFSET + 8 {
        return Err(AmmV4Error::AccountTooShort {
            account: "SPL token account",
            need: TOKEN_AMOUNT_OFFSET + 8,
            got: data.len(),
        });
    }
    Ok((read_pubkey(data, 0)?, read_u64(data, TOKEN_AMOUNT_OFFSET)?))
}

/// Read a token account amount from a `simulateTransaction` account snapshot.
fn account_amount(value: &serde_json::Value) -> Option<u64> {
    use base64::Engine as _;
    let encoded = value.get("data")?.as_array()?.first()?.as_str()?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    if data.len() < TOKEN_AMOUNT_OFFSET + 8 {
        return None;
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_OFFSET + 8]);
    Some(u64::from_le_bytes(bytes))
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, AmmV4Error> {
    let slice = data
        .get(offset..offset + 8)
        .ok_or(AmmV4Error::AccountTooShort {
            account: "AMM v4 pool",
            need: offset + 8,
            got: data.len(),
        })?;
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(slice);
    Ok(u64::from_le_bytes(bytes))
}

fn read_pubkey(data: &[u8], offset: usize) -> Result<Pubkey, AmmV4Error> {
    let slice = data
        .get(offset..offset + 32)
        .ok_or(AmmV4Error::AccountTooShort {
            account: "AMM v4 pool",
            need: offset + 32,
            got: data.len(),
        })?;
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(slice);
    Ok(Pubkey::new_from_array(bytes))
}

/// The observable result of simulating one AMM v4 swap on mainnet.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SwapProbe {
    pub pool: String,
    pub input_mint: String,
    pub output_mint: String,
    pub amount_in: u64,
    pub reserve_in: u64,
    pub reserve_out: u64,
    pub quoted_out: u64,
    pub minimum_out: u64,
    pub need_take_pnl_coin: u64,
    pub need_take_pnl_pc: u64,
    /// What the output vault actually gained, read from the simulation's
    /// post-execution snapshot. `None` when the snapshot was unavailable.
    pub actual_out: Option<u64>,
    /// `quoted_out - actual_out`. Zero means the local pricer is exact.
    pub quote_error: Option<i64>,
    /// `None` means the simulation succeeded.
    pub err: Option<serde_json::Value>,
    pub logs: Vec<String>,
    pub units_consumed: Option<u64>,
}

impl SwapProbe {
    pub fn ok(&self) -> bool {
        self.err.is_none()
    }
}

/// Prove one AMM v4 swap against mainnet without sending anything.
///
/// The probe wraps `amount_in` lamports into the wallet's WSOL account and
/// swaps them for the pool's quote mint. It is a simulation: nothing is signed
/// into a block and nothing is spent, but every account the instruction names
/// must exist and every constraint the program enforces must hold, so a
/// success is proof that the account list, the instruction id and the quoted
/// minimum are all correct.
///
/// The pool's base mint is the input. That keeps the probe unambiguous: the
/// direction is read from the pool rather than assumed by the caller.
pub async fn probe_swap(
    rpc: &beast_rpc::RpcClient,
    wallet: &crate::Wallet,
    pool_address: Address,
    amount_in: u64,
    slippage_bps: u16,
) -> crate::Result<SwapProbe> {
    #[allow(deprecated)]
    use solana_sdk::system_instruction;
    use solana_sdk::transaction::Transaction;

    let account = rpc.get_account_info(pool_address).await?.ok_or_else(|| {
        crate::ExecutionError::Unsupported(format!("AMM v4 pool {pool_address} not found"))
    })?;
    let pool = AmmV4Pool::decode(beast_flash::pubkey(&pool_address), &account.account.data)?;

    let vaults = rpc
        .get_multiple_accounts(&[
            beast_flash::address(&pool.vaults[0]),
            beast_flash::address(&pool.vaults[1]),
        ])
        .await?;
    let mut reserves = [0u64; 2];
    for (index, entry) in vaults.iter().enumerate() {
        let entry = entry.as_ref().ok_or_else(|| {
            crate::ExecutionError::Unsupported(format!(
                "AMM v4 vault {} not found",
                pool.vaults[index]
            ))
        })?;
        let (mint, amount) = token_account_mint_and_amount(&entry.data)?;
        if mint != pool.mints[index] {
            return Err(crate::ExecutionError::Unsupported(format!(
                "AMM v4 vault {index} holds mint {mint}, expected {}",
                pool.mints[index]
            )));
        }
        reserves[index] = amount;
    }

    let quoted_out = quote_base_in(
        &pool,
        amount_in,
        pool.effective_reserve(0, reserves[0]),
        pool.effective_reserve(1, reserves[1]),
    )
        .map_err(|error| crate::ExecutionError::Unsupported(error.to_string()))?;
    let minimum_out = quoted_out.saturating_mul(u64::from(10_000 - slippage_bps)) / 10_000;

    let token_program = spl_token::id();
    let payer = wallet.pubkey();
    let input_ata =
        get_associated_token_address_with_program_id(&payer, &pool.mints[0], &token_program);

    let instructions = vec![
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &payer,
            &payer,
            &pool.mints[0],
            &token_program,
        ),
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &payer,
            &payer,
            &pool.mints[1],
            &token_program,
        ),
        system_instruction::transfer(&payer, &input_ata, amount_in),
        spl_token::instruction::sync_native(&token_program, &input_ata)
            .map_err(|error| crate::ExecutionError::Transaction(format!("sync_native: {error}")))?,
        swap_base_in_instruction(
            &pool,
            payer,
            pool.mints[0],
            pool.mints[1],
            amount_in,
            minimum_out,
        )?,
    ];

    let (blockhash, _slot) = rpc.get_latest_blockhash().await?;
    let blockhash = blockhash
        .parse()
        .map_err(|error| crate::ExecutionError::Transaction(format!("blockhash: {error}")))?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer),
        &[wallet.keypair()],
        blockhash,
    );
    let encoded = base64_encode(
        &bincode::serialize(&transaction)
            .map_err(|error| crate::ExecutionError::Transaction(error.to_string()))?,
    );
    // Ask for the post-execution snapshot of the output vault: comparing the
    // vault's actual gain against the local quote is the only way to know the
    // pricer is exact rather than approximately right. An approximate quote is
    // what turns a detected profit into a reverted transaction.
    let simulation = rpc
        .simulate_transaction_with_accounts(
            &encoded,
            &[pool.vaults[0].to_string(), pool.vaults[1].to_string()],
            false,
        )
        .await?;

    let actual_out = simulation
        .post_accounts
        .get(1)
        .and_then(|value| value.as_ref())
        .and_then(account_amount)
        // Selling the base mint into the pool makes the quote vault *shrink*,
        // so the payout is the decrease, not the increase. Getting this
        // direction wrong is invisible once clamped at zero, which is exactly
        // how it read as "actual 0" on the first run.
        .map(|after| reserves[1].saturating_sub(after));
    let quote_error = actual_out.map(|actual| quoted_out as i64 - actual as i64);

    Ok(SwapProbe {
        pool: pool_address.to_string(),
        input_mint: pool.mints[0].to_string(),
        output_mint: pool.mints[1].to_string(),
        amount_in,
        reserve_in: reserves[0],
        reserve_out: reserves[1],
        quoted_out,
        minimum_out,
        need_take_pnl_coin: pool.need_take_pnl_coin,
        need_take_pnl_pc: pool.need_take_pnl_pc,
        actual_out,
        quote_error,
        err: simulation.err,
        logs: simulation.logs,
        units_consumed: simulation.units_consumed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real SOL/USDC AMM v4 pool, captured from mainnet.
    const SOL_USDC: &str = "58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2";
    const SOL_USDC_AUTHORITY: &str = "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1";

    fn pool() -> AmmV4Pool {
        AmmV4Pool {
            address: Pubkey::from_str(SOL_USDC).unwrap(),
            mints: [
                spl_token::native_mint::id(),
                Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap(),
            ],
            vaults: [
                Pubkey::from_str("DQyrAcCrDXQ7NeoqGgDCZwBvWDcYmFCjSb9JtteuvPpz").unwrap(),
                Pubkey::from_str("HLmqeL62xR1QoZ1HKKbXRrdN1p3phKpxRMb2VVopvBBz").unwrap(),
            ],
            open_orders: Pubkey::from_str("HmiHHzq4Fym9e1D4qzLS6LDDM3tNsCTBPDWHTLZ763jY").unwrap(),
            fee_numerator: 25,
            fee_denominator: 10_000,
            open_time: 0,
            need_take_pnl_coin: 0,
            need_take_pnl_pc: 0,
        }
    }

    #[test]
    fn authority_matches_the_live_vault_owner() {
        // The seed derivation is only correct if it reproduces the account that
        // actually owns the vaults on mainnet. The authority turned out to be a
        // single program-wide PDA rather than a per-pool one, and this is the
        // check that caught it before any transaction was built.
        assert_eq!(
            pool().authority().unwrap(),
            Pubkey::from_str(SOL_USDC_AUTHORITY).unwrap()
        );
    }

    #[test]
    fn effective_reserve_excludes_booked_pnl() {
        // Mainnet EUvnsWhM...: the base vault holds 12,720,754 lamports, but
        // the program prices against 9,169,922 because 3,550,832 lamports of
        // protocol PnL are booked and unclaimed. Those exact numbers appear in
        // the program's own `ray_log` during a simulated swap, which is how the
        // omission was found.
        let mut pool = pool();
        pool.need_take_pnl_coin = 3_550_832;
        pool.need_take_pnl_pc = 110_885;
        assert_eq!(pool.effective_reserve(0, 12_720_754), 9_169_922);
        assert_eq!(pool.effective_reserve(1, 1_049_744), 938_859);
    }

    #[test]
    fn quote_matches_the_mainnet_dust_pool_once_pnl_is_excluded() {
        // A simulated 5,000,000-lamport swap against EUvnsWhM... returned
        // exactly 330,749 USDC units. Pricing the raw vault instead quotes
        // 295,658 — 12% low, which is the error that reverted every cycle.
        let mut pool = pool();
        pool.need_take_pnl_coin = 3_550_832;
        pool.need_take_pnl_pc = 110_885;
        let out = quote_base_in(
            &pool,
            5_000_000,
            pool.effective_reserve(0, 12_720_754),
            pool.effective_reserve(1, 1_049_744),
        )
        .unwrap();
        assert_eq!(out, 330_749);
    }

    #[test]
    fn fee_is_ceil_of_the_ratio() {
        let pool = pool();
        // 0.25% of 1 SOL.
        assert_eq!(pool.fee_on(1_000_000_000).unwrap(), 2_500_000);
        // Rounded up: 1 lamport at 25/10000 is 0.0025, which ceil to 1.
        assert_eq!(pool.fee_on(1).unwrap(), 1);
    }

    #[test]
    fn quote_floors_and_respects_the_curve() {
        let pool = pool();
        let out =
            quote_base_in(&pool, 1_000_000_000, 94_000_000_000_000, 9_500_000_000_000).unwrap();
        // 1 SOL in at 0.25% fee against these reserves is a little over 100 USDC.
        assert!(out > 100_000_000, "got {out}");
        assert!(out < 102_000_000, "got {out}");
        // Twice the input must not more than double the output.
        let double =
            quote_base_in(&pool, 2_000_000_000, 94_000_000_000_000, 9_500_000_000_000).unwrap();
        assert!(double < out * 2);
    }

    #[test]
    fn zero_fee_denominator_is_rejected() {
        let mut pool = pool();
        pool.fee_denominator = 0;
        assert!(matches!(
            pool.fee_on(1_000),
            Err(AmmV4Error::ZeroFeeDenominator { .. })
        ));
    }

    #[test]
    fn instruction_has_the_v2_account_shape() {
        let pool = pool();
        let payer = Pubkey::new_unique();
        let wsol = spl_token::native_mint::id();
        let usdc = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
        let instruction = swap_base_in_instruction(&pool, payer, wsol, usdc, 1_000, 900).unwrap();

        assert_eq!(
            instruction.program_id,
            Pubkey::from_str(AMM_V4_PROGRAM_ID).unwrap()
        );
        assert_eq!(instruction.accounts.len(), 8);
        assert_eq!(instruction.accounts[0].pubkey, spl_token::id());
        assert_eq!(instruction.accounts[1].pubkey, pool.address);
        assert_eq!(instruction.accounts[2].pubkey, pool.authority().unwrap());
        assert_eq!(instruction.accounts[3].pubkey, pool.vaults[0]);
        assert_eq!(instruction.accounts[4].pubkey, pool.vaults[1]);
        assert_eq!(instruction.data.len(), 17);
        assert_eq!(instruction.data[0], SWAP_BASE_IN_V2);
        // No signer: the wallet is the fee payer and signs at the message level.
        assert!(instruction.accounts.iter().all(|meta| !meta.is_signer));
    }

    #[test]
    fn wrong_mint_is_rejected() {
        let pool = pool();
        let payer = Pubkey::new_unique();
        let stray = Pubkey::new_unique();
        assert!(matches!(
            swap_base_in_instruction(&pool, payer, stray, pool.mints[1], 1, 1),
            Err(AmmV4Error::MintNotInPool { .. })
        ));
    }
}
