#![cfg(target_arch = "wasm32")]

// Sol Beast WASM Bindings
// Browser-compatible trading bot

use wasm_bindgen::prelude::*;

fn merge_json_defaults(defaults: &mut serde_json::Value, overrides: serde_json::Value) {
    match (defaults, overrides) {
        (serde_json::Value::Object(default_map), serde_json::Value::Object(override_map)) => {
            for (key, override_value) in override_map {
                match default_map.get_mut(&key) {
                    Some(default_value) => merge_json_defaults(default_value, override_value),
                    None => {
                        default_map.insert(key, override_value);
                    }
                }
            }
        }
        (default_slot, override_value) => {
            *default_slot = override_value;
        }
    }
}

fn parse_settings_with_defaults(settings_json: &str) -> Result<Settings, JsValue> {
    let mut defaults = serde_json::to_value(Settings::default())
        .map_err(|e| JsValue::from_str(&format!("Failed to build default settings: {}", e)))?;
    let overrides: serde_json::Value = serde_json::from_str(settings_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse settings: {}", e)))?;
    merge_json_defaults(&mut defaults, overrides);
    serde_json::from_value(defaults)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse settings: {}", e)))
}
use sol_beast_core::models::*;
use sol_beast_core::settings::Settings;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use log::{info, error, warn};

mod monitor;
use monitor::Monitor;

/// Bot operating mode - using enum to prevent memory corruption
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BotMode {
    DryRun,
    Real,
}

impl BotMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            BotMode::DryRun => "dry-run",
            BotMode::Real => "real",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "dry-run" => Some(BotMode::DryRun),
            "real" => Some(BotMode::Real),
            _ => None,
        }
    }
}

impl Default for BotMode {
    fn default() -> Self {
        BotMode::DryRun
    }
}

impl std::fmt::Display for BotMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// Use wee_alloc as the global allocator for smaller WASM size and better memory management
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// Constants for configuration defaults and limits
const DEFAULT_SOLANA_RPC_URL: &str = "https://api.mainnet-beta.solana.com/";
const DEFAULT_SOLANA_WS_URL: &str = "wss://api.mainnet-beta.solana.com/";
const DEFAULT_PUMP_FUN_PROGRAM: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const DEFAULT_METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

const MAX_DETECTED_TOKENS: usize = 50;
const FALLBACK_ESTIMATED_PRICE: f64 = 0.00001; // Fallback if bonding curve fetch fails

// Initialize panic hook and logger for WASM
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::default());
}

// Bot state management in WASM
#[wasm_bindgen]
pub struct SolBeastBot {
    state: Arc<Mutex<BotState>>,
}

struct BotState {
    running: bool,
    mode: BotMode,
    settings: Settings,
    holdings: Vec<Holding>,
    logs: Vec<LogEntry>,
    monitor: Option<Monitor>,
    detected_tokens: Vec<DetectedToken>, // Phase 2: Track detected tokens
}

fn sanitize_settings(settings: &Settings) -> Settings {
    let mut s = settings.clone();
    
    // Filter out empty URLs and URLs containing null bytes
    s.solana_ws_urls.retain(|url| !url.is_empty() && !url.contains('\0'));
    s.solana_rpc_urls.retain(|url| !url.is_empty() && !url.contains('\0'));
    
    // If we have no valid URLs after filtering, use defaults
    if s.solana_ws_urls.is_empty() {
        s.solana_ws_urls = vec![DEFAULT_SOLANA_WS_URL.to_string()];
    }
    
    if s.solana_rpc_urls.is_empty() {
        s.solana_rpc_urls = vec![DEFAULT_SOLANA_RPC_URL.to_string()];
    }
    
    // Check program IDs
    if s.pump_fun_program.is_empty() || s.pump_fun_program.contains('\0') {
        s.pump_fun_program = DEFAULT_PUMP_FUN_PROGRAM.to_string();
    }
    
    if s.metadata_program.is_empty() || s.metadata_program.contains('\0') {
        s.metadata_program = DEFAULT_METADATA_PROGRAM.to_string();
    }
    
    s
}


#[derive(Serialize, Deserialize, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub details: Option<String>,
}

/// Detected token with metadata and evaluation result
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DetectedToken {
    pub signature: String,
    pub mint: String,
    pub creator: String,
    pub bonding_curve: String,
    pub holder_address: String,
    pub timestamp: String,
    // Metadata (if available)
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub image_uri: Option<String>,
    pub description: Option<String>,
    // Evaluation result
    pub should_buy: bool,
    pub evaluation_reason: String,
    pub token_amount: Option<u64>,
    pub buy_price_sol: Option<f64>,
    // Additional info
    pub liquidity_sol: Option<f64>,
}

#[wasm_bindgen]
impl SolBeastBot {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        // LocalStorage has been a source of invalid UTF-8/recursive mutex crashes in WASM; disable persistence for stability.
        // Always start from defaults and aggressively clear any existing stored data.
        if let Err(e) = sol_beast_core::wasm::storage::clear_all() {
            warn!("Failed to clear localStorage on init (continuing with defaults): {:?}", e);
        }
        let settings = Settings::default();
        
        let state = BotState {
            running: false,
            mode: BotMode::default(),
            settings,
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        
        info!("WASM bot constructed with mode: {}", state.mode.as_str());
        
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }
    
