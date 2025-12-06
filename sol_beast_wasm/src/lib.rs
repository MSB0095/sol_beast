// Sol Beast WASM Bindings
// Browser-compatible trading bot

use wasm_bindgen::prelude::*;
use sol_beast_core::models::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use log::{info, error, warn};

mod monitor;
use monitor::Monitor;

// Use wee_alloc as the global allocator for smaller WASM size and better memory management
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// Constants for configuration defaults and limits
const DEFAULT_SOLANA_RPC_URL: &str = "https://api.mainnet-beta.solana.com/";
const DEFAULT_SOLANA_WS_URL: &str = "wss://api.mainnet-beta.solana.com/";
const DEFAULT_PUMP_FUN_PROGRAM: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const DEFAULT_METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";
const DEFAULT_CACHE_CAPACITY: usize = 1000;
const DEFAULT_PRICE_CACHE_TTL_SECS: u64 = 60;
const MAX_FETCH_RETRIES: u8 = 3;
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
    mode: String, // "dry-run" or "real"
    settings: BotSettings,
    holdings: Vec<HoldingWithMint>,
    logs: Vec<LogEntry>,
    monitor: Option<Monitor>,
    detected_tokens: Vec<DetectedToken>, // Phase 2: Track detected tokens
}

impl BotState {
    /// Validate that mode is a valid string
    fn is_mode_valid(&self) -> bool {
        self.mode == "dry-run" || self.mode == "real"
    }
    
    /// Repair mode if it's corrupted
    fn repair_mode_if_needed(&mut self) {
        if !self.is_mode_valid() {
            error!("Invalid mode '{}' detected, resetting to 'dry-run'", self.mode);
            self.mode = "dry-run".to_string();
        }
    }
}

/// Holdings with mint address (for WASM serialization to frontend)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HoldingWithMint {
    pub mint: String,
    pub amount: u64,
    pub buy_price: f64,
    pub buy_time: String, // ISO 8601 timestamp
    pub metadata: Option<OffchainTokenMetadata>,
    pub onchain: Option<OnchainFullMetadata>,
    pub onchain_raw: Option<Vec<u8>>,
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
    #[serde(default = "default_enable_safer_sniping")]
    pub enable_safer_sniping: bool,
    #[serde(default)]
    pub shyft_api_key: Option<String>,
    #[serde(default = "default_shyft_graphql_url")]
    pub shyft_graphql_url: String,
}

fn default_shyft_graphql_url() -> String { "https://programs.shyft.to/v0/graphql".to_string() }

impl BotSettings {
    /// Validate that settings contain valid data (no corrupted strings or vectors)
    /// Returns true if settings are valid, false if corrupted
    fn is_valid(&self) -> bool {
        // Helper function to validate URLs
        let is_valid_url_list = |urls: &[String]| {
            !urls.is_empty() && !urls.iter().any(|url| url.is_empty() || url.contains('\0'))
        };
        
        // Check that URL vectors are not empty and contain valid data
        if !is_valid_url_list(&self.solana_ws_urls) || !is_valid_url_list(&self.solana_rpc_urls) {
            return false;
        }
        
        // Check that strings are not empty and don't contain null bytes
        if self.pump_fun_program.is_empty() || self.pump_fun_program.contains('\0') {
            return false;
        }
        if self.metadata_program.is_empty() || self.metadata_program.contains('\0') {
            return false;
        }
        
        true
    }
    
    /// Sanitize settings by removing empty URLs and invalid data
    /// Returns a new BotSettings with cleaned data, or None if settings cannot be repaired
    fn sanitize(&self) -> Option<Self> {
        // Filter out empty URLs and URLs containing null bytes
        let clean_ws_urls: Vec<String> = self.solana_ws_urls
            .iter()
            .filter(|url| !url.is_empty() && !url.contains('\0'))
            .cloned()
            .collect();
        
        let clean_rpc_urls: Vec<String> = self.solana_rpc_urls
            .iter()
            .filter(|url| !url.is_empty() && !url.contains('\0'))
            .cloned()
            .collect();
        
        // If we have no valid URLs after filtering, use defaults
        let ws_urls = if clean_ws_urls.is_empty() {
            vec![DEFAULT_SOLANA_WS_URL.to_string()]
        } else {
            clean_ws_urls
        };
        
        let rpc_urls = if clean_rpc_urls.is_empty() {
            vec![DEFAULT_SOLANA_RPC_URL.to_string()]
        } else {
            clean_rpc_urls
        };
        
        // Check program IDs
        let pump_fun_program = if self.pump_fun_program.is_empty() || self.pump_fun_program.contains('\0') {
            DEFAULT_PUMP_FUN_PROGRAM.to_string()
        } else {
            self.pump_fun_program.clone()
        };
        
        let metadata_program = if self.metadata_program.is_empty() || self.metadata_program.contains('\0') {
            DEFAULT_METADATA_PROGRAM.to_string()
        } else {
            self.metadata_program.clone()
        };
        
        Some(Self {
            solana_ws_urls: ws_urls,
            solana_rpc_urls: rpc_urls,
            pump_fun_program,
            metadata_program,
            tp_percent: self.tp_percent,
            sl_percent: self.sl_percent,
            timeout_secs: self.timeout_secs,
            buy_amount: self.buy_amount,
            max_holded_coins: self.max_holded_coins,
            slippage_bps: self.slippage_bps,
            min_tokens_threshold: self.min_tokens_threshold,
            max_sol_per_token: self.max_sol_per_token,
            min_liquidity_sol: self.min_liquidity_sol,
            max_liquidity_sol: self.max_liquidity_sol,
            dev_tip_percent: self.dev_tip_percent,
            dev_tip_fixed_sol: self.dev_tip_fixed_sol,
            enable_safer_sniping: self.enable_safer_sniping,
            shyft_api_key: self.shyft_api_key.clone(),
            shyft_graphql_url: self.shyft_graphql_url.clone(),
        })
    }
    
