mod api;
mod error;
mod buyer;

mod monitor;
mod rpc;
mod state;
mod ws;
// mod shyft_monitor; (Removed)
mod models;
mod buy_wrapper;
mod price_subscriber;
mod trade_logger;

// Use core library modules
use sol_beast_core::{
    error::CoreError, 
    settings, 
    native::http::NativeHttpClient, 
    native::rpc_impl::NativeRpcClient, 
    pipeline::process_new_token,
    detection::{NewTokenDetector, DetectionConfig},
};
type AppError = CoreError;
use api::{create_router, ApiState, BotStats, TradeRecord};
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

// Debounce for repetitive "max held coins" logs so we don't spam the logs.
static LAST_MAX_HELD_LOG: Lazy<tokio::sync::Mutex<Option<Instant>>> =
    Lazy::new(|| tokio::sync::Mutex::new(None));
const MAX_HELD_LOG_DEBOUNCE_SECS: u64 = 60;
const API_PORT: u16 = 8080;
const API_HOST: &str = "0.0.0.0";
use sol_beast_core::settings::{load_keypair_from_env_var, parse_private_key_string, Settings};
use crate::{
    models::{Holding, PriceCache},
    state::BuyRecord,
    price_subscriber::CliPriceSubscriber,
};
use chrono::Utc;
use log::{debug, error, info, warn};
use lru::LruCache;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signer;
use std::{collections::HashMap, fs, sync::Arc, time::Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::time::Duration;

/// Select the healthiest WSS endpoint with available slots.
/// Returns None if all endpoints are degraded or unavailable.
#[allow(dead_code)]
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
    let http_client = Arc::new(NativeHttpClient::new());
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

    let is_real = std::env::args().any(|arg| arg == "--real");
    // Load real keypair either from path or from JSON in config (optional)
    // Prefer base64 env var for keypairs to avoid storing keys on disk.
    let keypair: Option<std::sync::Arc<Keypair>> = if is_real {
        if let Some(bytes) = load_keypair_from_env_var("SOL_BEAST_KEYPAIR_B64") {
            Some(std::sync::Arc::new(
                Keypair::try_from(bytes.as_slice())
                    .map_err(|e| AppError::InvalidKeypair(e.to_string()))?,
            ))
        } else if let Some(pk_string) = settings.wallet_private_key_string.clone() {
            let bytes =
                parse_private_key_string(&pk_string).map_err(AppError::InvalidKeypair)?;
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
        load_keypair_from_env_var("SOL_BEAST_SIMULATE_KEYPAIR_B64")
    {
        Some(std::sync::Arc::new(
            Keypair::try_from(bytes.as_slice())
                .map_err(|e| AppError::InvalidKeypair(e.to_string()))?,
        ))
    } else if let Some(pk_string) = settings.simulate_wallet_private_key_string.clone() {
        let bytes =
            parse_private_key_string(&pk_string).map_err(AppError::InvalidKeypair)?;
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
    let settings_clone_monitor = settings.clone();
    let keypair_clone_monitor = keypair.clone();
    let simulate_keypair_clone = simulate_keypair.clone();
    let trades_map_clone_monitor = trades_map.clone();
    
    // Create new token detector with metrics
    let detection_config = DetectionConfig::from_settings(&settings);
    let detector = Arc::new(NewTokenDetector::new(detection_config));
    info!("Created new token detector with WebSocket-level filtering");

    // Channel for receiving WSS messages is created below
    // (removed Shyft channels)

    // Channel to control WSS (subscribe/unsubscribe)
    let (ws_control_tx, _ws_control_rx) = mpsc::channel::<WsRequest>(100);

    let price_subscriber = Arc::new(Mutex::new(CliPriceSubscriber::new(
        price_cache.clone(),
        ws_control_tx.clone(),
        settings.price_cache_ttl_secs,
        settings.pump_fun_program.clone(),
    )));

    // Spawn Standard WSS Monitors - USE ALL AVAILABLE WSS URLs in parallel
    // This significantly improves detection reliability and speed by:
    // 1. Avoiding single point of failure if one WSS connection drops
    // 2. Receiving notifications from multiple sources simultaneously
    // 3. Reducing latency by using geographically distributed endpoints
    let holdings_clone_ws = holdings.clone();
    let price_cache_clone_ws = price_cache.clone();
    let settings_clone_ws = settings.clone();
    
    // Channel for raw WSS messages (mostly logs)
    let (tx, mut rx) = mpsc::channel::<String>(1000); // Increased buffer for multiple connections
    
    // Get all configured WebSocket URLs
    let wss_urls: Vec<String> = if settings.solana_ws_urls.is_empty() {
        // Fallback to deriving from RPC URLs
        settings
            .solana_rpc_urls
            .iter()
            .map(|rpc_url| rpc_url.replace("https://", "wss://").replace("http://", "ws://"))
            .collect()
    } else {
        settings.solana_ws_urls.clone()
    };
    
    info!(
        "Spawning {} WebSocket connections for parallel memecoin detection",
        wss_urls.len()
    );
    
    // Spawn a separate WebSocket task for each URL
    let mut ws_handles = Vec::new();
    for (idx, wss_url) in wss_urls.iter().enumerate() {
        let seen_clone = seen.clone();
        let holdings_clone = holdings_clone_ws.clone();
        let price_cache_clone = price_cache_clone_ws.clone();
        let settings_clone = settings_clone_ws.clone();
        let tx_clone = tx.clone();
        let wss_url_clone = wss_url.clone();
        
        // Create a dedicated control channel for each WebSocket connection
        let (_ws_control_tx_local, ws_control_rx_local) = mpsc::channel::<WsRequest>(100);
        
        let ws_handle = tokio::spawn(async move {
            info!("Starting WebSocket connection #{} to {}", idx, wss_url_clone);
            if let Err(e) = ws::run_ws(
                &wss_url_clone,
                tx_clone,
                seen_clone,
                holdings_clone,
                price_cache_clone,
                ws_control_rx_local,
                settings_clone,
            )
            .await
            {
                error!("WSS task #{} failed ({}): {}", idx, wss_url_clone, e);
            }
        });
        
        ws_handles.push(ws_handle);
    }
    
    // Use the first WebSocket's control channel for price subscription
    // (in the future, we can use all of them for even better redundancy)

    // Now spawn price monitoring (after shyft_control_tx exists so monitor
    // can unsubscribe subscriptions on sell).
    let rpc_client_clone = rpc_client.clone();
    let simulate_keypair_clone_for_monitor = simulate_keypair_clone.clone();
    let trades_list_clone_for_monitor = trades_list.clone();
    let bot_control_for_monitor = bot_control.clone();
    
    let monitor_handle = tokio::spawn(async move {
        monitor::monitor_holdings(
            holdings_clone_monitor,
            rpc_client_clone,
            is_real,
            keypair_clone_monitor.as_deref(),
            simulate_keypair_clone_for_monitor.as_deref(),
            settings_clone_monitor,
            trades_map_clone_monitor,
            price_subscriber.clone(),
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

    // Spawn periodic detection metrics logging
    let detector_for_metrics = detector.clone();
    let metrics_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            detector_for_metrics.log_metrics();
        }
    });

    // Process messages
    let detector_clone = detector.clone();
    while let Some(msg_json) = rx.recv().await {
        // Parse raw JSON from WSS
        if let Err(e) = process_wss_message(
            &msg_json,
            &seen,
            &holdings,
            &rpc_client,
            &http_client,
            is_real,
            keypair.as_deref(),
            simulate_keypair.as_deref(),

            &settings,
            ws_control_tx.clone(),
            trades_map.clone(),
            detected_coins.clone(),
            trades_list.clone(),
            &detector_clone,
        )
        .await
        {
            error!("process_wss_message failed: {}", e);
            bot_log!(
                "error",
                "Failed to process WSS message",
                format!("{}", e)
            );
        }
    }

    // If the message processing loop ends, await all spawned tasks to ensure panics are caught.
    // This will block until all tasks complete or panic.
    info!("Main message processing loop ended. Awaiting background tasks...");

    let mut all_handles = vec![monitor_handle, api_sync_handle, api_server_handle, metrics_handle];
    all_handles.extend(ws_handles); // Add all WebSocket handles

    for handle in all_handles {
        if let Err(e) = handle.await {
            error!("A background task panicked or exited unexpectedly: {:?}", e);
            bot_log!("error", "Background task failed", format!("{:?}", e));
        }
    }

    Ok(())
}

async fn process_wss_message(
    msg_json: &str,
    seen: &Arc<Mutex<LruCache<String, ()>>>,
    holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    rpc_client: &Arc<RpcClient>,
    http_client: &Arc<NativeHttpClient>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    settings: &Arc<Settings>,
    ws_control_tx: mpsc::Sender<WsRequest>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    detected_coins: Arc<tokio::sync::Mutex<Vec<api::DetectedCoin>>>,
    trades_list: Arc<tokio::sync::Mutex<Vec<api::TradeRecord>>>,
    detector: &NewTokenDetector,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let v: serde_json::Value = serde_json::from_str(msg_json)?;
    
    // Use the detector to check if this notification should be processed
    match detector.should_process_notification(&v) {
        Ok(Some(signature)) => {
            // Check for duplicates
            if seen.lock().await.put(signature.clone(), ()).is_some() {
                // Already seen - record duplicate and skip
                detector.metrics().record_duplicate();
                return Ok(());
            }
            
            // Check if we've reached max holdings
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
                return Ok(());
            }

            let detect_time = Utc::now();
            
            // Process the new token using the detector
            if let Err(e) = handle_new_token_from_sig(
                &signature,
                rpc_client,
                http_client,
                settings,
                detect_time,
                detected_coins,
                holdings.clone(),
                is_real,
                keypair,
                simulate_keypair,
                ws_control_tx.clone(),
                trades_map.clone(),
                trades_list.clone(),
            ).await {
                error!("handle_new_token failed for {}: {}", signature, e);
                return Err(e);
            }
        }
        Ok(None) => {
            // Filtered out - nothing to do
        }
        Err(e) => {
            // Parse error - log and skip
            debug!("Error processing notification: {:?}", e);
        }
    }
    
    Ok(())
}

async fn handle_new_token_from_sig(
    signature: &str,
    rpc_client: &Arc<RpcClient>,
    http_client: &Arc<NativeHttpClient>,
    settings: &Arc<Settings>,
    detect_time: chrono::DateTime<Utc>,
    detected_coins: Arc<tokio::sync::Mutex<Vec<api::DetectedCoin>>>,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    _ws_control_tx: mpsc::Sender<WsRequest>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    trades_list: Arc<tokio::sync::Mutex<Vec<api::TradeRecord>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Phase 5: Use unified pipeline logic
    // Wrap the native solana_client RpcClient in our core-compatible NativeRpcClient
    let native_rpc = NativeRpcClient::from_arc(rpc_client.clone());
    
    // We pass the signature directly. The pipeline handles fetching tx, metadata, etc.
    let result = process_new_token(
        signature.to_string(),
        &native_rpc,
        http_client.as_ref(),
        settings,
    ).await?;

    // Log token detection to API (maintain CLI behavior)
    bot_log!(
        "info",
        format!("New token detected: {}", result.name.as_deref().unwrap_or("Unknown")),
        format!("Mint: {}, Creator: {}", result.mint, result.creator)
    );

    // Add to detected coins list
    {
        let mut coins = detected_coins.lock().await;
        coins.insert(
            0,
            api::DetectedCoin {
                mint: result.mint.clone(),
                name: result.name.clone(),
                symbol: result.symbol.clone(),
                image: result.image_uri.clone(),
                creator: result.creator.clone(),
                bonding_curve: result.bonding_curve.clone(),
                detected_at: detect_time.to_rfc3339(),
                metadata_uri: None, // Simplified, pipeline handles metadata extraction
                buy_price: Some(result.buy_price_sol),
                status: if result.should_buy { "buy_candidate" } else { "rejected" }.to_string(),
            },
        );
        // Keep only last 100 detected coins
        if coins.len() > 100 {
            coins.truncate(100);
        }
    }

    // Pass through pipeline decision
    // Pass through pipeline decision
    if result.should_buy {
        info!("Pipeline approved buy for {} (Reason: {})", result.mint, result.evaluation_reason);
        
        // Execute the buy
        let buy_cfg = sol_beast_core::buy_service::BuyConfig {
            mint: result.mint.clone(),
            sol_amount: settings.buy_amount,
            current_price_sol: result.buy_price_sol,
            bonding_curve_state: result.bonding_curve_state.clone(),
        };

        let signer = if is_real { 
            keypair.map(|k| {
                let owned_kp = Keypair::try_from(&k.to_bytes()[..]).unwrap();
                sol_beast_core::native::transaction_signer::NativeTransactionSigner::new(owned_kp)
            })
        } else {
            simulate_keypair.map(|k| {
                let owned_kp = Keypair::try_from(&k.to_bytes()[..]).unwrap();
                sol_beast_core::native::transaction_signer::NativeTransactionSigner::new(owned_kp)
            })
        };

        if let Some(signer) = signer {
            match sol_beast_core::buy_service::BuyService::execute_buy(
                buy_cfg,
                &native_rpc,
                &signer,
                settings,
            ).await {
                Ok(buy_res) => {
                    let buy_time = chrono::DateTime::from_timestamp(buy_res.timestamp, 0).unwrap_or_else(Utc::now);
                    
                    // Create Holding and insert
                    let holding = Holding {
                        mint: result.mint.clone(),
                        amount: buy_res.token_amount,
                        buy_price: result.buy_price_sol,
                        buy_time,
                        creator: Some(result.creator.clone()),
                        metadata: Some(crate::models::OffchainTokenMetadata {
                            name: result.name.clone(),
                            symbol: result.symbol.clone(),
                            image: result.image_uri.clone(),
                            description: result.description.clone(),
                            extras: None,
                        }),
                        onchain_raw: None,
                        onchain: None,
                    };
                    
                    holdings.lock().await.insert(result.mint.clone(), holding.clone());
                    
                    // Add to trades_map for PNL tracking
                    let buy_rec = BuyRecord {
                        mint: result.mint.clone(),
                        symbol: result.symbol.clone(),
                        name: result.name.clone(),
                        uri: None, // metadata uri not easily available from pipeline result
                        image: result.image_uri.clone(),
                        creator: result.creator.clone(),
                        detect_time,
                        buy_time,
                        buy_amount_sol: buy_res.sol_spent,
                        buy_amount_tokens: buy_res.token_amount,
                        buy_price: result.buy_price_sol,
                        buy_signature: Some(buy_res.transaction_signature.clone()),
                    };
                    trades_map.lock().await.insert(result.mint.clone(), buy_rec.clone());

                    // Add buy trade record to API
                    {
                        let mut trades = trades_list.lock().await;
                        trades.insert(0, TradeRecord {
                            mint: result.mint.clone(),
                            symbol: result.symbol.clone(),
                            name: result.name.clone(),
                            image: result.image_uri.clone(),
                            trade_type: "buy".to_string(),
                            timestamp: buy_time.to_rfc3339(),
                            tx_signature: Some(buy_res.transaction_signature.clone()),
                            amount_sol: buy_res.sol_spent,
                            amount_tokens: buy_res.token_amount as f64 / 1_000_000.0,
                            price_per_token: result.buy_price_sol,
                            profit_loss: None,
                            profit_loss_percent: None,
                            reason: Some(result.evaluation_reason.clone()),
                        });
                    }

                    // Log to file
                    trade_logger::log_buy(&buy_rec);

                    info!("Successfully bought token: {}", result.mint);
                    bot_log!(
                        "info",
                        format!("Successfully bought token {}", result.mint),
                        format!("Amount: {} tokens, Price: {:.18} SOL", buy_res.token_amount, result.buy_price_sol)
                    );
                }
                Err(e) => {
                    error!("Buy execution failed for {}: {}", result.mint, e);
                    bot_log!("error", format!("Buy failed for {}", result.mint), format!("{}", e));
                }
            }
        } else {
            warn!("No keypair available for buy execution");
        }
    } else {
        info!("Pipeline rejected {} (Reason: {})", result.mint, result.evaluation_reason);
    }
    
    Ok(())
}

