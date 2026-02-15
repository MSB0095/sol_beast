mod api;
mod buyer;
mod dev_fee;
mod error;
mod helius_sender;
mod idl;
mod models;
mod monitor;
mod onchain_idl;
mod rpc;
mod settings;
mod state;
mod tx_builder;
mod ws;
mod pumpportal;
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

/// Monotonically-increasing total of genuinely detected coins. The Vec<DetectedCoin>
/// is capped at `detected_coins_max` for memory, but this counter keeps growing.
pub(crate) static TOTAL_DETECTED_COINS: Lazy<AtomicUsize> =
    Lazy::new(|| AtomicUsize::new(0));
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
use solana_sdk::pubkey::Pubkey;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
    // Atomic counter for in-flight buys (between check & insert) to enforce max_holded_coins
    let in_flight_buys = Arc::new(AtomicUsize::new(0));
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
    let is_real_cli = std::env::args().any(|arg| arg == "--real");
    // Dynamic mode flag — updated by the API when the user toggles mode via
    // the dashboard. Buy/sell logic reads this each tick instead of the static
    // CLI bool so the mode switch takes effect immediately.
    let is_real_flag = Arc::new(AtomicBool::new(is_real_cli));
    // Always attempt to load a wallet keypair (if configured) so the user can
    // switch to real mode at runtime via the dashboard without restarting.
    let keypair: Option<std::sync::Arc<Keypair>> = if let Some(bytes) = settings::load_keypair_from_env_var("SOL_BEAST_KEYPAIR_B64") {
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
    } else if is_real_cli {
        return Err(AppError::InvalidKeypair("No wallet keypair configured! Set wallet_keypair_path, wallet_private_key_string, wallet_keypair_json, or SOL_BEAST_KEYPAIR_B64 env var".to_string()));
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
    let initial_mode = if is_real_cli {
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

    // Create broadcast channel for WebSocket updates (moved up for WSS access)
    let (ws_tx, _ws_rx) = tokio::sync::broadcast::channel::<String>(100);

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
        
        // Always run Solana WSS for price monitoring (accountSubscribe).
        // If PumpPortal is enabled, ws::run_ws will verify that setting internally
        // and skip "logsSubscribe" (new token detection) to avoid duplicates,
        // but it will still handle price updates for holdings.
        for wss_url in settings.solana_ws_urls.iter() {
            let tx = tx.clone();
            let seen = seen.clone();
            let holdings_clone = holdings.clone();
            let price_cache_clone = price_cache.clone();
            let settings_clone = settings.clone();
            let rpc_clone = rpc_client.clone();
            let wss_url = wss_url.clone();
            let ws_tx_clone = ws_tx.clone();
            let (ctrl_tx, ctrl_rx) = mpsc::channel(256);
            ws_control_senders.push(ctrl_tx.clone());
            // Spawn a single task that owns the control receiver. `ws::run_ws` will
            // manage its own internal state and reconnect logic where appropriate.
            let handle = tokio::spawn(async move {
                if let Err(e) = ws::run_ws(
                    &wss_url,
                    tx.clone(),
                    ws_tx_clone, 
                    seen.clone(),
                    holdings_clone.clone(),
                    price_cache_clone.clone(),
                    ctrl_rx,
                    settings_clone.clone(),
                    rpc_clone.clone(),
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

    // Spawn PumpPortal websocket workers if enabled
    let mut pumpportal_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    if settings.pumpportal_enabled {
        for pp_url in settings.pumpportal_wss.iter() {
            let tx_clone = tx.clone();
            let settings_clone = settings.clone();
            let pp_url = pp_url.clone();
            let handle = tokio::spawn(async move {
                if let Err(e) = pumpportal::run_pumpportal_ws(&pp_url, tx_clone, settings_clone).await {
                    error!("PumpPortal connection {} failed: {}", pp_url, e);
                }
            });
            pumpportal_handles.push(handle);
        }
    }
    let ws_control_senders = Arc::new(ws_control_senders);
    // Round-robin index for WSS sender selection (true round-robin)
    let next_wss_sender = Arc::new(AtomicUsize::new(0usize));

    // Now spawn price monitoring (after ws_control_senders exists so monitor
    // can unsubscribe subscriptions on sell).
    
    // ws_tx was created above
    
    let rpc_client_clone = rpc_client.clone();
    let ws_control_senders_clone_for_monitor = ws_control_senders.clone();
    let sub_map_clone_for_monitor = sub_map.clone();
    let next_wss_sender_clone_for_monitor = next_wss_sender.clone();
    let simulate_keypair_clone_for_monitor = simulate_keypair_clone.clone();
    let trades_list_clone_for_monitor = trades_list.clone();
    let bot_control_for_monitor = bot_control.clone();
    let ws_tx_for_monitor = ws_tx.clone();
    let is_real_flag_for_monitor = is_real_flag.clone();
    
    let monitor_handle = tokio::spawn(async move {
        monitor::monitor_holdings(
            holdings_clone_monitor,
            price_cache_clone_monitor,
            rpc_client_clone,
            is_real_flag_for_monitor,
            keypair_clone_monitor,
            simulate_keypair_clone_for_monitor,
            settings_clone_monitor,
            trades_map_clone_monitor,
            ws_control_senders_clone_for_monitor,
            sub_map_clone_for_monitor,
            next_wss_sender_clone_for_monitor,
            trades_list_clone_for_monitor,
            bot_control_for_monitor,
            ws_tx_for_monitor,
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
        running_state: Some("stopped".to_string()),
        mode: Some(if is_real_cli { "real" } else { "dry-run" }.to_string()),
    }));

    let api_state = ApiState {
        settings: Arc::new(tokio::sync::Mutex::new(settings.as_ref().clone())),
        stats: api_stats.clone(),
        bot_control: bot_control.clone(),
        detected_coins: detected_coins.clone(),
        trades: trades_list.clone(),
        ws_tx: ws_tx.clone(),
        is_real_flag: is_real_flag.clone(),
        has_keypair: keypair.is_some(),
    };

    // Add initial startup log — bot starts stopped, user must choose mode and start manually
    bot_control
        .add_log(
            "info",
            "Bot initialized in stopped state. Choose a mode and start manually.".to_string(),
            Some(format!(
                "Wallet: {}",
                if is_real_cli {
                    keypair
                        .as_ref()
                        .map(|k| k.pubkey().to_string())
                        .unwrap_or_else(|| "None".to_string())
                } else {
                    format!("Simulation mode{}",
                        if keypair.is_some() { " (keypair loaded, can switch to real)" } else { "" })
                }
            )),
        )
        .await;

    // Bot starts in Stopped state — user must manually choose mode and start via the API/dashboard.
    // running_state is already Stopped from BotControl::new_with_mode.

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
        let seen = seen.clone();
        let holdings = holdings.clone();
        let in_flight_buys = in_flight_buys.clone();
        let rpc_client = rpc_client.clone();
        let price_cache = price_cache.clone();
        let settings = settings.clone();
        let ws_control_senders = ws_control_senders.clone();
        let next_wss_sender = next_wss_sender.clone();
        let trades_map = trades_map.clone();
        let sub_map = sub_map.clone();
        let detected_coins = detected_coins.clone();
        let trades_list = trades_list.clone();
        let ws_tx = ws_tx.clone();
        let keypair = keypair.clone();
        let simulate_keypair = simulate_keypair.clone();
        let is_real_flag = is_real_flag.clone();

        tokio::spawn(async move {
            let is_real = is_real_flag.load(Ordering::Relaxed);
            if let Err(e) = process_message(
                &msg,
                &seen,
                &holdings,
                &in_flight_buys,
                &rpc_client,
                is_real,
                keypair.as_deref(),
                simulate_keypair.as_deref(),
                &price_cache,
                &settings,
                ws_control_senders,
                next_wss_sender,
                trades_map,
                sub_map,
                detected_coins,
                trades_list,
                ws_tx,
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
        });
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
    in_flight_buys: &Arc<AtomicUsize>,
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
    ws_tx: tokio::sync::broadcast::Sender<String>,
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
        // Deduplicate by signature early to avoid double-processing if the same
        // event is received from both PumpPortal and Solana logs.
        if let Some(signature) = params.get("signature").and_then(|s| s.as_str()) {
            if signature != "" && signature != "0" {
                if seen.lock().await.put(signature.to_string(), ()).is_some() {
                    return Ok(());
                }
            }
        }

        // If this message is a normalized PumpPortal payload, handle it using
        // a fast path that avoids RPC `getTransaction` when PumpPortal provided
        // the mint/creator/curve and optional metadata.
        if let Some(pp) = params.get("pumpportal") {
            // Extract fields
            let signature = params.get("signature").and_then(|s| s.as_str()).unwrap_or("").to_string();
            let mint = pp.get("mint").and_then(|m| m.as_str()).map(|s| s.to_string());
            let creator = pp.get("creator").and_then(|c| c.as_str()).map(|s| s.to_string());
            let curve = pp.get("bonding_curve").and_then(|c| c.as_str()).map(|s| s.to_string());
            let metadata_value = pp.get("metadata").cloned();
            let bonding_state = pp.get("bonding_state").cloned();

            if let Some(mint) = mint {
                // We require creator and curve for a confident detection; otherwise fall back to RPC flow
                let creator = creator.unwrap_or_else(|| "".to_string());
                let curve = curve.unwrap_or_else(|| "".to_string());
                let detect_time = chrono::Utc::now();
                if let Err(e) = handle_new_token_from_pumpportal(
                    &signature,
                    &mint,
                    &creator,
                    &curve,
                    metadata_value,
                    bonding_state,
                    holdings,
                    in_flight_buys,
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
                    ws_tx.clone(),
                )
                .await
                {
                    error!("handle_new_token_from_pumpportal failed for {}: {}", mint, e);
                    return Err(e);
                }
                // Skip the rest of standard processing for this message
                return Ok(());
            }
        }
        let logs_opt = params.get("logs").and_then(|l| l.as_array());
        let sig_opt = params.get("signature").and_then(|s| s.as_str());
        if logs_opt.is_none() {
            debug!("Incoming message missing logs field: {:?}", params);
        }
        if sig_opt.is_none() {
            debug!("Incoming message missing signature field: {:?}", params);
        }

        if let (Some(logs), Some(signature)) = (logs_opt, sig_opt) {
            // Only process if logs mention the pump.fun program id
            // This ensures we only process transactions that actually interact with pump.fun
            let pump_prog_id = &settings.pump_fun_program;
            let logs_mention_pump = logs.iter().any(|log| {
                log.as_str()
                    .map(|s| s.contains(pump_prog_id))
                    .unwrap_or(false)
            });
            
            if logs_mention_pump 
            {
                // If we're already at max holdings, skip detection work
                // but do not block processing of other websocket messages
                // (like account notifications). Debounce the debug log
                // so it doesn't spam the logs.
                let total_active = holdings.lock().await.len() + in_flight_buys.load(Ordering::SeqCst);
                if total_active >= settings.max_holded_coins {
                    let mut last_lock = LAST_MAX_HELD_LOG.lock().await;
                    let now = Instant::now();
                    let should_log = match *last_lock {
                        Some(ts) => now.duration_since(ts).as_secs() > MAX_HELD_LOG_DEBOUNCE_SECS,
                        None => true,
                    };
                    if should_log {
                        *last_lock = Some(now);
                        debug!(
                            "Max held coins reached ({} held + {} in-flight >= {}); skipping incoming message processing",
                            total_active - in_flight_buys.load(Ordering::SeqCst),
                            in_flight_buys.load(Ordering::SeqCst),
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
                    debug!("Skipping invalid signature for pump.fun notification: {}", signature);
                    return Ok(());
                }
                if let Err(e) = handle_new_token(
                    signature,
                    holdings,
                    in_flight_buys,
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
                    ws_tx.clone(),
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
    in_flight_buys: &Arc<AtomicUsize>,
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
    ws_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::sync::oneshot;

    // UI UPDATE: Add signature-only entry immediately to show activity
    {
        let mut coins = detected_coins.lock().await;
        let new_coin = api::DetectedCoin {
            mint: format!("sig:{}", &signature[..8]), // Placeholder
            name: Some("Fetching details...".to_string()),
            symbol: None,
            image: None,
            creator: "".to_string(),
            bonding_curve: "".to_string(),
            detected_at: detect_time.to_rfc3339(),
            metadata_uri: None,
            buy_price: None,
            status: "detected".to_string(),
        };
        coins.insert(0, new_coin.clone());
        let _ = ws_tx.send(serde_json::json!({"type": "detected-coin", "coin": new_coin}).to_string());
        if coins.len() > settings.detected_coins_max {
            coins.truncate(settings.detected_coins_max);
        }
    }

    let (creator, mint, curve_pda, holder_addr, is_initialization) =
        rpc::fetch_transaction_details(signature, rpc_client, settings).await?;
    
    // UI UPDATE: Remove the placeholder before continuing (or it will be updated by process_detected_token)
    {
        let mut coins = detected_coins.lock().await;
        coins.retain(|c| c.mint != format!("sig:{}", &signature[..8]));
    }

    if !is_initialization {
        // Not a pump.fun create instruction; skip detection.
        debug!("Transaction {} is not a pump.fun CREATE instruction; skipping detection", signature);
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
            if let Some(existing) = coins.iter_mut().find(|c| c.mint == mint) {
                existing.name = Some(token_name.clone());
                existing.symbol = token_symbol.clone();
                existing.image = offchain_meta.as_ref().and_then(|o| o.image.clone());
                existing.creator = creator.clone();
                existing.bonding_curve = curve_pda.clone();
                existing.detected_at = detect_time.to_rfc3339();
                existing.metadata_uri = metadata_uri_opt.clone();
                existing.buy_price = None;
                existing.status = "detected".to_string();
            } else {
                let new_coin = api::DetectedCoin {
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
                };
                coins.insert(0, new_coin.clone());
                TOTAL_DETECTED_COINS.fetch_add(1, Ordering::Relaxed);
                
                // Broadcast to WebSocket clients
                let ws_message = serde_json::json!({
                    "type": "detected-coin",
                    "coin": new_coin,
                    "total_detected_coins": TOTAL_DETECTED_COINS.load(Ordering::Relaxed)
                });
                let _ = ws_tx.send(ws_message.to_string());
            }
            // Keep only last `detected_coins_max` detected coins (configurable)
            if coins.len() > settings.detected_coins_max {
                coins.truncate(settings.detected_coins_max);
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
            // REFACTORED BUY LOGIC:
            // 1. Attempt WSS subscription if enabled.
            // 2. Fall back to RPC if WSS fails or is disabled (to ensure 'dry run' works).
            // 3. Buy if price available.
            
            // Track active subscription for cleanup: (ws_index, sub_id, sender_clone)
            let mut active_sub_details: Option<(usize, u64, tokio::sync::mpsc::Sender<WsRequest>)> = None;
            let price_source = settings.price_source.clone();

            // 1. Attempt WSS Subscription
            if price_source != "rpc" && !ws_control_senders.is_empty() {
                if let Some(idx) = select_healthy_wss(&ws_control_senders, settings).await {
                    let sender = ws_control_senders[idx].clone();
                    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<Result<u64, String>>();
                    // Use error handling for pubkey parsing (though likely valid if we got here)
                    if let (Ok(pump_prog_key), Ok(mint_pk)) = (
                        solana_sdk::pubkey::Pubkey::from_str(&settings.pump_fun_program),
                        solana_sdk::pubkey::Pubkey::from_str(&mint)
                    ) {
                        let (curve_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
                            &[b"bonding-curve", mint_pk.as_ref()],
                            &pump_prog_key,
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
                            match tokio::time::timeout(std::time::Duration::from_secs(5), resp_rx).await {
                                 Ok(Ok(Ok(sub_id))) => {
                                     debug!("Subscribed to {} on sub {}", mint, sub_id);
                                     active_sub_details = Some((idx, sub_id, sender.clone()));
                                 },
                                 Ok(Ok(Err(err_msg))) => {
                                     if err_msg.contains("max subscriptions") {
                                         debug!("Subscribe rejected for {} (WSS idx={} full)", mint, idx);
                                     } else if err_msg.contains("degraded") {
                                         debug!("WSS idx={} degraded, skipped {}", idx, mint);
                                     } else {
                                         log::warn!("Subscribe request rejected for {}: {}", mint, err_msg);
                                     }
                                 },
                                 Ok(Err(_)) => log::warn!("Subscribe channel closed for {} (WSS idx={})", mint, idx),
                                 Err(_) => log::warn!("Subscribe timed out for {} (WSS idx={}, 5s)", mint, idx),
                            }
                        }
                    }
                } else {
                     debug!("No healthy WSS available for {} (all degraded/full)", mint);
                }
            }

            // 2. Fetch or Prime Price (Hybrid RPC + WSS wait)
            let mut price_opt: Option<f64> = None;
            
            // Always try RPC prime first - it's fast enough and provides immediate fallback
             match rpc::fetch_current_price(
                &mint,
                price_cache,
                rpc_client,
                settings,
            ).await {
                Ok(p) => {
                    debug!("Primed price cache for {} via RPC: {:.18} SOL/token", mint, p);
                    price_opt = Some(p);
                }
                Err(e) => {
                     // Debug instead of warn because we might get it from WSS soon
                     debug!("RPC prime failed for {}: {}.", mint, e);
                }
            }

            // If we have a WSS subscription but no RPC price, wait for WSS update
            if price_opt.is_none() && active_sub_details.is_some() {
                 debug!("Waiting for WSS price update for {}...", mint);
                 let start = std::time::Instant::now();
                 while start.elapsed().as_secs() < settings.wss_subscribe_timeout_secs {
                     if let Some((_, price)) = price_cache.lock().await.get(&mint).cloned() {
                         price_opt = Some(price);
                         break;
                     }
                     tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                 }
            }

            // 3. Buy Token
            let mut keep_sub = false;
            
            if let Some(_price) = price_opt {
                 // Refresh price cache so buyer::buy_token uses the cached value
                 // and skips the slow multi-commitment RPC re-fetch sequence.
                 price_cache.lock().await.put(mint.clone(), (Instant::now(), _price));

                 // Lock holdings BRIEFLY — do NOT hold across the slow buy call.
                 // Reserve the slot with in_flight_buys to prevent concurrent over-buying.
                 let skip_buy = {
                     // Check if bot is running before attempting to buy
                     if let Some(control) = BOT_CONTROL.get() {
                         let rs = control.running_state.lock().await;
                         if !matches!(*rs, api::BotRunningState::Running) {
                             debug!("Bot not running; skipping buy for {}", mint);
                             true
                         } else {
                             false
                         }
                     } else {
                         true
                     }
                 } || {
                     let hg = holdings.lock().await;
                     if hg.contains_key(&mint) {
                         debug!("Already holding {}; skipping duplicate buy", mint);
                         true
                     } else if hg.len() + in_flight_buys.load(Ordering::SeqCst) >= settings.max_holded_coins {
                         info!("Max held coins reached ({} held + {} in-flight >= {}); skipping buy for {}", hg.len(), in_flight_buys.load(Ordering::SeqCst), settings.max_holded_coins, mint);
                         true
                     } else {
                         in_flight_buys.fetch_add(1, Ordering::SeqCst);
                         false
                     }
                 }; // holdings lock released

                 if skip_buy {
                      // fall through to subscription cleanup
                 } else {
                      match buyer::buy_token(
                          &mint,
                          settings.buy_amount,
                          is_real,
                          keypair,
                          simulate_keypair,
                          price_cache.clone(),
                          rpc_client,
                          settings,
                      ).await {
                          Ok(mut holding) => {
                               // --- SUCCESSFUL BUY LOGIC ---
                               holding.metadata = offchain_meta.clone();
                               holding.onchain_raw = onchain_raw.clone();
                               // Build compact onchain_struct
                               let mut onchain_struct: Option<crate::models::OnchainFullMetadata> = None;
                               if let Some(meta) = onchain_meta.as_ref() {
                                    let name = meta.name.trim_end_matches('\u{0}').to_string();
                                    let symbol = meta.symbol.trim_end_matches('\u{0}').to_string();
                                    let uri = meta.uri.trim_end_matches('\u{0}').to_string();
                                    onchain_struct = Some(crate::models::OnchainFullMetadata {
                                        name: if name.is_empty() { None } else { Some(name) },
                                        symbol: if symbol.is_empty() { None } else { Some(symbol) },
                                        uri: if uri.is_empty() { None } else { Some(uri) },
                                        seller_fee_basis_points: Some(meta.seller_fee_basis_points),
                                        raw: onchain_raw.clone(),
                                    });
                               } else if let Some(raw_bytes) = onchain_raw.as_ref() {
                                    if let Ok(parsed) = OnchainMetadataRaw::safe_deserialize(raw_bytes) {
                                         let name = parsed.name.trim_end_matches('\u{0}').to_string();
                                         let symbol = parsed.symbol.trim_end_matches('\u{0}').to_string();
                                         let uri = parsed.uri.trim_end_matches('\u{0}').to_string();
                                         onchain_struct = Some(crate::models::OnchainFullMetadata {
                                            name: if name.is_empty() { None } else { Some(name) },
                                            symbol: if symbol.is_empty() { None } else { Some(symbol) },
                                            uri: if uri.is_empty() { None } else { Some(uri) },
                                            seller_fee_basis_points: Some(parsed.seller_fee_basis_points),
                                            raw: Some(raw_bytes.clone()),
                                        });
                                    }
                               }
                               holding.onchain = onchain_struct.clone();
                               if let Some(off) = &holding.metadata {
                                    info!("Persisting off-chain metadata for {} into holdings: name={:?}, symbol={:?}, image={:?}", mint, off.name, off.symbol, off.image);
                               }
                               
                               let buy_record = BuyRecord {
                                   mint: mint.clone(),
                                   symbol: offchain_meta.as_ref().and_then(|o| o.symbol.clone()),
                                   name: offchain_meta.as_ref().and_then(|o| o.name.clone()),
                                   uri: offchain_meta.as_ref().and_then(|o| o.image.clone()).or_else(|| onchain_struct.as_ref().and_then(|on| on.uri.clone())),
                                   image: offchain_meta.as_ref().and_then(|o| o.image.clone()),
                                   creator: creator.clone(),
                                   detect_time,
                                   buy_time: holding.buy_time,
                                   buy_amount_sol: settings.buy_amount,
                                   buy_amount_tokens: holding.amount,
                                   buy_price: holding.buy_price,
                               };
                               // Log successful buy to API
                               bot_log!("info", format!("Successfully bought token {}", mint), format!("Amount: {} SOL, Price: {} SOL per token", settings.buy_amount, holding.buy_price));
                               
                               // Update detected coin status
                               {
                                   let mut coins = detected_coins.lock().await;
                                   if let Some(coin) = coins.iter_mut().find(|c| c.mint == mint) {
                                       coin.status = "bought".to_string();
                                       coin.buy_price = Some(holding.buy_price);
                                   }
                               }

                               // Add buy trade record
                               {
                                   let mut trades = trades_list.lock().await;
                                   let token_divisor = 10f64.powi(holding.decimals as i32);
                                   let amount_tokens = holding.amount as f64 / token_divisor;
                                   trades.insert(0, api::TradeRecord {
                                       mint: mint.clone(),
                                       symbol: offchain_meta.as_ref().and_then(|o| o.symbol.clone()),
                                       name: offchain_meta.as_ref().and_then(|o| o.name.clone()),
                                       image: offchain_meta.as_ref().and_then(|o| o.image.clone()),
                                       trade_type: "buy".to_string(),
                                       timestamp: holding.buy_time.to_rfc3339(),
                                       tx_signature: None,
                                       amount_sol: holding.buy_cost_sol.unwrap_or(settings.buy_amount),
                                       amount_tokens,
                                       price_per_token: holding.buy_price,
                                       profit_loss: None,
                                       profit_loss_percent: None,
                                       reason: None,
                                       decimals: holding.decimals,
                                       actual_sol_change: holding.buy_cost_sol.map(|c| -c),
                                       tx_fee_sol: None,
                                       simulated: !is_real,
                                   });
                                   if trades.len() > 200 { trades.truncate(200); }
                               }
                               
                               trades_map.lock().await.insert(mint.clone(), buy_record);
                               holdings.lock().await.insert(mint.clone(), holding);
                               in_flight_buys.fetch_sub(1, Ordering::SeqCst);

                               keep_sub = true;
                          },
                          Err(e) => {
                               in_flight_buys.fetch_sub(1, Ordering::SeqCst);
                               log::warn!("Failed to buy {}: {}", mint, e);
                               bot_log!("warn", format!("Failed to buy token {}", mint), format!("{}", e));

                               // Record failed buy attempt so it appears in Trading History
                               {
                                   let mut trades = trades_list.lock().await;
                                   trades.insert(0, api::TradeRecord {
                                       mint: mint.clone(),
                                       symbol: offchain_meta.as_ref().and_then(|o| o.symbol.clone()),
                                       name: offchain_meta.as_ref().and_then(|o| o.name.clone()),
                                       image: offchain_meta.as_ref().and_then(|o| o.image.clone()),
                                       trade_type: "buy".to_string(),
                                       timestamp: chrono::Utc::now().to_rfc3339(),
                                       tx_signature: None,
                                       amount_sol: settings.buy_amount,
                                       amount_tokens: 0.0,
                                       price_per_token: 0.0,
                                       profit_loss: None,
                                       profit_loss_percent: None,
                                       reason: Some(format!("FAILED: {}", e)),
                                       decimals: settings.default_token_decimals,
                                       actual_sol_change: None,
                                       tx_fee_sol: None,
                                       simulated: !is_real,
                                   });
                                   if trades.len() > 200 { trades.truncate(200); }
                               }

                               // Update detected coin status to buy_failed
                               {
                                   let mut coins = detected_coins.lock().await;
                                   if let Some(coin) = coins.iter_mut().find(|c| c.mint == mint) {
                                       coin.status = "buy_failed".to_string();
                                   }
                               }
                          }
                      }
                 }
            } else {
                  if active_sub_details.is_some() {
                       log::warn!("No price update received for {} (WSS timed out) and RPC failed. Skipping buy.", mint);
                  } else {
                       // dry run or RPC failed
                       debug!("No price available for {}, skipping buy (RPC failed)", mint);
                  }
            }

            // 4. Cleanup Subscription
            if let Some((idx, sub_id, sender)) = active_sub_details {
                if keep_sub {
                    let mut map = sub_map.lock().await;
                    map.insert(mint.clone(), (idx, sub_id));
                } else {
                    let (u_tx, u_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
                    let _ = sender.send(WsRequest::Unsubscribe { sub_id, resp: u_tx }).await;
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), u_rx).await;
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

/// Process a detected token using provided details. If onchain/offchain metadata
/// are None, this function will attempt RPC fetches as a fallback.
#[allow(clippy::too_many_arguments)]
async fn process_detected_token(
    signature: &str,
    mint: &str,
    creator: &str,
    curve_pda: &str,
    holder_addr: &str,
    detect_time: chrono::DateTime<Utc>,
    rpc_client: &Arc<RpcClient>,
    _is_real: bool,
    _keypair: Option<&Keypair>,
    _simulate_keypair: Option<&Keypair>,
    _price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
    _ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    _next_wss_sender: Arc<AtomicUsize>,
    _trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    _sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
    detected_coins: Arc<tokio::sync::Mutex<Vec<api::DetectedCoin>>>,
    _trades_list: Arc<tokio::sync::Mutex<Vec<api::TradeRecord>>>,
    onchain_meta_opt: Option<mpl_token_metadata::accounts::Metadata>,
    offchain_meta_opt: Option<crate::models::OffchainTokenMetadata>,
    onchain_raw_opt: Option<Vec<u8>>,
    ws_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // -------------------------------------------------------------------------
    // 1. EARLIEST POSSIBLE UI UPDATE (Before slow RPC calls)
    // -------------------------------------------------------------------------
    {
        // Calculate placeholder or preliminary info
        let pre_name = offchain_meta_opt.as_ref().and_then(|o| o.name.clone())
            .or_else(|| onchain_meta_opt.as_ref().map(|m| m.name.trim_end_matches('\u{0}').to_string()))
            .unwrap_or_else(|| "Searching metadata...".to_string());
        let pre_symbol = offchain_meta_opt.as_ref().and_then(|o| o.symbol.clone())
            .or_else(|| onchain_meta_opt.as_ref().map(|m| m.symbol.trim_end_matches('\u{0}').to_string()));
        let pre_image = offchain_meta_opt.as_ref().and_then(|o| o.image.clone());

        let mut coins = detected_coins.lock().await;
        if coins.iter().find(|c| c.mint == mint).is_none() {
            let new_coin = api::DetectedCoin {
                mint: mint.to_string(),
                name: Some(pre_name),
                symbol: pre_symbol,
                image: pre_image,
                creator: creator.to_string(),
                bonding_curve: curve_pda.to_string(),
                detected_at: detect_time.to_rfc3339(),
                metadata_uri: onchain_meta_opt.as_ref().map(|m| m.uri.trim_end_matches('\u{0}').to_string()),
                buy_price: None,
                status: "detected".to_string(),
            };
            coins.insert(0, new_coin.clone());
            TOTAL_DETECTED_COINS.fetch_add(1, Ordering::Relaxed);
            
            // Broadcast to WebSocket clients
            let ws_message = serde_json::json!({
                "type": "detected-coin",
                "coin": new_coin,
                "total_detected_coins": TOTAL_DETECTED_COINS.load(Ordering::Relaxed)
            });
            let _ = ws_tx.send(ws_message.to_string());
            
            if coins.len() > settings.detected_coins_max {
                coins.truncate(settings.detected_coins_max);
            }
        }
    }

    // Attempt to fetch bonding curve creator for additional verification
    let bonding_creator_opt = rpc::fetch_bonding_curve_creator(&mint.to_string(), rpc_client, settings).await.ok().flatten();
    if bonding_creator_opt.is_none() {
        debug!("Bonding curve creator not found for mint {} (sig={}) — continuing detection using provided values", mint, signature);
    }

    // Use provided metadata if present, otherwise fallback to RPC-fetched values
    let (onchain_meta, offchain_meta, _onchain_raw) = if onchain_meta_opt.is_some() || offchain_meta_opt.is_some() || onchain_raw_opt.is_some() {
        (onchain_meta_opt, offchain_meta_opt, onchain_raw_opt)
    } else {
        rpc::fetch_token_metadata(&mint.to_string(), rpc_client, settings).await?
    };

    // If we have offchain metadata but no `image`, try to extract it from `extras` or by
    // fetching a metadata URI if provided (PumpPortal commonly supplies `uri` in extras).
    let mut offchain_meta = offchain_meta;
    if let Some(mut off) = offchain_meta.take() {
        if off.image.is_none() {
            // Try common image keys in extras
            if let Some(ref extras) = off.extras {
                if let Some(img) = extract_first_string(extras, &["image", "image_url", "imageUri", "imageUrl"]) {
                    off.image = Some(img);
                }
            }
            // If still none, try to find a metadata URI and fetch it
            if off.image.is_none() {
                // Look for uri variants in extras
                if let Some(ref extras) = off.extras {
                    if let Some(uri) = extract_first_string(extras, &["uri", "metadataUri", "tokenUri", "uriStr"]) {
                        if uri.starts_with("http://") || uri.starts_with("https://") {
                            let client = reqwest::Client::new();
                            match client.get(&uri).send().await {
                                Ok(resp) => match resp.text().await {
                                    Ok(body) => if let Ok(body_val) = serde_json::from_str::<serde_json::Value>(&body) {
                                        if let Some(img2) = extract_first_string(&body_val, &["image", "image_url", "imageUri", "imageUrl"]) {
                                            off.image = Some(img2);
                                        } else {
                                            debug!("Fetched metadata {} but no image field found for mint {}", uri, mint);
                                        }
                                    } else { debug!("Failed to parse fetched metadata JSON {}", uri); },
                                    Err(e) => debug!("Failed to read metadata body {}: {}", uri, e),
                                },
                                Err(e) => debug!("HTTP error fetching metadata {}: {}", uri, e),
                            }
                        }
                    }
                }
            }
        }
        // normalize back into option
        offchain_meta = Some(off);
    }

    // Helper used above to extract common string fields from a JSON value
    fn extract_first_string(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
        for key in keys {
            if let Some(field) = v.get(*key) {
                match field {
                    serde_json::Value::String(s) => return Some(s.clone()),
                    serde_json::Value::Object(map) => {
                        if let Some(serde_json::Value::String(s2)) = map.get("en") { return Some(s2.clone()); }
                        for (_k, val) in map.iter() {
                            if let serde_json::Value::String(s3) = val { return Some(s3.clone()); }
                        }
                    }
                    serde_json::Value::Array(arr) => {
                        if let Some(serde_json::Value::String(s4)) = arr.get(0) { return Some(s4.clone()); }
                    }
                    other => return Some(other.to_string()),
                }
            }
        }
        None
    }

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
        Some(m.uri.trim_end_matches('\u{0}').to_string())
    } else if let Some(off) = offchain_meta.as_ref() { off.image.clone() } else { None };

    info!(
        "New pump.fun token detected: mint={} signature={} creator={} curve={} holder={} URI={}",
        mint,
        signature,
        creator,
        curve_pda,
        holder_addr,
        metadata_uri_opt.clone().unwrap_or_else(|| "<none>".to_string())
    );

    bot_log!(
        "info",
        format!("New token detected: {}", token_name),
        format!("Mint: {}, Creator: {}", mint, creator)
    );

    {
        let mut coins = detected_coins.lock().await;
        if let Some(existing) = coins.iter_mut().find(|c| c.mint == mint) {
            existing.name = Some(token_name.clone());
            existing.symbol = token_symbol.clone();
            existing.image = offchain_meta.as_ref().and_then(|o| o.image.clone());
            existing.creator = creator.to_string();
            existing.bonding_curve = curve_pda.to_string();
            existing.detected_at = detect_time.to_rfc3339();
            existing.metadata_uri = metadata_uri_opt.clone();
            existing.buy_price = None;
            existing.status = "detected".to_string();
        } else {
            let new_coin = api::DetectedCoin {
                mint: mint.to_string(),
                name: Some(token_name.clone()),
                symbol: token_symbol.clone(),
                image: offchain_meta.as_ref().and_then(|o| o.image.clone()),
                creator: creator.to_string(),
                bonding_curve: curve_pda.to_string(),
                detected_at: detect_time.to_rfc3339(),
                metadata_uri: metadata_uri_opt.clone(),
                buy_price: None,
                status: "detected".to_string(),
            };
            coins.insert(0, new_coin.clone());
            TOTAL_DETECTED_COINS.fetch_add(1, Ordering::Relaxed);
            
            // Broadcast to WebSocket clients
            let ws_message = serde_json::json!({
                "type": "detected-coin",
                "coin": new_coin,
                "total_detected_coins": TOTAL_DETECTED_COINS.load(Ordering::Relaxed)
            });
            let _ = ws_tx.send(ws_message.to_string());
            
            // Keep only last `detected_coins_max` detected coins (configurable)
            if coins.len() > settings.detected_coins_max {
                coins.truncate(settings.detected_coins_max);
            }
        }
    }

    Ok(())
}

/// Handle enriched PumpPortal event without doing full RPC `getTransaction` when possible
#[allow(clippy::too_many_arguments)]
async fn handle_new_token_from_pumpportal(
    signature: &str,
    mint: &str,
    creator: &str,
    curve_pda: &str,
    metadata_value: Option<serde_json::Value>,
    bonding_state: Option<serde_json::Value>,
    _holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    in_flight_buys: &Arc<AtomicUsize>,
    rpc_client: &Arc<RpcClient>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    next_wss_sender: Arc<AtomicUsize>,
    detect_time: chrono::DateTime<Utc>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
    detected_coins: Arc<tokio::sync::Mutex<Vec<api::DetectedCoin>>>,
    trades_list: Arc<tokio::sync::Mutex<Vec<api::TradeRecord>>>,
    ws_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse metadata_value into OffchainTokenMetadata if present
    let offchain_meta_opt: Option<crate::models::OffchainTokenMetadata> = if let Some(val) = metadata_value {
        match serde_json::from_value(val) {
            Ok(m) => Some(m),
            Err(e) => {
                debug!("Failed to parse PumpPortal metadata into OffchainTokenMetadata: {}", e);
                None
            }
        }
    } else { None };

    // If PumpPortal provided bonding_state/reserves, compute price and update price_cache
    if let Some(bstate) = bonding_state {
        // Try to extract common numeric fields used by bonding curve
        let get_u64 = |obj: &serde_json::Value, key: &str| -> Option<u64> {
            match obj.get(key) {
                Some(v) => {
                    if let Some(n) = v.as_u64() { Some(n) }
                    else if let Some(s) = v.as_str() { s.parse::<u64>().ok() }
                    else { None }
                }
                None => None,
            }
        };

        let vtok_opt = get_u64(&bstate, "virtual_token_reserves");
        let vsol_opt = get_u64(&bstate, "virtual_sol_reserves");
        let complete_flag = bstate.get("complete").and_then(|v| v.as_bool()).unwrap_or(false);

        if complete_flag {
            debug!("Bonding state from PumpPortal reports migrated/completed for mint {}", mint);
        } else if let (Some(vtok), Some(vsol)) = (vtok_opt, vsol_opt) {
            // Compute an initial price from PumpPortal reserve data and seed the
            // price cache so the buy path below has an immediate price available
            // instead of waiting for a WSS update that may never arrive for a
            // brand-new token.
            if vtok > 0 {
                // pump.fun tokens always use 6 decimals.  Use the proven formula
                // from BondingCurveState::spot_price_sol_per_token():
                // price = (vsol_lamports / vtok_base_units) * 1e-3
                // which is equivalent to (vsol/1e9) / (vtok/1e6)
                let price = (vsol as f64 / vtok as f64) * 1e-3;
                info!("Computed initial price from PumpPortal reserves for {}: {:.18} SOL/token (vtok={}, vsol={})", mint, price, vtok, vsol);
                price_cache.lock().await.put(mint.to_string(), (Instant::now(), price));
            } else {
                debug!("PumpPortal provided zero token reserves for {} — cannot compute price", mint);
            }
        }
    }
    // No onchain_raw or onchain_meta provided; process_detected_token will fallback to RPC if needed
    // If PumpPortal did not provide a bonding_curve PDA string, compute the canonical
    // pump.fun bonding-curve PDA here and pass it through so downstream code and the
    // API always see a populated `bonding_curve` field.
    let curve_string = if curve_pda.trim().is_empty() {
        match Pubkey::from_str(&settings.pump_fun_program) {
            Ok(pump_program) => match Pubkey::from_str(mint) {
                Ok(mint_pk) => {
                    let (pda, _b) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
                    pda.to_string()
                }
                Err(_) => "".to_string(),
            },
            Err(_) => "".to_string(),
        }
    } else {
        curve_pda.to_string()
    };

    // First, perform the usual detected-token processing which will populate the API list
    process_detected_token(
        signature,
        mint,
        creator,
        &curve_string,
        "",
        detect_time,
        rpc_client,
        is_real,
        keypair,
        simulate_keypair,
        price_cache,
        settings,
        ws_control_senders.clone(),
        next_wss_sender.clone(),
        trades_map.clone(),
        sub_map.clone(),
        detected_coins.clone(),
        trades_list.clone(),
        None,
        offchain_meta_opt.clone(),
        None,
        ws_tx.clone(),
    )
    .await?;

    // Attempt a buy on PumpPortal detections when a price is available.
    // Prefer WSS live price for the buy decision by subscribing just before buy
    // (latency-sensitive). If WSS subscription or price retrieval fails, fall
    // back to cached or RPC price as before.
    let mut price_opt: Option<f64> = None;
    let mut subscribed_idx: Option<usize> = None;
    let mut subscribed_sub_id: Option<u64> = None;
    let mut sub_was_created = false;

    // Try to create a short-lived WSS subscription to get a live price update
    if !ws_control_senders.is_empty() {
        if let Some(idx) = select_healthy_wss(&ws_control_senders, settings).await {
            let sender = &ws_control_senders[idx];
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<Result<u64, String>>();
            let pump_prog = Pubkey::from_str(&settings.pump_fun_program)?;
            if let Ok(mint_pk) = Pubkey::from_str(mint) {
                let (curve_pda, _b) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_prog);
                let subscribe_req = WsRequest::Subscribe { account: curve_pda.to_string(), mint: mint.to_string(), resp: resp_tx };
                if let Err(e) = sender.send(subscribe_req).await {
                    log::warn!("Failed to send subscribe request for {}: {}", mint, e);
                } else {
                    match tokio::time::timeout(std::time::Duration::from_secs(5), resp_rx).await {
                        Ok(Ok(Ok(sub_id))) => {
                            debug!("Subscribed to {} on sub {} (pre-buy)", mint, sub_id);
                            sub_was_created = true;
                            subscribed_idx = Some(idx);
                            subscribed_sub_id = Some(sub_id);
                        }
                        Ok(Ok(Err(err_msg))) => {
                            if err_msg.contains("max subscriptions") {
                                debug!("Subscribe rejected for {} (WSS idx={} full)", mint, idx);
                            } else if err_msg.contains("degraded") {
                                debug!("WSS idx={} degraded, skipped {}", idx, mint);
                            } else {
                                log::warn!("Subscribe request rejected for {}: {}", mint, err_msg);
                            }
                        }
                        Ok(Err(_)) => log::warn!("Subscribe channel closed for {} (WSS idx={})", mint, idx),
                        Err(_) => warn!("Subscribe timed out for {} (WSS idx={}, 5s)", mint, idx),
                    }
                }
            }
        } else {
            debug!("No healthy WSS available for {} (all degraded/full)", mint);
        }
    }

    // If subscription was established, wait briefly for a price to appear in the cache
    if sub_was_created {
        // First check quickly
        if let Some((_, price)) = price_cache.lock().await.get(mint).cloned() {
            price_opt = Some(price);
        } else {
            // Wait up to settings.wss_subscribe_timeout_secs for WSS to push initial price
            let start = Instant::now();
            while start.elapsed().as_secs() < settings.wss_subscribe_timeout_secs {
                if let Some((_, price)) = price_cache.lock().await.get(mint).cloned() {
                    price_opt = Some(price);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }

    // If we still don't have a WSS price, fall back to cached price then RPC as before
    if price_opt.is_none() {
        if let Some((_, price)) = price_cache.lock().await.get(mint).cloned() {
            price_opt = Some(price);
        }
    }
    if price_opt.is_none() {
        if let Ok(p) = rpc::fetch_current_price(&mint.to_string(), price_cache, rpc_client, settings).await {
            price_opt = Some(p);
        }
    }

    if let Some(_price) = price_opt {
        // Refresh price cache timestamp so buyer::buy_token finds a fresh entry
        // and skips the slow multi-commitment RPC re-fetch sequence.
        price_cache.lock().await.put(mint.to_string(), (Instant::now(), _price));

        // Check if bot is running before attempting to buy
        {
            if let Some(control) = BOT_CONTROL.get() {
                let rs = control.running_state.lock().await;
                if !matches!(*rs, crate::api::BotRunningState::Running) {
                    debug!("Bot not running; skipping PumpPortal buy for {}", mint);
                    if sub_was_created && subscribed_idx.is_some() && subscribed_sub_id.is_some() {
                        let sender = &ws_control_senders[subscribed_idx.unwrap()];
                        let (u_tx, u_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
                        let _ = sender.send(WsRequest::Unsubscribe { sub_id: subscribed_sub_id.unwrap(), resp: u_tx }).await;
                        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), u_rx).await;
                    }
                    return Ok(());
                }
            }
        }

        // Lock holdings BRIEFLY to check — do NOT hold across the slow buy call.
        // Reserve the slot with in_flight_buys to prevent concurrent over-buying.
        {
            let holdings_guard = _holdings.lock().await;
            if holdings_guard.contains_key(mint) {
                debug!("Already holding {}; skipping duplicate PumpPortal buy", mint);
                if sub_was_created && subscribed_idx.is_some() && subscribed_sub_id.is_some() {
                    let sender = &ws_control_senders[subscribed_idx.unwrap()];
                    let (u_tx, u_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
                    let _ = sender.send(WsRequest::Unsubscribe { sub_id: subscribed_sub_id.unwrap(), resp: u_tx }).await;
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), u_rx).await;
                }
                return Ok(());
            }
            if holdings_guard.len() + in_flight_buys.load(Ordering::SeqCst) >= settings.max_holded_coins {
                info!("Max held coins reached ({} held + {} in-flight >= {}); skipping buy for {}", holdings_guard.len(), in_flight_buys.load(Ordering::SeqCst), settings.max_holded_coins, mint);
                if sub_was_created && subscribed_idx.is_some() && subscribed_sub_id.is_some() {
                    let sender = &ws_control_senders[subscribed_idx.unwrap()];
                    let (u_tx, u_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
                    let _ = sender.send(WsRequest::Unsubscribe { sub_id: subscribed_sub_id.unwrap(), resp: u_tx }).await;
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), u_rx).await;
                }
                return Ok(());
            }
            // Reserve the slot BEFORE releasing the lock
            in_flight_buys.fetch_add(1, Ordering::SeqCst);
        } // Holdings Mutex released — slot is reserved via in_flight_buys

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
                // Persist metadata into the created holding
                holding.metadata = offchain_meta_opt.clone();
                // Update holdings and trades similar to RPC path
                let buy_record = BuyRecord {
                    mint: mint.to_string(),
                    symbol: offchain_meta_opt.as_ref().and_then(|o| o.symbol.clone()),
                    name: offchain_meta_opt.as_ref().and_then(|o| o.name.clone()),
                    uri: offchain_meta_opt.as_ref().and_then(|o| o.image.clone()),
                    image: offchain_meta_opt.as_ref().and_then(|o| o.image.clone()),
                    creator: creator.to_string(),
                    detect_time,
                    buy_time: holding.buy_time,
                    buy_amount_sol: settings.buy_amount,
                    buy_amount_tokens: holding.amount,
                    buy_price: holding.buy_price,
                };

                // Update detected coin status
                {
                    let mut coins = detected_coins.lock().await;
                    if let Some(coin) = coins.iter_mut().find(|c| c.mint == mint) {
                        coin.status = "bought".to_string();
                        coin.buy_price = Some(holding.buy_price);
                    }
                }

                // Emit trade record
                {
                    let mut trades = trades_list.lock().await;
                    let token_divisor = 10f64.powi(holding.decimals as i32);
                    let amount_tokens = holding.amount as f64 / token_divisor;
                    trades.insert(
                        0,
                        api::TradeRecord {
                            mint: mint.to_string(),
                            symbol: offchain_meta_opt.as_ref().and_then(|o| o.symbol.clone()),
                            name: offchain_meta_opt.as_ref().and_then(|o| o.name.clone()),
                            image: offchain_meta_opt.as_ref().and_then(|o| o.image.clone()),
                            trade_type: "buy".to_string(),
                            timestamp: holding.buy_time.to_rfc3339(),
                            tx_signature: None,
                            amount_sol: holding.buy_cost_sol.unwrap_or(settings.buy_amount),
                            amount_tokens,
                            price_per_token: holding.buy_price,
                            profit_loss: None,
                            profit_loss_percent: None,
                            reason: None,
                            decimals: holding.decimals,
                            actual_sol_change: holding.buy_cost_sol.map(|c| -c),
                            tx_fee_sol: None,
                            simulated: !is_real,
                        },
                    );
                    if trades.len() > 200 { trades.truncate(200); }
                }

                trades_map.lock().await.insert(mint.to_string(), buy_record);
                _holdings.lock().await.insert(mint.to_string(), holding);
                in_flight_buys.fetch_sub(1, Ordering::SeqCst);

                // If we created a subscription pre-buy, keep it active and persist mapping
                if sub_was_created && subscribed_idx.is_some() && subscribed_sub_id.is_some() {
                    let mut map = sub_map.lock().await;
                    map.insert(mint.to_string(), (subscribed_idx.unwrap(), subscribed_sub_id.unwrap()));
                } else {
                    // Otherwise, try to subscribe now (best effort) to keep monitoring
                    if !ws_control_senders.is_empty() {
                        if let Some(idx) = select_healthy_wss(&ws_control_senders, settings).await {
                            let sender = &ws_control_senders[idx];
                            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<Result<u64, String>>();
                            let pump_prog = Pubkey::from_str(&settings.pump_fun_program)?;
                            if let Ok(mint_pk) = Pubkey::from_str(mint) {
                                let (curve_pda, _) = Pubkey::find_program_address(
                                    &[b"bonding-curve", mint_pk.as_ref()],
                                    &pump_prog,
                                );
                                let subscribe_req = WsRequest::Subscribe { account: curve_pda.to_string(), mint: mint.to_string(), resp: resp_tx };
                                if let Err(e) = sender.send(subscribe_req).await {
                                    log::warn!("Failed to send subscribe request for {}: {}", mint, e);
                                } else {
                                    match tokio::time::timeout(std::time::Duration::from_secs(5), resp_rx).await {
                                        Ok(Ok(Ok(sub_id))) => {
                                            debug!("Subscribed to {} on sub {}", mint, sub_id);
                                            let mut map = sub_map.lock().await;
                                            map.insert(mint.to_string(), (idx, sub_id));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }

                info!("PumpPortal fast-path buy succeeded for {}", mint);
            }
            Err(e) => {
                in_flight_buys.fetch_sub(1, Ordering::SeqCst);
                log::warn!("Failed to buy {} (pumpportal fast-path): {}", mint, e);
                bot_log!("warn", format!("Failed to buy token {}", mint), format!("{}", e));

                // Record failed buy attempt so it appears in Trading History
                {
                    let mut trades = trades_list.lock().await;
                    trades.insert(0, api::TradeRecord {
                        mint: mint.to_string(),
                        symbol: offchain_meta_opt.as_ref().and_then(|o| o.symbol.clone()),
                        name: offchain_meta_opt.as_ref().and_then(|o| o.name.clone()),
                        image: offchain_meta_opt.as_ref().and_then(|o| o.image.clone()),
                        trade_type: "buy".to_string(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        tx_signature: None,
                        amount_sol: settings.buy_amount,
                        amount_tokens: 0.0,
                        price_per_token: 0.0,
                        profit_loss: None,
                        profit_loss_percent: None,
                        reason: Some(format!("FAILED: {}", e)),
                        decimals: settings.default_token_decimals,
                        actual_sol_change: None,
                        tx_fee_sol: None,
                        simulated: !is_real,
                    });
                    if trades.len() > 200 { trades.truncate(200); }
                }

                // Update detected coin status to buy_failed
                {
                    let mut coins = detected_coins.lock().await;
                    if let Some(coin) = coins.iter_mut().find(|c| c.mint == mint) {
                        coin.status = "buy_failed".to_string();
                    }
                }

                // If we created a subscription and didn't buy, unsubscribe to free slot
                if sub_was_created && subscribed_idx.is_some() && subscribed_sub_id.is_some() {
                    let sender = &ws_control_senders[subscribed_idx.unwrap()];
                    let (u_tx, u_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
                    let _ = sender.send(WsRequest::Unsubscribe { sub_id: subscribed_sub_id.unwrap(), resp: u_tx }).await;
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), u_rx).await;
                }
            }
        }
    } else {
        warn!("No price available for {} yet; skipping pumpportal fast-path buy", mint);
        // Unsubscribe if we created a subscription but got no price and won't buy
        if sub_was_created && subscribed_idx.is_some() && subscribed_sub_id.is_some() {
            let sender = &ws_control_senders[subscribed_idx.unwrap()];
            let (u_tx, u_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
            let _ = sender.send(WsRequest::Unsubscribe { sub_id: subscribed_sub_id.unwrap(), resp: u_tx }).await;
            let _ = tokio::time::timeout(std::time::Duration::from_secs(3), u_rx).await;
        }
    }

    Ok(())
}
