use wasm_bindgen::prelude::*;
use core::{
    WalletManager, TransactionBuilder, StrategyConfig, TradingStrategy,
    UserAccount, Holding, TradeRecord, models::UserSettings,
};

// Simple WASM RPC client
#[derive(Clone)]
pub struct WasmRpcClient {
    url: String,
}

impl WasmRpcClient {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    pub async fn get_account_info(&self, _pubkey: &str) -> Result<Option<Vec<u8>>, JsValue> {
        // TODO: Implement actual RPC call
        // For now, return None to simulate account not found
        Ok(None)
    }

    pub async fn get_recent_blockhash(&self) -> Result<String, JsValue> {
        // TODO: Implement actual RPC call
        // Return a dummy blockhash and touch the url to avoid dead field warning
        let _ = &self.url;
        Ok("11111111111111111111111111111111".to_string())
    }
}
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{window, WebSocket, MessageEvent, ErrorEvent, CloseEvent};
use serde::{Serialize, Deserialize};
use serde_json;
use solana_program::pubkey::Pubkey;
// spl_associated_token_account is not needed in WASM build
use std::str::FromStr;


// Constants for pump.fun program
#[allow(dead_code)]
const SYSTEM_PROGRAM_PUBKEY: &str = "11111111111111111111111111111111";
#[allow(dead_code)]
const TOKEN_PROGRAM_PUBKEY: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
#[allow(dead_code)]
const FEE_PROGRAM_PUBKEY: &str = "pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ";

use core::blockchain::tx_builder::{build_buy_instruction, build_sell_instruction};

// Note: Build instructions are implemented centrally in `core::tx_builder`.

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
/// Note: Uses Rc<RefCell<T>> for single-threaded WASM environment.
/// This provides interior mutability without locking.
#[wasm_bindgen]
pub struct SolBeastBot {
    wallet_manager: Rc<RefCell<WalletManager>>,
    transaction_builder: Rc<RefCell<Option<TransactionBuilder>>>,
    strategy: Rc<RefCell<Option<TradingStrategy>>>,
    holdings: Rc<RefCell<Vec<Holding>>>,
    trades: Rc<RefCell<Vec<TradeRecord>>>,
    user_account: Rc<RefCell<Option<UserAccount>>>,
    logs: Rc<RefCell<Vec<LogEntry>>>,
    stats: Rc<RefCell<BotStats>>,
    interval_id: Rc<RefCell<Option<i32>>>,
    ws: Rc<RefCell<Option<WebSocket>>>,
    ws_url: Rc<RefCell<String>>,
    rpc_client: Rc<RefCell<Option<WasmRpcClient>>>,
}

