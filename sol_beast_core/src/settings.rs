use crate::error::CoreError;
use serde::{Deserialize, Serialize};

#[cfg(feature = "native")]
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Settings {
    pub solana_ws_urls: Vec<String>,
    pub solana_rpc_urls: Vec<String>,
    pub pump_fun_program: String,
    pub metadata_program: String,
    #[serde(default)]
    pub wallet_keypair_path: Option<String>,
    #[serde(default)]
    pub wallet_keypair_json: Option<String>,
    #[serde(default)]
    pub wallet_private_key_string: Option<String>,
    #[serde(default)]
    pub simulate_wallet_private_key_string: Option<String>,
    #[serde(default = "default_tp_percent")]
    pub tp_percent: f64,
    #[serde(default = "default_sl_percent")]
    pub sl_percent: f64,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: i64,
    #[serde(default = "default_cache_capacity")]
    pub cache_capacity: usize,
    #[serde(default = "default_price_cache_ttl_secs")]
    pub price_cache_ttl_secs: u64,
    #[serde(default = "default_buy_amount")]
    pub buy_amount: f64,
    #[serde(default = "default_price_source")]
    pub price_source: String,
    #[serde(default = "default_rotate_rpc")]
    pub rotate_rpc: bool,
    #[serde(default = "default_rpc_rotate_interval_secs")]
    pub rpc_rotate_interval_secs: u64,
    #[serde(default = "default_max_holded_coins")]
    pub max_holded_coins: usize,
    #[serde(default = "default_max_subs_per_wss")]
    pub max_subs_per_wss: usize,
    #[serde(default = "default_sub_ttl_secs")]
    pub sub_ttl_secs: u64,
    #[serde(default = "default_wss_subscribe_timeout_secs")]
    pub wss_subscribe_timeout_secs: u64,
    #[serde(default = "default_max_create_to_buy_secs")]
    pub max_create_to_buy_secs: u64,
    #[serde(default = "default_bonding_curve_strict")]
    pub bonding_curve_strict: bool,
    #[serde(default = "default_bonding_curve_log_debounce_secs")]
    pub bonding_curve_log_debounce_secs: u64,
    #[serde(default)]
    pub simulate_wallet_keypair_json: Option<String>,
    #[serde(default = "default_min_tokens_threshold")]
    pub min_tokens_threshold: u64,
    #[serde(default = "default_max_sol_per_token")]
    pub max_sol_per_token: f64,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u64,
    #[serde(default = "default_enable_safer_sniping")]
    pub enable_safer_sniping: bool,
    #[serde(default = "default_min_liquidity_sol")]
    pub min_liquidity_sol: f64,
    #[serde(default = "default_max_liquidity_sol")]
    pub max_liquidity_sol: f64,
    // Helius Sender configuration
    #[serde(default)]
    pub helius_sender_enabled: bool,
    #[serde(default)]
    pub helius_api_key: Option<String>,
    #[serde(default = "default_helius_sender_endpoint")]
    pub helius_sender_endpoint: String,
    #[serde(default = "default_helius_min_tip_sol")]
    pub helius_min_tip_sol: f64,
    #[serde(default = "default_helius_priority_fee_multiplier")]
    pub helius_priority_fee_multiplier: f64,
    #[serde(default)]
    pub helius_use_swqos_only: bool,
    #[serde(default = "default_helius_use_dynamic_tips")]
    pub helius_use_dynamic_tips: bool,
    #[serde(default = "default_helius_confirm_timeout_secs")]
    pub helius_confirm_timeout_secs: u64,
    // Dev fee configuration
    #[serde(default = "default_dev_fee_enabled")]
    pub dev_fee_enabled: bool,
    #[serde(default)]
    pub dev_wallet_address: Option<String>,
    // Dev tip configuration - optional tips in addition to/instead of dev fee
    #[serde(default = "default_dev_tip_percent")]
    pub dev_tip_percent: f64,
    #[serde(default = "default_dev_tip_fixed_sol")]
    pub dev_tip_fixed_sol: f64,
    // Shyft configuration
    #[serde(default)]
    pub shyft_api_key: Option<String>,
    #[serde(default = "default_shyft_graphql_url")]
    pub shyft_graphql_url: String,
}

impl Settings {
        /// Returns true if dev tips are enabled (percent or fixed > 0)
        pub fn has_dev_tips(&self) -> bool {
            self.dev_tip_percent > 0.0 || self.dev_tip_fixed_sol > 0.0
        }
    #[cfg(feature = "native")]
    pub fn from_file(path: &str) -> Result<Self, CoreError> {
        let builder = config::Config::builder()
            .add_source(config::File::with_name(path));
        let cfg = builder.build()?;
        Ok(cfg.try_deserialize()?)
    }

