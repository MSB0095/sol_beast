use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Wallet error: {0}")]
    Wallet(String),
    
    #[error("Transaction error: {0}")]
    Transaction(String),
    
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Invalid configuration: {0}")]
    Config(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Not authorized: {0}")]
    Unauthorized(String),
    
    #[error("Invalid keypair: {0}")]
    InvalidKeypair(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

// Implement conversions from common error types
impl From<serde_json::Error> for CoreError {
    fn from(err: serde_json::Error) -> Self {
        CoreError::Serialization(err.to_string())
    }
}

impl From<bs58::decode::Error> for CoreError {
    fn from(err: bs58::decode::Error) -> Self {
        CoreError::Parse(err.to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<solana_sdk::pubkey::ParsePubkeyError> for CoreError {
    fn from(err: solana_sdk::pubkey::ParsePubkeyError) -> Self {
        CoreError::Parse(err.to_string())
    }
}

impl From<std::io::Error> for CoreError {
    fn from(err: std::io::Error) -> Self {
        CoreError::Storage(err.to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<solana_client::client_error::ClientError> for CoreError {
    fn from(err: solana_client::client_error::ClientError) -> Self {
        CoreError::Rpc(err.to_string())
    }
}

// For WASM, convert to JsValue for error propagation
#[cfg(target_arch = "wasm32")]
impl From<CoreError> for wasm_bindgen::JsValue {
    fn from(err: CoreError) -> Self {
        wasm_bindgen::JsValue::from_str(&err.to_string())
    }
}