#[wasm_bindgen]
impl SolBeastBot {
    /// Create a new bot instance
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            wallet_manager: Rc::new(RefCell::new(WalletManager::new())),
            transaction_builder: Rc::new(RefCell::new(None)),
            strategy: Rc::new(RefCell::new(None)),
            holdings: Rc::new(RefCell::new(Vec::new())),
            trades: Rc::new(RefCell::new(Vec::new())),
            user_account: Rc::new(RefCell::new(None)),
            logs: Rc::new(RefCell::new(Vec::new())),
            stats: Rc::new(RefCell::new(BotStats::default())),
            interval_id: Rc::new(RefCell::new(None)),
            ws: Rc::new(RefCell::new(None)),
            ws_url: Rc::new(RefCell::new("wss://solana-mainnet.core.chainstack.com/d25bb135a7850b65b8f59f73fee3aba8".to_string())),
            rpc_client: Rc::new(RefCell::new(None)),
        }
    }

    /// Initialize the bot with pump.fun program address
    #[wasm_bindgen]
    pub fn initialize(&self, pump_program: String) -> Result<(), JsValue> {
        let builder = TransactionBuilder::new(pump_program)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        
        *self.transaction_builder.borrow_mut() = Some(builder);
        
        Ok(())
    }

    /// Connect a wallet
    #[wasm_bindgen]
    pub async fn connect_wallet(&self, address: String) -> Result<JsValue, JsValue> {
        let mut wallet_mgr = self.wallet_manager.borrow_mut();
        wallet_mgr.connect_wallet(address.clone())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        
        // Load or create user account
            match core::config::wallet::storage::load_user_account(&address)
            .map_err(|e| JsValue::from_str(&e.to_string()))? 
        {
            Some(account) => {
                log::info!("Loaded existing user account for {}", address);
                *self.user_account.borrow_mut() = Some(account.clone());
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
                
                core::config::wallet::storage::save_user_account(&account)
                    .map_err(|e| JsValue::from_str(&e.to_string()))?;
                
                *self.user_account.borrow_mut() = Some(account.clone());
                
                Ok(serde_wasm_bindgen::to_value(&account)?)
            }
        }
    }

    /// Disconnect wallet
    #[wasm_bindgen]
    pub fn disconnect_wallet(&self) {
        let mut wallet_mgr = self.wallet_manager.borrow_mut();
        wallet_mgr.disconnect_wallet();
        *self.user_account.borrow_mut() = None;
    }

    /// Check if wallet is connected
    #[wasm_bindgen]
    pub fn is_connected(&self) -> bool {
        let wallet_mgr = self.wallet_manager.borrow();
        wallet_mgr.is_connected()
    }

    /// Get current wallet address
    #[wasm_bindgen]
    pub fn get_wallet_address(&self) -> Option<String> {
        let wallet_mgr = self.wallet_manager.borrow();
        wallet_mgr.get_wallet_address().map(|s| s.to_string())
    }

    /// Sign and send a transaction using connected wallet
    #[wasm_bindgen]
    pub async fn sign_and_send_transaction(&self, _instructions: JsValue) -> Result<String, JsValue> {
        // TODO: Implement Phantom wallet integration
        // For now, return placeholder
        log::info!("Transaction signing not yet implemented");

        // Placeholder signature
        Ok("placeholder_signature".to_string())
    }

    /// Update user settings for trading strategy
    #[wasm_bindgen]
    pub fn update_trading_settings(&self, settings_json: JsValue) -> Result<(), JsValue> {
        let settings: UserSettings = serde_wasm_bindgen::from_value(settings_json)?;

        // Create strategy config from user settings
        let strategy_config = StrategyConfig {
            tp_percent: settings.tp_percent,
            sl_percent: settings.sl_percent,
            timeout_secs: settings.timeout_secs,
            enable_safer_sniping: settings.enable_safer_sniping,
            min_tokens_threshold: settings.min_tokens_threshold,
            max_sol_per_token: settings.max_sol_per_token,
        };

        // Update strategy
        *self.strategy.borrow_mut() = Some(TradingStrategy::new(strategy_config));

        // Update user account if it exists
        let mut user_account = self.user_account.borrow_mut();
        if let Some(account) = user_account.as_mut() {
            account.settings = settings;
            account.last_active = chrono::Utc::now();
        }

        log::info!("Trading settings updated successfully");
        Ok(())
    }

    /// Get current user settings
    #[wasm_bindgen]
    pub fn get_user_settings(&self) -> Result<JsValue, JsValue> {
        let user_account = self.user_account.borrow();
        if let Some(account) = user_account.as_ref() {
            Ok(serde_wasm_bindgen::to_value(&account.settings)?)
        } else {
            // Return default settings if no user account
            let default_settings = UserSettings {
                tp_percent: 2.0,
                sl_percent: 1.0,
                timeout_secs: 300,
                buy_amount: 0.1,
                max_held_coins: 5,
                enable_safer_sniping: true,
                min_tokens_threshold: 1000000,
                max_sol_per_token: 0.0001,
                slippage_bps: 500,
            };
            Ok(serde_wasm_bindgen::to_value(&default_settings)?)
        }
    }

    /// Get user account
    #[wasm_bindgen]
    pub fn get_user_account(&self) -> Result<JsValue, JsValue> {
        let user_account = self.user_account.borrow();
        match user_account.as_ref() {
            Some(account) => Ok(serde_wasm_bindgen::to_value(account)?),
            None => Err(JsValue::from_str("No user account loaded")),
        }
    }

    /// Get current holdings
    #[wasm_bindgen]
    pub fn get_holdings(&self) -> Result<JsValue, JsValue> {
        let holdings = self.holdings.borrow();
        Ok(serde_wasm_bindgen::to_value(&*holdings)?)
    }

    /// Get trade history
    #[wasm_bindgen]
    pub fn get_trades(&self) -> Result<JsValue, JsValue> {
        let trades = self.trades.borrow();
        Ok(serde_wasm_bindgen::to_value(&*trades)?)
    }

    /// Calculate bonding curve PDA for a mint
    #[wasm_bindgen]
    pub fn get_bonding_curve_pda(&self, mint: String) -> Result<String, JsValue> {
        let builder = self.transaction_builder.borrow();
        match builder.as_ref() {
            Some(b) => b.get_bonding_curve_pda(&mint)
                .map_err(|e| JsValue::from_str(&e.to_string())),
            None => Err(JsValue::from_str("Bot not initialized")),
        }
    }

    /// Set the WebSocket URL for Solana RPC
    #[wasm_bindgen]
    pub fn set_ws_url(&self, url: String) {
        *self.ws_url.borrow_mut() = url.clone();
        log::info!("WS URL set to: {}", url);
    }

    /// Connect to Solana WebSocket and subscribe to pump.fun logs
    #[wasm_bindgen]
    pub fn connect_and_monitor(&self) -> Result<(), JsValue> {
        let ws_url = self.ws_url.borrow().clone();
        log::info!("Connecting to WS: {}", ws_url);

        let ws = WebSocket::new(&ws_url)
            .map_err(|e| JsValue::from_str(&format!("Failed to create WS: {:?}", e)))?;

        // Clone references for closures
        let logs = self.logs.clone();
        let stats = self.stats.clone();
        let transaction_builder = self.transaction_builder.clone();
        let wallet_manager = self.wallet_manager.clone();
        let holdings = self.holdings.clone();
        let trades = self.trades.clone();
        let strategy = self.strategy.clone();

        // On open
        let ws_clone = ws.clone();
        let onopen_callback = Closure::wrap(Box::new(move || {
            log::info!("WS connected, subscribing to pump.fun logs");

            // Subscribe to program logs
            let subscribe_msg = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "logsSubscribe",
                "params": [
                    {"mentions": ["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"]},
                    {"commitment": "confirmed"}
                ]
            });

            if let Ok(msg) = serde_json::to_string(&subscribe_msg) {
                let _ = ws_clone.send_with_str(&msg);
            }
        }) as Box<dyn FnMut()>);

        // On message
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Some(data) = e.data().as_string() {
                if let Ok(notification) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(method) = notification.get("method") {
                        if method == "logsNotification" {
                            // Handle new log
                            handle_new_log(&logs, &stats, &transaction_builder, &wallet_manager, &holdings, &trades, &strategy, &notification);
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        // On error
        let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
            log::error!("WS error: {:?}", e);
        }) as Box<dyn FnMut(ErrorEvent)>);

        // On close
        let onclose_callback = Closure::wrap(Box::new(move |e: CloseEvent| {
            log::warn!("WS closed: {:?}", e);
        }) as Box<dyn FnMut(CloseEvent)>);

        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));

        // Forget closures to keep them alive
        onopen_callback.forget();
        onmessage_callback.forget();
        onerror_callback.forget();
        onclose_callback.forget();

        *self.ws.borrow_mut() = Some(ws);
        Ok(())
    }

    /// Attempt to buy a newly detected coin
    #[wasm_bindgen]
    pub async fn buy_coin(&self, mint_address: String) -> Result<(), JsValue> {
        log::info!("Attempting to buy coin: {}", mint_address);

        // Check if RPC client and transaction builder are initialized
        let rpc_client = self.rpc_client.borrow();
        let transaction_builder = self.transaction_builder.borrow();
        let strategy = self.strategy.borrow();

        if rpc_client.is_none() {
            return Err(JsValue::from_str("RPC client not initialized"));
        }
        if transaction_builder.is_none() {
            return Err(JsValue::from_str("Transaction builder not initialized"));
        }
        if strategy.is_none() {
            return Err(JsValue::from_str("Strategy not initialized"));
        }

        let rpc = rpc_client.as_ref().unwrap();
        let builder = transaction_builder.as_ref().unwrap();
        let strategy_impl = strategy.as_ref().unwrap();

        // Get bonding curve PDA
        let bonding_curve_pda = builder.get_bonding_curve_pda(&mint_address)
            .map_err(|e| JsValue::from_str(&format!("Failed to get bonding curve PDA: {:?}", e)))?;

        log::info!("Bonding curve PDA: {}", bonding_curve_pda);

        // Fetch bonding curve state
        let account_data = rpc.get_account_info(&bonding_curve_pda).await
            .map_err(|e| JsValue::from_str(&format!("Failed to fetch bonding curve data: {:?}", e)))?;

        if account_data.is_none() {
            return Err(JsValue::from_str("Bonding curve account not found"));
        }

        let curve_state = core::rpc_client::parse_bonding_curve(&account_data.unwrap())
            .map_err(|e| JsValue::from_str(&format!("Failed to parse bonding curve: {:?}", e)))?;

        // Calculate current price
        let current_price = curve_state.spot_price_sol_per_token()
            .ok_or_else(|| JsValue::from_str("Failed to calculate spot price"))?;

        log::info!("Current price for {}: {:.18} SOL/token", mint_address, current_price);

        // Check if we should buy based on strategy
        let should_buy = strategy_impl.should_buy(&curve_state, current_price)
            .map_err(|e| JsValue::from_str(&format!("Strategy evaluation failed: {:?}", e)))?;

        if !should_buy {
            log::info!("Strategy rejected buy for {} at price {:.18}", mint_address, current_price);
            return Ok(());
        }

        // TODO: Calculate buy amount based on settings, build transaction, sign and send
        log::info!("Buy conditions met for {}, proceeding with purchase", mint_address);

        // Get user settings for buy amount
        let user_settings = self.get_user_settings()?;
        let buy_amount_sol = if let Ok(settings) = serde_wasm_bindgen::from_value::<UserSettings>(user_settings) {
            settings.buy_amount
        } else {
            0.1 // default
        };

        // Calculate token amount to buy (simplified - in practice you'd calculate based on bonding curve)
        let token_amount = (buy_amount_sol / current_price) as u64 * 1_000_000_000; // Convert to lamports
        let max_sol_cost = (buy_amount_sol * 1_000_000_000.0) as u64; // Convert SOL to lamports

        log::info!("Buying {} tokens for max {} lamports", token_amount, max_sol_cost);

        // Get wallet address
        let wallet_manager = self.wallet_manager.borrow();
        let wallet_info = wallet_manager.get_current_wallet()
            .ok_or_else(|| JsValue::from_str("No wallet connected"))?;
        let user_pubkey = Pubkey::from_str(&wallet_info.address)
            .map_err(|e| JsValue::from_str(&format!("Invalid wallet address: {:?}", e)))?;

        // Build buy instruction
        let program_id = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
            .map_err(|e| JsValue::from_str(&format!("Invalid program ID: {:?}", e)))?;
        let fee_recipient = Pubkey::from_str("CebN5WGQ4jvEPvsVU4EoHEpgzq1yyGNy8bL3UVqqhHv")
            .map_err(|e| JsValue::from_str(&format!("Invalid fee recipient: {:?}", e)))?;

        let _buy_instruction = build_buy_instruction(
            &program_id,
            &mint_address,
            token_amount,
            max_sol_cost,
            Some(true), // track volume
            &user_pubkey,
            &fee_recipient,
            None, // creator pubkey (optional)
        ).map_err(|e| JsValue::from_str(&format!("Failed to build buy instruction: {:?}", e)))?;

        // TODO: Sign and send transaction using Phantom wallet
        // For now, just log the instruction
        log::info!("Buy instruction built successfully for mint: {}", mint_address);

        // Add to holdings (simulated)
        let holding = Holding {
            mint: mint_address.clone(),
            amount: token_amount,
            buy_price: current_price,
            buy_time: chrono::Utc::now(),
            metadata: None,
            onchain_raw: None,
            onchain: None,
        };
        self.holdings.borrow_mut().push(holding);

        // Update stats
        self.stats.borrow_mut().total_buys += 1;

        // Log trade
        let trade = TradeRecord {
            mint: mint_address.clone(),
            symbol: None,
            name: None,
            image: None,
            trade_type: "buy".to_string(),
            timestamp: chrono::Utc::now(),
            tx_signature: None, // TODO: Add actual signature
            amount_sol: buy_amount_sol,
            amount_tokens: token_amount as f64,
            price_per_token: current_price,
            profit_loss: None,
            profit_loss_percent: None,
            reason: Some("Strategy buy".to_string()),
        };
        self.trades.borrow_mut().push(trade);

        log::info!("Successfully simulated buy for {} tokens of {}", token_amount, mint_address);

        Ok(())
    }

    /// Start the bot with full monitoring and trading capabilities
    #[wasm_bindgen]
    pub fn start_bot(&self, rpc_url: String, ws_url: String) -> Result<(), JsValue> {
        log::info!("Starting SolBeast bot...");

        // Set URLs
        self.set_ws_url(ws_url.clone());

        // Initialize RPC client
        let rpc_client = WasmRpcClient::new(rpc_url);
        *self.rpc_client.borrow_mut() = Some(rpc_client);

        // Initialize transaction builder with pump.fun program ID
        let transaction_builder = TransactionBuilder::new("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string())
            .map_err(|e| JsValue::from_str(&format!("Failed to create transaction builder: {:?}", e)))?;
        *self.transaction_builder.borrow_mut() = Some(transaction_builder);

        // Initialize strategy
        let strategy = TradingStrategy::new(StrategyConfig::default());
        *self.strategy.borrow_mut() = Some(strategy);

        // Start WebSocket monitoring
        self.connect_and_monitor()?;

        // Start monitoring loop for holdings
        self.start_monitoring_loop()?;

        log::info!("Bot started successfully!");
        Ok(())
    }

    /// Monitor holdings and execute sells based on TP/SL conditions
    #[wasm_bindgen]
    pub async fn monitor_and_sell(&self) -> Result<(), JsValue> {
        log::info!("Monitoring holdings for sell conditions...");

        let holdings = self.holdings.borrow().clone();
        let strategy = self.strategy.borrow();
        let rpc_client = self.rpc_client.borrow();

        if strategy.is_none() {
            return Err(JsValue::from_str("Strategy not initialized"));
        }
        if rpc_client.is_none() {
            return Err(JsValue::from_str("RPC client not initialized"));
        }

        let strategy_config = strategy.as_ref().unwrap().config();
        let _rpc = rpc_client.as_ref().unwrap();

        for holding in holdings.iter() {
            // Check timeout
            let now = chrono::Utc::now().timestamp();
            let holding_age = now - holding.buy_time.timestamp();
            let timeout_secs = strategy_config.timeout_secs as i64;

            if holding_age > timeout_secs {
                log::info!("Timeout reached for {} (age: {}s, timeout: {}s)", holding.mint, holding_age, timeout_secs);
                self.sell_holding(&holding.mint, holding.amount).await?;
                continue;
            }

            // TODO: Check TP/SL conditions by fetching current price
            // This would require getting bonding curve state and calculating current price
            // Then comparing against buy_price to determine if TP/SL thresholds are met
        }

        Ok(())
    }

    /// Sell a specific holding
    async fn sell_holding(&self, mint: &str, amount: u64) -> Result<(), JsValue> {
        log::info!("Selling {} tokens of {}", amount, mint);

        // Check if RPC client and transaction builder are initialized
        let rpc_client = self.rpc_client.borrow();
        let transaction_builder = self.transaction_builder.borrow();

        if rpc_client.is_none() {
            return Err(JsValue::from_str("RPC client not initialized"));
        }
        if transaction_builder.is_none() {
            return Err(JsValue::from_str("Transaction builder not initialized"));
        }

        let rpc = rpc_client.as_ref().unwrap();
        let builder = transaction_builder.as_ref().unwrap();

        // Get bonding curve PDA
        let bonding_curve_pda = builder.get_bonding_curve_pda(&mint)
            .map_err(|e| JsValue::from_str(&format!("Failed to get bonding curve PDA: {:?}", e)))?;

        // Fetch bonding curve state
        let account_data = rpc.get_account_info(&bonding_curve_pda).await
            .map_err(|e| JsValue::from_str(&format!("Failed to fetch bonding curve data: {:?}", e)))?;

        if account_data.is_none() {
            return Err(JsValue::from_str("Bonding curve account not found"));
        }

        let curve_state = core::rpc_client::parse_bonding_curve(&account_data.unwrap())
            .map_err(|e| JsValue::from_str(&format!("Failed to parse bonding curve: {:?}", e)))?;

        // Calculate current price
        let current_price = curve_state.spot_price_sol_per_token()
            .ok_or_else(|| JsValue::from_str("Failed to calculate spot price"))?;

        log::info!("Current price for {}: {:.18} SOL/token", mint, current_price);

        // Calculate minimum SOL output (with some slippage tolerance)
        let expected_sol_output = (amount as f64) * current_price;
        let min_sol_output = (expected_sol_output * 0.95 * 1_000_000_000.0) as u64; // 5% slippage, convert to lamports

        log::info!("Selling {} tokens for min {} lamports", amount, min_sol_output);

        // Get wallet address
        let wallet_manager = self.wallet_manager.borrow();
        let wallet_info = wallet_manager.get_current_wallet()
            .ok_or_else(|| JsValue::from_str("No wallet connected"))?;
        let user_pubkey = Pubkey::from_str(&wallet_info.address)
            .map_err(|e| JsValue::from_str(&format!("Invalid wallet address: {:?}", e)))?;

        // Build sell instruction
        let program_id = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
            .map_err(|e| JsValue::from_str(&format!("Invalid program ID: {:?}", e)))?;
        let fee_recipient = Pubkey::from_str("CebN5WGQ4jvEPvsVU4EoHEpgzq1yyGNy8bL3UVqqhHv")
            .map_err(|e| JsValue::from_str(&format!("Invalid fee recipient: {:?}", e)))?;

        let _sell_instruction = build_sell_instruction(
            &program_id,
            mint,
            amount,
            min_sol_output,
            &user_pubkey,
            &fee_recipient,
            None, // creator pubkey (optional)
        ).map_err(|e| JsValue::from_str(&format!("Failed to build sell instruction: {:?}", e)))?;

        // TODO: Sign and send transaction using Phantom wallet
        // For now, just log the instruction
        log::info!("Sell instruction built successfully for mint: {}", mint);

        // Remove from holdings
        let mut holdings = self.holdings.borrow_mut();
        holdings.retain(|h| h.mint != mint);

        // Update stats
        self.stats.borrow_mut().total_sells += 1;

        // Log trade
        let trade = TradeRecord {
            mint: mint.to_string(),
            symbol: None,
            name: None,
            image: None,
            trade_type: "sell".to_string(),
            timestamp: chrono::Utc::now(),
            tx_signature: None, // TODO: Add actual signature
            amount_sol: expected_sol_output,
            amount_tokens: amount as f64,
            price_per_token: current_price,
            profit_loss: None, // TODO: Calculate actual P&L
            profit_loss_percent: None,
            reason: Some("Strategy sell".to_string()),
        };
        self.trades.borrow_mut().push(trade);

        log::info!("Successfully simulated sell for {} tokens of {}", amount, mint);

        Ok(())
    }

    /// Start the monitoring loop that checks holdings periodically
    #[wasm_bindgen]
    pub fn start_monitoring_loop(&self) -> Result<(), JsValue> {
        log::info!("Starting monitoring loop...");

        // Set up interval to check holdings every 30 seconds
        let interval_ms = 30_000.0;

        let closure = Closure::wrap(Box::new(move || {
            // In a real implementation, this would call monitor_and_sell
            // For now, we'll just log that monitoring is running
            log::info!("Monitoring loop tick - checking holdings...");
        }) as Box<dyn FnMut()>);

        let interval_id = web_sys::window()
            .unwrap()
            .set_interval_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                interval_ms as i32,
            )
            .map_err(|e| JsValue::from_str(&format!("Failed to set monitoring interval: {:?}", e)))?;

        *self.interval_id.borrow_mut() = Some(interval_id);
        closure.forget(); // Keep the closure alive

        log::info!("Monitoring loop started with {}ms interval", interval_ms);
        Ok(())
    }

    /// Stop the monitoring loop
    #[wasm_bindgen]
    pub fn stop_monitoring_loop(&self) -> Result<(), JsValue> {
        if let Some(interval_id) = *self.interval_id.borrow() {
            web_sys::window()
                .unwrap()
                .clear_interval_with_handle(interval_id);
            *self.interval_id.borrow_mut() = None;
            log::info!("Monitoring loop stopped");
        }
        Ok(())
    }

    /// Start a lightweight monitor that logs heartbeats and updates stats every interval_ms
    #[wasm_bindgen]
    pub fn start_monitoring(&self, interval_ms: u32) -> Result<(), JsValue> {
        log::info!("Starting monitoring with interval {}ms", interval_ms);
        // ensure we don't double-start
        if self.interval_id.borrow().is_some() {
            log::warn!("Monitor already running");
            return Err(JsValue::from_str("Monitor already running"));
        }

        let logs = self.logs.clone();
        let stats = self.stats.clone();

        let closure = Closure::wrap(Box::new(move || {
            log::info!("WASM bot heartbeat");
            // push a heartbeat log
            let mut l = logs.borrow_mut();
            let entry = LogEntry::new("info", "WASM bot heartbeat");
            l.insert(0, entry.clone());
            if l.len() > 200 { l.truncate(200); }

            // update stats
            let mut s = stats.borrow_mut();
            s.uptime_secs = s.uptime_secs.saturating_add(interval_ms as u64 / 1000);
            s.last_heartbeat = Some(entry.timestamp.clone());
        }) as Box<dyn FnMut()>);

        let win = window().ok_or(JsValue::from_str("No window context"))?;
        let id = win.set_interval_with_callback_and_timeout_and_arguments_0(closure.as_ref().unchecked_ref(), interval_ms as i32)
            .map_err(|e| JsValue::from(e))?;
        *self.interval_id.borrow_mut() = Some(id);
        closure.forget();
        Ok(())
    }

    /// Stop the monitor if running
    #[wasm_bindgen]
    pub fn stop_monitoring(&self) -> Result<(), JsValue> {
        let mut id_lock = self.interval_id.borrow_mut();
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
        log::info!("Getting logs");
        let logs = self.logs.borrow();
        Ok(serde_wasm_bindgen::to_value(&*logs).map_err(|e| JsValue::from_str(&e.to_string()))?)
    }

    /// Clear the stored logs
    #[wasm_bindgen]
    pub fn clear_logs(&self) -> Result<(), JsValue> {
        let mut logs = self.logs.borrow_mut();
        logs.clear();
        Ok(())
    }

    #[wasm_bindgen]
    pub fn get_stats(&self) -> Result<JsValue, JsValue> {
        let stats = self.stats.borrow();
        Ok(serde_wasm_bindgen::to_value(&*stats).map_err(|e| JsValue::from_str(&e.to_string()))?)
    }
}

