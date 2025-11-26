use log::debug;
use serde_json::Value;
use async_trait::async_trait;
use crate::core::error::CoreError;

/// Trait that abstracts sending JSON-RPC requests to a provider.
#[async_trait]
pub trait RpcProvider: Send + Sync {
    async fn send_json(&self, request: Value) -> Result<Value, CoreError>;
}

#[cfg(feature = "native-rpc")]
pub mod native {
    use super::*;
    use reqwest::Client;

    pub struct ReqwestRpcProvider {
        client: Client,
        url: String,
    }

    impl ReqwestRpcProvider {
        pub fn new(url: String) -> Self {
            Self { client: Client::new(), url }
        }
    }

    #[async_trait]
    impl RpcProvider for ReqwestRpcProvider {
        async fn send_json(&self, request: Value) -> Result<Value, CoreError> {
            let resp = self
                .client
                .post(&self.url)
                .json(&request)
                .send()
                .await
                .map_err(|e| CoreError::Network(e.to_string()))?;
            let v = resp.json::<Value>().await.map_err(|e| CoreError::Serialization(e.to_string()))?;
            Ok(v)
        }
    }
}

/// A tiny helper that calls the provider and returns the raw JSON value.
pub async fn fetch_with_provider(
    provider: &dyn RpcProvider,
    request: Value,
) -> Result<Value, CoreError> {
    provider.send_json(request).await
}

use std::sync::Arc;

// A global optional JSON-RPC provider used by high-level helpers
static GLOBAL_JSON_RPC_PROVIDER: Lazy<tokio::sync::Mutex<Option<Arc<dyn RpcProvider + Send + Sync>>>> = Lazy::new(|| tokio::sync::Mutex::new(None));

/// Set the global JSON-RPC provider instance (optional).
pub async fn set_global_json_rpc_provider(provider: Option<Arc<dyn RpcProvider + Send + Sync>>) {
    let mut lock = GLOBAL_JSON_RPC_PROVIDER.lock().await;
    *lock = provider;
}

use serde::de::DeserializeOwned;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicUsize, Ordering};
use reqwest;

/// Try to perform a JSON-RPC request using a provider if configured, otherwise use a local HTTP client.
pub async fn fetch_with_fallback<T: DeserializeOwned + Send + 'static>(
    request: Value,
    _method: &str,
    _rpc_client: &Arc<dyn crate::rpc_client::RpcClient>,
    settings: &crate::config::settings::Settings,
) -> Result<crate::core::models::RpcResponse<T>, Box<dyn std::error::Error + Send + Sync>> {
    static RPC_ROUND_ROBIN: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
    let urls = &settings.solana_rpc_urls;
    if urls.is_empty() {
        return Err("No solana_rpc_urls configured".into());
    }
    let provider_opt = {
        let lock = GLOBAL_JSON_RPC_PROVIDER.lock().await;
        lock.clone()
    };
    let client = reqwest::Client::new();
    let start = if settings.rotate_rpc { RPC_ROUND_ROBIN.fetch_add(1, Ordering::Relaxed) % urls.len() } else { 0 };
    for i in 0..urls.len() {
        let idx = (start + i) % urls.len();
        let http = &urls[idx];
        let request_body = request.clone();
        if let Some(ref provider) = provider_opt {
            match provider.send_json(request_body.clone()).await {
                Ok(resp_value) => {
                    match serde_json::from_value::<crate::core::models::RpcResponse<T>>(resp_value.clone()) {
                        Ok(parsed) => {
                            if parsed.error.is_some() { return Err(format!("RPC error from {}: {:?}", http, parsed.error).into()); }
                            return Ok(parsed);
                        }
                        Err(e) => { debug!("JSON parse error from {}: {} -- body: {}", http, e, serde_json::to_string(&resp_value).unwrap_or_else(|_| "<unserializable>".to_string())); continue; }
                    }
                }
                Err(e) => { debug!("Provider error contacting {}: {}", http, e); continue; }
            }
        }
        match client.post(http).json(&request_body).send().await {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))?;
                if !status.is_success() { debug!("HTTP {} from {}: {}", status, http, text); continue; }
                match serde_json::from_str::<crate::core::models::RpcResponse<T>>(&text) {
                    Ok(parsed) => { if parsed.error.is_some() { return Err(format!("RPC error from {}: {:?}", http, parsed.error).into()); } return Ok(parsed); }
                    Err(e) => { debug!("JSON parse error from {}: {} -- body: {}", http, e, text); continue; }
                }
            }
            Err(e) => { debug!("HTTP request failure {}: {}", http, e); continue; }
        }
    }
    Err("RPC endpoints unavailable or parse failed".into())
}

// Re-export helpers from rpc_helpers to maintain the older `crate::rpc::*` API surface
pub use crate::rpc_helpers::fetch_token_metadata;
pub use crate::rpc_helpers::fetch_current_price;
pub use crate::rpc_helpers::fetch_transaction_details;
pub use crate::rpc_helpers::find_curve_account_by_mint;
pub use crate::rpc_helpers::fetch_bonding_curve_state;
pub use crate::rpc_helpers::detect_idl_for_mint;
pub use crate::rpc_helpers::build_missing_ata_preinstructions;
pub use crate::rpc_helpers::fetch_global_fee_recipient;
pub use crate::rpc_helpers::fetch_bonding_curve_creator;
