mod api;
mod buyer;
mod dev_fee;
mod error;
mod helius_sender;
mod idl;
mod models;
mod monitor;
mod rpc;
mod settings;
mod state;
mod tx_builder;
mod ws;
use crate::error::AppError;
use api::{create_router, ApiState, BotStats};
use ws::WsRequest;

// Global bot control for logging
static BOT_CONTROL: once_cell::sync::OnceCell<std::sync::Arc<api::BotControl>> =
    once_cell::sync::OnceCell::new();

// Helper macro to log to both console and API
macro_rules! bot_log {
    ($level:expr, $msg:expr) => {
        log::info!("[{}] {}", $level, $msg);
        if let Some(control) = BOT_CONTROL.get() {
            let control_clone = control.clone();
            let msg_string = $msg.to_string();
            let level_string = $level.to_string();
            tokio::spawn(async move {
                control_clone.add_log(&level_string, msg_string, None).await;
            });
        }
    };
    ($level:expr, $msg:expr, $detail:expr) => {
        log::info!("[{}] {} - {}", $level, $msg, $detail);
        if let Some(control) = BOT_CONTROL.get() {
            let control_clone = control.clone();
            let msg_string = $msg.to_string();
            let level_string = $level.to_string();
            let detail_string = Some($detail.to_string());
            tokio::spawn(async move {
                control_clone
                    .add_log(&level_string, msg_string, detail_string)
                    .await;
            });
        }
    };
}

use once_cell::sync::Lazy;
use std::str::FromStr;

// Debounce for repetitive "max held coins" logs so we don't spam the logs.
static LAST_MAX_HELD_LOG: Lazy<tokio::sync::Mutex<Option<Instant>>> =
    Lazy::new(|| tokio::sync::Mutex::new(None));
const MAX_HELD_LOG_DEBOUNCE_SECS: u64 = 60;
const API_PORT: u16 = 8080;
const API_HOST: &str = "0.0.0.0";
use crate::{
    models::{Holding, PriceCache},
    settings::Settings,
    state::BuyRecord,
};
use chrono::Utc;
use log::{debug, error, info, warn};
use lru::LruCache;
use mpl_token_metadata::accounts::Metadata as OnchainMetadataRaw;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signer;
use std::sync::atomic::AtomicUsize;
use std::{collections::HashMap, fs, sync::Arc, time::Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::time::Duration;

/// Select the healthiest WSS endpoint with available slots.
/// Returns None if all endpoints are degraded or unavailable.
async fn select_healthy_wss(
    ws_control_senders: &Arc<Vec<mpsc::Sender<WsRequest>>>,
    settings: &Settings,
) -> Option<usize> {
    if ws_control_senders.is_empty() {
        return None;
    }

    let mut health_scores: Vec<(usize, i32)> = Vec::new();

    for (idx, sender) in ws_control_senders.iter().enumerate() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if sender.send(WsRequest::GetHealth { resp: tx }).await.is_ok() {
            if let Ok(Ok(h)) = tokio::time::timeout(Duration::from_millis(100), rx).await {
                let mut score = 100;
                score -= (h.recent_timeouts as i32) * 30; // Heavy penalty for timeouts
                if h.active_subs >= settings.max_subs_per_wss {
                    score -= 1000; // Reject full endpoints
                }
                score -= (h.pending_subs as i32) * 15; // Penalty for pending work
                if h.is_healthy {
                    score += 50;
                }
                health_scores.push((idx, score));
            }
        }
    }

    health_scores.sort_by(|a, b| b.1.cmp(&a.1));
    health_scores
        .first()
        .filter(|(_, score)| *score > 0)
        .map(|(idx, _)| *idx)
}

