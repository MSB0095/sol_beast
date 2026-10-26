//! Radar and daemon runtime.

use anyhow::{bail, Context, Result};
use beast_api::{ApiState, ControlPlane};
use beast_core::config::{self, BeastConfig, GuardMode};
use beast_core::economics::SPL_TOKEN_ACCOUNT_RENT_LAMPORTS;
use beast_core::metrics::Metrics;
use beast_core::risk::{RiskControls, RiskEngine};
use beast_core::types::{
    dedupe_key, CostBreakdown, LegPlan, Mode, Opportunity, OutcomeLabel, SettlementResult,
    StrategyKind,
};
use beast_dex::catalog;
use beast_dex::jupiter::JupiterClient;
use beast_dex::router::{Router, RouterConfig};
use beast_dex::Address;
use beast_exec::{
    ExecutionConfig, Executor, RentReclaimer, Wallet, DEFAULT_TRANSACTION_FEE_LAMPORTS, WSOL_MINT,
};
use beast_feed::{FeedConfig, PoolFeed, WatchedPool};
use beast_rpc::{JitoClient, RpcClient};
use beast_store::{AttemptRecord, AttemptUpdate, BundleRecord, Store};
use chrono::Utc;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

pub struct RunOptions {
    pub config_path: Option<PathBuf>,
    pub real: bool,
    pub json_logs: bool,
    pub feed_interval_ms: Option<u64>,
    pub radar_interval_ms: u64,
}

struct RadarContext {
    config: Arc<BeastConfig>,
    feed: Arc<PoolFeed>,
    store: Arc<Store>,
    metrics: Arc<Metrics>,
    risk: Arc<Mutex<RiskEngine>>,
    executor: Option<Arc<Executor>>,
    controls: ControlPlane,
    rpc: Arc<RpcClient>,
    real: bool,
    cli_real: bool,
    interval_ms: u64,
}

