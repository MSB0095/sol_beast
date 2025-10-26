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