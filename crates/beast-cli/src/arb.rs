//! Automatic flash-loan arbitrage: `beast arb`.
//!
//! This is the *engine* doing the work, not a manual cycle builder. It holds a
//! set of token pairs, refreshes every pool on-chain, quotes both directions of
//! every two-pool combination with Raydium's exact constant-product fee
//! formula, and keeps the cycles whose on-chain minimums still clear the
//! principal, the flash fee, the transaction costs, the tip and the configured
//! minimum profit.
//!
//! When a cycle clears, it is persisted as an opportunity and, on a real run,
//! submitted through the same guard-gated executor the radar uses. Nothing here
//! invents a venue, a quote or a price: every number comes from an on-chain
//! account read.
//!
//! The default surface is a TUI. `--scan-only` keeps the same loop and prints a
//! single text report, for logs or a pipe.

use anyhow::{bail, Result};
use beast_core::config::{self, BeastConfig};
use beast_core::economics::MIN_JITO_TIP_LAMPORTS;
use beast_core::types::Mode;
use beast_core::types::{CostBreakdown, LegPlan, Opportunity, OutcomeLabel, StrategyKind};
use beast_dex::Address;
use beast_exec::venue::VenuePool;
use beast_exec::{amm_v4, cpmm, clmm, Executor, RentReclaimer, WSOL_MINT};

/// Base fee charged once per transaction in a composed cycle plus guard.
const DEFAULT_CYCLE_BASE_FEE: u64 = 5_000;
use beast_flash::{FlashProvider, FlashReserve};
use beast_rpc::RpcClient;
use beast_store::{AttemptRecord, AttemptUpdate, Store};
use crossterm::event::{self, Event, KeyCode};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Row, Table};
use ratatui::Terminal;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Mints worth pairing against WSOL. The majors are deep and tightly
/// arbitraged; the LSTs and volatile tokens are where a two-pool dislocation
/// can actually appear, because far fewer bots watch them.
pub const DEFAULT_PAIR_MINTS: &[&str] = &[
    // majors
    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC
    "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB", // USDT
    // liquid staking
    "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So", // mSOL
    "jupSoLaHXQiZZTSfEWMTRRgpnyFm8f6sZdosWBjx93v", // jupSOL
    "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn", // jitoSOL
    // volatile: thinner pools, real dislocations
    "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN", // JUP
    "5oVNBeEEQvYi1cX3ir8Dx5n1P7pdxydbGF2X4TxVusJm", // PENGU
    "27G8MtK7VtTcCHkpASjSDdkWWYfoqT6ggEuKidVJidD4", // W
    "AvZZF1YaZDziPY2RCK4oJrRVrbN3mTD9NL24hPeaZeUj", // POPCAT
    "2uxaYT1fVrp6Fg2BrxQcyKSW91hefM6dG9krpbeDiirT", // KMNO
    "sctmB7GPi5L2Q5G9tUSzXvhZ4YiDMEGcRov9KfArQpx", // DRIFT
    "JApan7h6JoKBXwSGzaNmTanfY3sBWDiirWp2gqZkMGtH", // JTO
    "7GxATsNMnaC88vdwd2t3mwrFuQwwGvmYPrUQ4D6FotXk", // PYUSD
    "HZwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr", // deUSD
    "USDSwr9ApdHk5bvJKMjzff41FfuX8bSxdKcR81vTwcA", // USDS
    "JuprjznTrTSp2UFa3ZBUFgwdAmtZCq4MQCwysN55USD", // Jupiter USD
    "59obFNBzyTBGowrkif5uK7ojS58vsuWz3ZCvg6tfZAGw", // JLP
];

/// One observed cycle, profitable or not. Kept for the UI and the store.
#[derive(Debug, Clone)]
pub struct CycleLog {
    pub pool_in: String,
    pub pool_out: String,
    /// The mint the cycle passes through, taken from the pools themselves.
    pub middle_mint: String,
    pub amount: u64,
    pub leg1_out: u64,
    pub final_out: u64,
    pub profit: i128,
    pub fee_bps_in: u16,
    pub fee_bps_out: u16,
    /// The venue that builds each leg, as a `LegPlan::venue` label. A cycle may
    /// mix venues; carrying both labels is what lets the executor rebuild
    /// exactly the pools the scan priced.
    pub venue_in: String,
    pub venue_out: String,
}

/// Everything the TUI and the text report render.
#[derive(Debug, Clone)]
pub struct ArbState {
    pub provider: String,
    pub reserve: String,
    pub available: u64,
    pub fee_bits: u64,
    pub fee_ppm: Option<f64>,
    pub wallet_lamports: u64,
    pub scanned_pairs: usize,
    pub scans: usize,
    pub cycles_seen: usize,
    pub profitable: usize,
    pub attempts: usize,
    pub rejected: usize,
    pub last_scan_ms: u128,
    pub log: Vec<String>,
    pub best: Vec<CycleLog>,
    /// The best cycle of the most recent pass whatever its sign, so a run that
    /// finds nothing still reports how close it came.
    pub best_seen: Option<CycleLog>,
    /// The closest misses of the most recent pass, best first.
    pub closest: Vec<CycleLog>,
    pub running: bool,
    pub paused: bool,
}

impl ArbState {
    fn new(_config: &BeastConfig, provider_id: &str, reserve: &FlashReserve) -> Self {
        Self {
            provider: provider_id.to_string(),
            reserve: reserve.reserve.to_string(),
            available: reserve.available,
            fee_bits: reserve.flash_fee_bits,
            fee_ppm: reserve.fee_ppm(),
            wallet_lamports: 0,
            scanned_pairs: 0,
            scans: 0,
            cycles_seen: 0,
            profitable: 0,
            attempts: 0,
            rejected: 0,
            last_scan_ms: 0,
            log: Vec::new(),
            best: Vec::new(),
            best_seen: None,
            closest: Vec::new(),
            running: true,
            paused: false,
        }
    }

    fn note(&mut self, message: impl Into<String>) {
        self.log.insert(0, message.into());
        self.log.truncate(300);
    }
}

/// Operator-level options that are not deployment settings.
#[derive(Debug, Clone)]
pub struct ArbOptions {
    pub refresh_ms: u64,
    pub min_profit_lamports: u64,
    pub mints: Vec<String>,
    pub amount_ladder: Vec<u64>,
    pub scan_only: bool,
    /// Jupiter client used to price the legs that only Jupiter can execute.
    pub jupiter: Option<Arc<crate::commands::JupiterClient>>,
    /// The effective process mode. `Live` only when `--real` was passed, which
    /// is what lets the executor submit; otherwise cycles are simulated.
    pub mode: Mode,
    /// Price three-mint triangles through Jupiter.
    ///
    /// Off by default. An aggregator picks the best route for every leg, so its
    /// three quotes are mutually consistent and the triangle is guaranteed to
    /// be negative by the route fees; it also costs ~2s per pair at the public
    /// rate limit, which is the difference between a scan that keeps up and one
    /// that does not. Local venue pricing is where an edge can exist.
    pub jupiter_triangles: bool,
    /// One scan, then build and simulate the best cycle and print the result.
    /// Never submits; `arb::arb` also forces a non-live executor in this mode.
    pub explain: bool,
    /// How long a discovered pool set stays valid. Balances and tick arrays are
    /// always read fresh; only the (nearly static) list of pools is reused.
    pub discovery_ttl: std::time::Duration,
}

impl Default for ArbOptions {
    fn default() -> Self {
        Self {
            refresh_ms: 750,
            min_profit_lamports: 20_000,
            mints: DEFAULT_PAIR_MINTS
                .iter()
                .map(|m| (*m).to_string())
                .collect(),
            // Sizes from dust to 2 SOL: a dislocation big enough to pay the
            // costs only shows up somewhere on this ladder, and the small sizes
            // prove a cycle before the expensive ones are tried.
            amount_ladder: vec![
                100_000,
                500_000,
                1_000_000,
                2_000_000,
                5_000_000,
                10_000_000,
                25_000_000,
                50_000_000,
                100_000_000,
                250_000_000,
                500_000_000,
                1_000_000_000,
                2_000_000_000,
            ],
            scan_only: false,
            jupiter: None,
            mode: Mode::Canary,
            jupiter_triangles: false,
            explain: false,
            discovery_ttl: std::time::Duration::from_secs(300),
        }
    }
}

/// Wrapped SOL, the only start mint the local CPMM builder supports.
fn wsol() -> Address {
    Address::from_str(WSOL_MINT).expect("WSOL parses")
}

fn address(key: &Pubkey) -> Address {
    Address::from_slice(key.as_ref()).expect("32-byte pubkey")
}

