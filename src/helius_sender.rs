// Allow deprecated system_instruction module until solana_system_interface is available
#![allow(deprecated)]

use solana_sdk::{
    transaction::VersionedTransaction,
    message::{Message, VersionedMessage},
    signature::{Keypair, Signer},
    instruction::Instruction,
    pubkey::Pubkey,
    system_instruction,
    compute_budget::ComputeBudgetInstruction,
    commitment_config::CommitmentConfig,
};
use solana_client::rpc_client::RpcClient;
use std::error::Error;
use std::sync::Arc;
use crate::settings::Settings;
use log::{debug, info, warn};
use serde_json::{json, Value};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};

// Helius Sender tip accounts (mainnet-beta)
pub const TIP_ACCOUNTS: &[&str] = &[
    "4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE",
    "D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ",
    "9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta",
    "5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn",
    "2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD",
    "2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ",
    "wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF",
    "3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT",
    "4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey",
    "4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or",
];

/// Jito tip floor API endpoint
const JITO_TIP_FLOOR_API: &str = "https://bundles.jito.wtf/api/v1/bundles/tip_floor";

/// Fetch dynamic tip amount from Jito API (75th percentile)
/// Falls back to minimum based on routing mode if API fails or dynamic tips disabled
pub async fn get_dynamic_tip_amount(settings: &Settings) -> Result<f64, Box<dyn Error + Send + Sync>> {
    // Check if dynamic tips are enabled
    if !settings.helius_use_dynamic_tips {
        let static_tip = settings.get_effective_min_tip_sol();
        debug!("Dynamic tips disabled, using static tip: {:.9} SOL", static_tip);
        return Ok(static_tip);
    }
    
    // Only fetch dynamic tips for dual routing mode
    // SWQOS-only should use minimum to keep costs low
    if settings.helius_use_swqos_only {
        let min_tip = settings.get_effective_min_tip_sol();
        debug!("SWQOS-only mode: using minimum tip {:.9} SOL", min_tip);
        return Ok(min_tip);
    }
    
    // Fetch dynamic tip for dual routing
    match fetch_jito_tip_floor().await {
        Ok(tip_75th) => {
            // Use 75th percentile but enforce minimum based on routing mode
            let min_tip = settings.get_effective_min_tip_sol();
            let effective_tip = tip_75th.max(min_tip);
            
            if effective_tip > min_tip {
                info!("Dynamic tip from Jito API: {:.9} SOL (75th percentile)", tip_75th);
            } else {
                debug!("Jito 75th percentile {:.9} SOL below minimum, using {:.9} SOL", tip_75th, min_tip);
            }
            
            Ok(effective_tip)
        }
        Err(e) => {
            let fallback_tip = settings.get_effective_min_tip_sol();
            warn!("Failed to fetch Jito tip floor ({}), using fallback: {:.9} SOL", e, fallback_tip);
            Ok(fallback_tip)
        }
    }
}

/// Fetch tip floor data from Jito API
async fn fetch_jito_tip_floor() -> Result<f64, Box<dyn Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    
    let response = client.get(JITO_TIP_FLOOR_API).send().await?;
    let data: Value = response.json().await?;
    
    // Parse response: array with first element containing landed_tips_75th_percentile
    if let Some(array) = data.as_array() {
        if let Some(first) = array.first() {
            if let Some(tip_75th) = first.get("landed_tips_75th_percentile") {
                if let Some(tip_value) = tip_75th.as_f64() {
                    return Ok(tip_value);
                }
            }
        }
    }
    
    Err("Invalid response format from Jito API".into())
}

/// Get a random tip account from the list
pub fn get_random_tip_account() -> Result<Pubkey, Box<dyn Error + Send + Sync>> {
    use rand::seq::SliceRandom;
    let account_str = TIP_ACCOUNTS
        .choose(&mut rand::thread_rng())
        .ok_or("No tip accounts available")?;
    Ok(std::str::FromStr::from_str(account_str)?)
}

/// Fetch priority fee estimate from Helius Priority Fee API
pub async fn get_priority_fee_estimate(
    rpc_url: &str,
    transaction_base64: &str,
    settings: &Settings,
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let response = client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "getPriorityFeeEstimate",
            "params": [{
                "transaction": transaction_base64,
                "options": { "recommended": true }
            }]
        }))
        .send()
        .await?;

    let json: Value = response.json().await?;
    
    if let Some(result) = json.get("result") {
        if let Some(fee) = result.get("priorityFeeEstimate") {
            if let Some(fee_f64) = fee.as_f64() {
                let adjusted_fee = (fee_f64 * settings.helius_priority_fee_multiplier).ceil() as u64;
                debug!("Priority fee estimate: {} (adjusted: {})", fee_f64, adjusted_fee);
                return Ok(adjusted_fee);
            }
        }
    }
    
    // Fallback to a safe default
    let fallback_fee = 50_000u64;
    debug!("Failed to get priority fee estimate, using fallback: {}", fallback_fee);
    Ok(fallback_fee)
}