#[tokio::main(worker_threads = 4)]
async fn main() -> Result<(), AppError> {
    env_logger::init();
    // Print an unconditional startup line so users see the binary started
    // even when RUST_LOG is not set (typo like RUST_LOGS will otherwise be silent).
    println!(
        "sol_beast starting (pid {}), RUST_LOG={:?}",
        std::process::id(),
        std::env::var("RUST_LOG").ok()
    );
    
    let config_path = std::env::var("SOL_BEAST_CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    let settings = Arc::new(Settings::from_file(&config_path)?);
    settings.validate()?;
    
    let rpc_client = Arc::new(RpcClient::new(settings.solana_rpc_urls[0].clone()));
    // Touch these settings here so they are used by the binary (avoid warnings)
    let price_source_cfg = settings.price_source.clone();
    let rpc_rotate_secs_cfg = settings.rpc_rotate_interval_secs;
    info!(
        "Configured price_source={} rpc_rotate_interval_secs={}",
        price_source_cfg, rpc_rotate_secs_cfg
    );
    let seen = Arc::new(Mutex::new(LruCache::new(
        settings.cache_capacity.try_into()?,
    )));
    let holdings = Arc::new(Mutex::new(HashMap::new()));
    // Map to track buy metadata so we can write completed trades to CSV on sell
    let trades_map: Arc<Mutex<HashMap<String, BuyRecord>>> = Arc::new(Mutex::new(HashMap::new()));
    // API data structures for detected coins and trades
    let detected_coins = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let trades_list = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let price_cache = Arc::new(Mutex::new(LruCache::new(
        settings.cache_capacity.try_into()?,
    )));
    // Shared map of active subscriptions for mints we care about. Value is (wss_sender_index, sub_id)
    let sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>> = Arc::new(Mutex::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel(1000);
    let is_real = std::env::args().any(|arg| arg == "--real");
    // Load real keypair either from path or from JSON in config (optional)
    // Prefer base64 env var for keypairs to avoid storing keys on disk.
    let keypair: Option<std::sync::Arc<Keypair>> = if is_real {
        if let Some(bytes) = settings::load_keypair_from_env_var("SOL_BEAST_KEYPAIR_B64") {
            Some(std::sync::Arc::new(
                Keypair::try_from(bytes.as_slice())
                    .map_err(|e| AppError::InvalidKeypair(e.to_string()))?,
            ))
        } else if let Some(pk_string) = settings.wallet_private_key_string.clone() {
            let bytes =
                settings::parse_private_key_string(&pk_string).map_err(AppError::InvalidKeypair)?;
            Some(std::sync::Arc::new(
                Keypair::try_from(bytes.as_slice())
                    .map_err(|e| AppError::InvalidKeypair(e.to_string()))?,
            ))
        } else if let Some(j) = settings.wallet_keypair_json.clone() {
            let bytes: Vec<u8> = serde_json::from_str(&j)?;
            Some(std::sync::Arc::new(
                Keypair::try_from(bytes.as_slice())
                    .map_err(|e| AppError::InvalidKeypair(e.to_string()))?,
            ))
        } else if let Some(path) = settings.wallet_keypair_path.clone() {
            let bytes = fs::read(path)?;
            Some(std::sync::Arc::new(
                Keypair::try_from(bytes.as_slice())
                    .map_err(|e| AppError::InvalidKeypair(e.to_string()))?,
            ))
        } else {
            return Err(AppError::InvalidKeypair("No wallet keypair configured! Set wallet_keypair_path, wallet_private_key_string, wallet_keypair_json, or SOL_BEAST_KEYPAIR_B64 env var".to_string()));
        }
    } else {
        None
    };

    // Optional simulation keypair (used for dry-run signing). If not provided,
    // the code will fall back to generating an ephemeral Keypair at runtime.
    let simulate_keypair: Option<std::sync::Arc<Keypair>> = if let Some(bytes) =
        settings::load_keypair_from_env_var("SOL_BEAST_SIMULATE_KEYPAIR_B64")
    {
        Some(std::sync::Arc::new(
            Keypair::try_from(bytes.as_slice())
                .map_err(|e| AppError::InvalidKeypair(e.to_string()))?,
        ))
    } else if let Some(pk_string) = settings.simulate_wallet_private_key_string.clone() {
        let bytes =
            settings::parse_private_key_string(&pk_string).map_err(AppError::InvalidKeypair)?;
        let kp = Keypair::try_from(bytes.as_slice())
            .map_err(|e| AppError::InvalidKeypair(e.to_string()))?;
        info!("Loaded simulate keypair, pubkey: {}", kp.pubkey());
        Some(std::sync::Arc::new(kp))
    } else if let Some(j) = settings.simulate_wallet_keypair_json.clone() {
        let bytes: Vec<u8> = serde_json::from_str(&j)?;
        Some(std::sync::Arc::new(
            Keypair::try_from(bytes.as_slice())
                .map_err(|e| AppError::InvalidKeypair(e.to_string()))?,
        ))
    } else {
        None
    };

    // Create bot control early so it can be used by all tasks
    let initial_mode = if is_real {
        api::BotMode::Real
    } else {
        api::BotMode::DryRun
    };
    let bot_control = Arc::new(api::BotControl::new_with_mode(initial_mode));

    // Set global bot control for logging across the application
    if BOT_CONTROL.set(bot_control.clone()).is_err() {
        return Err(AppError::Init(
            "Failed to set global bot control".to_string(),
        ));
    }

    // Spawn price monitoring
    let holdings_clone_monitor = holdings.clone();
    let price_cache_clone_monitor = price_cache.clone();
    let settings_clone_monitor = settings.clone();
    let keypair_clone_monitor = keypair.clone();
    let simulate_keypair_clone = simulate_keypair.clone();
    let trades_map_clone_monitor = trades_map.clone();

    // Spawn WSS tasks and keep control senders so we can request subscriptions
    let mut ws_control_senders: Vec<mpsc::Sender<WsRequest>> = Vec::new();
    let mut ws_handles = Vec::new(); // New vector to store JoinHandles
    for wss_url in settings.solana_ws_urls.iter() {
        let tx = tx.clone();
        let seen = seen.clone();
        let holdings_clone = holdings.clone();
        let price_cache_clone = price_cache.clone();
        let settings_clone = settings.clone();
        let wss_url = wss_url.clone();
        let (ctrl_tx, ctrl_rx) = mpsc::channel(256);
        ws_control_senders.push(ctrl_tx.clone());
        // Spawn a single task that owns the control receiver. `ws::run_ws` will
        // manage its own internal state and reconnect logic where appropriate.
        let handle = tokio::spawn(async move {
            if let Err(e) = ws::run_ws(
                &wss_url,
                tx.clone(),
                seen.clone(),
                holdings_clone.clone(),
                price_cache_clone.clone(),
                ctrl_rx,
                settings_clone.clone(),
            )
            .await
            {
                error!("WSS connection to {} failed: {}.", wss_url, e);
                bot_log!(
                    "error",
                    "WSS connection failed",
                    format!("WSS: {}, Error: {}", wss_url, e)
                );
            }
        });
        ws_handles.push(handle);
    }
    let ws_control_senders = Arc::new(ws_control_senders);
    // Round-robin index for WSS sender selection (true round-robin)
    let next_wss_sender = Arc::new(AtomicUsize::new(0usize));

    // Now spawn price monitoring (after ws_control_senders exists so monitor
    // can unsubscribe subscriptions on sell).
    let rpc_client_clone = rpc_client.clone();
    let ws_control_senders_clone_for_monitor = ws_control_senders.clone();
    let sub_map_clone_for_monitor = sub_map.clone();
    let next_wss_sender_clone_for_monitor = next_wss_sender.clone();
    let simulate_keypair_clone_for_monitor = simulate_keypair_clone.clone();
    let trades_list_clone_for_monitor = trades_list.clone();
    let bot_control_for_monitor = bot_control.clone();
    
    let monitor_handle = tokio::spawn(async move {
        monitor::monitor_holdings(
            holdings_clone_monitor,
            price_cache_clone_monitor,
            rpc_client_clone,
            is_real,
            keypair_clone_monitor.as_deref(),
            simulate_keypair_clone_for_monitor.as_deref(),
            settings_clone_monitor,
            trades_map_clone_monitor,
            ws_control_senders_clone_for_monitor,
            sub_map_clone_for_monitor,
            next_wss_sender_clone_for_monitor,
            trades_list_clone_for_monitor,
            bot_control_for_monitor,
        )
        .await
    });

    // Start REST API server

    let api_stats = Arc::new(tokio::sync::Mutex::new(BotStats {
        total_buys: 0,
        total_sells: 0,
        total_profit: 0.0,
        current_holdings: vec![],
        uptime_secs: 0,
        last_activity: chrono::Utc::now().to_rfc3339(),
        running_state: Some("running".to_string()),
        mode: Some(if is_real { "real" } else { "dry-run" }.to_string()),
    }));

    let api_state = ApiState {
        settings: Arc::new(tokio::sync::Mutex::new(settings.as_ref().clone())),
        stats: api_stats.clone(),
        bot_control: bot_control.clone(),
        detected_coins: detected_coins.clone(),
        trades: trades_list.clone(),
    };

    // Add initial startup log
    bot_control
        .add_log(
            "info",
            format!(
                "Bot initialized in {} mode",
                if is_real { "real" } else { "dry-run" }
            ),
            Some(format!(
                "Wallet: {}",
                if is_real {
                    keypair
                        .as_ref()
                        .map(|k| k.pubkey().to_string())
                        .unwrap_or_else(|| "None".to_string())
                } else {
                    "Simulation mode".to_string()
                }
            )),
        )
        .await;

    // Set initial bot state to Running (since the bot starts immediately)
    {
        let mut state = bot_control.running_state.lock().await;
        *state = api::BotRunningState::Running;
    }

    // Set initial mode based on is_real flag
    {
        let mut mode = bot_control.mode.lock().await;
        *mode = if is_real {
            api::BotMode::Real
        } else {
            api::BotMode::DryRun
        };
    }

    // Spawn a task to periodically sync holdings to API stats
    let holdings_for_sync = holdings.clone();
    let api_stats_for_sync = api_stats.clone();
    let bot_control_for_sync = bot_control.clone();
    let trades_for_sync = trades_list.clone();
    let start_time = Instant::now();
    let api_sync_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            // Sync holdings to API stats
            let holdings_vec: Vec<api::HoldingWithMint> = {
                let holdings_map = holdings_for_sync.lock().await;
                holdings_map
                    .iter()
                    .map(|(mint, holding)| api::HoldingWithMint {
                        mint: mint.clone(),
                        holding: holding.clone(),
                    })
                    .collect()
            };

            // Calculate trade stats from trades list
            let (total_buys, total_sells, total_profit) = {
                let trades = trades_for_sync.lock().await;
                let mut buys = 0u64;
                let mut sells = 0u64;
                let mut profit = 0.0f64;
                
                for trade in trades.iter() {
                    if trade.trade_type == "buy" {
                        buys += 1;
                    } else if trade.trade_type == "sell" {
                        sells += 1;
                        // Add profit from sell trades
                        if let Some(pl) = trade.profit_loss {
                            profit += pl;
                        }
                    }
                }
                
                (buys, sells, profit)
            };

            let mut stats = api_stats_for_sync.lock().await;
            stats.current_holdings = holdings_vec;
            stats.total_buys = total_buys;
            stats.total_sells = total_sells;
            stats.total_profit = total_profit;
            stats.uptime_secs = start_time.elapsed().as_secs();
            stats.last_activity = chrono::Utc::now().to_rfc3339();

            // Log holdings count periodically (every 10 seconds)
            if stats.uptime_secs % 10 == 0 && !stats.current_holdings.is_empty() {
                bot_control_for_sync
                    .add_log(
                        "info",
                        format!(
                            "Currently holding {} positions",
                            stats.current_holdings.len()
                        ),
                        None,
                    )
                    .await;
            }
        }
    });

    let api_router = create_router(api_state);
    let api_server_handle = tokio::spawn(async move {
        let bind_addr = format!("{}:{}", API_HOST, API_PORT);
        let listener = tokio::net::TcpListener::bind(&bind_addr).await;
        match listener {
            Ok(l) => {
                info!("API server listening on {}", bind_addr);
                if let Err(e) = axum::serve(l, api_router).await {
                    error!("API server failed: {}", e);
                    bot_log!("error", "API server failed", format!("{}", e));
                }
            }
            Err(e) => {
                error!("Failed to bind API server to {}: {}", bind_addr, e);
                bot_log!("error", "Failed to bind API server", format!("{}", e));
            }
        }
    });

    // Process messages
    while let Some(msg) = rx.recv().await {
        if let Err(e) = process_message(
            &msg,
            &seen,
            &holdings,
            &rpc_client,
            is_real,
            keypair.as_deref(),
            simulate_keypair.as_deref(),
            &price_cache,
            &settings,
            ws_control_senders.clone(),
            next_wss_sender.clone(),
            trades_map.clone(),
            sub_map.clone(),
            detected_coins.clone(),
            trades_list.clone(),
        )
        .await
        {
            // Log the error and a truncated preview of the incoming message for debugging.
            let preview: String = msg.chars().take(200).collect();
            error!(
                "process_message failed for incoming message (truncated): {}... error: {}",
                preview, e
            );
            bot_log!(
                "error",
                "Failed to process WebSocket message",
                format!("{}", e)
            );
        }
    }

    // If the message processing loop ends, await all spawned tasks to ensure panics are caught.
    // This will block until all tasks complete or panic.
    info!("Main message processing loop ended. Awaiting background tasks...");

    let all_handles = vec![monitor_handle, api_sync_handle, api_server_handle];

    for handle in all_handles {
        if let Err(e) = handle.await {
            error!("A background task panicked or exited unexpectedly: {:?}", e);
            bot_log!("error", "Background task failed", format!("{:?}", e));
        }
    }

    for handle in ws_handles {
        if let Err(e) = handle.await {
            error!("A WSS task panicked or exited unexpectedly: {:?}", e);
            bot_log!("error", "WSS task failed", format!("{:?}", e));
        }
    }

    Ok(())
}

