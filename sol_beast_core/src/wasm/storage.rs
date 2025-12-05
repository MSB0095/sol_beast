// WASM storage implementation using localStorage
use wasm_bindgen::prelude::*;
use web_sys::window;
use serde::{Serialize, Deserialize};

const SETTINGS_KEY: &str = "sol_beast_settings";
const STATE_KEY: &str = "sol_beast_state";

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

pub fn load_settings<T: for<'de> Deserialize<'de>>() -> Result<Option<T>, JsValue> {
    let storage = window()
        .ok_or_else(|| JsValue::from_str("No window"))?
        .local_storage()?
        .ok_or_else(|| JsValue::from_str("No localStorage"))?;
    
    match storage.get_item(SETTINGS_KEY)? {
        Some(json) => {
            // Try to deserialize, but if it fails (corrupted/outdated data), clear and return None
            match serde_json::from_str(&json) {
                Ok(settings) => Ok(Some(settings)),
                Err(e) => {
                    // Log the error via console
                    web_sys::console::warn_1(&format!("Failed to deserialize settings from localStorage (possibly corrupted or outdated): {}. Clearing corrupted data.", e).into());
                    
                    // Clear the corrupted data
                    let _ = storage.remove_item(SETTINGS_KEY);
                    
                    // Return None so defaults will be used
                    Ok(None)
                }
            }
        }
        None => Ok(None),
    }
}

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

pub fn load_state<T: for<'de> Deserialize<'de>>() -> Result<Option<T>, JsValue> {
    let storage = window()
        .ok_or_else(|| JsValue::from_str("No window"))?
        .local_storage()?
        .ok_or_else(|| JsValue::from_str("No localStorage"))?;
    
    match storage.get_item(STATE_KEY)? {
        Some(json) => {
            let state = serde_json::from_str(&json)
                .map_err(|e| JsValue::from_str(&format!("Deserialization error: {}", e)))?;
            Ok(Some(state))
        }
        None => Ok(None),
    }
}

pub fn clear_all() -> Result<(), JsValue> {
    let storage = window()
        .ok_or_else(|| JsValue::from_str("No window"))?
        .local_storage()?
        .ok_or_else(|| JsValue::from_str("No localStorage"))?;
    
    storage.remove_item(SETTINGS_KEY)?;
    storage.remove_item(STATE_KEY)?;
    Ok(())
}
