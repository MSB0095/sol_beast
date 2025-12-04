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
                let err_str = format!("{:?}", e);
                if (err_str.contains("Too many requests") || err_str.contains("429")) && attempts < max_retries {
                    let backoff_ms = 250 * attempts as u64;
                    debug!("Rate limited fetching tx {} (attempt {}), backing off {}ms", signature, attempts, backoff_ms);
                    
                    // Sleep using platform-appropriate method
                    #[cfg(feature = "native")]
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    
                    #[cfg(feature = "wasm")]
                    {
                        use wasm_bindgen_futures::JsFuture;
                        
                        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                            let window = web_sys::window().expect("no window");
                            window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, backoff_ms as i32)
                                .expect("setTimeout failed");
                        });
                        let _ = JsFuture::from(promise).await;
                    }
                    
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
/// NOTE: This function is now deprecated. Use `fetch_bonding_curve_state` and 
/// `calculate_price_from_bonding_curve` from rpc_client.rs instead.
/// Those functions provide more complete information including liquidity.
#[deprecated(note = "Use fetch_bonding_curve_state and calculate_price_from_bonding_curve from rpc_client.rs")]
pub async fn fetch_token_price<R: RpcClient + ?Sized>(
    mint: &str,
    bonding_curve: &str,
    rpc_client: &R,
) -> TxServiceResult<f64> {
    use crate::rpc_client::{fetch_bonding_curve_state, calculate_price_from_bonding_curve};
    
    let state = fetch_bonding_curve_state(mint, bonding_curve, rpc_client).await?;
    let price = calculate_price_from_bonding_curve(&state);
    Ok(price)
}

#[cfg(test)]
mod tests {
    // Tests would go here - need mock RPC client
}
