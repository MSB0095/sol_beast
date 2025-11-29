use crate::core_mod::error::CoreError;
use crate::core_mod::models::UserAccount;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub address: String,
    pub connected: bool,
    pub balance: Option<u64>, // lamports
}

/// Wallet manager handles wallet connections and user account management
pub struct WalletManager {
    current_wallet: Option<WalletInfo>,
}

impl WalletManager {
    pub fn new() -> Self {
        Self {
            current_wallet: None,
        }
    }

    pub fn connect_wallet(&mut self, address: String) -> Result<(), CoreError> {
        // Validate the address is a valid Solana pubkey
        Pubkey::from_str(&address)
            .map_err(|e| CoreError::Wallet(format!("Invalid wallet address: {}", e)))?;

        self.current_wallet = Some(WalletInfo {
            address,
            connected: true,
            balance: None,
        });

        Ok(())
    }

    pub fn disconnect_wallet(&mut self) {
        self.current_wallet = None;
    }

    pub fn get_current_wallet(&self) -> Option<&WalletInfo> {
        self.current_wallet.as_ref()
    }

    pub fn is_connected(&self) -> bool {
        self.current_wallet.as_ref().map(|w| w.connected).unwrap_or(false)
    }

    pub fn get_wallet_address(&self) -> Option<&str> {
        self.current_wallet.as_ref().map(|w| w.address.as_str())
    }

    pub fn update_balance(&mut self, balance: u64) {
        if let Some(wallet) = &mut self.current_wallet {
            wallet.balance = Some(balance);
        }
    }
}

impl Default for WalletManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Storage interface for user accounts
#[cfg(target_arch = "wasm32")]
pub mod storage {
    use super::*;
    use wasm_bindgen::JsValue;
    use web_sys::window;

    // Note: localStorage is origin-scoped but not protected against XSS attacks.
    // User data stored here does not contain private keys (those stay in wallet extension).
    // Consider additional encryption for sensitive fields in future versions.
    const STORAGE_KEY_PREFIX: &str = "sol_beast_user_";

    pub fn save_user_account(account: &UserAccount) -> Result<(), CoreError> {
        let window = window().ok_or_else(|| CoreError::Storage("No window object".to_string()))?;
        let storage = window
            .local_storage()
            .map_err(|_| CoreError::Storage("Failed to access localStorage".to_string()))?
            .ok_or_else(|| CoreError::Storage("localStorage not available".to_string()))?;

        let key = format!("{}{}", STORAGE_KEY_PREFIX, account.wallet_address);
        let value = serde_json::to_string(account)
            .map_err(|e| CoreError::Serialization(e.to_string()))?;

        storage
            .set_item(&key, &value)
            .map_err(|_| CoreError::Storage("Failed to save user account".to_string()))?;

        Ok(())
    }

    pub fn load_user_account(wallet_address: &str) -> Result<Option<UserAccount>, CoreError> {
        let window = window().ok_or_else(|| CoreError::Storage("No window object".to_string()))?;
        let storage = window
            .local_storage()
            .map_err(|_| CoreError::Storage("Failed to access localStorage".to_string()))?
            .ok_or_else(|| CoreError::Storage("localStorage not available".to_string()))?;

        let key = format!("{}{}", STORAGE_KEY_PREFIX, wallet_address);
        let value = storage
            .get_item(&key)
            .map_err(|_| CoreError::Storage("Failed to load user account".to_string()))?;

        match value {
            Some(json) => {
                let account = serde_json::from_str(&json)
                    .map_err(|e| CoreError::Serialization(e.to_string()))?;
                Ok(Some(account))
            }
            None => Ok(None),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod storage {
    use super::*;
    use std::path::PathBuf;

    fn get_storage_path(wallet_address: &str) -> Result<PathBuf, CoreError> {
        let mut path = std::env::current_dir()
            .map_err(|e| CoreError::Storage(format!("Failed to get current directory: {}", e)))?;
        path.push(".sol_beast_data");
        std::fs::create_dir_all(&path)
            .map_err(|e| CoreError::Storage(format!("Failed to create storage directory: {}", e)))?;
        path.push(format!("user_{}.json", wallet_address));
        Ok(path)
    }

    pub fn save_user_account(account: &UserAccount) -> Result<(), CoreError> {
        let path = get_storage_path(&account.wallet_address)?;
        let json = serde_json::to_string_pretty(account)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_user_account(wallet_address: &str) -> Result<Option<UserAccount>, CoreError> {
        let path = get_storage_path(wallet_address)?;
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(path)?;
        let account = serde_json::from_str(&json)?;
        Ok(Some(account))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_connection() {
        let mut manager = WalletManager::new();
        assert!(!manager.is_connected());

        let valid_address = "11111111111111111111111111111111";
        manager.connect_wallet(valid_address.to_string()).unwrap();
        assert!(manager.is_connected());
        assert_eq!(manager.get_wallet_address(), Some(valid_address));

        manager.disconnect_wallet();
        assert!(!manager.is_connected());
    }

    #[test]
    fn test_invalid_wallet_address() {
        let mut manager = WalletManager::new();
        let result = manager.connect_wallet("invalid".to_string());
        assert!(result.is_err());
    }
}