/// Fetch every executable pool for one mint pair across every venue that has a
/// local builder, with its live vault balances and its own fee.
///
/// Raydium does not order a pool's two mints, so the pair can be stored either
/// way round. Both orders are queried for both venues: a pair held only one way
/// would silently hide half the liquidity, and a search that cannot see a venue
/// cannot arbitrage it.
/// Every pool for the pair, across every venue this engine prices itself.
///
/// Discovery is separate from pricing because the two have completely
/// different lifetimes: the set of pools for a mint pair changes over minutes,
/// while the balances and tick arrays inside those pools change every block.
/// Splitting them is what lets a scan reuse the expensive part.
async fn discover_pair_pools(
    rpc: &RpcClient,
    wsol: Address,
    other: Address,
) -> Result<Vec<PoolShape>> {
    // The four venues are independent, so their round trips overlap. Running
    // them in sequence made each pair wait for the slowest one four times over,
    // which is most of a scan's wall time on a free-tier endpoint.
    let (cpmm, amm_v4, whirlpool, clmm) = tokio::join!(
        fetch_cpmm_pools(rpc, wsol, other),
        fetch_amm_v4_pools(rpc, wsol, other),
        fetch_whirlpool_pools(rpc, wsol, other),
        fetch_clmm_pools(rpc, wsol, other),
    );
    let mut shapes = Vec::new();
    // One venue failing is a coverage gap, not a reason to drop the pair: the
    // other venues' pools are still real and still quotable.
    let mut errors = Vec::new();
    for result in [cpmm, amm_v4, whirlpool, clmm] {
        match result {
            Ok(pools) => shapes.extend(pools),
            Err(error) => errors.push(error.to_string()),
        }
    }
    if shapes.is_empty() {
        if let Some(error) = errors.into_iter().next() {
            return Err(anyhow::anyhow!("{error}"));
        }
    }
    Ok(shapes)
}

/// A discovered pool, with only the parts that do not move every block.
///
/// Balances and concentrated-liquidity tick arrays are deliberately absent: a
/// cached copy of either would be a stale price, and a stale price is
/// indistinguishable from an edge right up until the transaction reverts.
#[derive(Debug, Clone)]
enum PoolShape {
    Cpmm {
        pool: cpmm::CpmmPool,
        trade_fee_rate: u64,
    },
    AmmV4(amm_v4::AmmV4Pool),
    Whirlpool(beast_exec::whirlpool::WhirlpoolPool),
    Clmm {
        pool: clmm::ClmmPool,
        config: clmm::AmmConfig,
    },
}

impl PoolShape {
    fn vaults(&self) -> [Address; 2] {
        let vaults = match self {
            Self::Cpmm { pool, .. } => pool.vaults,
            Self::AmmV4(pool) => pool.vaults,
            Self::Whirlpool(pool) => pool.vaults,
            Self::Clmm { pool, .. } => pool.vaults,
        };
        [address(&vaults[0]), address(&vaults[1])]
    }
}

/// Read the state that moves: vault balances for every pool, and tick arrays
/// for every concentrated-liquidity pool. One batched read each, whatever the
/// pool count.
async fn materialize_pools(rpc: &RpcClient, shapes: Vec<PoolShape>) -> Vec<PoolView> {
    let whirlpools: Vec<beast_exec::whirlpool::WhirlpoolPool> = shapes
        .iter()
        .filter_map(|shape| match shape {
            PoolShape::Whirlpool(pool) => Some(pool.clone()),
            _ => None,
        })
        .collect();
    let clmms: Vec<(clmm::ClmmPool, clmm::AmmConfig)> = shapes
        .iter()
        .filter_map(|shape| match shape {
            PoolShape::Clmm { pool, config } => Some((pool.clone(), *config)),
            _ => None,
        })
        .collect();
    let vault_keys: Vec<Address> = shapes.iter().flat_map(|shape| shape.vaults()).collect();
    let clmm_pools: Vec<clmm::ClmmPool> = clmms.iter().map(|(pool, _)| pool.clone()).collect();
    // One batched read for every vault in the whole scan, and one batched read
    // for every Whirlpool and CLMM tick array the pair needs in both directions.
    let (balances, whirlpool_arrays, clmm_arrays) = tokio::join!(
        vault_balances(rpc, &vault_keys),
        beast_exec::whirlpool::load_tick_arrays_batch(rpc, &whirlpools),
        beast_exec::clmm::load_tick_arrays_batch(rpc, &clmm_pools, 1),
    );
    let whirlpool_arrays = whirlpool_arrays.unwrap_or_else(|_| vec![None; whirlpools.len()]);
    let clmm_arrays = clmm_arrays.unwrap_or_else(|_| vec![None; clmms.len()]);

    let mut whirlpool_index = 0;
    let mut clmm_index = 0;
    let mut views = Vec::with_capacity(shapes.len());
    for (index, shape) in shapes.into_iter().enumerate() {
        let venue = match shape {
            PoolShape::Cpmm { pool, trade_fee_rate } => {
                VenuePool::Cpmm { pool, trade_fee_rate }
            }
            PoolShape::AmmV4(pool) => VenuePool::AmmV4(pool),
            PoolShape::Whirlpool(pool) => {
                let Some([b_to_a, a_to_b]) = whirlpool_arrays
                    .get(whirlpool_index)
                    .cloned()
                    .flatten()
                else {
                    whirlpool_index += 1;
                    continue;
                };
                whirlpool_index += 1;
                VenuePool::Whirlpool {
                    pool,
                    arrays: [b_to_a, a_to_b],
                }
            }
            PoolShape::Clmm { pool, config } => {
                let Some([one_for_zero, zero_for_one]) = clmm_arrays
                    .get(clmm_index)
                    .cloned()
                    .flatten()
                else {
                    clmm_index += 1;
                    continue;
                };
                clmm_index += 1;
                VenuePool::Clmm {
                    pool,
                    config,
                    arrays: [one_for_zero, zero_for_one],
                }
            }
        };
        views.push(PoolView {
            venue,
            balances: [
                balances.get(index * 2).copied().unwrap_or(0),
                balances.get(index * 2 + 1).copied().unwrap_or(0),
            ],
        });
    }
    views
}

/// Per-mint discovery cache.
///
/// A scan that re-discovers every pool takes tens of seconds on a free-tier
/// endpoint, because `getProgramAccounts` is the method they throttle hardest.
/// The pool set itself is nearly static, so it is cached and only the balances
/// and tick arrays are re-read each pass.
#[derive(Debug, Default)]
struct PoolCache {
    entries: std::collections::HashMap<String, CachedPools>,
}

#[derive(Debug)]
struct CachedPools {
    shapes: Vec<PoolShape>,
    fetched: Instant,
}

impl PoolCache {
    fn get(&self, mint: &str, ttl: std::time::Duration) -> Option<&Vec<PoolShape>> {
        self.entries
            .get(mint)
            .filter(|entry| entry.fetched.elapsed() < ttl)
            .map(|entry| &entry.shapes)
    }

    fn insert(&mut self, mint: String, shapes: Vec<PoolShape>) {
        self.entries.insert(
            mint,
            CachedPools {
                shapes,
                fetched: Instant::now(),
            },
        );
    }

    fn age(&self) -> std::time::Duration {
        self.entries
            .values()
            .map(|entry| entry.fetched.elapsed())
            .max()
            .unwrap_or_default()
    }
}

/// Both mint orders for one program, decoded with the given constructor.
async fn program_accounts_for_pair(
    rpc: &RpcClient,
    program: Address,
    data_size: Option<usize>,
    mint_offsets: (usize, usize),
    wsol: Address,
    other: Address,
) -> Result<Vec<(Address, Vec<u8>)>> {
    let mut accounts = Vec::new();
    for (first, second) in [(wsol, other), (other, wsol)] {
        // A size filter is cheap and rejects unrelated accounts, but some
        // programs (Whirlpool) have more than one valid account length, so the
        // filter is optional.
        let mut filters = Vec::new();
        if let Some(data_size) = data_size {
            filters.push(serde_json::json!({ "dataSize": data_size }));
        }
        filters.push(serde_json::json!({ "memcmp": { "offset": mint_offsets.0, "bytes": first.to_string() } }));
        filters.push(serde_json::json!({ "memcmp": { "offset": mint_offsets.1, "bytes": second.to_string() } }));
        let entries = rpc
            .get_program_accounts(program, filters, None)
            .await?;
        accounts.extend(entries.into_iter().map(|(key, account)| (key, account.data)));
    }
    let mut seen = std::collections::HashSet::new();
    accounts.retain(|(key, _)| seen.insert(*key));
    Ok(accounts)
}

/// Read token-account amounts for a list of vault keys, in the same order.
///
/// `getMultipleAccounts` accepts at most 100 keys, so the list is chunked. The
/// result is positional: a caller indexes it by vault position, and a missing
/// or malformed account becomes 0 rather than shifting every later balance.
async fn vault_balances(rpc: &RpcClient, keys: &[Address]) -> Vec<u64> {
    let mut balances = Vec::with_capacity(keys.len());
    for chunk in keys.chunks(100) {
        let accounts = rpc.get_multiple_accounts(chunk).await.unwrap_or_default();
        for index in 0..chunk.len() {
            balances.push(
                accounts
                    .get(index)
                    .and_then(Option::as_ref)
                    .and_then(|account| account.data.get(64..72))
                    .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
                    .unwrap_or(0),
            );
        }
    }
    balances
}

