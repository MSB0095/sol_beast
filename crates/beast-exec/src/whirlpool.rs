//! Orca Whirlpool: decoding, exact local pricing, and swap construction.
//!
//! Whirlpool is a concentrated-liquidity AMM, so it cannot be priced from two
//! vault balances the way a constant-product pool can. The price is
//! `sqrt_price` (Q64.64), the depth is `liquidity` (Q64.64 token units per unit
//! of sqrt-price), and a swap walks initialized ticks. This module implements
//! the program's own math — the 256-bit multiply/divide, the ceil/floor
//! rounding, and the tick traversal — so a quote computed here is the quote the
//! on-chain program computes, not an approximation of it.
//!
//! Facts verified against the program source (`orca-so/whirlpools`):
//!
//! * `Whirlpool` is 653 bytes; `tick_spacing` at 41, `fee_tier_index_seed` at
//!   43, `fee_rate` at 45, `protocol_fee_rate` at 47, `liquidity` at 49,
//!   `sqrt_price` at 65, `tick_current_index` at 81, mints at 101 / 181,
//!   vaults at 133 / 213;
//! * a `Tick` is 113 packed bytes: `initialized` at 0, `liquidity_net` (i128)
//!   at 1, `liquidity_gross` (u128) at 17;
//! * a fixed `TickArray` is `8 + 4 + 88 * 113 + 32 = 9_988` bytes, with
//!   `start_tick_index` at 8 and the ticks at 12;
//! * tick-array addresses are PDAs over `[b"tick_array", whirlpool, start]`
//!   under the program id, and the oracle is `[b"oracle", whirlpool]`;
//! * `fee_rate` is in hundredths of a basis point (`1e6` = 100%);
//! * a pool using an adaptive fee tier has `fee_tier_index != tick_spacing`.
//!   Those pools need oracle state the static quote does not model, so they are
//!   rejected rather than mispriced.
//!
//! The `Swap` instruction takes 11 accounts in a fixed order and the args
//! `amount, other_amount_threshold, sqrt_price_limit, amount_specified_is_input,
//! a_to_b`. It needs up to three tick arrays in the direction of the trade.

use crate::u256::{mul_u256, U256};
use beast_dex::jupiter::{RawAccountMeta, RawInstruction, SwapInstructions};
use beast_dex::Address;
use serde_json::Value;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use thiserror::Error;

/// The Whirlpool program id.
pub const WHIRLPOOL_PROGRAM_ID: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";
/// `Whirlpool` account length.
pub const WHIRLPOOL_LEN: usize = 653;
/// Number of ticks in a tick array.
pub const TICK_ARRAY_SIZE: usize = 88;
/// Packed `Tick` length.
pub const TICK_LEN: usize = 113;
/// A fixed tick array: 8 discriminator + 4 start index + ticks + 32 pool.
pub const TICK_ARRAY_LEN: usize = 8 + 4 + TICK_ARRAY_SIZE * TICK_LEN + 32;
/// Smallest and largest usable tick indices.
pub const MIN_TICK_INDEX: i32 = -443_636;
pub const MAX_TICK_INDEX: i32 = 443_636;

/// Fee rates are hundredths of a basis point; `1_000_000` is 100%.
pub const FEE_RATE_MUL_VALUE: u128 = 1_000_000;
/// Q64.64 fractional resolution.
pub const Q64_RESOLUTION: u8 = 64;
/// The low 64 bits of a Q64.64 number.
pub const Q64_MASK: u128 = 0xFFFF_FFFF_FFFF_FFFF;
/// Absolute bounds of the Q64.64 sqrt price.
pub const MIN_SQRT_PRICE_X64: u128 = 4_295_048_016;
pub const MAX_SQRT_PRICE_X64: u128 = 79_226_673_515_401_279_992_447_579_055;

// `Whirlpool` field offsets.
const CONFIG_OFFSET: usize = 8;
const TICK_SPACING_OFFSET: usize = 41;
const FEE_TIER_INDEX_OFFSET: usize = 43;
const FEE_RATE_OFFSET: usize = 45;
const PROTOCOL_FEE_RATE_OFFSET: usize = 47;
const LIQUIDITY_OFFSET: usize = 49;
const SQRT_PRICE_OFFSET: usize = 65;
const TICK_CURRENT_OFFSET: usize = 81;
pub const MINT_A_OFFSET: usize = 101;
const VAULT_A_OFFSET: usize = 133;
pub const MINT_B_OFFSET: usize = 181;
const VAULT_B_OFFSET: usize = 213;

// `TickArray` / `Tick` field offsets.
const TICK_ARRAY_START_OFFSET: usize = 8;
const TICK_ARRAY_TICKS_OFFSET: usize = 12;
const TICK_ARRAY_POOL_OFFSET: usize = TICK_ARRAY_TICKS_OFFSET + TICK_ARRAY_SIZE * TICK_LEN;

/// `Swap` discriminator (Anchor `global:swap`).
pub const SWAP_DISCRIMINATOR: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];

#[derive(Debug, Error)]
pub enum WhirlpoolError {
    #[error("{account} is {got} bytes, need {need}")]
    AccountTooShort {
        account: &'static str,
        need: usize,
        got: usize,
    },
    #[error("whirlpool pool is not swappable: {0}")]
    Unsupported(String),
    #[error("whirlpool math: {0}")]
    Math(String),
    #[error("whirlpool pubkey: {0}")]
    Pubkey(String),
}

type Result<T> = std::result::Result<T, WhirlpoolError>;

/// A decoded Whirlpool plus the tick arrays needed to price a swap in one
/// direction.
#[derive(Debug, Clone)]
pub struct WhirlpoolPool {
    pub address: Pubkey,
    pub config: Pubkey,
    pub tick_spacing: u16,
    pub fee_tier_index: u16,
    pub fee_rate: u16,
    pub protocol_fee_rate: u16,
    pub liquidity: u128,
    pub sqrt_price: u128,
    pub tick_current_index: i32,
    pub mints: [Pubkey; 2],
    pub vaults: [Pubkey; 2],
}

