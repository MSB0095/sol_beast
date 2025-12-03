// RPC Client abstraction - allows both native and WASM implementations

use crate::error::CoreError;
use crate::models::BondingCurveState;
use async_trait::async_trait;
use serde_json::Value;
use base64::{Engine as _, engine::general_purpose::STANDARD as Base64Engine};

/// Result type for RPC operations
pub type RpcResult<T> = Result<T, CoreError>;

/// Abstract RPC client trait that can be implemented for both native and WASM
#[async_trait(?Send)]
pub trait RpcClient {
    /// Get latest blockhash
    async fn get_latest_blockhash(&self) -> RpcResult<String>;
    
    /// Get account info
    async fn get_account_info(&self, pubkey: &str) -> RpcResult<Option<Value>>;
    
    /// Get transaction details
    async fn get_transaction(&self, signature: &str) -> RpcResult<Option<Value>>;
    
    /// Send transaction
    async fn send_transaction(&self, transaction: &[u8]) -> RpcResult<String>;
    
    /// Get token account balance
    async fn get_token_account_balance(&self, pubkey: &str) -> RpcResult<u64>;
    
    /// Get multiple accounts
    async fn get_multiple_accounts(&self, pubkeys: &[String]) -> RpcResult<Vec<Option<Value>>>;
    
    /// Simulate transaction
    async fn simulate_transaction(&self, transaction: &[u8]) -> RpcResult<Value>;
    
    /// Get program accounts
    async fn get_program_accounts(&self, program_id: &str, filters: Option<Value>) -> RpcResult<Vec<Value>>;
}

/// Helper functions for RPC operations

/// Fetch bonding curve state from account data
pub async fn fetch_bonding_curve_state<C: RpcClient + ?Sized>(
    mint: &str,
    bonding_curve_address: &str,
    client: &C,
) -> RpcResult<BondingCurveState> {
    let account_info = client.get_account_info(bonding_curve_address).await?
        .ok_or_else(|| CoreError::NotFound(format!("Bonding curve account not found: {}", bonding_curve_address)))?;
    
    // Parse account data to extract bonding curve state
    let data = account_info
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|d| d.as_str())
        .ok_or_else(|| CoreError::ParseError("Invalid account data format".to_string()))?;
    
    // Decode base64 data
    let decoded = Base64Engine.decode(data)
        .map_err(|e| CoreError::ParseError(format!("Failed to decode base64: {}", e)))?;
    
    // Parse bonding curve state (structure depends on pump.fun program)
    // This is a simplified version - actual implementation needs proper deserialization
    if decoded.len() < 64 {
        return Err(CoreError::ParseError("Account data too small for bonding curve state".to_string()));
    }
    
    // Extract fields from the account data
    // Note: This is a placeholder - actual offsets depend on the program's data structure
    let virtual_token_reserves = u64::from_le_bytes(decoded[8..16].try_into().unwrap_or([0u8; 8]));
    let virtual_sol_reserves = u64::from_le_bytes(decoded[16..24].try_into().unwrap_or([0u8; 8]));
    let real_token_reserves = u64::from_le_bytes(decoded[24..32].try_into().unwrap_or([0u8; 8]));
    let real_sol_reserves = u64::from_le_bytes(decoded[32..40].try_into().unwrap_or([0u8; 8]));
    let token_total_supply = u64::from_le_bytes(decoded[40..48].try_into().unwrap_or([0u8; 8]));
    let complete = decoded.get(48).map(|&b| b != 0).unwrap_or(false);
    
    Ok(BondingCurveState {
        virtual_token_reserves,
        virtual_sol_reserves,
        real_token_reserves,
        real_sol_reserves,
        token_total_supply,
        complete,
        creator: None, // Creator extraction requires more complex parsing
    })
}

/// Calculate current price from bonding curve state
pub fn calculate_price_from_bonding_curve(state: &BondingCurveState) -> f64 {
    if state.virtual_token_reserves == 0 {
        return 0.0;
    }
    
    // Price in SOL per token
    let sol_reserves = state.virtual_sol_reserves as f64 / 1_000_000_000.0; // Convert lamports to SOL
    let token_reserves = state.virtual_token_reserves as f64 / 1_000_000.0; // Convert to token units
    
    if token_reserves == 0.0 {
        return 0.0;
    }
    
    sol_reserves / token_reserves
}