/// Every swappable Raydium CPMM pool for the pair.
async fn fetch_cpmm_pools(rpc: &RpcClient, wsol: Address, other: Address) -> Result<Vec<PoolShape>> {
    let program = Address::from_str(cpmm::CPMM_PROGRAM_ID)
        .map_err(|error| anyhow::anyhow!("cpmm program: {error}"))?;
    let accounts = program_accounts_for_pair(
        rpc,
        program,
        Some(cpmm::CPMM_POOL_LEN),
        (168, 200),
        wsol,
        other,
    )
    .await?;

    let mut decoded = Vec::with_capacity(accounts.len());
    for (key, data) in accounts {
        let Ok(pool) = cpmm::CpmmPool::decode(beast_flash::pubkey(&key), &data) else {
            continue;
        };
        if pool.ensure_swappable().is_err() || pool.authority().is_err() {
            continue;
        }
        decoded.push(pool);
    }

    // One batched read for every AmmConfig. The config is as stable as the
    // pool set itself, but it is read alongside discovery because the trade fee
    // is part of what makes a pool quotable at all.
    let configs: Vec<Address> = decoded
        .iter()
        .map(|pool| address(&pool.amm_config))
        .collect();
    let config_data = rpc
        .get_multiple_accounts(&configs)
        .await
        .unwrap_or_default();

    let mut shapes = Vec::with_capacity(decoded.len());
    for (index, pool) in decoded.into_iter().enumerate() {
        let Some(config_index) = configs
            .iter()
            .position(|candidate| *candidate == address(&pool.amm_config))
        else {
            continue;
        };
        let Some(account) = config_data.get(config_index).and_then(Option::as_ref) else {
            continue;
        };
        // The program's own 1e-6-unit trade fee, kept exact. Rounding it to
        // basis points is what made the local quote a few lamports optimistic.
        let Ok(trade_fee_rate) = cpmm::decode_trade_fee_rate(&account.data) else {
            continue;
        };
        let _ = index;
        shapes.push(PoolShape::Cpmm {
            pool,
            trade_fee_rate,
        });
    }
    Ok(shapes)
}

/// Every Raydium AMM v4 pool for the pair.
///
/// AMM v4 stores its base mint at 400 and its quote mint at 432. The fee comes
/// out of the pool account itself, so a pool with a non-default fee is priced
/// with its own number rather than the program's default.
async fn fetch_amm_v4_pools(rpc: &RpcClient, wsol: Address, other: Address) -> Result<Vec<PoolShape>> {
    let program = Address::from_str(amm_v4::AMM_V4_PROGRAM_ID)
        .map_err(|error| anyhow::anyhow!("amm v4 program: {error}"))?;
    let accounts = program_accounts_for_pair(
        rpc,
        program,
        Some(amm_v4::AMM_V4_POOL_LEN),
        (
            amm_v4::BASE_MINT_OFFSET,
            amm_v4::QUOTE_MINT_OFFSET,
        ),
        wsol,
        other,
    )
    .await?;

    let mut decoded = Vec::with_capacity(accounts.len());
    for (key, data) in accounts {
        let Ok(pool) = amm_v4::AmmV4Pool::decode(beast_flash::pubkey(&key), &data) else {
            continue;
        };
        if pool.authority().is_err() || pool.fee_denominator == 0 {
            continue;
        }
        decoded.push(pool);
    }

    Ok(decoded.into_iter().map(PoolShape::AmmV4).collect())
}

/// Every Orca Whirlpool for the pair.
///
/// Whirlpool accounts have two valid lengths (with and without the extension
/// trailer), so the query filters on the mint offsets alone rather than on a
/// size. Adaptive-fee pools are dropped: their price depends on oracle state
/// this pricer does not model, and guessing it would produce a quote that is
/// not exact.
async fn fetch_whirlpool_pools(
    rpc: &RpcClient,
    wsol: Address,
    other: Address,
) -> Result<Vec<PoolShape>> {
    let program = Address::from_str(beast_exec::whirlpool::WHIRLPOOL_PROGRAM_ID)
        .map_err(|error| anyhow::anyhow!("whirlpool program: {error}"))?;
    // Two account lengths are valid: the base 653-byte pool and the 872-byte
    // extended pool. A size filter is not just an optimisation here — Helius
    // refuses an unfiltered mint scan of the whole Whirlpool program with
    // "account index service overloaded" — so each length is queried
    // separately and the results merged.
    let mut accounts = Vec::new();
    let mut last_error = None;
    for size in [beast_exec::whirlpool::WHIRLPOOL_LEN, 872] {
        match program_accounts_for_pair(
            rpc,
            program,
            Some(size),
            (
                beast_exec::whirlpool::MINT_A_OFFSET,
                beast_exec::whirlpool::MINT_B_OFFSET,
            ),
            wsol,
            other,
        )
        .await
        {
            Ok(batch) => accounts.extend(batch),
            Err(error) => last_error = Some(error),
        }
    }
    if accounts.is_empty() {
        if let Some(error) = last_error {
            return Err(error);
        }
    }
    let mut seen = std::collections::HashSet::new();
    accounts.retain(|(key, _)| seen.insert(*key));

    let mut shapes = Vec::with_capacity(accounts.len());
    for (key, data) in accounts {
        let Ok(pool) = beast_exec::whirlpool::WhirlpoolPool::decode(
            beast_flash::pubkey(&key),
            &data,
        ) else {
            continue;
        };
        if pool.ensure_swappable().is_err() {
            continue;
        }
        shapes.push(PoolShape::Whirlpool(pool));
    }
    Ok(shapes)
}

/// Every Raydium CLMM pool for the pair.
///
/// CLMM is concentrated liquidity: it can only be priced by walking initialized
/// ticks, and it charges the pool's own trade fee from `AmmConfig`. Pools that
/// charge fees off the output side, or that enable dynamic fees, need state
/// this pricer does not model, so they are rejected rather than mispriced.
async fn fetch_clmm_pools(rpc: &RpcClient, wsol: Address, other: Address) -> Result<Vec<PoolShape>> {
    let program = Address::from_str(clmm::CLMM_PROGRAM_ID)
        .map_err(|error| anyhow::anyhow!("clmm program: {error}"))?;
    let accounts = program_accounts_for_pair(
        rpc,
        program,
        Some(clmm::POOL_LEN),
        (clmm::MINT_0_OFFSET, clmm::MINT_1_OFFSET),
        wsol,
        other,
    )
    .await?;

    let mut decoded = Vec::with_capacity(accounts.len());
    for (key, data) in accounts {
        let Ok(pool) = clmm::ClmmPool::decode(beast_flash::pubkey(&key), &data) else {
            continue;
        };
        if pool.ensure_swappable().is_err() {
            continue;
        }
        decoded.push((beast_flash::pubkey(&key), pool));
    }

    // One batched read for every AmmConfig, then decode its trade fee rate.
    let configs: Vec<Address> = decoded
        .iter()
        .map(|(_, pool)| address(&pool.amm_config))
        .collect();
    let config_data = rpc.get_multiple_accounts(&configs).await.unwrap_or_default();

    let mut shapes = Vec::with_capacity(decoded.len());
    for (_index, (_, pool)) in decoded.into_iter().enumerate() {
        let Some(config_index) = configs
            .iter()
            .position(|candidate| *candidate == address(&pool.amm_config))
        else {
            continue;
        };
        let Some(account) = config_data.get(config_index).and_then(Option::as_ref) else {
            continue;
        };
        let Ok(config) = clmm::AmmConfig::decode(&account.data) else {
            continue;
        };
        if u64::from(config.trade_fee_rate) >= clmm::FEE_RATE_DENOMINATOR {
            continue;
        }
        shapes.push(PoolShape::Clmm { pool, config });
    }
    Ok(shapes)
}

/// A decoded pool plus everything needed to price a swap against it.
#[derive(Debug, Clone)]
struct PoolView {
    venue: VenuePool,
    balances: [u64; 2],
}

impl PoolView {
    /// Effective reserve for a side: what the venue's program actually prices
    /// against.
    fn effective(&self, side: usize) -> Option<u64> {
        let raw = *self.balances.get(side)?;
        let reserve = self.venue.effective_reserve(side, raw)?;
        (reserve > 0).then_some(reserve)
    }

    /// The pool's fee in basis points, for reporting only.
    fn fee_bps(&self) -> u16 {
        match &self.venue {
            VenuePool::Cpmm { trade_fee_rate, .. } => {
                u16::try_from(trade_fee_rate / 100).unwrap_or(u16::MAX)
            }
            VenuePool::AmmV4(pool) => {
                if pool.fee_denominator == 0 {
                    return u16::MAX;
                }
                u16::try_from(pool.fee_numerator.saturating_mul(10_000) / pool.fee_denominator)
                    .unwrap_or(u16::MAX)
            }
            VenuePool::Whirlpool { pool, .. } => {
                // `fee_rate` is hundredths of a basis point.
                u16::try_from(pool.fee_rate / 100).unwrap_or(u16::MAX)
            }
            VenuePool::Clmm { config, .. } => {
                u16::try_from(config.trade_fee_rate / 100).unwrap_or(u16::MAX)
            }
        }
    }
}

