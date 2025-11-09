use axum::{
    extract::{State, Json},
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

use crate::{
    models::Holding,
    settings::Settings,
};

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
            running_state: Arc::new(Mutex::new(BotRunningState::Running)),
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
}

#[derive(Clone)]
pub struct ApiState {
    pub settings: Arc<Mutex<Settings>>,
    pub stats: Arc<Mutex<BotStats>>,
    pub bot_control: Arc<BotControl>,
    pub detected_coins: Arc<Mutex<Vec<DetectedCoin>>>,
    pub trades: Arc<Mutex<Vec<TradeRecord>>>,
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
    
    // Apply updates to current settings
    // This is a basic merge; for more complex scenarios, consider a dedicated merge logic
    // For simplicity, we'll just replace fields that are present in the partial_settings
    // This requires manually updating each field. A better approach would be to use a crate
    // like `serde_merge` or implement a custom `merge` method on `Settings`.
    
    // Example of merging:
    if partial_settings.solana_rpc_urls != current_settings.solana_rpc_urls {
        current_settings.solana_rpc_urls = partial_settings.solana_rpc_urls;
    }
    if partial_settings.solana_ws_urls != current_settings.solana_ws_urls {
        current_settings.solana_ws_urls = partial_settings.solana_ws_urls;
    }
    if partial_settings.pump_fun_program != current_settings.pump_fun_program {
        current_settings.pump_fun_program = partial_settings.pump_fun_program;
    }
    if partial_settings.metadata_program != current_settings.metadata_program {
        current_settings.metadata_program = partial_settings.metadata_program;
    }
    if partial_settings.buy_amount != current_settings.buy_amount {
        current_settings.buy_amount = partial_settings.buy_amount;
    }
    if partial_settings.tp_percent != current_settings.tp_percent {
        current_settings.tp_percent = partial_settings.tp_percent;
    }
    if partial_settings.sl_percent != current_settings.sl_percent {
        current_settings.sl_percent = partial_settings.sl_percent;
    }
    if partial_settings.timeout_secs != current_settings.timeout_secs {
        current_settings.timeout_secs = partial_settings.timeout_secs;
    }
    if partial_settings.price_cache_ttl_secs != current_settings.price_cache_ttl_secs {
        current_settings.price_cache_ttl_secs = partial_settings.price_cache_ttl_secs;
    }
    if partial_settings.cache_capacity != current_settings.cache_capacity {
        current_settings.cache_capacity = partial_settings.cache_capacity;
    }
    if partial_settings.max_holded_coins != current_settings.max_holded_coins {
        current_settings.max_holded_coins = partial_settings.max_holded_coins;
    }
    if partial_settings.price_source != current_settings.price_source {
        current_settings.price_source = partial_settings.price_source;
    }
    if partial_settings.rpc_rotate_interval_secs != current_settings.rpc_rotate_interval_secs {
        current_settings.rpc_rotate_interval_secs = partial_settings.rpc_rotate_interval_secs;
    }
    if partial_settings.helius_sender_enabled != current_settings.helius_sender_enabled {
        current_settings.helius_sender_enabled = partial_settings.helius_sender_enabled;
    }
    if partial_settings.helius_api_key != current_settings.helius_api_key {
        current_settings.helius_api_key = partial_settings.helius_api_key;
    }
    if partial_settings.helius_sender_endpoint != current_settings.helius_sender_endpoint {
        current_settings.helius_sender_endpoint = partial_settings.helius_sender_endpoint;
    }
    if partial_settings.helius_use_swqos_only != current_settings.helius_use_swqos_only {
        current_settings.helius_use_swqos_only = partial_settings.helius_use_swqos_only;
    }
    if partial_settings.helius_use_dynamic_tips != current_settings.helius_use_dynamic_tips {
        current_settings.helius_use_dynamic_tips = partial_settings.helius_use_dynamic_tips;
    }
    if partial_settings.helius_min_tip_sol != current_settings.helius_min_tip_sol {
        current_settings.helius_min_tip_sol = partial_settings.helius_min_tip_sol;
    }
    if partial_settings.helius_priority_fee_multiplier != current_settings.helius_priority_fee_multiplier {
        current_settings.helius_priority_fee_multiplier = partial_settings.helius_priority_fee_multiplier;
    }
    if partial_settings.enable_safer_sniping != current_settings.enable_safer_sniping {
        current_settings.enable_safer_sniping = partial_settings.enable_safer_sniping;
    }
    if partial_settings.min_tokens_threshold != current_settings.min_tokens_threshold {
        current_settings.min_tokens_threshold = partial_settings.min_tokens_threshold;
    }
    if partial_settings.max_sol_per_token != current_settings.max_sol_per_token {
        current_settings.max_sol_per_token = partial_settings.max_sol_per_token;
    }
    if partial_settings.min_liquidity_sol != current_settings.min_liquidity_sol {
        current_settings.min_liquidity_sol = partial_settings.min_liquidity_sol;
    }
    if partial_settings.max_liquidity_sol != current_settings.max_liquidity_sol {
        current_settings.max_liquidity_sol = partial_settings.max_liquidity_sol;
    }
    if partial_settings.bonding_curve_strict != current_settings.bonding_curve_strict {
        current_settings.bonding_curve_strict = partial_settings.bonding_curve_strict;
    }
    if partial_settings.bonding_curve_log_debounce_secs != current_settings.bonding_curve_log_debounce_secs {
        current_settings.bonding_curve_log_debounce_secs = partial_settings.bonding_curve_log_debounce_secs;
    }
    if partial_settings.slippage_bps != current_settings.slippage_bps {
        current_settings.slippage_bps = partial_settings.slippage_bps;
    }
    if partial_settings.wallet_keypair_path != current_settings.wallet_keypair_path {
        current_settings.wallet_keypair_path = partial_settings.wallet_keypair_path;
    }
    if partial_settings.wallet_keypair_json != current_settings.wallet_keypair_json {
        current_settings.wallet_keypair_json = partial_settings.wallet_keypair_json;
    }
    if partial_settings.wallet_private_key_string != current_settings.wallet_private_key_string {
        current_settings.wallet_private_key_string = partial_settings.wallet_private_key_string;
    }
    if partial_settings.simulate_wallet_keypair_json != current_settings.simulate_wallet_keypair_json {
        current_settings.simulate_wallet_keypair_json = partial_settings.simulate_wallet_keypair_json;
    }
    if partial_settings.simulate_wallet_private_key_string != current_settings.simulate_wallet_private_key_string {
        current_settings.simulate_wallet_private_key_string = partial_settings.simulate_wallet_private_key_string;
    }
    
