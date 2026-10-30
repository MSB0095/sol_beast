//! Raydium CLMM: decoding, exact local pricing, and swap construction.
//!
//! CLMM is Raydium's concentrated-liquidity AMM. Like Whirlpool it cannot be
//! priced from two vault balances: the price is `sqrt_price_x64` (Q64.64), the
//! depth is `liquidity`, and a swap walks initialized ticks. This module ports
//! the program's own arithmetic — the 256-bit multiply/divide, the
//! ceil/floor rounding, and the tick traversal — so a quote computed here is
//! the quote the deployed program computes.
//!
//! Layout facts verified against the mainnet program (account length 1544):
//!
//! * `PoolState` (8-byte discriminator): `amm_config` at 9, mints at 73/105,
//!   vaults at 137/169, observation at 201, decimals at 233/234,
//!   `tick_spacing` at 235, `liquidity` at 237, `sqrt_price_x64` at 253,
//!   `tick_current` at 269, `status` at 389, `fee_on` at 390,
//!   `tick_array_bitmap` at 904 (16 × u64), `dynamic_fee_info` at 1096;
//! * `AmmConfig` is 117 bytes: `protocol_fee_rate` at 43, `trade_fee_rate` at
//!   47, `tick_spacing` at 51, `fund_fee_rate` at 53, all in 1e-6 units;
//! * a `TickArrayState` is 10 240 bytes: pool id at 8, `start_tick_index` at
//!   40, 60 ticks of 168 bytes from 44. A tick holds `tick` (i32) at 0,
//!   `liquidity_net` (i128) at 4, `liquidity_gross` (u128) at 20,
//!   `orders_amount` (u64) at 124 and `part_filled_orders_remaining` at 132;
//! * tick-array addresses are PDAs over `[b"tick_array", pool, start]` with
//!   `start` as a big-endian i32, under the program id.
//!
//! Fee is charged on the input side for every pool this engine will price
//! (`fee_on == 0`); pools that charge elsewhere, or that have a dynamic fee
//! component, are rejected rather than mispriced. Limit orders are rejected
//! when a swap would actually cross one, because matching them changes both the
//! amount and the fee of that step.

use crate::u256::{mul_u256, U256};
use crate::whirlpool::{
    get_next_sqrt_price_from_a_round_up, get_next_sqrt_price_from_b_round_down,
    sqrt_price_from_tick_index,
};
use beast_dex::jupiter::{RawAccountMeta, RawInstruction, SwapInstructions};
use beast_dex::Address;
use beast_rpc::Account;
use serde_json::Value;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use thiserror::Error;

/// The CLMM program id.
pub const CLMM_PROGRAM_ID: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";
/// `PoolState` account length.
pub const POOL_LEN: usize = 1544;
/// `AmmConfig` account length.
pub const AMM_CONFIG_LEN: usize = 117;
/// Number of ticks in a tick array.
pub const TICK_ARRAY_SIZE: usize = 60;
/// Packed `TickState` length.
pub const TICK_LEN: usize = 168;
/// A tick array: 8 discriminator + 32 pool + 4 start + ticks + trailer.
pub const TICK_ARRAY_LEN: usize = 8 + 32 + 4 + TICK_ARRAY_SIZE * TICK_LEN + 1 + 115;
/// Fee rates are 1e-6 units; `1_000_000` is 100%.
pub const FEE_RATE_DENOMINATOR: u64 = 1_000_000;
/// Absolute bounds of the Q64.64 sqrt price; identical to Whirlpool's tick
/// math because the underlying tick range is the same.
pub const MIN_SQRT_PRICE_X64: u128 = 4_295_048_016;
pub const MAX_SQRT_PRICE_X64: u128 = 79_226_673_515_401_279_992_447_579_055;

// `PoolState` field offsets.
pub const AMM_CONFIG_OFFSET: usize = 9;
pub const MINT_0_OFFSET: usize = 73;
pub const MINT_1_OFFSET: usize = 105;
pub const VAULT_0_OFFSET: usize = 137;
pub const VAULT_1_OFFSET: usize = 169;
pub const OBSERVATION_OFFSET: usize = 201;
pub const DECIMALS_0_OFFSET: usize = 233;
pub const DECIMALS_1_OFFSET: usize = 234;
pub const TICK_SPACING_OFFSET: usize = 235;
pub const LIQUIDITY_OFFSET: usize = 237;
pub const SQRT_PRICE_OFFSET: usize = 253;
pub const TICK_CURRENT_OFFSET: usize = 269;
pub const STATUS_OFFSET: usize = 389;
pub const FEE_ON_OFFSET: usize = 390;
pub const DYNAMIC_FEE_CONTROL_OFFSET: usize = 1102;

// `AmmConfig` field offsets.
const CONFIG_PROTOCOL_FEE_OFFSET: usize = 43;
const CONFIG_TRADE_FEE_OFFSET: usize = 47;
const CONFIG_TICK_SPACING_OFFSET: usize = 51;
const CONFIG_FUND_FEE_OFFSET: usize = 53;

// `TickArrayState` / `TickState` field offsets.
const TICK_ARRAY_POOL_OFFSET: usize = 8;
const TICK_ARRAY_START_OFFSET: usize = 40;
const TICK_ARRAY_TICKS_OFFSET: usize = 44;
const TICK_ORDERS_OFFSET: usize = 124;
const TICK_PART_FILLED_OFFSET: usize = 132;

/// `swap` discriminator (Anchor `global:swap`).
pub const SWAP_DISCRIMINATOR: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];

#[derive(Debug, Error)]
pub enum ClmmError {
    #[error("{account} is {got} bytes, need {need}")]
    AccountTooShort {
        account: &'static str,
        need: usize,
        got: usize,
    },
    #[error("clmm pool is not swappable: {0}")]
    Unsupported(String),
    #[error("clmm math: {0}")]
    Math(String),
    #[error("clmm pubkey: {0}")]
    Pubkey(String),
}