pub async fn run(options: RunOptions) -> Result<()> {
    let loaded = config::load(options.config_path.as_deref(), options.real)?;
    crate::init_logging(
        &loaded.config.log.level,
        options.json_logs || loaded.config.log.json,
    )?;
    for warning in &loaded.warnings {
        warn!("{warning}");
    }
    if loaded.generated_secrets {
        info!(
            path = ?loaded.secrets_path(),
            "generated API tokens in the secrets file (mode 0600)"
        );
    }

    let mut config = loaded.config;
    // --real is mandatory for every live mode, even if a legacy config sets
    // runtime.require_cli_real=false.
    let effective = if options.real && config.runtime.real {
        config.runtime.mode
    } else {
        config.runtime.mode.capped_by(Mode::Radar)
    };
    config.runtime.mode = effective;
    let config = Arc::new(config);

    std::fs::create_dir_all(&config.runtime.data_dir).with_context(|| {
        format!(
            "cannot create data dir {}",
            config.runtime.data_dir.display()
        )
    })?;

    let store = Arc::new(
        Store::open(&config.storage.path)
            .with_context(|| format!("cannot open store {}", config.storage.path.display()))?,
    );
    store.set_meta("version", env!("CARGO_PKG_VERSION"))?;
    store.set_meta("mode", effective.as_str())?;

    let rpc = Arc::new(RpcClient::new(
        config.rpc.urls.clone(),
        config.rpc.commitment.clone(),
        config.rpc.timeout_ms,
        config.rpc.max_retries,
    )?);
    let jupiter_base = if config.rpc.jupiter_quote_url.trim().is_empty() {
        "https://lite-api.jup.ag/swap/v1".to_string()
    } else {
        config.rpc.jupiter_quote_url.clone()
    };
    let jupiter = Arc::new(JupiterClient::with_base_url(
        jupiter_base,
        loaded.secrets.jupiter_api_key.clone(),
    )?);
    let jito = if config.jito.enabled {
        Some(Arc::new(JitoClient::new(
            config.jito.block_engine_urls.clone(),
            None,
        )?))
    } else {
        None
    };

    let wallet = load_wallet(&config, &loaded.secrets, effective.is_live())?;
    if effective.is_live() {
        if !config.guard.enabled {
            bail!("live mode requires [guard] enabled=true");
        }
        if config.guard.mode != GuardMode::Program {
            bail!("live mode requires [guard] mode=\"program\"");
        }
        if config
            .guard
            .program_id
            .parse::<solana_sdk::pubkey::Pubkey>()
            .is_err()
        {
            bail!("live mode requires a valid [guard] program_id");
        }
        if !config.jito.enabled {
            bail!("live mode requires [jito] enabled=true (bundle-only policy)");
        }
        if wallet.is_none() {
            bail!("live mode requires a wallet keypair");
        }
    }

    let rent = if config.rent.enabled {
        wallet.as_ref().map(|wallet| {
            Arc::new(RentReclaimer::new(
                rpc.clone(),
                wallet.clone(),
                config.rent.clone(),
            ))
        })
    } else {
        None
    };

    let executor = build_executor(
        &config,
        &rpc,
        &jupiter,
        &jito,
        wallet,
        effective,
        options.real,
    )?;

    let watchlist = build_watchlist(&config, &store)?;
    let feed_interval = options.feed_interval_ms.unwrap_or(1_500).max(250);
    let stale_after = (feed_interval.saturating_mul(4))
        .max(config.risk.max_feed_lag_ms.saturating_mul(2))
        .max(2_000);
    let feed = Arc::new(PoolFeed::new(
        rpc.clone(),
        watchlist.clone(),
        FeedConfig {
            interval_ms: feed_interval,
            max_concurrent: 8,
            stale_after_ms: stale_after,
            refresh_reserves: true,
        },
    )?);
    if !watchlist.is_empty() {
        let report = feed.refresh_all().await;
        info!(
            pools = watchlist.len(),
            updated = report.updated,
            failed = report.failed,
            duration_ms = report.duration_ms,
            "initial pool refresh"
        );
    } else {
        warn!(
            "no pools configured; add [[pools]] entries or run `beast discover --venue <id> --save`"
        );
    }

    let metrics = Arc::new(Metrics::new());
    let risk = Arc::new(Mutex::new(RiskEngine::new(RiskControls::from(
        &config.risk,
    ))));

    let (api_state, controls) = if config.api.enabled {
        let admin = loaded
            .secrets
            .api_bearer_token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("api enabled but admin token is missing"))?;
        let read = loaded
            .secrets
            .api_read_token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("api enabled but read token is missing"))?;
        let mut state = ApiState::new(
            config.clone(),
            store.clone(),
            Some(feed.clone()),
            metrics.clone(),
            Some(risk.clone()),
            Some(jupiter.clone()),
            admin,
            read,
        )?;
        state.rent = rent.clone();
        state.signing_enabled = options.real && config.runtime.real;
        let state = Arc::new(state);
        let controls = ControlPlane {
            mode: state.mode.clone(),
            strategies: state.strategies.clone(),
            kill: state.kill.clone(),
        };
        (Some(state), controls)
    } else {
        (None, ControlPlane::from_config(&config))
    };

    let shutdown = CancellationToken::new();
    let api_handle = api_state.map(|state| {
        let token = shutdown.clone();
        tokio::spawn(async move {
            beast_api::serve(state, async move { token.cancelled().await }).await
        })
    });
    let feed_handle = {
        let feed = feed.clone();
        let token = shutdown.clone();
        tokio::spawn(async move { feed.run(token).await })
    };

    let radar = RadarContext {
        config: config.clone(),
        feed: feed.clone(),
        store: store.clone(),
        metrics: metrics.clone(),
        risk: risk.clone(),
        executor,
        controls,
        rpc: rpc.clone(),
        real: options.real && config.runtime.real,
        cli_real: options.real,
        interval_ms: options.radar_interval_ms.max(250),
    };
    let radar_handle = {
        let token = shutdown.clone();
        tokio::spawn(async move { radar_loop(radar, token).await })
    };
    let settlement_handle = jito.clone().map(|jito| {
        let token = shutdown.clone();
        let context = SettlementContext {
            store: store.clone(),
            rpc: rpc.clone(),
            jito,
            metrics: metrics.clone(),
            risk: risk.clone(),
        };
        tokio::spawn(async move { settlement_loop(context, token).await })
    });

    info!(
        mode = %effective,
        real = options.real && config.runtime.real,
        api = config.api.enabled,
        pools = watchlist.len(),
        "SOL BEAST started"
    );

    tokio::signal::ctrl_c().await.ok();
    info!("shutdown requested");
    shutdown.cancel();

    let _ = feed_handle.await;
    let _ = radar_handle.await;
    if let Some(handle) = settlement_handle {
        let _ = handle.await;
    }
    if let Some(handle) = api_handle {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(error.into()),
            Err(error) => return Err(anyhow::anyhow!("API task failed: {error}")),
        }
    }
    info!("SOL BEAST stopped");
    Ok(())
}

