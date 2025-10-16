// Rust
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub solana_ws_urls: Vec<String>,
    pub solana_rpc_urls: Vec<String>,
    pub pump_fun_contract: String,
    // ... add other fields as needed
}

impl Settings {
    pub fn from_file(path: &str) -> Self {
        let builder = config::Config::builder()
            .add_source(config::File::with_name(path));
        let cfg = builder.build().expect("Failed to build config");
        cfg.try_deserialize().expect("Failed to load config")
    }
}