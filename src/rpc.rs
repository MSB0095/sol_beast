use crate::{
    error::AppError,
    models::{
    
    BondingCurveState,
    PriceCache,
    RpcResponse,
    OffchainTokenMetadata,
    },
    settings::Settings,
};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use solana_program::program_pack::Pack;

/// Pump.fun `create` instruction discriminator: [24, 30, 200, 40, 5, 28, 7, 119]
/// This is the 8-byte Anchor discriminator for the `create` instruction that creates new tokens.
pub const PUMP_CREATE_DISCRIMINATOR: [u8; 8] = [24, 30, 200, 40, 5, 28, 7, 119];

/// Extracted data from a pump.fun create instruction
struct PumpCreateData {
    mint: String,
    creator: String,
    curve: Option<String>,
}

/// Attempts to extract pump.fun create instruction data from an instruction object.
/// 
/// Returns Some(PumpCreateData) if the instruction is a valid pump.fun create instruction,
/// None otherwise.
fn try_extract_pump_create_data(
    instr: &serde_json::Value,
    account_keys: &[String],
    pump_fun_program_id: &str,
) -> Option<PumpCreateData> {
    // Get program ID
    let program_id = instr
        .get("programId")
        .and_then(|p| p.as_str())
        .or_else(|| {
            instr
                .get("programIdIndex")
                .and_then(|idx| idx.as_u64())
                .and_then(|i| account_keys.get(i as usize).map(|s| s.as_str()))
        })?;
    
    if program_id != pump_fun_program_id {
        return None;
    }
    
    // Check instruction data for create discriminator
    let data_str = instr.get("data").and_then(|d| d.as_str())?;
    let data_bytes = bs58::decode(data_str).into_vec().ok()?;
    
    if data_bytes.len() < 8 || data_bytes[..8] != PUMP_CREATE_DISCRIMINATOR {
        return None;
    }
    
    // Extract accounts - per pump.fun IDL:
    // [0] mint, [1] mint_authority, [2] bonding_curve, [3] associated_bonding_curve,
    // [4] global, [5] mpl_token_metadata, [6] metadata, [7] user (creator/signer)
    let accounts = instr.get("accounts").and_then(|a| a.as_array())?;
    
    if accounts.len() < 8 {
        return None;
    }
    
    // Helper to extract account at index
    let get_account = |idx: usize| -> Option<String> {
        let account_val = accounts.get(idx)?;
        if let Some(i) = account_val.as_u64() {
            account_keys.get(i as usize).cloned()
        } else {
            account_val.as_str().map(|s| s.to_string())
        }
    };
    
    let mint = get_account(0)?;
    let creator = get_account(7)?;
    let curve = get_account(2);
    
    Some(PumpCreateData { mint, creator, curve })
}

/// Computes the holder address (associated token account) for a bonding curve and mint.
/// 
/// # Parameters
/// - `owner`: The owner of the token account (bonding curve PDA)
/// - `mint`: The mint address
fn compute_holder_address(owner: &str, mint: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let owner_pk = Pubkey::from_str(owner)?;
    let mint_pk = Pubkey::from_str(mint)?;
    
    // Use the imported get_associated_token_address function to compute the ATA
    let holder_addr = get_associated_token_address(&owner_pk, &mint_pk);
    Ok(holder_addr.to_string())
}

/// Processes extracted pump.fun create data and computes derived addresses.
/// Returns (creator, mint, curve, holder_addr) on success.
fn process_pump_create_data(
    data: PumpCreateData,
    pump_fun_program: &str,
    location: &str,
) -> Result<(String, String, String, String), Box<dyn std::error::Error + Send + Sync>> {
    // Log info if mint doesn't end with "pump" (unusual but possible)
    if !data.mint.ends_with("pump") {
        info!("{} mint {} does not end with 'pump' (unusual for pump.fun, but accepting)", location, data.mint);
    }
    
    // Compute bonding curve PDA if not already extracted
    let curve = match data.curve {
        Some(c) => c,
        None => {
            let pump_program = Pubkey::from_str(pump_fun_program)
                .map_err(|e| format!("Failed to parse pump.fun program ID: {}", e))?;
            let mint_pk = Pubkey::from_str(&data.mint)
                .map_err(|e| format!("Failed to parse mint address {}: {}", data.mint, e))?;
            let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
            curve_pda.to_string()
        }
    };
    
    // Compute the holder address (ATA for the bonding curve)
    let holder_addr = compute_holder_address(&curve, &data.mint)?;
    
    debug!("Pump.fun CREATE in {}: mint={} creator={} curve={} holder_addr={}", 
           location, data.mint, data.creator, curve, holder_addr);
    
    Ok((data.creator, data.mint, curve, holder_addr))
}

// `select_ok` was previously used for parallel RPC fetch; after switching to
// a rotating sequential probe we no longer need it.
use log::{info, warn, error, debug};
use mpl_token_metadata::accounts::Metadata;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use solana_client::rpc_client::RpcClient;
use crate::tx_builder::{build_sell_instruction, SELL_DISCRIMINATOR};
use crate::idl::load_all_idls;
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};
use solana_program::pubkey::Pubkey;
use spl_token::{self, instruction::close_account};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::{str::FromStr, sync::Arc, time::Instant};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::time::Duration;
use crate::idl::SimpleIdl;