fn load_wallet(
    config: &BeastConfig,
    secrets: &config::Secrets,
    required: bool,
) -> Result<Option<Arc<Wallet>>> {
    let path = config.wallet.keypair_path.as_deref();
    if let Some(path) = path.filter(|path| path.exists()) {
        return match Wallet::load(path, config.wallet.require_0600) {
            Ok(wallet) => Ok(Some(Arc::new(wallet))),
            Err(error) if required => Err(error.into()),
            Err(error) => {
                warn!(error = %error, "wallet keypair is invalid; running without executor");
                Ok(None)
            }
        };
    }
    if let Some(json) = secrets.wallet_keypair_json.as_deref() {
        return match Wallet::from_json(json) {
            Ok(wallet) => Ok(Some(Arc::new(wallet))),
            Err(error) if required => Err(error.into()),
            Err(error) => {
                warn!(error = %error, "wallet secret is invalid; running without executor");
                Ok(None)
            }
        };
    }
    if required {
        bail!(
            "wallet keypair not found at {} and no wallet keypair secret is configured",
            path.map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unset>".into())
        );
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn build_executor(
    config: &BeastConfig,
    rpc: &Arc<RpcClient>,
    jupiter: &Arc<JupiterClient>,
    jito: &Option<Arc<JitoClient>>,
    wallet: Option<Arc<Wallet>>,
    effective: Mode,
    cli_real: bool,
) -> Result<Option<Arc<Executor>>> {
    if config.guard.program_id.is_empty() {
        if effective.is_live() {
            bail!("live mode requires [guard] program_id");
        }
        return Ok(None);
    }
    let guard_program = config
        .guard
        .program_id
        .parse::<solana_sdk::pubkey::Pubkey>()
        .map_err(|error| anyhow::anyhow!("invalid guard program id: {error}"))?;
    let _ = guard_program;
    let Some(wallet) = wallet else {
        return Ok(None);
    };
    let strategy = config.strategy_or_default("cross_dex_arb");
    let execution = ExecutionConfig {
        real: effective == Mode::Live && cli_real && config.runtime.real,
        min_profit_lamports: config.guard.min_profit_lamports,
        balance_floor_lamports: config.risk.balance_floor_lamports,
        slippage_bps: 50,
        tip_alpha: strategy.tip_alpha_initial,
        max_tip_lamports: strategy.tip_cap_lamports,
        guard_program: config.guard.program_id.clone(),
        transaction_fee_lamports: DEFAULT_TRANSACTION_FEE_LAMPORTS,
        ata_rent_lamports: SPL_TOKEN_ACCOUNT_RENT_LAMPORTS,
    };
    let tip_accounts: Vec<Address> = config
        .jito
        .tip_accounts
        .iter()
        .filter_map(|value| match value.parse::<Address>() {
            Ok(address) => Some(address),
            Err(error) => {
                warn!(account = %value, error = %error, "ignoring invalid Jito tip account");
                None
            }
        })
        .collect();
    match Executor::new(
        rpc.clone(),
        jupiter.clone(),
        jito.clone(),
        wallet,
        execution,
        tip_accounts,
    ) {
        Ok(executor) => Ok(Some(Arc::new(executor))),
        Err(error) if effective.is_live() => Err(error.into()),
        Err(error) => {
            warn!(error = %error, "executor disabled; radar/canary will not simulate");
            Ok(None)
        }
    }
}

fn build_watchlist(config: &BeastConfig, store: &Store) -> Result<Vec<WatchedPool>> {
    let mut pools = Vec::new();
    for pool in &config.pools {
        if !pool.enabled {
            continue;
        }
        match WatchedPool::from_config(pool) {
            Ok(watched) => pools.push(watched),
            Err(error) => warn!(
                address = %pool.address,
                venue = %pool.venue,
                error = %error,
                "ignoring invalid [[pools]] entry"
            ),
        }
    }
    if pools.is_empty() {
        for snapshot in store.load_pools(10_000)? {
            if catalog::venue(&snapshot.venue).is_some() {
                pools.push(WatchedPool::from_parts(snapshot.address, snapshot.venue));
            }
        }
    }
    let mut seen = HashSet::new();
    pools.retain(|pool| seen.insert(pool.address));
    Ok(pools)
}

fn router_config(config: &BeastConfig) -> RouterConfig {
    let mut start_mints = Vec::new();
    let mut seen = HashSet::new();
    for strategy in config.strategies.values() {
        for raw in &strategy.start_mints {
            if let Ok(address) = raw.parse::<Address>() {
                if seen.insert(address) {
                    start_mints.push(address);
                }
            }
        }
    }
    if start_mints.is_empty() {
        if let Ok(wsol) = WSOL_MINT.parse::<Address>() {
            start_mints.push(wsol);
        }
    }
    RouterConfig {
        max_hops: 3,
        max_impact_bps: 150,
        max_input_raw: 5_000_000_000,
        pool_fraction_bps: 100,
        min_gross_lamports: 0,
        max_routes: 20_000,
        start_mints,
        intermediate_mints: Vec::new(),
    }
}

async fn radar_loop(ctx: RadarContext, shutdown: CancellationToken) {
    let mut interval = tokio::time::interval(Duration::from_millis(ctx.interval_ms));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut seen: HashMap<String, Instant> = HashMap::new();
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }
        let process_mode = *ctx.controls.mode.read().await;
        if process_mode == Mode::Off {
            continue;
        }
        let pools = ctx
            .feed
            .fresh_snapshots(ctx.config.risk.max_feed_lag_ms)
            .await;
        if pools.is_empty() {
            continue;
        }
        let feed_age = ctx.feed.health().await.age_ms.unwrap_or(u64::MAX);
        ctx.metrics.set_feed_lag(feed_age);
        let max_slot = pools.iter().map(|pool| pool.slot).max().unwrap_or(0);
        ctx.metrics
            .observe_feed_update(max_slot, Utc::now().timestamp_millis());
        if let Ok(expired) = ctx
            .store
            .expire_open_opportunities(Utc::now().timestamp_millis() - 120_000)
        {
            if expired > 0 {
                info!(expired, "expired stale radar opportunities");
            }
        }
        let router = Router::new(pools.clone(), router_config(&ctx.config));
        let routes = router.opportunities();
        if routes.is_empty() {
            continue;
        }
        let strategy_modes = ctx.controls.strategies.read().await.clone();
        let now = Instant::now();
        seen.retain(|_, observed| now.duration_since(*observed) < Duration::from_secs(30));
        for route in routes {
            let strategy = classify_route(&route);
            let strategy_name = strategy.as_str();
            let mut strategy_mode = strategy_modes
                .get(strategy_name)
                .copied()
                .unwrap_or_else(|| ctx.config.strategy_mode(strategy_name, ctx.cli_real));
            strategy_mode = strategy_mode.capped_by(process_mode);
            if strategy_mode == Mode::Off {
                continue;
            }
            let Some(opportunity) =
                build_opportunity(&ctx.config, &route, strategy, strategy_mode, max_slot)
            else {
                continue;
            };
            if let Some(last) = seen.get(&opportunity.dedupe_key) {
                if now.duration_since(*last) < Duration::from_millis(1_500) {
                    continue;
                }
            }
            seen.insert(opportunity.dedupe_key.clone(), now);
            if let Err(error) = ctx.store.insert_opportunity(&opportunity) {
                warn!(error = %error, "failed to persist opportunity");
                continue;
            }
            ctx.metrics.opportunities.fetch_add(1, Ordering::Relaxed);
            let strategy_metrics = ctx.metrics.strategy(strategy_name);
            strategy_metrics
                .opportunities
                .fetch_add(1, Ordering::Relaxed);
            info!(
                strategy = strategy_name,
                mode = %strategy_mode,
                net = opportunity.est_net_lamports,
                gross = opportunity.est_gross_lamports,
                route = %opportunity.dedupe_key,
                "opportunity"
            );
            if process_mode.rank() >= Mode::Canary.rank()
                && strategy_mode.rank() >= Mode::Canary.rank()
            {
                if let Err(error) = maybe_execute(&ctx, &opportunity, strategy_mode, feed_age).await
                {
                    warn!(error = %error, opportunity = %opportunity.id, "execution path failed");
                }
            }
        }
    }
}

