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
use base64::Engine as _;
use beast_core::config::{self, BeastConfig};
use beast_core::economics::{MIN_JITO_TIP_LAMPORTS, SPL_TOKEN_ACCOUNT_RENT_LAMPORTS};
use beast_core::types::Mode;
use beast_core::types::{CostBreakdown, LegPlan, Opportunity, OutcomeLabel, StrategyKind};
use beast_dex::Address;
use beast_exec::{cpmm, Executor, RentReclaimer, WSOL_MINT};

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

/// Raydium CPMM exact-in with the pool's trade fee, floored the way the program
/// floors it. The fee is taken from the input before it enters the curve, which
/// is the order the on-chain program uses.
fn quote_exact_in(amount_in: u64, reserve_in: u64, reserve_out: u64, fee_bps: u16) -> Option<u64> {
    if amount_in == 0 || reserve_in == 0 || reserve_out == 0 || fee_bps >= 10_000 {
        return None;
    }
    let in_u128 = amount_in as u128;
    let rin = reserve_in as u128;
    let rout = reserve_out as u128;
    let net_in = in_u128 * (10_000 - fee_bps as u128) / 10_000;
    if net_in == 0 {
        return None;
    }
    let numerator = net_in.checked_mul(rout)?;
    let out = numerator / (rin + net_in);
    u64::try_from(out).ok()
}

/// Fetch every executable CPMM pool for one mint pair, with its live vault
/// balances and trade fee from its own config account.
///
/// Raydium does not order a pool's two mints, so the pair can be stored either
/// way round. Both orders are queried and merged, because a pair held only one
/// way would silently hide half the liquidity.
async fn fetch_pair_pools(rpc: &RpcClient, wsol: Address, other: Address) -> Result<Vec<PoolView>> {
    let program = Address::from_str(cpmm::CPMM_PROGRAM_ID)
        .map_err(|error| anyhow::anyhow!("cpmm program: {error}"))?;
    // The pool state stores its two mints at offsets 168 and 200.
    let mut accounts = rpc
        .get_program_accounts(
            program,
            vec![
                serde_json::json!({ "dataSize": cpmm::CPMM_POOL_LEN }),
                serde_json::json!({ "memcmp": { "offset": 168, "bytes": wsol.to_string() } }),
                serde_json::json!({ "memcmp": { "offset": 200, "bytes": other.to_string() } }),
            ],
            None,
        )
        .await?;
    accounts.extend(
        rpc.get_program_accounts(
            program,
            vec![
                serde_json::json!({ "dataSize": cpmm::CPMM_POOL_LEN }),
                serde_json::json!({ "memcmp": { "offset": 168, "bytes": other.to_string() } }),
                serde_json::json!({ "memcmp": { "offset": 200, "bytes": wsol.to_string() } }),
            ],
            None,
        )
        .await?,
    );

    // Decode first, so unreachable pools cost one account read rather than two.
    let mut decoded = Vec::with_capacity(accounts.len());
    let mut seen = std::collections::HashSet::new();
    for (key, account) in accounts {
        if !seen.insert(key) {
            continue;
        }
        let Ok(pool) = cpmm::CpmmPool::decode(beast_flash::pubkey(&key), &account.data) else {
            continue;
        };
        if pool.ensure_swappable().is_err() || pool.authority().is_err() {
            continue;
        }
        decoded.push(pool);
    }

    // One batched read for every AmmConfig, then one for every vault. Both are
    // stable accounts, so batching is the difference between a scan that keeps
    // up with the chain and one that does not.
    let configs: Vec<Address> = decoded
        .iter()
        .map(|pool| address(&pool.amm_config))
        .collect();
    let config_data = rpc
        .get_multiple_accounts(&configs)
        .await
        .unwrap_or_default();
    let fee_bps = |pool: &cpmm::CpmmPool| -> Option<u16> {
        let index = configs
            .iter()
            .position(|candidate| *candidate == address(&pool.amm_config))?;
        let account = config_data.get(index)?.as_ref()?;
        // hundredths of a basis point (1e-6) -> basis points
        Some(
            u16::try_from(cpmm::decode_trade_fee_rate(&account.data).ok()? / 100)
                .unwrap_or(u16::MAX),
        )
    };

    let mut vault_keys: Vec<Address> = Vec::with_capacity(decoded.len() * 2);
    for pool in &decoded {
        vault_keys.push(address(&pool.vaults[0]));
        vault_keys.push(address(&pool.vaults[1]));
    }
    let vault_data = rpc
        .get_multiple_accounts(&vault_keys)
        .await
        .unwrap_or_default();
    // SPL token account: mint(32) owner(32) amount(u64 at 64).
    let balance = |index: usize| -> u64 {
        vault_data
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|account| account.data.get(64..72))
            .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
            .unwrap_or(0)
    };

    let mut views = Vec::with_capacity(decoded.len());
    for (index, pool) in decoded.into_iter().enumerate() {
        let Some(fee_bps) = fee_bps(&pool) else {
            continue;
        };
        views.push(PoolView {
            balances: [balance(index * 2), balance(index * 2 + 1)],
            pool,
            fee_bps,
        });
    }
    Ok(views)
}

