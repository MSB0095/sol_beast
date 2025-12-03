use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[cfg(feature = "native")]
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Invalid keypair: {0}")]
    InvalidKeypair(String),
    
    #[cfg(feature = "native")]
    #[error("I/O error: {0}")]
    Io(String),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("TOML serialization error: {0}")]
    TomlSerialization(String),
    
    #[error("Integer conversion error: {0}")]
    IntConversion(#[from] std::num::TryFromIntError),
    
    #[error("Initialization error: {0}")]
    Init(String),
    
    #[error("Conversion error: {0}")]
    Conversion(String),
    
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("WebSocket error: {0}")]
    WebSocket(String),
    
    #[error("Transaction error: {0}")]
    Transaction(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

// Alias for backward compatibility
pub type AppError = CoreError;

#[cfg(feature = "native")]
impl From<std::io::Error> for CoreError {
    fn from(err: std::io::Error) -> Self {
        CoreError::Io(err.to_string())
    }
}

#[cfg(feature = "native")]
impl From<config::ConfigError> for CoreError {
    fn from(err: config::ConfigError) -> Self {
        CoreError::Config(err.to_string())
    }
}

impl From<toml::ser::Error> for CoreError {
    fn from(err: toml::ser::Error) -> Self {
        CoreError::TomlSerialization(err.to_string())
    }
}