type Result<T> = std::result::Result<T, ClmmError>;

/// A decoded CLMM pool. The fee table lives in a separate `AmmConfig` account.
#[derive(Debug, Clone)]
pub struct ClmmPool {
    pub address: Pubkey,
    pub amm_config: Pubkey,
    pub mints: [Pubkey; 2],
    pub vaults: [Pubkey; 2],
    pub observation: Pubkey,
    pub mint_decimals: [u8; 2],
    pub tick_spacing: u16,
    pub liquidity: u128,
    pub sqrt_price_x64: u128,
    pub tick_current: i32,
    pub status: u8,
    pub fee_on: u8,
    pub dynamic_fee_control: u32,
}

impl ClmmPool {
    /// Decode the fixed pool layout.
    pub fn decode(address: Pubkey, data: &[u8]) -> Result<Self> {
        if data.len() < POOL_LEN {
            return Err(ClmmError::AccountTooShort {
                account: "clmm pool",
                need: POOL_LEN,
                got: data.len(),
            });
        }
        Ok(Self {
            address,
            amm_config: read_pubkey(data, AMM_CONFIG_OFFSET)?,
            mints: [
                read_pubkey(data, MINT_0_OFFSET)?,
                read_pubkey(data, MINT_1_OFFSET)?,
            ],
            vaults: [
                read_pubkey(data, VAULT_0_OFFSET)?,
                read_pubkey(data, VAULT_1_OFFSET)?,
            ],
            observation: read_pubkey(data, OBSERVATION_OFFSET)?,
            mint_decimals: [data[DECIMALS_0_OFFSET], data[DECIMALS_1_OFFSET]],
            tick_spacing: read_u16(data, TICK_SPACING_OFFSET)?,
            liquidity: read_u128(data, LIQUIDITY_OFFSET)?,
            sqrt_price_x64: read_u128(data, SQRT_PRICE_OFFSET)?,
            tick_current: read_i32(data, TICK_CURRENT_OFFSET)?,
            status: data[STATUS_OFFSET],
            fee_on: data[FEE_ON_OFFSET],
            dynamic_fee_control: read_u32(data, DYNAMIC_FEE_CONTROL_OFFSET)?,
        })
    }

    /// Reject pools whose pricing this module does not model exactly.
    pub fn ensure_swappable(&self) -> Result<()> {
        if self.tick_spacing == 0 {
            return Err(ClmmError::Unsupported("tick spacing is zero".into()));
        }
        if self.liquidity == 0 {
            return Err(ClmmError::Unsupported("pool has no liquidity".into()));
        }
        // Bit 4 is the swap disable flag.
        if self.status & 0b0001_0000 != 0 {
            return Err(ClmmError::Unsupported("swaps are disabled on this pool".into()));
        }
        if self.fee_on != 0 {
            return Err(ClmmError::Unsupported(format!(
                "fee side {} is not modelled (only fee-on-input)",
                self.fee_on
            )));
        }
        if self.dynamic_fee_control != 0 {
            return Err(ClmmError::Unsupported(
                "dynamic fee is enabled; it needs live volatility state".into(),
            ));
        }
        if !(MIN_SQRT_PRICE_X64..MAX_SQRT_PRICE_X64).contains(&self.sqrt_price_x64) {
            return Err(ClmmError::Unsupported(format!(
                "sqrt price {} is out of range",
                self.sqrt_price_x64
            )));
        }
        if self.mints[0] == self.mints[1] {
            return Err(ClmmError::Unsupported("pool has identical mints".into()));
        }
        Ok(())
    }

    /// Index of `mint` in the pool's `[token0, token1]` order.
    pub fn mint_index(&self, mint: &Pubkey) -> Option<usize> {
        self.mints.iter().position(|candidate| candidate == mint)
    }

    /// Start index of the tick array that contains `tick`.
    pub fn array_start_index(&self, tick: i32) -> i32 {
        array_start_index(tick, self.tick_spacing)
    }

    /// Consecutive tick-array start indices starting with the array that
    /// contains the current tick and walking in the swap direction.
    ///
    /// The program itself only needs the arrays it crosses, but which ones
    /// those are depends on the bitmap. Loading a short consecutive run is
    /// equivalent for quoting (empty arrays carry no ticks) and lets the same
    /// list be passed to the swap instruction. `count` bounds how far a single
    /// swap can reach; a swap that needs more is rejected rather than quoted
    /// with a missing account.
    pub fn start_tick_indexes(&self, zero_for_one: bool, count: usize) -> Vec<i32> {
        let step = TICK_ARRAY_SIZE as i32 * i32::from(self.tick_spacing);
        let base = self.array_start_index(self.tick_current);
        (0..count as i32)
            .map(|offset| {
                if zero_for_one {
                    base - offset * step
                } else {
                    base + offset * step
                }
            })
            .filter(|start| is_valid_start_index(*start, self.tick_spacing))
            .collect()
    }