    /// Convert BotSettings to core Settings
    pub fn to_core_settings(&self) -> sol_beast_core::settings::Settings {
        sol_beast_core::settings::Settings {
            solana_ws_urls: self.solana_ws_urls.clone(),
            solana_rpc_urls: self.solana_rpc_urls.clone(),
            pump_fun_program: self.pump_fun_program.clone(),
            metadata_program: self.metadata_program.clone(),
            wallet_keypair_path: None,
            wallet_keypair_json: None,
            wallet_private_key_string: None,
            simulate_wallet_private_key_string: None,
            tp_percent: self.tp_percent,
            sl_percent: self.sl_percent,
            timeout_secs: self.timeout_secs,
            cache_capacity: DEFAULT_CACHE_CAPACITY,
            price_cache_ttl_secs: DEFAULT_PRICE_CACHE_TTL_SECS,
            buy_amount: self.buy_amount,
            price_source: "wss".to_string(),
            rotate_rpc: false,
            rpc_rotate_interval_secs: 60,
            max_holded_coins: self.max_holded_coins,
            max_subs_per_wss: 4,
            sub_ttl_secs: 900,
            wss_subscribe_timeout_secs: 6,
            max_create_to_buy_secs: 6,
            bonding_curve_strict: false,
            bonding_curve_log_debounce_secs: 300,
            simulate_wallet_keypair_json: None,
            min_tokens_threshold: self.min_tokens_threshold,
            max_sol_per_token: self.max_sol_per_token,
            slippage_bps: self.slippage_bps,
            enable_safer_sniping: self.enable_safer_sniping,
            min_liquidity_sol: self.min_liquidity_sol,
            max_liquidity_sol: self.max_liquidity_sol,
            helius_sender_enabled: false,
            helius_api_key: None,
            helius_sender_endpoint: "https://sender.helius-rpc.com/fast".to_string(),
            helius_min_tip_sol: 0.001,
            helius_priority_fee_multiplier: 1.2,
            helius_use_swqos_only: false,
            helius_use_dynamic_tips: true,
            helius_confirm_timeout_secs: 15,
            dev_fee_enabled: false,
            dev_wallet_address: None,
            dev_tip_percent: self.dev_tip_percent,
            dev_tip_fixed_sol: self.dev_tip_fixed_sol,
            shyft_api_key: self.shyft_api_key.clone(),
            shyft_graphql_url: self.shyft_graphql_url.clone(),
        }
    }
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
        // 2. Validate loaded settings to ensure they're not corrupted
        // 3. Fall back to built-in defaults if localStorage is empty or corrupted
        // Note: The frontend will also try loading from static bot-settings.json
        // if it detects settings are invalid after bot initialization.
        let settings = match sol_beast_core::wasm::load_settings::<BotSettings>() {
            Ok(Some(saved_settings)) => {
                // Validate the loaded settings to prevent memory access errors
                if saved_settings.is_valid() {
                    info!("Loaded valid settings from localStorage");
                    saved_settings
                } else {
                    warn!("Loaded settings from localStorage are invalid, attempting to sanitize");
                    // Try to sanitize the settings first before falling back to defaults
                    match saved_settings.sanitize() {
                        Some(sanitized) if sanitized.is_valid() => {
                            info!("Successfully sanitized settings from localStorage");
                            // Save the sanitized settings back to localStorage
                            if let Err(e) = sol_beast_core::wasm::save_settings(&sanitized) {
                                error!("Failed to save sanitized settings: {:?}", e);
                            }
                            sanitized
                        }
                        _ => {
                            error!("Failed to sanitize settings, clearing and using defaults");
                            // Clear corrupted data from localStorage
                            if let Err(e) = sol_beast_core::wasm::storage::clear_all() {
                                error!("Failed to clear corrupted localStorage: {:?}", e);
                            }
                            BotSettings::default()
                        }
                    }
                }
            },
            Ok(None) => {
                info!("No saved settings found, using defaults");
                BotSettings::default()
            },
            Err(e) => {
                error!("Failed to load settings from localStorage: {:?}, using defaults", e);
                // Try to clear potentially corrupted data
                if let Err(clear_err) = sol_beast_core::wasm::storage::clear_all() {
                    error!("Failed to clear localStorage after error: {:?}", clear_err);
                }
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
        // Acquire mutable lock to allow mode repair if needed
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in start, recovering...");
                poisoned.into_inner()
            }
        };
        if state.running {
            return Err(JsValue::from_str("Bot is already running"));
        }
        
