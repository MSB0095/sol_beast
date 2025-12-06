mod api;
mod error;
mod buyer;
mod dev_fee;
mod helius_sender;
mod monitor;
mod rpc;
mod settings;
mod state;
mod ws;
mod shyft_monitor;

// Use core library modules
use sol_beast_core::{models, idl, tx_builder, error::CoreError};
type AppError = CoreError;
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
use sol_beast_core::settings::{load_keypair_from_env_var, parse_private_key_string, Settings};
use crate::{
    models::{Holding, PriceCache},
    state::BuyRecord,
};
use chrono::Utc;
use log::{debug, error, info, warn};
use lru::LruCache;
use mpl_token_metadata::accounts::Metadata as OnchainMetadataRaw;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signer;
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
    let price_cache_clone_monitor = price_cache.clone();
    let settings_clone_monitor = settings.clone();
    let keypair_clone_monitor = keypair.clone();
    let simulate_keypair_clone = simulate_keypair.clone();
    let trades_map_clone_monitor = trades_map.clone();

    // Channel for receiving new token notifications from Shyft
    let (tx, mut rx) = mpsc::channel::<shyft_monitor::ShyftMonitorMessage>(100);
    let (shyft_control_tx, shyft_control_rx) = mpsc::channel::<shyft_monitor::ShyftControlMessage>(100);

    // Spawn Shyft Monitor
    let holdings_clone_shyft = holdings.clone();
    let price_cache_clone_shyft = price_cache.clone();
    let settings_clone_shyft = settings.clone();
    let tx_clone = tx.clone();
    
    let shyft_handle = tokio::spawn(async move {
        shyft_monitor::start_shyft_monitor(
            settings_clone_shyft,
            holdings_clone_shyft,
            price_cache_clone_shyft,
            tx_clone,
            shyft_control_rx,
        ).await;
    });

    // Now spawn price monitoring (after shyft_control_tx exists so monitor
    // can unsubscribe subscriptions on sell).
    let rpc_client_clone = rpc_client.clone();
    let shyft_control_tx_clone_for_monitor = shyft_control_tx.clone();
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
            shyft_control_tx_clone_for_monitor,
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
        if let Err(e) = process_shyft_message(
            msg,
            &seen,
            &holdings,
            &rpc_client,
            is_real,
            keypair.as_deref(),
            simulate_keypair.as_deref(),
            &price_cache,
            &settings,
            shyft_control_tx.clone(),
            trades_map.clone(),
            detected_coins.clone(),
            trades_list.clone(),
        )
        .await
        {
            error!("process_shyft_message failed: {}", e);
            bot_log!(
                "error",
                "Failed to process Shyft message",
                format!("{}", e)
            );
        }
    }

    // If the message processing loop ends, await all spawned tasks to ensure panics are caught.
    // This will block until all tasks complete or panic.
    info!("Main message processing loop ended. Awaiting background tasks...");

    let all_handles = vec![monitor_handle, api_sync_handle, api_server_handle, shyft_handle];

    for handle in all_handles {
        if let Err(e) = handle.await {
            error!("A background task panicked or exited unexpectedly: {:?}", e);
            bot_log!("error", "Background task failed", format!("{:?}", e));
        }
    }

    Ok(())
}

async fn process_shyft_message(
    msg: shyft_monitor::ShyftMonitorMessage,
    seen: &Arc<Mutex<LruCache<String, ()>>>,
    holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    rpc_client: &Arc<RpcClient>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
    shyft_control_tx: mpsc::Sender<shyft_monitor::ShyftControlMessage>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    detected_coins: Arc<tokio::sync::Mutex<Vec<api::DetectedCoin>>>,
    trades_list: Arc<tokio::sync::Mutex<Vec<api::TradeRecord>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match msg {
        shyft_monitor::ShyftMonitorMessage::NewToken(tx) => {
            let signature = tx.signature.clone();
            if seen.lock().await.put(signature.clone(), ()).is_none() {
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
                if let Err(e) = handle_new_token(
                    &tx,
                    holdings,
                    rpc_client,
                    is_real,
                    keypair,
                    simulate_keypair,
                    price_cache,
                    settings,
                    shyft_control_tx,
                    detect_time,
                    trades_map,
                    detected_coins,
                    trades_list,
                )
                .await
                {
                    error!("handle_new_token failed for {}: {}", signature, e);
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

async fn handle_new_token(
    tx: &sol_beast_core::shyft::ShyftTransaction,
    holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    rpc_client: &Arc<RpcClient>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
    shyft_control_tx: mpsc::Sender<shyft_monitor::ShyftControlMessage>,
    detect_time: chrono::DateTime<Utc>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    detected_coins: Arc<tokio::sync::Mutex<Vec<api::DetectedCoin>>>,
    trades_list: Arc<tokio::sync::Mutex<Vec<api::TradeRecord>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::sync::oneshot;
    
    // Extract details from ShyftTransaction
    // We look for the instruction that calls pump.fun
    let pump_prog = &settings.pump_fun_program;
    let instruction = tx.instructions.iter().find(|ix| ix.program_id == *pump_prog);
    
    let (creator, mint, curve_pda, holder_addr) = if let Some(ix) = instruction {
        // Assuming standard create instruction layout
        // Accounts: [Mint, MintAuth, BondingCurve, BondingCurveVault, ..., Creator]
        // Expected account layout for pump.fun create instruction:
        // [0] = Mint account
        // [1] = Mint authority  
        // [2] = Bonding curve account
        // [3] = Associated bonding curve token account
        // [4] = Global account
        // [5] = MPL token metadata
        // [6] = Metadata account
        // [7] = Creator/payer account
        // This layout is based on the pump.fun program's create instruction format.
        // If the instruction format changes, this will need to be updated.
        if ix.accounts.len() > 7 {
            (
                ix.accounts[7].clone(), // Creator
                ix.accounts[0].clone(), // Mint
                ix.accounts[2].clone(), // Bonding Curve
                ix.accounts[7].clone(), // Holder (Creator)
            )
        } else {
            // Fallback to RPC if accounts are missing or structure is different
            let (c, m, cp, h, is_init) = rpc::fetch_transaction_details(&tx.signature, rpc_client, settings).await?;
            if !is_init { return Ok(()); }
            (c, m, cp, h)
        }
    } else {
        return Ok(());
    };

    let signature = &tx.signature;
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
            // Note: The old WebSocket subscription code (~350 lines) has been removed.
            // Price updates are now provided by the Shyft GraphQL WebSocket monitor which
            // subscribes to bonding curve accounts and updates the shared price_cache.
            // This centralized approach eliminates complex per-token subscription management
            // and provides more reliable price updates through Shyft's infrastructure.
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