/// Handle new log notification from WebSocket
fn handle_new_log(
    logs: &Rc<RefCell<Vec<LogEntry>>>,
    stats: &Rc<RefCell<BotStats>>,
    _transaction_builder: &Rc<RefCell<Option<TransactionBuilder>>>,
    _wallet_manager: &Rc<RefCell<WalletManager>>,
    _holdings: &Rc<RefCell<Vec<Holding>>>,
    _trades: &Rc<RefCell<Vec<TradeRecord>>>,
    _strategy: &Rc<RefCell<Option<TradingStrategy>>>,
    notification: &serde_json::Value,
) {
    if let Some(params) = notification.get("params") {
        if let Some(result) = params.get("result") {
            if let Some(value) = result.get("value") {
                if let Some(logs_array) = value.get("logs") {
                    if let Some(logs_vec) = logs_array.as_array() {
                        for log in logs_vec {
                            if let Some(log_str) = log.as_str() {
                                // Check if this is a pump.fun coin creation log
                                if log_str.contains("Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P") {
                                    log::info!("New pump.fun coin detected: {}", log_str);

                                    // Add to logs
                                    let mut logs_store = logs.borrow_mut();
                                    logs_store.push(LogEntry {
                                        timestamp: chrono::Utc::now().timestamp().to_string(),
                                        message: log_str.to_string(),
                                        level: "INFO".to_string(),
                                    });

                                    // Update stats
                                    let mut stats_store = stats.borrow_mut();
                                    stats_store.total_buys += 1;

                                    // Extract signature and attempt to parse mint address
                                    if let Some(signature) = value.get("signature").and_then(|s| s.as_str()) {
                                        log::info!("Transaction signature: {}", signature);
                                        // TODO: In a real implementation, we would:
                                        // 1. Make an RPC call to getTransaction
                                        // 2. Parse the transaction to extract the mint address
                                        // 3. Call buy_coin with the extracted mint
                                        // For now, we'll skip this step as it requires more complex parsing
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
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