/// Fetches transaction details and extracts pump.fun token creation information.
/// 
/// # Returns
/// On success, returns a tuple of:
/// - `creator`: The user/signer who created the token (account index 7 in create instruction)
/// - `mint`: The mint address of the newly created token (account index 0)
/// - `curve_pda`: The bonding curve PDA address (account index 2)
/// - `holder_addr`: The associated token account owned by the bonding curve PDA
/// - `is_creation`: Whether this transaction contains a pump.fun create instruction
/// 
/// # Detection Logic
/// Uses the pump.fun `create` instruction discriminator [24, 30, 200, 40, 5, 28, 7, 119]
/// to reliably identify token creation transactions. Checks both main instructions and
/// inner instructions (for CPI cases).
pub async fn fetch_transaction_details(
    signature: &str,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<(String, String, String, String, bool), Box<dyn std::error::Error + Send + Sync>> {
    // Request base64 encoding to get raw instruction data for discriminator checking
    let mut attempts = 0u8;
    let data_value: serde_json::Value = loop {
        attempts += 1;
        let resp: Result<RpcResponse<Value>, Box<dyn std::error::Error + Send + Sync>> = fetch_with_fallback::<Value>(
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "getTransaction",
                "params": [ signature, { "encoding": "jsonParsed", "commitment": "confirmed", "maxSupportedTransactionVersion": 0 } ]
            }),
            "getTransaction",
            rpc_client,
            settings,
        )
        .await;

        match resp {
            Ok(rpc_resp) => {
                if let Some(result_val) = rpc_resp.result {
                    break result_val;
                } else {
                    error!("getTransaction returned no result for signature {}", signature);
                    return Err("No transaction data".into());
                }
            }
            Err(e) => {
                let s = e.to_string();
                // Handle common transient errors (rate limit). Retry a few times with backoff.
                if (s.contains("Too many requests") || s.contains("429")) && attempts < 4 {
                    let backoff = std::time::Duration::from_millis(250 * attempts as u64);
                    debug!("Rate limited fetching tx {} (attempt {}), backing off {:?}: {}", signature, attempts, backoff, s);
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                return Err(e);
            }
        }
    };

    info!("Transaction data retrieved for {}", signature);
    debug!("Transaction raw JSON: {}", data_value);

    // Normalize account keys from the transaction message
    let account_keys_arr = data_value
        .get("transaction")
        .and_then(|t| t.get("message"))
        .and_then(|m| m.get("accountKeys"))
        .and_then(|v| v.as_array())
        .ok_or("Missing accountKeys in transaction")?;

    let mut account_keys: Vec<String> = Vec::with_capacity(account_keys_arr.len());
    for key in account_keys_arr {
        if let Some(s) = key.as_str() {
            account_keys.push(s.to_string());
        } else if let Some(obj) = key.as_object() {
            if let Some(pubkey) = obj.get("pubkey").and_then(|p| p.as_str()) {
                account_keys.push(pubkey.to_string());
            } else {
                account_keys.push(serde_json::to_string(obj)?);
            }
        } else {
            account_keys.push(key.to_string());
        }
    }

    let pump_fun_program_id = &settings.pump_fun_program;

    // STEP 1: Check for pump.fun `create` instruction in main instructions
    if let Some(instructions) = data_value
        .get("transaction")
        .and_then(|t| t.get("message"))
        .and_then(|m| m.get("instructions"))
        .and_then(|i| i.as_array())
    {
        for instr in instructions {
            if let Some(data) = try_extract_pump_create_data(instr, &account_keys, pump_fun_program_id) {
                let (creator, mint, curve, holder_addr) = 
                    process_pump_create_data(data, pump_fun_program_id, "main instructions")?;
                return Ok((creator, mint, curve, holder_addr, true));
            }
        }
    }

    // STEP 2: Fallback - check inner instructions for pump.fun create
    // This handles cases where the create is wrapped in a CPI call
    if let Some(meta) = data_value.get("meta") {
        if let Some(inner_instructions) = meta.get("innerInstructions").and_then(|v| v.as_array()) {
            for inner_instruction in inner_instructions {
                if let Some(instructions) = inner_instruction.get("instructions").and_then(|v| v.as_array()) {
                    for instr in instructions {
                        if let Some(data) = try_extract_pump_create_data(instr, &account_keys, pump_fun_program_id) {
                            let (creator, mint, curve, holder_addr) = 
                                process_pump_create_data(data, pump_fun_program_id, "inner instructions")?;
                            return Ok((creator, mint, curve, holder_addr, true));
                        }
                    }
                }
            }
        }
    }

    // No pump.fun create instruction found
    debug!("No pump.fun CREATE instruction found in tx {}", signature);
    debug!("Account keys (len={}): {:?}", account_keys.len(), account_keys);
    Err("Could not find pump.fun create instruction".into())
}



pub async fn fetch_token_metadata(
    mint: &str,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<(Option<Metadata>, Option<OffchainTokenMetadata>, Option<Vec<u8>>), Box<dyn std::error::Error + Send + Sync>> {
    let metadata_program_pk = Pubkey::from_str(&settings.metadata_program)?;
    let mint_pk = Pubkey::from_str(mint)?;
    let metadata_pda = Pubkey::find_program_address(
        &[b"metadata", metadata_program_pk.as_ref(), mint_pk.as_ref()],
        &metadata_program_pk,
    )
    .0;
    debug!("Fetching token metadata for mint {} -> metadata PDA {}", mint, metadata_pda);
    let data: RpcResponse<Value> = fetch_with_fallback::<Value>(
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
            "params": [ metadata_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
        }),
        "getAccountInfo",
        rpc_client,
        settings,
    )
    .await?;
    if let Some(r) = data.result {
        // Normalize: some RPC implementations put the account under result.value
        let account_obj = if let Some(v) = r.get("value") { v.clone() } else { r.clone() };
        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
            match Base64Engine.decode(base64_str) {
                Ok(decoded) => match Metadata::safe_deserialize(&decoded) {
                    Ok(meta) => {
                        // Try to fetch off-chain metadata JSON from the URI in on-chain metadata
                        let uri = meta.uri.trim_end_matches('\u{0}').to_string();
                        if !uri.is_empty() && (uri.starts_with("http://") || uri.starts_with("https://")) {
                            let client = Client::new();
                            match client.get(&uri).send().await {
                                Ok(resp) => match resp.text().await {
                                    Ok(body) => match serde_json::from_str::<serde_json::Value>(&body) {
                                            Ok(body_val) => {
                                                // Try to pull common top-level fields from the JSON using flexible heuristics
                                                fn extract_first_string(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
                                                    for key in keys {
                                                        if let Some(field) = v.get(*key) {
                                                            match field {
                                                                serde_json::Value::String(s) => return Some(s.clone()),
                                                                serde_json::Value::Object(map) => {
                                                                    // Try `en` locale or first string value
                                                                    if let Some(serde_json::Value::String(s2)) = map.get("en") {
                                                                        return Some(s2.clone());
                                                                    }
                                                                    for (_k, val) in map.iter() {
                                                                        if let serde_json::Value::String(s3) = val {
                                                                            return Some(s3.clone());
                                                                        }
                                                                    }
                                                                }
                                                                serde_json::Value::Array(arr) => {
                                                                    if let Some(serde_json::Value::String(s4)) = arr.get(0) {
                                                                        return Some(s4.clone());
                                                                    }
                                                                }
                                                                other => {
                                                                    // Fallback: use string representation for numbers or bools
                                                                    return Some(other.to_string());
                                                                }
                                                            }
                                                        }
                                                    }
                                                    None
                                                }

                                                let mut off = OffchainTokenMetadata {
                                                    name: extract_first_string(&body_val, &["name", "title", "token_name"]),
                                                    symbol: extract_first_string(&body_val, &["symbol", "ticker"]),
                                                    description: body_val.get("description").and_then(|d| d.as_str().map(|s| s.to_string())),
                                                    image: extract_first_string(&body_val, &["image", "image_url", "imageUri"]),
                                                    extras: Some(body_val.clone()),
                                                };
                                                // Normalize and extract fields from extras
                                                off.normalize();
                                                debug!("Fetched off-chain metadata for {}: {:?}", mint, off);
                                                Ok((Some(meta), Some(off), Some(decoded)))
                                            }
                                        Err(e) => {
                                            warn!("Failed to parse off-chain metadata JSON for {}: {}", uri, e);
                                            Ok((Some(meta), None, Some(decoded)))
                                        }
                                    },
                                    Err(e) => {
                                        warn!("Failed to read off-chain metadata body for {}: {}", uri, e);
                                        Ok((Some(meta), None, Some(decoded)))
                                    }
                                },
                                Err(e) => {
                                    warn!("HTTP error fetching off-chain metadata {}: {}", uri, e);
                                    Ok((Some(meta), None, Some(decoded)))
                                }
                            }
                            } else {
                            Ok((Some(meta), None, Some(decoded)))
                        }
                    }
                    Err(e) => {
                        error!("Failed to deserialize metadata for mint {}: {}", mint, e);
                        Ok((None, None, Some(decoded)))
                    }
                },
                Err(e) => {
                    error!("Base64 decode error for metadata PDA {} mint {}: {}", metadata_pda, mint, e);
                    Ok((None, None, None))
                }
            }
        } else {
            // Metadata PDA may legitimately exist but be empty for some mints at
            // creation time; log at debug level to avoid excessive warning spam.
            debug!("No data field returned in account info for metadata PDA {} mint {}", metadata_pda, mint);
            Ok((None, None, None))
        }
    } else {
        // Not fatal: the account may not exist or be unindexed for some RPCs; debug-level log
        debug!("getAccountInfo returned no result for metadata PDA {} mint {}", metadata_pda, mint);
        Ok((None, None, None))
    }
}