impl WhirlpoolPool {
    /// Decode the fixed pool layout.
    pub fn decode(address: Pubkey, data: &[u8]) -> Result<Self> {
        if data.len() < WHIRLPOOL_LEN {
            return Err(WhirlpoolError::AccountTooShort {
                account: "whirlpool",
                need: WHIRLPOOL_LEN,
                got: data.len(),
            });
        }
        Ok(Self {
            address,
            config: read_pubkey(data, CONFIG_OFFSET)?,
            tick_spacing: read_u16(data, TICK_SPACING_OFFSET)?,
            fee_tier_index: read_u16(data, FEE_TIER_INDEX_OFFSET)?,
            fee_rate: read_u16(data, FEE_RATE_OFFSET)?,
            protocol_fee_rate: read_u16(data, PROTOCOL_FEE_RATE_OFFSET)?,
            liquidity: read_u128(data, LIQUIDITY_OFFSET)?,
            sqrt_price: read_u128(data, SQRT_PRICE_OFFSET)?,
            tick_current_index: i32::from_le_bytes(
                data[TICK_CURRENT_OFFSET..TICK_CURRENT_OFFSET + 4]
                    .try_into()
                    .expect("4 bytes"),
            ),
            mints: [
                read_pubkey(data, MINT_A_OFFSET)?,
                read_pubkey(data, MINT_B_OFFSET)?,
            ],
            vaults: [
                read_pubkey(data, VAULT_A_OFFSET)?,
                read_pubkey(data, VAULT_B_OFFSET)?,
            ],
        })
    }

    /// Reject pools this pricer cannot quote exactly.
    pub fn ensure_swappable(&self) -> Result<()> {
        if self.tick_spacing == 0 {
            return Err(WhirlpoolError::Unsupported("tick spacing is zero".into()));
        }
        if self.fee_tier_index != self.tick_spacing {
            return Err(WhirlpoolError::Unsupported(
                "adaptive-fee pool; oracle state is required to price it".into(),
            ));
        }
        if u128::from(self.fee_rate) >= FEE_RATE_MUL_VALUE {
            return Err(WhirlpoolError::Unsupported(format!(
                "fee rate {} is out of range",
                self.fee_rate
            )));
        }
        if self.liquidity == 0 {
            return Err(WhirlpoolError::Unsupported("pool has no liquidity".into()));
        }
        if !(MIN_SQRT_PRICE_X64..=MAX_SQRT_PRICE_X64).contains(&self.sqrt_price) {
            return Err(WhirlpoolError::Unsupported(format!(
                "sqrt price {} is out of range",
                self.sqrt_price
            )));
        }
        if self.mints[0] == self.mints[1] {
            return Err(WhirlpoolError::Unsupported(
                "pool has identical mints".into(),
            ));
        }
        Ok(())
    }

    /// Index of `mint` in the pool's `[a, b]` order.
    pub fn mint_index(&self, mint: &Pubkey) -> Option<usize> {
        self.mints.iter().position(|candidate| candidate == mint)
    }

    /// The tick-array start indices the program needs for one direction.
    ///
    /// Exact port of `get_start_tick_indexes` in `SparseSwapTickSequenceBuilder`:
    /// a to b walks left from the array holding the current tick; b to a walks
    /// right, shifted by one array when the current tick is at the last usable
    /// tick of its array.
    pub fn start_tick_indexes(&self, a_to_b: bool) -> Vec<i32> {
        let spacing = self.tick_spacing as i32;
        let ticks_in_array = TICK_ARRAY_SIZE as i32 * spacing;
        let base = floor_div(self.tick_current_index, ticks_in_array) * ticks_in_array;
        let offsets: [i32; 3] = if a_to_b {
            [0, -1, -2]
        } else if self.tick_current_index + spacing >= base + ticks_in_array {
            [1, 2, 3]
        } else {
            [0, 1, 2]
        };
        offsets
            .iter()
            .filter_map(|offset| {
                let start = base + offset * ticks_in_array;
                is_valid_start_tick(start, self.tick_spacing).then_some(start)
            })
            .collect()
    }

    /// Exact-in output for `amount_in`, walking the supplied tick arrays.
    ///
    /// `arrays` must contain the arrays returned by
    /// [`WhirlpoolPool::start_tick_indexes`] for the same direction. The walk
    /// mirrors `swap_manager::swap`: each step prices against the next
    /// initialized tick, crossing it applies its `liquidity_net` and continues,
    /// and a step that exhausts the amount ends the swap.
    pub fn quote_exact_in(
        &self,
        arrays: &[TickArray],
        amount_in: u64,
        a_to_b: bool,
    ) -> Result<u64> {
        if amount_in == 0 {
            return Err(WhirlpoolError::Math("swap amount is zero".into()));
        }
        let mut amount_remaining = amount_in;
        let mut total_out = 0u64;
        let mut sqrt_price = self.sqrt_price;
        let mut tick_index = self.tick_current_index;
        let mut liquidity = self.liquidity;

        for _ in 0..128 {
            let (boundary, initialized) =
                next_boundary(arrays, tick_index, self.tick_spacing, a_to_b)
                .ok_or_else(|| {
                    WhirlpoolError::Unsupported(format!(
                        "pool {} has no tick boundary in range for this direction",
                        self.address
                    ))
                })?;
            let target = sqrt_price_from_tick_index(boundary);
            let step = compute_swap(
                amount_remaining,
                u32::from(self.fee_rate),
                liquidity,
                sqrt_price,
                target,
                true,
                a_to_b,
            )?;
            total_out = total_out
                .checked_add(step.amount_out)
                .ok_or_else(|| WhirlpoolError::Math("output overflow".into()))?;

            let spent = step
                .amount_in
                .checked_add(step.fee_amount)
                .ok_or_else(|| WhirlpoolError::Math("input overflow".into()))?;
            if spent > amount_remaining {
                return Err(WhirlpoolError::Math(format!(
                    "step spent {spent} of {amount_remaining}"
                )));
            }
            amount_remaining -= spent;
            sqrt_price = step.next_price;

            if step.next_price == target {
                if amount_remaining == 0 {
                    return Ok(total_out);
                }
                // A boundary that is not an initialized tick is the far edge of
                // the last array in the sequence. The program would need the
                // next array, which is not part of this swap's account list.
                if !initialized {
                    return Err(WhirlpoolError::Unsupported(
                        "swap would run past the supplied tick arrays".into(),
                    ));
                }
                // Crossed the tick: the liquidity change applies to everything
                // after it.
                if let Some(tick) = find_tick(arrays, boundary, self.tick_spacing) {
                    // Moving left applies `-liquidity_net`, moving right
                    // applies `+liquidity_net`.
                    let delta = if a_to_b {
                        tick.liquidity_net
                            .checked_neg()
                            .ok_or_else(|| WhirlpoolError::Math("liquidity net overflow".into()))?
                    } else {
                        tick.liquidity_net
                    };
                    liquidity = add_liquidity_delta(liquidity, delta)?;
                }
                tick_index = if a_to_b { boundary - 1 } else { boundary };
            }

            if amount_remaining == 0 {
                return Ok(total_out);
            }
            if liquidity == 0 {
                return Err(WhirlpoolError::Unsupported(
                    "swap exhausted pool liquidity".into(),
                ));
            }
        }

        Err(WhirlpoolError::Math(
            "swap crossed too many ticks to quote".into(),
        ))
    }
}