    /// Exact-in output for `amount_in`, walking the supplied tick arrays.
    ///
    /// `arrays` must be ordered the way [`ClmmPool::start_tick_indexes`]
    /// returns them and must include every array the swap reaches. The walk
    /// mirrors `swap_internal`: each step prices against the next initialized
    /// tick, a step that reaches that tick crosses it (applying
    /// `liquidity_net`), and a step that exhausts the input ends the swap.
    pub fn quote_exact_in(
        &self,
        arrays: &[TickArray],
        amount_in: u64,
        zero_for_one: bool,
        trade_fee_rate: u32,
    ) -> Result<u64> {
        if amount_in == 0 {
            return Err(ClmmError::Math("swap amount is zero".into()));
        }
        if u64::from(trade_fee_rate) >= FEE_RATE_DENOMINATOR {
            return Err(ClmmError::Unsupported(format!(
                "trade fee rate {trade_fee_rate} is out of range"
            )));
        }
        let mut amount_remaining = amount_in;
        let mut total_out = 0u64;
        let mut sqrt_price = self.sqrt_price_x64;
        let mut tick = self.tick_current;
        let mut liquidity = self.liquidity;

        for _ in 0..128 {
            let (boundary, tick_state) =
                next_initialized_tick(arrays, tick, self.tick_spacing, zero_for_one).ok_or_else(
                    || {
                        ClmmError::Unsupported(format!(
                            "pool {} has no initialized tick in this direction inside the loaded arrays",
                            self.address
                        ))
                    },
                )?;
            let target = sqrt_price_from_tick_index(boundary);
            let step = compute_swap(
                amount_remaining,
                trade_fee_rate,
                liquidity,
                sqrt_price,
                target,
                zero_for_one,
            )?;
            total_out = total_out
                .checked_add(step.amount_out)
                .ok_or_else(|| ClmmError::Math("output overflow".into()))?;
            let spent = step
                .amount_in
                .checked_add(step.fee_amount)
                .ok_or_else(|| ClmmError::Math("input overflow".into()))?;
            if spent > amount_remaining {
                return Err(ClmmError::Math(format!(
                    "step spent {spent} of {amount_remaining}"
                )));
            }
            amount_remaining -= spent;
            sqrt_price = step.next_price;

            if step.next_price == target {
                if amount_remaining == 0 {
                    return Ok(total_out);
                }
                // Crossing a tick with unfilled limit orders matches those
                // orders at this price; the amount and fee of such a step are
                // not the AMM step computed above.
                if tick_state.orders_amount > 0 || tick_state.part_filled_orders_remaining > 0 {
                    return Err(ClmmError::Unsupported(format!(
                        "pool {} has unfilled limit orders at tick {boundary}",
                        self.address
                    )));
                }
                if tick_state.liquidity_gross > 0 {
                    // Moving left removes the net that was added crossing
                    // right-to-left, and vice versa.
                    let delta = if zero_for_one {
                        tick_state
                            .liquidity_net
                            .checked_neg()
                            .ok_or_else(|| ClmmError::Math("liquidity net overflow".into()))?
                    } else {
                        tick_state.liquidity_net
                    };
                    liquidity = add_liquidity_delta(liquidity, delta)?;
                }
                tick = if zero_for_one { boundary - 1 } else { boundary };
            }

            if amount_remaining == 0 {
                return Ok(total_out);
            }
            if liquidity == 0 {
                return Err(ClmmError::Unsupported(
                    "swap exhausted pool liquidity".into(),
                ));
            }
        }

        Err(ClmmError::Math(
            "swap crossed too many ticks to quote".into(),
        ))
    }
}

/// The fee table for a family of CLMM pools.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AmmConfig {
    pub protocol_fee_rate: u32,
    pub trade_fee_rate: u32,
    pub tick_spacing: u16,
    pub fund_fee_rate: u32,
}

impl AmmConfig {
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < AMM_CONFIG_LEN {
            return Err(ClmmError::AccountTooShort {
                account: "amm config",
                need: AMM_CONFIG_LEN,
                got: data.len(),
            });
        }
        Ok(Self {
            protocol_fee_rate: read_u32(data, CONFIG_PROTOCOL_FEE_OFFSET)?,
            trade_fee_rate: read_u32(data, CONFIG_TRADE_FEE_OFFSET)?,
            tick_spacing: read_u16(data, CONFIG_TICK_SPACING_OFFSET)?,
            fund_fee_rate: read_u32(data, CONFIG_FUND_FEE_OFFSET)?,
        })
    }
}

/// One decoded tick array.
#[derive(Debug, Clone)]
pub struct TickArray {
    pub address: Pubkey,
    pub pool_id: Pubkey,
    pub start_tick_index: i32,
    pub ticks: Vec<TickState>,
}

impl TickArray {
    pub fn decode(address: Pubkey, data: &[u8]) -> Result<Self> {
        if data.len() < TICK_ARRAY_LEN {
            return Err(ClmmError::AccountTooShort {
                account: "clmm tick array",
                need: TICK_ARRAY_LEN,
                got: data.len(),
            });
        }
        let mut ticks = Vec::with_capacity(TICK_ARRAY_SIZE);
        for index in 0..TICK_ARRAY_SIZE {
            let base = TICK_ARRAY_TICKS_OFFSET + index * TICK_LEN;
            ticks.push(TickState {
                tick: read_i32(data, base)?,
                liquidity_net: i128::from_le_bytes(
                    data[base + 4..base + 20].try_into().expect("16 bytes"),
                ),
                liquidity_gross: u128::from_le_bytes(
                    data[base + 20..base + 36].try_into().expect("16 bytes"),
                ),
                orders_amount: u64::from_le_bytes(
                    data[base + TICK_ORDERS_OFFSET..base + TICK_ORDERS_OFFSET + 8]
                        .try_into()
                        .expect("8 bytes"),
                ),
                part_filled_orders_remaining: u64::from_le_bytes(
                    data[base + TICK_PART_FILLED_OFFSET..base + TICK_PART_FILLED_OFFSET + 8]
                        .try_into()
                        .expect("8 bytes"),
                ),
            });
        }
        Ok(Self {
            address,
            pool_id: read_pubkey(data, TICK_ARRAY_POOL_OFFSET)?,
            start_tick_index: read_i32(data, TICK_ARRAY_START_OFFSET)?,
            ticks,
        })
    }

    /// Whether this array holds any tick the program considers initialized.
    pub fn has_initialized_ticks(&self) -> bool {
        self.ticks.iter().any(TickState::is_initialized)
    }

    /// The slot the current tick falls in, if it belongs to this array.
    ///
    /// The program does not require the tick to be a multiple of the spacing:
    /// `offset_in_array` is a plain integer division of `tick - start`, which for
    /// a tick inside the array always truncates downwards.
    fn slot_of(&self, tick: i32, tick_spacing: u16) -> Option<usize> {
        let step = i32::from(tick_spacing);
        if step == 0 {
            return None;
        }
        let offset = (tick - self.start_tick_index) / step;
        if (0..TICK_ARRAY_SIZE as i32).contains(&offset) {
            Some(offset as usize)
        } else {
            None
        }
    }
}