pub async fn fetch_with_fallback<T: for<'de> Deserialize<'de> + Send + 'static>(
    request: Value,
    _method: &str,
    _rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<RpcResponse<T>, Box<dyn std::error::Error + Send + Sync>> {
    // Rotate starting index for RPC URLs to spread load when `rotate_rpc` is enabled.
    static RPC_ROUND_ROBIN: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
    let urls = &settings.solana_rpc_urls;
    if urls.is_empty() {
        return Err("No solana_rpc_urls configured".into());
    }
    let client = reqwest::Client::new();
    // Determine start index
    let start = if settings.rotate_rpc {
        RPC_ROUND_ROBIN.fetch_add(1, Ordering::Relaxed) % urls.len()
    } else {
        0
    };
    // Try each endpoint in round-robin order, return first successful parse
    for i in 0..urls.len() {
        let idx = (start + i) % urls.len();
        let http = &urls[idx];
        let request_body = request.clone();
        match client.post(http).json(&request_body).send().await {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))?;
                if !status.is_success() {
                    debug!("HTTP {} from {}: {}", status, http, text);
                    continue;
                }
                match serde_json::from_str::<RpcResponse<T>>(&text) {
                    Ok(parsed) => {
                        if parsed.error.is_some() {
                            return Err(format!("RPC error from {}: {:?}", http, parsed.error).into());
                        }
                        return Ok(parsed);
                    }
                    Err(e) => {
                        debug!("JSON parse error from {}: {} -- body: {}", http, e, text);
                        continue;
                    }
                }
            }
            Err(e) => {
                debug!("HTTP error contacting {}: {}", http, e);
                continue;
            }
        }
    }
    Err("All RPC endpoints failed to respond successfully".into())
}