/// One decoded tick array.
#[derive(Debug, Clone)]
pub struct TickArray {
    pub address: Pubkey,
    pub start_tick_index: i32,
    pub ticks: Vec<Tick>,
    pub whirlpool: Pubkey,
}

impl TickArray {
    /// Decode a fixed-size tick array.
    pub fn decode(address: Pubkey, data: &[u8]) -> Result<Self> {
        if data.len() < TICK_ARRAY_LEN {
            return Err(WhirlpoolError::AccountTooShort {
                account: "tick array",
                need: TICK_ARRAY_LEN,
                got: data.len(),
            });
        }
        let start_tick_index = i32::from_le_bytes(
            data[TICK_ARRAY_START_OFFSET..TICK_ARRAY_START_OFFSET + 4]
                .try_into()
                .expect("4 bytes"),
        );
        let mut ticks = Vec::with_capacity(TICK_ARRAY_SIZE);
        for index in 0..TICK_ARRAY_SIZE {
            let base = TICK_ARRAY_TICKS_OFFSET + index * TICK_LEN;
            ticks.push(Tick {
                initialized: data[base] != 0,
                liquidity_net: i128::from_le_bytes(
                    data[base + 1..base + 17].try_into().expect("16 bytes"),
                ),
                liquidity_gross: u128::from_le_bytes(
                    data[base + 17..base + 33].try_into().expect("16 bytes"),
                ),
            });
        }
        Ok(Self {
            address,
            start_tick_index,
            ticks,
            whirlpool: read_pubkey(data, TICK_ARRAY_POOL_OFFSET)?,
        })
    }

    /// The tick at `tick_index` if it belongs to this array and is a multiple
    /// of the spacing.
    pub fn tick(&self, tick_index: i32, tick_spacing: u16) -> Option<&Tick> {
        if tick_spacing == 0 {
            return None;
        }
        let offset = offset_in_array(tick_index, self.start_tick_index, tick_spacing);
        if !(0..TICK_ARRAY_SIZE as isize).contains(&offset) {
            return None;
        }
        if tick_index % tick_spacing as i32 != 0 {
            return None;
        }
        self.ticks.get(offset as usize)
    }
}

/// One tick's liquidity delta.
#[derive(Debug, Clone, Copy)]
pub struct Tick {
    pub initialized: bool,
    pub liquidity_net: i128,
    pub liquidity_gross: u128,
}

/// The address of the tick array starting at `start_tick_index`.
pub fn tick_array_address(pool: &Pubkey, start_tick_index: i32) -> Pubkey {
    let program = Pubkey::from_str(WHIRLPOOL_PROGRAM_ID).expect("valid program id");
    Pubkey::find_program_address(
        &[
            b"tick_array",
            pool.as_ref(),
            start_tick_index.to_string().as_bytes(),
        ],
        &program,
    )
    .0
}

/// The oracle PDA for a pool. The swap instruction always takes it.
pub fn oracle_address(pool: &Pubkey) -> Pubkey {
    let program = Pubkey::from_str(WHIRLPOOL_PROGRAM_ID).expect("valid program id");
    Pubkey::find_program_address(&[b"oracle", pool.as_ref()], &program).0
}

/// Build the exact-in `swap` instruction for `pool`.
///
/// `arrays` are the tick-array addresses to pass, in any order; the program
/// sorts and deduplicates them. They must include the arrays for `a_to_b`.
#[allow(clippy::too_many_arguments)]
pub fn swap_instruction(
    pool: &WhirlpoolPool,
    arrays: &[Pubkey; 3],
    payer: Pubkey,
    input_mint: Pubkey,
    output_mint: Pubkey,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<Instruction> {
    if amount_in == 0 {
        return Err(WhirlpoolError::Math("swap amount is zero".into()));
    }
    let input_index = pool
        .mint_index(&input_mint)
        .ok_or_else(|| WhirlpoolError::Unsupported(format!("mint {input_mint} not in pool")))?;
    let output_index = 1 - input_index;
    if pool.mints[output_index] != output_mint {
        return Err(WhirlpoolError::Unsupported(format!(
            "mint {output_mint} not in pool"
        )));
    }
    let a_to_b = input_index == 0;

    let token_program = spl_token::id();
    let authority = payer;
    let owner_a =
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &payer,
            &pool.mints[0],
            &token_program,
        );
    let owner_b =
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &payer,
            &pool.mints[1],
            &token_program,
        );

    let accounts = vec![
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(authority, true),
        AccountMeta::new(pool.address, false),
        AccountMeta::new(owner_a, false),
        AccountMeta::new(pool.vaults[0], false),
        AccountMeta::new(owner_b, false),
        AccountMeta::new(pool.vaults[1], false),
        AccountMeta::new(arrays[0], false),
        AccountMeta::new(arrays[1], false),
        AccountMeta::new(arrays[2], false),
        AccountMeta::new_readonly(oracle_address(&pool.address), false),
    ];

    // Anchor: 8-byte discriminator, then Borsh args.
    let mut data = Vec::with_capacity(8 + 8 + 8 + 16 + 1 + 1);
    data.extend_from_slice(&SWAP_DISCRIMINATOR);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&minimum_amount_out.to_le_bytes());
    // No explicit limit: the program substitutes the directional bound.
    data.extend_from_slice(&0u128.to_le_bytes());
    data.push(1); // amount_specified_is_input = true
    data.push(u8::from(a_to_b));

    Ok(Instruction {
        program_id: Pubkey::from_str(WHIRLPOOL_PROGRAM_ID)
            .map_err(|error| WhirlpoolError::Pubkey(error.to_string()))?,
        accounts,
        data,
    })
}

/// Wrap one native swap instruction in the envelope the composer expects.
pub fn as_swap_instructions(instruction: Instruction) -> Result<SwapInstructions> {
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

// ---------------------------------------------------------------------------
// Mainnet probe.
// ---------------------------------------------------------------------------

/// The observable result of simulating one Whirlpool swap on mainnet.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WhirlpoolProbe {
    pub pool: String,
    pub input_mint: String,
    pub output_mint: String,
    pub amount_in: u64,
    pub a_to_b: bool,
    pub sqrt_price: u128,
    pub liquidity: u128,
    pub tick_current_index: i32,
    pub fee_rate: u16,
    pub tick_arrays: Vec<String>,
    pub quoted_out: u64,
    /// Quoting the output back in the opposite direction. Two fees and the
    /// price impact of both legs mean this must come back slightly *below* the
    /// original input; a value above it says the reverse pricer is wrong.
    pub reverse_quoted: Option<u64>,
    pub minimum_out: u64,
    pub actual_out: Option<u64>,
    pub quote_error: Option<i64>,
    pub err: Option<Value>,
    pub logs: Vec<String>,
    pub units_consumed: Option<u64>,
}

