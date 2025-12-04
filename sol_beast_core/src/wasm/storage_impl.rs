// WASM localStorage-based storage implementation

use crate::storage_trait::{StorageBackend, StorageResult};
use crate::error::CoreError;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use web_sys::window;
use log::debug;

/// LocalStorage-based storage backend for WASM mode
pub struct LocalStorageBackend {
    prefix: String,
}

impl LocalStorageBackend {
    /// Create a new localStorage backend with the specified key prefix
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }
    
    /// Create default instance with "sol_beast_" prefix
    pub fn default_instance() -> Self {
        Self::new("sol_beast_".to_string())
    }
    
    /// Get the full key with prefix
    fn get_full_key(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key)
    }
    
    /// Get localStorage instance
    fn get_storage(&self) -> StorageResult<web_sys::Storage> {
        window()
            .ok_or_else(|| CoreError::Init("No window object available".to_string()))?
            .local_storage()
            .map_err(|e| CoreError::WebSocket(format!("Failed to access localStorage: {:?}", e)))?
            .ok_or_else(|| CoreError::Init("localStorage not available".to_string()))
    }
}

#[async_trait(?Send)]
impl StorageBackend for LocalStorageBackend {
    async fn save<T: Serialize>(&self, key: &str, data: &T) -> StorageResult<()> {
        let full_key = self.get_full_key(key);
        debug!("Saving data to localStorage: {}", full_key);
        
        let storage = self.get_storage()?;
        
        // Serialize to JSON
        let json = serde_json::to_string(data)
            .map_err(CoreError::Json)?;
        
        // Save to localStorage
        storage.set_item(&full_key, &json)
            .map_err(|e| CoreError::WebSocket(format!("Failed to save to localStorage: {:?}", e)))?;
        
        debug!("Data saved successfully to localStorage: {}", full_key);
        Ok(())
    }
    
    async fn load<T: DeserializeOwned>(&self, key: &str) -> StorageResult<Option<T>> {
        let full_key = self.get_full_key(key);
        debug!("Loading data from localStorage: {}", full_key);
        
        let storage = self.get_storage()?;
        
        // Get item from localStorage
        let json = storage.get_item(&full_key)
            .map_err(|e| CoreError::WebSocket(format!("Failed to read from localStorage: {:?}", e)))?;
        
        // Return None if key doesn't exist
        let json = match json {
            Some(j) => j,
            None => {
                debug!("Key does not exist in localStorage: {}", full_key);
                return Ok(None);
            }
        };
        
        // Deserialize
        let data = serde_json::from_str(&json)
            .map_err(CoreError::Json)?;
        
        debug!("Data loaded successfully from localStorage: {}", full_key);
        Ok(Some(data))
    }
    
    async fn remove(&self, key: &str) -> StorageResult<()> {
        let full_key = self.get_full_key(key);
        debug!("Removing from localStorage: {}", full_key);
        
        let storage = self.get_storage()?;
        
        storage.remove_item(&full_key)
            .map_err(|e| CoreError::WebSocket(format!("Failed to remove from localStorage: {:?}", e)))?;
        
        debug!("Item removed successfully from localStorage: {}", full_key);
        Ok(())
    }
    
    async fn exists(&self, key: &str) -> StorageResult<bool> {
        let full_key = self.get_full_key(key);
        let storage = self.get_storage()?;
        
        let item = storage.get_item(&full_key)
            .map_err(|e| CoreError::WebSocket(format!("Failed to check localStorage: {:?}", e)))?;
        
        Ok(item.is_some())
    }
    
    async fn list_keys(&self) -> StorageResult<Vec<String>> {
        debug!("Listing keys from localStorage with prefix: {}", self.prefix);
        
        let storage = self.get_storage()?;
        let length = storage.length()
            .map_err(|e| CoreError::WebSocket(format!("Failed to get localStorage length: {:?}", e)))?;
        
        let mut keys = Vec::new();
        for i in 0..length {
            if let Ok(Some(key)) = storage.key(i) {
                // Only include keys with our prefix
                if key.starts_with(&self.prefix) {
                    // Strip the prefix
                    let stripped = key[self.prefix.len()..].to_string();
                    keys.push(stripped);
                }
            }
        }
        
        debug!("Found {} keys with prefix {}", keys.len(), self.prefix);
        Ok(keys)
    }
}