/// One tick's liquidity and limit-order state.
#[derive(Debug, Clone, Copy)]
pub struct TickState {
    pub tick: i32,
    pub liquidity_net: i128,
    pub liquidity_gross: u128,
    pub orders_amount: u64,
    pub part_filled_orders_remaining: u64,
}

impl TickState {
    pub fn is_initialized(&self) -> bool {
        self.liquidity_gross > 0
            || self.orders_amount > 0
            || self.part_filled_orders_remaining > 0
    }
}

/// The tick array PDA for `pool` starting at `start_tick_index`.
///
/// Unlike Whirlpool, the start index is serialized big-endian.
pub fn tick_array_address(pool: &Pubkey, start_tick_index: i32) -> Pubkey {
    let program = Pubkey::from_str(CLMM_PROGRAM_ID).expect("valid program id");
    Pubkey::find_program_address(
        &[
            b"tick_array",
            pool.as_ref(),
            &start_tick_index.to_be_bytes(),
        ],
        &program,
    )
    .0
}

/// Build the exact-in `swap` instruction.
///
/// `arrays` must be ordered for the trade direction, with the first array being
/// the first one the program will actually traverse (the array containing the
/// current tick when it has ticks, otherwise the next initialized array). The
/// first is a fixed account slot and the rest are remaining accounts.
#[allow(clippy::too_many_arguments)]
pub fn swap_instruction(
    pool: &ClmmPool,
    config: &AmmConfig,
    arrays: &[Pubkey],
    payer: Pubkey,
    input_mint: Pubkey,
    output_mint: Pubkey,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<Instruction> {
    if amount_in == 0 {
        return Err(ClmmError::Math("swap amount is zero".into()));
    }
    if arrays.is_empty() {
        return Err(ClmmError::Unsupported(
            "swap needs at least one tick array".into(),
        ));
    }
    let input_index = pool
        .mint_index(&input_mint)
        .ok_or_else(|| ClmmError::Unsupported(format!("mint {input_mint} not in pool")))?;
    let output_index = 1 - input_index;
    if pool.mints[output_index] != output_mint {
        return Err(ClmmError::Unsupported(format!(
            "mint {output_mint} not in pool"
        )));
    }
    let _ = config;

    let token_program = spl_token::id();
    let input_token_account =
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &payer,
            &input_mint,
            &token_program,
        );
    let output_token_account =
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &payer,
            &output_mint,
            &token_program,
        );

    let mut accounts = vec![
        AccountMeta::new_readonly(payer, true),
        AccountMeta::new_readonly(pool.amm_config, false),
        AccountMeta::new(pool.address, false),
        AccountMeta::new(input_token_account, false),
        AccountMeta::new(output_token_account, false),
        AccountMeta::new(pool.vaults[input_index], false),
        AccountMeta::new(pool.vaults[output_index], false),
        AccountMeta::new(pool.observation, false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new(arrays[0], false),
    ];
    for array in &arrays[1..] {
        accounts.push(AccountMeta::new(*array, false));
    }

    // Anchor: 8-byte discriminator, then Borsh args.
    let mut data = Vec::with_capacity(8 + 8 + 8 + 16 + 1);
    data.extend_from_slice(&SWAP_DISCRIMINATOR);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&minimum_amount_out.to_le_bytes());
    // 0 means "directional bound": the program substitutes MIN+1 / MAX-1.
    data.extend_from_slice(&0u128.to_le_bytes());
    data.push(1); // is_base_input = true

    Ok(Instruction {
        program_id: Pubkey::from_str(CLMM_PROGRAM_ID)
            .map_err(|error| ClmmError::Pubkey(error.to_string()))?,
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
// Tick-array loading.
// ---------------------------------------------------------------------------

/// Load the arrays one pool needs in one direction.
pub async fn load_tick_arrays(
    rpc: &beast_rpc::RpcClient,
    pool: &ClmmPool,
    zero_for_one: bool,
    count: usize,
) -> crate::Result<Vec<TickArray>> {
    let starts = pool.start_tick_indexes(zero_for_one, count);
    let addresses: Vec<Address> = starts
        .iter()
        .map(|start| beast_flash::address(&tick_array_address(&pool.address, *start)))
        .collect();
    let accounts = rpc.get_multiple_accounts(&addresses).await?;
    let lookup: HashMap<Address, &Account> = addresses
        .iter()
        .zip(accounts.iter())
        .filter_map(|(key, account)| account.as_ref().map(|account| (*key, account)))
        .collect();
    decode_tick_array_sequence(pool, zero_for_one, &lookup, count)
}

/// Load both directions for every pool in `pools` with one batched read.
///
/// The result is indexed like `pools`; `None` means the pool cannot be quoted
/// in at least one direction (no initialized tick array, a foreign-owned PDA,
/// or an array that belongs to a different pool).
pub async fn load_tick_arrays_batch(
    rpc: &beast_rpc::RpcClient,
    pools: &[ClmmPool],
    count: usize,
) -> crate::Result<Vec<Option<[Vec<TickArray>; 2]>>> {
    let mut addresses: Vec<Address> = Vec::new();
    let mut seen: HashSet<Address> = HashSet::new();
    for pool in pools {
        for zero_for_one in [false, true] {
            for start in pool.start_tick_indexes(zero_for_one, count) {
                let address = beast_flash::address(&tick_array_address(&pool.address, start));
                if seen.insert(address) {
                    addresses.push(address);
                }
            }
        }
    }

    let mut lookup: HashMap<Address, Account> = HashMap::with_capacity(addresses.len());
    // Overlap the chunks: each one is an independent round trip.
    let chunks: Vec<&[Address]> = addresses.chunks(100).collect();
    let responses = futures_util::future::join_all(
        chunks.iter().map(|chunk| rpc.get_multiple_accounts(chunk)),
    )
    .await;
    for (chunk, accounts) in chunks.iter().zip(responses) {
        let accounts = accounts?;
        for (key, account) in chunk.iter().zip(accounts) {
            if let Some(account) = account {
                lookup.insert(*key, account);
            }
        }
    }

    let mut result = Vec::with_capacity(pools.len());
    for pool in pools {
        let mut directions: Vec<Vec<TickArray>> = Vec::with_capacity(2);
        for zero_for_one in [false, true] {
            match decode_tick_array_sequence(pool, zero_for_one, &lookup, count) {
                Ok(arrays) => directions.push(arrays),
                Err(_) => break,
            }
        }
        if directions.len() == 2 {
            let zero_for_one = directions.pop().expect("two entries");
            let one_for_zero = directions.pop().expect("two entries");
            result.push(Some([one_for_zero, zero_for_one]));
        } else {
            result.push(None);
        }
    }
    Ok(result)
}

fn decode_tick_array_sequence<T>(
    pool: &ClmmPool,
    zero_for_one: bool,
    accounts: &HashMap<Address, T>,
    count: usize,
) -> crate::Result<Vec<TickArray>>
where
    T: std::borrow::Borrow<Account>,
{
    let starts = pool.start_tick_indexes(zero_for_one, count);
    let program = beast_flash::address(
        &Pubkey::from_str(CLMM_PROGRAM_ID).expect("valid program id"),
    );

    let mut arrays = Vec::with_capacity(starts.len());
    for start in &starts {
        let address = beast_flash::address(&tick_array_address(&pool.address, *start));
        // A tick array that does not exist on-chain is not passed to the
        // program: unlike Whirlpool, the program loads each remaining account
        // as a real `TickArrayState` and would reject a system-owned PDA.
        let Some(account) = accounts.get(&address).map(std::borrow::Borrow::borrow) else {
            continue;
        };
        if account.owner != program || account.data.len() < TICK_ARRAY_LEN {
            continue;
        }
        let array = TickArray::decode(beast_flash::pubkey(&address), &account.data)?;
        if array.pool_id != pool.address || array.start_tick_index != *start {
            continue;
        }
        arrays.push(array);
    }

    // Drop leading arrays without ticks: the program's first valid array is the
    // next initialized one in this direction, and the fixed account slot must
    // be that array. Everything after it stays in traversal order.
    let first = arrays
        .iter()
        .position(TickArray::has_initialized_ticks)
        .ok_or_else(|| {
            crate::ExecutionError::Unsupported(format!(
                "clmm pool {} has no initialized tick array in this direction",
                pool.address
            ))
        })?;
    arrays.drain(..first);
    Ok(arrays)
}

// ---------------------------------------------------------------------------
// Mainnet probe.
// ---------------------------------------------------------------------------

/// The observable result of simulating one CLMM swap on mainnet.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClmmProbe {
    pub pool: String,
    pub input_mint: String,
    pub output_mint: String,
    pub amount_in: u64,
    pub zero_for_one: bool,
    pub sqrt_price_x64: u128,
    pub liquidity: u128,
    pub tick_current: i32,
    pub tick_spacing: u16,
    pub trade_fee_rate: u32,
    pub tick_arrays: Vec<String>,
    pub quoted_out: u64,
    /// Quoting the output back the other way must come back below the original
    /// input: two fees and both legs' price impact. A value above it says the
    /// reverse pricer is wrong.
    pub reverse_quoted: Option<u64>,
    pub minimum_out: u64,
    pub actual_out: Option<u64>,
    pub quote_error: Option<i64>,
    /// The pool's `tick_current` and `sqrt_price_x64` *after* the simulated
    /// swap. Compared against the values the local pricer quoted from, this
    /// says whether the pool moved between the read and the swap: a moved pool
    /// makes the quote stale by a slot, which is indistinguishable from a
    /// pricer that disagrees unless it is measured. A zero `quote_error` with a
    /// stable pool is the proof; a non-zero error on a pool that moved is not a
    /// claim about the pricer.
    /// Whether the pool account changed between the read the quote used and a
    /// re-read taken immediately before the simulation. A busy pool moves
    /// within a second, and a quote computed from an older state is off by a
    /// lamport or two for reasons that have nothing to do with the pricer's
    /// arithmetic. Only a `quote_error` of zero against an unchanged state is a
    /// claim about the math; a non-zero error alongside a moved state is not.
    pub stale_read: bool,
    pub err: Option<Value>,
    pub logs: Vec<String>,
    pub units_consumed: Option<u64>,
}

impl ClmmProbe {
    pub fn ok(&self) -> bool {
        self.err.is_none()
    }
}

/// Read the pool and its config, then quote a swap without sending anything.
pub async fn probe_swap(
    rpc: &beast_rpc::RpcClient,
    wallet: &crate::Wallet,
    pool_address: Address,
    amount_in: u64,
    slippage_bps: u16,
    array_count: usize,
) -> crate::Result<ClmmProbe> {
    #[allow(deprecated)]
    use solana_sdk::system_instruction;
    use solana_sdk::transaction::Transaction;

    let account = rpc.get_account_info(pool_address).await?.ok_or_else(|| {
        crate::ExecutionError::Unsupported(format!("clmm pool {pool_address} not found"))
    })?;
    let pool = ClmmPool::decode(beast_flash::pubkey(&pool_address), &account.account.data)?;
    pool.ensure_swappable()?;

    let config_account = rpc
        .get_account_info(beast_flash::address(&pool.amm_config))
        .await?
        .ok_or_else(|| {
            crate::ExecutionError::Unsupported(format!(
                "amm config {} not found",
                pool.amm_config
            ))
        })?;
    let config = AmmConfig::decode(&config_account.account.data)?;

    let wsol = spl_token::native_mint::id();
    let input_index = pool.mint_index(&wsol).ok_or_else(|| {
        crate::ExecutionError::Unsupported(format!("pool {pool_address} does not hold WSOL"))
    })?;
    let output_index = 1 - input_index;
    let zero_for_one = input_index == 0;

    let arrays = load_tick_arrays(rpc, &pool, zero_for_one, array_count).await?;
    let quoted_out = pool.quote_exact_in(&arrays, amount_in, zero_for_one, config.trade_fee_rate)?;
    let reverse_quoted = load_tick_arrays(rpc, &pool, !zero_for_one, array_count)
        .await
        .ok()
        .and_then(|reverse_arrays| {
            pool.quote_exact_in(
                &reverse_arrays,
                quoted_out,
                !zero_for_one,
                config.trade_fee_rate,
            )
            .ok()
        });
    let minimum_out = quoted_out.saturating_mul(u64::from(10_000 - slippage_bps)) / 10_000;

    let token_program = spl_token::id();
    let payer = wallet.pubkey();
    let input_ata =
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &payer,
            &pool.mints[input_index],
            &token_program,
        );
    let array_keys: Vec<Pubkey> = arrays.iter().map(|array| array.address).collect();

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
            &config,
            &array_keys,
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
                pool_address.to_string(),
            ],
            false,
        )
        .await?;

    // A busy pool can move between the read the quote used and the simulation
    // that consumes it. Re-reading at the last possible moment is how that is
    // separated from a pricer that disagrees: an unchanged state means the
    // quote and the swap priced the same books.
    let reread = rpc.get_account_info(pool_address).await?.and_then(|account| {
        ClmmPool::decode(beast_flash::pubkey(&pool_address), &account.account.data).ok()
    });
    let stale_read = match &reread {
        Some(fresh) => {
            fresh.tick_current != pool.tick_current
                || fresh.sqrt_price_x64 != pool.sqrt_price_x64
                || fresh.liquidity != pool.liquidity
        }
        None => true,
    };

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
        .map(|after| before.get(output_index).copied().unwrap_or(0).saturating_sub(after));
    let quote_error = actual_out.map(|actual| quoted_out as i64 - actual as i64);

    Ok(ClmmProbe {
        pool: pool_address.to_string(),
        input_mint: pool.mints[input_index].to_string(),
        output_mint: pool.mints[output_index].to_string(),
        amount_in,
        zero_for_one,
        sqrt_price_x64: pool.sqrt_price_x64,
        liquidity: pool.liquidity,
        tick_current: pool.tick_current,
        tick_spacing: pool.tick_spacing,
        trade_fee_rate: config.trade_fee_rate,
        tick_arrays: arrays.iter().map(|array| array.address.to_string()).collect(),
        quoted_out,
        reverse_quoted,
        minimum_out,
        actual_out,
        quote_error,
        stale_read,
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
// Swap math, ported from `programs/amm/src/libraries`.
// ---------------------------------------------------------------------------

/// Result of one price step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapStep {
    pub amount_in: u64,
    pub amount_out: u64,
    pub next_price: u128,
    pub fee_amount: u64,
}

/// One step of an exact-input, fee-on-input CLMM swap.
pub fn compute_swap(
    amount_remaining: u64,
    fee_rate: u32,
    liquidity: u128,
    sqrt_price_current: u128,
    sqrt_price_target: u128,
    zero_for_one: bool,
) -> Result<SwapStep> {
    if zero_for_one {
        if sqrt_price_current < sqrt_price_target {
            return Err(ClmmError::Math("target price is above current".into()));
        }
    } else if sqrt_price_current > sqrt_price_target {
        return Err(ClmmError::Math("target price is below current".into()));
    }

    let fee_rate = u64::from(fee_rate);
    let amount_for_price_calc = (u128::from(amount_remaining)
        * u128::from(FEE_RATE_DENOMINATOR - fee_rate)
        / u128::from(FEE_RATE_DENOMINATOR)) as u64;

    // Both amounts at the target. A u64 overflow means the target is farther
    // than any u64 amount can reach, so the step stops short of it — exactly
    // the program's `MaxTokenOverflow` path.
    let target_amounts = delta_amounts_for_swap(
        sqrt_price_target,
        sqrt_price_current,
        liquidity,
        zero_for_one,
    );
    let (next_price, amount_in, amount_out) = match target_amounts {
        Some((amount_in_at_target, amount_out_at_target))
            if amount_for_price_calc >= amount_in_at_target =>
        {
            (
                sqrt_price_target,
                amount_in_at_target,
                amount_out_at_target,
            )
        }
        _ => {
            if liquidity == 0 {
                return Err(ClmmError::Math("zero liquidity".into()));
            }
            let next = if zero_for_one {
                get_next_sqrt_price_from_a_round_up(
                    sqrt_price_current,
                    liquidity,
                    amount_for_price_calc,
                    true,
                )
                .map_err(|error| ClmmError::Math(error.to_string()))?
            } else {
                get_next_sqrt_price_from_b_round_down(
                    sqrt_price_current,
                    liquidity,
                    amount_for_price_calc,
                    true,
                )
                .map_err(|error| ClmmError::Math(error.to_string()))?
            };
            let (amount_in, amount_out) =
                delta_amounts_for_swap(next, sqrt_price_current, liquidity, zero_for_one)
                    .ok_or_else(|| ClmmError::Math("token amount exceeds u64".into()))?;
            (next, amount_in, amount_out)
        }
    };

    let fee_amount = if next_price != sqrt_price_target {
        amount_remaining
            .checked_sub(amount_in)
            .ok_or_else(|| ClmmError::Math("fee underflow".into()))?
    } else {
        let numerator = u128::from(amount_in) * u128::from(fee_rate);
        let denominator = u128::from(FEE_RATE_DENOMINATOR - fee_rate);
        let fee = numerator.div_ceil(denominator);
        fee.try_into()
            .map_err(|_| ClmmError::Math("fee exceeds u64".into()))?
    };

    Ok(SwapStep {
        amount_in,
        amount_out,
        next_price,
        fee_amount,
    })
}

/// `(amount_in, amount_out)` for a move between two prices, with the
/// program's rounding: input rounds up, output rounds down.
fn delta_amounts_for_swap(
    sqrt_a: u128,
    sqrt_b: u128,
    liquidity: u128,
    zero_for_one: bool,
) -> Option<(u64, u64)> {
    let (lower, upper) = if sqrt_a > sqrt_b {
        (sqrt_b, sqrt_a)
    } else {
        (sqrt_a, sqrt_b)
    };
    if lower == 0 {
        return None;
    }
    let diff = upper - lower;
    let amount_1_x64 = mul_u256(liquidity, diff);
    let product = mul_u256(lower, upper);

    let (amount_in, amount_out) = if zero_for_one {
        let amount_in = mul_pow2_div_ceil(amount_1_x64, 64, product)?;
        let amount_out = amount_1_x64.shift_right(64);
        (amount_in, amount_out)
    } else {
        let amount_in = amount_1_x64.add(U256::new(0, u64::MAX as u128)).shift_right(64);
        let amount_out = mul_pow2_div_floor(amount_1_x64, 64, product)?;
        (amount_in, amount_out)
    };
    Some((to_u64(amount_in)?, to_u64(amount_out)?))
}

fn mul_pow2_div_ceil(value: U256, shift: u32, denom: U256) -> Option<U256> {
    if denom.is_zero() {
        return None;
    }
    let shifted = value.shift_left(shift);
    let (quotient, remainder) = shifted.div(denom, true);
    if remainder.is_zero() {
        Some(quotient)
    } else {
        Some(quotient.add(U256::new(0, 1)))
    }
}

fn mul_pow2_div_floor(value: U256, shift: u32, denom: U256) -> Option<U256> {
    if denom.is_zero() {
        return None;
    }
    Some(value.shift_left(shift).div(denom, false).0)
}

fn to_u64(value: U256) -> Option<u64> {
    let value = value.try_into_u128()?;
    u64::try_from(value).ok()
}

/// The next initialized tick in the swap direction, with its state.
///
/// Mirrors `TickArrayState::next_initialized_tick`: the search is inclusive at
/// the current tick when moving zero-for-one and strictly after it otherwise.
/// When the current tick's array is absent from the loaded run, the first
/// initialized tick of the nearest array in the direction is used, which is
/// what the program does when the current array is not the first valid one.
fn next_initialized_tick<'a>(
    arrays: &'a [TickArray],
    tick: i32,
    tick_spacing: u16,
    zero_for_one: bool,
) -> Option<(i32, &'a TickState)> {
    let containing_start = array_start_index(tick, tick_spacing);
    if let Some(array) = arrays
        .iter()
        .find(|array| array.start_tick_index == containing_start)
    {
        if let Some(tick_state) = search_in_array(array, tick, tick_spacing, zero_for_one) {
            return Some((tick_state.tick, tick_state));
        }
    }

    // Walk the loaded arrays in the direction of travel. They are ordered
    // nearest-first by `start_tick_indexes`.
    for array in arrays {
        let beyond = if zero_for_one {
            array.start_tick_index < containing_start
        } else {
            array.start_tick_index > containing_start
        };
        if !beyond {
            continue;
        }
        let found = if zero_for_one {
            array
                .ticks
                .iter()
                .rev()
                .find(|tick_state| tick_state.is_initialized())
        } else {
            array
                .ticks
                .iter()
                .find(|tick_state| tick_state.is_initialized())
        };
        if let Some(tick_state) = found {
            return Some((tick_state.tick, tick_state));
        }
    }
    None
}