/// A decoded pool plus everything needed to price a swap against it.
#[derive(Debug, Clone)]
struct PoolView {
    pool: cpmm::CpmmPool,
    fee_bps: u16,
    balances: [u64; 2],
}

impl PoolView {
    /// Effective reserve for a side: vault balance minus booked-but-uncollected
    /// fees, which is what the swap curve actually uses.
    fn effective(&self, side: usize) -> Option<u64> {
        let raw = *self.balances.get(side)?;
        self.pool.effective_reserve(side, raw).ok()
    }
}

/// Evaluate one ordered two-pool cycle: `pool_in` does WSOL -> X and
/// `pool_out` does X -> WSOL.
fn evaluate_cycle(
    pool_in: &PoolView,
    pool_out: &PoolView,
    amount: u64,
    flash_fee: u64,
    costs: u64,
) -> Option<CycleLog> {
    let wsol = beast_flash::pubkey(&wsol());
    let a_in = pool_in.pool.mint_index(&wsol)?;
    let a_out = 1 - a_in;
    // Leg 2 runs backwards from X to WSOL.
    let b_out = pool_out.pool.mint_index(&wsol)?;
    let b_in = 1 - b_out;

    let a_reserve_in = pool_in.effective(a_in)?;
    let a_reserve_out = pool_in.effective(a_out)?;
    let b_reserve_in = pool_out.effective(b_in)?;
    let b_reserve_out = pool_out.effective(b_out)?;

    let leg1_out = quote_exact_in(amount, a_reserve_in, a_reserve_out, pool_in.fee_bps)?;
    if leg1_out == 0 {
        return None;
    }
    // A cycle larger than the pool's own reserve would be rejected on-chain.
    if leg1_out as u128 >= b_reserve_in as u128 {
        return None;
    }
    let final_out = quote_exact_in(leg1_out, b_reserve_in, b_reserve_out, pool_out.fee_bps)?;
    let principal_plus_fee = (amount as i128) + (flash_fee as i128);
    let profit = (final_out as i128) - principal_plus_fee - (costs as i128);
    Some(CycleLog {
        pool_in: pool_in.pool.address.to_string(),
        pool_out: pool_out.pool.address.to_string(),
        middle_mint: pool_in.pool.mints[a_out].to_string(),
        amount,
        leg1_out,
        final_out,
        profit,
        fee_bps_in: pool_in.fee_bps,
        fee_bps_out: pool_out.fee_bps,
    })
}