impl WhirlpoolProbe {
    pub fn ok(&self) -> bool {
        self.err.is_none()
    }
}

/// Load the tick arrays a swap in `a_to_b` direction needs.
///
/// A tick array that has never been initialized is submitted to the program as
/// a system-owned empty PDA and treated as all-zero, so a missing account is
/// kept as an empty array rather than dropped: the program needs the address in
/// its account list either way.
pub async fn load_tick_arrays(
    rpc: &beast_rpc::RpcClient,
    pool: &WhirlpoolPool,
    a_to_b: bool,
) -> crate::Result<Vec<TickArray>> {
    let starts = pool.start_tick_indexes(a_to_b);
    let addresses: Vec<Address> = starts
        .iter()
        .map(|start| {
            beast_flash::address(&tick_array_address(&pool.address, *start))
        })
        .collect();
    let accounts = rpc.get_multiple_accounts(&addresses).await?;
    let program = beast_flash::address(
        &Pubkey::from_str(WHIRLPOOL_PROGRAM_ID).expect("valid program id"),
    );
    let system = beast_flash::address(&solana_sdk::system_program::id());
    let mut arrays = Vec::with_capacity(starts.len());
    for (index, start) in starts.iter().enumerate() {
        let address = beast_flash::pubkey(&addresses[index]);
        let empty = || TickArray {
            address,
            start_tick_index: *start,
            ticks: vec![
                Tick {
                    initialized: false,
                    liquidity_net: 0,
                    liquidity_gross: 0,
                };
                TICK_ARRAY_SIZE
            ],
            whirlpool: pool.address,
        };
        match accounts.get(index).and_then(Option::as_ref) {
            Some(account) => {
                if account.owner == program && account.data.len() >= TICK_ARRAY_LEN {
                    let array = TickArray::decode(address, &account.data)?;
                    if array.whirlpool != pool.address {
                        return Err(crate::ExecutionError::Unsupported(format!(
                            "tick array {address} belongs to {}, not {}",
                            array.whirlpool, pool.address
                        )));
                    }
                    arrays.push(array);
                } else if account.owner == system && account.data.is_empty() {
                    arrays.push(empty());
                } else {
                    // PDAs are unique per program, not globally: another
                    // program can own an account at this exact address. The
                    // Whirlpool program rejects a foreign-owned account in the
                    // tick-array slot, so the sequence has to stop here rather
                    // than pass it. If this is the first (current) array the
                    // direction cannot be quoted at all.
                    break;
                }
            }
            None => arrays.push(empty()),
        }
    }
    if arrays.is_empty() {
        return Err(crate::ExecutionError::Unsupported(format!(
            "whirlpool {} has no usable tick array in this direction",
            pool.address
        )));
    }
    Ok(arrays)
}

/// Prove one Whirlpool swap against mainnet without sending anything.
///
/// Wraps `amount_in` lamports into WSOL, swaps it for the other side, and asks
/// the RPC for the post-execution vault balances. The local quote is compared
/// against the output vault's actual gain: a non-zero error means the pricer
/// disagrees with the program, and a probe that reverts means the account list
/// or the instruction encoding is wrong.
pub async fn probe_swap(
    rpc: &beast_rpc::RpcClient,
    wallet: &crate::Wallet,
    pool_address: Address,
    amount_in: u64,
    slippage_bps: u16,
) -> crate::Result<WhirlpoolProbe> {
    #[allow(deprecated)]
    use solana_sdk::system_instruction;
    use solana_sdk::transaction::Transaction;

    let account = rpc.get_account_info(pool_address).await?.ok_or_else(|| {
        crate::ExecutionError::Unsupported(format!(
            "whirlpool pool {pool_address} not found"
        ))
    })?;
    let pool = WhirlpoolPool::decode(beast_flash::pubkey(&pool_address), &account.account.data)?;
    pool.ensure_swappable()?;

    let wsol = spl_token::native_mint::id();
    let input_index = pool
        .mint_index(&wsol)
        .ok_or_else(|| {
            crate::ExecutionError::Unsupported(format!(
                "pool {pool_address} does not hold WSOL"
            ))
        })?;
    let output_index = 1 - input_index;
    let a_to_b = input_index == 0;

    let arrays = load_tick_arrays(rpc, &pool, a_to_b).await?;
    let quoted_out = pool.quote_exact_in(&arrays, amount_in, a_to_b)?;
    // Round-trip through the opposite direction. This needs no wallet and no
    // transaction: it is a pure consistency check on the reverse code path,
    // which is otherwise only exercised when a cycle happens to run backwards.
    let reverse_quoted = load_tick_arrays(rpc, &pool, !a_to_b)
        .await
        .ok()
        .and_then(|reverse_arrays| pool.quote_exact_in(&reverse_arrays, quoted_out, !a_to_b).ok());
    let minimum_out = quoted_out.saturating_mul(u64::from(10_000 - slippage_bps)) / 10_000;

    let token_program = spl_token::id();
    let payer = wallet.pubkey();
    let input_ata =
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &payer,
            &pool.mints[input_index],
            &token_program,
        );

    let mut array_keys: Vec<Pubkey> = arrays.iter().map(|array| array.address).collect();
    while array_keys.len() < 3 {
        array_keys.push(*array_keys.last().expect("at least one array"));
    }

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
        swap_instruction(
            &pool,
            &[array_keys[0], array_keys[1], array_keys[2]],
            payer,
            pool.mints[input_index],
            pool.mints[output_index],
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
    let simulation = rpc
        .simulate_transaction_with_accounts(
            &encoded,
            &[
                pool.vaults[0].to_string(),
                pool.vaults[1].to_string(),
            ],
            false,
        )
        .await?;

    let before = rpc
        .get_multiple_accounts(&[
            beast_flash::address(&pool.vaults[0]),
            beast_flash::address(&pool.vaults[1]),
        ])
        .await?;
    let before: Vec<u64> = before
        .iter()
        .map(|entry| {
            entry
                .as_ref()
                .and_then(|account| account_amount(&account.data))
                .unwrap_or(0)
        })
        .collect();
    let actual_out = simulation
        .post_accounts
        .get(output_index)
        .and_then(|value| value.as_ref())
        .and_then(account_amount_json)
        // A payout *decreases* the output vault: the pool sends tokens out to
        // the swapper. Reading the delta in the other direction clamps to zero
        // and looks like a pricer that returned nothing.
        .map(|after| before.get(output_index).copied().unwrap_or(0).saturating_sub(after));
    let quote_error = actual_out.map(|actual| quoted_out as i64 - actual as i64);

    Ok(WhirlpoolProbe {
        pool: pool_address.to_string(),
        input_mint: pool.mints[input_index].to_string(),
        output_mint: pool.mints[output_index].to_string(),
        amount_in,
        a_to_b,
        sqrt_price: pool.sqrt_price,
        liquidity: pool.liquidity,
        tick_current_index: pool.tick_current_index,
        fee_rate: pool.fee_rate,
        tick_arrays: arrays
            .iter()
            .map(|array| array.address.to_string())
            .collect(),
        quoted_out,
        reverse_quoted,
        minimum_out,
        actual_out,
        quote_error,
        err: simulation.err,
        logs: simulation.logs,
        units_consumed: simulation.units_consumed,
    })
}

