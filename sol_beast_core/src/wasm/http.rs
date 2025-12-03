// WASM HTTP client implementation using fetch API

use crate::metadata::{HttpClient, MetadataResult};
use crate::error::CoreError;
use async_trait::async_trait;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};
use log::debug;

/// WASM HTTP client using browser fetch API
pub struct WasmHttpClient;

impl WasmHttpClient {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WasmHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl HttpClient for WasmHttpClient {
    async fn fetch_text(&self, url: &str) -> MetadataResult<String> {
        debug!("Fetching URL via WASM: {}", url);
        
        // Create request
        let opts = RequestInit::new();
        opts.set_method("GET");
        opts.set_mode(RequestMode::Cors);
        
        let request = Request::new_with_str_and_init(url, &opts)
            .map_err(|e| CoreError::Rpc(format!("Failed to create request: {:?}", e)))?;
        
        // Get window and fetch
        let window = web_sys::window()
            .ok_or_else(|| CoreError::Init("No window object available".to_string()))?;
        
        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| CoreError::Rpc(format!("Fetch failed: {:?}", e)))?;
        
        // Cast to Response
        let resp: Response = resp_value.dyn_into()
            .map_err(|_| CoreError::Rpc("Failed to cast response".to_string()))?;
        
        // Check status
        if !resp.ok() {
            return Err(CoreError::Rpc(format!("HTTP error: {}", resp.status())));
        }
        
        // Get text
        let text_promise = resp.text()
            .map_err(|e| CoreError::Rpc(format!("Failed to get text: {:?}", e)))?;
        
        let text_value = JsFuture::from(text_promise)
            .await
            .map_err(|e| CoreError::Rpc(format!("Failed to await text: {:?}", e)))?;
        
        let text = text_value.as_string()
            .ok_or_else(|| CoreError::Rpc("Response text is not a string".to_string()))?;
        
        Ok(text)
    }
}