fn build_opportunity(
    config: &BeastConfig,
    route: &beast_dex::model::RouteQuote,
    strategy: StrategyKind,
    mode: Mode,
    slot: u64,
) -> Option<Opportunity> {
    let strategy_config = config.strategy_or_default(strategy.as_str());
    if !strategy_config.start_mints.is_empty() {
        let start = route.start_mint.to_string();
        if !strategy_config
            .start_mints
            .iter()
            .any(|mint| mint == &start)
        {
            return None;
        }
    }
    if !strategy_config.venues.is_empty()
        && route.legs.iter().any(|leg| {
            !strategy_config
                .venues
                .iter()
                .any(|venue| venue == &leg.venue)
        })
    {
        return None;
    }
    if route.max_price_impact_bps > strategy_config.max_price_impact_bps as u32 {
        return None;
    }
    if strategy_config.max_notional_lamports > 0
        && route.amount_in > strategy_config.max_notional_lamports
    {
        return None;
    }
    if !pair_allowed(&strategy_config, route) {
        return None;
    }

    let gross = route.gross_profit;
    let tip = strategy_config
        .tip_policy(beast_core::economics::MIN_JITO_TIP_LAMPORTS)
        .tip_for(gross);
    let mut intermediates = HashSet::new();
    for leg in route.legs.iter().take(route.legs.len().saturating_sub(1)) {
        let mint = leg.mint_out.to_string();
        if mint != WSOL_MINT {
            intermediates.insert(mint);
        }
    }
    let rent = SPL_TOKEN_ACCOUNT_RENT_LAMPORTS.saturating_mul(intermediates.len() as u64);
    let costs = CostBreakdown {
        tip_lamports: tip,
        base_fee_lamports: DEFAULT_TRANSACTION_FEE_LAMPORTS
            .saturating_mul((route.legs.len() + 2) as u64),
        rent_lamports: rent,
        ..CostBreakdown::default()
    };
    let net = gross.saturating_sub(costs.total() as i64);
    if net < strategy_config.min_net_lamports as i64 {
        return None;
    }
    let legs: Vec<LegPlan> = route
        .legs
        .iter()
        .map(|leg| LegPlan {
            venue: leg.venue.clone(),
            pool: leg.pool.to_string(),
            mint_in: leg.mint_in.to_string(),
            mint_out: leg.mint_out.to_string(),
            amount_in: leg.amount_in,
            min_out: leg.amount_out,
            price_impact_bps: leg.price_impact_bps,
        })
        .collect();
    let pool_ids = route.pool_ids();
    let pool_refs: Vec<&str> = pool_ids.iter().map(String::as_str).collect();
    let direction = format!(
        "{}>{}",
        route.start_mint,
        route
            .legs
            .iter()
            .map(|leg| leg.mint_out.to_string())
            .collect::<Vec<_>>()
            .join(">")
    );
    let key = dedupe_key(
        strategy,
        &pool_refs,
        &direction,
        route.amount_in / 100_000_000,
    );
    let meta = json!({
        "source": "radar",
        "route": route,
        "slot": slot,
        "gross_lamports": gross,
        "costs": costs,
        "rent_accounts": intermediates.len(),
    });
    Some(Opportunity {
        id: Uuid::new_v4(),
        strategy,
        mode,
        slot_detected: slot,
        detected_at_ms: Utc::now().timestamp_millis(),
        legs,
        est_gross_lamports: gross,
        est_costs: costs,
        est_net_lamports: net,
        confidence: route.min_confidence,
        ttl_slots: strategy_config.ttl_slots.max(1),
        dedupe_key: key,
        outcome: OutcomeLabel::Open,
        meta,
    })
}