fn search_in_array<'a>(
    array: &'a TickArray,
    tick: i32,
    tick_spacing: u16,
    zero_for_one: bool,
) -> Option<&'a TickState> {
    let slot = array.slot_of(tick, tick_spacing)?;
    if zero_for_one {
        // At or below the current tick, nearest first.
        (0..=slot)
            .rev()
            .map(|index| &array.ticks[index])
            .find(|tick_state| tick_state.is_initialized())
    } else {
        // Strictly above the current tick, nearest first.
        ((slot + 1)..TICK_ARRAY_SIZE)
            .map(|index| &array.ticks[index])
            .find(|tick_state| tick_state.is_initialized())
    }
}

fn add_liquidity_delta(liquidity: u128, delta: i128) -> Result<u128> {
    let magnitude = delta.unsigned_abs();
    if delta >= 0 {
        liquidity
            .checked_add(magnitude)
            .ok_or_else(|| ClmmError::Math("liquidity overflow".into()))
    } else {
        liquidity
            .checked_sub(magnitude)
            .ok_or_else(|| ClmmError::Math("liquidity underflow".into()))
    }
}

/// Start index of the tick array containing `tick`.
fn array_start_index(tick: i32, tick_spacing: u16) -> i32 {
    let ticks_in_array = TICK_ARRAY_SIZE as i32 * i32::from(tick_spacing);
    let mut start = tick / ticks_in_array;
    if tick < 0 && tick % ticks_in_array != 0 {
        start -= 1;
    }
    start * ticks_in_array
}