    #[cfg(feature = "native")]
    pub fn save_to_file(&self, path: &str) -> Result<(), CoreError> {
        let toml_string = toml::to_string(self)?;
        std::fs::write(path, toml_string)?;
        Ok(())
    }

    /// Merge another Settings struct, only updating fields that differ
    /// This is used for partial updates from API requests
    pub fn merge(&mut self, other: &Settings) {
        // Only update non-default values; use serde_json to detect changes
        if other.solana_rpc_urls != self.solana_rpc_urls {
            self.solana_rpc_urls = other.solana_rpc_urls.clone();
        }
        if other.solana_ws_urls != self.solana_ws_urls {
            self.solana_ws_urls = other.solana_ws_urls.clone();
        }
        if other.pump_fun_program != self.pump_fun_program {
            self.pump_fun_program = other.pump_fun_program.clone();
        }
        if other.metadata_program != self.metadata_program {
            self.metadata_program = other.metadata_program.clone();
        }
        if other.buy_amount != self.buy_amount {
            self.buy_amount = other.buy_amount;
        }
        if other.tp_percent != self.tp_percent {
            self.tp_percent = other.tp_percent;
        }
        if other.sl_percent != self.sl_percent {
            self.sl_percent = other.sl_percent;
        }
        if other.timeout_secs != self.timeout_secs {
            self.timeout_secs = other.timeout_secs;
        }
        if other.price_cache_ttl_secs != self.price_cache_ttl_secs {
            self.price_cache_ttl_secs = other.price_cache_ttl_secs;
        }
        if other.cache_capacity != self.cache_capacity {
            self.cache_capacity = other.cache_capacity;
        }
        if other.max_holded_coins != self.max_holded_coins {
            self.max_holded_coins = other.max_holded_coins;
        }
        if other.price_source != self.price_source {
            self.price_source = other.price_source.clone();
        }
        if other.rpc_rotate_interval_secs != self.rpc_rotate_interval_secs {
            self.rpc_rotate_interval_secs = other.rpc_rotate_interval_secs;
        }
        if other.helius_sender_enabled != self.helius_sender_enabled {
            self.helius_sender_enabled = other.helius_sender_enabled;
        }
        if other.helius_api_key != self.helius_api_key {
            self.helius_api_key = other.helius_api_key.clone();
        }
        if other.helius_sender_endpoint != self.helius_sender_endpoint {
            self.helius_sender_endpoint = other.helius_sender_endpoint.clone();
        }
        if other.helius_use_swqos_only != self.helius_use_swqos_only {
            self.helius_use_swqos_only = other.helius_use_swqos_only;
        }
        if other.helius_use_dynamic_tips != self.helius_use_dynamic_tips {
            self.helius_use_dynamic_tips = other.helius_use_dynamic_tips;
        }
        if other.helius_min_tip_sol != self.helius_min_tip_sol {
            self.helius_min_tip_sol = other.helius_min_tip_sol;
        }
        if other.helius_priority_fee_multiplier != self.helius_priority_fee_multiplier {
            self.helius_priority_fee_multiplier = other.helius_priority_fee_multiplier;
        }
        if other.enable_safer_sniping != self.enable_safer_sniping {
            self.enable_safer_sniping = other.enable_safer_sniping;
        }
        if other.min_tokens_threshold != self.min_tokens_threshold {
            self.min_tokens_threshold = other.min_tokens_threshold;
        }
        if other.max_sol_per_token != self.max_sol_per_token {
            self.max_sol_per_token = other.max_sol_per_token;
        }
        if other.min_liquidity_sol != self.min_liquidity_sol {
            self.min_liquidity_sol = other.min_liquidity_sol;
        }
        if other.max_liquidity_sol != self.max_liquidity_sol {
            self.max_liquidity_sol = other.max_liquidity_sol;
        }
        if other.bonding_curve_strict != self.bonding_curve_strict {
            self.bonding_curve_strict = other.bonding_curve_strict;
        }
        if other.bonding_curve_log_debounce_secs != self.bonding_curve_log_debounce_secs {
            self.bonding_curve_log_debounce_secs = other.bonding_curve_log_debounce_secs;
        }
        if other.slippage_bps != self.slippage_bps {
            self.slippage_bps = other.slippage_bps;
        }
        if other.wallet_keypair_path != self.wallet_keypair_path {
            self.wallet_keypair_path = other.wallet_keypair_path.clone();
        }
        if other.wallet_keypair_json != self.wallet_keypair_json {
            self.wallet_keypair_json = other.wallet_keypair_json.clone();
        }
        if other.wallet_private_key_string != self.wallet_private_key_string {
            self.wallet_private_key_string = other.wallet_private_key_string.clone();
        }
        if other.simulate_wallet_keypair_json != self.simulate_wallet_keypair_json {
            self.simulate_wallet_keypair_json = other.simulate_wallet_keypair_json.clone();
        }
        if other.simulate_wallet_private_key_string != self.simulate_wallet_private_key_string {
            self.simulate_wallet_private_key_string = other.simulate_wallet_private_key_string.clone();
        }
        if other.dev_tip_percent != self.dev_tip_percent {
            self.dev_tip_percent = other.dev_tip_percent;
        }
        if other.dev_tip_fixed_sol != self.dev_tip_fixed_sol {
            self.dev_tip_fixed_sol = other.dev_tip_fixed_sol;
        }
        if other.shyft_api_key != self.shyft_api_key {
            self.shyft_api_key = other.shyft_api_key.clone();
        }
        if other.shyft_graphql_url != self.shyft_graphql_url {
            self.shyft_graphql_url = other.shyft_graphql_url.clone();
        }
    }

