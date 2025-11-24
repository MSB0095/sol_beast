use wasm_bindgen::prelude::*;
use sol_beast_core::{
    WalletManager, TransactionBuilder, StrategyConfig, TradingStrategy,
    UserAccount, Holding, TradeRecord, models::UserSettings,
};
use std::sync::{Arc, Mutex};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::window;
use serde::{Serialize, Deserialize};

// Set panic hook for better error messages in console
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("Failed to initialize logger");
    log::info!("sol_beast WASM module initialized");
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

impl LogEntry {
    pub fn new(level: &str, message: &str) -> Self {
        Self { timestamp: chrono::Utc::now().to_rfc3339(), level: level.to_string(), message: message.to_string() }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct BotStats {
    pub total_buys: u64,
    pub total_sells: u64,
    pub total_profit: f64,
    pub uptime_secs: u64,
    pub last_heartbeat: Option<String>,
}

/// Main bot instance for browser environment
/// 
/// Note: Uses Arc<Mutex<T>> pattern for consistency with potential future multi-threaded
/// scenarios, even though WASM is currently single-threaded. This maintains API consistency
/// with the native version and prepares for future Web Workers support.
#[wasm_bindgen]
pub struct SolBeastBot {
    wallet_manager: Arc<Mutex<WalletManager>>,
    transaction_builder: Arc<Mutex<Option<TransactionBuilder>>>,
    strategy: Arc<Mutex<Option<TradingStrategy>>>,
    holdings: Arc<Mutex<Vec<Holding>>>,
    trades: Arc<Mutex<Vec<TradeRecord>>>,
    user_account: Arc<Mutex<Option<UserAccount>>>,
    logs: Arc<Mutex<Vec<LogEntry>>>,
    stats: Arc<Mutex<BotStats>>,
    interval_id: Arc<Mutex<Option<i32>>>,
}

#[wasm_bindgen]
impl SolBeastBot {
    /// Create a new bot instance
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            wallet_manager: Arc::new(Mutex::new(WalletManager::new())),
            transaction_builder: Arc::new(Mutex::new(None)),
            strategy: Arc::new(Mutex::new(None)),
            holdings: Arc::new(Mutex::new(Vec::new())),
            trades: Arc::new(Mutex::new(Vec::new())),
            user_account: Arc::new(Mutex::new(None)),
            logs: Arc::new(Mutex::new(Vec::new())),
            stats: Arc::new(Mutex::new(BotStats::default())),
            interval_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the bot with pump.fun program address
    #[wasm_bindgen]
    pub fn initialize(&self, pump_program: String) -> Result<(), JsValue> {
        let builder = TransactionBuilder::new(pump_program)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        
        *self.transaction_builder.lock()
            .expect("Failed to acquire transaction_builder lock") = Some(builder);
        
        Ok(())
    }

    /// Connect a wallet
    #[wasm_bindgen]
    pub async fn connect_wallet(&self, address: String) -> Result<JsValue, JsValue> {
        let mut wallet_mgr = self.wallet_manager.lock()
            .expect("Failed to acquire wallet_manager lock");
        wallet_mgr.connect_wallet(address.clone())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        
        // Load or create user account
        match sol_beast_core::wallet::storage::load_user_account(&address)
            .map_err(|e| JsValue::from_str(&e.to_string()))? 
        {
            Some(account) => {
                log::info!("Loaded existing user account for {}", address);
                *self.user_account.lock()
                    .expect("Failed to acquire user_account lock") = Some(account.clone());
                Ok(serde_wasm_bindgen::to_value(&account)?)
            }
            None => {
                log::info!("Creating new user account for {}", address);
                let account = UserAccount {
                    wallet_address: address.clone(),
                    created_at: chrono::Utc::now(),
                    last_active: chrono::Utc::now(),
                    total_trades: 0,
                    total_profit_loss: 0.0,
                    settings: UserSettings::default(),
                };
                
                sol_beast_core::wallet::storage::save_user_account(&account)
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;
                
                *self.user_account.lock()
                    .expect("Failed to acquire user_account lock") = Some(account.clone());
                
                Ok(serde_wasm_bindgen::to_value(&account)?)
            }
        }
    }

    /// Disconnect wallet
    #[wasm_bindgen]
    pub fn disconnect_wallet(&self) {
        let mut wallet_mgr = self.wallet_manager.lock()
            .expect("Failed to acquire wallet_manager lock");
        wallet_mgr.disconnect_wallet();
        *self.user_account.lock()
            .expect("Failed to acquire user_account lock") = None;
    }

    /// Check if wallet is connected
    #[wasm_bindgen]
    pub fn is_connected(&self) -> bool {
        let wallet_mgr = self.wallet_manager.lock()
            .expect("Failed to acquire wallet_manager lock");
        wallet_mgr.is_connected()
    }

    /// Get current wallet address
    #[wasm_bindgen]
    pub fn get_wallet_address(&self) -> Option<String> {
        let wallet_mgr = self.wallet_manager.lock()
            .expect("Failed to acquire wallet_manager lock");
        wallet_mgr.get_wallet_address().map(|s| s.to_string())
    }

    /// Update user settings
    #[wasm_bindgen]
    pub fn update_settings(&self, settings_json: JsValue) -> Result<(), JsValue> {
        let settings: UserSettings = serde_wasm_bindgen::from_value(settings_json)?;
        
        let mut user_account = self.user_account.lock()
            .expect("Failed to acquire user_account lock");
        if let Some(account) = user_account.as_mut() {
            account.settings = settings.clone();
            account.last_active = chrono::Utc::now();
            
            // Save to storage
            sol_beast_core::wallet::storage::save_user_account(account)
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
            
            // Update strategy
            let strategy_config = StrategyConfig {
                tp_percent: settings.tp_percent,
                sl_percent: settings.sl_percent,
                timeout_secs: settings.timeout_secs,
                enable_safer_sniping: settings.enable_safer_sniping,
                min_tokens_threshold: settings.min_tokens_threshold,
                max_sol_per_token: settings.max_sol_per_token,
            };
            
            *self.strategy.lock()
                .expect("Failed to acquire strategy lock") = Some(TradingStrategy::new(strategy_config));
        }
        
        Ok(())
    }

    /// Get user account
    #[wasm_bindgen]
    pub fn get_user_account(&self) -> Result<JsValue, JsValue> {
        let user_account = self.user_account.lock()
            .expect("Failed to acquire user_account lock");
        match user_account.as_ref() {
            Some(account) => Ok(serde_wasm_bindgen::to_value(account)?),
            None => Err(JsValue::from_str("No user account loaded")),
        }
    }

    /// Get current holdings
    #[wasm_bindgen]
    pub fn get_holdings(&self) -> Result<JsValue, JsValue> {
        let holdings = self.holdings.lock()
            .expect("Failed to acquire holdings lock");
        Ok(serde_wasm_bindgen::to_value(&*holdings)?)
    }

    /// Get trade history
    #[wasm_bindgen]
    pub fn get_trades(&self) -> Result<JsValue, JsValue> {
        let trades = self.trades.lock()
            .expect("Failed to acquire trades lock");
        Ok(serde_wasm_bindgen::to_value(&*trades)?)
    }

    /// Calculate bonding curve PDA for a mint
    #[wasm_bindgen]
    pub fn get_bonding_curve_pda(&self, mint: String) -> Result<String, JsValue> {
        let builder = self.transaction_builder.lock()
            .expect("Failed to acquire transaction_builder lock");
        match builder.as_ref() {
            Some(b) => b.get_bonding_curve_pda(&mint)
                .map_err(|e| JsValue::from_str(&e.to_string())),
            None => Err(JsValue::from_str("Bot not initialized")),
        }
    }

    /// Calculate expected token output for a SOL amount
    #[wasm_bindgen]
    pub fn calculate_token_output(
        &self,
        sol_amount: f64,
        virtual_sol_reserves: f64,
        virtual_token_reserves: f64,
    ) -> f64 {
        let builder = self.transaction_builder.lock()
            .expect("Failed to acquire transaction_builder lock");
        if let Some(b) = builder.as_ref() {
            let tokens = b.calculate_token_output(
                sol_amount,
                (virtual_sol_reserves * 1e9) as u64,
                (virtual_token_reserves * 1e6) as u64,
            );
            tokens as f64 / 1e6
        } else {
            0.0
        }
    }

    /// Start a lightweight monitor that logs heartbeats and updates stats every interval_ms
    #[wasm_bindgen]
    pub fn start_monitoring(&self, interval_ms: u32) -> Result<(), JsValue> {
        // ensure we don't double-start
        if self.interval_id.lock().unwrap().is_some() {
            return Err(JsValue::from_str("Monitor already running"));
        }

        let logs = self.logs.clone();
        let stats = self.stats.clone();

        let closure = Closure::wrap(Box::new(move || {
            // push a heartbeat log
            let mut l = logs.lock().unwrap();
            let entry = LogEntry::new("info", "WASM bot heartbeat");
            l.insert(0, entry.clone());
            if l.len() > 200 { l.truncate(200); }

            // update stats
            let mut s = stats.lock().unwrap();
            s.uptime_secs = s.uptime_secs.saturating_add(interval_ms as u64 / 1000);
            s.last_heartbeat = Some(entry.timestamp.clone());
        }) as Box<dyn FnMut()>);

        let win = window().ok_or(JsValue::from_str("No window context"))?;
        let id = win.set_interval_with_callback_and_timeout_and_arguments_0(closure.as_ref().unchecked_ref(), interval_ms as i32)
            .map_err(|e| JsValue::from(e))?;
        *self.interval_id.lock().unwrap() = Some(id);
        closure.forget();
        Ok(())
    }

    /// Stop the monitor if running
    #[wasm_bindgen]
    pub fn stop_monitoring(&self) -> Result<(), JsValue> {
        let mut id_lock = self.interval_id.lock().unwrap();
        if let Some(id) = *id_lock {
            let win = window().ok_or(JsValue::from_str("No window"))?;
            win.clear_interval_with_handle(id);
            *id_lock = None;
            return Ok(());
        }
        Err(JsValue::from_str("Monitor not running"))
    }

    #[wasm_bindgen]
    pub fn get_logs(&self) -> Result<JsValue, JsValue> {
        let logs = self.logs.lock().unwrap();
        Ok(serde_wasm_bindgen::to_value(&*logs).map_err(|e| JsValue::from_str(&e.to_string()))?)
    }

    /// Clear the stored logs
    #[wasm_bindgen]
    pub fn clear_logs(&self) -> Result<(), JsValue> {
        let mut logs = self.logs.lock().unwrap();
        logs.clear();
        Ok(())
    }

    #[wasm_bindgen]
    pub fn get_stats(&self) -> Result<JsValue, JsValue> {
        let stats = self.stats.lock().unwrap();
        Ok(serde_wasm_bindgen::to_value(&*stats).map_err(|e| JsValue::from_str(&e.to_string()))?)
    }
}

/// Helper functions for JS interop
#[wasm_bindgen]
pub fn log_to_console(message: String) {
    log::info!("{}", message);
}

#[wasm_bindgen]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