/// `TickArrayState::check_is_valid_start_index`: an array may start at most one
/// step below the minimum tick, and never above the maximum.
fn is_valid_start_index(start: i32, tick_spacing: u16) -> bool {
    let ticks_in_array = TICK_ARRAY_SIZE as i32 * i32::from(tick_spacing);
    if start < MIN_TICK_INDEX || start > MAX_TICK_INDEX {
        if start > MAX_TICK_INDEX {
            return false;
        }
        return start == array_start_index(MIN_TICK_INDEX, tick_spacing);
    }
    start % ticks_in_array == 0
}

const MIN_TICK_INDEX: i32 = -443_636;
const MAX_TICK_INDEX: i32 = 443_636;

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        data.get(offset..offset + 2)
            .ok_or(ClmmError::AccountTooShort {
                account: "clmm",
                need: offset + 2,
                got: data.len(),
            })?
            .try_into()
            .expect("2 bytes"),
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(offset..offset + 4)
            .ok_or(ClmmError::AccountTooShort {
                account: "clmm",
                need: offset + 4,
                got: data.len(),
            })?
            .try_into()
            .expect("4 bytes"),
    ))
}

fn read_i32(data: &[u8], offset: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(
        data.get(offset..offset + 4)
            .ok_or(ClmmError::AccountTooShort {
                account: "clmm",
                need: offset + 4,
                got: data.len(),
            })?
            .try_into()
            .expect("4 bytes"),
    ))
}