/// Send a transaction via Helius Sender endpoint
/// 
/// This function:
/// 1. Fetches dynamic tip amount from Jito API (dual routing) or uses minimum (SWQOS)
/// 2. Simulates transaction to determine optimal compute units
/// 3. Fetches dynamic priority fees from Helius API
/// 4. Adds compute budget instructions (compute unit limit and price)
/// 5. Adds a tip instruction (SOL transfer to random tip account)
/// 6. Validates blockhash before sending
/// 7. Sends the transaction to Helius Sender with skipPreflight=true
/// 
/// Returns the transaction signature on success
pub async fn send_transaction_via_helius(
    instructions: Vec<Instruction>,
    payer: &Keypair,
    settings: &Arc<Settings>,
    rpc_client: &RpcClient,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    if !settings.helius_sender_enabled {
        return Err("Helius Sender is not enabled in settings".into());
    }

    let payer_pubkey = payer.pubkey();
    
    // Fetch dynamic tip amount (uses Jito API for dual routing, minimum for SWQOS)
    let tip_amount_sol = get_dynamic_tip_amount(settings).await?;
    let tip_lamports = (tip_amount_sol * 1_000_000_000.0) as u64;
    
    // Log routing mode and tip
    let routing_mode = if settings.helius_use_swqos_only {
        "SWQOS-only"
    } else {
        "dual routing (validators + Jito)"
    };
    info!("Using {} with tip: {:.9} SOL", routing_mode, tip_amount_sol);
    
    // Build a test transaction to get compute units via simulation
    let test_instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
    ];
    let mut all_test_instructions = test_instructions;
    all_test_instructions.extend(instructions.clone());
    
    // Add tip instruction
    let tip_account = get_random_tip_account()?;
    let tip_instruction = system_instruction::transfer(
        &payer_pubkey,
        &tip_account,
        tip_lamports,
    );
    all_test_instructions.push(tip_instruction.clone());
    
    // Create test transaction for simulation
    let test_message = Message::new(&all_test_instructions, Some(&payer_pubkey));
    let test_tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(test_message),
        &[payer],
    )?;
    
    // Simulate to get compute units
    let sim_result = rpc_client.simulate_transaction(&test_tx)?;
    let compute_units = if let Some(units) = sim_result.value.units_consumed {
        // Add 10% margin to units consumed
        let units_with_margin = (units as f64 * 1.1).ceil() as u32;
        std::cmp::max(units_with_margin, 1000)
    } else {
        debug!("Simulation did not return compute units, using default");
        100_000u32
    };
    
    debug!("Compute units for transaction: {}", compute_units);
    
    // Serialize test transaction for priority fee estimation
    let serialized_test_tx = bincode::serialize(&test_tx)?;
    let test_tx_base64 = Base64Engine.encode(&serialized_test_tx);
    
    // Get priority fee estimate
    let rpc_url = &settings.solana_rpc_urls[0];
    let priority_fee = get_priority_fee_estimate(rpc_url, &test_tx_base64, settings).await?;
    
    debug!(
        "Building final transaction with compute_units={}, priority_fee={}, tip={} SOL",
        compute_units, priority_fee, settings.helius_min_tip_sol
    );
    
    // Build final transaction with optimized compute budget
    let mut final_instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(compute_units),
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
    ];
    final_instructions.extend(instructions);
    final_instructions.push(tip_instruction);
    
    // Create final versioned transaction
    let message = Message::new(&final_instructions, Some(&payer_pubkey));
    let mut tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(message),
        &[payer],
    )?;
    
    // Update with fresh blockhash
    let final_blockhash = rpc_client.get_latest_blockhash()?;
    if let VersionedMessage::Legacy(ref mut msg) = tx.message {
        msg.recent_blockhash = final_blockhash;
    }
    
    // Re-sign with updated blockhash
    let signature_bytes = payer.try_sign_message(tx.message.serialize().as_slice())?;
    tx.signatures[0] = signature_bytes;
    
    // Serialize transaction for sending
    let serialized_tx = bincode::serialize(&tx)?;
    let tx_base64 = Base64Engine.encode(&serialized_tx);
    
    // Build Helius Sender endpoint URL with routing mode and optional API key
    let mut endpoint = settings.helius_sender_endpoint.clone();
    let mut params = Vec::new();
    
    // Add SWQOS-only parameter if enabled
    if settings.helius_use_swqos_only {
        params.push("swqos_only=true".to_string());
    }
    
    // Add API key if provided
    if let Some(api_key) = &settings.helius_api_key {
        params.push(format!("api-key={}", api_key));
    }
    
    // Append parameters to endpoint
    if !params.is_empty() {
        let separator = if endpoint.contains('?') { "&" } else { "?" };
        endpoint = format!("{}{}{}", endpoint, separator, params.join("&"));
    }
    
    let routing_mode = if settings.helius_use_swqos_only {
        "SWQOS-only"
    } else {
        "dual routing (validators + Jito)"
    };
    info!("Sending transaction via Helius Sender ({}) to: {}", routing_mode, endpoint);
    
    // Send transaction via Helius Sender
    let client = reqwest::Client::new();
    let response = client
        .post(&endpoint)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": chrono::Utc::now().timestamp_millis().to_string(),
            "method": "sendTransaction",
            "params": [
                tx_base64,
                {
                    "encoding": "base64",
                    "skipPreflight": true,
                    "maxRetries": 0
                }
            ]
        }))
        .send()
        .await?;
    
    let json: Value = response.json().await?;
    
    if let Some(error) = json.get("error") {
        return Err(format!("Helius Sender error: {}", error).into());
    }
    
    if let Some(result) = json.get("result") {
        if let Some(sig) = result.as_str() {
            info!("Transaction sent via Helius Sender: {}", sig);
            return Ok(sig.to_string());
        }
    }
    
    Err("Invalid response from Helius Sender".into())
}

