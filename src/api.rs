use axum::{
    extract::{State, Json},
    extract::ws::{WebSocket, WebSocketUpgrade, Message},
    routing::{get, post},
    Router,
    response::IntoResponse,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::json;
use log::{info, warn};
use chrono::Utc;
use axum::http::StatusCode;
use tower_http::cors::CorsLayer;
use tokio::sync::broadcast;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    models::Holding,
    settings::Settings,
};

// Error message constants
const ERROR_BOT_MUST_BE_STOPPED: &str = "Bot must be stopped before changing settings or mode";

// Bot control structures
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BotRunningState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BotMode {
    DryRun,
    Real,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String, // "info", "warn", "error"
    pub message: String,
    pub details: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BotControl {
    pub running_state: Arc<Mutex<BotRunningState>>,
    pub mode: Arc<Mutex<BotMode>>,
    pub logs: Arc<Mutex<Vec<LogEntry>>>,
}

impl BotControl {
    pub fn new_with_mode(initial_mode: BotMode) -> Self {
        Self {
            running_state: Arc::new(Mutex::new(BotRunningState::Stopped)),
            mode: Arc::new(Mutex::new(initial_mode)),
            logs: Arc::new(Mutex::new(Vec::with_capacity(100))),
        }
    }

    pub async fn add_log(&self, level: &str, message: String, details: Option<String>) {
        let mut logs = self.logs.lock().await;
        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            level: level.to_string(),
            message,
            details,
        };
        logs.insert(0, entry);
        // Keep only last 100 logs
        if logs.len() > 100 {
            logs.truncate(100);
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DetectedCoin {
    pub mint: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub image: Option<String>,
    pub creator: String,
    pub bonding_curve: String,
    pub detected_at: String,
    pub metadata_uri: Option<String>,
    pub buy_price: Option<f64>,
    pub status: String, // "detected", "bought", "skipped"
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TradeRecord {
    pub mint: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub image: Option<String>,
    #[serde(rename = "type")]
    pub trade_type: String, // "buy" or "sell"
    pub timestamp: String,
    pub tx_signature: Option<String>,
    pub amount_sol: f64,
    pub amount_tokens: f64,
    pub price_per_token: f64,
    pub profit_loss: Option<f64>,
    pub profit_loss_percent: Option<f64>,
    pub reason: Option<String>, // "TP", "SL", "TIMEOUT", "MANUAL"
    /// Token decimals (e.g. 6 for most pump.fun tokens)
    #[serde(default = "default_trade_decimals")]
    pub decimals: u8,
    /// Actual SOL balance change from on-chain data (real mode only).
    /// For buys this is negative (SOL spent), for sells positive (SOL received).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_sol_change: Option<f64>,
    /// Transaction fee in SOL (gas + priority fee, from on-chain data, real mode only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_fee_sol: Option<f64>,
    /// Whether this trade was executed in dry-run (simulated) mode.
    #[serde(default)]
    pub simulated: bool,
}

fn default_trade_decimals() -> u8 { 6 }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    #[serde(rename = "detected-coin")]
    DetectedCoin { coin: DetectedCoin },
    #[serde(rename = "price-update")]
    PriceUpdate { mint: String, price: f64, profit_percent: Option<f64> },
    #[serde(rename = "holding-update")]
    HoldingUpdate { holdings: Vec<HoldingWithMint> },
}

#[derive(Clone)]
pub struct ApiState {
    pub settings: Arc<Mutex<Settings>>,
    pub stats: Arc<Mutex<BotStats>>,
    pub bot_control: Arc<BotControl>,
    pub detected_coins: Arc<Mutex<Vec<DetectedCoin>>>,
    pub trades: Arc<Mutex<Vec<TradeRecord>>>,
    pub ws_tx: broadcast::Sender<String>,
    /// Shared atomic flag that controls real vs dry-run trading.
    /// Updated by the mode-toggle API and read by buy/sell logic each tick.
    pub is_real_flag: Arc<AtomicBool>,
    /// Whether a wallet keypair was loaded at startup (required for real mode).
    pub has_keypair: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HoldingWithMint {
    pub mint: String,
    #[serde(flatten)]
    pub holding: Holding,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BotStats {
    pub total_buys: u64,
    pub total_sells: u64,
    pub total_profit: f64,
    pub current_holdings: Vec<HoldingWithMint>,
    pub uptime_secs: u64,
    pub last_activity: String,
    pub running_state: Option<String>,
    pub mode: Option<String>,
}

pub fn create_router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/stats", get(stats_handler))
        .route("/settings", get(get_settings_handler))
        .route("/settings", post(update_settings_handler))
        .route("/bot/state", get(get_bot_state_handler))
        .route("/bot/start", post(start_bot_handler))
        .route("/bot/stop", post(stop_bot_handler))
        .route("/bot/mode", post(set_bot_mode_handler))
        .route("/logs", get(get_logs_handler))
        .route("/detected-coins", get(get_detected_coins_handler))
        .route("/trades", get(get_trades_handler))
        .route("/ws", get(ws_handler))
        .with_state(state)
        .layer(CorsLayer::permissive())
}

async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "timestamp": Utc::now().to_rfc3339()
    }))
}

