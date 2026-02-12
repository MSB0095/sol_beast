use crate::error::AppError;
use serde::{Deserialize, Serialize};
use base64::engine::general_purpose::STANDARD as Base64Engine;
use base64::Engine;
use std::env;

/// A single take-profit level: when profit reaches `trigger_percent`, sell `sell_percent`% of the original position.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct TpLevel {
    pub trigger_percent: f64,
    pub sell_percent: f64,
}

/// A single stop-loss level: when loss reaches `trigger_percent` (negative), sell `sell_percent`% of the original position.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct SlLevel {
    pub trigger_percent: f64,
    pub sell_percent: f64,
}

fn default_tp_levels() -> Vec<TpLevel> {
    vec![TpLevel { trigger_percent: 30.0, sell_percent: 100.0 }]
}

fn default_sl_levels() -> Vec<SlLevel> {
    vec![SlLevel { trigger_percent: -20.0, sell_percent: 100.0 }]
}

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
    /// Multi-level take-profit configuration (1-4 levels). Sum of sell_percent must be <= 100.
    #[serde(default = "default_tp_levels")]
    pub tp_levels: Vec<TpLevel>,
    /// Multi-level stop-loss configuration (1-4 levels). Sum of sell_percent must be <= 100.
    #[serde(default = "default_sl_levels")]
    pub sl_levels: Vec<SlLevel>,
    pub timeout_secs: i64,
    pub cache_capacity: usize,
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
    #[serde(default = "default_pumpportal_enabled")]
    pub pumpportal_enabled: bool,
    #[serde(default = "default_pumpportal_wss")]
    pub pumpportal_wss: Vec<String>,
    #[serde(default = "default_detected_coins_max")]
    pub detected_coins_max: usize,
    #[serde(default = "default_token_decimals")]
    pub default_token_decimals: u8,
    // Dev fee configuration
    #[serde(default = "default_dev_fee_enabled")]
    pub dev_fee_enabled: bool,
}

fn default_token_decimals() -> u8 { 6 }
fn default_dev_fee_enabled() -> bool { true }

impl Settings {
    pub fn from_file(path: &str) -> Result<Self, AppError> {
        let builder = config::Config::builder()
            .add_source(config::File::with_name(path));
        let cfg = builder.build()?;
        Ok(cfg.try_deserialize()?)
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), AppError> {
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
        if other.tp_levels != self.tp_levels {
            self.tp_levels = other.tp_levels.clone();
        }
        if other.sl_levels != self.sl_levels {
            self.sl_levels = other.sl_levels.clone();
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
        if other.pumpportal_enabled != self.pumpportal_enabled {
            self.pumpportal_enabled = other.pumpportal_enabled;
        }
        if other.pumpportal_wss != self.pumpportal_wss {
            self.pumpportal_wss = other.pumpportal_wss.clone();
        }
        if other.detected_coins_max != self.detected_coins_max {
            self.detected_coins_max = other.detected_coins_max;
        }
        if other.default_token_decimals != self.default_token_decimals {
            self.default_token_decimals = other.default_token_decimals;
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
        if other.dev_fee_enabled != self.dev_fee_enabled {
            self.dev_fee_enabled = other.dev_fee_enabled;
        }
    }

    /// Validate settings ranges and constraints
    pub fn validate(&self) -> Result<(), AppError> {
        // Validate TP levels
        if self.tp_levels.is_empty() {
            return Err(AppError::Validation("At least one TP level is required".to_string()));
        }
        if self.tp_levels.len() > 4 {
            return Err(AppError::Validation("Maximum 4 TP levels allowed".to_string()));
        }
        let mut tp_sell_sum = 0.0;
        for (i, level) in self.tp_levels.iter().enumerate() {
            if level.trigger_percent <= 0.0 {
                return Err(AppError::Validation(format!("TP level {} trigger_percent must be > 0", i + 1)));
            }
            if level.sell_percent <= 0.0 || level.sell_percent > 100.0 {
                return Err(AppError::Validation(format!("TP level {} sell_percent must be between 0 and 100", i + 1)));
            }
            tp_sell_sum += level.sell_percent;
        }
        if tp_sell_sum > 100.0 + f64::EPSILON {
            return Err(AppError::Validation(format!("TP levels sell_percent sum ({:.1}%) must be <= 100%", tp_sell_sum)));
        }

        // Validate SL levels
        if self.sl_levels.is_empty() {
            return Err(AppError::Validation("At least one SL level is required".to_string()));
        }
        if self.sl_levels.len() > 4 {
            return Err(AppError::Validation("Maximum 4 SL levels allowed".to_string()));
        }
        let mut sl_sell_sum = 0.0;
        for (i, level) in self.sl_levels.iter().enumerate() {
            if level.trigger_percent >= 0.0 {
                return Err(AppError::Validation(format!("SL level {} trigger_percent must be < 0", i + 1)));
            }
            if level.sell_percent <= 0.0 || level.sell_percent > 100.0 {
                return Err(AppError::Validation(format!("SL level {} sell_percent must be between 0 and 100", i + 1)));
            }
            sl_sell_sum += level.sell_percent;
        }
        if sl_sell_sum > 100.0 + f64::EPSILON {
            return Err(AppError::Validation(format!("SL levels sell_percent sum ({:.1}%) must be <= 100%", sl_sell_sum)));
        }

        if self.buy_amount <= 0.0 {
            return Err(AppError::Validation("buy_amount must be > 0".to_string()));
        }
        if self.timeout_secs <= 0 {
            return Err(AppError::Validation("timeout_secs must be > 0".to_string()));
        }
        if self.cache_capacity == 0 {
            return Err(AppError::Validation("cache_capacity must be > 0".to_string()));
        }
        if self.max_holded_coins == 0 {
            return Err(AppError::Validation("max_holded_coins must be > 0".to_string()));
        }
        if self.max_liquidity_sol < self.min_liquidity_sol {
            return Err(AppError::Validation("max_liquidity_sol must be >= min_liquidity_sol".to_string()));
        }
        Ok(())
    }
}

/// Try to read a base64-encoded keypair from the given env var. Returns
/// the raw decoded bytes if present and valid, otherwise None.
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
fn default_buy_amount() -> f64 { 0.1 }
fn default_price_source() -> String { "wss".to_string() }
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
fn default_pumpportal_enabled() -> bool { false }
fn default_pumpportal_wss() -> Vec<String> { vec!["wss://pumpportal.fun/api/data".to_string()] }

fn default_detected_coins_max() -> usize { 300 }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_example_config() {
        // This test validates that `Settings::from_file` can load the example
        // config without panicking and that a couple of fields match expected
        // placeholder values from `config.example.toml`.
        let s = Settings::from_file("config.example.toml").unwrap();
        assert_eq!(s.tp_levels.len(), 2);
        assert_eq!(s.tp_levels[0].trigger_percent, 30.0);
        assert_eq!(s.tp_levels[0].sell_percent, 50.0);
        assert_eq!(s.tp_levels[1].trigger_percent, 100.0);
        assert_eq!(s.tp_levels[1].sell_percent, 50.0);
        assert_eq!(s.sl_levels.len(), 1);
        assert_eq!(s.sl_levels[0].trigger_percent, -20.0);
        assert_eq!(s.cache_capacity, 1024);
    }
}