use serde::Deserialize;
use base64::engine::general_purpose::STANDARD as Base64Engine;
use base64::Engine;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub solana_ws_urls: Vec<String>,
    pub solana_rpc_urls: Vec<String>,
    pub pump_fun_program: String,
    pub metadata_program: String,
    pub wallet_keypair_path: String,
    #[serde(default)]
    pub wallet_keypair_json: Option<String>,
    #[serde(default)]
    pub wallet_private_key_string: Option<String>,
    pub tp_percent: f64,
    pub sl_percent: f64,
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
    #[serde(default = "default_max_detect_to_buy_secs")]
    pub max_detect_to_buy_secs: u64,
    #[serde(default = "default_bonding_curve_strict")]
    pub bonding_curve_strict: bool,
    #[serde(default = "default_bonding_curve_log_debounce_secs")]
    pub bonding_curve_log_debounce_secs: u64,
    #[serde(default)]
    pub simulate_wallet_keypair_json: Option<String>,
}

impl Settings {
    pub fn from_file(path: &str) -> Self {
        let builder = config::Config::builder()
            .add_source(config::File::with_name(path));
        let cfg = builder.build().expect("Failed to build config");
        cfg.try_deserialize().expect("Failed to load config")
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
fn default_max_detect_to_buy_secs() -> u64 { 6 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_example_config() {
        // This test validates that `Settings::from_file` can load the example
        // config without panicking and that a couple of fields match expected
        // placeholder values from `config.example.toml`.
        let s = Settings::from_file("config.example.toml");
        assert_eq!(s.tp_percent, 30.0);
        assert_eq!(s.sl_percent, -20.0);
        assert_eq!(s.cache_capacity, 1024);
    }
}