pub async fn fetch_current_price(
    mint: &str,
    price_cache: &Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let mut cache = price_cache.lock().await;
    if let Some((timestamp, price)) = cache.get(mint) {
        if Instant::now().duration_since(*timestamp) < std::time::Duration::from_secs(settings.price_cache_ttl_secs) {
            return Ok(*price);
        }
    }

    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let mint_pubkey = Pubkey::from_str(mint)?;
    // PDA seeds per pump.fun IDL: ["bonding-curve", mint]
    let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pubkey.as_ref()], &pump_program);
    debug!("Fetching bonding curve account for mint {} -> curve PDA {}", mint, curve_pda);
    // Try multiple commitment levels and a few retries to handle
    // RPC variations and transient propagation delays. We try `processed`
    // first for fastest visibility of newly-created tokens, then fall back
    // to stronger commitments.
    let commitments = ["processed", "confirmed", "finalized"];
    let mut last_err: Option<String> = None;
    let mut decoded_opt: Option<Vec<u8>> = None;
    for c in &commitments {
        // Try a few attempts per commitment to allow different RPC endpoints to respond
        // with populated data (fetch_with_fallback picks the first HTTP success which
        // might still have an empty `value`).
        for attempt in 0..3 {
            let request = json!({
                "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
                "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": c } ]
            });
            match fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await {
                Ok(data) => {
                    if let Some(result_val) = data.result {
                        let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
                            match Base64Engine.decode(base64_str) {
                                Ok(decoded) => {
                                    decoded_opt = Some(decoded);
                                    break;
                                }
                                Err(e) => {
                                    last_err = Some(format!("Decode error for bonding curve {} mint {}: {}", curve_pda, mint, e));
                                }
                            }
                        } else {
                            last_err = Some(format!("No data field in account object for curve PDA {} at commitment {} (attempt {})", curve_pda, c, attempt));
                        }
                    } else {
                        last_err = Some(format!("getAccountInfo returned no result for curve PDA {} at commitment {} (attempt {})", curve_pda, c, attempt));
                    }
                }
                Err(e) => {
                    last_err = Some(format!("RPC error fetching curve PDA {} at commitment {} (attempt {}): {}", curve_pda, c, attempt, e));
                }
            }
            // slight backoff between attempts
            tokio::time::sleep(std::time::Duration::from_millis(150 * (attempt as u64 + 1))).await;
        }
        if decoded_opt.is_some() {
            break;
        }
    }

    // If we couldn't read the expected curve PDA, try a server-side search of the pump.fun
    // program accounts using getProgramAccountsV2 with memcmp filters for the mint. Some
    // providers index program accounts differently or the PDA may not be available at the
    // requested commitment; a targeted program-side search is more likely to find the
    // bonding-curve account than scanning token program accounts client-side.
    if decoded_opt.is_none() {
        // If direct queries failed, try a pump.fun program-side lookup (may not always
        // succeed because the mint is not necessarily stored in the account data). We
        // also attempt a direct per-endpoint probe for the computed PDA to see if any
        // RPC node has the account populated.
        if let Ok(Some(found_curve)) = find_curve_account_by_mint(mint, rpc_client, settings).await {
            debug!("Found curve account via pump.fun program lookup for mint {} -> {}", mint, found_curve);
            // Try to fetch the account data for the found curve pubkey once (confirmed)
            let request = json!({
                "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
                "params": [ found_curve, { "encoding": "base64", "commitment": "confirmed" } ]
            });
            if let Ok(data) = fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await {
                if let Some(result_val) = data.result {
                    let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                    if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
                        if let Ok(decoded) = Base64Engine.decode(base64_str) {
                            decoded_opt = Some(decoded);
                        }
                    }
                }
            }
        }
        // Direct per-endpoint probe for the computed PDA: try each configured RPC URL
        // and pick the first one that returns populated account data. This avoids the
        // `select_ok` behavior which can return the first HTTP success even when the
        // `value` is null.
        if decoded_opt.is_none() {
            for http in &settings.solana_rpc_urls {
                let client = reqwest::Client::new();
                let request = json!({
                    "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
                    "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "finalized" } ]
                });
                match client.post(http).json(&request).send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        let text = resp.text().await.unwrap_or_else(|_| "<failed to read body>".to_string());
                        if !status.is_success() {
                            debug!("Endpoint {} returned HTTP {} for curve PDA {}: {}", http, status, curve_pda, text);
                            continue;
                        }
                        match serde_json::from_str::<RpcResponse<Value>>(&text) {
                            Ok(parsed) => {
                                if let Some(rv) = parsed.result {
                                    let account_obj = if let Some(v) = rv.get("value") { v.clone() } else { rv.clone() };
                                    if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
                                        if let Ok(decoded) = Base64Engine.decode(base64_str) {
                                            decoded_opt = Some(decoded);
                                            break;
                                        }
                                    } else {
                                        debug!("Endpoint {} returned no data for curve PDA {}", http, curve_pda);
                                    }
                                } else {
                                    debug!("Endpoint {} returned null result for curve PDA {}", http, curve_pda);
                                }
                            }
                            Err(e) => {
                                debug!("Failed to parse JSON from {} for curve PDA {}: {} -- body: {}", http, curve_pda, e, text);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("HTTP error contacting {} for curve PDA {}: {}", http, curve_pda, e);
                    }
                }
            }
        }
    }

    let price: f64;
    if let Some(decoded) = decoded_opt {
    // Validate expected pump.fun discriminator and minimum length. The pump.fun
    // bonding-curve account uses an 8-byte Anchor discriminator prefix followed
    // by the following fields (after the 8 bytes): five u64 (40 bytes) and a
    // bool (1 byte) => 41 bytes expected post-discriminator. Total minimum
    // account length = 8 + 41 = 49 bytes.
    const PUMP_CURVE_DISCRIMINATOR: [u8; 8] = [0x17, 0xb7, 0xf8, 0x37, 0x60, 0xd8, 0xac, 0x60];
    let min_total_len: usize = 8 + 41;

    // Helper: rate-limited/logging for curve errors. Use a global debounce map
    // keyed by curve_pda+mint so we avoid log spam when many RPC calls fail in
    // a short period.
    static BONDING_CURVE_ERROR_TIMES: Lazy<tokio::sync::Mutex<HashMap<String, Instant>>> =
        Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));

    async fn report_curve_issue(
        settings: &crate::settings::Settings,
        curve_pda: &Pubkey,
        mint: &str,
        short_msg: &str,
        decoded: &[u8],
    ) {
        let key = format!("{}:{}", curve_pda, mint);
        let b64 = Base64Engine.encode(decoded);
        let full_hex: String = decoded.iter().map(|b| format!("{:02x}", b)).collect();

        if settings.bonding_curve_strict {
            error!("{} for curve {} mint {}", short_msg, curve_pda, mint);
            debug!("Bonding curve raw (base64) for {}: {}", curve_pda, b64);
            debug!("Bonding curve raw (hex) for {}: {}", curve_pda, full_hex);
        } else {
            // Tolerant mode: rate-limit warnings to avoid spam. If debounced,
            // emit a debug line instead of warn.
            let mut map = BONDING_CURVE_ERROR_TIMES.lock().await;
            let now = Instant::now();
            let debounce_secs = settings.bonding_curve_log_debounce_secs;
                    match map.get(&key) {
                Some(last) if now.duration_since(*last) < Duration::from_secs(debounce_secs) => {
                    debug!("Debounced curve issue for {}: {}", key, short_msg);
                }
                _ => {
                    map.insert(key.clone(), now);
                    // Emit a conspicuous warning but mark it as tolerated.
                    warn!("Tolerated bonding curve issue for {} mint {}: {}", curve_pda, mint, short_msg);
                    // Print encoded forms for interactive debugging only (no files).
                    debug!("Bonding curve raw (base64) for {}: {}", curve_pda, b64);
                    debug!("Bonding curve raw (hex) for {}: {}", curve_pda, full_hex);
                }
            }
        }
    }

    if decoded.len() < 8 {
        report_curve_issue(settings, &curve_pda, mint, &format!("too short: len={} < 8 (no discriminator)", decoded.len()), &decoded).await;
        return Err(format!("Bonding curve account too short for {}", mint).into());
    }
    let disc_bytes = &decoded[..8];
    if disc_bytes != PUMP_CURVE_DISCRIMINATOR {
        report_curve_issue(settings, &curve_pda, mint, "unexpected discriminator", &decoded).await;
        return Err(format!("Unexpected discriminator for {}: not a pump.fun curve", mint).into());
    }
    if decoded.len() < min_total_len {
        report_curve_issue(settings, &curve_pda, mint, &format!("too short: len={} < expected {}", decoded.len(), min_total_len), &decoded).await;
        return Err(format!("Bonding curve account too short for {}", mint).into());
    }

    // Safe to slice past the 8-byte discriminator now
    let slice = &decoded[8..];
    // Add detailed debug info to help diagnose layout mismatches: length and
    // a short hex prefix of the on-chain data (post-discriminator).
    let prefix_len = std::cmp::min(64, slice.len());
    let prefix_hex: String = slice[..prefix_len].iter().map(|b| format!("{:02x}", b)).collect();
    debug!(
        "Bonding curve raw bytes len={} slice_len={} discriminator={:?} first{}={}",
        decoded.len(),
        slice.len(),
        disc_bytes,
        prefix_len,
        prefix_hex
    );

    // Manually parse the fixed fields (5 * u64 + bool = 41 bytes) from the
    // beginning of the slice. This is more tolerant to trailing bytes that
    // some pump.fun curve accounts contain (avoid Borsh failing on "not all
    // bytes read"). We already verified the discriminator and minimum total
    // length above.
    let needed = 8 * 5 + 1; // 41
    if slice.len() < needed {
        report_curve_issue(settings, &curve_pda, mint, &format!("post-discriminator too short: {} < {}", slice.len(), needed), &decoded).await;
        return Err(format!("Bonding curve post-discriminator too short for {}", mint).into());
    }

    // Safe to index because we checked length
    let virtual_token_reserves = u64::from_le_bytes(slice[0..8].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?);
    let virtual_sol_reserves = u64::from_le_bytes(slice[8..16].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?);
    let real_token_reserves = u64::from_le_bytes(slice[16..24].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?);
    let real_sol_reserves = u64::from_le_bytes(slice[24..32].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?);
    let token_total_supply = u64::from_le_bytes(slice[32..40].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?);
    let complete = slice[40] != 0;

    // Parse creator (32 bytes after complete bool)
    // Layout: 5*u64 (40 bytes) + bool (1 byte) + creator (32 bytes)
    let creator = if slice.len() >= 73 {
        Pubkey::try_from(&slice[41..73]).ok()
    } else {
        None
    };

    if slice.len() > needed {
        debug!(
            "Bonding curve slice for {} has {} extra bytes after expected fields (total {} bytes, creator {:?})",
            mint,
            slice.len() - needed,
            slice.len(),
            creator.map(|p| p.to_string())
        );
    }

        let state = BondingCurveState {
        virtual_token_reserves,
        virtual_sol_reserves,
        real_token_reserves,
        real_sol_reserves,
        token_total_supply,
        complete,
        creator,
    };
    // Print parsed on-chain bonding curve info at info level so operators can
    // see the core fields in logs without having to inspect files.
    info!(
        "Bonding curve for mint {} curve {}: virtual_token_reserves={} virtual_sol_reserves={} ({} SOL) real_token_reserves={} real_sol_reserves={} ({} SOL) token_total_supply={} complete={} creator={:?}",
        mint,
        curve_pda,
        state.virtual_token_reserves,
        state.virtual_sol_reserves,
        state.virtual_sol_reserves as f64 / 1_000_000_000.0,
        state.real_token_reserves,
        state.real_sol_reserves,
        state.real_sol_reserves as f64 / 1_000_000_000.0,
        state.token_total_supply,
        state.complete,
        state.creator.map(|p| p.to_string())
    );
        if state.complete {
            error!("Bonding curve state reports migrated for mint {}", mint);
            return Err(format!("Token {} migrated to Raydium", mint).into());
        }
        // Compute price as SOL per token using virtual reserves (per new directive).
        // SOL = lamports / 1e9; tokens = base_units / 1e6
        let token_reserves = state.virtual_token_reserves as f64;
        let lamports_reserves = state.virtual_sol_reserves as f64; // lamports
        if token_reserves > 0.0 {
            // Fetch mint decimals for correct token units
            let decimals = match fetch_mint_decimals(mint, rpc_client, settings).await {
                Ok(d) => d,
                Err(e) => {
                    warn!("Failed to fetch mint decimals for {}: {} -- falling back to {}", mint, e, settings.default_token_decimals);
                    settings.default_token_decimals
                }
            };
            price = (lamports_reserves / 1_000_000_000.0) / (token_reserves / 10f64.powi(decimals as i32));
        } else {
            error!("Invalid reserves for {}: zero tokens (state: {:?})", mint, state);
            return Err(format!("Invalid reserves for {}: zero tokens", mint).into());
        }
    } else {
        let msg = last_err.unwrap_or_else(|| format!("Bonding curve account not found or empty for {} (curve PDA {})", mint, curve_pda));
        error!("{}", msg);
        return Err(format!("Bonding curve account missing or unreadable for {}: {}", mint, msg).into());
    }

    // `price` is SOL per token already. Cache and return.
    cache.put(mint.to_string(), (Instant::now(), price));
    Ok(price)
}

