// Rust
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub wss_url: String,
    pub https_url: String,
    pub pump_fun_program: String,
}

impl Settings {
    pub fn from_file(path: &str) -> Self {
        let builder = config::Config::builder()
            .add_source(config::File::with_name(path));
        let cfg = builder.build().expect("Failed to build config");
        cfg.try_deserialize().expect("Failed to load config")
    }
}