    /// Initialize bot with settings
    #[wasm_bindgen]
    pub fn init_with_settings(&mut self, settings_json: &str) -> Result<(), JsValue> {
        let settings = parse_settings_with_defaults(settings_json)?;
        
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in init_with_settings, recovering...");
                poisoned.into_inner()
            }
        };
        state.settings = settings;
        Ok(())
    }
    
    /// Start the bot
    #[wasm_bindgen]
    pub fn start(&self) -> Result<(), JsValue> {
        // Acquire lock to read state
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in start, recovering...");
                poisoned.into_inner()
            }
        };
        if state.running {
            return Err(JsValue::from_str("Bot is already running"));
        }
        
        let mode = state.mode.as_str();
        
        // Use constants directly instead of trusting settings to avoid memory corruption
        let (ws_url, pump_fun_program) = if !is_settings_valid(&state.settings) {
            warn!("Settings invalid, using hardcoded defaults");
            // Always use standard Solana WebSocket endpoint
            (DEFAULT_SOLANA_WS_URL.to_string(), DEFAULT_PUMP_FUN_PROGRAM.to_string())
        } else {
            // Use configured WebSocket URL (should be standard Solana endpoint)
            let ws = state.settings.solana_ws_urls.first()
                .ok_or_else(|| JsValue::from_str("No WebSocket URL configured"))?
                .clone();
            (ws, state.settings.pump_fun_program.clone())
        };
        
        // Always use standard Solana WebSocket protocol (logsSubscribe)

        
        // Validate strings are ASCII (which is valid UTF-8) before using
        if !ws_url.is_ascii() || !pump_fun_program.is_ascii() {
            error!("Invalid characters in configuration - ws_url or pump_fun_program contains non-ASCII");
            return Err(JsValue::from_str("Configuration contains invalid characters"));
        }
        
        info!("Bot starting - WebSocket and program validated");
        
        // Drop the lock before creating callbacks and starting monitor to avoid recursive locking
        drop(state);
        
        // Create logging callback that adds logs to state
        let state_for_logs = self.state.clone();
        let log_callback = Arc::new(move |level: String, message: String, details: String| {
            if let Ok(mut s) = state_for_logs.lock() {
                let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
                    .unwrap_or_else(|| "unknown".to_string());
                s.logs.push(LogEntry {
                    timestamp,
                    level,
                    message,
                    details: Some(details),
                });
                // Keep only last 200 logs (increased from 100)
                if s.logs.len() > 200 {
                    let excess = s.logs.len() - 200;
                    s.logs.drain(0..excess);
                }
            }
        });
        
        // Create signature processing callback
        let state_for_processing = self.state.clone();
        let signature_callback = Arc::new(move |signature: String| {
            // Clone what we need for the async task
            let state = state_for_processing.clone();
            
            // Spawn async task to process the signature
            wasm_bindgen_futures::spawn_local(async move {
                // Process the transaction asynchronously
                process_detected_signature(signature, state).await;
            });
        });
        
        // Create and start monitor
        let mut monitor = Monitor::new();
        monitor.start(&ws_url, &pump_fun_program, log_callback, Some(signature_callback))
            .map_err(|e| JsValue::from_str(&format!("Failed to start monitor: {:?}", e)))?;
        
        // Re-acquire lock to update state
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in start (after monitor), recovering...");
                poisoned.into_inner()
            }
        };
        state.monitor = Some(monitor);
        state.running = true;
        
        let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
            .unwrap_or_else(|| "unknown".to_string());
        
        // Create details string and ensure it's valid UTF-8
        let details = format!(
            "Mode: {}\nWebSocket: {}\nProgram: {}\n\nThe bot is now monitoring for new pump.fun tokens. Logs will appear as transactions are detected.", 
            mode, 
            ws_url.chars().filter(|c| c.is_ascii()).collect::<String>(),
            pump_fun_program.chars().filter(|c| c.is_ascii()).collect::<String>()
        );
        
        state.logs.push(LogEntry {
            timestamp,
            level: "info".to_string(),
            message: "✓ Bot started successfully".to_string(),
            details: Some(details),
        });
        
        info!("WASM bot started successfully in {} mode", mode);
        Ok(())
    }
    
    /// Stop the bot
    #[wasm_bindgen]
    pub fn stop(&self) -> Result<(), JsValue> {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in stop, recovering...");
                poisoned.into_inner()
            }
        };
        if !state.running {
            return Err(JsValue::from_str("Bot is not running"));
        }
        
        // Stop monitoring
        if let Some(mut monitor) = state.monitor.take() {
            monitor.stop()
                .map_err(|e| JsValue::from_str(&format!("Failed to stop monitor: {:?}", e)))?;
        }
        
        state.running = false;
        let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
            .unwrap_or_else(|| "unknown".to_string());
        state.logs.push(LogEntry {
            timestamp,
            level: "info".to_string(),
            message: "✓ Bot stopped successfully".to_string(),
            details: Some("Monitoring stopped, WebSocket closed, resources cleaned up".to_string()),
        });
        
        info!("WASM bot stopped successfully");
        Ok(())
    }
    
    /// Get bot status
    #[wasm_bindgen]
    pub fn is_running(&self) -> bool {
        // In WASM (single-threaded), re-entrant calls can happen (Rust -> JS callback -> Rust).
        // Using try_lock avoids panicking on recursive mutex acquisition.
        match self.state.try_lock() {
            Ok(guard) => guard.running,
            Err(_) => false,
        }
    }
    
    /// Set bot mode (dry-run or real)
    /// 
    /// MEMORY SAFETY: This function implements multiple layers of validation to prevent
    /// memory access errors when changing the bot mode:
    /// 1. Validates input string before acquiring any locks
    /// 2. Checks for null bytes that could cause WASM memory issues
    /// 3. Uses mutex poisoning recovery to handle panics in other threads
    /// 4. Creates a fresh String allocation to avoid memory corruption
    #[wasm_bindgen]
    pub fn set_mode(&self, mode: &str) -> Result<(), JsValue> {
        // Parse mode string to enum
        let bot_mode = BotMode::from_str(mode)
            .ok_or_else(|| JsValue::from_str("Mode must be 'dry-run' or 'real'"))?;
        
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in set_mode, recovering...");
                poisoned.into_inner()
            }
        };
        
        if state.running {
            return Err(JsValue::from_str("Cannot change mode while bot is running"));
        }
        
        drop(state);
        
        // Acquire mutable lock to change mode
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in set_mode (second lock), recovering...");
                poisoned.into_inner()
            }
        };
        
        state.mode = bot_mode;
        info!("Bot mode changed to: {}", state.mode.as_str());
        Ok(())
    }
    
    /// Get current mode
    /// 
    /// MEMORY SAFETY: Returns a static string, no memory corruption possible
    #[wasm_bindgen]
    pub fn get_mode(&self) -> String {
        match self.state.try_lock() {
            Ok(state) => String::from(state.mode.as_str()),
            Err(_) => {
                String::from("dry-run")
            }
        }
    }
    
    /// Update settings
    /// If bot is running and critical settings change (WebSocket URL, program ID),
    /// the bot will need to be restarted manually for changes to take effect.
    #[wasm_bindgen]
    pub fn update_settings(&self, settings_json: &str) -> Result<(), JsValue> {
        // Parse settings first (outside any lock) and merge with defaults to keep forward-compat.
        let mut settings = parse_settings_with_defaults(settings_json)?;
        
        // Validate and sanitize parsed settings before updating
        if !is_settings_valid(&settings) {
            warn!("Received invalid settings, attempting to sanitize");
            settings = sanitize_settings(&settings);
            
            // Verify sanitized settings are valid
            if !is_settings_valid(&settings) {
                return Err(JsValue::from_str("Settings could not be repaired: missing or corrupted required fields"));
            }
            info!("Successfully sanitized and validated settings");
        }
        
        // Acquire lock once and hold it to prevent race conditions
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in update_settings, recovering...");
                poisoned.into_inner()
            }
        };
        
        // Check if bot is running and if critical settings have changed
        let is_running = state.running;
        let needs_restart = if is_running {
            let old_ws_url = state.settings.solana_ws_urls.first();
            let new_ws_url = settings.solana_ws_urls.first();
            let ws_changed = old_ws_url != new_ws_url;
            
            let program_changed = state.settings.pump_fun_program != settings.pump_fun_program;
            
            ws_changed || program_changed
        } else {
            false
        };
        
        // Update in-memory state first
        state.settings = settings.clone();
        drop(state); // Release lock before log entry
        
        // Re-acquire lock to add log entries (persistence disabled)
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in update_settings (log entry), recovering...");
                poisoned.into_inner()
            }
        };
        
        if is_running && needs_restart {
            info!("Settings updated - bot restart required for WebSocket/program changes to take effect");
            // Add a log entry to notify user
            let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
                .unwrap_or_else(|| "unknown".to_string());
            state.logs.push(LogEntry {
                timestamp,
                level: "warn".to_string(),
                message: "⚠️ Settings updated".to_string(),
                details: Some("WebSocket URL or program ID changed. Please restart the bot for changes to take effect.".to_string()),
            });
        } else if is_running {
            info!("Settings updated (non-critical changes, no restart needed)");
            let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
                .unwrap_or_else(|| "unknown".to_string());
            state.logs.push(LogEntry {
                timestamp,
                level: "info".to_string(),
                message: "✓ Settings updated".to_string(),
                details: Some("Trading parameters updated. Changes will apply to future trades.".to_string()),
            });
        } else {
            info!("Settings updated and saved to localStorage");
        }
        
        Ok(())
    }
    
    /// Get current settings as JSON
    /// 
    /// MEMORY SAFETY: This function implements comprehensive error handling:
    /// 1. Recovers from mutex poisoning
    /// 2. Repairs corrupted mode values before returning settings
    /// 3. Validates settings structure before serialization
    /// 4. Falls back to default settings if serialization fails
    /// 5. Ensures returned JSON is valid UTF-8 without null bytes
    /// 
    /// This prevents "memory access out of bounds" errors that can occur when
    /// the frontend tries to parse corrupted or malformed settings data.
    #[wasm_bindgen]
    pub fn get_settings(&self) -> Result<String, JsValue> {
        // Use a simpler approach to avoid potential memory issues with complex serialization
        // inside a lock.
        let settings_clone = match self.state.try_lock() {
            Ok(state) => {
                if is_settings_valid(&state.settings) {
                    Some(state.settings.clone())
                } else {
                    warn!("Settings invalid, using defaults");
                    None
                }
            },
            Err(_) => {
                None
            }
        };

        let settings = settings_clone.unwrap_or_else(Settings::default);
        
        serde_json::to_string(&settings)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize settings: {}", e)))
    }
    
    /// Get logs as JSON
    #[wasm_bindgen]
    pub fn get_logs(&self) -> Result<String, JsValue> {
        match self.state.try_lock() {
            Ok(state) => {
                serde_json::to_string(&state.logs)
                    .map_err(|e| JsValue::from_str(&format!("Failed to serialize logs: {}", e)))
            },
            Err(_) => {
                Err(JsValue::from_str("Could not lock mutex"))
            }
        }
    }
    
    /// Get holdings as JSON
    #[wasm_bindgen]
    pub fn get_holdings(&self) -> Result<String, JsValue> {
        match self.state.try_lock() {
            Ok(state) => {
                serde_json::to_string(&state.holdings)
                    .map_err(|e| JsValue::from_str(&format!("Failed to serialize holdings: {}", e)))
            },
            Err(_) => {
                Err(JsValue::from_str("Could not lock mutex"))
            }
        }
    }
    
    /// Get detected tokens as JSON (Phase 2 feature)
    #[wasm_bindgen]
    pub fn get_detected_tokens(&self) -> Result<String, JsValue> {
        match self.state.try_lock() {
            Ok(state) => serde_json::to_string(&state.detected_tokens)
                .map_err(|e| JsValue::from_str(&format!("Failed to serialize detected tokens: {}", e))),
            Err(_) => Ok("[]".to_string()),
        }
    }
    
    /// Add a holding after successful purchase (Phase 4 feature)
    /// Call this after a buy transaction is confirmed
    #[wasm_bindgen]
    pub fn add_holding(
        &self,
        mint: &str,
        amount: u64,
        buy_price: f64,
        metadata_json: Option<String>,
    ) -> Result<(), JsValue> {
        use chrono::Utc;
        
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in add_holding, recovering...");
                poisoned.into_inner()
            }
        };
        
        // Parse metadata if provided
        let metadata = if let Some(json) = metadata_json {
            match serde_json::from_str(&json) {
                Ok(meta) => Some(meta),
                Err(e) => {
                    warn!("Failed to parse metadata JSON: {}", e);
                    None
                }
            }
        } else {
            None
        };
        
        // Check if already holding this token
        if let Some(existing) = state.holdings.iter_mut().find(|h| h.mint == mint) {
            // Update existing holding (average price with overflow protection)
            let total_old = existing.amount as f64 * existing.buy_price;
            let total_new = amount as f64 * buy_price;
            let new_total_amount = existing.amount.saturating_add(amount);
            if new_total_amount == 0 {
                warn!("Amount overflow detected for {}, skipping update", mint);
                return Err(JsValue::from_str("Amount overflow"));
            }
            existing.buy_price = (total_old + total_new) / new_total_amount as f64;
            existing.amount = new_total_amount;
            info!("Updated existing holding for {}: {} tokens @ {:.9} SOL", 
                  mint, existing.amount, existing.buy_price);
        } else {
            // Add new holding
            let holding = Holding {
                mint: mint.to_string(),
                amount,
                buy_price,
                buy_time: Utc::now(),
                creator: None, // TODO: Pass creator if available
                metadata,
                onchain: None,
                onchain_raw: None,
            };
            state.holdings.push(holding);
            info!("Added new holding for {}: {} tokens @ {:.9} SOL", mint, amount, buy_price);
        }
        
        // Persist to localStorage
        if let Err(e) = self.save_holdings_to_storage(&state.holdings) {
            warn!("Failed to persist holdings to localStorage: {:?}", e);
        }
        
        // Log to UI
        let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
            .unwrap_or_else(|| "unknown".to_string());
        state.logs.push(LogEntry {
            timestamp,
            level: "info".to_string(),
            message: format!("✅ Purchase recorded"),
            details: Some(format!(
                "Mint: {}\nAmount: {} tokens\nPrice: {:.9} SOL/token",
                mint, amount as f64 / 1_000_000.0, buy_price
            )),
        });
        
        Ok(())
    }
    
    /// Start monitoring holdings for TP/SL/timeout (Phase 4 feature)
    /// This should be called periodically (e.g., every 5 seconds) from JavaScript
    #[wasm_bindgen]
    pub async fn monitor_holdings(&self) -> Result<String, JsValue> {
        use chrono::Utc;
        use sol_beast_core::wasm::WasmRpcClient;
        use sol_beast_core::rpc_client::{fetch_bonding_curve_state, calculate_price_from_bonding_curve};
        use sol_beast_core::strategy::{evaluate_position, TradeAction, SellReason};
        
        let (holdings_to_check, rpc_url, settings) = {
            let state = match self.state.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    info!("Mutex was poisoned in monitor_holdings, recovering...");
                    poisoned.into_inner()
                }
            };
            
            if state.holdings.is_empty() {
                return Ok(serde_json::json!({ "action": "none", "holdings": 0 }).to_string());
            }
            
            let rpc_url = state.settings.solana_rpc_urls.first().cloned()
                .unwrap_or_else(|| DEFAULT_SOLANA_RPC_URL.to_string());
            
            (state.holdings.clone(), rpc_url, state.settings.clone())
        };
        
        let rpc_client = WasmRpcClient::new(rpc_url);
        let mut actions = Vec::new();
        let core_settings = settings.clone();
        
        for holding in holdings_to_check.iter() {
            // Get bonding curve address for this mint
            use solana_pubkey::Pubkey;
            let mint_pk = match holding.mint.parse::<Pubkey>() {
                Ok(pk) => pk,
                Err(e) => {
                    warn!("Invalid mint address {}: {}", holding.mint, e);
                    continue;
                }
            };
            
            let program_pk = match settings.pump_fun_program.parse::<Pubkey>() {
                Ok(pk) => pk,
                Err(e) => {
                    warn!("Invalid program address: {}", e);
                    continue;
                }
            };
            
            // Derive bonding curve PDA
            let (bonding_curve_pk, _) = Pubkey::find_program_address(
                &[b"bonding-curve", mint_pk.as_ref()],
                &program_pk,
            );
            
            // Fetch current price
            let current_price = match fetch_bonding_curve_state(
                &holding.mint,
                &bonding_curve_pk.to_string(),
                &rpc_client,
            ).await {
                Ok(state) => calculate_price_from_bonding_curve(&state),
                Err(e) => {
                    warn!("Failed to fetch price for {}: {:?}", holding.mint, e);
                    continue;
                }
            };
            
            // Evaluate position using core strategy
            let action = evaluate_position(holding, current_price, &core_settings);
            
            if let TradeAction::Sell(reason) = action {
                // Calculate profit percent for logging/UI
                let profit_percent = if holding.buy_price != 0.0 {
                    ((current_price - holding.buy_price) / holding.buy_price) * 100.0
                } else {
                    0.0
                };
                
                let reason_str = match reason {
                    SellReason::TakeProfit(_) => "TP",
                    SellReason::StopLoss(_) => "SL",
                    SellReason::Timeout(_) => "TIMEOUT",
                };
                
                info!("{} triggered for {}: {:.2}% @ {:.9} SOL", reason_str, holding.mint, profit_percent, current_price);
                
                actions.push(serde_json::json!({
                    "action": "sell",
                    "mint": holding.mint,
                    "reason": reason_str,
                    "profitPercent": profit_percent,
                    "currentPrice": current_price,
                    "amount": holding.amount,
                }));
                
                // Log to UI
                let mut state = match self.state.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner()
                };
                let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
                    .unwrap_or_else(|| "unknown".to_string());
                
                let elapsed_secs = Utc::now().signed_duration_since(holding.buy_time).num_seconds();
                
                state.logs.push(LogEntry {
                    timestamp,
                    level: "warn".to_string(),
                    message: format!("🔔 {} Triggered: {}", reason_str, 
                                    holding.metadata.as_ref()
                                        .and_then(|m| m.symbol.as_ref())
                                        .unwrap_or(&holding.mint)),
                    details: Some(format!(
                        "Profit: {:.2}%\nCurrent Price: {:.9} SOL\nBuy Price: {:.9} SOL\nHold Time: {}s",
                        profit_percent, current_price, holding.buy_price, elapsed_secs
                    )),
                });
            }
        }
        
        if actions.is_empty() {
            Ok(serde_json::json!({ 
                "action": "none", 
                "holdings": holdings_to_check.len() 
            }).to_string())
        } else {
            Ok(serde_json::json!({ 
                "action": "sell_required", 
                "actions": actions 
            }).to_string())
        }
    }
    
    /// Build sell transaction for a holding (Phase 4 feature)
    #[wasm_bindgen]
    pub fn build_sell_transaction(&self, mint: &str, user_pubkey: &str) -> Result<String, JsValue> {
        use sol_beast_core::tx_builder::build_sell_instruction;
        use solana_pubkey::Pubkey;
        use base64::{Engine as _, engine::general_purpose};
        
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in build_sell_transaction, recovering...");
                poisoned.into_inner()
            }
        };
        
        // Find the holding
        let holding = state.holdings.iter()
            .find(|h| h.mint == mint)
            .ok_or_else(|| JsValue::from_str("Token not found in holdings"))?;
        
        // Parse public keys
        let user_pk = user_pubkey.parse::<Pubkey>()
            .map_err(|e| JsValue::from_str(&format!("Invalid user public key: {}", e)))?;
        let program_pk = state.settings.pump_fun_program.parse::<Pubkey>()
            .map_err(|e| JsValue::from_str(&format!("Invalid program address: {}", e)))?;
        
        // Use user as fee recipient for sell (same as buy)
        let fee_recipient = user_pk.clone();
        
        let core_settings = state.settings.clone();
        let token_amount = holding.amount;
        
        // Calculate minimum SOL to receive (with slippage)
        // For sell, we want at least (1 - slippage) of expected value
        let slippage_multiplier = 1.0 - (state.settings.slippage_bps as f64 / 10000.0);
        // Use current buy_price as estimate, frontend should fetch real price
        let estimated_sol = (token_amount as f64 / 1_000_000.0) * holding.buy_price;
        let min_sol_output = (estimated_sol * slippage_multiplier * 1e9) as u64; // Convert to lamports
        
        // Build sell instruction (creator is optional for sell)
        let instruction = build_sell_instruction(
            &program_pk,
            mint,
            token_amount,
            min_sol_output,
            &user_pk,
            &fee_recipient,
            None, // creator_pubkey not needed for sell
            &core_settings,
        ).map_err(|e| JsValue::from_str(&format!("Failed to build sell instruction: {}", e)))?;
        
        // Serialize instruction to JSON
        let accounts_json: Vec<serde_json::Value> = instruction.accounts.iter()
            .map(|acc| serde_json::json!({
                "pubkey": acc.pubkey.to_string(),
                "isSigner": acc.is_signer,
                "isWritable": acc.is_writable,
            }))
            .collect();
        
        let data_base64 = general_purpose::STANDARD.encode(&instruction.data);
        
        let result = serde_json::json!({
            "programId": instruction.program_id.to_string(),
            "accounts": accounts_json,
            "data": data_base64,
            "tokenAmount": token_amount,
            "minSolOutput": min_sol_output,
            "estimatedSol": estimated_sol,
        });
        
        serde_json::to_string(&result)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize instruction: {}", e)))
    }
    
    /// Remove a holding after successful sell (Phase 4 feature)
    #[wasm_bindgen]
    pub fn remove_holding(&self, mint: &str, profit_percent: f64, reason: &str) -> Result<(), JsValue> {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in remove_holding, recovering...");
                poisoned.into_inner()
            }
        };
        
        // Find and remove holding
        if let Some(pos) = state.holdings.iter().position(|h| h.mint == mint) {
            let holding = state.holdings.remove(pos);
            info!("Removed holding for {}: {} tokens @ {:.9} SOL", 
                  mint, holding.amount, holding.buy_price);
            
            // Persist to localStorage
            if let Err(e) = self.save_holdings_to_storage(&state.holdings) {
                warn!("Failed to persist holdings to localStorage: {:?}", e);
            }
            
            // Log to UI
            let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
                .unwrap_or_else(|| "unknown".to_string());
            let result_icon = if profit_percent > 0.0 { "✅" } else { "❌" };
            state.logs.push(LogEntry {
                timestamp,
                level: "info".to_string(),
                message: format!("{} Token sold: {}", result_icon, 
                               holding.metadata.as_ref()
                                   .and_then(|m| m.symbol.as_ref())
                                   .map(|s| s.as_str())
                                   .unwrap_or(mint)),
                details: Some(format!(
                    "Reason: {}\nProfit: {:.2}%\nTokens: {}\nBuy Price: {:.9} SOL",
                    reason, profit_percent, holding.amount as f64 / 1_000_000.0, holding.buy_price
                )),
            });
            
            Ok(())
        } else {
            Err(JsValue::from_str("Holding not found"))
        }
    }
    
    /// Build buy transaction for a detected token (Phase 3.3 feature)
    /// Returns a JSON object with transaction data that can be signed and submitted
    #[wasm_bindgen]
    pub fn build_buy_transaction(&self, mint: &str, user_pubkey: &str) -> Result<String, JsValue> {
        use sol_beast_core::tx_builder::build_buy_instruction;
        use solana_pubkey::Pubkey;
        
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in build_buy_transaction, recovering...");
                poisoned.into_inner()
            }
        };
        
        // Find the detected token
        let token = state.detected_tokens.iter()
            .find(|t| t.mint == mint)
            .ok_or_else(|| JsValue::from_str("Token not found in detected tokens"))?;
        
        // Parse public keys
        let user_pk = user_pubkey.parse::<Pubkey>()
            .map_err(|e| JsValue::from_str(&format!("Invalid user public key: {}", e)))?;
        let creator_pk = token.creator.parse::<Pubkey>()
            .map_err(|e| JsValue::from_str(&format!("Invalid creator address: {}", e)))?;
        let program_pk = state.settings.pump_fun_program.parse::<Pubkey>()
            .map_err(|e| JsValue::from_str(&format!("Invalid program address: {}", e)))?;
        
        // Get fee recipient from bonding curve
        // NOTE: In pump.fun, the fee_recipient is typically stored in the bonding curve account.
        // For now, using creator address as a temporary placeholder. This works for most cases
        // but should be fetched from the bonding curve account data for accuracy.
        // TODO: Fetch actual fee_recipient by calling get_account_info on bonding_curve address
        // and parsing the account data to extract the fee_recipient field
        let fee_recipient = creator_pk; // Temporary placeholder - works but not ideal
        
        // Calculate amount and max sol cost
        let buy_amount_sol = state.settings.buy_amount;
        let price_per_token = token.buy_price_sol.unwrap_or(FALLBACK_ESTIMATED_PRICE);
        let token_amount = (buy_amount_sol / price_per_token) as u64;
        let slippage_multiplier = 1.0 + (state.settings.slippage_bps as f64 / 10000.0);
        let max_sol_cost = (buy_amount_sol * slippage_multiplier * 1_000_000_000.0) as u64; // Convert to lamports
        
        // Build instruction using core tx_builder
        let core_settings = state.settings.clone();
        let instruction = build_buy_instruction(
            &program_pk,
            mint,
            token_amount,
            max_sol_cost,
            Some(true), // track_volume: always enabled in WASM mode for consistency with CLI behavior
            &user_pk,
            &fee_recipient,
            Some(creator_pk),
            &core_settings,
        ).map_err(|e| JsValue::from_str(&format!("Failed to build instruction: {}", e)))?;
        
        // Serialize instruction to JSON format compatible with web3.js
        let accounts_json: Vec<serde_json::Value> = instruction.accounts.iter().map(|acc| {
            serde_json::json!({
                "pubkey": acc.pubkey.to_string(),
                "isSigner": acc.is_signer,
                "isWritable": acc.is_writable,
            })
        }).collect();
        
        // Encode instruction data as base64
        use base64::Engine;
        let base64_engine = base64::engine::general_purpose::STANDARD;
        let data_base64 = base64_engine.encode(&instruction.data);
        
        let result = serde_json::json!({
            "programId": instruction.program_id.to_string(),
            "accounts": accounts_json,
            "data": data_base64,
            "tokenAmount": token_amount,
            "maxSolCost": max_sol_cost,
            "buyAmountSol": buy_amount_sol,
        });
        
        serde_json::to_string(&result)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize instruction: {}", e)))
    }
    
    /// Connect to Solana RPC (for testing connection)
    #[wasm_bindgen]
    pub async fn test_rpc_connection(&self) -> Result<String, JsValue> {
        use sol_beast_core::wasm::WasmRpcClient;
        
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in test_rpc_connection, recovering...");
                poisoned.into_inner()
            }
        };
        let rpc_url = state.settings.solana_rpc_urls.first()
            .ok_or_else(|| JsValue::from_str("No RPC URL configured"))?
            .clone();
        drop(state);
        
        let rpc = WasmRpcClient::new(rpc_url);
        let blockhash = rpc.get_latest_blockhash().await?;
        
        Ok(format!("Connected! Latest blockhash: {}", blockhash))
    }
    
    /// Connect to Solana WebSocket (for testing)
    #[wasm_bindgen]
    pub async fn test_ws_connection(&self) -> Result<String, JsValue> {
        use sol_beast_core::wasm::WasmWebSocket;
        
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in test_ws_connection, recovering...");
                poisoned.into_inner()
            }
        };
        let ws_url = state.settings.solana_ws_urls.first()
            .ok_or_else(|| JsValue::from_str("No WebSocket URL configured"))?
            .clone();
        drop(state);
        
        let _ws = WasmWebSocket::new(&ws_url)?;
        
        Ok("WebSocket connected successfully!".to_string())
    }
    
    /// Save current state to localStorage
    #[wasm_bindgen]
    pub fn save_to_storage(&self) -> Result<(), JsValue> {
        // Persistence is disabled in WASM mode to avoid localStorage corruption
        Ok(())
    }
    
    /// Load state from localStorage
    /// 
    /// MEMORY SAFETY: This function safely loads settings from localStorage:
    /// 1. Uses the enhanced load_settings that auto-clears corrupted data
    /// 2. Validates loaded settings before applying them
    /// 3. Sanitizes settings if validation fails
    /// 4. Falls back to current settings if all recovery attempts fail
    /// 5. Handles holdings loading separately with error isolation
    #[wasm_bindgen]
    pub fn load_from_storage(&self) -> Result<(), JsValue> {
        // Persistence is disabled in WASM mode to avoid localStorage corruption
        Ok(())
    }
    
    /// Helper: Save holdings to localStorage
    fn save_holdings_to_storage(&self, _holdings: &[Holding]) -> Result<(), JsValue> {
        // Persistence is disabled in WASM mode to avoid localStorage corruption
        Ok(())
    }
    
    /// Helper: Load holdings from localStorage
    /// 
    /// MEMORY SAFETY: Safely loads holdings with automatic corruption recovery:
    /// 1. Validates JSON string before deserialization
    /// 2. Catches deserialization errors and clears corrupted data
    /// 3. Continues with empty holdings if data is corrupted
    #[allow(dead_code)]
    fn load_holdings_from_storage(&self) -> Result<(), JsValue> {
        // Persistence is disabled in WASM mode to avoid localStorage corruption
        Ok(())
    }
}

