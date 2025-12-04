// WASM RPC Client using browser fetch API with WebSocket fallback for CORS
use crate::rpc_client::{RpcClient, RpcResult};
use crate::error::CoreError;
use async_trait::async_trait;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response, WebSocket, MessageEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use log::debug;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcResponse<T> {
    jsonrpc: String,
    id: u64,
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcError {
    code: i64,
    message: String,
}

/// Pending RPC request awaiting response
struct PendingRequest {
    sender: Arc<Mutex<Option<Result<Value, String>>>>,
}

pub struct WasmRpcClient {
    http_endpoint: String,
    ws_endpoint: Option<String>,
    use_websocket: std::cell::RefCell<bool>,
    websocket: Arc<Mutex<Option<WebSocket>>>,
    pending_requests: Arc<Mutex<HashMap<u64, PendingRequest>>>,
    request_id: std::cell::RefCell<u64>,
}

impl WasmRpcClient {
    pub fn new(endpoint: String) -> Self {
        // Convert HTTP endpoint to WebSocket endpoint
        let ws_endpoint = Self::http_to_ws_endpoint(&endpoint);
        
        Self {
            http_endpoint: endpoint,
            ws_endpoint,
            use_websocket: std::cell::RefCell::new(false),
            websocket: Arc::new(Mutex::new(None)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            request_id: std::cell::RefCell::new(1),
        }
    }

    fn next_id(&self) -> u64 {
        let mut id = self.request_id.borrow_mut();
        *id += 1;
        *id
    }

    /// Convert HTTP(S) RPC endpoint to WebSocket (WS/WSS) endpoint
    /// This allows using the same endpoint for both HTTP and WebSocket
    fn http_to_ws_endpoint(http_url: &str) -> Option<String> {
        if http_url.starts_with("https://") {
            Some(http_url.replace("https://", "wss://"))
        } else if http_url.starts_with("http://") {
            Some(http_url.replace("http://", "ws://"))
        } else {
            None
        }
    }

    /// Check if an error is a CORS-related error
    fn is_cors_error(error: &JsValue) -> bool {
        if let Some(error_str) = error.as_string() {
            let lower = error_str.to_lowercase();
            return lower.contains("cors") 
                || lower.contains("network error")
                || lower.contains("failed to fetch")
                || lower.contains("networkerror");
        }
        false
    }

    /// Initialize WebSocket connection for RPC calls
    /// This is called automatically when CORS errors are detected
    async fn init_websocket(&self) -> Result<(), JsValue> {
        let ws_url = self.ws_endpoint.as_ref()
            .ok_or_else(|| JsValue::from_str("No WebSocket endpoint available"))?;

        debug!("Initializing WebSocket connection to: {}", ws_url);
        web_sys::console::log_1(&JsValue::from_str(&format!("🔌 Connecting to RPC via WebSocket: {}", ws_url)));

        let ws = WebSocket::new(ws_url)?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let pending_requests = Arc::clone(&self.pending_requests);

        // Message handler - process RPC responses
        let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let message: String = txt.into();
                
                // Parse the response
                if let Ok(parsed) = serde_json::from_str::<Value>(&message) {
                    if let Some(id) = parsed.get("id").and_then(|v| v.as_u64()) {
                        // This is an RPC response, match it with pending request
                        if let Ok(mut pending) = pending_requests.lock() {
                            if let Some(req) = pending.remove(&id) {
                                if let Ok(mut sender) = req.sender.lock() {
                                    *sender = Some(Ok(parsed));
                                }
                            }
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);

        let pending_requests_err = Arc::clone(&self.pending_requests);
        let on_error = Closure::wrap(Box::new(move |e: web_sys::ErrorEvent| {
            web_sys::console::error_1(&JsValue::from_str(&format!("WebSocket error: {:?}", e)));
            // Fail all pending requests
            if let Ok(mut pending) = pending_requests_err.lock() {
                for (_, req) in pending.drain() {
                    if let Ok(mut sender) = req.sender.lock() {
                        *sender = Some(Err("WebSocket connection error".to_string()));
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);

        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        // Store the closures to keep them alive
        on_message.forget();
        on_error.forget();

        // Wait for connection to open
        let ws_clone = ws.clone();
        let open_promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let onopen = Closure::once(move || {
                resolve.call0(&JsValue::NULL).unwrap();
            });
            ws_clone.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            onopen.forget();
        });

        JsFuture::from(open_promise).await?;

        // Store the WebSocket
        if let Ok(mut ws_lock) = self.websocket.lock() {
            *ws_lock = Some(ws);
        }

        web_sys::console::log_1(&JsValue::from_str("✅ WebSocket RPC connection established"));
        Ok(())
    }

    pub async fn call<T>(&self, method: &str, params: serde_json::Value) -> Result<T, JsValue>
    where
        T: for<'de> Deserialize<'de>,
    {
        // Try HTTP first if we haven't switched to WebSocket
        if !*self.use_websocket.borrow() {
            let result = self.try_http_call(method, params.clone()).await;
            
            // If CORS error detected, switch to WebSocket for all future requests
            if let Err(ref err) = result {
                if Self::is_cors_error(err) {
                    debug!("CORS error detected, switching to WebSocket RPC...");
                    web_sys::console::log_1(&JsValue::from_str("⚠️ CORS error detected. Switching to WebSocket RPC (no CORS restrictions)..."));
                    
                    // Enable WebSocket mode
                    *self.use_websocket.borrow_mut() = true;
                    
                    // Initialize WebSocket connection
                    self.init_websocket().await?;
                    
                    // Retry via WebSocket
                    return self.try_websocket_call(method, params).await;
                }
            }
            
            return result;
        }
        
        // Use WebSocket for the call
        self.try_websocket_call(method, params).await
    }

    /// Try making an RPC call via HTTP
    async fn try_http_call<T>(&self, method: &str, params: Value) -> Result<T, JsValue>
    where
        T: for<'de> Deserialize<'de>,
    {
        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id(),
            method: method.to_string(),
            params,
        };

        let body = serde_json::to_string(&request)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;

        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);
        opts.set_body(&JsValue::from_str(&body));

        let req = Request::new_with_str_and_init(&self.http_endpoint, &opts)?;
        req.headers().set("Content-Type", "application/json")?;

        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
        let resp_value = JsFuture::from(window.fetch_with_request(&req)).await?;
        let resp: Response = resp_value.dyn_into()?;

        let json = JsFuture::from(resp.json()?).await?;
        let response: RpcResponse<T> = serde_wasm_bindgen::from_value(json)
            .map_err(|e| JsValue::from_str(&format!("Deserialization error: {:?}", e)))?;

        if let Some(error) = response.error {
            return Err(JsValue::from_str(&format!(
                "RPC error {}: {}",
                error.code, error.message
            )));
        }

        response
            .result
            .ok_or_else(|| JsValue::from_str("No result in response"))
    }

    /// Try making an RPC call via WebSocket
    async fn try_websocket_call<T>(&self, method: &str, params: Value) -> Result<T, JsValue>
    where
        T: for<'de> Deserialize<'de>,
    {
        // Ensure WebSocket is connected
        let ws = {
            let ws_lock = self.websocket.lock()
                .map_err(|e| JsValue::from_str(&format!("Failed to lock websocket: {:?}", e)))?;
            ws_lock.as_ref()
                .ok_or_else(|| JsValue::from_str("WebSocket not initialized"))?
                .clone()
        };

        let id = self.next_id();
        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        // Create pending request
        let pending = PendingRequest {
            sender: Arc::new(Mutex::new(None)),
        };
        let sender_clone = Arc::clone(&pending.sender);

        // Register pending request
        {
            let mut pending_reqs = self.pending_requests.lock()
                .map_err(|e| JsValue::from_str(&format!("Failed to lock pending requests: {:?}", e)))?;
            pending_reqs.insert(id, pending);
        }

        // Send the request
        let body = serde_json::to_string(&request)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;
        ws.send_with_str(&body)?;

        // Wait for response (with timeout)
        let timeout_ms = 30000; // 30 seconds
        let start = js_sys::Date::now();

        loop {
            // Check if response arrived
            {
                let sender_lock = sender_clone.lock()
                    .map_err(|e| JsValue::from_str(&format!("Failed to lock sender: {:?}", e)))?;
                
                if let Some(result) = sender_lock.as_ref() {
                    match result {
                        Ok(response_value) => {
                            // Parse the response
                            let response: RpcResponse<T> = serde_json::from_value(response_value.clone())
                                .map_err(|e| JsValue::from_str(&format!("Failed to parse response: {}", e)))?;

                            if let Some(error) = response.error {
                                return Err(JsValue::from_str(&format!(
                                    "RPC error {}: {}",
                                    error.code, error.message
                                )));
                            }

                            return response.result
                                .ok_or_else(|| JsValue::from_str("No result in response"));
                        }
                        Err(e) => {
                            return Err(JsValue::from_str(e));
                        }
                    }
                }
            }

            // Check timeout
            if js_sys::Date::now() - start > timeout_ms as f64 {
                // Remove pending request
                let mut pending_reqs = self.pending_requests.lock()
                    .map_err(|e| JsValue::from_str(&format!("Failed to lock pending requests: {:?}", e)))?;
                pending_reqs.remove(&id);
                return Err(JsValue::from_str("RPC request timeout"));
            }

            // Small delay before next check
            let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                let window = web_sys::window().unwrap();
                let closure = Closure::once(move || {
                    resolve.call0(&JsValue::NULL).unwrap();
                });
                window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    closure.as_ref().unchecked_ref(),
                    10 // 10ms delay
                ).unwrap();
                closure.forget();
            });
            JsFuture::from(promise).await?;
        }
    }

    // Common Solana RPC methods
    pub async fn get_balance(&self, pubkey: &str) -> Result<u64, JsValue> {
        let params = json!([pubkey]);
        let result: serde_json::Value = self.call("getBalance", params).await?;
        
        result["value"]
            .as_u64()
            .ok_or_else(|| JsValue::from_str("Invalid balance response"))
    }

    pub async fn get_latest_blockhash(&self) -> Result<String, JsValue> {
        let params = json!([]);
        let result: serde_json::Value = self.call("getLatestBlockhash", params).await?;
        
        result["value"]["blockhash"]
            .as_str()
            .ok_or_else(|| JsValue::from_str("Invalid blockhash response"))
            .map(|s| s.to_string())
    }

    pub async fn send_transaction(&self, signed_tx: &str) -> Result<String, JsValue> {
        let params = json!([
            signed_tx,
            {
                "encoding": "base64",
                "skipPreflight": false,
                "preflightCommitment": "confirmed"
            }
        ]);
        
        self.call("sendTransaction", params).await
    }

    pub async fn get_account_info(&self, pubkey: &str) -> Result<Option<serde_json::Value>, JsValue> {
        let params = json!([pubkey, { "encoding": "jsonParsed" }]);
        let result: serde_json::Value = self.call("getAccountInfo", params).await?;
        
        Ok(result.get("value").cloned())
    }

    pub async fn get_token_accounts_by_owner(
        &self,
        owner: &str,
        mint: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, JsValue> {
        let filter = if let Some(mint_addr) = mint {
            json!({ "mint": mint_addr })
        } else {
            json!({ "programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" })
        };

        let params = json!([owner, filter, { "encoding": "jsonParsed" }]);
        let result: serde_json::Value = self.call("getTokenAccountsByOwner", params).await?;
        
        result["value"]
            .as_array()
            .ok_or_else(|| JsValue::from_str("Invalid token accounts response"))
            .cloned()
    }
}

// Implement the RpcClient trait for WasmRpcClient
#[async_trait(?Send)]
impl RpcClient for WasmRpcClient {
    async fn get_latest_blockhash(&self) -> RpcResult<String> {
        debug!("WASM RPC: get_latest_blockhash");
        let params = json!([]);
        let result: Value = self.call("getLatestBlockhash", params)
            .await
            .map_err(|e| CoreError::Rpc(format!("getLatestBlockhash failed: {:?}", e)))?;
        
        result["value"]["blockhash"]
            .as_str()
            .ok_or_else(|| CoreError::ParseError("Invalid blockhash response".to_string()))
            .map(|s| s.to_string())
    }
    
    async fn get_account_info(&self, pubkey: &str) -> RpcResult<Option<Value>> {
        debug!("WASM RPC: get_account_info for {}", pubkey);
        let params = json!([pubkey, { "encoding": "base64", "commitment": "confirmed" }]);
        let result: Value = self.call("getAccountInfo", params)
            .await
            .map_err(|e| CoreError::Rpc(format!("getAccountInfo failed: {:?}", e)))?;
        
        Ok(result.get("value").cloned())
    }
    
    async fn get_transaction(&self, signature: &str) -> RpcResult<Option<Value>> {
        debug!("WASM RPC: get_transaction for {}", signature);
        let params = json!([
            signature,
            {
                "encoding": "jsonParsed",
                "commitment": "confirmed",
                "maxSupportedTransactionVersion": 0
            }
        ]);
        
        let result: Value = self.call("getTransaction", params)
            .await
            .map_err(|e| CoreError::Rpc(format!("getTransaction failed: {:?}", e)))?;
        
        // Transaction might not exist yet, return None if null
        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }
    
    async fn send_transaction(&self, transaction: &[u8]) -> RpcResult<String> {
        debug!("WASM RPC: send_transaction");
        
        // Encode transaction as base64
        use base64::{Engine as _, engine::general_purpose::STANDARD as Base64Engine};
        let tx_base64 = Base64Engine.encode(transaction);
        
        let params = json!([
            tx_base64,
            {
                "encoding": "base64",
                "skipPreflight": false,
                "preflightCommitment": "confirmed"
            }
        ]);
        
        self.call("sendTransaction", params)
            .await
            .map_err(|e| CoreError::Transaction(format!("sendTransaction failed: {:?}", e)))
    }
    
    async fn get_token_account_balance(&self, pubkey: &str) -> RpcResult<u64> {
        debug!("WASM RPC: get_token_account_balance for {}", pubkey);
        let params = json!([pubkey]);
        let result: Value = self.call("getTokenAccountBalance", params)
            .await
            .map_err(|e| CoreError::Rpc(format!("getTokenAccountBalance failed: {:?}", e)))?;
        
        // Parse amount from response
        result["value"]["amount"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| CoreError::ParseError("Invalid token balance response".to_string()))
    }
    
    async fn get_multiple_accounts(&self, pubkeys: &[String]) -> RpcResult<Vec<Option<Value>>> {
        debug!("WASM RPC: get_multiple_accounts for {} keys", pubkeys.len());
        let params = json!([pubkeys, { "encoding": "base64", "commitment": "confirmed" }]);
        let result: Value = self.call("getMultipleAccounts", params)
            .await
            .map_err(|e| CoreError::Rpc(format!("getMultipleAccounts failed: {:?}", e)))?;
        
        let accounts_array = result["value"]
            .as_array()
            .ok_or_else(|| CoreError::ParseError("Invalid multiple accounts response".to_string()))?;
        
        let mut accounts = Vec::new();
        for account in accounts_array {
            if account.is_null() {
                accounts.push(None);
            } else {
                accounts.push(Some(account.clone()));
            }
        }
        
        Ok(accounts)
    }
    
    async fn simulate_transaction(&self, transaction: &[u8]) -> RpcResult<Value> {
        debug!("WASM RPC: simulate_transaction");
        
        // Encode transaction as base64
        use base64::{Engine as _, engine::general_purpose::STANDARD as Base64Engine};
        let tx_base64 = Base64Engine.encode(transaction);
        
        let params = json!([
            tx_base64,
            {
                "encoding": "base64",
                "commitment": "confirmed"
            }
        ]);
        
        self.call("simulateTransaction", params)
            .await
            .map_err(|e| CoreError::Transaction(format!("simulateTransaction failed: {:?}", e)))
    }
    
    async fn get_program_accounts(&self, program_id: &str, filters: Option<Value>) -> RpcResult<Vec<Value>> {
        debug!("WASM RPC: get_program_accounts for {}", program_id);
        
        let mut config = serde_json::json!({
            "encoding": "base64",
            "commitment": "confirmed"
        });
        
        if let Some(f) = filters {
            config["filters"] = f;
        }
        
        let params = json!([program_id, config]);
        let result: Value = self.call("getProgramAccounts", params)
            .await
            .map_err(|e| CoreError::Rpc(format!("getProgramAccounts failed: {:?}", e)))?;
        
        result.as_array()
            .ok_or_else(|| CoreError::ParseError("Invalid program accounts response".to_string()))
            .cloned()
    }
}