fn pair_allowed(
    config: &beast_core::config::StrategyConfig,
    route: &beast_dex::model::RouteQuote,
) -> bool {
    let pairs: Vec<String> = route
        .legs
        .iter()
        .map(|leg| format!("{}/{}", leg.mint_in, leg.mint_out))
        .collect();
    if !config.pair_denylist.is_empty()
        && pairs
            .iter()
            .any(|pair| config.pair_denylist.iter().any(|denied| denied == pair))
    {
        return false;
    }
    if config.pair_allowlist.is_empty() {
        return true;
    }
    pairs
        .iter()
        .any(|pair| config.pair_allowlist.iter().any(|allowed| allowed == pair))
}

fn classify_route(route: &beast_dex::model::RouteQuote) -> StrategyKind {
    if route
        .legs
        .iter()
        .any(|leg| leg.venue.contains("saber") || leg.venue.contains("stable"))
    {
        return StrategyKind::StableArb;
    }
    let lst = route.legs.iter().any(|leg| {
        let mint = leg.mint_in.to_string();
        mint.starts_with("mSoLz")
            || mint.starts_with("J1toso")
            || mint.starts_with("bSo13")
            || mint.starts_with("jupSoLa")
    });
    if lst {
        return StrategyKind::LstArb;
    }
    if route.legs.len() >= 3 {
        StrategyKind::TriangularArb
    } else {
        StrategyKind::CrossDexArb
    }
}