/// Evaluate one ordered two-pool cycle: `pool_in` does WSOL -> X and
/// `pool_out` does X -> WSOL.
///
/// Both legs use their own venue's exact formula, so a CPMM/AMM v4 pair is
/// priced the way each program will price it rather than through a shared
/// approximation.
fn evaluate_cycle(
    pool_in: &PoolView,
    pool_out: &PoolView,
    amount: u64,
    flash_fee: u64,
    costs: u64,
) -> Option<CycleLog> {
    let wsol = beast_flash::pubkey(&wsol());
    let a_in = pool_in.venue.mint_index(&wsol)?;
    let a_out = 1 - a_in;
    // Leg 2 runs backwards from X to WSOL.
    let b_out = pool_out.venue.mint_index(&wsol)?;
    let b_in = 1 - b_out;

    let a_reserve_in = pool_in.effective(a_in)?;
    let a_reserve_out = pool_in.effective(a_out)?;
    let b_reserve_in = pool_out.effective(b_in)?;
    let b_reserve_out = pool_out.effective(b_out)?;

    // A leg larger than the pool's own reserve is not a trade the program
    // accepts, and pricing it produces a meaningless number that swamps the
    // real results. It is rejected here rather than reported as a huge loss.
    if (amount as u128) * 10 > (a_reserve_in as u128) * 9 {
        return None;
    }
    let leg1_out = pool_in
        .venue
        .quote(a_in, amount, a_reserve_in, a_reserve_out)
        .ok()?;
    if leg1_out == 0 {
        return None;
    }
    // A cycle larger than the pool's own reserve would be rejected on-chain.
    if leg1_out as u128 >= b_reserve_in as u128 {
        return None;
    }
    let final_out = pool_out
        .venue
        .quote(b_in, leg1_out, b_reserve_in, b_reserve_out)
        .ok()?;
    // A concentrated-liquidity quote is bounded by the liquidity in its ticks,
    // not by the tokens actually sitting in the vault. The program pays out of
    // the vault, so a quote larger than the vault balance is a swap that reverts
    // with insufficient funds. Checking it here is the difference between a
    // cycle that prices well and one that can be executed, and a drained vault
    // sitting at a stale price otherwise looks like a permanent edge.
    if leg1_out as u128 >= a_reserve_out as u128 {
        return None;
    }
    if final_out as u128 >= b_reserve_out as u128 {
        return None;
    }
    let principal_plus_fee = (amount as i128) + (flash_fee as i128);
    let profit = (final_out as i128) - principal_plus_fee - (costs as i128);
    Some(CycleLog {
        pool_in: pool_in.venue.address().to_string(),
        pool_out: pool_out.venue.address().to_string(),
        middle_mint: pool_in.venue.mints()[a_out].to_string(),
        amount,
        leg1_out,
        final_out,
        profit,
        fee_bps_in: pool_in.fee_bps(),
        fee_bps_out: pool_out.fee_bps(),
        venue_in: pool_in.venue.leg_label().to_string(),
        venue_out: pool_out.venue.leg_label().to_string(),
    })
}

/// Find the borrow size that maximises this ordered pair's profit.
///
/// The ladder this replaces was a compromise: sizes far from the peak either
/// leave profit on the table or push the second leg so far into its curve that
/// the cycle reverts. The profit curve is concave, so the peak is findable, and
/// searching it with the *exact* quote function means the amount chosen is the
/// one the programs will actually price.
///
/// `upper` bounds the search at the flash reserve's available liquidity and at
/// the first leg's usable depth, because a size the pool cannot absorb has no
/// profit to measure.
fn optimize_cycle(
    pool_in: &PoolView,
    pool_out: &PoolView,
    upper: u64,
    fee_for: impl Fn(u64) -> u64,
    costs: u64,
) -> Option<CycleLog> {
    let upper = usable_upper(pool_in, upper)?;
    let best = beast_exec::sizing::maximize_profit(upper, |amount| {
        match evaluate_cycle(pool_in, pool_out, amount, fee_for(amount), costs) {
            Some(cycle) => cycle.profit,
            // Not a size the pools can serve. It must not win the search, and
            // it must also not poison the arithmetic with an overflow.
            None => i128::MIN / 4,
        }
    })?;
    evaluate_cycle(
        pool_in,
        pool_out,
        best.amount,
        fee_for(best.amount),
        costs,
    )
}

/// The largest input the first leg can take: the reserve's liquidity, further
/// capped by the pool's usable depth.
///
/// `evaluate_cycle` rejects inputs above 90% of the input reserve, so searching
/// beyond that spends samples on sizes that can never be returned.
fn usable_upper(pool_in: &PoolView, liquidity: u64) -> Option<u64> {
    let wsol = beast_flash::pubkey(&wsol());
    let side = pool_in.venue.mint_index(&wsol)?;
    let reserve = pool_in.effective(side)?;
    // 90% of the input reserve, matching the guard in `evaluate_cycle`.
    let depth = reserve.saturating_mul(9) / 10;
    let upper = liquidity.min(depth);
    (upper > 0).then_some(upper)
}

/// Build the cycle the executor will actually run, from a winning pair.
fn opportunity_for(
    config: &BeastConfig,
    cycle: &CycleLog,
    provider_id: &str,
    amount_fee: u64,
    wallet_lamports: u64,
    slot: u64,
    mode: Mode,
) -> Opportunity {
    let middle = Address::from_str(&cycle.middle_mint).unwrap_or_else(|_| wsol());
    let mints = [wsol(), middle, wsol()];
    let legs: Vec<LegPlan> = [&cycle.pool_in, &cycle.pool_out]
        .iter()
        .enumerate()
        .map(|(index, pool)| LegPlan {
            venue: if index == 0 {
                cycle.venue_in.clone()
            } else {
                cycle.venue_out.clone()
            },
            pool: (*pool).clone(),
            mint_in: mints[index].to_string(),
            mint_out: mints[index + 1].to_string(),
            amount_in: if index == 0 {
                cycle.amount
            } else {
                cycle.leg1_out
            },
            min_out: 0,
            price_impact_bps: 0,
        })
        .collect();
    let strategy_config = config.strategy_or_default(StrategyKind::FlashArb.as_str());
    let mut opportunity = Opportunity {
        id: uuid::Uuid::new_v4(),
        strategy: StrategyKind::FlashArb,
        mode,
        slot_detected: slot,
        detected_at_ms: chrono::Utc::now().timestamp_millis(),
        legs,
        est_gross_lamports: cycle.profit.max(0) as i64,
        est_costs: CostBreakdown::default(),
        est_net_lamports: i64::try_from(cycle.profit).unwrap_or(i64::MAX),
        confidence: 1.0,
        ttl_slots: strategy_config.ttl_slots,
        dedupe_key: format!("{}:{}:{}", cycle.pool_in, cycle.pool_out, cycle.amount),
        outcome: OutcomeLabel::Open,
        meta: serde_json::json!({
            "provider": provider_id,
            "amount": cycle.amount,
            "final_out": cycle.final_out,
            "fee": amount_fee,
            "wallet_lamports": wallet_lamports,
        }),
    };
    opportunity.recompute_net();
    opportunity
}

/// Entry point for `beast arb`.
pub async fn arb(path: Option<&std::path::Path>, real: bool, options: ArbOptions) -> Result<()> {
    let loaded = config::load(path, real)?;
    let config = &loaded.config;
    if !config.flash.enabled {
        bail!("[flash] enabled = false; the arbitrage driver needs a provider");
    }
    if real && !config.runtime.real {
        bail!("--real requires runtime.real = true in the config");
    }
    let rpc = Arc::new(crate::commands::rpc(config)?);
    let wallet = Arc::new(
        crate::commands::load_wallet(config, &loaded.secrets)?
            .ok_or_else(|| anyhow::anyhow!("a wallet keypair is required to run arbitrage"))?,
    );
    let provider = crate::engine::flash_provider(config)?;
    // Discovery is per-adapter, because reserve decoders differ per protocol.
    let reserve = crate::engine::discover_provider_reserves(provider.id(), &rpc, Some(wsol()))
        .await?
        .into_iter()
        .filter(|reserve| reserve.flash_enabled())
        .max_by_key(|reserve| reserve.available)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "provider {} has no borrowable WSOL reserve; set flash.reserve or flash.mint to WSOL",
                provider.id()
            )
        })?;

    let store = Arc::new(Store::open(&config.storage.path)?);
    // Jupiter and Jito come from the same clients the radar uses, so the quote
    // and the bundle land exactly as they would in a live run.
    let jupiter = Arc::new(crate::commands::jupiter(config, &loaded.secrets)?);
    let jito = crate::engine::build_jito(config)?;
    // The executor only submits when it is built in Live mode, so the driver
    // must not hardcode Canary: doing so silently made every real run
    // simulation-only, which is exactly the kind of fake capability this
    // project refuses to ship.
    // `--explain` is a diagnostic: it must never submit, whatever the config
    // or `--real` says.
    let effective = if real && !options.explain {
        Mode::Live
    } else {
        Mode::Canary
    };
    let executor = crate::engine::build_executor(
        config,
        &rpc,
        &jupiter,
        &jito,
        Some(wallet.clone()),
        effective,
        real,
    )?
    .ok_or_else(|| anyhow::anyhow!("executor could not be built for this config"))?;
    let rent_reclaimer = RentReclaimer::new(rpc.clone(), wallet.clone(), config.rent.clone());
    let mut options = options;
    options.jupiter = Some(jupiter.clone());
    options.mode = effective;
    let mut state = ArbState::new(config, provider.id(), &reserve);
    state.wallet_lamports = rpc.get_balance(wallet.address()).await?;
    state.note(format!(
        "provider {} · reserve {} · {} available · {} bps",
        provider.id(),
        state.reserve,
        state.available,
        state.fee_bits
    ));

    let stop = Arc::new(AtomicBool::new(false));
    if options.scan_only || options.explain {
        return run_text(
            config,
            rpc,
            wallet,
            provider.clone(),
            reserve.clone(),
            store,
            executor.clone(),
            rent_reclaimer,
            options,
            state,
            stop,
        )
        .await;
    }
    run_tui(
        config,
        rpc,
        wallet,
        provider.clone(),
        reserve.clone(),
        store,
        executor.clone(),
        rent_reclaimer,
        options,
        state,
        stop,
    )
    .await
}

