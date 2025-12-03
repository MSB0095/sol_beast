// High-level transaction service - combines RPC calls with parsing
// Platform-agnostic business logic for fetching and parsing pump.fun transactions

use crate::error::CoreError;
use crate::rpc_client::RpcClient;
use crate::tx_parser::{parse_transaction, ParsedTransaction};
use crate::metadata::{HttpClient, TokenMetadata, fetch_token_metadata, compute_metadata_pda, decode_account_data};
use log::{info, debug, warn};

/// Result type for transaction service operations
pub type TxServiceResult<T> = Result<T, CoreError>;

/// Fetch and parse transaction details from signature
/// 
/// This function fetches a transaction from RPC and parses it to extract
/// pump.fun token creation information.
/// 
/// # Arguments
/// * `signature` - Transaction signature to fetch
/// * `rpc_client` - RPC client implementation
/// * `pump_fun_program_id` - The pump.fun program ID to look for
/// 
/// # Returns
/// * `Ok(ParsedTransaction)` if transaction is found and contains a create instruction
/// * `Err(CoreError)` if transaction not found, parsing fails, or no create instruction
pub async fn fetch_and_parse_transaction<R: RpcClient + ?Sized>(
    signature: &str,
    rpc_client: &R,
    pump_fun_program_id: &str,
    max_retries: u8,
) -> TxServiceResult<ParsedTransaction> {
    info!("Fetching transaction: {}", signature);
    
    let mut attempts = 0;
    let transaction_json = loop {
        attempts += 1;
        
        match rpc_client.get_transaction(signature).await {
            Ok(Some(tx)) => break tx,
            Ok(None) => {
                return Err(CoreError::NotFound(format!("Transaction not found: {}", signature)));
            }
            Err(e) => {
                // Handle transient errors with retry
                // Check if error is retryable (rate limiting)
                // TODO: Consider adding is_retryable() method to RpcClient trait
                let err_str = format!("{:?}", e);
                let is_rate_limit = err_str.contains("Too many requests") || err_str.contains("429");
                if is_rate_limit && attempts < max_retries {
                    let backoff_ms = 250 * attempts as u64;
                    debug!("Rate limited fetching tx {} (attempt {}), backing off {}ms", signature, attempts, backoff_ms);
                    
                    // Sleep using platform-appropriate method
                    #[cfg(feature = "native")]
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    
                    #[cfg(feature = "wasm")]
                    crate::wasm::sleep_ms(backoff_ms).await;
                    
                    continue;
                }
                return Err(e);
            }
        }
    };
    
    info!("Transaction data retrieved for {}", signature);
    debug!("Transaction raw JSON: {}", transaction_json);
    
    // Parse the transaction
    parse_transaction(&transaction_json, pump_fun_program_id)
}

/// Fetch token metadata (both on-chain and off-chain)
/// 
/// # Arguments
/// * `mint` - Token mint address
/// * `metadata_program` - Metadata program ID (usually Metaplex Token Metadata)
/// * `rpc_client` - RPC client implementation
/// * `http_client` - HTTP client for fetching off-chain metadata
/// 
/// # Returns
/// * `Ok(TokenMetadata)` with whatever metadata could be fetched
pub async fn fetch_complete_token_metadata<R: RpcClient + ?Sized, H: HttpClient>(
    mint: &str,
    metadata_program: &str,
    rpc_client: &R,
    http_client: &H,
) -> TxServiceResult<TokenMetadata> {
    debug!("Fetching complete token metadata for mint: {}", mint);
    
    // Compute metadata PDA
    let metadata_pda = compute_metadata_pda(mint, metadata_program)?;
    debug!("Metadata PDA: {}", metadata_pda);
    
    // Fetch account info
    let account_info = rpc_client.get_account_info(&metadata_pda).await?;
    
    // Decode account data if present
    let account_data = if let Some(info) = account_info {
        match decode_account_data(&info) {
            Ok(data) => Some(data),
            Err(e) => {
                warn!("Failed to decode account data for metadata PDA {} (mint {}): {:?}", metadata_pda, mint, e);
                None
            }
        }
    } else {
        debug!("No account info found for metadata PDA {} (mint {})", metadata_pda, mint);
        None
    };
    
    // Fetch and parse metadata
    fetch_token_metadata(mint, metadata_program, account_data, http_client).await
}

/// Fetch current price from bonding curve
/// 
/// This is a placeholder for price fetching logic that should be centralized.
/// The actual implementation depends on the bonding curve structure.
pub async fn fetch_token_price<R: RpcClient + ?Sized>(
    _mint: &str,
    _bonding_curve: &str,
    _rpc_client: &R,
) -> TxServiceResult<f64> {
    // TODO: Implement actual price fetching from bonding curve
    // This requires fetching the bonding curve account and parsing its state
    warn!("fetch_token_price not yet implemented");
    Err(CoreError::NotImplemented("Price fetching not yet implemented".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Tests would go here - need mock RPC client
}
