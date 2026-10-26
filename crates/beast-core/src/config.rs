//! Layered configuration v2: embedded defaults < config.toml < BEAST_*
//! environment variables. Secrets live in a separate `secrets.toml` (0600) or
//! environment variables and are never serialized by the API.

use crate::economics::TipPolicy;
use crate::error::{BeastError, Result};
use crate::types::{Mode, StrategyKind};
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG_TOML: &str = include_str!("default_config.toml");

fn default_true() -> bool {
    true
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}

fn default_config_version() -> u32 {
    crate::CONFIG_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeastConfig {
    #[serde(default = "default_config_version")]
    pub config_version: u32,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub wallet: WalletConfig,
    #[serde(default)]
    pub rpc: RpcConfig,
    #[serde(default)]
    pub jito: JitoConfig,
    #[serde(default)]
    pub risk: RiskConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub guard: GuardConfig,
    #[serde(default)]
    pub safety: SafetyConfig,
    #[serde(default)]
    pub venues: VenueConfig,
    #[serde(default)]
    pub pools: Vec<PoolConfig>,
    #[serde(default)]
    pub rent: RentConfig,
    #[serde(default)]
    pub strategies: BTreeMap<String, StrategyConfig>,
}

impl Default for BeastConfig {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG_TOML).expect("embedded default config must be valid")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub real: bool,
    #[serde(default = "default_true")]
    pub require_cli_real: bool,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Radar,
            real: false,
            require_cli_real: true,
            data_dir: default_data_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletConfig {
    #[serde(default = "default_keypair_path")]
    pub keypair_path: Option<PathBuf>,
    #[serde(default)]
    pub sniper_keypair_path: Option<PathBuf>,
    #[serde(default)]
    pub cold_address: Option<String>,
    #[serde(default = "default_true")]
    pub require_0600: bool,
}

fn default_keypair_path() -> Option<PathBuf> {
    Some(PathBuf::from("./keypair.json"))
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            keypair_path: default_keypair_path(),
            sniper_keypair_path: None,
            cold_address: None,
            require_0600: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcConfig {
    #[serde(default = "default_rpc_urls")]
    pub urls: Vec<String>,
    #[serde(default = "default_ws_urls")]
    pub ws_urls: Vec<String>,
    #[serde(default = "default_commitment")]
    pub commitment: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_retries")]
    pub max_retries: u32,
    #[serde(default = "default_jupiter_lite")]
    pub jupiter_quote_url: String,
    #[serde(default = "default_jupiter_lite")]
    pub jupiter_swap_instructions_url: String,
}

fn default_rpc_urls() -> Vec<String> {
    vec!["https://api.mainnet-beta.solana.com".into()]
}
fn default_ws_urls() -> Vec<String> {
    vec!["wss://api.mainnet-beta.solana.com/".into()]
}
fn default_commitment() -> String {
    "confirmed".into()
}
fn default_timeout_ms() -> u64 {
    5_000
}
fn default_retries() -> u32 {
    2
}
fn default_jupiter_lite() -> String {
    "https://lite-api.jup.ag/swap/v1".into()
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            urls: default_rpc_urls(),
            ws_urls: default_ws_urls(),
            commitment: default_commitment(),
            timeout_ms: default_timeout_ms(),
            max_retries: default_retries(),
            jupiter_quote_url: default_jupiter_lite(),
            jupiter_swap_instructions_url: default_jupiter_lite(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JitoConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_jito_regions")]
    pub block_engine_urls: Vec<String>,
    #[serde(default = "default_tip_floor_url")]
    pub tip_floor_url: String,
    #[serde(default)]
    pub tip_accounts: Vec<String>,
    #[serde(default = "default_tip_percentile")]
    pub tip_percentile: u8,
    #[serde(default = "default_send_regions")]
    pub send_regions: usize,
    #[serde(default = "default_jito_rps")]
    pub requests_per_second: u32,
    #[serde(default = "default_leader_window")]
    pub leader_window: u8,
    #[serde(default = "default_true")]
    pub bundle_only: bool,
}

fn default_jito_regions() -> Vec<String> {
    vec![
        "https://amsterdam.mainnet.block-engine.jito.wtf".into(),
        "https://frankfurt.mainnet.block-engine.jito.wtf".into(),
        "https://london.mainnet.block-engine.jito.wtf".into(),
        "https://ny.mainnet.block-engine.jito.wtf".into(),
        "https://slc.mainnet.block-engine.jito.wtf".into(),
        "https://singapore.mainnet.block-engine.jito.wtf".into(),
        "https://tokyo.mainnet.block-engine.jito.wtf".into(),
    ]
}
fn default_tip_floor_url() -> String {
    "https://bundles.jito.wtf/api/v1/bundles/tip_floor".into()
}
fn default_tip_percentile() -> u8 {
    75
}
fn default_send_regions() -> usize {
    2
}
fn default_jito_rps() -> u32 {
    1
}
fn default_leader_window() -> u8 {
    2
}

impl Default for JitoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            block_engine_urls: default_jito_regions(),
            tip_floor_url: default_tip_floor_url(),
            tip_accounts: vec![],
            tip_percentile: default_tip_percentile(),
            send_regions: default_send_regions(),
            requests_per_second: default_jito_rps(),
            leader_window: default_leader_window(),
            bundle_only: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskConfig {
    #[serde(default = "default_balance_floor")]
    pub balance_floor_lamports: u64,
    #[serde(default = "default_max_inflight")]
    pub max_inflight_bundles: usize,
    #[serde(default = "default_max_inflight_strategy")]
    pub max_inflight_per_strategy: usize,
    #[serde(default = "default_max_attempts")]
    pub max_attempts_per_min: u32,
    #[serde(default = "default_daily_cost_cap")]
    pub daily_landed_cost_cap_lamports: u64,
    #[serde(default = "default_model_error")]
    pub max_model_error_bps: i64,
    #[serde(default = "default_feed_lag")]
    pub max_feed_lag_ms: u64,
    #[serde(default = "default_slot_lag")]
    pub max_slot_lag: u64,
    #[serde(default = "default_clock_skew")]
    pub max_clock_skew_ms: u64,
    #[serde(default = "default_consecutive_failures")]
    pub max_consecutive_failures: u32,
    #[serde(default = "default_unexpected_delta")]
    pub unexpected_balance_delta_lamports: u64,
    #[serde(default = "default_kill_switch")]
    pub kill_switch_file: PathBuf,
}

fn default_balance_floor() -> u64 {
    50_000_000
}
fn default_max_inflight() -> usize {
    6
}
fn default_max_inflight_strategy() -> usize {
    3
}
fn default_max_attempts() -> u32 {
    60
}
fn default_daily_cost_cap() -> u64 {
    100_000_000
}
fn default_model_error() -> i64 {
    50
}
fn default_feed_lag() -> u64 {
    2_000
}
fn default_slot_lag() -> u64 {
    3
}
fn default_clock_skew() -> u64 {
    250
}
fn default_consecutive_failures() -> u32 {
    25
}
fn default_unexpected_delta() -> u64 {
    1_000
}
fn default_kill_switch() -> PathBuf {
    PathBuf::from("./STOP")
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            balance_floor_lamports: default_balance_floor(),
            max_inflight_bundles: default_max_inflight(),
            max_inflight_per_strategy: default_max_inflight_strategy(),
            max_attempts_per_min: default_max_attempts(),
            daily_landed_cost_cap_lamports: default_daily_cost_cap(),
            max_model_error_bps: default_model_error(),
            max_feed_lag_ms: default_feed_lag(),
            max_slot_lag: default_slot_lag(),
            max_clock_skew_ms: default_clock_skew(),
            max_consecutive_failures: default_consecutive_failures(),
            unexpected_balance_delta_lamports: default_unexpected_delta(),
            kill_switch_file: default_kill_switch(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_api_bind")]
    pub bind: String,
    #[serde(default)]
    pub allow_remote: bool,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_api_rps")]
    pub rate_limit_per_sec: u32,
    #[serde(default = "default_api_read_rps")]
    pub read_rate_limit_per_sec: u32,
    #[serde(default = "default_body_limit")]
    pub body_limit_bytes: usize,
    /// Optional directory containing a built frontend. When set, the API serves
    /// it with a single-page-app fallback. The path must contain `index.html`.
    #[serde(default)]
    pub static_dir: Option<PathBuf>,
}

fn default_api_bind() -> String {
    "127.0.0.1:8081".into()
}
fn default_api_rps() -> u32 {
    20
}
fn default_api_read_rps() -> u32 {
    60
}
fn default_body_limit() -> usize {
    65_536
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: default_api_bind(),
            allow_remote: false,
            allowed_origins: vec![],
            rate_limit_per_sec: default_api_rps(),
            read_rate_limit_per_sec: default_api_read_rps(),
            body_limit_bytes: default_body_limit(),
            static_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    #[serde(default = "default_storage_path")]
    pub path: PathBuf,
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    #[serde(default = "default_feed_retention")]
    pub feed_retention_days: u32,
}

fn default_storage_path() -> PathBuf {
    PathBuf::from("./data/beast.sqlite3")
}
fn default_retention() -> u32 {
    30
}
fn default_feed_retention() -> u32 {
    7
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: default_storage_path(),
            retention_days: default_retention(),
            feed_retention_days: default_feed_retention(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub json: bool,
}

fn default_log_level() -> String {
    "info".into()
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            json: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GuardMode {
    #[default]
    Lite,
    Program,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub mode: GuardMode,
    #[serde(default)]
    pub program_id: String,
    #[serde(default = "default_guard_min_profit")]
    pub min_profit_lamports: u64,
}

fn default_guard_min_profit() -> u64 {
    10_000
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: GuardMode::Lite,
            program_id: String::new(),
            min_profit_lamports: default_guard_min_profit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SafetyConfig {
    /// Allow live execution without a deployed on-chain guard program.
    /// If true, the bundle has NO on-chain revert guarantee. Only off-chain checks apply.
    pub allow_unguarded_live: bool,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            allow_unguarded_live: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VenueConfig {
    #[serde(default)]
    pub disabled: Vec<String>,
    #[serde(default)]
    pub quote_only: Vec<String>,
    #[serde(default = "default_jupiter_quote")]
    pub jupiter_quote_url: String,
    #[serde(default = "default_jupiter_swap")]
    pub jupiter_swap_instructions_url: String,
    #[serde(default = "default_jupiter_accounts")]
    pub max_accounts: u8,
    #[serde(default = "default_true")]
    pub restrict_intermediate_tokens: bool,
}

fn default_jupiter_quote() -> String {
    "https://lite-api.jup.ag/swap/v1/quote".into()
}
fn default_jupiter_swap() -> String {
    "https://lite-api.jup.ag/swap/v1/swap-instructions".into()
}
fn default_jupiter_accounts() -> u8 {
    48
}

impl Default for VenueConfig {
    fn default() -> Self {
        Self {
            disabled: vec![],
            quote_only: vec![],
            jupiter_quote_url: default_jupiter_quote(),
            jupiter_swap_instructions_url: default_jupiter_swap(),
            max_accounts: default_jupiter_accounts(),
            restrict_intermediate_tokens: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolConfig {
    pub address: String,
    pub venue: String,
    pub mint_a: String,
    pub mint_b: String,
    #[serde(default)]
    pub vault_a: Option<String>,
    #[serde(default)]
    pub vault_b: Option<String>,
    #[serde(default)]
    pub fee_bps: Option<u64>,
    #[serde(default)]
    pub decimals_a: Option<u8>,
    #[serde(default)]
    pub decimals_b: Option<u8>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RentConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_rent_batch")]
    pub batch_size: usize,
    #[serde(default = "default_true")]
    pub include_token_2022: bool,
    #[serde(default)]
    pub dust_burn: bool,
    #[serde(default = "default_dust_threshold")]
    pub dust_threshold_usd: f64,
    #[serde(default = "default_min_recovered")]
    pub min_recovered_lamports: u64,
}

fn default_rent_batch() -> usize {
    20
}
fn default_dust_threshold() -> f64 {
    1.0
}
fn default_min_recovered() -> u64 {
    10_000
}

impl Default for RentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            batch_size: default_rent_batch(),
            include_token_2022: true,
            dust_burn: false,
            dust_threshold_usd: default_dust_threshold(),
            min_recovered_lamports: default_min_recovered(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default = "default_min_net")]
    pub min_net_lamports: u64,
    #[serde(default)]
    pub max_notional_lamports: u64,
    #[serde(default = "default_price_impact")]
    pub max_price_impact_bps: u64,
    #[serde(default = "default_tip_alpha")]
    pub tip_alpha_initial: f64,
    #[serde(default = "default_tip_cap")]
    pub tip_cap_lamports: u64,
    #[serde(default = "default_strategy_attempts")]
    pub max_attempts_per_min: u32,
    #[serde(default = "default_strategy_daily_cap")]
    pub daily_cost_cap_lamports: u64,
    #[serde(default = "default_strategy_inflight")]
    pub max_inflight: usize,
    #[serde(default = "default_ttl")]
    pub ttl_slots: u8,
    #[serde(default)]
    pub start_mints: Vec<String>,
    #[serde(default)]
    pub venues: Vec<String>,
    #[serde(default)]
    pub pair_allowlist: Vec<String>,
    #[serde(default)]
    pub pair_denylist: Vec<String>,
}

fn default_min_net() -> u64 {
    20_000
}
fn default_price_impact() -> u64 {
    80
}
fn default_tip_alpha() -> f64 {
    0.5
}
fn default_tip_cap() -> u64 {
    2_000_000
}
fn default_strategy_attempts() -> u32 {
    30
}
fn default_strategy_daily_cap() -> u64 {
    50_000_000
}
fn default_strategy_inflight() -> usize {
    2
}
fn default_ttl() -> u8 {
    2
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Off,
            min_net_lamports: default_min_net(),
            max_notional_lamports: 0,
            max_price_impact_bps: default_price_impact(),
            tip_alpha_initial: default_tip_alpha(),
            tip_cap_lamports: default_tip_cap(),
            max_attempts_per_min: default_strategy_attempts(),
            daily_cost_cap_lamports: default_strategy_daily_cap(),
            max_inflight: default_strategy_inflight(),
            ttl_slots: default_ttl(),
            start_mints: vec![],
            venues: vec![],
            pair_allowlist: vec![],
            pair_denylist: vec![],
        }
    }
}

impl StrategyConfig {
    pub fn tip_policy(&self, floor_lamports: u64) -> TipPolicy {
        TipPolicy {
            alpha: self.tip_alpha_initial,
            floor_lamports,
            cap_lamports: self.tip_cap_lamports.max(floor_lamports),
        }
    }
}

/// Secrets are intentionally a separate type and never appear inside
/// `BeastConfig`, so `GET /api/config` cannot leak them.
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Secrets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_bearer_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_read_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallet_keypair_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jupiter_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helius_api_key: Option<String>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

impl fmt::Debug for Secrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secrets")
            .field(
                "api_bearer_token",
                &self.api_bearer_token.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "api_read_token",
                &self.api_read_token.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "wallet_keypair_json",
                &self.wallet_keypair_json.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "jupiter_api_key",
                &self.jupiter_api_key.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "helius_api_key",
                &self.helius_api_key.as_ref().map(|_| "<redacted>"),
            )
            .field("path", &self.path)
            .finish()
    }
}

impl Secrets {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                path: Some(path.to_path_buf()),
                ..Self::default()
            });
        }
        let text = fs::read_to_string(path).map_err(|e| {
            BeastError::Config(format!("cannot read secrets file {}: {e}", path.display()))
        })?;
        let mut secrets: Secrets = toml::from_str(&text).map_err(|e| {
            BeastError::Config(format!("invalid secrets file {}: {e}", path.display()))
        })?;
        secrets.path = Some(path.to_path_buf());
        Ok(secrets)
    }

    fn apply_env(&mut self, env: &HashMap<String, String>) {
        if let Some(v) = env.get("BEAST_API_TOKEN") {
            self.api_bearer_token = Some(v.clone());
        }
        if let Some(v) = env.get("BEAST_API_READ_TOKEN") {
            self.api_read_token = Some(v.clone());
        }
        if let Some(v) = env.get("BEAST_WALLET_KEYPAIR_JSON") {
            self.wallet_keypair_json = Some(v.clone());
        }
        if let Some(v) = env.get("BEAST_JUPITER_API_KEY") {
            self.jupiter_api_key = Some(v.clone());
        }
        if let Some(v) = env.get("BEAST_HELIUS_API_KEY") {
            self.helius_api_key = Some(v.clone());
        }
    }

    pub fn validate(&self) -> Result<()> {
        for (name, token) in [
            ("api_bearer_token", self.api_bearer_token.as_deref()),
            ("api_read_token", self.api_read_token.as_deref()),
        ] {
            if let Some(token) = token {
                if token.len() < 24 {
                    return Err(BeastError::Config(format!(
                        "{name} must be at least 24 characters"
                    )));
                }
                if token.chars().any(char::is_whitespace) {
                    return Err(BeastError::Config(format!(
                        "{name} must not contain whitespace"
                    )));
                }
            }
        }
        if let (Some(admin), Some(read)) = (&self.api_bearer_token, &self.api_read_token) {
            if admin == read {
                return Err(BeastError::Config(
                    "admin and read-only API tokens must differ".into(),
                ));
            }
        }
        Ok(())
    }

    /// Create missing API tokens on first run. The file is written with mode
    /// 0600 on Unix. Returns true when a file was written.
    fn ensure_tokens(&mut self) -> Result<bool> {
        if self.api_bearer_token.is_some() && self.api_read_token.is_some() {
            return Ok(false);
        }
        self.api_bearer_token
            .get_or_insert_with(|| random_token("beast_admin_"));
        self.api_read_token
            .get_or_insert_with(|| random_token("beast_read_"));
        let path = self
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from("./secrets.toml"));
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    BeastError::Config(format!(
                        "cannot create secrets directory {}: {e}",
                        parent.display()
                    ))
                })?;
            }
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| BeastError::Config(format!("cannot serialize secrets: {e}")))?;
        write_private_file(&path, text.as_bytes())?;
        self.path = Some(path);
        Ok(true)
    }
}