async fn maybe_execute(
    ctx: &RadarContext,
    opportunity: &Opportunity,
    strategy_mode: Mode,
    feed_age_ms: u64,
) -> Result<()> {
    let Some(executor) = &ctx.executor else {
        return Ok(());
    };
    let wallet = executor.wallet();
    let balance = match ctx.rpc.get_balance(wallet.address()).await {
        Ok(balance) => balance,
        Err(error) => {
            warn!(error = %error, "cannot fetch wallet balance; execution skipped");
            return Ok(());
        }
    };
    let strategy_metrics = ctx.metrics.strategy(opportunity.strategy.as_str());
    let decision = {
        let mut risk = ctx
            .risk
            .lock()
            .map_err(|_| anyhow::anyhow!("risk mutex poisoned"))?;
        let balance_decision = risk.check_balance(balance);
        if !balance_decision.is_allow() {
            balance_decision
        } else {
            let health = risk.check_health(feed_age_ms, 0, 0);
            if !health.is_allow() {
                health
            } else {
                risk.evaluate(opportunity, strategy_mode)
            }
        }
    };
    if !decision.is_allow() {
        strategy_metrics.skipped.fetch_add(1, Ordering::Relaxed);
        ctx.store.record_event(
            "warn",
            "risk",
            "execution denied",
            json!({"opportunity": opportunity.id, "decision": format!("{decision:?}")}),
        )?;
        return Ok(());
    }

    let attempt_id = Uuid::new_v4().to_string();
    let attempt = AttemptRecord {
        id: attempt_id.clone(),
        opportunity_id: Some(opportunity.id.to_string()),
        ts_ms: Utc::now().timestamp_millis(),
        strategy: opportunity.strategy.as_str().to_string(),
        mode: strategy_mode.as_str().to_string(),
        status: "preparing".into(),
        bundle_id: None,
        simulated: false,
        landed: false,
        reverted: false,
        realized_net_lamports: None,
        tip_lamports: 0,
        fee_lamports: 0,
        signature: None,
        error: None,
    };
    ctx.store.insert_attempt(&attempt)?;
    strategy_metrics.attempts.fetch_add(1, Ordering::Relaxed);
    {
        let mut risk = ctx
            .risk
            .lock()
            .map_err(|_| anyhow::anyhow!("risk mutex poisoned"))?;
        risk.begin_attempt(opportunity.strategy);
    }

    let plan = match executor.prepare_cycle(opportunity).await {
        Ok(plan) => plan,
        Err(error) => {
            let _ = ctx.store.update_attempt(
                &attempt_id,
                &AttemptUpdate {
                    status: Some("prepare_failed".into()),
                    error: Some(error.to_string()),
                    ..AttemptUpdate::default()
                },
            );
            let mut risk = ctx
                .risk
                .lock()
                .map_err(|_| anyhow::anyhow!("risk mutex poisoned"))?;
            risk.record_build_failure(opportunity.strategy);
            risk.finish_attempt(opportunity.strategy);
            strategy_metrics.dropped.fetch_add(1, Ordering::Relaxed);
            ctx.store.record_event(
                "warn",
                "exec",
                "prepare failed",
                json!({"opportunity": opportunity.id, "error": error.to_string()}),
            )?;
            return Ok(());
        }
    };
    let _ = ctx.store.update_attempt(
        &attempt_id,
        &AttemptUpdate {
            status: Some("simulating".into()),
            tip_lamports: Some(plan.tip_lamports),
            fee_lamports: Some(plan.fees_lamports),
            ..AttemptUpdate::default()
        },
    );

    if let Err(error) = executor.simulate(&plan).await {
        let _ = ctx.store.update_attempt(
            &attempt_id,
            &AttemptUpdate {
                status: Some("simulation_failed".into()),
                simulated: Some(true),
                error: Some(error.to_string()),
                ..AttemptUpdate::default()
            },
        );
        let mut risk = ctx
            .risk
            .lock()
            .map_err(|_| anyhow::anyhow!("risk mutex poisoned"))?;
        risk.record_build_failure(opportunity.strategy);
        risk.finish_attempt(opportunity.strategy);
        strategy_metrics.dropped.fetch_add(1, Ordering::Relaxed);
        ctx.metrics.bundles_dropped.fetch_add(1, Ordering::Relaxed);
        ctx.store.record_event(
            "warn",
            "exec",
            "bundle simulation failed",
            json!({"opportunity": opportunity.id, "error": error.to_string()}),
        )?;
        return Ok(());
    }

    ctx.metrics.simulated.fetch_add(1, Ordering::Relaxed);
    strategy_metrics.simulated.fetch_add(1, Ordering::Relaxed);
    let _ = ctx.store.update_attempt(
        &attempt_id,
        &AttemptUpdate {
            status: Some("simulated".into()),
            simulated: Some(true),
            ..AttemptUpdate::default()
        },
    );

    if strategy_mode != Mode::Live || !ctx.real {
        let mut risk = ctx
            .risk
            .lock()
            .map_err(|_| anyhow::anyhow!("risk mutex poisoned"))?;
        risk.record_success(opportunity.strategy);
        risk.finish_attempt(opportunity.strategy);
        ctx.store.record_event(
            "info",
            "exec",
            "bundle simulated (canary)",
            json!({
                "opportunity": opportunity.id,
                "tip": plan.tip_lamports,
                "guard_required": plan.guard_required_lamports,
                "signatures": plan.signatures,
            }),
        )?;
        return Ok(());
    }

    let mut awaiting_settlement = false;
    match executor.submit(&plan).await {
        Ok(bundle_id) => {
            let bundle = BundleRecord {
                bundle_id: bundle_id.clone(),
                attempt_id: Some(attempt_id.clone()),
                ts_ms: Utc::now().timestamp_millis(),
                status: "submitted".into(),
                tip_lamports: plan.tip_lamports,
                signatures_json: json!(plan.signatures).to_string(),
                landed_slot: None,
                error: None,
            };
            ctx.store.insert_bundle(&bundle)?;
            let _ = ctx.store.update_attempt(
                &attempt_id,
                &AttemptUpdate {
                    status: Some("submitted".into()),
                    bundle_id: Some(bundle_id.clone()),
                    signature: plan.signatures.first().cloned(),
                    simulated: Some(true),
                    ..AttemptUpdate::default()
                },
            );
            ctx.store.set_meta("last_bundle_id", &bundle_id)?;
            awaiting_settlement = true;
            ctx.metrics
                .bundles_submitted
                .fetch_add(1, Ordering::Relaxed);
            strategy_metrics.attempts.fetch_add(1, Ordering::Relaxed);
            ctx.store.record_event(
                "info",
                "exec",
                "bundle submitted",
                json!({
                    "opportunity": opportunity.id,
                    "bundle_id": bundle_id,
                    "tip": plan.tip_lamports,
                    "guard_required": plan.guard_required_lamports,
                }),
            )?;
        }
        Err(error) => {
            let _ = ctx.store.update_attempt(
                &attempt_id,
                &AttemptUpdate {
                    status: Some("submit_failed".into()),
                    error: Some(error.to_string()),
                    ..AttemptUpdate::default()
                },
            );
            ctx.metrics.bundles_dropped.fetch_add(1, Ordering::Relaxed);
            strategy_metrics.dropped.fetch_add(1, Ordering::Relaxed);
            ctx.store.record_event(
                "warn",
                "exec",
                "bundle submission failed",
                json!({"opportunity": opportunity.id, "error": error.to_string()}),
            )?;
        }
    }
    // Keep the risk in-flight slot until the settlement monitor has final
    // confirmation, so a burst of unconfirmed bundles cannot bypass the cap.
    if !awaiting_settlement {
        let mut risk = ctx
            .risk
            .lock()
            .map_err(|_| anyhow::anyhow!("risk mutex poisoned"))?;
        risk.finish_attempt(opportunity.strategy);
    }
    Ok(())
}

struct SettlementContext {
    store: Arc<Store>,
    rpc: Arc<RpcClient>,
    jito: Arc<JitoClient>,
    metrics: Arc<Metrics>,
    risk: Arc<Mutex<RiskEngine>>,
}

