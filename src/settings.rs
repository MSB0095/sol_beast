use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub solana_ws_urls: Vec<String>,
    pub solana_rpc_urls: Vec<String>,
    pub pump_fun_program: String,
    pub metadata_program: String,
    pub wallet_keypair_path: String,
    pub tp_percent: f64,
    pub sl_percent: f64,
    pub timeout_secs: i64,
    pub cache_capacity: usize,
    pub price_cache_ttl_secs: u64,
}

impl Settings {
    pub fn from_file(path: &str) -> Self {
        let builder = config::Config::builder()
            .add_source(config::File::with_name(path));
        let cfg = builder.build().expect("Failed to build config");
        cfg.try_deserialize().expect("Failed to load config")
    }
}