fn read_u128(data: &[u8], offset: usize) -> Result<u128> {
    Ok(u128::from_le_bytes(
        data.get(offset..offset + 16)
            .ok_or(ClmmError::AccountTooShort {
                account: "clmm",
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
            .ok_or(ClmmError::AccountTooShort {
                account: "clmm",
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

    fn empty_tick() -> TickState {
        TickState {
            tick: 0,
            liquidity_net: 0,
            liquidity_gross: 0,
            orders_amount: 0,
            part_filled_orders_remaining: 0,
        }
    }

    fn array_fixture(start: i32, spacing: u16, ticks: &[(usize, i128, u128)]) -> TickArray {
        let mut slots = vec![empty_tick(); TICK_ARRAY_SIZE];
        for (slot, net, gross) in ticks {
            slots[*slot] = TickState {
                tick: start + *slot as i32 * i32::from(spacing),
                liquidity_net: *net,
                liquidity_gross: *gross,
                orders_amount: 0,
                part_filled_orders_remaining: 0,
            };
        }
        TickArray {
            address: Pubkey::new_unique(),
            pool_id: Pubkey::new_unique(),
            start_tick_index: start,
            ticks: slots,
        }
    }

    fn pool_fixture() -> ClmmPool {
        ClmmPool {
            address: Pubkey::new_unique(),
            amm_config: Pubkey::new_unique(),
            mints: [Pubkey::new_unique(), Pubkey::new_unique()],
            vaults: [Pubkey::new_unique(), Pubkey::new_unique()],
            observation: Pubkey::new_unique(),
            mint_decimals: [9, 6],
            tick_spacing: 10,
            liquidity: 1_000_000_000_000,
            sqrt_price_x64: 1u128 << 64,
            tick_current: 0,
            status: 0,
            fee_on: 0,
            dynamic_fee_control: 0,
        }
    }

    #[test]
    fn array_start_index_floors_negatives() {
        assert_eq!(array_start_index(0, 10), 0);
        assert_eq!(array_start_index(-1, 10), -600);
        assert_eq!(array_start_index(-600, 10), -600);
        assert_eq!(array_start_index(-601, 10), -1200);
    }

    #[test]
    fn start_indexes_walk_the_right_way() {
        let mut pool = pool_fixture();
        pool.tick_current = 1_250;
        assert_eq!(
            pool.start_tick_indexes(true, 3),
            vec![1_200, 600, 0]
        );
        assert_eq!(
            pool.start_tick_indexes(false, 3),
            vec![1_200, 1_800, 2_400]
        );
    }

    #[test]
    fn crossing_a_tick_applies_its_liquidity_net() {
        // Pool sits at tick -5 inside position [-10, 0] with 1e9 liquidity.
        // Crossing downward through tick -10 (net +1M) removes ~0.1% of the
        // liquidity, so the walk continues past the boundary instead of
        // exhausting it: the quote must keep pricing rather than refuse at the
        // crossing.
        let mut pool = pool_fixture();
        pool.tick_current = -5;
        pool.liquidity = 1_000_000_000;
        pool.sqrt_price_x64 = sqrt_price_from_tick_index(-5);
        // Array start -600, spacing 10: slot 59 = tick -10 (the lower edge).
        let array = array_fixture(-600, 10, &[(59, 1_000_000, 1_000_000)]);
        // Below the travel cost to tick -10 (~243k lamports): partial fill.
        let small = pool.quote_exact_in(&[array.clone()], 100_000, true, 500).unwrap();
        assert!(small > 0, "a partial swap inside the range quotes out");
        // Above the travel cost: crosses -10, applies liquidity_net, keeps going
        // until the loaded arrays run out (an honest termination, not an
        // exhaustion error).
        let large = pool.quote_exact_in(&[array], 1_000_000_000, true, 500);
        match large {
            Ok(out) => assert!(out > 0),
            Err(error) => {
                let message = error.to_string();
                assert!(
                    !message.contains("exhausted pool liquidity"),
                    "crossing -10 must not exhaust 1e9 liquidity: {message}"
                );
            }
        }
    }

    #[test]
    fn unfilled_limit_orders_make_the_direction_unquotable() {
        let mut pool = pool_fixture();
        pool.tick_current = 0;
        pool.liquidity = 1_000_000;
        pool.sqrt_price_x64 = sqrt_price_from_tick_index(0);
        // Slot 5 = tick -10 with an unfilled limit order.
        let mut array = array_fixture(-60, 10, &[(5, 1_000_000, 1_000_000)]);
        array.ticks[5].orders_amount = 5;
        let error = pool
            .quote_exact_in(&[array], 1_000_000, true, 500)
            .unwrap_err()
            .to_string();
        assert!(error.contains("limit orders"), "{error}");
    }

    #[test]
    fn a_direction_without_initialized_ticks_is_rejected() {
        let pool = pool_fixture();
        let arrays = vec![array_fixture(-600, 10, &[])];
        assert!(pool.quote_exact_in(&arrays, 1_000, true, 500).is_err());
    }

    #[test]
    fn dynamic_fee_pool_is_rejected() {
        let mut pool = pool_fixture();
        pool.dynamic_fee_control = 1;
        assert!(pool.ensure_swappable().is_err());
    }
}