fn account_amount_json(value: &Value) -> Option<u64> {
    use base64::Engine as _;
    let encoded = value.get("data")?.as_array()?.first()?.as_str()?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    account_amount(&data)
}

fn account_amount(data: &[u8]) -> Option<u64> {
    data.get(64..72)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
}

// ---------------------------------------------------------------------------
// Swap math, ported from `programs/whirlpool/src/math`.
// ---------------------------------------------------------------------------

/// Result of one price step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapStep {
    pub amount_in: u64,
    pub amount_out: u64,
    pub next_price: u128,
    pub fee_amount: u64,
}

/// One step of the exact-input swap, matching `swap_math::compute_swap`.
pub fn compute_swap(
    amount_remaining: u64,
    fee_rate: u32,
    liquidity: u128,
    sqrt_price_current: u128,
    sqrt_price_target: u128,
    amount_specified_is_input: bool,
    a_to_b: bool,
) -> Result<SwapStep> {
    let initial_amount_fixed_delta = try_get_amount_fixed_delta(
        sqrt_price_current,
        sqrt_price_target,
        liquidity,
        amount_specified_is_input,
        a_to_b,
    )?;

    let mut amount_calc = amount_remaining;
    if amount_specified_is_input {
        let fee_complement = FEE_RATE_MUL_VALUE
            .checked_sub(u128::from(fee_rate))
            .ok_or_else(|| WhirlpoolError::Math("fee rate exceeds one".into()))?;
        amount_calc = checked_mul_div(
            u128::from(amount_remaining),
            fee_complement,
            FEE_RATE_MUL_VALUE,
        )?
        .try_into()
        .map_err(|_| WhirlpoolError::Math("amount calc exceeds u64".into()))?;
    }

    let next_sqrt_price = if amount_fixed_delta_lte(initial_amount_fixed_delta, amount_calc) {
        sqrt_price_target
    } else {
        get_next_sqrt_price(
            sqrt_price_current,
            liquidity,
            amount_calc,
            amount_specified_is_input,
            a_to_b,
        )?
    };

    let is_max_swap = next_sqrt_price == sqrt_price_target;
    let amount_unfixed_delta = get_amount_unfixed_delta(
        sqrt_price_current,
        next_sqrt_price,
        liquidity,
        amount_specified_is_input,
        a_to_b,
    )?;

    let amount_fixed_delta = if !is_max_swap || initial_amount_fixed_delta.is_none() {
        get_amount_fixed_delta(
            sqrt_price_current,
            next_sqrt_price,
            liquidity,
            amount_specified_is_input,
            a_to_b,
        )?
    } else {
        initial_amount_fixed_delta.expect("checked above") as u64
    };

    let (amount_in, mut amount_out) = if amount_specified_is_input {
        (amount_fixed_delta, amount_unfixed_delta)
    } else {
        (amount_unfixed_delta, amount_fixed_delta)
    };

    if !amount_specified_is_input && amount_out > amount_remaining {
        amount_out = amount_remaining;
    }

    let fee_amount = if amount_specified_is_input && !is_max_swap {
        amount_remaining
            .checked_sub(amount_in)
            .ok_or_else(|| WhirlpoolError::Math("fee underflow".into()))?
    } else {
        checked_mul_div_round_up(
            u128::from(amount_in),
            u128::from(fee_rate),
            FEE_RATE_MUL_VALUE
                .checked_sub(u128::from(fee_rate))
                .ok_or_else(|| WhirlpoolError::Math("fee rate exceeds one".into()))?,
        )?
        .try_into()
        .map_err(|_| WhirlpoolError::Math("fee exceeds u64".into()))?
    };

    Ok(SwapStep {
        amount_in,
        amount_out,
        next_price: next_sqrt_price,
        fee_amount,
    })
}

/// `AmountDeltaU64`: either a value or "does not fit in u64".
type AmountDelta = Option<u64>;

fn amount_fixed_delta_lte(delta: AmountDelta, other: u64) -> bool {
    delta.is_some_and(|value| value <= other)
}

fn try_get_amount_fixed_delta(
    sqrt_price_current: u128,
    sqrt_price_target: u128,
    liquidity: u128,
    amount_specified_is_input: bool,
    a_to_b: bool,
) -> Result<AmountDelta> {
    if a_to_b == amount_specified_is_input {
        try_get_amount_delta_a(
            sqrt_price_current,
            sqrt_price_target,
            liquidity,
            amount_specified_is_input,
        )
    } else {
        try_get_amount_delta_b(
            sqrt_price_current,
            sqrt_price_target,
            liquidity,
            amount_specified_is_input,
        )
    }
}

fn get_amount_fixed_delta(
    sqrt_price_current: u128,
    sqrt_price_target: u128,
    liquidity: u128,
    amount_specified_is_input: bool,
    a_to_b: bool,
) -> Result<u64> {
    if a_to_b == amount_specified_is_input {
        get_amount_delta_a(
            sqrt_price_current,
            sqrt_price_target,
            liquidity,
            amount_specified_is_input,
        )
    } else {
        get_amount_delta_b(
            sqrt_price_current,
            sqrt_price_target,
            liquidity,
            amount_specified_is_input,
        )
    }
}

fn get_amount_unfixed_delta(
    sqrt_price_current: u128,
    sqrt_price_target: u128,
    liquidity: u128,
    amount_specified_is_input: bool,
    a_to_b: bool,
) -> Result<u64> {
    if a_to_b == amount_specified_is_input {
        get_amount_delta_b(
            sqrt_price_current,
            sqrt_price_target,
            liquidity,
            !amount_specified_is_input,
        )
    } else {
        get_amount_delta_a(
            sqrt_price_current,
            sqrt_price_target,
            liquidity,
            !amount_specified_is_input,
        )
    }
}

/// `get_amount_delta_a`, floor or ceil.
pub fn get_amount_delta_a(
    sqrt_price_0: u128,
    sqrt_price_1: u128,
    liquidity: u128,
    round_up: bool,
) -> Result<u64> {
    match try_get_amount_delta_a(sqrt_price_0, sqrt_price_1, liquidity, round_up)? {
        Some(value) => Ok(value),
        None => Err(WhirlpoolError::Math(
            "token amount exceeds u64".into(),
        )),
    }
}

