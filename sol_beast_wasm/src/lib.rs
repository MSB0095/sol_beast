// Sol Beast WASM Bindings
// Browser-compatible trading bot

use wasm_bindgen::prelude::*;
use sol_beast_core::models::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use log::{info, error};

mod monitor;
use monitor::Monitor;

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
    mode: String, // "dry-run" or "real"
    settings: BotSettings,
    holdings: Vec<Holding>,
    logs: Vec<LogEntry>,
    monitor: Option<Monitor>,
    detected_tokens: Vec<DetectedToken>, // Phase 2: Track detected tokens
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BotSettings {
    pub solana_ws_urls: Vec<String>,
    pub solana_rpc_urls: Vec<String>,
    pub pump_fun_program: String,
    pub metadata_program: String,
    pub tp_percent: f64,
    pub sl_percent: f64,
    pub timeout_secs: i64,
    pub buy_amount: f64,
    pub max_holded_coins: usize,
    pub slippage_bps: u64,
    pub min_tokens_threshold: u64,
    pub max_sol_per_token: f64,
    pub min_liquidity_sol: f64,
    pub max_liquidity_sol: f64,
    #[serde(default = "default_dev_tip_percent")]
    pub dev_tip_percent: f64,
    #[serde(default = "default_dev_tip_fixed_sol")]
    pub dev_tip_fixed_sol: f64,
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
        // Settings loading hierarchy for WASM deployment:
        // 1. Try localStorage (user's saved settings from previous session)
        // 2. Fall back to built-in defaults if localStorage is empty or corrupted
        // Note: The frontend will also try loading from static bot-settings.json
        // if it detects settings are invalid after bot initialization.
        let settings = match sol_beast_core::wasm::load_settings::<BotSettings>() {
            Ok(Some(saved_settings)) => {
                info!("Loaded settings from localStorage");
                saved_settings
            },
            Ok(None) => {
                info!("No saved settings found, using defaults");
                BotSettings::default()
            },
            Err(e) => {
                error!("Failed to load settings from localStorage: {:?}, using defaults", e);
                BotSettings::default()
            }
        };
        
        let state = BotState {
            running: false,
            mode: "dry-run".to_string(),
            settings,
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }
    
    /// Initialize bot with settings
    #[wasm_bindgen]
    pub fn init_with_settings(&mut self, settings_json: &str) -> Result<(), JsValue> {
        let settings: BotSettings = serde_json::from_str(settings_json)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse settings: {}", e)))?;
        
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
        
        let mode = state.mode.clone();
        let ws_url = state.settings.solana_ws_urls.first()
            .ok_or_else(|| JsValue::from_str("No WebSocket URL configured"))?
            .clone();
        let pump_fun_program = state.settings.pump_fun_program.clone();
        
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
        
        // Create and start monitor
        let mut monitor = Monitor::new();
        monitor.start(&ws_url, &pump_fun_program, log_callback)
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
        state.logs.push(LogEntry {
            timestamp,
            level: "info".to_string(),
            message: "✓ Bot started successfully".to_string(),
            details: Some(format!("Mode: {}\nWebSocket: {}\nProgram: {}\n\nThe bot is now monitoring for new pump.fun tokens. Logs will appear as transactions are detected.", mode, ws_url, pump_fun_program)),
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
        match self.state.lock() {
            Ok(guard) => guard.running,
            Err(poisoned) => {
                info!("Mutex was poisoned in is_running, recovering...");
                poisoned.into_inner().running
            }
        }
    }
    
    /// Set bot mode (dry-run or real)
    #[wasm_bindgen]
    pub fn set_mode(&self, mode: &str) -> Result<(), JsValue> {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in set_mode, recovering...");
                poisoned.into_inner()
            }
        };
        if state.running {
            return Err(JsValue::from_str("Cannot change mode while bot is running"));
        }
        if mode != "dry-run" && mode != "real" {
            return Err(JsValue::from_str("Mode must be 'dry-run' or 'real'"));
        }
        state.mode = mode.to_string();
        Ok(())
    }
    
    /// Get current mode
    #[wasm_bindgen]
    pub fn get_mode(&self) -> String {
        match self.state.lock() {
            Ok(guard) => guard.mode.clone(),
            Err(poisoned) => {
                info!("Mutex was poisoned in get_mode, recovering...");
                poisoned.into_inner().mode.clone()
            }
        }
    }
    
    /// Update settings
    /// If bot is running and critical settings change (WebSocket URL, program ID),
    /// the bot will need to be restarted manually for changes to take effect.
    #[wasm_bindgen]
    pub fn update_settings(&self, settings_json: &str) -> Result<(), JsValue> {
        // Parse settings first (outside any lock)
        let settings: BotSettings = serde_json::from_str(settings_json)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse settings: {}", e)))?;
        
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
        drop(state); // Release lock before localStorage operation
        
        // Save to localStorage after successful state update
        sol_beast_core::wasm::save_settings(&settings)
            .map_err(|e| {
                error!("Failed to save settings to localStorage: {:?}", e);
                JsValue::from_str(&format!("Settings updated but failed to save to localStorage: {:?}", e))
            })?;
        
        // Re-acquire lock to add log entries
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
    #[wasm_bindgen]
    pub fn get_settings(&self) -> Result<String, JsValue> {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in get_settings, recovering...");
                poisoned.into_inner()
            }
        };
        
        match serde_json::to_string(&state.settings) {
            Ok(json) => Ok(json),
            Err(e) => {
                error!("Failed to serialize settings: {}", e);
                Err(JsValue::from_str(&format!("Failed to serialize settings: {}", e)))
            }
        }
    }
    
    /// Get logs as JSON
    #[wasm_bindgen]
    pub fn get_logs(&self) -> Result<String, JsValue> {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in get_logs, recovering...");
                poisoned.into_inner()
            }
        };
        
        match serde_json::to_string(&state.logs) {
            Ok(json) => Ok(json),
            Err(e) => {
                error!("Failed to serialize logs: {}", e);
                Err(JsValue::from_str(&format!("Failed to serialize logs: {}", e)))
            }
        }
    }
    
    /// Get holdings as JSON
    #[wasm_bindgen]
    pub fn get_holdings(&self) -> Result<String, JsValue> {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in get_holdings, recovering...");
                poisoned.into_inner()
            }
        };
        
        match serde_json::to_string(&state.holdings) {
            Ok(json) => Ok(json),
            Err(e) => {
                error!("Failed to serialize holdings: {}", e);
                Err(JsValue::from_str(&format!("Failed to serialize holdings: {}", e)))
            }
        }
    }
    
    /// Get detected tokens as JSON (Phase 2 feature)
    #[wasm_bindgen]
    pub fn get_detected_tokens(&self) -> Result<String, JsValue> {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in get_detected_tokens, recovering...");
                poisoned.into_inner()
            }
        };
        
        match serde_json::to_string(&state.detected_tokens) {
            Ok(json) => Ok(json),
            Err(e) => {
                error!("Failed to serialize detected tokens: {}", e);
                Err(JsValue::from_str(&format!("Failed to serialize detected tokens: {}", e)))
            }
        }
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
        use sol_beast_core::wasm::save_settings;
        
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in save_to_storage, recovering...");
                poisoned.into_inner()
            }
        };
        save_settings(&state.settings)?;
        
        Ok(())
    }
    
    /// Load state from localStorage
    #[wasm_bindgen]
    pub fn load_from_storage(&self) -> Result<(), JsValue> {
        use sol_beast_core::wasm::load_settings;
        
        if let Some(settings) = load_settings::<BotSettings>()? {
            let mut state = match self.state.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    info!("Mutex was poisoned in load_from_storage, recovering...");
                    poisoned.into_inner()
                }
            };
            state.settings = settings;
        }
        
        Ok(())
    }
}


impl Default for BotSettings {
    fn default() -> Self {
        Self {
            solana_ws_urls: vec!["wss://api.mainnet-beta.solana.com/".to_string()],
            solana_rpc_urls: vec!["https://api.mainnet-beta.solana.com/".to_string()],
            pump_fun_program: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string(),
            metadata_program: "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s".to_string(),
            tp_percent: 100.0,
            sl_percent: -50.0,
            timeout_secs: 50,
            buy_amount: 0.001,
            max_holded_coins: 4,
            slippage_bps: 500,
            min_tokens_threshold: 30000,
            max_sol_per_token: 0.002,
            min_liquidity_sol: 0.0,
            max_liquidity_sol: 15.0,
            dev_tip_percent: 2.0,
            dev_tip_fixed_sol: 0.0,
        }
    }
}

fn default_dev_tip_percent() -> f64 { 2.0 }
fn default_dev_tip_fixed_sol() -> f64 { 0.0 }
