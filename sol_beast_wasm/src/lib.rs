// Sol Beast WASM Bindings
// Browser-compatible trading bot

use wasm_bindgen::prelude::*;
use sol_beast_core::models::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use log::info;

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
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub details: Option<String>,
}

#[wasm_bindgen]
impl SolBeastBot {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let state = BotState {
            running: false,
            mode: "dry-run".to_string(),
            settings: BotSettings::default(),
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
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
        
        let mut state = self.state.lock().unwrap();
        state.settings = settings;
        Ok(())
    }
    
    /// Start the bot
    #[wasm_bindgen]
    pub fn start(&self) -> Result<(), JsValue> {
        let mut state = self.state.lock().unwrap();
        if state.running {
            return Err(JsValue::from_str("Bot is already running"));
        }
        
        let mode = state.mode.clone();
        let ws_url = state.settings.solana_ws_urls.first()
            .ok_or_else(|| JsValue::from_str("No WebSocket URL configured"))?
            .clone();
        let pump_fun_program = state.settings.pump_fun_program.clone();
        
        // Create logging callback that adds logs to state
        let state_for_logs = self.state.clone();
        let log_callback = Arc::new(Mutex::new(move |level: String, message: String, details: String| {
            if let Ok(mut s) = state_for_logs.lock() {
                s.logs.push(LogEntry {
                    timestamp: js_sys::Date::new_0().to_iso_string().as_string().unwrap(),
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
        }));
        
        // Create and start monitor
        let mut monitor = Monitor::new();
        monitor.start(&ws_url, &pump_fun_program, log_callback)
            .map_err(|e| JsValue::from_str(&format!("Failed to start monitor: {:?}", e)))?;
        
        state.monitor = Some(monitor);
        state.running = true;
        
        state.logs.push(LogEntry {
            timestamp: js_sys::Date::new_0().to_iso_string().as_string().unwrap(),
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
        let mut state = self.state.lock().unwrap();
        if !state.running {
            return Err(JsValue::from_str("Bot is not running"));
        }
        
        // Stop monitoring
        if let Some(mut monitor) = state.monitor.take() {
            monitor.stop()
                .map_err(|e| JsValue::from_str(&format!("Failed to stop monitor: {:?}", e)))?;
        }
        
        state.running = false;
        state.logs.push(LogEntry {
            timestamp: js_sys::Date::new_0().to_iso_string().as_string().unwrap(),
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
        self.state.lock().unwrap().running
    }
    
    /// Set bot mode (dry-run or real)
    #[wasm_bindgen]
    pub fn set_mode(&self, mode: &str) -> Result<(), JsValue> {
        let mut state = self.state.lock().unwrap();
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
        self.state.lock().unwrap().mode.clone()
    }
    
    /// Update settings (only when stopped)
    #[wasm_bindgen]
    pub fn update_settings(&self, settings_json: &str) -> Result<(), JsValue> {
        let state = self.state.lock().unwrap();
        if state.running {
            return Err(JsValue::from_str("Cannot update settings while bot is running"));
        }
        drop(state);
        
        let settings: BotSettings = serde_json::from_str(settings_json)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse settings: {}", e)))?;
        
        let mut state = self.state.lock().unwrap();
        state.settings = settings;
        Ok(())
    }
    
    /// Get current settings as JSON
    #[wasm_bindgen]
    pub fn get_settings(&self) -> Result<String, JsValue> {
        let state = self.state.lock().unwrap();
        serde_json::to_string(&state.settings)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize settings: {}", e)))
    }
    
    /// Get logs as JSON
    #[wasm_bindgen]
    pub fn get_logs(&self) -> Result<String, JsValue> {
        let state = self.state.lock().unwrap();
        serde_json::to_string(&state.logs)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize logs: {}", e)))
    }
    
    /// Get holdings as JSON
    #[wasm_bindgen]
    pub fn get_holdings(&self) -> Result<String, JsValue> {
        let state = self.state.lock().unwrap();
        serde_json::to_string(&state.holdings)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize holdings: {}", e)))
    }
    
    /// Connect to Solana RPC (for testing connection)
    #[wasm_bindgen]
    pub async fn test_rpc_connection(&self) -> Result<String, JsValue> {
        use sol_beast_core::wasm::WasmRpcClient;
        
        let state = self.state.lock().unwrap();
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
        
        let state = self.state.lock().unwrap();
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
        
        let state = self.state.lock().unwrap();
        save_settings(&state.settings)?;
        
        Ok(())
    }
    
    /// Load state from localStorage
    #[wasm_bindgen]
    pub fn load_from_storage(&self) -> Result<(), JsValue> {
        use sol_beast_core::wasm::load_settings;
        
        if let Some(settings) = load_settings::<BotSettings>()? {
            let mut state = self.state.lock().unwrap();
            state.settings = settings;
        }
        
        Ok(())
    }
}


impl Default for BotSettings {
    fn default() -> Self {
        Self {
            solana_ws_urls: vec!["wss://solana-mainnet.core.chainstack.com/".to_string()],
            solana_rpc_urls: vec!["https://solana-mainnet.core.chainstack.com/".to_string()],
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
        }
    }
}