        // Repair mode if needed before starting
        state.repair_mode_if_needed();
        
        let mode = state.mode.clone();
        let shyft_api_key = state.settings.shyft_api_key.clone();
        let shyft_graphql_url = state.settings.shyft_graphql_url.clone();
        
        let (ws_url, is_shyft) = if let Some(key) = shyft_api_key {
             (format!("{}?api_key={}", shyft_graphql_url.replace("https", "wss").replace("http", "ws"), key), true)
        } else {
             (state.settings.solana_ws_urls.first()
                .ok_or_else(|| JsValue::from_str("No WebSocket URL configured"))?
                .clone(), false)
        };

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
        monitor.start(&ws_url, &pump_fun_program, log_callback, Some(signature_callback), is_shyft)
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
    /// 
    /// MEMORY SAFETY: This function implements multiple layers of validation to prevent
    /// memory access errors when changing the bot mode:
    /// 1. Validates input string before acquiring any locks
    /// 2. Checks for null bytes that could cause WASM memory issues
    /// 3. Uses mutex poisoning recovery to handle panics in other threads
    /// 4. Creates a fresh String allocation to avoid memory corruption
    #[wasm_bindgen]
    pub fn set_mode(&self, mode: &str) -> Result<(), JsValue> {
        // Validate input before acquiring lock to prevent corruption
        // Check for valid modes and ensure no null bytes (defense in depth)
        if mode.is_empty() || mode.len() > 50 {
            return Err(JsValue::from_str("Invalid mode length"));
        }
        
        if (mode != "dry-run" && mode != "real") || mode.contains('\0') {
            return Err(JsValue::from_str("Mode must be 'dry-run' or 'real'"));
        }
        
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
        
        // Create new string to ensure clean memory allocation
        // This prevents any potential corruption from being carried forward
        state.mode = mode.to_string();
        
        // Verify the mode was set correctly (defense in depth)
        if !state.is_mode_valid() {
            error!("Mode validation failed after setting, forcing to dry-run");
            state.mode = "dry-run".to_string();
        }
        
        info!("Bot mode changed to: {}", state.mode);
        Ok(())
    }
    