fn try_get_amount_delta_a(
    sqrt_price_0: u128,
    sqrt_price_1: u128,
    liquidity: u128,
    round_up: bool,
) -> Result<AmountDelta> {
    let (lower, upper) = increasing_price_order(sqrt_price_0, sqrt_price_1);
    let diff = upper - lower;

    let numerator = mul_u256(liquidity, diff)
        .checked_shift_word_left()
        .ok_or_else(|| WhirlpoolError::Math("multiplication overflow".into()))?;
    let denominator = mul_u256(upper, lower);
    let (quotient, remainder) = numerator.div(denominator, round_up);

    let result = if round_up && !remainder.is_zero() {
        quotient.add(U256::new(0, 1)).try_into_u128()
    } else {
        quotient.try_into_u128()
    };

    match result {
        Some(result) if result > u64::MAX as u128 => Ok(None),
        Some(result) => Ok(Some(result as u64)),
        None => Ok(None),
    }
}

/// `get_amount_delta_b`, floor or ceil.
pub fn get_amount_delta_b(
    sqrt_price_0: u128,
    sqrt_price_1: u128,
    liquidity: u128,
    round_up: bool,
) -> Result<u64> {
    match try_get_amount_delta_b(sqrt_price_0, sqrt_price_1, liquidity, round_up)? {
        Some(value) => Ok(value),
        None => Err(WhirlpoolError::Math(
            "token amount exceeds u64".into(),
        )),
    }
}

fn try_get_amount_delta_b(
    sqrt_price_0: u128,
    sqrt_price_1: u128,
    liquidity: u128,
    round_up: bool,
) -> Result<AmountDelta> {
    let (lower, upper) = increasing_price_order(sqrt_price_0, sqrt_price_1);
    let n1 = upper - lower;
    if liquidity == 0 || n1 == 0 {
        return Ok(Some(0));
    }
    match liquidity.checked_mul(n1) {
        Some(product) => {
            let result = (product >> Q64_RESOLUTION) as u64;
            let should_round = round_up && (product & Q64_MASK > 0);
            if should_round && result == u64::MAX {
                return Ok(None);
            }
            Ok(Some(if should_round { result + 1 } else { result }))
        }
        None => Ok(None),
    }
}

fn add_liquidity_delta(liquidity: u128, delta: i128) -> Result<u128> {
    let magnitude = delta.unsigned_abs();
    if delta >= 0 {
        liquidity
            .checked_add(magnitude)
            .ok_or_else(|| WhirlpoolError::Math("liquidity overflow".into()))
    } else {
        liquidity
            .checked_sub(magnitude)
            .ok_or_else(|| WhirlpoolError::Math("liquidity underflow".into()))
    }
}

fn increasing_price_order(a: u128, b: u128) -> (u128, u128) {
    if a > b {
        (b, a)
    } else {
        (a, b)
    }
}

/// `get_next_sqrt_price_from_a_round_up`.
pub fn get_next_sqrt_price_from_a_round_up(
    sqrt_price: u128,
    liquidity: u128,
    amount: u64,
    amount_specified_is_input: bool,
) -> Result<u128> {
    if amount == 0 {
        return Ok(sqrt_price);
    }
    let product = mul_u256(sqrt_price, u128::from(amount));
    let numerator = mul_u256(liquidity, sqrt_price)
        .checked_shift_word_left()
        .ok_or_else(|| WhirlpoolError::Math("multiplication overflow".into()))?;
    let liquidity_shift_left = U256::new(0, liquidity).shift_word_left();
    if !amount_specified_is_input && liquidity_shift_left <= product {
        return Err(WhirlpoolError::Math("division by zero".into()));
    }
    let denominator = if amount_specified_is_input {
        liquidity_shift_left.add(product)
    } else {
        liquidity_shift_left.sub(product)
    };
    let price = div_round_up_if_u256(numerator, denominator, true)?;
    if price < MIN_SQRT_PRICE_X64 {
        return Err(WhirlpoolError::Math("sqrt price below minimum".into()));
    }
    if price > MAX_SQRT_PRICE_X64 {
        return Err(WhirlpoolError::Math("sqrt price above maximum".into()));
    }
    Ok(price)
}

/// `get_next_sqrt_price_from_b_round_down`.
pub fn get_next_sqrt_price_from_b_round_down(
    sqrt_price: u128,
    liquidity: u128,
    amount: u64,
    amount_specified_is_input: bool,
) -> Result<u128> {
    let amount_x64 = u128::from(amount) << Q64_RESOLUTION;
    let delta = div_round_up_if(amount_x64, liquidity, !amount_specified_is_input)?;
    if amount_specified_is_input {
        sqrt_price
            .checked_add(delta)
            .ok_or_else(|| WhirlpoolError::Math("sqrt price overflow".into()))
    } else {
        sqrt_price
            .checked_sub(delta)
            .ok_or_else(|| WhirlpoolError::Math("sqrt price underflow".into()))
    }
}

fn get_next_sqrt_price(
    sqrt_price: u128,
    liquidity: u128,
    amount: u64,
    amount_specified_is_input: bool,
    a_to_b: bool,
) -> Result<u128> {
    if amount_specified_is_input == a_to_b {
        get_next_sqrt_price_from_a_round_up(
            sqrt_price,
            liquidity,
            amount,
            amount_specified_is_input,
        )
    } else {
        get_next_sqrt_price_from_b_round_down(
            sqrt_price,
            liquidity,
            amount,
            amount_specified_is_input,
        )
    }
}

/// `checked_mul_div`, floor.
pub fn checked_mul_div(n0: u128, n1: u128, d: u128) -> Result<u128> {
    checked_mul_div_round_up_if(n0, n1, d, false)
}

/// `checked_mul_div_round_up`, ceil.
pub fn checked_mul_div_round_up(n0: u128, n1: u128, d: u128) -> Result<u128> {
    checked_mul_div_round_up_if(n0, n1, d, true)
}

fn checked_mul_div_round_up_if(n0: u128, n1: u128, d: u128, round_up: bool) -> Result<u128> {
    if d == 0 {
        return Err(WhirlpoolError::Math("division by zero".into()));
    }
    let product = n0
        .checked_mul(n1)
        .ok_or_else(|| WhirlpoolError::Math("mul-div overflow".into()))?;
    let quotient = product / d;
    Ok(if round_up && product % d > 0 {
        quotient + 1
    } else {
        quotient
    })
}

fn div_round_up_if(n: u128, d: u128, round_up: bool) -> Result<u128> {
    if d == 0 {
        return Err(WhirlpoolError::Math("division by zero".into()));
    }
    let quotient = n / d;
    Ok(if round_up && n % d > 0 {
        quotient + 1
    } else {
        quotient
    })
}