    /// Validate settings ranges and constraints
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.tp_percent <= 0.0 {
            return Err(CoreError::Validation("tp_percent must be > 0".to_string()));
        }
        if self.sl_percent >= 0.0 {
            return Err(CoreError::Validation("sl_percent must be < 0".to_string()));
        }
        if self.buy_amount <= 0.0 {
            return Err(CoreError::Validation("buy_amount must be > 0".to_string()));
        }
        if self.timeout_secs <= 0 {
            return Err(CoreError::Validation("timeout_secs must be > 0".to_string()));
        }
        if self.cache_capacity == 0 {
            return Err(CoreError::Validation("cache_capacity must be > 0".to_string()));
        }
        if self.max_holded_coins == 0 {
            return Err(CoreError::Validation("max_holded_coins must be > 0".to_string()));
        }
        if self.max_liquidity_sol < self.min_liquidity_sol {
            return Err(CoreError::Validation("max_liquidity_sol must be >= min_liquidity_sol".to_string()));
        }
        if self.dev_tip_percent < 0.0 {
            return Err(CoreError::Validation("dev_tip_percent must be >= 0".to_string()));
        }
        if self.dev_tip_fixed_sol < 0.0 {
            return Err(CoreError::Validation("dev_tip_fixed_sol must be >= 0".to_string()));
        }
        Ok(())
    }
}


#[cfg(feature = "native")]
use std::env;

/// Try to read a base64-encoded keypair from the given env var. Returns
/// the raw decoded bytes if present and valid, otherwise None.
#[cfg(feature = "native")]
pub fn load_keypair_from_env_var(var: &str) -> Option<Vec<u8>> {
    if let Ok(s) = env::var(var) {
        match Base64Engine.decode(&s) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                eprintln!("Failed to decode {}: {}", var, e);
                None
            }
        }
    } else {
        None
    }
}

/// Parse a private key string in various formats:
/// - Base58 (standard Solana format, 88 chars)
/// - JSON array string like "[1,2,3,...]" 
/// - Comma-separated bytes like "1,2,3,..."
pub fn parse_private_key_string(s: &str) -> Result<Vec<u8>, String> {
    let trimmed = s.trim();
    
    // Try base58 first (most common format)
    if trimmed.len() >= 80 && !trimmed.starts_with('[') && !trimmed.contains(',') {
        return bs58::decode(trimmed)
            .into_vec()
            .map_err(|e| format!("Base58 decode failed: {}", e));
    }
    
    // Try JSON array format: [1,2,3,...]
    if trimmed.starts_with('[') {
        return serde_json::from_str::<Vec<u8>>(trimmed)
            .map_err(|e| format!("JSON parse failed: {}", e));
    }
    
    // Try comma-separated format: 1,2,3,...
    if trimmed.contains(',') {
        let parts: Result<Vec<u8>, _> = trimmed
            .split(',')
            .map(|s| s.trim().parse::<u8>())
            .collect();
        return parts.map_err(|e| format!("CSV parse failed: {}", e));
    }
    
    Err("Unrecognized private key format. Expected: base58, JSON array, or comma-separated bytes".to_string())
}