async fn process_message(
    text: &str,
    seen: &Arc<Mutex<LruCache<String, ()>>>,
    holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    rpc_client: &Arc<RpcClient>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    next_wss_sender: Arc<AtomicUsize>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
    detected_coins: Arc<tokio::sync::Mutex<Vec<api::DetectedCoin>>>,
    trades_list: Arc<tokio::sync::Mutex<Vec<api::TradeRecord>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // NOTE: don't short-circuit all incoming messages when max held coins is
    // reached — that would also skip websocket account notifications which we
    // rely on to update cached prices. Only skip handling of new token
    // detection (InitializeMint2) when at capacity. We'll perform a debounced
    // debug log where appropriate.
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            error!(
                "Failed to parse incoming websocket message as JSON: {}. message (truncated)={}",
                e,
                text.chars().take(200).collect::<String>()
            );
            return Err(Box::new(e));
        }
    };
    if let Some(params) = value
        .get("params")
        .and_then(|p| p.get("result"))
        .and_then(|r| r.get("value"))
    {
        let logs_opt = params.get("logs").and_then(|l| l.as_array());
        let sig_opt = params.get("signature").and_then(|s| s.as_str());
        if logs_opt.is_none() {
            debug!("Incoming message missing logs field: {:?}", params);
        }
        if sig_opt.is_none() {
            debug!("Incoming message missing signature field: {:?}", params);
        }

        if let (Some(logs), Some(signature)) = (logs_opt, sig_opt) {
            // Trigger if logs mention InitializeMint or the pump.fun program id
            let pump_prog_id = &settings.pump_fun_program;
            if logs.iter().any(|log| {
                // Accept common variants: InitializeMint2 and InitializeMint; match case-insensitively
                // Match both 'initializemint', 'initialize mint', or 'initializemint2' variations.
                log.as_str()
                    .map(|s| {
                        let s = s.to_lowercase();
                        s.contains("initializemint") || s.contains("initialize mint") || s.contains(pump_prog_id)
                    })
                    .unwrap_or(false)
            })
                && seen.lock().await.put(signature.to_string(), ()).is_none()
            {
                // If we're already at max holdings, skip detection work
                // but do not block processing of other websocket messages
                // (like account notifications). Debounce the debug log
                // so it doesn't spam the logs.
                if holdings.lock().await.len() >= settings.max_holded_coins {
                    let mut last_lock = LAST_MAX_HELD_LOG.lock().await;
                    let now = Instant::now();
                    let should_log = match *last_lock {
                        Some(ts) => now.duration_since(ts).as_secs() > MAX_HELD_LOG_DEBOUNCE_SECS,
                        None => true,
                    };
                    if should_log {
                        *last_lock = Some(now);
                        debug!(
                            "Max held coins reached ({}); skipping incoming message processing",
                            settings.max_holded_coins
                        );
                    }
                    // Do not attempt handle_new_token when at capacity.
                    return Ok(());
                }

                let detect_time = Utc::now();
                // Validate signature before attempting expensive RPC fetch to skip obvious invalid signatures.
                let signature_valid = solana_sdk::signature::Signature::from_str(signature).is_ok();
                if !signature_valid {
                    debug!("Skipping invalid signature for InitializeMint notification: {}", signature);
                    return Ok(());
                }
                if let Err(e) = handle_new_token(
                    signature,
                    holdings,
                    rpc_client,
                    is_real,
                    keypair,
                    simulate_keypair,
                    price_cache,
                    settings,
                    ws_control_senders.clone(),
                    next_wss_sender.clone(),
                    detect_time,
                    trades_map.clone(),
                    sub_map.clone(),
                    detected_coins.clone(),
                    trades_list.clone(),
                )
                .await
                {
                    error!("handle_new_token failed for {}: {}", signature, e);
                    return Err(e);
                }
            }
        }
    } else {
        debug!("Websocket message missing params/result/value: {:?}", value);
    }
    Ok(())
}