fn div_round_up_if_u256(n: U256, d: U256, round_up: bool) -> Result<u128> {
    let (quotient, remainder) = n.div(d, round_up);
    let result = if round_up && !remainder.is_zero() {
        quotient.add(U256::new(0, 1))
    } else {
        quotient
    };
    result
        .try_into_u128()
        .ok_or_else(|| WhirlpoolError::Math("quotient exceeds u128".into()))
}

// ---------------------------------------------------------------------------
// Tick boundary sqrt price, ported from `math/tick_math.rs`.
// ---------------------------------------------------------------------------

/// `sqrt(1.0001)^tick` as Q64.64.
pub fn sqrt_price_from_tick_index(tick: i32) -> u128 {
    if tick >= 0 {
        sqrt_price_positive(tick)
    } else {
        sqrt_price_negative(tick)
    }
}

fn mul_shift_96(n0: u128, n1: u128) -> u128 {
    mul_u256(n0, n1)
        .shift_right(96)
        .try_into_u128()
        .expect("shift fits")
}

fn sqrt_price_positive(tick: i32) -> u128 {
    let mut ratio: u128 = if tick & 1 != 0 {
        79_232_123_823_359_799_118_286_999_567
    } else {
        79_228_162_514_264_337_593_543_950_336
    };
    const FACTORS: [u128; 18] = [
        79_236_085_330_515_764_027_303_304_731,
        79_244_008_939_048_815_603_706_035_061,
        79_259_858_533_276_714_757_314_932_305,
        79_291_567_232_598_584_799_939_703_904,
        79_355_022_692_464_371_645_785_046_466,
        79_482_085_999_252_804_386_437_311_141,
        79_736_823_300_114_093_921_829_183_326,
        80_248_749_790_819_932_309_965_073_892,
        81_282_483_887_344_747_381_513_967_011,
        83_390_072_131_320_151_908_154_831_281,
        87_770_609_709_833_776_024_991_924_138,
        97_234_110_755_111_693_312_479_820_773,
        119_332_217_159_966_728_226_237_229_890,
        179_736_315_981_702_064_433_883_588_727,
        407_748_233_172_238_350_107_850_275_304,
        2_098_478_828_474_011_932_436_660_412_517,
        55_581_415_166_113_811_149_459_800_483_533,
        38_992_368_544_603_139_932_233_054_999_993_551,
    ];
    for (bit, factor) in FACTORS.iter().enumerate() {
        if tick & (2 << bit) != 0 {
            ratio = mul_shift_96(ratio, *factor);
        }
    }
    ratio >> 32
}

fn sqrt_price_negative(tick: i32) -> u128 {
    let abs_tick = tick.unsigned_abs();
    let mut ratio: u128 = if abs_tick & 1 != 0 {
        18_445_821_805_675_392_311
    } else {
        18_446_744_073_709_551_616
    };
    const FACTORS: [u128; 18] = [
        18_444_899_583_751_176_498,
        18_443_055_278_223_354_162,
        18_439_367_220_385_604_838,
        18_431_993_317_065_449_817,
        18_417_254_355_718_160_513,
        18_387_811_781_193_591_352,
        18_329_067_761_203_520_168,
        18_212_142_134_806_087_854,
        17_980_523_815_641_551_639,
        17_526_086_738_831_147_013,
        16_651_378_430_235_024_244,
        15_030_750_278_693_429_944,
        12_247_334_978_882_834_399,
        8_131_365_268_884_726_200,
        3_584_323_654_723_342_297,
        696_457_651_847_595_233,
        26_294_789_957_452_057,
        37_481_735_321_082,
    ];
    for (bit, factor) in FACTORS.iter().enumerate() {
        if abs_tick & (2 << bit) != 0 {
            ratio = (ratio * *factor) >> 64;
        }
    }
    ratio
}

// ---------------------------------------------------------------------------
// Tick array helpers.
// ---------------------------------------------------------------------------

/// The next initialized tick at or below `tick_index` when going a to b, or
/// strictly above it when going b to a. Falls back to the far edge of the last
/// loaded array, which is what the program does when a sequence has no
/// initialized tick left; because the fallback is always farther than the
/// amount can reach, it does not change the quote.
fn next_boundary(
    arrays: &[TickArray],
    tick_index: i32,
    tick_spacing: u16,
    a_to_b: bool,
) -> Option<(i32, bool)> {
    let mut best: Option<i32> = None;
    for array in arrays {
        for (offset, tick) in array.ticks.iter().enumerate() {
            if !tick.initialized {
                continue;
            }
            let candidate = array.start_tick_index + offset as i32 * i32::from(tick_spacing);
            let eligible = if a_to_b {
                candidate <= tick_index
            } else {
                candidate > tick_index
            };
            if !eligible {
                continue;
            }
            best = Some(match best {
                None => candidate,
                Some(current) if a_to_b => current.max(candidate),
                Some(current) => current.min(candidate),
            });
        }
    }
    if let Some(best) = best {
        return Some((best, true));
    }
    // No initialized tick in the supplied sequence: price against the far edge
    // of the last array, exactly as the program does. Going a to b the last
    // array is the one with the smallest start; going b to a, the largest.
    let last = if a_to_b {
        arrays.iter().min_by_key(|array| array.start_tick_index)?
    } else {
        arrays.iter().max_by_key(|array| array.start_tick_index)?
    };
    Some((
        if a_to_b {
            last.start_tick_index
        } else {
            last.start_tick_index + TICK_ARRAY_SIZE as i32 * i32::from(tick_spacing) - 1
        },
        false,
    ))
}

/// The tick at `tick_index`, if it belongs to any supplied array.
///
/// The offset within an array is measured in *ticks of the pool's spacing*, not
/// in slot positions: slot `i` holds tick `start + i * tick_spacing`. Comparing
/// a tick index against `start + i` silently misses every tick when the spacing
/// is not one, which leaves the walk's liquidity unchanged across a crossing
/// and produces a quote far larger than the program will honour.
fn find_tick(arrays: &[TickArray], tick_index: i32, tick_spacing: u16) -> Option<&Tick> {
    arrays
        .iter()
        .find_map(|array| array.tick(tick_index, tick_spacing))
}

fn offset_in_array(tick_index: i32, start_tick_index: i32, tick_spacing: u16) -> isize {
    let lhs = tick_index - start_tick_index;
    let rhs = tick_spacing as i32;
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r < 0 {
        (d - 1) as isize
    } else {
        d as isize
    }
}

fn floor_div(value: i32, divisor: i32) -> i32 {
    let d = value / divisor;
    let r = value % divisor;
    if r != 0 && (r < 0) != (divisor < 0) {
        d - 1
    } else {
        d
    }
}

