// WASM RPC Client using browser fetch API
use crate::rpc_client::{RpcClient, RpcResult};
use crate::error::CoreError;
use async_trait::async_trait;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use log::debug;

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

pub struct WasmRpcClient {
    endpoint: String,
    request_id: std::cell::RefCell<u64>,
}

impl WasmRpcClient {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            request_id: std::cell::RefCell::new(1),
        }
    }

    fn next_id(&self) -> u64 {
        let mut id = self.request_id.borrow_mut();
        *id += 1;
        *id
    }

    pub async fn call<T>(&self, method: &str, params: serde_json::Value) -> Result<T, JsValue>
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

        let req = Request::new_with_str_and_init(&self.endpoint, &opts)?;
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
            .map(|arr| arr.clone())
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
            .map(|arr| arr.clone())
    }
}
