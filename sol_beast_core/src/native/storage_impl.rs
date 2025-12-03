// Native file-based storage implementation

use crate::storage_trait::{StorageBackend, StorageResult};
use crate::error::CoreError;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use std::path::PathBuf;
use log::debug;

/// File-based storage backend for native (CLI) mode
pub struct FileStorage {
    base_dir: PathBuf,
}

impl FileStorage {
    /// Create a new file storage backend with the specified base directory
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
    
    /// Get the full path for a storage key
    fn get_path(&self, key: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", key))
    }
}

#[async_trait(?Send)]
impl StorageBackend for FileStorage {
    async fn save<T: Serialize>(&self, key: &str, data: &T) -> StorageResult<()> {
        let path = self.get_path(key);
        debug!("Saving data to file: {:?}", path);
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| CoreError::Io(format!("Failed to create directory: {}", e)))?;
        }
        
        // Serialize to JSON
        let json = serde_json::to_string_pretty(data)
            .map_err(|e| CoreError::Json(e))?;
        
        // Write to file
        tokio::fs::write(&path, json).await
            .map_err(|e| CoreError::Io(format!("Failed to write file: {}", e)))?;
        
        debug!("Data saved successfully to {:?}", path);
        Ok(())
    }
    
    async fn load<T: DeserializeOwned>(&self, key: &str) -> StorageResult<Option<T>> {
        let path = self.get_path(key);
        debug!("Loading data from file: {:?}", path);
        
        // Check if file exists
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            debug!("File does not exist: {:?}", path);
            return Ok(None);
        }
        
        // Read file
        let json = tokio::fs::read_to_string(&path).await
            .map_err(|e| CoreError::Io(format!("Failed to read file: {}", e)))?;
        
        // Deserialize
        let data = serde_json::from_str(&json)
            .map_err(|e| CoreError::Json(e))?;
        
        debug!("Data loaded successfully from {:?}", path);
        Ok(Some(data))
    }
    
    async fn remove(&self, key: &str) -> StorageResult<()> {
        let path = self.get_path(key);
        debug!("Removing file: {:?}", path);
        
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            tokio::fs::remove_file(&path).await
                .map_err(|e| CoreError::Io(format!("Failed to remove file: {}", e)))?;
            debug!("File removed successfully: {:?}", path);
        } else {
            debug!("File does not exist, nothing to remove: {:?}", path);
        }
        
        Ok(())
    }
    
    async fn exists(&self, key: &str) -> StorageResult<bool> {
        let path = self.get_path(key);
        Ok(tokio::fs::try_exists(&path).await.unwrap_or(false))
    }
    
    async fn list_keys(&self) -> StorageResult<Vec<String>> {
        debug!("Listing keys in directory: {:?}", self.base_dir);
        
        // Create directory if it doesn't exist
        if !tokio::fs::try_exists(&self.base_dir).await.unwrap_or(false) {
            return Ok(Vec::new());
        }
        
        let mut keys = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.base_dir).await
            .map_err(|e| CoreError::Io(format!("Failed to read directory: {}", e)))?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| CoreError::Io(format!("Failed to read directory entry: {}", e)))? {
            
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    keys.push(stem.to_string());
                }
            }
        }
        
        debug!("Found {} keys", keys.len());
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;
    
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        value: String,
        count: u32,
    }
    
    #[tokio::test]
    async fn test_file_storage_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_path_buf());
        
        let data = TestData {
            value: "test".to_string(),
            count: 42,
        };
        
        // Save
        storage.save("test_key", &data).await.unwrap();
        
        // Load
        let loaded: Option<TestData> = storage.load("test_key").await.unwrap();
        assert_eq!(loaded, Some(data));
    }
    
    #[tokio::test]
    async fn test_file_storage_remove() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_path_buf());
        
        let data = TestData {
            value: "test".to_string(),
            count: 1,
        };
        
        // Save
        storage.save("test_key", &data).await.unwrap();
        assert!(storage.exists("test_key").await.unwrap());
        
        // Remove
        storage.remove("test_key").await.unwrap();
        assert!(!storage.exists("test_key").await.unwrap());
    }
}