/// Fetch mint decimals by reading the SPL Mint account and parsing the Mint state.
pub async fn fetch_mint_decimals(
    mint: &str,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<u8, Box<dyn std::error::Error + Send + Sync>> {
    // Try multiple commitments and retries (similar to fetch_current_price) to
    // robustly fetch mint account data and parse the `decimals` field.
    let mint_pk = Pubkey::from_str(mint)?;
    let commitments = ["processed", "confirmed", "finalized"];
    for c in &commitments {
        for attempt in 0..3 {
            let request = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getAccountInfo",
                "params": [ mint_pk.to_string(), { "encoding": "base64", "commitment": c } ]
            });
            match fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await {
                Ok(v) => {
                    if let Some(result_val) = v.result {
                        let account_obj = if let Some(x) = result_val.get("value") { x.clone() } else { result_val.clone() };
                        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
                            match Base64Engine.decode(base64_str) {
                                Ok(decoded) => {
                                    if let Ok(mint_state) = spl_token::state::Mint::unpack(&decoded) {
                                        debug!("Fetched mint decimals for {} at commitment {}: {}", mint, c, mint_state.decimals);
                                        return Ok(mint_state.decimals);
                                    } else {
                                        debug!("Failed to parse Mint state for {} (attempt {}, commitment {})", mint, attempt, c);
                                        // continue retries
                                    }
                                }
                                Err(e) => {
                                    debug!("Failed to base64-decode mint account for {}: {}", mint, e);
                                }
                            }
                        } else {
                            debug!("No account data for mint {} (commitment={}, attempt={})", mint, c, attempt);
                        }
                    }
                }
                Err(e) => debug!("RPC error fetching mint {} at commitment {} (attempt {}): {}", mint, c, attempt, e),
            }
            // slight backoff
            tokio::time::sleep(std::time::Duration::from_millis(150 * (attempt as u64 + 1))).await;
        }
    }

    // Fallback: try `getTokenSupply` which some RPCs expose and that returns `decimals`
    let commitments = ["processed", "confirmed", "finalized"];
    for c in &commitments {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTokenSupply",
            "params": [ mint_pk.to_string(), { "commitment": c } ]
        });
        if let Ok(resp) = fetch_with_fallback::<Value>(request, "getTokenSupply", rpc_client, settings).await {
            if let Some(result_val) = resp.result {
                let value = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                if let Some(dec) = value.get("decimals").and_then(|d| d.as_u64()) {
                    debug!("Fetched decimals for {} via getTokenSupply (commitment={}): {}", mint, c, dec);
                    return Ok(dec as u8);
                }
            }
        }
    }

    Err(format!("Failed to fetch mint decimals for {} after retries", mint).into())
}

/// Compute price from raw reserves using the mint decimals (fallback to 6 decimals if
/// fetching decimals fails). Price = (vsol_lamports / 1e9) / (vtok / 10^decimals)
pub async fn price_from_reserves(
    mint: &str,
    vtok: u64,
    vsol: u64,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    // Try to fetch decimals; pump.fun tokens always use 6 decimals so that is the
    // correct fallback (not the configurable default_token_decimals).
    let decimals: u8 = match fetch_mint_decimals(mint, rpc_client, settings).await {
        Ok(d) => d,
        Err(e) => {
            warn!("Failed to fetch mint decimals for {}: {} -- falling back to 6 (pump.fun default)", mint, e);
            6
        }
    }; 
    let price = (vsol as f64 / 1_000_000_000.0) / (vtok as f64 / 10f64.powi(decimals as i32));
    Ok(price)
}

pub async fn detect_idl_for_mint(mint: &str, rpc_client: &Arc<RpcClient>, settings: &Arc<Settings>) -> Option<SimpleIdl> {
    let idls = load_all_idls();
    if idls.is_empty() { return None; }
    let mint_pk = match Pubkey::from_str(mint) { Ok(m) => m, Err(_) => return None };
    for (_k, idl) in idls.into_iter() {
        // compute PDA seeds per pump.fun: ["bonding-curve", mint]
        let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &idl.address);
        let req = json!({"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":[curve_pda.to_string(), {"encoding":"base64","commitment":"confirmed"}]});
        if let Ok(resp) = fetch_with_fallback::<Value>(req, "getAccountInfo", rpc_client, settings).await {
            if resp.result.is_some() {
                return Some(idl);
            }
        }
    }
    None
}

/// Given a list of AccountMeta and known context (mint, user, creator, bonding_curve),
/// return a list of create_associated_token_account instructions to create missing ATAs.
pub async fn build_missing_ata_preinstructions(
    context: &HashMap<String, Pubkey>,
) -> Result<Vec<solana_program::instruction::Instruction>, Box<dyn std::error::Error + Send + Sync>> {
    let mut pre: Vec<solana_program::instruction::Instruction> = Vec::new();
    // For candidate owners, prepare owner->mint pairs to check
    let mut candidates: Vec<(Pubkey, Pubkey)> = Vec::new();
    // extract mint from context
    let mint_pk = if let Some(m) = context.get("mint") { *m } else { return Ok(pre); };
    
    // ONLY create ATAs for the user - NOT for bonding_curve or creator
    // The pump.fun program creates those internally via CPI
    // Attempting to create them manually will fail with "Provided owner is not allowed"
    if let Some(user) = context.get("user") { 
        candidates.push((*user, mint_pk)); 
    }

    // Also inspect explicit AccountMeta entries to detect ATAs by pattern
    for (owner, mint) in candidates.iter() {
        // ALWAYS create ATA instruction (it's idempotent - won't fail if exists)
        // This avoids race conditions on very early sniping
        let payer = context.get("payer").cloned().unwrap_or(*owner);
        pre.push(create_associated_token_account(&payer, owner, mint, &spl_token::id()));
        debug!("Adding create ATA instruction for owner {} mint {}", owner, mint);
    }
    Ok(pre)
}