async fn stats_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let mut stats = state.stats.lock().await.clone();
    
    // Add bot state info
    let running_state = state.bot_control.running_state.lock().await;
    let mode = state.bot_control.mode.lock().await;
    
    stats.running_state = Some(format!("{:?}", *running_state).to_lowercase());
    stats.mode = Some(match *mode {
        BotMode::DryRun => "dry-run".to_string(),
        BotMode::Real => "real".to_string(),
    });
    
    Json(stats)
}

async fn get_settings_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let settings = state.settings.lock().await;
    Json(settings.clone())
}

async fn update_settings_handler(
    State(state): State<ApiState>,
    Json(updates): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    info!("Settings update request received: {:?}", updates);
    
    let bot_control = state.bot_control.clone();
    
    // Check if bot is stopped before allowing settings changes
    let running_state = state.bot_control.running_state.lock().await;
    if *running_state != BotRunningState::Stopped {
        warn!("{}", ERROR_BOT_MUST_BE_STOPPED);
        bot_control.add_log("warn", ERROR_BOT_MUST_BE_STOPPED.to_string(), None).await;
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": ERROR_BOT_MUST_BE_STOPPED
            }))
        );
    }
    drop(running_state);
    
    let mut current_settings = state.settings.lock().await;
    
    // Attempt to deserialize updates into a partial Settings struct
    let partial_settings: Settings = match serde_json::from_value(updates.clone()) {
        Ok(s) => s,
        Err(e) => {
            let error_msg = format!("Failed to parse settings updates: {}", e);
            warn!("{}", error_msg);
            bot_control.add_log("error", error_msg.clone(), None).await;
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "status": "error",
                    "message": error_msg
                }))
            );
        }
    };
    
    // Merge settings
    current_settings.merge(&partial_settings);
    
    // Validate merged settings
    if let Err(e) = current_settings.validate() {
        let error_msg = format!("Settings validation failed: {}", e);
        warn!("{}", error_msg);
        bot_control.add_log("error", error_msg.clone(), None).await;
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": error_msg
            }))
        );
    }
    
    // Get config path from environment or use default
    let config_path = std::env::var("SOL_BEAST_CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    
    // Save updated settings to config.toml
    if let Err(e) = current_settings.save_to_file(&config_path) {
        let error_msg = format!("Failed to save settings to file: {}", e);
        warn!("{}", error_msg);
        bot_control.add_log("error", error_msg.clone(), None).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "status": "error",
                "message": error_msg
            }))
        );
    }
    
    info!("Settings updated and saved successfully.");
    bot_control.add_log("info", "Settings updated and saved successfully".to_string(), None).await;
    
    (
        StatusCode::OK,
        Json(json!({
            "status": "success",
            "message": "Settings updated successfully"
        }))
    )
}

// Bot control endpoints
async fn get_bot_state_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let running_state = state.bot_control.running_state.lock().await;
    let mode = state.bot_control.mode.lock().await;
    
    Json(json!({
        "running_state": format!("{:?}", *running_state).to_lowercase(),
        "mode": match *mode {
            BotMode::DryRun => "dry-run",
            BotMode::Real => "real",
        }
    }))
}

async fn start_bot_handler(
    State(state): State<ApiState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut running_state = state.bot_control.running_state.lock().await;
    
    if *running_state != BotRunningState::Stopped {
        warn!("Attempted to start bot while not stopped: {:?}", *running_state);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": "Bot is not stopped"
            }))
        );
    }
    
    *running_state = BotRunningState::Starting;
    drop(running_state);
    
    let bot_control = state.bot_control.clone();
    let mode = bot_control.mode.lock().await.clone();
    
    info!("Bot starting in {:?} mode", mode);
    bot_control.add_log(
        "info",
        format!("Bot starting in {:?} mode", mode),
        None
    ).await;
    
    // Simulate startup delay
    let bot_control_clone = bot_control.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let mut state = bot_control_clone.running_state.lock().await;
        *state = BotRunningState::Running;
        bot_control_clone.add_log(
            "info",
            "Bot is now running".to_string(),
            None
        ).await;
        info!("Bot state changed to Running");
    });
    
    (
        StatusCode::OK,
        Json(json!({
            "status": "success",
            "message": "Bot is starting"
        }))
    )
}