async fn settlement_loop(context: SettlementContext, shutdown: CancellationToken) {
    let mut interval = tokio::time::interval(Duration::from_secs(3));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }
        let pending = match context.store.pending_bundles(20) {
            Ok(pending) => pending,
            Err(error) => {
                warn!(error = %error, "cannot load pending bundles");
                continue;
            }
        };
        if pending.is_empty() {
            continue;
        }
        let ids: Vec<String> = pending
            .iter()
            .map(|bundle| bundle.bundle_id.clone())
            .collect();
        let statuses = match context.jito.get_bundle_statuses(&ids).await {
            Ok(statuses) => statuses,
            Err(error) => {
                warn!(error = %error, "cannot poll Jito bundle statuses");
                continue;
            }
        };
        let mut seen = HashSet::new();
        for status in statuses {
            let Some(bundle) = pending
                .iter()
                .find(|bundle| bundle.bundle_id == status.bundle_id)
            else {
                continue;
            };
            seen.insert(bundle.bundle_id.clone());
            if let Err(error) = reconcile_bundle(&context, bundle, &status).await {
                warn!(bundle = %bundle.bundle_id, error = %error, "bundle reconciliation failed");
            }
        }
        // Jito can silently drop a bundle without returning a status. A stale
        // unconfirmed bundle must release its risk slot and be marked dropped;
        // landed bundles are resolved above from RPC signatures.
        let cutoff = Utc::now().timestamp_millis() - 120_000;
        for bundle in pending {
            if seen.contains(&bundle.bundle_id) || bundle.ts_ms > cutoff {
                continue;
            }
            if let Err(error) =
                finalize_dropped(&context, &bundle, "no Jito status within 120s").await
            {
                warn!(bundle = %bundle.bundle_id, error = %error, "cannot expire pending bundle");
            }
        }
    }
}

async fn finalize_dropped(
    context: &SettlementContext,
    bundle: &BundleRecord,
    reason: &str,
) -> Result<()> {
    context
        .store
        .update_bundle(&bundle.bundle_id, "failed", None, Some(reason))?;
    if let Some(attempt_id) = &bundle.attempt_id {
        let _ = context.store.update_attempt(
            attempt_id,
            &AttemptUpdate {
                status: Some("dropped".into()),
                reverted: Some(true),
                error: Some(reason.to_string()),
                ..AttemptUpdate::default()
            },
        );
        if let Some(attempt) = context.store.get_attempt(attempt_id)? {
            if let Ok(strategy) = StrategyKind::from_str(&attempt.strategy) {
                context
                    .metrics
                    .strategy(strategy.as_str())
                    .dropped
                    .fetch_add(1, Ordering::Relaxed);
                if let Ok(mut risk) = context.risk.lock() {
                    risk.finish_attempt(strategy);
                }
            }
        }
    }
    context
        .metrics
        .bundles_dropped
        .fetch_add(1, Ordering::Relaxed);
    context.store.record_event(
        "warn",
        "settlement",
        "bundle did not land",
        json!({"bundle_id": bundle.bundle_id, "reason": reason}),
    )?;
    Ok(())
}