async fn handle_new_token(
    signature: &str,
    holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    rpc_client: &Arc<RpcClient>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    _next_wss_sender: Arc<AtomicUsize>,
    detect_time: chrono::DateTime<Utc>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
    detected_coins: Arc<tokio::sync::Mutex<Vec<api::DetectedCoin>>>,
    trades_list: Arc<tokio::sync::Mutex<Vec<api::TradeRecord>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::sync::oneshot;
    let (creator, mint, curve_pda, holder_addr, is_initialization) =
        rpc::fetch_transaction_details(signature, rpc_client, settings).await?;
    if !is_initialization {
        // Not a mint initialization transaction; skip detection.
        debug!("Transaction {} is not an InitializeMint; skipping detection", signature);
        return Ok(());
    }
    let (onchain_meta, offchain_meta, onchain_raw) =
        rpc::fetch_token_metadata(&mint, rpc_client, settings).await?;
    // Attempt to fetch the bonding curve creator so we can validate pump.fun token
    // and use the creator for additional verification, but do not require it for detection
    // because some RPCs may not have the PDA available immediately.
    let bonding_creator_opt = rpc::fetch_bonding_curve_creator(&mint, rpc_client, settings).await.ok().flatten();
    if bonding_creator_opt.is_none() {
        // Not fatal: the bonding curve PDA may not be indexed yet by the RPC node.
        // Log at debug and continue detection using transaction-parsed creator and curve.
        debug!("Bonding curve creator not found for mint {} (tx sig={}) — continuing detection using transaction parsed values", mint, signature);
    }

    // We treat lack of on-chain metadata as expected in some cases — continue
    // detection even when `onchain_meta` is None. Use offchain meta or onchain
    // raw to fill in display fields when available.
    // NOTE: we want to continue regardless of name and URI being present.
    if onchain_meta.is_some() || offchain_meta.is_some() || onchain_raw.is_some() {
        // Prepare display fields using available on-chain or off-chain metadata
        let token_name = offchain_meta
            .as_ref()
            .and_then(|off| off.name.clone())
            .or_else(|| onchain_meta.as_ref().map(|m| m.name.trim_end_matches('\u{0}').to_string()))
            .unwrap_or_else(|| "Unknown".to_string());
        let token_symbol = offchain_meta
            .as_ref()
            .and_then(|off| off.symbol.clone())
            .or_else(|| onchain_meta.as_ref().map(|m| m.symbol.trim_end_matches('\u{0}').to_string()));
        let metadata_uri_opt = if let Some(m) = onchain_meta.as_ref() {
            let uri = m.uri.trim_end_matches('\u{0}').to_string();
            if uri.is_empty() { None } else { Some(uri) }
        } else if let Some(off) = offchain_meta.as_ref() { off.image.clone() } else { None };

        // Log token detection using the available fields
        info!(
            "New pump.fun token detected: mint={} signature={} creator={} curve={} holder={} URI={}",
            mint,
            signature,
            creator,
            curve_pda,
            holder_addr,
            metadata_uri_opt.clone().unwrap_or_else(|| "<none>".to_string())
        );
        // (already logged above using `metadata_uri_opt`)

        // Log new token detection to API
        bot_log!(
            "info",
            format!("New token detected: {}", token_name),
            format!("Mint: {}, Creator: {}", mint, creator)
        );

        // Add to detected coins list. Use offchain metadata if present, else fallback to on-chain
        // parsed metadata; otherwise mark name as Unknown and symbol None.
        {
            let mut coins = detected_coins.lock().await;
            coins.insert(
                0,
                api::DetectedCoin {
                    mint: mint.clone(),
                    name: Some(token_name.clone()),
                    symbol: token_symbol.clone(),
                    image: offchain_meta.as_ref().and_then(|o| o.image.clone()),
                    creator: creator.clone(),
                    bonding_curve: curve_pda.clone(),
                    detected_at: detect_time.to_rfc3339(),
                    metadata_uri: metadata_uri_opt.clone(),
                    buy_price: None,
                    status: "detected".to_string(),
                },
            );
            // Keep only last 100 detected coins
            if coins.len() > 100 {
                coins.truncate(100);
            }
        }

        if let Some(off) = &offchain_meta {
            info!(
                "Off-chain metadata for {}: name={:?}, symbol={:?}, image={:?}",
                mint, off.name, off.symbol, off.image
            );
        }
        if let Some(m) = onchain_meta.as_ref() {
            if !m.uri.trim_end_matches('\u{0}').is_empty() && m.seller_fee_basis_points < 500 {
            // Try to get a fast WSS-provided price first depending on price_source.
            // If `price_source` == "rpc" we skip WSS and use RPC only.
            // If `price_source` == "wss" we attempt WSS and do NOT fall back to RPC.
            let price_source = settings.price_source.clone();
            let mut _used_wss = false;
            if price_source != "rpc" && !ws_control_senders.is_empty() {
                // Health-based WSS selection (avoid degraded/full endpoints)
                if let Some(idx) = select_healthy_wss(&ws_control_senders, settings).await {
                    let sender = &ws_control_senders[idx];
                    let (resp_tx, resp_rx) = oneshot::channel::<Result<u64, String>>();
                    // Subscribe to the bonding_curve PDA (streamed state includes virtual reserves)
                    let pump_prog =
                        solana_sdk::pubkey::Pubkey::from_str(&settings.pump_fun_program)?;
                    let mint_pk = solana_sdk::pubkey::Pubkey::from_str(&mint)?;
                    let (curve_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
                        &[b"bonding-curve", mint_pk.as_ref()],
                        &pump_prog,
                    );
                    let subscribe_req = WsRequest::Subscribe {
                        account: curve_pda.to_string(),
                        mint: mint.clone(),
                        resp: resp_tx,
                    };
                    if let Err(e) = sender.send(subscribe_req).await {
                        log::warn!("Failed to send subscribe request for {}: {}", mint, e);
                    } else {
                        // Use aggressive 5s timeout for fast failover
                        match tokio::time::timeout(std::time::Duration::from_secs(5), resp_rx).await
                        {
                            Ok(Ok(Ok(sub_id))) => {
                                _used_wss = true;
                                debug!("Subscribed to {} on sub {}", mint, sub_id);
                                // Attempt an immediate RPC fetch to prime the price cache.
                                // Some WSS providers delay the initial account notification;
                                // fetching the curve once via RPC gives us an initial price
                                // we can act on while the WSS subscription delivers updates.
                                let mut price_opt: Option<f64> = None;
                                match rpc::fetch_current_price(
                                    &mint,
                                    price_cache,
                                    rpc_client,
                                    settings,
                                )
                                .await
                                {
                                    Ok(p) => {
                                        debug!(
                                            "Primed price cache for {} via RPC: {:.18} SOL/token",
                                            mint, p
                                        );
                                        price_opt = Some(p);
                                    }
                                    Err(e) => {
                                        debug!("RPC prime failed for {}: {}. Will wait for WSS initial notification ({}s)", mint, e, settings.wss_subscribe_timeout_secs);
                                    }
                                }

                                // If RPC prime didn't succeed, wait briefly for the price to appear in the shared cache
                                // Track whether we want to keep the WSS subscription
                                // alive after the function returns. If we buy the token
                                // we keep the subscription active so the monitor gets
                                // live updates; otherwise we unsubscribe to free slots.
                                let mut keep_sub = false;
                                if price_opt.is_none() {
                                    let start = Instant::now();
                                    while start.elapsed().as_secs()
                                        < settings.wss_subscribe_timeout_secs
                                    {
                                        if let Some((_, price)) =
                                            price_cache.lock().await.get(&mint).cloned()
                                        {
                                            price_opt = Some(price);
                                            break;
                                        }
                                        tokio::time::sleep(std::time::Duration::from_millis(200))
                                            .await;
                                    }
                                }
                                if let Some(_price) = price_opt {
                                    let mut holdings_guard = holdings.lock().await;
                                    if holdings_guard.len() >= settings.max_holded_coins {
                                        info!(
                                            "Max held coins reached ({}); skipping buy for {}",
                                            settings.max_holded_coins, mint
                                        );
                                        return Ok(());
                                    }
                                    match buyer::buy_token(
                                        &mint,
                                        settings.buy_amount,
                                        is_real,
                                        keypair,
                                        simulate_keypair,
                                        price_cache.clone(),
                                        rpc_client,
                                        settings,
                                    )
                                    .await
                                    {
                                        Ok(mut holding) => {
                                            holding.metadata = offchain_meta.clone();
                                            holding.onchain_raw = onchain_raw.clone();
                                            // Build compact onchain_struct as before
                                            let mut onchain_struct: Option<
                                                crate::models::OnchainFullMetadata,
                                            > = None;
                                            if let Some(meta) = onchain_meta.as_ref() {
                                                let name =
                                                    meta.name.trim_end_matches('\u{0}').to_string();
                                                let symbol = meta
                                                    .symbol
                                                    .trim_end_matches('\u{0}')
                                                    .to_string();
                                                let uri =
                                                    meta.uri.trim_end_matches('\u{0}').to_string();
                                                onchain_struct =
                                                    Some(crate::models::OnchainFullMetadata {
                                                        name: if name.is_empty() {
                                                            None
                                                        } else {
                                                            Some(name)
                                                        },
                                                        symbol: if symbol.is_empty() {
                                                            None
                                                        } else {
                                                            Some(symbol)
                                                        },
                                                        uri: if uri.is_empty() {
                                                            None
                                                        } else {
                                                            Some(uri)
                                                        },
                                                        seller_fee_basis_points: Some(
                                                            meta.seller_fee_basis_points,
                                                        ),
                                                        raw: onchain_raw.clone(),
                                                    });
                                            } else if let Some(raw_bytes) = onchain_raw.as_ref() {
                                                if let Ok(parsed) =
                                                    OnchainMetadataRaw::safe_deserialize(raw_bytes)
                                                {
                                                    let name = parsed
                                                        .name
                                                        .trim_end_matches('\u{0}')
                                                        .to_string();
                                                    let symbol = parsed
                                                        .symbol
                                                        .trim_end_matches('\u{0}')
                                                        .to_string();
                                                    let uri = parsed
                                                        .uri
                                                        .trim_end_matches('\u{0}')
                                                        .to_string();
                                                    onchain_struct =
                                                        Some(crate::models::OnchainFullMetadata {
                                                            name: if name.is_empty() {
                                                                None
                                                            } else {
                                                                Some(name)
                                                            },
                                                            symbol: if symbol.is_empty() {
                                                                None
                                                            } else {
                                                                Some(symbol)
                                                            },
                                                            uri: if uri.is_empty() {
                                                                None
                                                            } else {
                                                                Some(uri)
                                                            },
                                                            seller_fee_basis_points: Some(
                                                                parsed.seller_fee_basis_points,
                                                            ),
                                                            raw: Some(raw_bytes.clone()),
                                                        });
                                                }
                                            }
                                            holding.onchain = onchain_struct.clone();
                                            if let Some(off) = &holding.metadata {
                                                info!("Persisting off-chain metadata for {} into holdings: name={:?}, symbol={:?}, image={:?}", mint, off.name, off.symbol, off.image);
                                            }
                                            if let Some(raw) = &holding.onchain_raw {
                                                info!("Persisting on-chain raw metadata for {} into holdings ({} bytes)", mint, raw.len());
                                            }
                                            if let Some(onchain) = &holding.onchain {
                                                info!("Persisting parsed on-chain metadata for {} into holdings: name={:?}, symbol={:?}, uri={:?}, seller_fee_basis_points={:?}", mint, onchain.name, onchain.symbol, onchain.uri, onchain.seller_fee_basis_points);
                                            }
                                            let buy_record = BuyRecord {
                                                mint: mint.clone(),
                                                symbol: offchain_meta
                                                    .as_ref()
                                                    .and_then(|o| o.symbol.clone()),
                                                name: offchain_meta
                                                    .as_ref()
                                                    .and_then(|o| o.name.clone()),
                                                uri: offchain_meta
                                                    .as_ref()
                                                    .and_then(|o| o.image.clone())
                                                    .or_else(|| {
                                                        onchain_struct
                                                            .as_ref()
                                                            .and_then(|on| on.uri.clone())
                                                    }),
                                                image: offchain_meta
                                                    .as_ref()
                                                    .and_then(|o| o.image.clone()),
                                                creator: creator.clone(),
                                                detect_time,
                                                buy_time: holding.buy_time,
                                                buy_amount_sol: settings.buy_amount,
                                                buy_amount_tokens: holding.amount,
                                                buy_price: holding.buy_price,
                                            };
                                            // Log successful buy to API (before moving holding)
                                            bot_log!(
                                                "info",
                                                format!("Successfully bought token {}", mint),
                                                format!(
                                                    "Amount: {} SOL, Price: {} SOL per token",
                                                    settings.buy_amount, holding.buy_price
                                                )
                                            );

                                            // Update detected coin status to "bought"
                                            {
                                                let mut coins = detected_coins.lock().await;
                                                if let Some(coin) =
                                                    coins.iter_mut().find(|c| c.mint == mint)
                                                {
                                                    coin.status = "bought".to_string();
                                                    coin.buy_price = Some(holding.buy_price);
                                                }
                                            }

                                            // Add buy trade record
                                            {
                                                let mut trades = trades_list.lock().await;
                                                // Convert amount from microtokens to tokens
                                                let amount_tokens = holding.amount as f64 / 1_000_000.0;
                                                trades.insert(
                                                    0,
                                                    api::TradeRecord {
                                                        mint: mint.clone(),
                                                        symbol: offchain_meta
                                                            .as_ref()
                                                            .and_then(|o| o.symbol.clone()),
                                                        name: offchain_meta
                                                            .as_ref()
                                                            .and_then(|o| o.name.clone()),
                                                        image: offchain_meta
                                                            .as_ref()
                                                            .and_then(|o| o.image.clone()),
                                                        trade_type: "buy".to_string(),
                                                        timestamp: holding.buy_time.to_rfc3339(),
                                                        tx_signature: None, // TX signature not readily available here
                                                        amount_sol: settings.buy_amount,
                                                        amount_tokens,
                                                        price_per_token: holding.buy_price,
                                                        profit_loss: None,
                                                        profit_loss_percent: None,
                                                        reason: None,
                                                    },
                                                );
                                                // Keep only last 200 trades
                                                if trades.len() > 200 {
                                                    trades.truncate(200);
                                                }
                                            }

                                            trades_map
                                                .lock()
                                                .await
                                                .insert(mint.clone(), buy_record);
                                            holdings_guard.insert(mint.clone(), holding);

                                            // Keep the subscription active for this mint
                                            keep_sub = true;
                                            // Persist the mapping of mint -> (wss_idx, sub_id)
                                            let mut map = sub_map.lock().await;
                                            map.insert(mint.clone(), (idx, sub_id));
                                        }
                                        Err(e) => {
                                            log::warn!("Failed to buy {}: {}", mint, e);
                                            bot_log!(
                                                "warn",
                                                format!("Failed to buy token {}", mint),
                                                format!("{}", e)
                                            );
                                        }
                                    }
                                } else {
                                    // Collect some diagnostic info so operator can manually
                                    // inspect the bonding-curve PDA in a Solana explorer.
                                    let cached_price = price_cache
                                        .lock()
                                        .await
                                        .get(&mint)
                                        .map(|(_, p)| format!("{:.18}", p))
                                        .unwrap_or_else(|| "none".to_string());

                                    log::warn!(
                                    "No WSS price update received for {} within {}s; skipping buy. details: mint={} curve_pda={} holder_addr={} sub_id={} pump_fun_program={} cached_price_sol={}. Paste the curve_pda into a Solana explorer to inspect account data and reserves.",
                                    mint,
                                    settings.wss_subscribe_timeout_secs,
                                    mint,
                                    curve_pda,
                                    holder_addr,
                                    sub_id,
                                    settings.pump_fun_program,
                                    cached_price
                                );
                                }
                                // Unsubscribe to minimize active subscriptions if we did
                                // not keep the subscription (e.g., we skipped buy or buy failed).
                                if !keep_sub {
                                    let (u_tx, u_rx) = oneshot::channel::<Result<(), String>>();
                                    let _ = sender
                                        .send(WsRequest::Unsubscribe { sub_id, resp: u_tx })
                                        .await;
                                    let _ = tokio::time::timeout(
                                        std::time::Duration::from_secs(3),
                                        u_rx,
                                    )
                                    .await;
                                }
                            }
                            Ok(Ok(Err(err_msg))) => {
                                if err_msg.contains("max subscriptions") {
                                    debug!(
                                        "Subscribe rejected for {} (WSS idx={} full)",
                                        mint, idx
                                    );
                                } else if err_msg.contains("degraded") {
                                    debug!("WSS idx={} degraded, skipped {}", idx, mint);
                                } else {
                                    log::warn!(
                                        "Subscribe request rejected for {}: {}",
                                        mint,
                                        err_msg
                                    );
                                }
                            }
                            Ok(Err(_)) => log::warn!(
                                "Subscribe channel closed for {} (WSS idx={})",
                                mint,
                                idx
                            ),
                            Err(_) => {
                                warn!("Subscribe timed out for {} (WSS idx={}, 5s)", mint, idx)
                            }
                        }
                    }
                } else {
                    debug!("No healthy WSS available for {} (all degraded/full)", mint);
                }
            }
        } else {
            // No on-chain metadata to validate or price-seller checks; skip buy logic
            debug!("No on-chain metadata to validate buy for mint {} (sig={}) — detected only", mint, signature);
        }
            // We do not fall back to RPC here; WSS-only mode requires a streamed
            // price update from the bonding_curve PDA. If no WSS price was used,
            // skip buying.
        }
    }
    Ok(())
}