/// Fetch bonding curve state for safety checks (liquidity validation)
pub async fn fetch_bonding_curve_state(mint: &str, rpc_client: &Arc<RpcClient>, settings: &Arc<Settings>) -> Result<BondingCurveState, Box<dyn std::error::Error + Send + Sync>> {
    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let mint_pubkey = Pubkey::from_str(mint)?;
    let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pubkey.as_ref()], &pump_program);
    
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
        "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
    });
    
    let data = fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await?;
    if let Some(result_val) = data.result {
        let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
            let decoded = Base64Engine.decode(base64_str)?;
            
            // Parse bonding curve state
            const PUMP_CURVE_DISCRIMINATOR: [u8; 8] = [0x17, 0xb7, 0xf8, 0x37, 0x60, 0xd8, 0xac, 0x60];
            if decoded.len() >= 49 && decoded[..8] == PUMP_CURVE_DISCRIMINATOR {
                let slice = &decoded[8..];
                // Layout: 5*u64 (40 bytes) + bool (1 byte) + creator (32 bytes) = 73 bytes minimum
                let creator = if slice.len() >= 73 {
                    Pubkey::try_from(&slice[41..73]).ok()
                } else {
                    None
                };
                
                let state = BondingCurveState {
                    virtual_token_reserves: u64::from_le_bytes(slice[0..8].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?),
                    virtual_sol_reserves: u64::from_le_bytes(slice[8..16].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?),
                    real_token_reserves: u64::from_le_bytes(slice[16..24].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?),
                    real_sol_reserves: u64::from_le_bytes(slice[24..32].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?),
                    token_total_supply: u64::from_le_bytes(slice[32..40].try_into().map_err(|e: std::array::TryFromSliceError| Box::new(AppError::Conversion(e.to_string())))?),
                    complete: slice[40] != 0,
                    creator,
                };
                return Ok(state);
            }
        }
    }
    Err("Failed to fetch bonding curve state".into())
}

/// Fetch the fee_recipient from the Global PDA account
/// Global account layout (after 8-byte discriminator):
/// - initialized: bool (1 byte)
/// - authority: Pubkey (32 bytes)
/// - fee_recipient: Pubkey (32 bytes) ← at offset 8 + 1 + 32 = 41
pub async fn fetch_global_fee_recipient(rpc_client: &Arc<RpcClient>, settings: &Arc<Settings>) -> Result<Pubkey, Box<dyn std::error::Error + Send + Sync>> {
    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &pump_program);
    
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
        "params": [ global_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
    });
    
    let data = fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await?;
    if let Some(result_val) = data.result {
        let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
            let decoded = Base64Engine.decode(base64_str)?;
            
            // Global discriminator
            const GLOBAL_DISCRIMINATOR: [u8; 8] = [0xa7, 0xe8, 0xe8, 0xb1, 0xc8, 0x6c, 0x72, 0x7f];
            if decoded.len() >= 73 && decoded[..8] == GLOBAL_DISCRIMINATOR {
                let slice = &decoded[8..];
                // Layout: initialized (bool, 1 byte) + authority (32 bytes) + fee_recipient (32 bytes)
                // fee_recipient is at offset 1 + 32 = 33
                if slice.len() >= 65 {
                    let fee_recipient = Pubkey::try_from(&slice[33..65])?;
                    info!("Fetched fee_recipient from Global PDA: {}", fee_recipient);
                    return Ok(fee_recipient);
                }
            }
        }
    }
    Err("Failed to fetch fee_recipient from Global account".into())
}


/// Fetch the bonding curve account for `mint` and attempt to read the creator pubkey
/// from the on-chain `BondingCurve` struct. Returns `None` if the account is missing
/// or the layout is unexpected.
pub async fn fetch_bonding_curve_creator(mint: &str, rpc_client: &Arc<RpcClient>, settings: &Arc<Settings>) -> Result<Option<Pubkey>, Box<dyn std::error::Error + Send + Sync>> {
    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let mint_pk = Pubkey::from_str(mint)?;
    let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
    let request = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
        "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
    });
    match fetch_with_fallback::<serde_json::Value>(request, "getAccountInfo", rpc_client, settings).await {
        Ok(resp) => {
            if let Some(result_val) = resp.result {
                let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
                    if let Ok(decoded) = Base64Engine.decode(base64_str) {
                        // BondingCurve layout per IDL: 8-byte discriminator + 5*u64 + bool + creator(pubkey)
                        let needed = 8 + 8*5 + 1 + 32; // 8 + 40 + 1 + 32 = 81
                        if decoded.len() >= needed {
                            let creator_offset = 8 + 8*5 + 1;
                            let creator_bytes = &decoded[creator_offset..creator_offset+32];
                            if let Ok(pk) = Pubkey::try_from(creator_bytes) {
                                return Ok(Some(pk));
                            }
                        }
                    }
                }
            }
            Ok(None)
        }
        Err(e) => {
            warn!("Failed to fetch curve PDA for creator extraction: {}", e);
            Ok(None)
        }
    }
}