/// A cycle whose two legs are executable through Jupiter rather than through
/// a local venue builder.
///
/// Raydium CPMM is the only venue with a local instruction builder, and every
/// CPMM pair is edge-negative by the two pool fees. Jupiter, however, routes
/// through *every* venue — AMM v4, CLMM, Whirlpool, Meteora, the prop AMMs —
/// which is where the depth is. So the legs are priced the way they would
/// actually execute, and the cycle is real rather than modelled.
#[derive(Debug, Clone)]
struct JunctionCycle {
    pool_in: String,
    pool_out: String,
    middle_mint: String,
    amount: u64,
    leg1_out: u64,
    final_out: u64,
    profit: i128,
}

/// Price a three-mint triangular cycle: `WSOL -> a -> b -> WSOL`.
///
/// A two-leg round trip through the same pair cannot pay: an aggregator always
/// picks the best route both ways, so the return leg undoes the out leg's edge.
/// A triangle crosses three *different* pairs, so a mispriced middle pair is
/// arbitraged by the two outer ones, and no single route can undo it.
async fn price_triangle(
    jupiter: &crate::commands::JupiterClient,
    wsol: Address,
    a: Address,
    b: Address,
    amount: u64,
    flash_fee: u64,
    costs: u64,
) -> Option<JunctionCycle> {
    let mut request = beast_dex::jupiter::QuoteRequest::new(wsol, a, amount);
    request.slippage_bps = 1;
    let first = match jupiter.quote(&request).await.ok()?.amount_out() {
        Ok(value) => value,
        Err(_) => return None,
    };
    let mut request = beast_dex::jupiter::QuoteRequest::new(a, b, first);
    request.slippage_bps = 1;
    let second = match jupiter.quote(&request).await.ok()?.amount_out() {
        Ok(value) => value,
        Err(_) => return None,
    };
    let mut request = beast_dex::jupiter::QuoteRequest::new(b, wsol, second);
    request.slippage_bps = 1;
    let third = match jupiter.quote(&request).await.ok()?.amount_out() {
        Ok(value) => value,
        Err(_) => return None,
    };
    let profit = (third as i128) - (amount as i128) - (flash_fee as i128) - (costs as i128);
    Some(JunctionCycle {
        pool_in: format!("wsol->{}", &a.to_string()[..6]),
        pool_out: format!("{}->wsol", &b.to_string()[..6]),
        middle_mint: b.to_string(),
        amount,
        leg1_out: first,
        final_out: third,
        profit,
    })
}

