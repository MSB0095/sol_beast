// Native HTTP client implementation using reqwest

use crate::metadata::{HttpClient, MetadataResult};
use crate::error::CoreError;
use async_trait::async_trait;
use reqwest::Client;
use log::debug;

/// Native HTTP client using reqwest
pub struct NativeHttpClient {
    client: Client,
}

impl NativeHttpClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for NativeHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl HttpClient for NativeHttpClient {
    async fn fetch_text(&self, url: &str) -> MetadataResult<String> {
        debug!("Fetching URL: {}", url);
        
        let response = self.client.get(url)
            .send()
            .await
            .map_err(|e| CoreError::Rpc(format!("HTTP request failed: {}", e)))?;
        
        let text = response.text()
            .await
            .map_err(|e| CoreError::Rpc(format!("Failed to read response body: {}", e)))?;
        
        Ok(text)
    }
}