fn random_token(prefix: &str) -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    format!(
        "{prefix}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    )
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| BeastError::Config(format!("cannot write {}: {e}", path.display())))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: BeastConfig,
    pub secrets: Secrets,
    pub config_path: Option<PathBuf>,
    pub warnings: Vec<String>,
    pub generated_secrets: bool,
}

impl LoadedConfig {
    pub fn data_dir(&self) -> PathBuf {
        self.config.runtime.data_dir.clone()
    }

    pub fn secrets_path(&self) -> Option<PathBuf> {
        self.secrets.path.clone()
    }
}

pub fn load(config_path: Option<&Path>, cli_real: bool) -> Result<LoadedConfig> {
    let env: HashMap<String, String> = std::env::vars().collect();
    load_with_env(config_path, cli_real, &env)
}

/// Deterministic entry point used by tests and the CLI.
pub fn load_with_env(
    config_path: Option<&Path>,
    cli_real: bool,
    env: &HashMap<String, String>,
) -> Result<LoadedConfig> {
    let mut warnings = Vec::new();
    let resolved_config = config_path
        .map(Path::to_path_buf)
        .or_else(|| env.get("BEAST_CONFIG").map(PathBuf::from))
        .or_else(|| {
            let default = PathBuf::from("./config.toml");
            default.exists().then_some(default)
        });

    let default_value: toml::Value =
        toml::from_str(DEFAULT_CONFIG_TOML).expect("embedded default config must parse");

    let mut root = default_value;
    if let Some(path) = &resolved_config {
        if path.exists() {
            let text = fs::read_to_string(path)
                .map_err(|e| BeastError::Config(format!("cannot read {}: {e}", path.display())))?;
            let file_value: toml::Value = toml::from_str(&text).map_err(|e| {
                BeastError::Config(format!("invalid TOML in {}: {e}", path.display()))
            })?;
            let file_value = migrate_if_needed(file_value, &mut warnings)?;
            merge_toml(&mut root, file_value);
        } else if config_path.is_some() {
            return Err(BeastError::Config(format!(
                "config file {} does not exist",
                path.display()
            )));
        }
    }

    apply_env(&mut root, env, &mut warnings)?;

    let config: BeastConfig = root.try_into().map_err(|e| {
        BeastError::Config(format!(
            "configuration does not match schema v{}: {e}",
            crate::CONFIG_VERSION
        ))
    })?;

    let secrets_path = env
        .get("BEAST_SECRETS")
        .map(PathBuf::from)
        .or_else(|| {
            resolved_config
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.join("secrets.toml"))
        })
        .unwrap_or_else(|| PathBuf::from("./secrets.toml"));

    let mut secrets = Secrets::load(&secrets_path)?;
    secrets.apply_env(env);
    let generated_secrets = if config.api.enabled {
        secrets.ensure_tokens()?
    } else {
        false
    };
    secrets.validate()?;

    if config.runtime.mode.is_live() && !config.runtime.real {
        return Err(BeastError::Config(
            "runtime.mode is canary/live but runtime.real is false; refusing to start".into(),
        ));
    }
    if config.runtime.mode.is_live() && !cli_real {
        return Err(BeastError::Config(
            "runtime.mode is canary/live but --real was not passed; refusing to start".into(),
        ));
    }
    if config.runtime.mode == Mode::Live && !config.runtime.require_cli_real {
        return Err(BeastError::Config(
            "runtime.require_cli_real=false is forbidden for live mode".into(),
        ));
    }
    config.validate_with_secrets(&secrets)?;

    if let Some(secrets_path) = &secrets.path {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(secrets_path) {
                let mode = meta.permissions().mode() & 0o777;
                if mode & 0o077 != 0 {
                    let msg = format!(
                        "{} has permissions {:o}; expected 0600",
                        secrets_path.display(),
                        mode
                    );
                    if config.runtime.real {
                        return Err(BeastError::Config(msg));
                    }
                    warnings.push(msg);
                }
            }
        }
    }

    if config.runtime.mode.is_live() && !cli_real {
        warnings.push("live mode requested without --real; demoted to radar".into());
    }

    Ok(LoadedConfig {
        config,
        secrets,
        config_path: resolved_config,
        warnings,
        generated_secrets,
    })
}