async fn stop_bot_handler(
    State(state): State<ApiState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut running_state = state.bot_control.running_state.lock().await;
    
    if *running_state != BotRunningState::Running {
        warn!("Attempted to stop bot while not running: {:?}", *running_state);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": "Bot is not running"
            }))
        );
    }
    
    *running_state = BotRunningState::Stopping;
    drop(running_state);
    
    let bot_control = state.bot_control.clone();
    
    // Check if there are active holdings to drain
    let holdings_count = state.stats.lock().await.current_holdings.len();
    
    if holdings_count == 0 {
        // No holdings — stop immediately
        let mut rs = bot_control.running_state.lock().await;
        *rs = BotRunningState::Stopped;
        drop(rs);
        info!("Bot stopped (no active holdings)");
        bot_control.add_log(
            "info",
            "Bot stopped (no active holdings)".to_string(),
            None
        ).await;
        (
            StatusCode::OK,
            Json(json!({
                "status": "success",
                "message": "Bot stopped"
            }))
        )
    } else {
        // Holdings exist — enter draining mode, monitor will transition to Stopped when empty
        info!("Bot stopping — draining {} active holdings (TP/SL/Timeout)", holdings_count);
        bot_control.add_log(
            "info",
            format!("Bot stopping — draining {} active holding(s). Will stop after all positions close via TP/SL/Timeout.", holdings_count),
            None
        ).await;
        (
            StatusCode::OK,
            Json(json!({
                "status": "success",
                "message": format!("Bot is draining {} holding(s) before stopping", holdings_count)
            }))
        )
    }
}

#[derive(Debug, serde::Deserialize)]
struct SetModeRequest {
    mode: String,
}

async fn set_bot_mode_handler(
    State(state): State<ApiState>,
    Json(payload): Json<SetModeRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let running_state = state.bot_control.running_state.lock().await;
    
    // Can only change mode when stopped
    if *running_state != BotRunningState::Stopped {
        warn!("Attempted to change mode while bot is {:?}", *running_state);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": ERROR_BOT_MUST_BE_STOPPED
            }))
        );
    }
    drop(running_state);
    
    let new_mode = match payload.mode.as_str() {
        "dry-run" => BotMode::DryRun,
        "real" => {
            if !state.has_keypair {
                warn!("Attempted to switch to real mode without a loaded keypair");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "status": "error",
                        "message": "Cannot switch to real mode: no wallet keypair configured. Set wallet_private_key_string or equivalent in config.toml and restart."
                    }))
                );
            }
            BotMode::Real
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "status": "error",
                    "message": "Invalid mode. Must be 'dry-run' or 'real'"
                }))
            );
        }
    };
    
    let mut mode = state.bot_control.mode.lock().await;
    *mode = new_mode.clone();
    drop(mode);
    
    // Sync the atomic is_real flag so buy/sell logic picks up the change immediately
    let is_now_real = matches!(new_mode, BotMode::Real);
    state.is_real_flag.store(is_now_real, Ordering::SeqCst);
    
    let bot_control = state.bot_control.clone();
    info!("Bot mode changed to {:?} (is_real={})", new_mode, is_now_real);
    bot_control.add_log(
        "info",
        format!("Bot mode changed to {:?} — trading will {} use real transactions", new_mode, if is_now_real { "now" } else { "NOT" }),
        None
    ).await;
    
    (
        StatusCode::OK,
        Json(json!({
            "status": "success",
            "mode": payload.mode
        }))
    )
}

async fn get_logs_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let logs = state.bot_control.logs.lock().await;
    Json(json!({
        "logs": logs.clone()
    }))
}

async fn get_detected_coins_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let coins = state.detected_coins.lock().await;
    let total = crate::TOTAL_DETECTED_COINS.load(Ordering::Relaxed);
    info!("Serving /detected-coins: {} coins (total detected: {})", coins.len(), total);
    Json(json!({
        "coins": coins.clone(),
        "total": total
    }))
}

async fn get_trades_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let trades = state.trades.lock().await;
    Json(trades.clone())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: ApiState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.ws_tx.subscribe();
    
    // Send initial data
    let initial_data = {
        let coins = state.detected_coins.lock().await;
        let holdings = state.stats.lock().await.current_holdings.clone();
        let total = crate::TOTAL_DETECTED_COINS.load(Ordering::Relaxed);
        json!({
            "type": "initial",
            "detected_coins": coins.clone(),
            "holdings": holdings,
            "total_detected_coins": total,
        }).to_string()
    };
    
    if let Err(e) = sender.send(Message::Text(initial_data)).await {
        warn!("WebSocket send error: {}", e);
        return;
    }
    
    // Spawn task to handle incoming messages (ping/pong)
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });
    
    // Handle incoming messages
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                Message::Ping(data) => {
                    // Respond to ping (handled automatically by axum)
                    info!("Received ping: {} bytes", data.len());
                }
                _ => {}
            }
        }
    });
    
    // Wait for either task to finish
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }
}

