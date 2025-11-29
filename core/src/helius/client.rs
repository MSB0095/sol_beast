use log::debug;
use serde_json::json;
use serde_json::Value;

#[cfg(not(target_arch = "wasm32"))]
pub struct HeliusClient {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

#[cfg(not(target_arch = "wasm32"))]
impl HeliusClient {
    pub fn new(base_url: &str, api_key: Option<String>) -> Self {
        let client = reqwest::Client::new();
        Self { base_url: base_url.to_string(), api_key, client }
    }

    /// Send a base64-encoded transaction using Helius Sender endpoint
    pub async fn send_transaction(&self, base64_tx: &str, skip_preflight: bool, max_retries: u32) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut url = self.base_url.clone();
        if let Some(key) = &self.api_key {
            // Append api key safely (works even if query params exist)
            if url.contains('?') {
                url = format!("{}&api-key={}", url, key);
            } else {
                url = format!("{}?api-key={}", url, key);
            }
        }

        let params = json!([
            base64_tx,
            {
                "encoding": "base64",
                "skipPreflight": skip_preflight,
                "maxRetries": max_retries
            }
        ]);

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": params,
        });

        debug!("Helius sendTransaction to {}", url);

        let res = self.client.post(&url)
            .json(&body)
            .send()
            .await?;

        let text = res.text().await?;
        let v: Value = serde_json::from_str(&text)?;
        if let Some(result) = v.get("result") {
            if let Some(sig) = result.as_str() {
                return Ok(sig.to_string());
            }
        }
        if let Some(err) = v.get("error") {
            return Err(format!("Helius sendTransaction error: {}", err).into());
        }

        Err("Helius sendTransaction unexpected response".into())
    }

    /// Get a transaction (parsed) via RPC endpoint
    pub async fn get_transaction(&self, signature: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let mut url = self.base_url.clone();
        if let Some(key) = &self.api_key {
            if url.contains('?') {
                url = format!("{}&api-key={}", url, key);
            } else {
                url = format!("{}?api-key={}", url, key);
            }
        }

        let params = json!([signature, {"encoding":"jsonParsed"}]);
        let body = json!({"jsonrpc":"2.0","id":1,"method":"getTransaction","params": params});

        let res = self.client.post(&url)
            .json(&body)
            .send()
            .await?;
        let text = res.text().await?;
        let v: Value = serde_json::from_str(&text)?;
        Ok(v)
    }
}
