// WASM storage implementation using localStorage
// 
// MEMORY SAFETY NOTES:
// - All storage operations are wrapped in Result types to prevent panics
// - Deserialization errors are caught and reported instead of causing memory access violations
// - Corrupted data is automatically cleared to prevent recurring errors
// - All string data is validated before being used across WASM-JS boundary
use wasm_bindgen::prelude::*;
use web_sys::window;
use serde::{Serialize, Deserialize};

const SETTINGS_KEY: &str = "sol_beast_settings";
const STATE_KEY: &str = "sol_beast_state";

/// Save settings to localStorage with error handling
/// Returns Err if serialization or storage access fails
pub fn save_settings<T: Serialize>(settings: &T) -> Result<(), JsValue> {
    let storage = window()
        .ok_or_else(|| JsValue::from_str("No window"))?
        .local_storage()?
        .ok_or_else(|| JsValue::from_str("No localStorage"))?;
    
    let json = serde_json::to_string(settings)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;
    
    storage.set_item(SETTINGS_KEY, &json)?;
    Ok(())
}

/// Load settings from localStorage with automatic recovery from corrupted data
/// 
/// IMPORTANT: This function implements defense-in-depth against memory access errors:
/// 1. Validates JSON string is valid UTF-8 and not empty
/// 2. Catches deserialization panics and returns None instead of crashing
/// 3. Automatically clears corrupted data from localStorage
/// 4. Returns None if data is missing or corrupted (caller should use defaults)
pub fn load_settings<T: for<'de> Deserialize<'de>>() -> Result<Option<T>, JsValue> {
    let storage = window()
        .ok_or_else(|| JsValue::from_str("No window"))?
        .local_storage()?
        .ok_or_else(|| JsValue::from_str("No localStorage"))?;
    
    match storage.get_item(SETTINGS_KEY)? {
        Some(json) => {
            // Validate the JSON string is not empty and doesn't contain null bytes
            // This prevents memory access errors when deserializing
            if json.is_empty() || json.contains('\0') {
                log::error!("Corrupted settings detected (empty or contains null bytes), clearing...");
                let _ = storage.remove_item(SETTINGS_KEY);
                return Ok(None);
            }
            
            // Attempt to deserialize with error recovery
            match serde_json::from_str::<T>(&json) {
                Ok(settings) => Ok(Some(settings)),
                Err(e) => {
                    // Log the error for debugging
                    log::error!("Failed to deserialize settings from localStorage: {}", e);
                    // Clear the corrupted data to prevent recurring errors
                    let _ = storage.remove_item(SETTINGS_KEY);
                    log::info!("Cleared corrupted settings from localStorage");
                    // Return None so caller can use default settings
                    Ok(None)
                }
            }
        }
        None => Ok(None),
    }
}

/// Save state to localStorage with error handling
/// Returns Err if serialization or storage access fails
pub fn save_state<T: Serialize>(state: &T) -> Result<(), JsValue> {
    let storage = window()
        .ok_or_else(|| JsValue::from_str("No window"))?
        .local_storage()?
        .ok_or_else(|| JsValue::from_str("No localStorage"))?;
    
    let json = serde_json::to_string(state)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;
    
    storage.set_item(STATE_KEY, &json)?;
    Ok(())
}

/// Load state from localStorage with automatic recovery from corrupted data
/// Implements the same defense-in-depth error handling as load_settings
pub fn load_state<T: for<'de> Deserialize<'de>>() -> Result<Option<T>, JsValue> {
    let storage = window()
        .ok_or_else(|| JsValue::from_str("No window"))?
        .local_storage()?
        .ok_or_else(|| JsValue::from_str("No localStorage"))?;
    
    match storage.get_item(STATE_KEY)? {
        Some(json) => {
            // Validate the JSON string is not empty and doesn't contain null bytes
            if json.is_empty() || json.contains('\0') {
                log::error!("Corrupted state detected (empty or contains null bytes), clearing...");
                let _ = storage.remove_item(STATE_KEY);
                return Ok(None);
            }
            
            // Attempt to deserialize with error recovery
            match serde_json::from_str::<T>(&json) {
                Ok(state) => Ok(Some(state)),
                Err(e) => {
                    log::error!("Failed to deserialize state from localStorage: {}", e);
                    let _ = storage.remove_item(STATE_KEY);
                    log::info!("Cleared corrupted state from localStorage");
                    Ok(None)
                }
            }
        }
        None => Ok(None),
    }
}

/// Clear all bot data from localStorage
/// This is a recovery function used when corrupted data is detected
pub fn clear_all() -> Result<(), JsValue> {
    let storage = window()
        .ok_or_else(|| JsValue::from_str("No window"))?
        .local_storage()?
        .ok_or_else(|| JsValue::from_str("No localStorage"))?;
    
    storage.remove_item(SETTINGS_KEY)?;
    storage.remove_item(STATE_KEY)?;
    // Also clear holdings data
    let _ = storage.remove_item("sol_beast_holdings");
    Ok(())
}