fn default_bonding_curve_strict() -> bool { false }
fn default_bonding_curve_log_debounce_secs() -> u64 { 300 }
fn default_tp_percent() -> f64 { 100.0 }
fn default_sl_percent() -> f64 { -50.0 }
fn default_timeout_secs() -> i64 { 50 }
fn default_cache_capacity() -> usize { 1024 }
fn default_buy_amount() -> f64 { 0.1 }
fn default_price_source() -> String { "wss".to_string() }
fn default_price_cache_ttl_secs() -> u64 { 60 }
fn default_rotate_rpc() -> bool { true }
fn default_rpc_rotate_interval_secs() -> u64 { 60 }
fn default_max_holded_coins() -> usize { 100 }
fn default_max_subs_per_wss() -> usize { 4 }
fn default_sub_ttl_secs() -> u64 { 900 }
fn default_wss_subscribe_timeout_secs() -> u64 { 6 }
fn default_max_create_to_buy_secs() -> u64 { 6 }
fn default_min_tokens_threshold() -> u64 { 1_000_000 }
fn default_max_sol_per_token() -> f64 { 0.0001 }
fn default_slippage_bps() -> u64 { 500 }
fn default_enable_safer_sniping() -> bool { false }
fn default_min_liquidity_sol() -> f64 { 0.0 }
fn default_max_liquidity_sol() -> f64 { 100.0 }
fn default_helius_sender_endpoint() -> String { "https://sender.helius-rpc.com/fast".to_string() }
fn default_helius_min_tip_sol() -> f64 { 0.001 }
fn default_helius_priority_fee_multiplier() -> f64 { 1.2 }
fn default_helius_use_dynamic_tips() -> bool { true }
fn default_helius_confirm_timeout_secs() -> u64 { 15 }
fn default_dev_fee_enabled() -> bool { true }
fn default_dev_tip_percent() -> f64 { 2.0 }
fn default_dev_tip_fixed_sol() -> f64 { 0.0 }
fn default_shyft_graphql_url() -> String { "https://programs.shyft.to/v0/graphql".to_string() }

impl Settings {
    /// Get the effective minimum tip amount based on routing mode
    /// - Default dual routing: uses configured helius_min_tip_sol (default 0.001 SOL)
    /// - SWQOS-only: uses minimum 0.000005 SOL unless helius_min_tip_sol is higher
    pub fn get_effective_min_tip_sol(&self) -> f64 {
        if self.helius_use_swqos_only {
            // SWQOS-only minimum is 0.000005 SOL, but respect user's higher setting
            self.helius_min_tip_sol.max(0.000005)
        } else {
            // Default dual routing minimum is 0.001 SOL
            self.helius_min_tip_sol.max(0.001)
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            solana_ws_urls: vec![],
            solana_rpc_urls: vec![],
            pump_fun_program: "".to_string(),
            metadata_program: "".to_string(),
            wallet_keypair_path: None,
            wallet_keypair_json: None,
            wallet_private_key_string: None,
            simulate_wallet_private_key_string: None,
            tp_percent: default_tp_percent(),
            sl_percent: default_sl_percent(),
            timeout_secs: default_timeout_secs(),
            cache_capacity: default_cache_capacity(),
            price_cache_ttl_secs: default_price_cache_ttl_secs(),
            buy_amount: default_buy_amount(),
            price_source: default_price_source(),
            rotate_rpc: default_rotate_rpc(),
            rpc_rotate_interval_secs: default_rpc_rotate_interval_secs(),
            max_holded_coins: default_max_holded_coins(),
            max_subs_per_wss: default_max_subs_per_wss(),
            sub_ttl_secs: default_sub_ttl_secs(),
            wss_subscribe_timeout_secs: default_wss_subscribe_timeout_secs(),
            max_create_to_buy_secs: default_max_create_to_buy_secs(),
            bonding_curve_strict: default_bonding_curve_strict(),
            bonding_curve_log_debounce_secs: default_bonding_curve_log_debounce_secs(),
            simulate_wallet_keypair_json: None,
            min_tokens_threshold: default_min_tokens_threshold(),
            max_sol_per_token: default_max_sol_per_token(),
            slippage_bps: default_slippage_bps(),
            enable_safer_sniping: default_enable_safer_sniping(),
            min_liquidity_sol: default_min_liquidity_sol(),
            max_liquidity_sol: default_max_liquidity_sol(),
            helius_sender_enabled: false,
            helius_api_key: None,
            helius_sender_endpoint: default_helius_sender_endpoint(),
            helius_min_tip_sol: default_helius_min_tip_sol(),
            helius_priority_fee_multiplier: default_helius_priority_fee_multiplier(),
            helius_use_swqos_only: false,
            helius_use_dynamic_tips: default_helius_use_dynamic_tips(),
            helius_confirm_timeout_secs: default_helius_confirm_timeout_secs(),
            dev_fee_enabled: default_dev_fee_enabled(),
            dev_wallet_address: None,
            dev_tip_percent: default_dev_tip_percent(),
            dev_tip_fixed_sol: default_dev_tip_fixed_sol(),
            shyft_api_key: None,
            shyft_graphql_url: default_shyft_graphql_url(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "native")]
    #[ignore] // Requires config.example.toml in working directory
    fn load_example_config() {
        // This test validates that `Settings::from_file` can load the example
        // config without panicking and that a couple of fields match expected
        // placeholder values from `config.example.toml`.
        let s = Settings::from_file("config.example.toml").unwrap();
        assert_eq!(s.tp_percent, 30.0);
        assert_eq!(s.sl_percent, -20.0);
        assert_eq!(s.cache_capacity, 1024);
    }
}