fn is_valid_start_tick(tick_index: i32, tick_spacing: u16) -> bool {
    let ticks_in_array = TICK_ARRAY_SIZE as i32 * tick_spacing as i32;
    if tick_index < MIN_TICK_INDEX || tick_index > MAX_TICK_INDEX {
        if tick_index > MIN_TICK_INDEX {
            return false;
        }
        let min_array_start = MIN_TICK_INDEX - (MIN_TICK_INDEX % ticks_in_array + ticks_in_array);
        return tick_index == min_array_start;
    }
    tick_index % ticks_in_array == 0
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        data.get(offset..offset + 2)
            .ok_or(WhirlpoolError::AccountTooShort {
                account: "whirlpool",
                need: offset + 2,
                got: data.len(),
            })?
            .try_into()
            .expect("2 bytes"),
    ))
}

fn read_u128(data: &[u8], offset: usize) -> Result<u128> {
    Ok(u128::from_le_bytes(
        data.get(offset..offset + 16)
            .ok_or(WhirlpoolError::AccountTooShort {
                account: "whirlpool",
                need: offset + 16,
                got: data.len(),
            })?
            .try_into()
            .expect("16 bytes"),
    ))
}

fn read_pubkey(data: &[u8], offset: usize) -> Result<Pubkey> {
    Ok(Pubkey::new_from_array(
        data.get(offset..offset + 32)
            .ok_or(WhirlpoolError::AccountTooShort {
                account: "whirlpool",
                need: offset + 32,
                got: data.len(),
            })?
            .try_into()
            .expect("32 bytes"),
    ))
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_prices_are_ordered_and_bounded() {
        assert_eq!(sqrt_price_from_tick_index(0), 1u128 << 64);
        assert!(sqrt_price_from_tick_index(1) > 1u128 << 64);
        assert!(sqrt_price_from_tick_index(-1) < 1u128 << 64);
        // A tick is a 1.0001 step; one tick either side must straddle it.
        let up = sqrt_price_from_tick_index(1) as f64 / (1u128 << 64) as f64;
        let down = sqrt_price_from_tick_index(-1) as f64 / (1u128 << 64) as f64;
        assert!((up - 1.00005).abs() < 1e-5);
        assert!((down - 0.99995).abs() < 1e-5);
        assert_eq!(
            sqrt_price_from_tick_index(MIN_TICK_INDEX),
            MIN_SQRT_PRICE_X64
        );
        assert_eq!(
            sqrt_price_from_tick_index(MAX_TICK_INDEX),
            MAX_SQRT_PRICE_X64
        );
    }

    #[test]
    fn pda_matches_the_program_test_vector() {
        let pool: Pubkey = "HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ"
            .parse()
            .unwrap();
        assert_eq!(
            tick_array_address(&pool, -11_264).to_string(),
            "81T5kNuPRkyVzhwbe2RpKR7wmQpGJ7RBkGPdTqyfa5vq"
        );
        assert_eq!(
            tick_array_address(&pool, 0).to_string(),
            "JCpxMSDRDPBMqjoX7LkhMwro2y6r85Q8E6p5zNdBZyWa"
        );
    }

    #[test]
    fn start_tick_indexes_match_the_program_test_vectors() {
        let mut pool = pool_fixture();
        pool.tick_spacing = 64;
        pool.tick_current_index = 0;
        assert_eq!(pool.start_tick_indexes(true), vec![0, -5632, -11264]);
        assert_eq!(pool.start_tick_indexes(false), vec![0, 5632, 11264]);

        pool.tick_current_index = 5568;
        assert_eq!(pool.start_tick_indexes(false), vec![5632, 11264, 16896]);
    }

    #[test]
    fn zero_liquidity_is_rejected() {
        let mut pool = pool_fixture();
        pool.liquidity = 0;
        assert!(pool.ensure_swappable().is_err());
    }

    #[test]
    fn adaptive_fee_pool_is_rejected() {
        let mut pool = pool_fixture();
        pool.fee_tier_index = 1;
        pool.tick_spacing = 64;
        assert!(pool.ensure_swappable().is_err());
    }

    pub(super) fn pool_fixture() -> WhirlpoolPool {
        WhirlpoolPool {
            address: Pubkey::new_unique(),
            config: Pubkey::new_unique(),
            tick_spacing: 64,
            fee_tier_index: 64,
            fee_rate: 3_000,
            protocol_fee_rate: 0,
            liquidity: 1_000_000_000,
            sqrt_price: 1u128 << 64,
            tick_current_index: 0,
            mints: [Pubkey::new_unique(), Pubkey::new_unique()],
            vaults: [Pubkey::new_unique(), Pubkey::new_unique()],
        }
    }
}

#[cfg(test)]
mod probe_tests {
    use super::tests::pool_fixture;
    use super::*;

    /// A pool whose price sits one lamport above an initialized tick that holds
    /// all of its liquidity. Crossing removes that liquidity, leaving nothing
    /// to trade against, so the swap cannot complete. The pricer must say so:
    /// before this was fixed the crossing left `liquidity` unchanged (the tick
    /// lookup compared slot offsets as if the spacing were one) and returned a
    /// quote worth more than the pool's entire output vault.
    #[test]
    fn crossing_the_last_liquidity_tick_is_not_quotable() {
        let pool_sqrt_price: u128 = 3_427_297_493_386_627_302;
        let mut pool = pool_fixture();
        pool.tick_spacing = 128;
        pool.fee_tier_index = 128;
        pool.fee_rate = 10_000;
        pool.liquidity = 255_207_058;
        pool.sqrt_price = pool_sqrt_price;
        pool.tick_current_index = -33_664;

        let arrays: Vec<TickArray> = [-33_792, -45_056, -56_320]
            .iter()
            .map(|start| {
                let mut ticks = vec![
                    Tick {
                        initialized: false,
                        liquidity_net: 0,
                        liquidity_gross: 0,
                    };
                    TICK_ARRAY_SIZE
                ];
                if *start == -33_792 {
                    // The only position starts at the current tick.
                    ticks[1] = Tick {
                        initialized: true,
                        liquidity_net: 255_207_058,
                        liquidity_gross: 255_207_058,
                    };
                }
                TickArray {
                    address: Pubkey::new_unique(),
                    start_tick_index: *start,
                    ticks,
                    whirlpool: pool.address,
                }
            })
            .collect();

        assert_eq!(
            find_tick(&arrays, -33_664, 128).map(|tick| tick.liquidity_net),
            Some(255_207_058)
        );
        for amount in [100_000u64, 1_370_704] {
            assert!(
                pool.quote_exact_in(&arrays, amount, true).is_err(),
                "a swap with no liquidity left should not quote"
            );
        }
    }
}
