// Native RPC client implementation wrapping solana_client::RpcClient

use crate::rpc_client::{RpcClient as RpcClientTrait, RpcResult};
use crate::error::CoreError;
use async_trait::async_trait;
use solana_client::rpc_client::RpcClient as SolanaRpcClient;
use serde_json::{json, Value};
use log::debug;
use base64::{Engine as _, engine::general_purpose::STANDARD as Base64Engine};
use std::sync::Arc;

/// Native RPC client wrapping solana_client::RpcClient
pub struct NativeRpcClient {
    client: Arc<SolanaRpcClient>,
}

impl NativeRpcClient {
    /// Create a new native RPC client
    pub fn new(endpoint: String) -> Self {
        Self {
            client: Arc::new(SolanaRpcClient::new(endpoint)),
        }
    }
    
    /// Create from existing Arc<RpcClient>
    pub fn from_arc(client: Arc<SolanaRpcClient>) -> Self {
        Self { client }
    }
    
    /// Get reference to underlying Solana RPC client
    pub fn inner(&self) -> &Arc<SolanaRpcClient> {
        &self.client
    }
}

#[async_trait(?Send)]
impl RpcClientTrait for NativeRpcClient {
    async fn get_latest_blockhash(&self) -> RpcResult<String> {
        debug!("Native RPC: get_latest_blockhash");
        
        let client = self.client.clone();
        let blockhash = tokio::task::spawn_blocking(move || {
            client.get_latest_blockhash()
        })
        .await
        .map_err(|e| CoreError::Rpc(format!("Task join error: {}", e)))?
        .map_err(|e| CoreError::Rpc(format!("get_latest_blockhash failed: {}", e)))?;
        
        Ok(blockhash.to_string())
    }
    
    async fn get_account_info(&self, pubkey: &str) -> RpcResult<Option<Value>> {
        debug!("Native RPC: get_account_info for {}", pubkey);
        
        use solana_sdk::pubkey::Pubkey;
        use std::str::FromStr;
        
        let pubkey = Pubkey::from_str(pubkey)
            .map_err(|e| CoreError::ParseError(format!("Invalid pubkey: {}", e)))?;
        
        let client = self.client.clone();
        let account = tokio::task::spawn_blocking(move || {
            client.get_account(&pubkey)
        })
        .await
        .map_err(|e| CoreError::Rpc(format!("Task join error: {}", e)))?;
        
        match account {
            Ok(acc) => {
                // Convert account to JSON format similar to RPC response
                let data_base64 = Base64Engine.encode(&acc.data);
                let account_json = json!({
                    "data": [data_base64, "base64"],
                    "executable": acc.executable,
                    "lamports": acc.lamports,
                    "owner": acc.owner.to_string(),
                    "rentEpoch": acc.rent_epoch,
                });
                Ok(Some(account_json))
            }
            Err(_) => Ok(None),
        }
    }
    
