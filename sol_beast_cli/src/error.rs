use std::num::TryFromIntError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Invalid keypair: {0}")]
    InvalidKeypair(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML serialization error: {0}")]
    TomlSerialization(#[from] toml::ser::Error),
    #[error("Integer conversion error: {0}")]
    IntConversion(#[from] TryFromIntError),
    #[error("Initialization error: {0}")]
    Init(String),
    #[error("Conversion error: {0}")]
    Conversion(String),
}