fn migrate_if_needed(mut value: toml::Value, warnings: &mut Vec<String>) -> Result<toml::Value> {
    let version = value
        .get("config_version")
        .and_then(toml::Value::as_integer)
        .unwrap_or(1);
    if version == crate::CONFIG_VERSION as i64 {
        return Ok(value);
    }
    if version > crate::CONFIG_VERSION as i64 {
        return Err(BeastError::Config(format!(
            "config_version {version} is newer than supported version {}",
            crate::CONFIG_VERSION
        )));
    }

    // Legacy v0/v1 files are sniper configs. Migrate the fields that overlap
    // with 2.0 and explicitly ignore the rest; never silently import a key.
    let mut migrated: toml::Value =
        toml::from_str(DEFAULT_CONFIG_TOML).expect("embedded default config");
    let mut ignored = Vec::new();
    if let Some(table) = value.as_table_mut() {
        if let Some(urls) = table.remove("solana_rpc_urls") {
            if let Some(arr) = urls.as_array() {
                let urls: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                if !urls.is_empty() {
                    set_nested(
                        &mut migrated,
                        &["rpc", "urls"],
                        toml::Value::Array(urls.into_iter().map(toml::Value::String).collect()),
                    );
                }
            }
        }
        if let Some(urls) = table.remove("solana_ws_urls") {
            if let Some(arr) = urls.as_array() {
                let urls: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                if !urls.is_empty() {
                    set_nested(
                        &mut migrated,
                        &["rpc", "ws_urls"],
                        toml::Value::Array(urls.into_iter().map(toml::Value::String).collect()),
                    );
                }
            }
        }
        if let Some(path) = table.remove("wallet_keypair_path") {
            if let Some(s) = path.as_str() {
                set_nested(
                    &mut migrated,
                    &["wallet", "keypair_path"],
                    toml::Value::String(s.into()),
                );
            }
        }
        if table.remove("wallet_private_key_string").is_some() {
            warnings.push(
                "ignored wallet_private_key_string from legacy config; use secrets.toml or 0600 keypair file"
                    .into(),
            );
        }
        for key in table.keys() {
            ignored.push(key.clone());
        }
    }
    if !ignored.is_empty() {
        warnings.push(format!(
            "migrated legacy config v{version}; ignored legacy keys: {}",
            ignored.join(", ")
        ));
    }
    warnings
        .push("migrated legacy config to schema v2 (dev-fee settings are never imported)".into());
    Ok(migrated)
}