    // Save updated settings to config.toml
    if let Err(e) = (*current_settings).save_to_file("config.toml") {
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
    
    info!("Bot stopping");
    bot_control.add_log(
        "info",
        "Bot stopping".to_string(),
        None
    ).await;
    
    // Simulate shutdown delay
    let bot_control_clone = bot_control.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let mut state = bot_control_clone.running_state.lock().await;
        *state = BotRunningState::Stopped;
        bot_control_clone.add_log(
            "info",
            "Bot stopped successfully".to_string(),
            None
        ).await;
        info!("Bot state changed to Stopped");
    });
    
    (
        StatusCode::OK,
        Json(json!({
            "status": "success",
            "message": "Bot is stopping"
        }))
    )
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
                "message": "Bot must be stopped before changing mode"
            }))
        );
    }
    drop(running_state);
    
    let new_mode = match payload.mode.as_str() {
        "dry-run" => BotMode::DryRun,
        "real" => BotMode::Real,
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
    
    let bot_control = state.bot_control.clone();
    info!("Bot mode changed to {:?}", new_mode);
    bot_control.add_log(
        "info",
        format!("Bot mode changed to {:?}", new_mode),
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
    Json(coins.clone())
}

async fn get_trades_handler(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let trades = state.trades.lock().await;
    Json(trades.clone())
}
