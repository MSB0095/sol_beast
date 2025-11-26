use crate::core::error::CoreError;
use crate::models::BondingCurveState;

/// RPC client trait for both native and WASM implementations
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait RpcClient: Send + Sync {
    async fn get_account_info(&self, pubkey: &str) -> Result<Option<Vec<u8>>, CoreError>;
    async fn get_balance(&self, pubkey: &str) -> Result<u64, CoreError>;
    async fn send_transaction(&self, transaction: &[u8]) -> Result<String, CoreError>;
    async fn confirm_transaction(&self, signature: &str) -> Result<bool, CoreError>;

    // Return the latest blockhash for transaction signing
    async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash, CoreError>;

    // Simulate a transaction with a config. For wasm builds we accept JSON config;
    // native implementations may translate this into RpcSimulateTransactionConfig.
    async fn simulate_transaction_with_config(&self, tx: &solana_sdk::transaction::Transaction, config: serde_json::Value) -> Result<serde_json::Value, CoreError>;

    // Send and confirm a transaction (blocking call); returns signature string
    async fn send_and_confirm_transaction(&self, tx: &solana_sdk::transaction::Transaction) -> Result<String, CoreError>;
}

/// Parse bonding curve account data
pub fn parse_bonding_curve(data: &[u8]) -> Result<BondingCurveState, CoreError> {
    // Strip 8-byte discriminator
    if data.len() < 8 {
        return Err(CoreError::Parse("Account data too short".to_string()));
    }
    
    let data = &data[8..];
    
    if data.len() < 41 {
        return Err(CoreError::Parse(format!(
            "Bonding curve data too short: {} bytes",
            data.len()
        )));
    }
    
    // Parse fields manually
    let virtual_token_reserves = u64::from_le_bytes(
        data[0..8].try_into().map_err(|_| CoreError::Parse("Failed to parse virtual_token_reserves".to_string()))?
    );
    let virtual_sol_reserves = u64::from_le_bytes(
        data[8..16].try_into().map_err(|_| CoreError::Parse("Failed to parse virtual_sol_reserves".to_string()))?
    );
    let real_token_reserves = u64::from_le_bytes(
        data[16..24].try_into().map_err(|_| CoreError::Parse("Failed to parse real_token_reserves".to_string()))?
    );
    let real_sol_reserves = u64::from_le_bytes(
        data[24..32].try_into().map_err(|_| CoreError::Parse("Failed to parse real_sol_reserves".to_string()))?
    );
    let token_total_supply = u64::from_le_bytes(
        data[32..40].try_into().map_err(|_| CoreError::Parse("Failed to parse token_total_supply".to_string()))?
    );
    let complete = data[40] != 0;
    
    let creator = if data.len() >= 73 {
        // Creator is 32 bytes starting at index 41
        let creator_bytes = &data[41..73];
        Some(bs58::encode(creator_bytes).into_string())
    } else {
        None
    };
    
    Ok(BondingCurveState {
        virtual_token_reserves,
        virtual_sol_reserves,
        real_token_reserves,
        real_sol_reserves,
        token_total_supply,
        complete,
        creator,
    })
}

#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
pub mod native {
    use super::*;
    use solana_client::rpc_client::RpcClient as SolanaRpcClient;
    use solana_sdk::commitment_config::CommitmentConfig;
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    pub struct NativeRpcClient {
        client: SolanaRpcClient,
    }

    impl NativeRpcClient {
        pub fn new(url: String) -> Self {
            Self {
                client: SolanaRpcClient::new_with_commitment(url, CommitmentConfig::confirmed()),
            }
        }
    }

    #[async_trait::async_trait]
    impl RpcClient for NativeRpcClient {
        async fn get_account_info(&self, pubkey: &str) -> Result<Option<Vec<u8>>, CoreError> {
            let pubkey = Pubkey::from_str(pubkey)?;
            match self.client.get_account(&pubkey) {
                Ok(account) => Ok(Some(account.data)),
                Err(e) => {
                    if e.to_string().contains("AccountNotFound") {
                        Ok(None)
                    } else {
                        Err(CoreError::Rpc(e.to_string()))
                    }
                }
            }
        }

        async fn get_balance(&self, pubkey: &str) -> Result<u64, CoreError> {
            let pubkey = Pubkey::from_str(pubkey)?;
            self.client.get_balance(&pubkey).map_err(CoreError::from)
        }

        async fn send_transaction(&self, _transaction: &[u8]) -> Result<String, CoreError> {
            // Implementation would involve deserializing and sending the transaction
            Err(CoreError::Internal("Not implemented".to_string()))
        }

        async fn confirm_transaction(&self, _signature: &str) -> Result<bool, CoreError> {
            // Implementation would check transaction confirmation
            Err(CoreError::Internal("Not implemented".to_string()))
        }

        async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash, CoreError> {
            Ok(self.client.get_latest_blockhash()?)
        }

        async fn simulate_transaction_with_config(&self, tx: &solana_sdk::transaction::Transaction, _config: serde_json::Value) -> Result<serde_json::Value, CoreError> {
            let config = solana_client::rpc_config::RpcSimulateTransactionConfig { 
                sig_verify: false, 
                replace_recent_blockhash: true, 
                commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
                encoding: None,
                accounts: None,
                min_context_slot: None,
                inner_instructions: false,
            };
            let result = self.client.simulate_transaction_with_config(tx, config).map_err(CoreError::from)?;
            Ok(serde_json::to_value(&result).map_err(|e| CoreError::Serialization(e.to_string()))?)
        }

        async fn send_and_confirm_transaction(&self, tx: &solana_sdk::transaction::Transaction) -> Result<String, CoreError> {
            let sig = self.client.send_and_confirm_transaction(tx).map_err(CoreError::from)?;
            Ok(sig.to_string())
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::*;
    use wasm_bindgen::JsValue;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, Response};
    use serde_json::Value;