fn merge_toml(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, value) in overlay_table {
                if let Some(existing) = base_table.get_mut(&key) {
                    merge_toml(existing, value);
                } else {
                    base_table.insert(key, value);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn apply_env(
    root: &mut toml::Value,
    env: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let reserved = [
        "BEAST_CONFIG",
        "BEAST_SECRETS",
        "BEAST_API_TOKEN",
        "BEAST_API_READ_TOKEN",
        "BEAST_WALLET_KEYPAIR_JSON",
        "BEAST_JUPITER_API_KEY",
        "BEAST_HELIUS_API_KEY",
    ];
    for (key, raw) in env {
        let Some(rest) = key.strip_prefix("BEAST_") else {
            continue;
        };
        if reserved.contains(&key.as_str()) || rest.is_empty() {
            continue;
        }
        let parts: Vec<String> = rest
            .split("__")
            .map(|part| part.to_ascii_lowercase())
            .collect();
        if parts.len() < 2 {
            warnings.push(format!(
                "ignored environment variable {key}: use BEAST_SECTION__KEY"
            ));
            continue;
        }
        let path: Vec<&str> = parts.iter().map(String::as_str).collect();
        let existing = get_nested(root, &path);
        let value = coerce_env(raw, existing).map_err(|e| {
            BeastError::Config(format!("invalid value for environment variable {key}: {e}"))
        })?;
        set_nested(root, &path, value);
    }
    Ok(())
}

fn get_nested<'a>(root: &'a toml::Value, path: &[&str]) -> Option<&'a toml::Value> {
    let mut current = root;
    for part in path {
        current = current.get(*part)?;
    }
    Some(current)
}