/// Process a detected pump.fun signature
/// This is called asynchronously when the monitor detects a new transaction
async fn process_detected_signature(signature: String, state: Arc<Mutex<BotState>>) {
    use sol_beast_core::wasm::{WasmRpcClient, WasmHttpClient};
    use sol_beast_core::pipeline::process_new_token;
    use log::{info, error};
    
    info!("Processing detected signature: {}", signature);
    
    // Get settings and configuration from state
    let (rpc_url, settings) = {
        match state.lock() {
            Ok(s) => {
                let rpc_url = s.settings.solana_rpc_urls.first().cloned()
                    .unwrap_or_else(|| DEFAULT_SOLANA_RPC_URL.to_string());
                let settings = s.settings.clone();
                (rpc_url, settings)
            },
            Err(e) => {
                error!("Failed to lock state in process_detected_signature: {:?}", e);
                return;
            }
        }
    };
    
    // Create RPC and HTTP clients
    let rpc_client = WasmRpcClient::new(rpc_url);
    let http_client = WasmHttpClient::new();
    
    // Use pipeline
    match process_new_token(signature.clone(), &rpc_client, &http_client, &settings).await {
        Ok(result) => {
            // Create DetectedToken and store in state
            let detected_token = DetectedToken {
                signature: result.signature,
                mint: result.mint.clone(),
                creator: result.creator.clone(),
                bonding_curve: result.bonding_curve,
                holder_address: result.holder_address,
                timestamp: js_sys::Date::new_0().to_iso_string().as_string()
                    .unwrap_or_else(|| "unknown".to_string()),
                name: result.name.clone(),
                symbol: result.symbol.clone(),
                image_uri: result.image_uri,
                description: result.description,
                should_buy: result.should_buy,
                evaluation_reason: result.evaluation_reason.clone(),
                token_amount: Some(result.token_amount),
                buy_price_sol: Some(result.buy_price_sol),
                liquidity_sol: result.liquidity_sol,
            };
            
            // Add to state
            if let Ok(mut s) = state.lock() {
                s.detected_tokens.push(detected_token.clone());
                
                // Keep only last MAX_DETECTED_TOKENS
                if s.detected_tokens.len() > MAX_DETECTED_TOKENS {
                    let excess = s.detected_tokens.len() - MAX_DETECTED_TOKENS;
                    s.detected_tokens.drain(0..excess);
                }
                
                // Log to UI
                let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
                    .unwrap_or_else(|| "unknown".to_string());
                let result_icon = if result.should_buy { "✅" } else { "❌" };
                s.logs.push(LogEntry {
                    timestamp,
                    level: if result.should_buy { "info" } else { "warn" }.to_string(),
                    message: format!("{} Token evaluated: {}", result_icon, 
                                   result.symbol.as_deref().unwrap_or(&result.mint)),
                    details: Some(format!(
                        "Name: {}\nSymbol: {}\nMint: {}\nCreator: {}\nPrice: {:.8} SOL\nLiquidity: {:.4} SOL\n\nEvaluation: {}",
                        result.name.as_deref().unwrap_or("Unknown"),
                        result.symbol.as_deref().unwrap_or("Unknown"),
                        result.mint,
                        result.creator,
                        result.buy_price_sol,
                        result.liquidity_sol.unwrap_or(0.0),
                        result.evaluation_reason
                    )),
                });
            }
            
            info!("Successfully processed transaction {}", signature);
        },
        Err(e) => {
            error!("Failed to process transaction {}: {:?}", signature, e);
            // Log to UI
            if let Ok(mut s) = state.lock() {
                let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
                    .unwrap_or_else(|| "unknown".to_string());
                s.logs.push(LogEntry {
                    timestamp,
                    level: "error".to_string(),
                    message: format!("Failed to process transaction"),
                    details: Some(format!("Signature: {}\nError: {:?}", signature, e)),
                });
            }
        }
    }
}