    pub struct WasmRpcClient {
        url: String,
    }

    impl WasmRpcClient {
        pub fn new(url: String) -> Self {
            Self { url }
        }

        async fn rpc_call(&self, method: &str, params: Value) -> Result<Value, CoreError> {
            let window = web_sys::window()
                .ok_or_else(|| CoreError::Network("No window object".to_string()))?;

            let request_body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params,
            });

            let opts = RequestInit::new();
            opts.set_method("POST");
            let body_js = JsValue::from_str(&request_body.to_string());
            opts.set_body(&body_js);

            let request = Request::new_with_str_and_init(&self.url, &opts)
                .map_err(|_| CoreError::Network("Failed to create request".to_string()))?;

            request
                .headers()
                .set("Content-Type", "application/json")
                .map_err(|_| CoreError::Network("Failed to set headers".to_string()))?;

            let resp_value = JsFuture::from(window.fetch_with_request(&request))
                .await
                .map_err(|_| CoreError::Network("Fetch failed".to_string()))?;

            let resp: Response = resp_value.dyn_into()
                .map_err(|_| CoreError::Network("Invalid response".to_string()))?;

            let json = JsFuture::from(resp.json()
                .map_err(|_| CoreError::Network("Failed to parse JSON".to_string()))?)
                .await
                .map_err(|_| CoreError::Network("Failed to get JSON".to_string()))?;

            let json_str = js_sys::JSON::stringify(&json)
                .map_err(|_| CoreError::Network("Failed to stringify".to_string()))?;
            
            let json_string = json_str.as_string()
                .ok_or_else(|| CoreError::Network("Failed to convert JSON to string".to_string()))?;
            
            let value: serde_json::Value = serde_json::from_str(&json_string)
                .map_err(|e| CoreError::Serialization(e.to_string()))?;

            Ok(value)
        }
    }

    #[async_trait::async_trait(?Send)]
    impl RpcClient for WasmRpcClient {
        async fn get_account_info(&self, pubkey: &str) -> Result<Option<Vec<u8>>, CoreError> {
            let result = self.rpc_call("getAccountInfo", serde_json::json!([
                pubkey,
                {"encoding": "base64"}
            ])).await?;

            if let Some(value) = result.get("result").and_then(|r| r.get("value")) {
                if value.is_null() {
                    return Ok(None);
                }
                
                if let Some(data) = value.get("data").and_then(|d| d.get(0)).and_then(|s| s.as_str()) {
                    use base64::Engine;
                    let decoded = base64::engine::general_purpose::STANDARD.decode(data)
                        .map_err(|e| CoreError::Parse(e.to_string()))?;
                    return Ok(Some(decoded));
                }
            }

            Ok(None)
        }

        async fn get_balance(&self, pubkey: &str) -> Result<u64, CoreError> {
            let result = self.rpc_call("getBalance", serde_json::json!([pubkey])).await?;
            
            result.get("result")
                .and_then(|r| r.get("value"))
                .and_then(|v| v.as_u64())
                .ok_or_else(|| CoreError::Rpc("Failed to parse balance".to_string()))
        }

        async fn send_transaction(&self, _transaction: &[u8]) -> Result<String, CoreError> {
            Err(CoreError::Internal("Not implemented".to_string()))
        }

        async fn confirm_transaction(&self, _signature: &str) -> Result<bool, CoreError> {
            Err(CoreError::Internal("Not implemented".to_string()))
        }

        async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash, CoreError> {
            let res = self.rpc_call("getLatestBlockhash", serde_json::json!([])).await?;
            if let Some(b) = res.get("result").and_then(|r| r.get("value")).and_then(|v| v.get("blockhash")).and_then(|s| s.as_str()) {
                Ok(solana_sdk::hash::Hash::from_str(b).map_err(|e| CoreError::Parse(e.to_string()))?)
            } else {
                Err(CoreError::Rpc("Failed to parse getLatestBlockhash response".to_string()))
            }
        }

        async fn simulate_transaction_with_config(&self, _tx: &solana_sdk::transaction::Transaction, _config: serde_json::Value) -> Result<serde_json::Value, CoreError> {
            Err(CoreError::Internal("simulate_transaction not implemented for wasm".to_string()))
        }

        async fn send_and_confirm_transaction(&self, _tx: &solana_sdk::transaction::Transaction) -> Result<String, CoreError> {
            Err(CoreError::Internal("send_and_confirm_transaction not implemented for wasm".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bonding_curve() {
        // Create mock data with discriminator + fields (need at least 73 bytes for creator)
        let mut data = vec![0u8; 80];
        
        // Discriminator (8 bytes) - will be stripped
        data[0..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        
        // After discriminator (starting at index 8):
        // virtual_token_reserves
        data[8..16].copy_from_slice(&1000u64.to_le_bytes());
        // virtual_sol_reserves
        data[16..24].copy_from_slice(&2000u64.to_le_bytes());
        // real_token_reserves
        data[24..32].copy_from_slice(&3000u64.to_le_bytes());
        // real_sol_reserves
        data[32..40].copy_from_slice(&4000u64.to_le_bytes());
        // token_total_supply
        data[40..48].copy_from_slice(&5000u64.to_le_bytes());
        // complete (1 byte)
        data[48] = 1;
        // creator (32 bytes starting at 49)
        // Leave as zeros for test
        
        let result = parse_bonding_curve(&data).unwrap();
        
        assert_eq!(result.virtual_token_reserves, 1000);
        assert_eq!(result.virtual_sol_reserves, 2000);
        assert_eq!(result.complete, true);
    }
}