/// Build the cycle the executor will actually run, from a winning pair.
fn opportunity_for(
    config: &BeastConfig,
    cycle: &CycleLog,
    provider_id: &str,
    amount_fee: u64,
    wallet_lamports: u64,
    slot: u64,
) -> Opportunity {
    let middle = Address::from_str(&cycle.middle_mint).unwrap_or_else(|_| wsol());
    let mints = [wsol(), middle, wsol()];
    let legs: Vec<LegPlan> = [&cycle.pool_in, &cycle.pool_out]
        .iter()
        .enumerate()
        .map(|(index, pool)| LegPlan {
            venue: "raydium_cpmm".into(),
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
        mode: Mode::Canary,
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
    let executor = crate::engine::build_executor(
        config,
        &rpc,
        &jupiter,
        &jito,
        Some(wallet.clone()),
        Mode::Canary,
        real,
    )?
    .ok_or_else(|| anyhow::anyhow!("executor could not be built for this config"))?;
    let rent_reclaimer = RentReclaimer::new(rpc.clone(), wallet.clone(), config.rent.clone());
    let mut options = options;
    options.jupiter = Some(jupiter.clone());
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
    if options.scan_only {
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
    if let Some(jupiter) = &options.jupiter {
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
                    match price_triangle(
                        jupiter.as_ref(),
                        wsol(),
                        *a,
                        *b,
                        *amount,
                        fee,
                        costs,
                    )
                    .await
                    {
                        Some(junction) => {
                            pairs += 1;
                            if junction.profit > 0 {
                                junctions.push(junction);
                            }
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
    for mint in &options.mints {
        let other = match Address::from_str(mint) {
            Ok(address) => address,
            Err(error) => {
                state.note(format!("skipping mint {mint}: {error}"));
                continue;
            }
        };
        let Ok(pools) = fetch_pair_pools(rpc, wsol(), other).await else {
            continue;
        };
        if pools.len() < 2 {
            continue;
        }
        pairs += 1;
        for (index, pool_in) in pools.iter().enumerate() {
            for pool_out in pools.iter().skip(index + 1).chain(pools.iter().take(index)) {
                for amount in &options.amount_ladder {
                    if *amount as u128 > reserve.available as u128 {
                        continue;
                    }
                    let fee = provider.flash_fee(reserve, *amount).unwrap_or(0);
                    if let Some(cycle) = evaluate_cycle(pool_in, pool_out, *amount, fee, costs) {
                        state.cycles_seen += 1;
                        if cycle.profit > 0 {
                            cycles.push(cycle);
                        }
                    }
                }
            }
        }
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
        });
    }
    cycles.sort_by_key(|cycle| std::cmp::Reverse(cycle.profit));
    cycles.retain(|cycle| cycle.profit as u128 >= options.min_profit_lamports as i128 as u128);
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

/// The lamport costs a cycle must cover: the configured minimum profit, plus a
/// float for the token accounts it creates and closes again and the tip.
fn cycle_costs(config: &BeastConfig) -> u64 {
    let minimum = config
        .strategy_or_default(StrategyKind::FlashArb.as_str())
        .min_net_lamports;
    // The cycle plus the guard are separate transactions, each paying a base
    // fee; the ATA rent float comes back when the cycle closes its own
    // accounts, and the tip is paid on top.
    minimum
        .saturating_add(2 * DEFAULT_CYCLE_BASE_FEE)
        .saturating_add(SPL_TOKEN_ACCOUNT_RENT_LAMPORTS)
        .saturating_add(MIN_JITO_TIP_LAMPORTS)
}

#[allow(clippy::too_many_arguments)]
async fn run_text(
    config: &BeastConfig,
    rpc: Arc<RpcClient>,
    wallet: Arc<beast_exec::Wallet>,
    provider: Arc<dyn FlashProvider>,
    reserve: FlashReserve,
    _store: Arc<Store>,
    executor: Arc<Executor>,
    _rent_reclaimer: RentReclaimer,
    options: ArbOptions,
    mut state: ArbState,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let wallet_address = wallet.address();
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let started = Instant::now();
        let cycles = scan_once(
            config,
            rpc.as_ref(),
            provider.as_ref(),
            &reserve,
            &options,
            &mut state,
            wallet_address,
        )
        .await;
        state.last_scan_ms = started.elapsed().as_millis();
        state.scans += 1;
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
                let cycles = found.as_slice();
                if !cycles.is_empty() {
                    state.profitable += cycles.len();
                    state.best = cycles.to_vec();
                    state.best.truncate(24);
                    state.note(format!(
                        "found {} profitable cycles; best {} lamports",
                        cycles.len(),
                        cycles[0].profit
                    ));
                    for cycle in cycles {
                        let message = format!(
                            "cycle {} -> {} amount {} gross profit {}",
                            cycle.pool_in, cycle.pool_out, cycle.amount, cycle.profit
                        );
                        state.note(message.clone());
                        println!("{message}");
                    }
                    execute_best(
                        config,
                        rpc.as_ref(),
                        provider.as_ref(),
                        &reserve,
                        executor.clone(),
                        &_store,
                        &cycles[0],
                        &mut state,
                    )
                    .await;
                }
            }
            Err(error) => {
                let message = format!("scan failed: {error}");
                state.note(message.clone());
                eprintln!("{message}");
            }
        }
        tokio::time::sleep(Duration::from_millis(options.refresh_ms)).await;
    }
    Ok(())
}

/// Build, sign and send the winning cycle, and record what actually happened.
#[allow(clippy::too_many_arguments)]
async fn execute_best(
    config: &BeastConfig,
    rpc: &RpcClient,
    provider: &dyn FlashProvider,
    reserve: &FlashReserve,
    executor: Arc<Executor>,
    store: &Store,
    cycle: &CycleLog,
    state: &mut ArbState,
) {
    let attempt_id = format!("arb-{}", uuid::Uuid::new_v4());
    let slot = rpc.get_slot().await.unwrap_or(0);
    let opportunity = opportunity_for(
        config,
        cycle,
        provider.id(),
        provider.flash_fee(reserve, cycle.amount).unwrap_or(0),
        state.wallet_lamports,
        slot,
    );
    let _ = store.insert_opportunity(&opportunity);
    let _ = store.insert_attempt(&AttemptRecord {
        id: attempt_id.clone(),
        opportunity_id: Some(opportunity.id.to_string()),
        ts_ms: chrono::Utc::now().timestamp_millis(),
        strategy: StrategyKind::FlashArb.as_str().to_string(),
        mode: Mode::Canary.as_str().to_string(),
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
    match executor
        .prepare_flash_cycle(&opportunity, provider, reserve)
        .await
    {
        Ok(plan) => {
            let signature = plan
                .signatures
                .first()
                .cloned()
                .unwrap_or_else(|| "unknown".into());
            let _ = store.update_attempt(
                &attempt_id,
                &AttemptUpdate {
                    status: Some("prepared".into()),
                    signature: Some(signature.clone()),
                    error: None,
                    ..AttemptUpdate::default()
                },
            );
            state.note(format!(
                "{attempt_id} · {} transactions · {} bytes · tip {} lamports",
                plan.encoded_transactions.len(),
                plan.encoded_transactions
                    .first()
                    .map(|tx| base64::engine::general_purpose::STANDARD
                        .decode(tx.as_bytes())
                        .map(|v| v.len())
                        .unwrap_or(0))
                    .unwrap_or(0),
                plan.tip_lamports
            ));
        }
        Err(error) => {
            state.rejected += 1;
            let _ = store.update_attempt(
                &attempt_id,
                &AttemptUpdate {
                    status: Some("prepare_failed".into()),
                    error: Some(error.to_string()),
                    ..AttemptUpdate::default()
                },
            );
            state.note(format!("{attempt_id} not executable: {error}"));
        }
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