/// Check if blockhash is still valid
async fn is_blockhash_valid(
    rpc_client: &RpcClient,
    last_valid_block_height: u64,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let current_height = rpc_client.get_block_height()?;
    Ok(current_height <= last_valid_block_height)
}

/// Confirm a transaction with polling and timeout
pub async fn confirm_transaction(
    signature: &str,
    rpc_client: &RpcClient,
    timeout_secs: u64,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let interval = std::time::Duration::from_millis(3000);
    let start_time = std::time::Instant::now();
    
    debug!("Waiting for transaction confirmation: {}", signature);
    
    while start_time.elapsed() < timeout {
        match rpc_client.get_signature_statuses(&[signature.parse()?]) {
            Ok(response) => {
                if let Some(statuses) = response.value.first() {
                    if let Some(status) = statuses {
                        let confirmation = status.confirmation_status.as_ref()
                            .map(|s| format!("{:?}", s))
                            .unwrap_or_else(|| "pending".to_string());
                        
                        debug!("Transaction {} status: {}", signature, confirmation);
                        
                        // Check for confirmed or finalized (check the debug string)
                        if confirmation == "Confirmed" || confirmation == "Finalized" {
                            info!("Transaction confirmed: {}", signature);
                            return Ok(signature.to_string());
                        }
                        
                        // Check for errors
                        if let Some(err) = &status.err {
                            return Err(format!("Transaction failed: {:?}", err).into());
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Status check failed for {}: {}", signature, e);
            }
        }
        
        tokio::time::sleep(interval).await;
    }
    
    warn!("Transaction confirmation timeout after {}s: {}", timeout_secs, signature);
    Err(format!("Transaction confirmation timeout: {}", signature).into())
}

/// Retry logic for sending transactions via Helius Sender with blockhash validation
/// Attempts up to max_retries times with exponential backoff
pub async fn send_transaction_with_retry(
    instructions: Vec<Instruction>,
    payer: &Keypair,
    settings: &Arc<Settings>,
    rpc_client: &RpcClient,
    max_retries: usize,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut last_error: Option<Box<dyn Error + Send + Sync>> = None;
    
    // Get blockhash info once at the start
    let blockhash_info = rpc_client.get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())?;
    let last_valid_block_height = blockhash_info.1;
    
    debug!("Starting transaction send with blockhash valid until block height: {}", last_valid_block_height);
    
    for attempt in 0..max_retries {
        // Check if blockhash is still valid before attempting
        match is_blockhash_valid(rpc_client, last_valid_block_height).await {
            Ok(true) => {
                debug!("Blockhash still valid, attempting send (attempt {}/{})", attempt + 1, max_retries);
            }
            Ok(false) => {
                let err = "Blockhash expired before send attempt";
                warn!("{}", err);
                return Err(err.into());
            }
            Err(e) => {
                warn!("Failed to check blockhash validity: {}", e);
                // Continue anyway, let the RPC reject if expired
            }
        }
        
        match send_transaction_via_helius(instructions.clone(), payer, settings, rpc_client).await {
            Ok(sig) => {
                info!("Transaction sent successfully: {}", sig);
                
                // Optional: Wait for confirmation with timeout
                // Uncomment to enable confirmation checking
                // match confirm_transaction(&sig, rpc_client, 15).await {
                //     Ok(_) => return Ok(sig),
                //     Err(e) => {
                //         warn!("Transaction sent but confirmation failed: {}", e);
                //         return Ok(sig); // Return signature anyway
                //     }
                // }
                
                return Ok(sig);
            }
            Err(e) => {
                warn!("Helius Sender attempt {}/{} failed: {}", attempt + 1, max_retries, e);
                last_error = Some(e);
                
                if attempt < max_retries - 1 {
                    let delay_ms = 1000 * (2_u64.pow(attempt as u32));
                    debug!("Retrying in {} ms...", delay_ms);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| "All retry attempts failed".into()))
}