    async fn get_transaction(&self, signature: &str) -> RpcResult<Option<Value>> {
        debug!("Native RPC: get_transaction for {}", signature);
        
        use solana_sdk::signature::Signature;
        use std::str::FromStr;
        use solana_client::rpc_config::RpcTransactionConfig;
        use solana_sdk::commitment_config::CommitmentConfig;
        use solana_transaction_status::UiTransactionEncoding;
        
        let signature = Signature::from_str(signature)
            .map_err(|e| CoreError::ParseError(format!("Invalid signature: {}", e)))?;
        
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::JsonParsed),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };
        
        let client = self.client.clone();
        let tx = tokio::task::spawn_blocking(move || {
            client.get_transaction_with_config(&signature, config)
        })
        .await
        .map_err(|e| CoreError::Rpc(format!("Task join error: {}", e)))?;
        
        match tx {
            Ok(tx_with_status) => {
                // Serialize to JSON
                let json = serde_json::to_value(tx_with_status)
                    .map_err(|e| CoreError::Json(e))?;
                Ok(Some(json))
            }
            Err(_) => Ok(None),
        }
    }
    
    async fn send_transaction(&self, transaction: &[u8]) -> RpcResult<String> {
        debug!("Native RPC: send_transaction");
        
        use solana_sdk::transaction::Transaction;
        use bincode;
        
        let tx: Transaction = bincode::deserialize(transaction)
            .map_err(|e| CoreError::ParseError(format!("Failed to deserialize transaction: {}", e)))?;
        
        let client = self.client.clone();
        let signature = tokio::task::spawn_blocking(move || {
            client.send_and_confirm_transaction(&tx)
        })
        .await
        .map_err(|e| CoreError::Rpc(format!("Task join error: {}", e)))?
        .map_err(|e| CoreError::Transaction(format!("send_transaction failed: {}", e)))?;
        
        Ok(signature.to_string())
    }
    
    async fn get_token_account_balance(&self, pubkey: &str) -> RpcResult<u64> {
        debug!("Native RPC: get_token_account_balance for {}", pubkey);
        
        use solana_sdk::pubkey::Pubkey;
        use std::str::FromStr;
        
        let pubkey = Pubkey::from_str(pubkey)
            .map_err(|e| CoreError::ParseError(format!("Invalid pubkey: {}", e)))?;
        
        let client = self.client.clone();
        let balance = tokio::task::spawn_blocking(move || {
            client.get_token_account_balance(&pubkey)
        })
        .await
        .map_err(|e| CoreError::Rpc(format!("Task join error: {}", e)))?
        .map_err(|e| CoreError::Rpc(format!("get_token_account_balance failed: {}", e)))?;
        
        balance.amount.parse::<u64>()
            .map_err(|e| CoreError::ParseError(format!("Failed to parse token balance: {}", e)))
    }
    
    async fn get_multiple_accounts(&self, pubkeys: &[String]) -> RpcResult<Vec<Option<Value>>> {
        debug!("Native RPC: get_multiple_accounts for {} keys", pubkeys.len());
        
        use solana_sdk::pubkey::Pubkey;
        use std::str::FromStr;
        
        let pubkeys_parsed: Result<Vec<Pubkey>, _> = pubkeys.iter()
            .map(|s| Pubkey::from_str(s))
            .collect();
        
        let pubkeys_parsed = pubkeys_parsed
            .map_err(|e| CoreError::ParseError(format!("Invalid pubkey: {}", e)))?;
        
        let client = self.client.clone();
        let accounts = tokio::task::spawn_blocking(move || {
            client.get_multiple_accounts(&pubkeys_parsed)
        })
        .await
        .map_err(|e| CoreError::Rpc(format!("Task join error: {}", e)))?
        .map_err(|e| CoreError::Rpc(format!("get_multiple_accounts failed: {}", e)))?;
        
        let mut result = Vec::new();
        for account_opt in accounts {
            if let Some(acc) = account_opt {
                let data_base64 = Base64Engine.encode(&acc.data);
                let account_json = json!({
                    "data": [data_base64, "base64"],
                    "executable": acc.executable,
                    "lamports": acc.lamports,
                    "owner": acc.owner.to_string(),
                    "rentEpoch": acc.rent_epoch,
                });
                result.push(Some(account_json));
            } else {
                result.push(None);
            }
        }
        
        Ok(result)
    }
    
    async fn simulate_transaction(&self, transaction: &[u8]) -> RpcResult<Value> {
        debug!("Native RPC: simulate_transaction");
        
        use solana_sdk::transaction::Transaction;
        use bincode;
        
        let tx: Transaction = bincode::deserialize(transaction)
            .map_err(|e| CoreError::ParseError(format!("Failed to deserialize transaction: {}", e)))?;
        
        let client = self.client.clone();
        let result = tokio::task::spawn_blocking(move || {
            client.simulate_transaction(&tx)
        })
        .await
        .map_err(|e| CoreError::Rpc(format!("Task join error: {}", e)))?
        .map_err(|e| CoreError::Transaction(format!("simulate_transaction failed: {}", e)))?;
        
        serde_json::to_value(result)
            .map_err(|e| CoreError::Json(e))
    }
    
    async fn get_program_accounts(&self, program_id: &str, _filters: Option<Value>) -> RpcResult<Vec<Value>> {
        debug!("Native RPC: get_program_accounts for {}", program_id);
        
        use solana_sdk::pubkey::Pubkey;
        use std::str::FromStr;
        
        let program_id = Pubkey::from_str(program_id)
            .map_err(|e| CoreError::ParseError(format!("Invalid program_id: {}", e)))?;
        
        let client = self.client.clone();
        let accounts = tokio::task::spawn_blocking(move || {
            client.get_program_accounts(&program_id)
        })
        .await
        .map_err(|e| CoreError::Rpc(format!("Task join error: {}", e)))?
        .map_err(|e| CoreError::Rpc(format!("get_program_accounts failed: {}", e)))?;
        
        let mut result = Vec::new();
        for (pubkey, account) in accounts {
            let data_base64 = Base64Engine.encode(&account.data);
            let account_json = json!({
                "pubkey": pubkey.to_string(),
                "account": {
                    "data": [data_base64, "base64"],
                    "executable": account.executable,
                    "lamports": account.lamports,
                    "owner": account.owner.to_string(),
                    "rentEpoch": account.rent_epoch,
                }
            });
            result.push(account_json);
        }
        
        Ok(result)
    }
}
