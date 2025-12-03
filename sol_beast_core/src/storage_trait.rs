// Storage abstraction - allows both file-based (native) and localStorage (WASM)

use crate::error::CoreError;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

/// Result type for storage operations
pub type StorageResult<T> = Result<T, CoreError>;

/// Abstract storage backend trait
/// Native implementations can use files, WASM can use localStorage
#[async_trait(?Send)]
pub trait StorageBackend {
    /// Save data with a key
    async fn save<T: Serialize>(&self, key: &str, data: &T) -> StorageResult<()>;
    
    /// Load data by key
    async fn load<T: DeserializeOwned>(&self, key: &str) -> StorageResult<Option<T>>;
    
    /// Remove data by key
    async fn remove(&self, key: &str) -> StorageResult<()>;
    
    /// Check if key exists
    async fn exists(&self, key: &str) -> StorageResult<bool>;
    
    /// List all keys (optional, may not be supported by all backends)
    async fn list_keys(&self) -> StorageResult<Vec<String>> {
        Ok(Vec::new())
    }
}

/// Standard storage keys used across the application
pub mod keys {
    pub const SETTINGS: &str = "bot_settings";
    pub const HOLDINGS: &str = "bot_holdings";
    pub const TRADES: &str = "bot_trades";
    pub const STATE: &str = "bot_state";
}