/// Find a token account for `mint` whose owner is `owner_pubkey` by querying the
/// SPL Token program with `getProgramAccounts` and memcmp filters on the mint and owner
/// fields of the token account layout. Returns the first matching token account pubkey
/// if found.
pub async fn find_token_account_owned_by_owner(
    mint: &str,
    owner_pubkey: &str,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    // SPL Token program id
    let token_program = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
    // Use getProgramAccountsV2 which supports pagination and avoids large-result errors
    // on providers like Helius. Request the smallest page possible (limit/pageSize=1).
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getProgramAccountsV2",
        "params": [
            token_program,
            {
                "encoding": "base64",
                "commitment": "confirmed",
                "filters": [
                    { "memcmp": { "offset": 0, "bytes": mint } },
                    { "memcmp": { "offset": 32, "bytes": owner_pubkey } }
                ],
                // Helius accepts `pageSize` for V2 pagination; request a single page entry
                "pageSize": 1
            }
        ]
    });

    match fetch_with_fallback::<Value>(request, "getProgramAccountsV2", rpc_client, settings).await {
        Ok(resp) => {
            if let Some(result_val) = resp.result {
                // V2 may return either an array of entries or an object with an `accounts` array.
                if let Some(arr) = result_val.as_array() {
                    if !arr.is_empty() {
                        if let Some(entry) = arr.first() {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                } else if let Some(obj_arr) = result_val.get("accounts").and_then(|v| v.as_array()) {
                    if !obj_arr.is_empty() {
                        if let Some(entry) = obj_arr.first() {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                } else if let Some(obj_arr) = result_val.get("value").and_then(|v| v.as_array()) {
                    if !obj_arr.is_empty() {
                        if let Some(entry) = obj_arr.first() {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                }
            }
            Ok(None)
        }
        Err(e) => {
            // This is expected for RPCs that don't support getProgramAccounts (e.g., Chainstack)
            // Not a critical error since we have other fallback methods
            debug!("getProgramAccountsV2 not supported or failed for mint {} owner {}: {}", mint, owner_pubkey, e);
            Ok(None)
        }
    }
}

/// Query the pump.fun program accounts (server-side) to find a bonding-curve account
/// that references `mint`. This uses `getProgramAccountsV2` with a memcmp filter on
/// the mint bytes. Returns the first matching account pubkey if found.
async fn find_curve_account_by_mint(
    mint: &str,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    let pump_program = &settings.pump_fun_program;
    // memcmp expects raw bytes; put the base58 mint into the `bytes` field. Providers
    // will accept the base58 string as the memcmp value for filters.
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getProgramAccountsV2",
        "params": [
            pump_program,
            {
                "encoding": "base64",
                "commitment": "confirmed",
                "filters": [
                    { "memcmp": { "offset": 8, "bytes": mint } }
                ],
                "pageSize": 1
            }
        ]
    });

    match fetch_with_fallback::<Value>(request, "getProgramAccountsV2", rpc_client, settings).await {
        Ok(resp) => {
            if let Some(result_val) = resp.result {
                if let Some(arr) = result_val.as_array() {
                    if !arr.is_empty() {
                        if let Some(entry) = arr.first() {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                } else if let Some(obj_arr) = result_val.get("accounts").and_then(|v| v.as_array()) {
                    if !obj_arr.is_empty() {
                        if let Some(entry) = obj_arr.first() {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                } else if let Some(obj_arr) = result_val.get("value").and_then(|v| v.as_array()) {
                    if !obj_arr.is_empty() {
                        if let Some(entry) = obj_arr.first() {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                }
            }
            Ok(None)
        }
        Err(e) => {
            // This is expected for RPCs that don't support getProgramAccounts (e.g., Chainstack)
            // Not a critical error since we have other fallback methods
            debug!("pump.fun getProgramAccountsV2 not supported or failed for mint {}: {}", mint, e);
            Ok(None)
        }
    }
}



pub async fn sell_token(
    mint: &str,
    amount: u64,
    current_price: f64,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
    is_final_sell: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // current_price is SOL per token
    let sol_received = amount as f64 * current_price;
    info!(
        "Sell {}: {} tokens for {:.9} SOL (price: {:.18} SOL/token)",
        mint,
        amount,
        sol_received,
        current_price
    );
    let client = RpcClient::new(&settings.solana_rpc_urls[0]);
    let _idls = load_all_idls();
    let creator_opt = fetch_bonding_curve_creator(mint, rpc_client, settings).await.ok().flatten();
    
    // Fetch fee_recipient from Global PDA
    let fee_recipient = fetch_global_fee_recipient(rpc_client, settings).await?;

    if is_real {
        // Real run: build instruction with the real keypair's pubkey as user (signer)
        let payer = keypair.ok_or("Keypair required")?;
        let user_pubkey = payer.pubkey();
        // Try to detect IDL for this mint and build context
        let detected_idl_opt = detect_idl_for_mint(mint, rpc_client, settings).await;
        let mut instruction_opt: Option<solana_program::instruction::Instruction> = None;
        let mint_pk = Pubkey::from_str(mint)?;
        let mut context: HashMap<String, Pubkey> = HashMap::new();
        context.insert("mint".to_string(), mint_pk);
        context.insert("user".to_string(), user_pubkey);
        if let Some(c) = creator_opt { context.insert("bonding_curve.creator".to_string(), c); }
        let pump_program_pk = Pubkey::from_str(&settings.pump_fun_program)?;
        let (curve_pda_fallback, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program_pk);
        context.insert("bonding_curve".to_string(), curve_pda_fallback);
        if let Some(creator) = context.get("bonding_curve.creator") {
            let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program_pk);
            context.insert("creator_vault".to_string(), creator_vault);
        }
        // Use actual fee_recipient from bonding curve
        context.insert("fee_recipient".to_string(), fee_recipient);
        // Add fee_program - for SELL it IS included in the main instruction accounts (unlike buy)
        let fee_program_pubkey = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;
        context.insert("fee_program".to_string(), fee_program_pubkey);
        let try_idls: Vec<SimpleIdl> = if let Some(idl) = detected_idl_opt { vec![idl] } else { load_all_idls().into_values().collect() };
        for idl in try_idls {
            match idl.build_accounts_for("sell", &context) {
                Ok(metas) => {
                    let mut d = SELL_DISCRIMINATOR.to_vec();
                    // Calculate min_sol_output: (tokens / 1e6) * (SOL per token) * 1e9 lamports
                    // Simplifies to: tokens * current_price * 1000
                    let min_sol_output = (amount as f64 * current_price * 1000.0) as u64;
                    // Apply slippage tolerance (reduce minimum by slippage percentage)
                    let slippage_multiplier = 1.0 - (settings.slippage_bps as f64 / 10000.0);
                    let min_sol_with_slippage = (min_sol_output as f64 * slippage_multiplier) as u64;
                    d.extend(borsh::to_vec(&crate::tx_builder::SellArgs { amount, min_sol_output: min_sol_with_slippage })?);
                    instruction_opt = Some(solana_program::instruction::Instruction { program_id: idl.address, accounts: metas, data: d });
                    break;
                }
                Err(e) => debug!("IDL build failed for sell: {}", e),
            }
        }
        let instruction = if let Some(instr) = instruction_opt { instr } else {
            // fallback to legacy builder using configured pump program
            let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
            // Calculate min_sol_output with slippage
            let min_sol_output = (amount as f64 * current_price * 1000.0) as u64;
            let slippage_multiplier = 1.0 - (settings.slippage_bps as f64 / 10000.0);
            let min_sol_with_slippage = (min_sol_output as f64 * slippage_multiplier) as u64;
            build_sell_instruction(
                &program_id,
                mint,
                amount,
                min_sol_with_slippage,
                &user_pubkey,
                &fee_recipient,
                creator_opt,
                settings,
            )?
        };
        // Ensure ATA exists for user (sell path may not need it but check)
        let ata = get_associated_token_address(&user_pubkey, &mint_pk);
        match fetch_with_fallback::<Value>(json!({
            "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
            "params": [ ata.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
        }), "getAccountInfo", rpc_client, settings).await {
            Ok(info) => {
                if info.result.is_none() {
                    // create ATA as pre-instruction if real send (simulate will include it too)
                }
            }
            Err(e) => debug!("Failed to check ATA existence for sell {}: {}", ata, e),
        }
        debug!("Sending real sell TX for mint {} amount {} tokens", mint, amount);
        // For sell, we don't need to create any ATAs - they should already exist from buy
        // Just build the transaction with sell instruction + close_account
        let mut all_instrs: Vec<solana_program::instruction::Instruction> = Vec::new();
        all_instrs.push(instruction);
        
        // After selling, close the ATA to reclaim rent (~0.00203928 SOL)
        // Only close the ATA on the FINAL sell (when position is fully exited)
        if is_final_sell {
            let ata = get_associated_token_address(&user_pubkey, &mint_pk);
            let close_ata_instruction = close_account(
                &spl_token::id(),           // token program
                &ata,                        // account to close
                &user_pubkey,               // destination for lamports (rent refund)
                &user_pubkey,               // owner of the account
                &[],                         // no additional signers needed
            )?;
            all_instrs.push(close_ata_instruction);
            info!("Added close_account instruction to reclaim ~0.00203928 SOL rent from ATA {}", ata);
        }
        
        // Add unconditional 1% dev fee
        {
            let sol_received_lamports = (sol_received * 1_000_000_000.0) as u64;
            crate::dev_fee::add_dev_fee_to_instructions(&mut all_instrs, &user_pubkey, sol_received_lamports)?;
            info!("Added 1% dev fee to sell transaction (expected: {:.9} SOL)", sol_received);
        }
        
        // Before sending: record pre-send SOL and token balances so we can compute exact deltas
        let pre_sol_lamports = client.get_balance(&user_pubkey)?;
        let mut pre_token_amount: u64 = 0;
        if let Ok(Some(acc)) = find_token_account_owned_by_owner(mint, &user_pubkey.to_string(), rpc_client, settings).await {
            if let Ok(pk) = Pubkey::from_str(&acc) {
                if let Ok(bal) = client.get_token_account_balance(&pk) {
                    if let Ok(v) = bal.amount.parse::<u64>() { pre_token_amount = v; }
                }
            }
        }

        // Choose transaction submission method and capture signature
        let signature: String;
        if settings.helius_sender_enabled {
            info!("Using Helius Sender for sell transaction of mint {}", mint);
            signature = crate::helius_sender::send_transaction_with_retry(
                all_instrs,
                payer,
                settings,
                &client,
                3, // max retries
            ).await?;
            info!("Sell transaction sent via Helius Sender: {}", signature);
        } else {
            let mut tx = Transaction::new_with_payer(&all_instrs, Some(&payer.pubkey()));
            let blockhash = client.get_latest_blockhash()?;
            tx.sign(&[payer], blockhash);
            let sig = client.send_and_confirm_transaction(&tx)?;
            signature = sig.to_string();
        }

        // After send: fetch post-send balances and transaction fee via getTransaction
        // Give RPC a short moment to index the tx
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let post_sol_lamports = client.get_balance(&user_pubkey)?;
        let mut post_token_amount: u64 = 0;
        if let Ok(Some(acc)) = find_token_account_owned_by_owner(mint, &user_pubkey.to_string(), rpc_client, settings).await {
            if let Ok(pk) = Pubkey::from_str(&acc) {
                if let Ok(bal) = client.get_token_account_balance(&pk) {
                    if let Ok(v) = bal.amount.parse::<u64>() { post_token_amount = v; }
                }
            }
        }

        // Compute deltas
        let sol_delta_lamports: i128 = post_sol_lamports as i128 - pre_sol_lamports as i128;
        let tokens_delta: i128 = pre_token_amount as i128 - post_token_amount as i128; // tokens sold

        // Fetch transaction meta to get exact fee if available
        let mut tx_fee_lamports: u64 = 0;
        for _ in 0..4 {
            let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "getTransaction", "params": [ signature, { "encoding": "jsonParsed", "commitment": "confirmed", "maxSupportedTransactionVersion": 0 } ] });
            match fetch_with_fallback::<Value>(req, "getTransaction", rpc_client, settings).await {
                Ok(resp) => {
                    if let Some(r) = resp.result {
                        if let Some(meta) = r.get("meta") {
                            if let Some(fee) = meta.get("fee").and_then(|v| v.as_u64()) {
                                tx_fee_lamports = fee;
                            }
                        }
                        break;
                    }
                }
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    continue;
                }
            }
        }

        // Expected dev fee (approx) from configured percent on sol_received
        let sol_received_lamports = (sol_received * 1_000_000_000.0) as u64;
        let expected_dev_fee = crate::dev_fee::calculate_dev_fee(sol_received_lamports);

        // Log exact accounting
        info!("Sell accounting for {}: tokens_sold={} (base units), sol_delta={} lamports ({} SOL)", mint, tokens_delta, sol_delta_lamports, (sol_delta_lamports as f64)/1_000_000_000.0);
        info!("Transaction fee: {} lamports ({} SOL), expected dev fee: {} lamports ({} SOL)", tx_fee_lamports, (tx_fee_lamports as f64)/1_000_000_000.0, expected_dev_fee, (expected_dev_fee as f64)/1_000_000_000.0);
        let net_lamports = sol_delta_lamports - tx_fee_lamports as i128 - expected_dev_fee as i128;
        info!("Net PnL (approx) for {}: {} lamports ({} SOL)", mint, net_lamports, (net_lamports as f64)/1_000_000_000.0);
    } else {
        // Dry-run simulation: construct same instruction and simulate it using
        // either the provided simulate_keypair or an ephemeral Keypair fallback.
        let mut _maybe_owned_sim: Option<Keypair> = None;
                let sim_payer_ref: &Keypair = if let Some(k) = simulate_keypair {
                    k
                } else {
                    _maybe_owned_sim = Some(Keypair::new());
                    _maybe_owned_sim.as_ref().ok_or_else(|| Box::<dyn std::error::Error + Send + Sync>::from("Failed to get sim keypair ref"))?
                };
        let sim_payer_pubkey = sim_payer_ref.pubkey();
        let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
        // Calculate min_sol_output with slippage
        let min_sol_output = (amount as f64 * current_price * 1000.0) as u64;
        let slippage_multiplier = 1.0 - (settings.slippage_bps as f64 / 10000.0);
        let min_sol_with_slippage = (min_sol_output as f64 * slippage_multiplier) as u64;
        let instruction = build_sell_instruction(
            &program_id,
            mint,
            amount,
            min_sol_with_slippage,
            &sim_payer_pubkey,
            &fee_recipient,
            creator_opt,
            settings,
        )?;
        debug!("Preparing simulated sell TX for mint {} amount {} tokens (dry run)", mint, amount);
        
        // Debug: log instruction details before simulation
        debug!("DRY RUN sell simulation for {}: program_id={}", mint, instruction.program_id);
        debug!("  Instruction has {} accounts:", instruction.accounts.len());
        for (i, acc) in instruction.accounts.iter().enumerate() {
            debug!("    [{}] {} (signer={}, writable={})", i, acc.pubkey, acc.is_signer, acc.is_writable);
        }
        debug!("  Instruction data length: {} bytes", instruction.data.len());
        debug!("  Payer (sim wallet): {}", sim_payer_pubkey);
        
        let mut tx = Transaction::new_with_payer(std::slice::from_ref(&instruction), Some(&sim_payer_pubkey));
        match client.get_latest_blockhash() {
            Ok(blockhash) => {
                // For dry-run simulation with ephemeral keypair:
                // We build the transaction correctly but cannot fully simulate because
                // the ephemeral keypair has no SOL and its ATAs don't exist on-chain.
                // This is expected - the transaction building itself validates the logic.
                tx.message.recent_blockhash = blockhash;
                // Sign the transaction with the simulate payer to produce a signed tx for remote simulation
                tx.sign(&[sim_payer_ref], blockhash);
                match bincode::serialize(&tx) {
                    Ok(serialized) => {
                        let tx_base64 = base64::engine::general_purpose::STANDARD.encode(&serialized);
                        match crate::helius_sender::simulate_transaction_via_helius(&tx_base64, settings).await {
                            Ok(json) => {
                                if let Some(err) = json.get("error") {
                                    warn!("DRY RUN sell simulation error for {}: {}", mint, err);
                                } else {
                                    info!("DRY RUN sell simulation completed for {} (helius)", mint);
                                }
                            }
                            Err(e) => warn!("DRY RUN sell simulation (helius) failed for {}: {}", mint, e),
                        }
                    }
                    Err(e) => warn!("Failed to serialize TX for dry-run sell simulate: {}", e),
                }
            }
            Err(e) => warn!("DRY RUN cannot get latest blockhash for sell {}: {}", mint, e),
        }
    }

    // Complete the function by returning success
    Ok(())
}