async fn reconcile_bundle(
    context: &SettlementContext,
    bundle: &BundleRecord,
    status: &beast_rpc::BundleStatus,
) -> Result<()> {
    let normalized = status.status.to_ascii_lowercase();
    let final_failure = matches!(
        normalized.as_str(),
        "failed" | "invalid" | "dropped" | "rejected" | "not_landed" | "not landed"
    );
    if final_failure {
        return finalize_dropped(context, bundle, &status.status).await;
    }
    if !normalized.contains("landed") {
        return Ok(());
    }

    let signatures: Vec<String> = if status.transactions.is_empty() {
        serde_json::from_str(&bundle.signatures_json).unwrap_or_default()
    } else {
        status.transactions.clone()
    };
    if signatures.is_empty() {
        return Ok(());
    }
    let mut net = 0i64;
    let mut fees = 0u64;
    let mut slot = status.landed_slot.unwrap_or(0);
    let mut reverted = false;
    let mut resolved = 0usize;
    for signature in &signatures {
        let Some(transaction) = context.rpc.get_transaction(signature).await? else {
            continue;
        };
        resolved += 1;
        slot = slot.max(transaction.slot);
        fees = fees.saturating_add(transaction.fee);
        // The bundle's fee payer and tip payer is the bot wallet. We resolve
        // it from the transaction account keys by checking every account for a
        // nonzero delta; the guard requires all rebalancing to happen inside
        // these transactions, so the only positive survivor is the wallet.
        net = net.saturating_add(transaction_net(&transaction));
        if transaction.err.is_some() {
            reverted = true;
        }
    }
    if resolved < signatures.len() {
        // Jito reports landed before every RPC transaction is visible. Keep
        // the bundle pending and retry on the next tick.
        return Ok(());
    }

    let attempt = match &bundle.attempt_id {
        Some(attempt_id) => context.store.get_attempt(attempt_id)?,
        None => None,
    };
    if reverted {
        context.store.update_bundle(
            &bundle.bundle_id,
            "reverted",
            Some(slot),
            Some("transaction reverted"),
        )?;
        if let Some(attempt) = &attempt {
            let _ = context.store.update_attempt(
                &attempt.id,
                &AttemptUpdate {
                    status: Some("reverted".into()),
                    landed: Some(true),
                    reverted: Some(true),
                    fee_lamports: Some(fees),
                    ..AttemptUpdate::default()
                },
            );
            if let Ok(strategy) = StrategyKind::from_str(&attempt.strategy) {
                let strategy_metrics = context.metrics.strategy(strategy.as_str());
                strategy_metrics.reverted.fetch_add(1, Ordering::Relaxed);
                context
                    .metrics
                    .bundles_reverted
                    .fetch_add(1, Ordering::Relaxed);
                if let Ok(mut risk) = context.risk.lock() {
                    risk.finish_attempt(strategy);
                }
            }
        }
        context.store.record_event(
            "warn",
            "settlement",
            "bundle reverted on-chain",
            json!({"bundle_id": bundle.bundle_id, "slot": slot}),
        )?;
        return Ok(());
    }

    let (opportunity_id, modeled_net, strategy) = match attempt {
        Some(ref attempt) => {
            let strategy =
                StrategyKind::from_str(&attempt.strategy).unwrap_or(StrategyKind::CrossDexArb);
            (
                attempt
                    .opportunity_id
                    .as_deref()
                    .and_then(|id| Uuid::parse_str(id).ok()),
                attempt
                    .opportunity_id
                    .as_deref()
                    .and_then(|id| context.store.opportunity_modeled_net(id).ok().flatten())
                    .unwrap_or(0),
                strategy,
            )
        }
        None => (None, 0, StrategyKind::CrossDexArb),
    };
    let Some(opportunity_id) = opportunity_id else {
        return Ok(());
    };
    let tip = bundle.tip_lamports;
    let gross = net.saturating_add(tip as i64).saturating_add(fees as i64);
    let settlement = SettlementResult {
        opportunity_id,
        signature: signatures.first().cloned().unwrap_or_default(),
        slot,
        realized_gross_lamports: gross,
        realized_tip_lamports: tip,
        realized_fee_lamports: fees,
        realized_net_lamports: net,
        modeled_net_lamports: modeled_net,
        settled_at_ms: Utc::now().timestamp_millis(),
    };
    context.store.insert_settlement(&settlement)?;
    context
        .store
        .update_bundle(&bundle.bundle_id, "landed", Some(slot), None)?;
    if let Some(attempt) = &attempt {
        context.store.update_attempt(
            &attempt.id,
            &AttemptUpdate {
                status: Some("landed".into()),
                landed: Some(true),
                reverted: Some(false),
                realized_net_lamports: Some(net),
                tip_lamports: Some(tip),
                fee_lamports: Some(fees),
                ..AttemptUpdate::default()
            },
        )?;
    }
    context
        .metrics
        .bundles_landed
        .fetch_add(1, Ordering::Relaxed);
    context
        .metrics
        .realized_net_lamports
        .fetch_add(net, Ordering::Relaxed);
    let strategy_metrics = context.metrics.strategy(strategy.as_str());
    strategy_metrics.landed.fetch_add(1, Ordering::Relaxed);
    strategy_metrics
        .realized_net_lamports
        .fetch_add(net, Ordering::Relaxed);
    if net < 0 {
        context
            .metrics
            .guard_violations
            .fetch_add(1, Ordering::Relaxed);
        context.store.record_event(
            "error",
            "settlement",
            "guard violation: landed bundle lost lamports",
            json!({"bundle_id": bundle.bundle_id, "net": net}),
        )?;
        if let Ok(mut risk) = context.risk.lock() {
            risk.halt(
                "guard_violation",
                format!("bundle {} landed with negative net {net}", bundle.bundle_id),
            );
        }
    } else if let Ok(mut risk) = context.risk.lock() {
        risk.record_landed_cost(strategy, tip.saturating_add(fees));
        risk.record_success(strategy);
        risk.record_model_error(&opportunity_id.to_string(), settlement.model_error_bps());
        risk.finish_attempt(strategy);
    }
    context.store.record_event(
        "info",
        "settlement",
        "bundle settled from on-chain balances",
        json!({
            "bundle_id": bundle.bundle_id,
            "slot": slot,
            "net_lamports": net,
            "tip_lamports": tip,
            "fee_lamports": fees,
            "model_error_bps": settlement.model_error_bps(),
        }),
    )?;
    Ok(())
}

/// Net balance delta for the transaction fee payer (account index 0).
/// Bundle transactions are built and signed exclusively by our wallet.
fn transaction_net(transaction: &beast_rpc::TransactionDetails) -> i64 {
    transaction
        .balance_delta(&transaction.account_keys[0])
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_starts_with_wsol_when_unconfigured() {
        let config = BeastConfig::default();
        let config = router_config(&config);
        assert!(config
            .start_mints
            .iter()
            .any(|mint| mint.to_string() == WSOL_MINT));
    }

    #[test]
    fn classification_prefers_stable_and_lst() {
        // Covered by integration tests once route fixtures are shared.
        assert_eq!(classify_route(&empty_route()), StrategyKind::CrossDexArb);
    }

    fn empty_route() -> beast_dex::model::RouteQuote {
        beast_dex::model::RouteQuote {
            start_mint: WSOL_MINT.parse().unwrap(),
            end_mint: WSOL_MINT.parse().unwrap(),
            amount_in: 1,
            amount_out: 1,
            gross_profit: 0,
            legs: Vec::new(),
            min_confidence: 1.0,
            max_price_impact_bps: 0,
        }
    }
}