fn set_nested(root: &mut toml::Value, path: &[&str], value: toml::Value) {
    if path.is_empty() {
        *root = value;
        return;
    }
    let table = root
        .as_table_mut()
        .expect("configuration root must be a table");
    if path.len() == 1 {
        table.insert(path[0].to_string(), value);
        return;
    }
    let entry = table
        .entry(path[0].to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    if !entry.is_table() {
        *entry = toml::Value::Table(toml::map::Map::new());
    }
    set_nested(entry, &path[1..], value);
}

fn coerce_env(
    raw: &str,
    existing: Option<&toml::Value>,
) -> std::result::Result<toml::Value, String> {
    if let Some(existing) = existing {
        return match existing {
            toml::Value::Boolean(_) => raw
                .parse::<bool>()
                .map(toml::Value::Boolean)
                .map_err(|e| e.to_string()),
            toml::Value::Integer(_) => raw
                .parse::<i64>()
                .map(toml::Value::Integer)
                .map_err(|e| e.to_string()),
            toml::Value::Float(_) => raw
                .parse::<f64>()
                .map(toml::Value::Float)
                .map_err(|e| e.to_string()),
            toml::Value::Array(arr) => {
                let template = arr.first();
                let values = if raw.trim().is_empty() {
                    vec![]
                } else {
                    raw.split(',')
                        .map(str::trim)
                        .map(|item| coerce_scalar(item, template))
                        .collect::<std::result::Result<Vec<_>, _>>()?
                };
                Ok(toml::Value::Array(values))
            }
            _ => Ok(toml::Value::String(raw.to_string())),
        };
    }
    if raw.eq_ignore_ascii_case("true") || raw.eq_ignore_ascii_case("false") {
        return Ok(toml::Value::Boolean(raw.eq_ignore_ascii_case("true")));
    }
    if let Ok(v) = raw.parse::<i64>() {
        return Ok(toml::Value::Integer(v));
    }
    if let Ok(v) = raw.parse::<f64>() {
        return Ok(toml::Value::Float(v));
    }
    if raw.contains(',') {
        return Ok(toml::Value::Array(
            raw.split(',')
                .map(|s| toml::Value::String(s.trim().to_string()))
                .collect(),
        ));
    }
    Ok(toml::Value::String(raw.to_string()))
}

fn coerce_scalar(
    raw: &str,
    template: Option<&toml::Value>,
) -> std::result::Result<toml::Value, String> {
    match template {
        Some(toml::Value::Integer(_)) => raw
            .parse::<i64>()
            .map(toml::Value::Integer)
            .map_err(|e| e.to_string()),
        Some(toml::Value::Float(_)) => raw
            .parse::<f64>()
            .map(toml::Value::Float)
            .map_err(|e| e.to_string()),
        Some(toml::Value::Boolean(_)) => raw
            .parse::<bool>()
            .map(toml::Value::Boolean)
            .map_err(|e| e.to_string()),
        _ => Ok(toml::Value::String(raw.to_string())),
    }
}

impl BeastConfig {
    pub fn effective_ceiling(&self, cli_real: bool) -> Mode {
        if self.runtime.real && (!self.runtime.require_cli_real || cli_real) {
            self.runtime.mode
        } else {
            self.runtime.mode.capped_by(Mode::Radar)
        }
    }

    pub fn strategy_mode(&self, name: &str, cli_real: bool) -> Mode {
        let configured = self
            .strategies
            .get(name)
            .map(|s| s.mode)
            .unwrap_or(Mode::Off);
        configured.capped_by(self.effective_ceiling(cli_real))
    }

    pub fn strategy(&self, name: &str) -> Option<&StrategyConfig> {
        self.strategies.get(name)
    }

    pub fn strategy_or_default(&self, name: &str) -> StrategyConfig {
        self.strategies.get(name).cloned().unwrap_or_default()
    }

    pub fn validate_with_secrets(&self, secrets: &Secrets) -> Result<()> {
        if self.config_version != crate::CONFIG_VERSION {
            return Err(BeastError::Config(format!(
                "config_version must be {}",
                crate::CONFIG_VERSION
            )));
        }
        if self.runtime.real && !self.runtime.mode.is_live() {
            // Valid: useful for the legacy sniper or a live run gated later.
        }
        if self.rpc.urls.is_empty() {
            return Err(BeastError::Config("rpc.urls must not be empty".into()));
        }
        for url in &self.rpc.urls {
            if !(url.starts_with("https://")
                || url.starts_with("http://127.0.0.1")
                || url.starts_with("http://localhost"))
            {
                return Err(BeastError::Config(format!(
                    "rpc URL must use https (or localhost http): {url}"
                )));
            }
        }
        if self.rpc.timeout_ms < 250 {
            return Err(BeastError::Config("rpc.timeout_ms must be >= 250".into()));
        }
        if self.jito.enabled {
            if self.jito.block_engine_urls.is_empty() {
                return Err(BeastError::Config(
                    "jito.block_engine_urls must not be empty".into(),
                ));
            }
            if self.jito.tip_percentile == 0 || self.jito.tip_percentile > 100 {
                return Err(BeastError::Config(
                    "jito.tip_percentile must be 1..=100".into(),
                ));
            }
            if self.jito.send_regions == 0 {
                return Err(BeastError::Config("jito.send_regions must be >= 1".into()));
            }
            if self.jito.requests_per_second == 0 {
                return Err(BeastError::Config(
                    "jito.requests_per_second must be >= 1".into(),
                ));
            }
        }
        if self.risk.balance_floor_lamports == 0 {
            return Err(BeastError::Config(
                "risk.balance_floor_lamports must be > 0".into(),
            ));
        }
        if self.risk.max_inflight_bundles == 0 || self.risk.max_inflight_per_strategy == 0 {
            return Err(BeastError::Config(
                "risk in-flight limits must be > 0".into(),
            ));
        }
        if self.risk.max_model_error_bps <= 0 || self.risk.max_model_error_bps > 2_000 {
            return Err(BeastError::Config(
                "risk.max_model_error_bps must be in 1..=2000".into(),
            ));
        }
        if self.risk.max_feed_lag_ms == 0 || self.risk.max_slot_lag == 0 {
            return Err(BeastError::Config(
                "risk feed/slot lag limits must be > 0".into(),
            ));
        }

        if self.api.enabled {
            if self.api.body_limit_bytes < 1_024 || self.api.body_limit_bytes > 1_048_576 {
                return Err(BeastError::Config(
                    "api.body_limit_bytes must be 1 KiB..1 MiB".into(),
                ));
            }
            if self.api.rate_limit_per_sec == 0 || self.api.read_rate_limit_per_sec == 0 {
                return Err(BeastError::Config("api rate limits must be > 0".into()));
            }
            let addr: SocketAddr = self
                .api
                .bind
                .parse()
                .map_err(|e| BeastError::Config(format!("api.bind must be host:port: {e}")))?;
            if !addr.ip().is_loopback() && !self.api.allow_remote {
                return Err(BeastError::Config(
                    "api.bind is not loopback; set api.allow_remote=true and configure tokens/origins explicitly"
                        .into(),
                ));
            }
            if !addr.ip().is_loopback() && self.api.allowed_origins.is_empty() {
                return Err(BeastError::Config(
                    "remote API requires a non-empty api.allowed_origins allowlist".into(),
                ));
            }
            if secrets.api_bearer_token.is_none() || secrets.api_read_token.is_none() {
                return Err(BeastError::Config(
                    "api.enabled requires BEAST_API_TOKEN and BEAST_API_READ_TOKEN (or secrets.toml)"
                        .into(),
                ));
            }
        }

        if self.guard.enabled {
            if self.guard.min_profit_lamports == 0 {
                return Err(BeastError::Config(
                    "guard.min_profit_lamports must be > 0".into(),
                ));
            }
            if self.guard.mode == GuardMode::Program && self.guard.program_id.len() < 32 {
                return Err(BeastError::Config(
                    "guard.program_id is required in program mode".into(),
                ));
            }
        }

        if self.rent.enabled && !(1..=50).contains(&self.rent.batch_size) {
            return Err(BeastError::Config("rent.batch_size must be 1..=50".into()));
        }
        if self.rent.dust_burn && self.rent.dust_threshold_usd < 0.0 {
            return Err(BeastError::Config(
                "rent.dust_threshold_usd must be >= 0".into(),
            ));
        }

        for (name, strategy) in &self.strategies {
            if strategy.mode.rank() > self.runtime.mode.rank() {
                return Err(BeastError::Config(format!(
                    "strategy {name} mode {} exceeds runtime ceiling {}",
                    strategy.mode, self.runtime.mode
                )));
            }
            if !(0.01..=0.90).contains(&strategy.tip_alpha_initial)
                || !strategy.tip_alpha_initial.is_finite()
            {
                return Err(BeastError::Config(format!(
                    "strategy {name} tip_alpha_initial must be within [0.01, 0.90]"
                )));
            }
            if strategy.tip_cap_lamports < 1_000
                && name
                    .parse::<StrategyKind>()
                    .map(StrategyKind::requires_bundle_guard)
                    .unwrap_or(true)
            {
                return Err(BeastError::Config(format!(
                    "strategy {name} tip_cap_lamports must be >= 1000 for bundle strategies"
                )));
            }
            if strategy.max_attempts_per_min == 0 {
                return Err(BeastError::Config(format!(
                    "strategy {name} max_attempts_per_min must be > 0"
                )));
            }
            if strategy.mode.is_live() && strategy.max_notional_lamports == 0 {
                if let Ok(kind) = name.parse::<StrategyKind>() {
                    if kind.requires_bundle_guard() {
                        return Err(BeastError::Config(format!(
                            "strategy {name} is live but max_notional_lamports is 0"
                        )));
                    }
                }
            }
        }

        if self.runtime.real {
            let has_env_key = secrets.wallet_keypair_json.is_some();
            let key_path = self.wallet.keypair_path.as_ref();
            if !has_env_key {
                let Some(path) = key_path else {
                    return Err(BeastError::Config(
                        "real mode requires wallet.keypair_path or BEAST_WALLET_KEYPAIR_JSON"
                            .into(),
                    ));
                };
                if !path.exists() {
                    return Err(BeastError::Config(format!(
                        "wallet keypair file {} does not exist",
                        path.display()
                    )));
                }
                #[cfg(unix)]
                if self.wallet.require_0600 {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = fs::metadata(path)
                        .map_err(|e| BeastError::Config(format!("cannot stat keypair: {e}")))?
                        .permissions()
                        .mode()
                        & 0o777;
                    if mode & 0o077 != 0 {
                        return Err(BeastError::Config(format!(
                            "wallet keypair {} has permissions {:o}; chmod 600 it first",
                            path.display(),
                            mode
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn temp_secrets(dir: &Path) -> PathBuf {
        dir.join("secrets.toml")
    }

    #[test]
    fn embedded_defaults_parse_and_validate() {
        let cfg = BeastConfig::default();
        let secrets = Secrets {
            api_bearer_token: Some("a".repeat(24)),
            api_read_token: Some("b".repeat(24)),
            ..Secrets::default()
        };
        cfg.validate_with_secrets(&secrets).unwrap();
        assert_eq!(cfg.config_version, 2);
        assert_eq!(cfg.runtime.mode, Mode::Radar);
        assert_eq!(cfg.strategies["cross_dex_arb"].mode, Mode::Radar);
    }

    #[test]
    fn env_overrides_nested_and_arrays() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = temp_secrets(dir.path());
        let env = env(&[
            ("BEAST_RUNTIME__MODE", "canary"),
            ("BEAST_RUNTIME__REAL", "true"),
            ("BEAST_RPC__URLS", "https://a.example,https://b.example"),
            ("BEAST_STRATEGIES__CROSS_DEX_ARB__MODE", "canary"),
            ("BEAST_STRATEGIES__CROSS_DEX_ARB__MIN_NET_LAMPORTS", "42000"),
            ("BEAST_API__BIND", "127.0.0.1:9999"),
            ("BEAST_API_TOKEN", &"x".repeat(24)),
            ("BEAST_API_READ_TOKEN", &"y".repeat(24)),
            ("BEAST_SECRETS", secrets.to_str().unwrap()),
            ("BEAST_WALLET_KEYPAIR_JSON", "[1,2,3]"),
        ]);
        let loaded = load_with_env(None, true, &env).unwrap();
        assert_eq!(loaded.config.rpc.urls.len(), 2);
        assert_eq!(loaded.config.strategies["cross_dex_arb"].mode, Mode::Canary);
        assert_eq!(
            loaded.config.strategies["cross_dex_arb"].min_net_lamports,
            42_000
        );
        assert_eq!(loaded.config.api.bind, "127.0.0.1:9999");
        assert_eq!(loaded.config.runtime.mode, Mode::Canary);
    }

    #[test]
    fn canary_without_real_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("config.toml");
        fs::write(
            &cfg_path,
            r#"
config_version = 2
[runtime]
mode = "canary"
real = false
"#,
        )
        .unwrap();
        let env = env(&[
            ("BEAST_API_TOKEN", &"x".repeat(24)),
            ("BEAST_API_READ_TOKEN", &"y".repeat(24)),
            ("BEAST_SECRETS", temp_secrets(dir.path()).to_str().unwrap()),
        ]);
        let err = load_with_env(Some(&cfg_path), true, &env).unwrap_err();
        assert!(format!("{err}").contains("runtime.real is false"));
    }

    #[test]
    fn read_token_must_be_configured() {
        let cfg = BeastConfig::default();
        let secrets = Secrets {
            api_bearer_token: Some("a".repeat(24)),
            ..Secrets::default()
        };
        assert!(cfg.validate_with_secrets(&secrets).is_err());
    }

    #[test]
    fn admin_and_read_tokens_must_differ() {
        let same = "z".repeat(24);
        let secrets = Secrets {
            api_bearer_token: Some(same.clone()),
            api_read_token: Some(same),
            ..Secrets::default()
        };
        assert!(secrets.validate().is_err());
    }

    #[test]
    fn legacy_config_is_migrated_and_warns() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("config.toml");
        fs::write(
            &cfg_path,
            r#"
solana_rpc_urls = ["https://legacy.example"]
solana_ws_urls = ["wss://legacy.example"]
wallet_keypair_path = "./old-key.json"
dev_fee_enabled = true
buy_amount = 0.1
"#,
        )
        .unwrap();
        let env = env(&[
            ("BEAST_API_TOKEN", &"x".repeat(24)),
            ("BEAST_API_READ_TOKEN", &"y".repeat(24)),
            ("BEAST_SECRETS", temp_secrets(dir.path()).to_str().unwrap()),
        ]);
        let loaded = load_with_env(Some(&cfg_path), false, &env).unwrap();
        assert_eq!(loaded.config.rpc.urls, vec!["https://legacy.example"]);
        assert!(loaded
            .warnings
            .iter()
            .any(|w| w.contains("migrated legacy config")));
        assert_eq!(loaded.config.runtime.mode, Mode::Radar);
    }

    #[test]
    fn strategy_mode_cannot_exceed_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("config.toml");
        fs::write(
            &cfg_path,
            r#"
config_version = 2
[runtime]
mode = "radar"
[strategies.cross_dex_arb]
mode = "live"
max_notional_lamports = 1
"#,
        )
        .unwrap();
        let env = env(&[
            ("BEAST_API_TOKEN", &"x".repeat(24)),
            ("BEAST_API_READ_TOKEN", &"y".repeat(24)),
            ("BEAST_SECRETS", temp_secrets(dir.path()).to_str().unwrap()),
        ]);
        let err = load_with_env(Some(&cfg_path), false, &env).unwrap_err();
        assert!(format!("{err}").contains("exceeds runtime ceiling"));
    }

    #[test]
    fn secrets_file_is_created_0600() {
        let dir = tempfile::tempdir().unwrap();
        let env = env(&[("BEAST_SECRETS", temp_secrets(dir.path()).to_str().unwrap())]);
        let loaded = load_with_env(None, false, &env).unwrap();
        assert!(loaded.generated_secrets);
        let path = loaded.secrets_path().unwrap();
        assert!(path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn remote_api_requires_allowlist() {
        let mut cfg = BeastConfig::default();
        cfg.api.bind = "0.0.0.0:8081".into();
        cfg.api.allow_remote = true;
        let secrets = Secrets {
            api_bearer_token: Some("a".repeat(24)),
            api_read_token: Some("b".repeat(24)),
            ..Secrets::default()
        };
        assert!(cfg.validate_with_secrets(&secrets).is_err());
    }
}