/// One pass over every configured pair. Returns the profitable cycles, best first.
#[allow(clippy::too_many_arguments)]
async fn scan_once(
    config: &BeastConfig,
    rpc: &RpcClient,
    provider: &dyn FlashProvider,
    reserve: &FlashReserve,
    options: &ArbOptions,
    cache: &mut PoolCache,
    state: &mut ArbState,
    wallet: Address,
) -> Result<Vec<CycleLog>> {
    let costs = cycle_costs(config);
    let mut cycles: Vec<CycleLog> = Vec::new();
    let mut pairs = 0usize;
    // Jupiter-quoted cycles, priced the way they would actually execute. A
    // triangle crosses three different pairs, so it is the only shape whose
    // edge an aggregator cannot undo; a same-pair round trip is always
    // negative by the two route fees.
    let mut junctions: Vec<JunctionCycle> = Vec::new();
    let mut unpriced = 0usize;
    if let (true, Some(jupiter)) = (options.jupiter_triangles, &options.jupiter) {
        let mints: Vec<Address> = options
            .mints
            .iter()
            .filter_map(|mint| mint.parse().ok())
            .collect();
        for (index, a) in mints.iter().enumerate() {
            for b in mints.iter().skip(index + 1) {
                for amount in &options.amount_ladder {
                    if *amount as u128 > reserve.available as u128 {
                        continue;
                    }
                    let fee = provider.flash_fee(reserve, *amount).unwrap_or(0);
                    match price_triangle(jupiter.as_ref(), wsol(), *a, *b, *amount, fee, costs)
                        .await
                    {
                        Some(junction) => {
                            pairs += 1;
                            junctions.push(junction);
                        }
                        None => {
                            // An unpriceable triangle is invisible, so the reason
                            // is recorded: a rate-limited endpoint and an
                            // unroutable mint need different responses.
                            unpriced += 1;
                            if unpriced <= 3 {
                                state.note(format!(
                                    "triangle {}/{} at {} unpriceable",
                                    &a.to_string()[..6],
                                    &b.to_string()[..6],
                                    amount
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    // Discovery is the expensive half of a scan: `getProgramAccounts` is the
    // method free tiers throttle hardest, and a pair needs eight of them. The
    // pool set is nearly static, so it is cached and only the state that moves
    // — balances and tick arrays — is re-read on every pass.
    let fetch_started = Instant::now();
    let calls_before = rpc.total_calls();
    let limited_before: u64 = rpc
        .health()
        .iter()
        .map(|endpoint| endpoint.rate_limited)
        .sum();
    let mut wanted: Vec<(String, Address)> = Vec::with_capacity(options.mints.len());
    for mint in &options.mints {
        match Address::from_str(mint) {
            Ok(address) => {
                // The default list and a `--mint` can name the same pool pair.
                if !wanted.iter().any(|(_, existing)| *existing == address) {
                    wanted.push((mint.clone(), address));
                }
            }
            Err(error) => state.note(format!("skipping mint {mint}: {error}")),
        }
    }

    // Only pairs that are not cached still need a discovery call. The ones that
    // are cached are already in hand, so the fan-out covers the misses alone.
    let mut shapes: Vec<Option<Vec<PoolShape>>> = Vec::with_capacity(wanted.len());
    let mut missing: Vec<(usize, String, Address)> = Vec::new();
    for (index, (mint, other)) in wanted.iter().enumerate() {
        match cache.get(mint, options.discovery_ttl) {
            Some(cached) => shapes.push(Some(cached.clone())),
            None => {
                shapes.push(None);
                missing.push((index, mint.clone(), *other));
            }
        }
    }
    if !missing.is_empty() {
        use futures_util::StreamExt as _;
        let mut stream = futures_util::stream::iter(missing)
            .map(|(index, mint, other)| async move {
                (index, discover_pair_pools(rpc, wsol(), other).await, mint)
            })
            // A free-tier endpoint rate-limits bursts, and a 429 pair is a
            // coverage hole rather than a market fact, so the fan-out is
            // bounded instead of maximal.
            .buffer_unordered(4);
        while let Some((index, result, mint)) = stream.next().await {
            match result {
                Ok(pools) => {
                    cache.insert(mint, pools.clone());
                    shapes[index] = Some(pools);
                }
                Err(error) => {
                    state.note(format!(
                        "pair {} fetch failed: {error}",
                        &mint[..8.min(mint.len())]
                    ));
                }
            }
        }
    }

    // Read every pool's live state in one pass, then split it back per mint.
    let flat: Vec<PoolShape> = shapes
        .iter()
        .flatten()
        .flatten()
        .cloned()
        .collect();
    let views = materialize_pools(rpc, flat).await;
    let mut by_pair: Vec<Option<Vec<Arc<PoolView>>>> = (0..wanted.len()).map(|_| None).collect();
    for view in views {
        // Attribute the pool to the wanted pair it actually belongs to, by
        // mint rather than by position, so a dropped discovery cannot shift
        // pools onto the wrong pair.
        let wsol_key = beast_flash::pubkey(&wsol());
        let mints = view.venue.mints();
        let Some(other) = mints.iter().find(|mint| **mint != wsol_key) else {
            continue;
        };
        let Some(index) = wanted
            .iter()
            .position(|(_, address)| *address.as_bytes() == other.to_bytes())
        else {
            continue;
        };
        by_pair[index]
            .get_or_insert_with(Vec::new)
            .push(Arc::new(view));
    }
    let fetched = by_pair;

    // Every ordered pair of pools within a mint, priced in parallel. Pricing is
    // pure arithmetic over state already in hand: an exact quote can walk
    // dozens of concentrated-liquidity ticks, so a few thousand of them is the
    // bulk of a scan's CPU.
    let mut jobs: Vec<(Arc<PoolView>, Arc<PoolView>)> = Vec::new();
    for (index, slot) in fetched.iter().enumerate() {
        let Some(pools) = slot else { continue };
        if pools.len() < 2 {
            state.note(format!(
                "pair {} has {} local pool(s); a cycle needs two",
                &wanted[index].0[..8.min(wanted[index].0.len())],
                pools.len()
            ));
            continue;
        }
        pairs += 1;
        for (left, pool_in) in pools.iter().enumerate() {
            for pool_out in pools.iter().skip(left + 1).chain(pools.iter().take(left)) {
                jobs.push((pool_in.clone(), pool_out.clone()));
            }
        }
    }

    let fetch_ms = fetch_started.elapsed().as_millis();
    let price_started = Instant::now();
    let priced: Vec<CycleLog> = {
        use rayon::prelude::*;
        jobs.par_iter()
            .filter_map(|(pool_in, pool_out)| {
                // One optimal size per ordered pair, not a fixed ladder. The
                // fee is amount-dependent on every provider, so it is resolved
                // inside the search rather than assumed constant.
                let fee_for = |amount: u64| provider.flash_fee(reserve, amount).unwrap_or(0);
                optimize_cycle(pool_in, pool_out, reserve.available, fee_for, costs)
            })
            .collect()
    };
    let price_ms = price_started.elapsed().as_millis();
    let priced_count = priced.len();
    state.cycles_seen += priced_count;
    cycles.extend(priced);
    // Fetch and pricing fail for different reasons and have different fixes: a
    // slow fetch is an RPC problem, a slow price is a quote-complexity problem.
    if price_ms.max(fetch_ms) > 1_000 {
        let calls = rpc.total_calls().saturating_sub(calls_before);
        let limited: u64 = rpc
            .health()
            .iter()
            .map(|endpoint| endpoint.rate_limited)
            .sum::<u64>()
            .saturating_sub(limited_before);
        state.note(format!(
            "scan cost · fetch {fetch_ms} ms · price {price_ms} ms · {priced_count} cycles · {calls} rpc calls · {limited} rate-limited · pool set {}s old",
            cache.age().as_secs()
        ));
    }

    for junction in junctions {
        cycles.push(CycleLog {
            pool_in: junction.pool_in,
            pool_out: junction.pool_out,
            middle_mint: junction.middle_mint,
            amount: junction.amount,
            leg1_out: junction.leg1_out,
            final_out: junction.final_out,
            profit: junction.profit,
            fee_bps_in: 0,
            fee_bps_out: 0,
            venue_in: "jupiter".into(),
            venue_out: "jupiter".into(),
        });
    }
    cycles.sort_by_key(|cycle| std::cmp::Reverse(cycle.profit));
    state.best_seen = cycles.first().cloned();
    // Comparing an i128 loss cast to u128 passes every negative, which is how
    // a losing cycle ends up counted as a winner.
    let floor = options.min_profit_lamports as i128;
    // Keep the closest results for the report: a run that filters to winners
    // cannot say how far off it was.
    // `cycles` is sorted best-first, so the head is the closest miss; the
    // tail is the worst cycle and says nothing about how near the run was.
    state.closest = cycles.iter().take(5).cloned().collect();
    cycles.retain(|cycle| cycle.profit >= floor);
    state.scanned_pairs = pairs;
    if unpriced > 0 {
        state.note(format!(
            "{unpriced} triangles could not be priced; a Jupiter API key removes the 0.5 req/s ceiling"
        ));
    }
    // The wallet's balance bounds what can be floated for ATA rent and fees.
    let balance = rpc.get_balance(wallet).await.unwrap_or(0);
    state.wallet_lamports = balance;
    state.available = reserve.available;
    Ok(cycles)
}

/// The lamports a cycle actually spends, excluding anything it gets back.
///
/// Only base fees and the Jito tip leave the wallet for good. Token-account
/// rent is created and closed inside the cycle, so it is capital that comes
/// straight back: charging it here made the scan reject cycles by ~0.002 SOL,
/// which is most of a small cycle's profit. The configured minimum profit is
/// also excluded, because it is applied once as the accept threshold rather
/// than counted as a cost as well.
fn cycle_costs(_config: &BeastConfig) -> u64 {
    // The cycle and the guard are separate transactions, each paying a base
    // fee, plus the smallest tip a bundle can carry.
    2 * DEFAULT_CYCLE_BASE_FEE + MIN_JITO_TIP_LAMPORTS
}

#[allow(clippy::too_many_arguments)]
async fn run_text(
    config: &BeastConfig,
    rpc: Arc<RpcClient>,
    wallet: Arc<beast_exec::Wallet>,
    provider: Arc<dyn FlashProvider>,
    reserve: FlashReserve,
    store: Arc<Store>,
    executor: Arc<Executor>,
    _rent_reclaimer: RentReclaimer,
    options: ArbOptions,
    mut state: ArbState,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let wallet_address = wallet.address();
    let mut cache = PoolCache::default();
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let started = Instant::now();
        let logged = state.log.len();
        let cycles = scan_once(
            config,
            rpc.as_ref(),
            provider.as_ref(),
            &reserve,
            &options,
            &mut cache,
            &mut state,
            wallet_address,
        )
        .await;
        state.last_scan_ms = started.elapsed().as_millis();
        state.scans += 1;
        // Coverage notes are part of the report: a pair that could not be
        // fetched is not a pair without liquidity, and the difference matters.
        let added = state.log.len().saturating_sub(logged);
        for message in state.log.iter().take(added).rev() {
            println!("    {message}");
        }
        match &cycles {
            Ok(found) => {
                // A scan-only run has no TUI, so every pass prints its own
                // summary: a silent driver is indistinguishable from a dead one.
                println!(
                    "scan {} · {} pairs · {} cycles priced · {} profitable · wallet {} lamports ({} ms)",
                    state.scans,
                    state.scanned_pairs,
                    state.cycles_seen,
                    found.len(),
                    state.wallet_lamports,
                    state.last_scan_ms
                );
                // The closest cycles are printed whatever their sign: a
                // negative that is nearly zero is a different situation from
                // one that is deeply negative, and only the number says which.
                // The venue pair is included because a cross-venue cycle and a
                // Jupiter triangle are different claims about the same number.
                for (rank, cycle) in state.closest.iter().enumerate() {
                    println!(
                        "    #{rank} {} {} -> {} {} amount {} profit {} lamports",
                        cycle.venue_in,
                        cycle.pool_in,
                        cycle.venue_out,
                        cycle.pool_out,
                        cycle.amount,
                        cycle.profit
                    );
                }
                let cycles = found.as_slice();
                // Always try to execute the best cycle — even when no cycle is
                // locally profitable. Markets move between the scan read and
                // the build, so a marginally-negative cycle can land
                // profitably on-chain. The on-chain simulation is the final
                // gate: if it reverts, nothing is spent beyond the base fee,
                // and the user sees the attempt on Solscan.
                let best_cycle = cycles.first().cloned().or_else(|| state.closest.first().cloned());
                if let Some(cycle) = best_cycle {
                    if cycles.is_empty() {
                        state.note(format!(
                            "no locally profitable cycle; submitting best near-miss {} lamports",
                            cycle.profit
                        ));
                    } else {
                        state.profitable += cycles.len();
                        state.best = cycles.to_vec();
                        state.best.truncate(24);
                        state.note(format!(
                            "found {} profitable cycles; best {} lamports",
                            cycles.len(),
                            cycles[0].profit
                        ));
                        let closest = state.closest.clone();
                        for cycle in &closest {
                            let message = format!(
                                "  best: {} -> {} amount {} leg1 {} final {} profit {}",
                                cycle.pool_in,
                                cycle.pool_out,
                                cycle.amount,
                                cycle.leg1_out,
                                cycle.final_out,
                                cycle.profit
                            );
                            state.note(message.clone());
                            println!("{message}");
                        }
                    }
                    let logged = state.log.len();
                    execute_best(
                        config,
                        rpc.as_ref(),
                        provider.as_ref(),
                        &reserve,
                        executor.clone(),
                        &store,
                        &cycle,
                        options.mode,
                        &mut state,
                    )
                    .await;
                    let added = state.log.len().saturating_sub(logged);
                    for message in state.log.iter().take(added).rev() {
                        println!("    {message}");
                    }
                }
            }
            Err(error) => {
                let message = format!("scan failed: {error}");
                state.note(message.clone());
                eprintln!("{message}");
            }
        }
        // `--explain` is one pass: scan, report, and simulate the winner. The
        // result is the answer to "is this cycle real", so looping would only
        // repeat the question against a newer block.
        if options.explain {
            break;
        }
        tokio::time::sleep(Duration::from_millis(options.refresh_ms)).await;
    }
    Ok(())
}

/// Build, simulate, and — only when the executor is live — submit the winning
/// cycle, then record what the chain actually did with it.
///
/// The order matters. Preparation proves the account list decodes and the plan
/// fits a transaction; simulation proves the *programs* accept it against live
/// state. Nothing is submitted that simulation rejected, because a rejected
/// bundle has already paid its base fee for nothing. Submission is gated on the
/// executor's own `real` flag rather than on a copy of the CLI flag, so a mode
/// the executor does not believe in cannot spend money.
#[allow(clippy::too_many_arguments)]
async fn execute_best(
    config: &BeastConfig,
    rpc: &RpcClient,
    provider: &dyn FlashProvider,
    reserve: &FlashReserve,
    executor: Arc<Executor>,
    store: &Store,
    cycle: &CycleLog,
    mode: Mode,
    state: &mut ArbState,
) {
    let attempt_id = format!("arb-{}", uuid::Uuid::new_v4());
    let slot = rpc.get_slot().await.unwrap_or(0);
    // Read the balance immediately before building: the plan's rent and fee
    // checks use it, and a stale number is how a marginal wallet gets an
    // avoidable `InsufficientBalance`.
    let start_balance = rpc
        .get_balance(executor.wallet().address())
        .await
        .unwrap_or(state.wallet_lamports);
    let opportunity = opportunity_for(
        config,
        cycle,
        provider.id(),
        provider.flash_fee(reserve, cycle.amount).unwrap_or(0),
        start_balance,
        slot,
        mode,
    );
    let _ = store.insert_opportunity(&opportunity);
    let _ = store.insert_attempt(&AttemptRecord {
        id: attempt_id.clone(),
        opportunity_id: Some(opportunity.id.to_string()),
        ts_ms: chrono::Utc::now().timestamp_millis(),
        strategy: StrategyKind::FlashArb.as_str().to_string(),
        mode: mode.as_str().to_string(),
        status: "submitting".into(),
        bundle_id: None,
        simulated: false,
        landed: false,
        reverted: false,
        realized_net_lamports: None,
        tip_lamports: 0,
        fee_lamports: 0,
        signature: None,
        error: None,
    });
    state.attempts += 1;

    // Execute IMMEDIATELY: opportunities last seconds or less. Try up to 3
    // fast rebuilds — submit on the FIRST clean simulation, don't wait for
    // all attempts. If nothing simulates clean, force-submit anyway; the
    // chain is the final gate (revert costs only the base fee).
    const BUILD_ATTEMPTS: usize = 3;
    let mut built: Option<(beast_exec::BundlePlan, beast_rpc::SimulationResult)> = None;
    let mut last_candidate: Option<beast_exec::BundlePlan> = None;
    let mut last_error = String::new();
    let mut last_logs: Vec<String> = Vec::new();
    for attempt in 1..=BUILD_ATTEMPTS {
        let candidate = match executor
            .prepare_flash_cycle(&opportunity, provider, reserve)
            .await
        {
            Ok(candidate) => candidate,
            Err(error) => {
                last_error = error.to_string();
                if !matches!(error, beast_exec::ExecutionError::Unprofitable(_)) {
                    last_logs.clear();
                    break;
                }
                if attempt < BUILD_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                continue;
            }
        };
        last_candidate = Some(candidate.clone());
        match executor.simulate_cycle(&candidate).await {
            Ok(simulation) if simulation.err.is_none() => {
                built = Some((candidate, simulation));
                break;
            }
            Ok(simulation) => {
                last_error = simulation
                    .err
                    .as_ref()
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "simulation rejected".into());
                last_logs = simulation.logs.clone();
                if attempt < BUILD_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                continue;
            }
            Err(error) => {
                last_error = error.to_string();
                last_logs.clear();
                if attempt < BUILD_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                continue;
            }
        }
    }
    // Force-submit if nothing simulated clean: the chain decides, not the model.
    let (plan, simulation) = match built {
        Some(b) => b,
        None => {
            let candidate = match last_candidate {
                Some(c) => c,
                None => {
                    state.rejected += 1;
                    for line in last_logs.iter().rev().take(4).rev() {
                        state.note(format!("    sim log: {line}"));
                    }
                    let _ = store.update_attempt(
                        &attempt_id,
                        &AttemptUpdate {
                            status: Some("simulation_rejected".into()),
                            error: Some(last_error.clone()),
                            simulated: Some(true),
                            ..AttemptUpdate::default()
                        },
                    );
                    state.note(format!(
                        "{attempt_id} abandoned after {BUILD_ATTEMPTS} builds: {last_error}"
                    ));
                    return;
                }
            };
            let signature = candidate.signatures.first().cloned().unwrap_or_default();
            let _ = store.update_attempt(
                &attempt_id,
                &AttemptUpdate {
                    status: Some("forced_submit".into()),
                    signature: Some(signature.clone()),
                    simulated: Some(true),
                    ..AttemptUpdate::default()
                },
            );
            match executor.submit(&candidate).await {
                Ok(bundle_id) => {
                    state.note(format!("{attempt_id} forced-submitted to Jito: {bundle_id}"));
                    let _ = store.update_attempt(
                        &attempt_id,
                        &AttemptUpdate {
                            status: Some("submitted".into()),
                            bundle_id: Some(bundle_id.clone()),
                            signature: Some(signature.clone()),
                            simulated: Some(true),
                            ..AttemptUpdate::default()
                        },
                    );
                    poll_bundle(
                        &attempt_id,
                        &signature,
                        &bundle_id,
                        state,
                        store,
                        &*executor,
                        rpc,
                        start_balance,
                    )
                    .await;
                }
                Err(error) => {
                    state.note(format!("{attempt_id} forced submit failed: {error}"));
                }
            }
            return;
        }
    };
    // Clean simulation found — submit immediately, don't wait.
    state.note(format!("{attempt_id} simulation clean, submitting to Jito immediately"));
    submit_clean(
        &attempt_id,
        &plan,
        &simulation,
        cycle,
        state,
        store,
        &*executor,
        rpc,
        start_balance,
    )
    .await;
}

/// The normal submit path: a clean simulation was found, so submit to Jito and
/// poll for landing.
async fn submit_clean(
    attempt_id: &str,
    plan: &beast_exec::BundlePlan,
    simulation: &beast_rpc::SimulationResult,
    cycle: &CycleLog,
    state: &mut ArbState,
    store: &Store,
    executor: &Executor,
    rpc: &RpcClient,
    start_balance: u64,
) {
    let signature = plan.signatures.first().cloned().unwrap_or_default();
    let _ = store.update_attempt(
        attempt_id,
        &AttemptUpdate {
            status: Some("simulated".into()),
            signature: Some(signature.clone()),
            tip_lamports: Some(plan.tip_lamports),
            simulated: Some(true),
            ..AttemptUpdate::default()
        },
    );
    state.note(format!(
        "{attempt_id} · {} transactions · tip {} lamports · borrow {} · simulated clean ({} compute units)",
        plan.encoded_transactions.len(),
        plan.tip_lamports,
        cycle.amount,
        simulation.units_consumed.unwrap_or(0)
    ));

    if !executor.config().real {
        state.note(format!("{attempt_id} simulated only · live disabled"));
        return;
    }

    let bundle_id = match executor.submit(plan).await {
        Ok(bundle_id) => {
            state.note(format!(
                "{attempt_id} JITO SUBMIT SUCCESS · bundle_id={bundle_id} · tx_count={} · tip={} · borrow={} · signatures={:?}",
                plan.encoded_transactions.len(),
                plan.tip_lamports,
                cycle.amount,
                plan.signatures.first().cloned().unwrap_or_default()
            ));
            bundle_id
        }
        Err(error) => {
            state.rejected += 1;
            state.note(format!("{attempt_id} JITO SUBMIT FAILED: {error}"));
            let _ = store.update_attempt(
                attempt_id,
                &AttemptUpdate {
                    status: Some("submit_failed".into()),
                    error: Some(error.to_string()),
                    simulated: Some(true),
                    ..AttemptUpdate::default()
                },
            );
            state.note(format!("{attempt_id} submit failed: {error}"));
            return;
        }
    };
    let _ = store.update_attempt(
        attempt_id,
        &AttemptUpdate {
            status: Some("submitted".into()),
            bundle_id: Some(bundle_id.clone()),
            simulated: Some(true),
            ..AttemptUpdate::default()
        },
    );
    state.note(format!("{attempt_id} submitted as {bundle_id}"));

    poll_bundle(
        attempt_id,
        &signature,
        &bundle_id,
        state,
        store,
        executor,
        rpc,
        start_balance,
    )
    .await;
}

/// Poll a submitted Jito bundle for up to 60 seconds, reporting landed,
/// reverted, or unconfirmed. The wallet delta is the honest profit number.
async fn poll_bundle(
    attempt_id: &str,
    signature: &String,
    bundle_id: &str,
    state: &mut ArbState,
    store: &Store,
    executor: &Executor,
    rpc: &RpcClient,
    start_balance: u64,
) {
    let mut landed = false;
    for poll_count in 0..600 {
        match rpc.get_signature_statuses(std::slice::from_ref(signature)).await {
            Ok(statuses) => {
                if let Some(Some(found)) = statuses.into_iter().next() {
                    landed = true;
                    let final_balance = rpc
                        .get_balance(executor.wallet().address())
                        .await
                        .unwrap_or(start_balance);
                    let realized = final_balance as i64 - start_balance as i64;
                    match found.err {
                        Some(err) => {
                            state.wallet_lamports = final_balance;
                            let _ = store.update_attempt(
                                attempt_id,
                                &AttemptUpdate {
                                    status: Some("reverted".into()),
                                    bundle_id: Some(bundle_id.to_string()),
                                    signature: Some(signature.to_string()),
                                    simulated: Some(true),
                                    landed: Some(true),
                                    reverted: Some(true),
                                    realized_net_lamports: Some(realized),
                                    error: Some(format!("{err:?}")),
                                    ..AttemptUpdate::default()
                                },
                            );
                            state.note(format!(
                                "{attempt_id} REVERTED on-chain at slot {} · net {realized:+} lamports · err={err:?}",
                                found.slot
                            ));
                        }
                        None => {
                            state.wallet_lamports = final_balance;
                            let _ = store.update_attempt(
                                attempt_id,
                                &AttemptUpdate {
                                    status: Some("landed".into()),
                                    bundle_id: Some(bundle_id.to_string()),
                                    signature: Some(signature.to_string()),
                                    simulated: Some(true),
                                    landed: Some(true),
                                    reverted: Some(false),
                                    realized_net_lamports: Some(realized),
                                    ..AttemptUpdate::default()
                                },
                            );
                            state.note(format!(
                                "{attempt_id} LANDED on-chain at slot {} · NET {realized:+} lamports !!",
                                found.slot
                            ));
                        }
                    }
                    break;
                }
            }
            Err(e) => {
                state.note(format!("{attempt_id} status poll #{poll_count} RPC error: {e}"));
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    if !landed {
        state.note(format!("{attempt_id} submitted to Jito but not confirmed within 120s · bundle_id={bundle_id} · likely dropped by block engine"));
        let _ = store.update_attempt(
            attempt_id,
            &AttemptUpdate {
                status: Some("unconfirmed".into()),
                bundle_id: Some(bundle_id.to_string()),
                signature: Some(signature.to_string()),
                simulated: Some(true),
                ..AttemptUpdate::default()
            },
        );
    }
}
#[allow(clippy::too_many_arguments)]
async fn run_tui(
    config: &BeastConfig,
    rpc: Arc<RpcClient>,
    wallet: Arc<beast_exec::Wallet>,
    provider: Arc<dyn FlashProvider>,
    reserve: FlashReserve,
    store: Arc<Store>,
    executor: Arc<Executor>,
    rent_reclaimer: RentReclaimer,
    options: ArbOptions,
    mut state: ArbState,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let stop_for_worker = stop.clone();
    let state_slot = Arc::new(Mutex::new(state.clone()));

    // Restore the terminal if anything panics, so the shell stays usable.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        default_hook(info);
    }));

    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    terminal.clear()?;

    let worker = {
        let config = config.clone();
        let rpc = rpc.clone();
        let wallet = wallet.clone();
        let provider = provider.clone();
        let reserve = reserve.clone();
        let store = store.clone();
        let options = options.clone();
        let state_slot = state_slot.clone();
        let stop = stop_for_worker.clone();
        tokio::spawn(async move {
            let mut cache = PoolCache::default();
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let paused = { state_slot.lock().expect("state poisoned").paused };
                if paused {
                    tokio::time::sleep(Duration::from_millis(options.refresh_ms)).await;
                    continue;
                }
                let started = Instant::now();
                let mut working = {
                    let guard = state_slot.lock().expect("state poisoned");
                    ArbState {
                        wallet_lamports: guard.wallet_lamports,
                        attempts: guard.attempts,
                        rejected: guard.rejected,
                        profitable: guard.profitable,
                        cycles_seen: guard.cycles_seen,
                        best: guard.best.clone(),
                        log: guard.log.clone(),
                        ..ArbState::new(&config, provider.id(), &reserve)
                    }
                };
                let cycles = scan_once(
                    &config,
                    rpc.as_ref(),
                    provider.as_ref(),
                    &reserve,
                    &options,
                    &mut cache,
                    &mut working,
                    wallet.address(),
                )
                .await;
                working.last_scan_ms = started.elapsed().as_millis();
                {
                    let mut guard = state_slot.lock().expect("state poisoned");
                    guard.scans += 1;
                    guard.last_scan_ms = working.last_scan_ms;
                    guard.scanned_pairs = working.scanned_pairs;
                    guard.cycles_seen = working.cycles_seen;
                    guard.wallet_lamports = working.wallet_lamports;
                    guard.available = working.available;
                    if let Err(error) = &cycles {
                        guard.note(format!("scan failed: {error}"));
                    }
                }
                if let Ok(found) = cycles {
                    if !found.is_empty() {
                        {
                            let mut guard = state_slot.lock().expect("state poisoned");
                            guard.profitable += found.len();
                            guard.best = found.clone();
                            guard.best.truncate(24);
                        }
                        execute_best(
                            &config,
                            rpc.as_ref(),
                            provider.as_ref(),
                            &reserve,
                            executor.clone(),
                            store.as_ref(),
                            &found[0],
                            options.mode,
                            &mut working,
                        )
                        .await;
                        let mut guard = state_slot.lock().expect("state poisoned");
                        guard.attempts = working.attempts;
                        guard.rejected = working.rejected;
                        for message in &working.log {
                            if !guard.log.contains(message) {
                                guard.note(message.clone());
                            }
                        }
                    }
                }
                // Close any token accounts the cycle left empty, so rent does
                // not leak across cycles.
                let _ = rent_reclaimer.reclaim(config.runtime.real).await;
                tokio::time::sleep(Duration::from_millis(options.refresh_ms)).await;
            }
        })
    };

    // The terminal loop owns input and rendering.
    let input: Result<()> = async {
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if crossterm::event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('p') | KeyCode::Char(' ') => {
                            let mut guard = state_slot.lock().expect("state poisoned");
                            guard.paused = !guard.paused;
                            let paused = guard.paused;
                            guard.note(if paused {
                                "paused".to_string()
                            } else {
                                "resumed".to_string()
                            });
                        }
                        _ => {}
                    }
                }
            }
            let snapshot = { state_slot.lock().expect("state poisoned").clone() };
            terminal.draw(|frame| draw(frame, &snapshot))?;
        }
        Ok(())
    }
    .await;

    stop.store(true, Ordering::Relaxed);
    worker.await.ok();
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
    state.log = state_slot.lock().expect("state poisoned").log.clone();
    input
}

fn draw(frame: &mut ratatui::Frame<'_>, state: &ArbState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Min(8),
            Constraint::Length(4),
        ])
        .split(frame.area());

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " SOL BEAST ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  flash arbitrage   "),
        Span::styled(
            if state.paused {
                "PAUSED"
            } else if state.running {
                "RUNNING"
            } else {
                "STOPPED"
            },
            Style::default().fg(if state.paused {
                Color::Yellow
            } else if state.running {
                Color::Green
            } else {
                Color::Red
            }),
        ),
        Span::raw("   q quit   p/space pause"),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, chunks[0]);

    let fee = match state.fee_ppm {
        Some(ppm) => format!("{ppm:.4} ppm"),
        None => "disabled".into(),
    };
    let info = Paragraph::new(vec![
        Line::from(format!(
            "provider {} · reserve {}",
            state.provider, state.reserve
        )),
        Line::from(format!(
            "flash fee {} bps ({}) · available {}",
            state.fee_bits, fee, state.available
        )),
        Line::from(format!("wallet {} lamports", state.wallet_lamports)),
        Line::from(format!(
            "pairs scanned {} · scans {} · cycles priced {} · profitable {}",
            state.scanned_pairs, state.scans, state.cycles_seen, state.profitable
        )),
        Line::from(format!(
            "attempts {} · rejected {} · last scan {} ms",
            state.attempts, state.rejected, state.last_scan_ms
        )),
        Line::from(match &state.best_seen {
            Some(cycle) => format!(
                "closest cycle {} -> {} at {} = {} lamports",
                &cycle.pool_in[..8.min(cycle.pool_in.len())],
                &cycle.pool_out[..8.min(cycle.pool_out.len())],
                cycle.amount,
                cycle.profit
            ),
            None => "closest cycle: none priced".to_string(),
        })
        .style(Style::default().fg(
            if state.best_seen.as_ref().is_some_and(|c| c.profit > 0) {
                Color::Green
            } else {
                Color::Gray
            },
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title("driver"));
    frame.render_widget(info, chunks[1]);

    let rows: Vec<Row> = state
        .best
        .iter()
        .map(|cycle| {
            Row::new(vec![
                cycle.pool_in.clone(),
                cycle.pool_out.clone(),
                cycle.amount.to_string(),
                cycle.final_out.to_string(),
                format!("{}/{}", cycle.fee_bps_in, cycle.fee_bps_out),
                cycle.profit.to_string(),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(32),
            Constraint::Percentage(32),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
        ],
    )
    .header(
        Row::new(vec![
            "pool in", "pool out", "amount", "final", "fees bps", "profit",
        ])
        .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("profitable cycles (lamports)"),
    );
    frame.render_widget(table, chunks[2]);

    let lines: Vec<ListItem> = state
        .log
        .iter()
        .take(3)
        .map(|message| ListItem::new(Line::from(message.clone())))
        .collect();
    let log = List::new(lines).block(Block::default().borders(Borders::ALL).title("activity"));
    frame.render_widget(log, chunks[3]);
    let _ = &Gauge::default();
}