    /// Get current mode
    /// 
    /// MEMORY SAFETY: This function always returns a valid mode string:
    /// 1. Recovers from mutex poisoning gracefully
    /// 2. Automatically repairs corrupted mode values
    /// 3. Returns a freshly cloned string to avoid memory issues
    #[wasm_bindgen]
    pub fn get_mode(&self) -> String {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in get_mode, recovering...");
                poisoned.into_inner()
            }
        };
        
        // Repair mode if it's corrupted (e.g., from bad localStorage data)
        state.repair_mode_if_needed();
        
        // Return a fresh clone of the validated mode
        state.mode.clone()
    }
    
    /// Update settings
    /// If bot is running and critical settings change (WebSocket URL, program ID),
    /// the bot will need to be restarted manually for changes to take effect.
    #[wasm_bindgen]
    pub fn update_settings(&self, settings_json: &str) -> Result<(), JsValue> {
        // Parse settings first (outside any lock)
        let mut settings: BotSettings = serde_json::from_str(settings_json)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse settings: {}", e)))?;
        
        // Validate and sanitize parsed settings before updating
        if !settings.is_valid() {
            warn!("Received invalid settings, attempting to sanitize");
            settings = settings.sanitize()
                .ok_or_else(|| JsValue::from_str("Invalid settings: missing or corrupted required fields"))?;
            
            // Verify sanitized settings are valid
            if !settings.is_valid() {
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
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                info!("Mutex was poisoned in get_settings, recovering...");
                poisoned.into_inner()
            }
        };
        
        // Repair mode if needed before any operations
        // This prevents mode corruption from propagating to the settings response
        state.repair_mode_if_needed();
        
        // Validate settings before serialization to prevent memory access errors
        if !state.settings.is_valid() {
            error!("Settings validation failed - attempting to sanitize");
            
            // Try to sanitize the settings
            if let Some(sanitized) = state.settings.sanitize() {
                if sanitized.is_valid() {
                    info!("Successfully sanitized settings");
                    state.settings = sanitized;
                    // Save the sanitized settings to prevent future errors
                    if let Err(e) = sol_beast_core::wasm::save_settings(&state.settings) {
                        error!("Failed to save sanitized settings: {:?}", e);
                    }
                } else {
                    error!("Sanitization failed, falling back to defaults");
                    state.settings = BotSettings::default();
                }
            } else {
                error!("Cannot sanitize settings, using defaults");
                state.settings = BotSettings::default();
            }
        }
        
        // Attempt to serialize the validated/sanitized settings
        match serde_json::to_string(&state.settings) {
            Ok(json) => {
                // Ensure we have valid UTF-8 JSON without null bytes
                if json.is_empty() || json.contains('\0') {
                    error!("Settings serialized to invalid JSON (empty or contains null bytes), using defaults");
                    return match serde_json::to_string(&BotSettings::default()) {
                        Ok(default_json) => Ok(default_json),
                        Err(e) => Err(JsValue::from_str(&format!("Failed to serialize default settings: {}", e)))
                    };
                }
                Ok(json)
            },
            Err(e) => {
                error!("Failed to serialize settings: {}, falling back to defaults", e);
                // Return default settings as fallback
                match serde_json::to_string(&BotSettings::default()) {
                    Ok(default_json) => Ok(default_json),
                    Err(e2) => Err(JsValue::from_str(&format!("Failed to serialize settings: {}. Failed to serialize defaults: {}", e, e2)))
                }
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
            let holding = HoldingWithMint {
                mint: mint.to_string(),
                amount,
                buy_price,
                buy_time: Utc::now().to_rfc3339(),
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
            
            // Calculate profit/loss
            let profit_percent = if holding.buy_price != 0.0 {
                ((current_price - holding.buy_price) / holding.buy_price) * 100.0
            } else {
                0.0
            };
            
            // Parse buy time
            let buy_time = match chrono::DateTime::parse_from_rfc3339(&holding.buy_time) {
                Ok(dt) => dt.with_timezone(&Utc),
                Err(e) => {
                    warn!("Failed to parse buy time for {}: {}", holding.mint, e);
                    continue;
                }
            };
            
            let elapsed_secs = Utc::now().signed_duration_since(buy_time).num_seconds();
            
            // Check TP/SL/Timeout conditions
            let should_sell = if profit_percent >= settings.tp_percent {
                Some(("TP", profit_percent, current_price))
            } else if profit_percent <= settings.sl_percent {
                Some(("SL", profit_percent, current_price))
            } else if elapsed_secs >= settings.timeout_secs {
                Some(("TIMEOUT", profit_percent, current_price))
            } else {
                None
            };
            
            if let Some((reason, pct, price)) = should_sell {
                info!("{} triggered for {}: {:.2}% @ {:.9} SOL", reason, holding.mint, pct, price);
                
                actions.push(serde_json::json!({
                    "action": "sell",
                    "mint": holding.mint,
                    "reason": reason,
                    "profitPercent": pct,
                    "currentPrice": price,
                    "amount": holding.amount,
                }));
                
                // Log to UI
                let mut state = match self.state.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner()
                };
                let timestamp = js_sys::Date::new_0().to_iso_string().as_string()
                    .unwrap_or_else(|| "unknown".to_string());
                state.logs.push(LogEntry {
                    timestamp,
                    level: "warn".to_string(),
                    message: format!("🔔 {} Triggered: {}", reason, 
                                    holding.metadata.as_ref()
                                        .and_then(|m| m.symbol.as_ref())
                                        .unwrap_or(&holding.mint)),
                    details: Some(format!(
                        "Profit: {:.2}%\nCurrent Price: {:.9} SOL\nBuy Price: {:.9} SOL\nHold Time: {}s",
                        pct, price, holding.buy_price, elapsed_secs
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
        
        let core_settings = state.settings.to_core_settings();
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
        let core_settings = state.settings.to_core_settings();
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
    /// 
    /// MEMORY SAFETY: This function safely loads settings from localStorage:
    /// 1. Uses the enhanced load_settings that auto-clears corrupted data
    /// 2. Validates loaded settings before applying them
    /// 3. Sanitizes settings if validation fails
    /// 4. Falls back to current settings if all recovery attempts fail
    /// 5. Handles holdings loading separately with error isolation
    #[wasm_bindgen]
    pub fn load_from_storage(&self) -> Result<(), JsValue> {
        use sol_beast_core::wasm::load_settings;
        
        // Attempt to load settings from localStorage
        // The load_settings function will automatically clear corrupted data
        match load_settings::<BotSettings>() {
            Ok(Some(settings)) => {
                // Validate the loaded settings before applying them
                if settings.is_valid() {
                    let mut state = match self.state.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            info!("Mutex was poisoned in load_from_storage, recovering...");
                            poisoned.into_inner()
                        }
                    };
                    state.settings = settings;
                    info!("Successfully loaded valid settings from localStorage");
                } else {
                    // Try to sanitize the loaded settings
                    warn!("Loaded settings failed validation, attempting to sanitize");
                    if let Some(sanitized) = settings.sanitize() {
                        if sanitized.is_valid() {
                            let mut state = match self.state.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => {
                                    info!("Mutex was poisoned in load_from_storage (sanitize), recovering...");
                                    poisoned.into_inner()
                                }
                            };
                            state.settings = sanitized;
                            info!("Successfully sanitized and loaded settings");
                        } else {
                            warn!("Sanitization produced invalid settings, keeping current settings");
                        }
                    } else {
                        warn!("Cannot sanitize settings, keeping current settings");
                    }
                }
            },
            Ok(None) => {
                info!("No settings found in localStorage");
            },
            Err(e) => {
                // The load_settings function already handled the error and cleared corrupted data
                warn!("Error loading settings from localStorage: {:?}", e);
            }
        }
        
        // Also load holdings from localStorage (isolated error handling)
        if let Err(e) = self.load_holdings_from_storage() {
            warn!("Failed to load holdings from localStorage: {:?}", e);
            // Continue - this is not a critical error
        }
        
        Ok(())
    }
    
    /// Helper: Save holdings to localStorage
    fn save_holdings_to_storage(&self, holdings: &[HoldingWithMint]) -> Result<(), JsValue> {
        let json = serde_json::to_string(holdings)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize holdings: {}", e)))?;
        
        let window = web_sys::window()
            .ok_or_else(|| JsValue::from_str("No window object"))?;
        let storage = window.local_storage()
            .map_err(|e| JsValue::from_str(&format!("Failed to access localStorage: {:?}", e)))?
            .ok_or_else(|| JsValue::from_str("localStorage not available"))?;
        
        storage.set_item("sol_beast_holdings", &json)
            .map_err(|e| JsValue::from_str(&format!("Failed to save to localStorage: {:?}", e)))?;
        
        Ok(())
    }
    
    /// Helper: Load holdings from localStorage
    /// 
    /// MEMORY SAFETY: Safely loads holdings with automatic corruption recovery:
    /// 1. Validates JSON string before deserialization
    /// 2. Catches deserialization errors and clears corrupted data
    /// 3. Continues with empty holdings if data is corrupted
    fn load_holdings_from_storage(&self) -> Result<(), JsValue> {
        let window = web_sys::window()
            .ok_or_else(|| JsValue::from_str("No window object"))?;
        let storage = window.local_storage()
            .map_err(|e| JsValue::from_str(&format!("Failed to access localStorage: {:?}", e)))?
            .ok_or_else(|| JsValue::from_str("localStorage not available"))?;
        
        if let Some(json) = storage.get_item("sol_beast_holdings")
            .map_err(|e| JsValue::from_str(&format!("Failed to read from localStorage: {:?}", e)))? {
            
            // Validate JSON string before attempting deserialization
            if json.is_empty() || json.contains('\0') {
                error!("Corrupted holdings data detected (empty or contains null bytes), clearing...");
                let _ = storage.remove_item("sol_beast_holdings");
                return Ok(());
            }
            
            // Attempt to parse holdings with error recovery
            match serde_json::from_str::<Vec<HoldingWithMint>>(&json) {
                Ok(holdings) => {
                    let mut state = match self.state.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            info!("Mutex was poisoned in load_holdings_from_storage, recovering...");
                            poisoned.into_inner()
                        }
                    };
                    
                    state.holdings = holdings;
                    info!("Loaded {} holdings from localStorage", state.holdings.len());
                },
                Err(e) => {
                    error!("Failed to parse holdings from localStorage: {}", e);
                    // Clear corrupted holdings data
                    let _ = storage.remove_item("sol_beast_holdings");
                    info!("Cleared corrupted holdings data");
                    // Continue with empty holdings
                }
            }
        }
        
        Ok(())
    }
}

/// Process a detected pump.fun signature
/// This is called asynchronously when the monitor detects a new transaction
async fn process_detected_signature(signature: String, state: Arc<Mutex<BotState>>) {
    use sol_beast_core::wasm::{WasmRpcClient, WasmHttpClient};
    use sol_beast_core::transaction_service::{fetch_and_parse_transaction, fetch_complete_token_metadata};
    use sol_beast_core::buyer::evaluate_buy_heuristics;
    use log::{info, error, warn};
    
    info!("Processing detected signature: {}", signature);
    
    // Get settings and configuration from state
    let (rpc_url, metadata_program, pump_fun_program, core_settings) = {
        match state.lock() {
            Ok(s) => {
                let rpc_url = s.settings.solana_rpc_urls.first().cloned()
                    .unwrap_or_else(|| DEFAULT_SOLANA_RPC_URL.to_string());
                let metadata_program = s.settings.metadata_program.clone();
                let pump_fun_program = s.settings.pump_fun_program.clone();
                let core_settings = s.settings.to_core_settings();
                (rpc_url, metadata_program, pump_fun_program, core_settings)
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
    
    // Step 1: Fetch and parse transaction
    let parsed_tx = match fetch_and_parse_transaction(
        &signature,
        &rpc_client,
        &pump_fun_program,
        MAX_FETCH_RETRIES,
    ).await {
        Ok(tx) => {
            info!("Successfully parsed transaction: mint={}, creator={}", tx.mint, tx.creator);
            tx
        },
        Err(e) => {
            error!("Failed to fetch/parse transaction {}: {:?}", signature, e);
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
            return;
        }
    };
    
    // Step 2: Fetch token metadata
    let metadata = match fetch_complete_token_metadata(
        &parsed_tx.mint,
        &metadata_program,
        &rpc_client,
        &http_client,
    ).await {
        Ok(meta) => {
            let name = meta.offchain.as_ref().and_then(|m| m.name.clone())
                .or_else(|| meta.onchain.as_ref().map(|m| m.name.clone()));
            let symbol = meta.offchain.as_ref().and_then(|m| m.symbol.clone())
                .or_else(|| meta.onchain.as_ref().map(|m| m.symbol.clone()));
            info!("Successfully fetched metadata for {}: name={:?}, symbol={:?}", 
                  parsed_tx.mint, name, symbol);
            meta
        },
        Err(e) => {
            warn!("Failed to fetch metadata for {}: {:?}", parsed_tx.mint, e);
            // Continue without metadata
            sol_beast_core::metadata::TokenMetadata {
                onchain: None,
                offchain: None,
                raw_account_data: None,
            }
        }
    };
    
    // Step 3: Fetch bonding curve state and calculate real price
    use sol_beast_core::rpc_client::{fetch_bonding_curve_state, calculate_price_from_bonding_curve, calculate_liquidity_sol};
    
    let (bonding_curve_state, estimated_price, liquidity_sol) = match fetch_bonding_curve_state(
        &parsed_tx.mint,
        &parsed_tx.bonding_curve,
        &rpc_client,
    ).await {
        Ok(state) => {
            let price = calculate_price_from_bonding_curve(&state);
            let liquidity = calculate_liquidity_sol(&state);
            info!("Fetched bonding curve state: price={:.8} SOL, liquidity={:.4} SOL", price, liquidity);
            (Some(state), price, Some(liquidity))
        },
        Err(e) => {
            warn!("Failed to fetch bonding curve state for {}: {:?}, using fallback price", parsed_tx.mint, e);
            (None, FALLBACK_ESTIMATED_PRICE, None)
        }
    };
    
    // Step 4: Evaluate buy heuristics with real price and bonding curve state
    let buy_amount = core_settings.buy_amount;
    
    let evaluation = evaluate_buy_heuristics(
        &parsed_tx.mint,
        buy_amount,
        estimated_price,
        bonding_curve_state.as_ref(),
        &core_settings,
    );
    
    info!("Buy evaluation for {}: should_buy={}, reason={}", 
          parsed_tx.mint, evaluation.should_buy, evaluation.reason);
    
    // Step 5: Extract metadata fields from TokenMetadata
    let name = metadata.offchain.as_ref().and_then(|m| m.name.clone())
        .or_else(|| metadata.onchain.as_ref().map(|m| m.name.clone()));
    let symbol = metadata.offchain.as_ref().and_then(|m| m.symbol.clone())
        .or_else(|| metadata.onchain.as_ref().map(|m| m.symbol.clone()));
    let image_uri = metadata.offchain.as_ref().and_then(|m| m.image.clone());
    let description = metadata.offchain.as_ref().and_then(|m| m.description.clone());
    
    // Create DetectedToken and store in state
    let detected_token = DetectedToken {
        signature: signature.clone(),
        mint: parsed_tx.mint.clone(),
        creator: parsed_tx.creator.clone(),
        bonding_curve: parsed_tx.bonding_curve.clone(),
        holder_address: parsed_tx.holder_address.clone(),
        timestamp: js_sys::Date::new_0().to_iso_string().as_string()
            .unwrap_or_else(|| "unknown".to_string()),
        name: name.clone(),
        symbol: symbol.clone(),
        image_uri,
        description,
        should_buy: evaluation.should_buy,
        evaluation_reason: evaluation.reason.clone(),
        token_amount: Some(evaluation.token_amount),
        buy_price_sol: Some(evaluation.buy_price_sol),
        liquidity_sol
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
        let result_icon = if evaluation.should_buy { "✅" } else { "❌" };
        s.logs.push(LogEntry {
            timestamp,
            level: if evaluation.should_buy { "info" } else { "warn" }.to_string(),
            message: format!("{} Token evaluated: {}", result_icon, 
                           symbol.as_deref().unwrap_or(&parsed_tx.mint)),
            details: Some(format!(
                "Name: {}\nSymbol: {}\nMint: {}\nCreator: {}\nPrice: {:.8} SOL\nLiquidity: {:.4} SOL\n\nEvaluation: {}",
                name.as_deref().unwrap_or("Unknown"),
                symbol.as_deref().unwrap_or("Unknown"),
                parsed_tx.mint,
                parsed_tx.creator,
                estimated_price,
                liquidity_sol.unwrap_or(0.0),
                evaluation.reason
            )),
        });
    }
    
    info!("Successfully processed transaction {}", signature);
}

impl Default for BotSettings {
    fn default() -> Self {
        Self {
            solana_ws_urls: vec![DEFAULT_SOLANA_WS_URL.to_string()],
            solana_rpc_urls: vec![DEFAULT_SOLANA_RPC_URL.to_string()],
            pump_fun_program: DEFAULT_PUMP_FUN_PROGRAM.to_string(),
            metadata_program: DEFAULT_METADATA_PROGRAM.to_string(),
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
            enable_safer_sniping: false,
            shyft_api_key: None,
            shyft_graphql_url: default_shyft_graphql_url(),
        }
    }
}

fn default_dev_tip_percent() -> f64 { 2.0 }
fn default_dev_tip_fixed_sol() -> f64 { 0.0 }
fn default_enable_safer_sniping() -> bool { false }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_settings() {
        let settings = BotSettings::default();
        assert!(settings.is_valid(), "Default settings should be valid");
    }

    #[test]
    fn test_empty_ws_urls() {
        let mut settings = BotSettings::default();
        settings.solana_ws_urls = vec![];
        assert!(!settings.is_valid(), "Settings with empty WS URLs should be invalid");
    }

    #[test]
    fn test_empty_rpc_urls() {
        let mut settings = BotSettings::default();
        settings.solana_rpc_urls = vec![];
        assert!(!settings.is_valid(), "Settings with empty RPC URLs should be invalid");
    }

    #[test]
    fn test_empty_pump_fun_program() {
        let mut settings = BotSettings::default();
        settings.pump_fun_program = String::new();
        assert!(!settings.is_valid(), "Settings with empty pump_fun_program should be invalid");
    }

    #[test]
    fn test_empty_metadata_program() {
        let mut settings = BotSettings::default();
        settings.metadata_program = String::new();
        assert!(!settings.is_valid(), "Settings with empty metadata_program should be invalid");
    }

    #[test]
    fn test_null_byte_in_pump_fun_program() {
        let mut settings = BotSettings::default();
        settings.pump_fun_program = "test\0corrupted".to_string();
        assert!(!settings.is_valid(), "Settings with null byte in pump_fun_program should be invalid");
    }

    #[test]
    fn test_null_byte_in_metadata_program() {
        let mut settings = BotSettings::default();
        settings.metadata_program = "test\0corrupted".to_string();
        assert!(!settings.is_valid(), "Settings with null byte in metadata_program should be invalid");
    }

    #[test]
    fn test_null_byte_in_ws_url() {
        let mut settings = BotSettings::default();
        settings.solana_ws_urls = vec!["wss://test\0corrupted".to_string()];
        assert!(!settings.is_valid(), "Settings with null byte in WS URL should be invalid");
    }

    #[test]
    fn test_null_byte_in_rpc_url() {
        let mut settings = BotSettings::default();
        settings.solana_rpc_urls = vec!["https://test\0corrupted".to_string()];
        assert!(!settings.is_valid(), "Settings with null byte in RPC URL should be invalid");
    }
    
    #[test]
    fn test_valid_mode_dry_run() {
        let state = BotState {
            running: false,
            mode: "dry-run".to_string(),
            settings: BotSettings::default(),
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        assert!(state.is_mode_valid(), "Mode 'dry-run' should be valid");
    }
    
    #[test]
    fn test_valid_mode_real() {
        let state = BotState {
            running: false,
            mode: "real".to_string(),
            settings: BotSettings::default(),
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        assert!(state.is_mode_valid(), "Mode 'real' should be valid");
    }
    
    #[test]
    fn test_invalid_mode() {
        let state = BotState {
            running: false,
            mode: "invalid-mode".to_string(),
            settings: BotSettings::default(),
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        assert!(!state.is_mode_valid(), "Invalid mode should be detected");
    }
    
    #[test]
    fn test_corrupted_mode_repair() {
        let mut state = BotState {
            running: false,
            mode: "corrupted\0mode".to_string(),
            settings: BotSettings::default(),
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        assert!(!state.is_mode_valid(), "Corrupted mode should be invalid");
        state.repair_mode_if_needed();
        assert_eq!(state.mode, "dry-run", "Corrupted mode should be repaired to 'dry-run'");
        assert!(state.is_mode_valid(), "Repaired mode should be valid");
    }
    
    #[test]
    fn test_empty_mode_repair() {
        let mut state = BotState {
            running: false,
            mode: String::new(),
            settings: BotSettings::default(),
            holdings: Vec::new(),
            logs: Vec::new(),
            monitor: None,
            detected_tokens: Vec::new(),
        };
        assert!(!state.is_mode_valid(), "Empty mode should be invalid");
        state.repair_mode_if_needed();
        assert_eq!(state.mode, "dry-run", "Empty mode should be repaired to 'dry-run'");
        assert!(state.is_mode_valid(), "Repaired mode should be valid");
    }

    #[test]
    fn test_empty_string_in_ws_urls() {
        let mut settings = BotSettings::default();
        settings.solana_ws_urls = vec!["".to_string()];
        assert!(!settings.is_valid(), "Settings with empty string in WS URLs should be invalid");
    }

    #[test]
    fn test_empty_string_in_rpc_urls() {
        let mut settings = BotSettings::default();
        settings.solana_rpc_urls = vec!["".to_string()];
        assert!(!settings.is_valid(), "Settings with empty string in RPC URLs should be invalid");
    }

    #[test]
    fn test_valid_settings_with_multiple_urls() {
        let mut settings = BotSettings::default();
        settings.solana_ws_urls = vec![
            "wss://api.mainnet-beta.solana.com/".to_string(),
            "wss://api.secondary.solana.com/".to_string(),
        ];
        settings.solana_rpc_urls = vec![
            "https://api.mainnet-beta.solana.com/".to_string(),
            "https://api.secondary.solana.com/".to_string(),
        ];
        assert!(settings.is_valid(), "Settings with multiple valid URLs should be valid");
    }

    #[test]
    fn test_sanitize_empty_urls() {
        let mut settings = BotSettings::default();
        settings.solana_ws_urls = vec!["".to_string()];
        settings.solana_rpc_urls = vec!["".to_string()];
        
        assert!(!settings.is_valid(), "Settings with empty URLs should be invalid");
        
        let sanitized = settings.sanitize().expect("Sanitize should succeed");
        assert!(sanitized.is_valid(), "Sanitized settings should be valid");
        assert_eq!(sanitized.solana_ws_urls, vec![DEFAULT_SOLANA_WS_URL]);
        assert_eq!(sanitized.solana_rpc_urls, vec![DEFAULT_SOLANA_RPC_URL]);
    }

    #[test]
    fn test_sanitize_mixed_urls() {
        let mut settings = BotSettings::default();
        settings.solana_ws_urls = vec![
            "wss://valid.url.com".to_string(),
            "".to_string(),
            "wss://another.valid.url".to_string(),
        ];
        settings.solana_rpc_urls = vec![
            "".to_string(),
            "https://valid.rpc.com".to_string(),
        ];
        
        assert!(!settings.is_valid(), "Settings with mixed valid/invalid URLs should be invalid");
        
        let sanitized = settings.sanitize().expect("Sanitize should succeed");
        assert!(sanitized.is_valid(), "Sanitized settings should be valid");
        assert_eq!(sanitized.solana_ws_urls.len(), 2, "Should have 2 valid WS URLs");
        assert_eq!(sanitized.solana_rpc_urls.len(), 1, "Should have 1 valid RPC URL");
    }

    #[test]
    fn test_sanitize_null_bytes() {
        let mut settings = BotSettings::default();
        settings.solana_ws_urls = vec!["wss://test\0corrupted".to_string()];
        settings.pump_fun_program = "test\0corrupted".to_string();
        
        assert!(!settings.is_valid(), "Settings with null bytes should be invalid");
        
        let sanitized = settings.sanitize().expect("Sanitize should succeed");
        assert!(sanitized.is_valid(), "Sanitized settings should be valid");
        // Corrupted URLs get filtered out, so default is used
        assert_eq!(sanitized.solana_ws_urls, vec![DEFAULT_SOLANA_WS_URL]);
        // Corrupted program ID gets replaced with default
        assert_eq!(sanitized.pump_fun_program, DEFAULT_PUMP_FUN_PROGRAM);
    }

    #[test]
    fn test_sanitize_preserves_valid_settings() {
        let settings = BotSettings::default();
        
        assert!(settings.is_valid(), "Default settings should be valid");
        
        let sanitized = settings.sanitize().expect("Sanitize should succeed");
        assert!(sanitized.is_valid(), "Sanitized default settings should be valid");
        assert_eq!(sanitized.solana_ws_urls, settings.solana_ws_urls);
        assert_eq!(sanitized.solana_rpc_urls, settings.solana_rpc_urls);
        assert_eq!(sanitized.pump_fun_program, settings.pump_fun_program);
    }
}