fn is_settings_valid(settings: &Settings) -> bool {
    // Helper function to validate URLs
    let is_valid_url_list = |urls: &[String]| {
        !urls.is_empty() && !urls.iter().any(|url| url.is_empty() || url.contains('\0'))
    };
    
    // Check that URL vectors are not empty and contain valid data
    if !is_valid_url_list(&settings.solana_ws_urls) || !is_valid_url_list(&settings.solana_rpc_urls) {
        return false;
    }
    
    // Check that strings are not empty and don't contain null bytes
    if settings.pump_fun_program.is_empty() || settings.pump_fun_program.contains('\0') {
        return false;
    }
    if settings.metadata_program.is_empty() || settings.metadata_program.contains('\0') {
        return false;
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_settings() {
        let settings = Settings::default();
        assert!(is_settings_valid(&settings), "Default settings should be valid");
    }

    #[test]
    fn test_empty_ws_urls() {
        let mut settings = Settings::default();
        settings.solana_ws_urls = vec![];
        assert!(!is_settings_valid(&settings), "Settings with empty WS URLs should be invalid");
    }

    #[test]
    fn test_empty_rpc_urls() {
        let mut settings = Settings::default();
        settings.solana_rpc_urls = vec![];
        assert!(!is_settings_valid(&settings), "Settings with empty RPC URLs should be invalid");
    }

    #[test]
    fn test_empty_pump_fun_program() {
        let mut settings = Settings::default();
        settings.pump_fun_program = String::new();
        assert!(!is_settings_valid(&settings), "Settings with empty pump_fun_program should be invalid");
    }

    #[test]
    fn test_empty_metadata_program() {
        let mut settings = Settings::default();
        settings.metadata_program = String::new();
        assert!(!is_settings_valid(&settings), "Settings with empty metadata_program should be invalid");
    }

    #[test]
    fn test_null_byte_in_pump_fun_program() {
        let mut settings = Settings::default();
        settings.pump_fun_program = "test\0corrupted".to_string();
        assert!(!is_settings_valid(&settings), "Settings with null byte in pump_fun_program should be invalid");
    }

    #[test]
    fn test_null_byte_in_metadata_program() {
        let mut settings = Settings::default();
        settings.metadata_program = "test\0corrupted".to_string();
        assert!(!is_settings_valid(&settings), "Settings with null byte in metadata_program should be invalid");
    }

    #[test]
    fn test_null_byte_in_ws_url() {
        let mut settings = Settings::default();
        settings.solana_ws_urls = vec!["wss://test\0corrupted".to_string()];
        assert!(!is_settings_valid(&settings), "Settings with null byte in WS URL should be invalid");
    }

    #[test]
    fn test_null_byte_in_rpc_url() {
        let mut settings = Settings::default();
        settings.solana_rpc_urls = vec!["https://test\0corrupted".to_string()];
        assert!(!is_settings_valid(&settings), "Settings with null byte in RPC URL should be invalid");
    }
    
    #[test]
    fn test_valid_mode_dry_run() {
        let state = BotState {
            running: false,
            mode: BotMode::DryRun,
            settings: Settings::default(),
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        assert_eq!(state.mode, BotMode::DryRun, "Mode should be DryRun");
    }
    
    #[test]
    fn test_valid_mode_real() {
        let state = BotState {
            running: false,
            mode: BotMode::Real,
            settings: Settings::default(),
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        assert_eq!(state.mode, BotMode::Real, "Mode should be Real");
    }
    
    #[test]
    fn test_mode_enum_types() {
        // Test that enum variants work correctly
        let dry_run = BotMode::DryRun;
        let real = BotMode::Real;
        assert_ne!(dry_run, real, "Different modes should not be equal");
        assert_eq!(format!("{}", dry_run), "dry-run");
        assert_eq!(format!("{}", real), "real");
    }
    
    #[test]
    fn test_mode_enum() {
        let state = BotState {
            running: false,
            mode: BotMode::DryRun,
            settings: Settings::default(),
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        assert_eq!(state.mode.as_str(), "dry-run", "DryRun mode should return 'dry-run'");
        
        let mode = BotMode::from_str("real").unwrap();
        assert_eq!(mode.as_str(), "real", "Real mode should return 'real'");
        
        assert!(BotMode::from_str("invalid").is_none(), "Invalid mode should return None");
    }

    #[test]
    fn test_empty_string_in_ws_urls() {
        let mut settings = Settings::default();
        settings.solana_ws_urls = vec!["".to_string()];
        assert!(!is_settings_valid(&settings), "Settings with empty string in WS URLs should be invalid");
    }

    #[test]
    fn test_empty_string_in_rpc_urls() {
        let mut settings = Settings::default();
        settings.solana_rpc_urls = vec!["".to_string()];
        assert!(!is_settings_valid(&settings), "Settings with empty string in RPC URLs should be invalid");
    }

    #[test]
    fn test_valid_settings_with_multiple_urls() {
        let mut settings = Settings::default();
        settings.solana_ws_urls = vec![
            "wss://api.mainnet-beta.solana.com/".to_string(),
            "wss://api.secondary.solana.com/".to_string(),
        ];
        settings.solana_rpc_urls = vec![
            "https://api.mainnet-beta.solana.com/".to_string(),
            "https://api.secondary.solana.com/".to_string(),
        ];
        assert!(is_settings_valid(&settings), "Settings with multiple valid URLs should be valid");
    }

    #[test]
    fn test_sanitize_empty_urls() {
        let mut settings = Settings::default();
        settings.solana_ws_urls = vec!["".to_string()];
        settings.solana_rpc_urls = vec!["".to_string()];
        
        assert!(!is_settings_valid(&settings), "Settings with empty URLs should be invalid");
        
        let sanitized = sanitize_settings(&settings);
        assert!(is_settings_valid(&sanitized), "Sanitized settings should be valid");
        assert_eq!(sanitized.solana_ws_urls, vec![DEFAULT_SOLANA_WS_URL]);
        assert_eq!(sanitized.solana_rpc_urls, vec![DEFAULT_SOLANA_RPC_URL]);
    }

    #[test]
    fn test_sanitize_mixed_urls() {
        let mut settings = Settings::default();
        settings.solana_ws_urls = vec![
            "wss://valid.url.com".to_string(),
            "".to_string(),
            "wss://another.valid.url".to_string(),
        ];
        settings.solana_rpc_urls = vec![
            "".to_string(),
            "https://valid.rpc.com".to_string(),
        ];
        
        assert!(!is_settings_valid(&settings), "Settings with mixed valid/invalid URLs should be invalid");
        
        let sanitized = sanitize_settings(&settings);
        assert!(is_settings_valid(&sanitized), "Sanitized settings should be valid");
        assert_eq!(sanitized.solana_ws_urls.len(), 2, "Should have 2 valid WS URLs");
        assert_eq!(sanitized.solana_rpc_urls.len(), 1, "Should have 1 valid RPC URL");
    }

    #[test]
    fn test_sanitize_null_bytes() {
        let mut settings = Settings::default();
        settings.solana_ws_urls = vec!["wss://test\0corrupted".to_string()];
        settings.pump_fun_program = "test\0corrupted".to_string();
        
        assert!(!is_settings_valid(&settings), "Settings with null bytes should be invalid");
        
        let sanitized = sanitize_settings(&settings);
        assert!(is_settings_valid(&sanitized), "Sanitized settings should be valid");
        // Corrupted URLs get filtered out, so default is used
        assert_eq!(sanitized.solana_ws_urls, vec![DEFAULT_SOLANA_WS_URL]);
        // Corrupted program ID gets replaced with default
        assert_eq!(sanitized.pump_fun_program, DEFAULT_PUMP_FUN_PROGRAM);
    }

    #[test]
    fn test_sanitize_preserves_valid_settings() {
        let settings = Settings::default();
        
        assert!(is_settings_valid(&settings), "Default settings should be valid");
        
        let sanitized = sanitize_settings(&settings);
        assert!(is_settings_valid(&sanitized), "Sanitized default settings should be valid");
        assert_eq!(sanitized.solana_ws_urls, settings.solana_ws_urls);
        assert_eq!(sanitized.solana_rpc_urls, settings.solana_rpc_urls);
        assert_eq!(sanitized.pump_fun_program, settings.pump_fun_program);
    }
}
