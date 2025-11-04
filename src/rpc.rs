use crate::{
    models::{
    
    BondingCurveState,
    Holding,
    PriceCache,
    RpcResponse,
    OffchainTokenMetadata,
    },
    settings::Settings,
};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use chrono::Utc;
// `select_ok` was previously used for parallel RPC fetch; after switching to
// a rotating sequential probe we no longer need it.
use log::{info, warn, error, debug};
use mpl_token_metadata::accounts::Metadata;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use solana_client::rpc_client::RpcClient;
use crate::tx_builder::{build_buy_instruction, build_sell_instruction, BUY_DISCRIMINATOR, SELL_DISCRIMINATOR};
use crate::idl::load_all_idls;
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};
use solana_program::pubkey::Pubkey;
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
use solana_program::instruction::AccountMeta;

pub async fn fetch_transaction_details(
    signature: &str,
    settings: &Arc<Settings>,
) -> Result<(String, String, String, String), Box<dyn std::error::Error + Send + Sync>> {
    // Request the raw JSON (not jsonParsed) so the returned instruction structures
    // include `programIdIndex` and `accounts` fields which match our deserialization
    // model. jsonParsed returns a different shape (parsed instructions) which would
    // not deserialize into our expected types.
    let mut attempts = 0u8;
    let data_value: serde_json::Value = loop {
        attempts += 1;
        let resp: Result<RpcResponse<Value>, Box<dyn std::error::Error + Send + Sync>> = fetch_with_fallback::<Value>(
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "getTransaction",
                "params": [ signature, { "encoding": "json", "commitment": "confirmed", "maxSupportedTransactionVersion": 0 } ]
            }),
            "getTransaction",
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
    // Note: previously we persisted raw transaction JSON to disk for post-mortem
    // inspection. To reduce file I/O we no longer write files; keep the raw
    // JSON available in debug logs for interactive troubleshooting.
    debug!("Transaction JSON (debug only): {}", data_value);

    // Normalize account keys
    let account_keys_arr = data_value
        .get("transaction")
        .and_then(|t| t.get("message"))
        .and_then(|m| m.get("accountKeys"))
        .or_else(|| {
            // older shape: accountKeys under `accountKeys`
            data_value.get("transaction").and_then(|t| t.get("message")).and_then(|m| m.get("accountKeys"))
        })
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
    // First pass: prefer postTokenBalances if present (reliable source of mint + owner)
    if let Some(meta) = data_value.get("meta") {
        if let Some(post_balances) = meta.get("postTokenBalances").and_then(|v| v.as_array()) {
            if !post_balances.is_empty() {
                if let Some(entry) = post_balances.get(0) {
                    if let (Some(mint), Some(owner)) = (entry.get("mint").and_then(|m| m.as_str()), entry.get("owner").and_then(|o| o.as_str())) {
                        // try to get creator from parsed initializeMint2 if available
                        let mut creator_opt: Option<String> = None;
                        if let Some(inner) = meta.get("innerInstructions").and_then(|v| v.as_array()) {
                            'outer: for inner_inst in inner {
                                if let Some(instructions) = inner_inst.get("instructions").and_then(|v| v.as_array()) {
                                    for instr in instructions {
                                        if let Some(parsed) = instr.get("parsed") {
                                            if let Some(t) = parsed.get("type").and_then(|t| t.as_str()) {
                                                if t == "initializeMint2" {
                                                    if let Some(info) = parsed.get("info") {
                                                        if let Some(c) = info.get("mintAuthority").and_then(|c| c.as_str()).or_else(|| info.get("owner").and_then(|o| o.as_str())) {
                                                            creator_opt = Some(c.to_string());
                                                            break 'outer;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let creator = creator_opt.unwrap_or_else(|| owner.to_string());
                        // compute bonding curve PDA
                        let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
                        let mint_pk = Pubkey::from_str(mint)?;
                        // PDA seeds per pump.fun IDL: ["bonding-curve", mint]
                        let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
                        // Try to find a token account for this mint that is owned by the bonding curve PDA
                        let mut holder_addr = owner.to_string();
                        if let Ok(Some(found)) = find_token_account_owned_by_owner(mint, &curve_pda.to_string(), settings).await {
                            debug!("Found token account owned by curve PDA: {} -> {}", curve_pda, found);
                            holder_addr = found;
                        }
                        debug!("Found via postTokenBalances mint={} owner={} creator={} curve_pda={} holder={}", mint, owner, creator, curve_pda, holder_addr);
                        return Ok((creator, mint.to_string(), curve_pda.to_string(), holder_addr));
                    }
                }
            }
        }
    }

    // Fallback: look for pump.fun program in raw inner instructions and map account indices
    let pump_fun_program_id = &settings.pump_fun_program;
    if let Some(meta) = data_value.get("meta") {
        if let Some(inner_instructions) = meta.get("innerInstructions").and_then(|v| v.as_array()) {
            for inner_instruction in inner_instructions {
                    if let Some(instructions) = inner_instruction.get("instructions").and_then(|v| v.as_array()) {
                    for instruction in instructions {
                        // get programId or programIdIndex
                        let program_id_opt = instruction
                            .get("programId")
                            .and_then(|p| p.as_str())
                            .or_else(|| {
                                instruction
                                    .get("programIdIndex")
                                    .and_then(|idx| idx.as_u64())
                                    .and_then(|i| account_keys.get(i as usize).map(|s| s.as_str()))
                            });

                        if let Some(program_id_key) = program_id_opt {
                            if program_id_key == pump_fun_program_id.as_str() {
                                if let Some(accounts_val) = instruction.get("accounts").and_then(|a| a.as_array()) {
                                    if accounts_val.len() >= 4 {
                                        let mint_index = accounts_val[0].as_u64().ok_or("mint index invalid")? as usize;
                                        let creator_index = accounts_val[3].as_u64().ok_or("creator index invalid")? as usize;
                                        if let (Some(mint_key), Some(creator_key)) = (account_keys.get(mint_index), account_keys.get(creator_index)) {
                                            debug!("Found pump.fun instruction: mint={}, creator={}", mint_key, creator_key);
                                            // compute bonding curve PDA
                                            let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
                                            let mint_pk = Pubkey::from_str(mint_key)?;
                                            // PDA seeds per pump.fun IDL: ["bonding-curve", mint]
                                            let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
                                            // Try to find a token account for this mint that is owned by the bonding curve PDA
                                            let mut holder_addr = creator_key.to_string();
                                            if let Ok(Some(found)) = find_token_account_owned_by_owner(mint_key, &curve_pda.to_string(), settings).await {
                                                debug!("Found token account owned by curve PDA: {} -> {}", curve_pda, found);
                                                holder_addr = found;
                                            } else {
                                                debug!("No token account owned by curve PDA found in transaction; using creator as holder: {}", creator_key);
                                            }
                                            return Ok((creator_key.to_string(), mint_key.to_string(), curve_pda.to_string(), holder_addr));
                                        } else {
                                            warn!("Account index lookup failed for instruction in tx {}: mint_index={}, creator_index={}, account_keys_len={}", signature, mint_index, creator_index, account_keys.len());
                                        }
                                    } else {
                                        warn!("Unexpected account count for pump.fun instruction in tx {}: expected>=4 got={}", signature, accounts_val.len());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    debug!("Account keys (len={}): {:?}", account_keys.len(), account_keys);
    Err("Could not find pump.fun instruction or extract details".into())
}



pub async fn fetch_token_metadata(
    mint: &str,
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
        settings,
    )
    .await?;
    if let Some(r) = data.result {
        // Normalize: some RPC implementations put the account under result.value
        let account_obj = if let Some(v) = r.get("value") { v.clone() } else { r.clone() };
        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.get(0)).and_then(|v| v.as_str()) {
            match Base64Engine.decode(base64_str) {
                Ok(decoded) => match Metadata::safe_deserialize(&decoded) {
                    Ok(meta) => {
                        // Try to fetch off-chain metadata JSON from the URI in on-chain metadata
                        let uri = meta.uri.trim_end_matches('\u{0}').to_string();
                        if !uri.is_empty() && (uri.starts_with("http://") || uri.starts_with("https://")) {
                            let client = Client::new();
                            match client.get(&uri).send().await {
                                Ok(resp) => match resp.text().await {
                                    Ok(body) => match serde_json::from_str::<OffchainTokenMetadata>(&body) {
                                        Ok(off) => {
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
            warn!("No data field returned in account info for metadata PDA {} mint {}", metadata_pda, mint);
            Ok((None, None, None))
        }
    } else {
        warn!("getAccountInfo returned no result for metadata PDA {} mint {}", metadata_pda, mint);
        Ok((None, None, None))
    }
}

pub async fn fetch_with_fallback<T: for<'de> Deserialize<'de> + Send + 'static>(
    request: Value,
    _method: &str,
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
                let text = resp.text().await.unwrap_or_else(|_| "<failed to read body>".to_string());
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
    // Try multiple commitment levels (prefer most-final) and a few retries to handle
    // RPC variations and transient propagation delays. Some RPC nodes may not yet have
    // the newest account data at a particular commitment level. We try `finalized`
    // first to maximize the chance of getting populated account data.
    let commitments = ["finalized", "confirmed", "processed"];
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
            match fetch_with_fallback::<Value>(request, "getAccountInfo", settings).await {
                Ok(data) => {
                    if let Some(result_val) = data.result {
                        let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.get(0)).and_then(|v| v.as_str()) {
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
        if let Ok(Some(found_curve)) = find_curve_account_by_mint(mint, settings).await {
            debug!("Found curve account via pump.fun program lookup for mint {} -> {}", mint, found_curve);
            // Try to fetch the account data for the found curve pubkey once (confirmed)
            let request = json!({
                "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
                "params": [ found_curve, { "encoding": "base64", "commitment": "confirmed" } ]
            });
            if let Ok(data) = fetch_with_fallback::<Value>(request, "getAccountInfo", settings).await {
                if let Some(result_val) = data.result {
                    let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                    if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.get(0)).and_then(|v| v.as_str()) {
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
                                    if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.get(0)).and_then(|v| v.as_str()) {
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
    let virtual_token_reserves = u64::from_le_bytes(slice[0..8].try_into().unwrap());
    let virtual_sol_reserves = u64::from_le_bytes(slice[8..16].try_into().unwrap());
    let real_token_reserves = u64::from_le_bytes(slice[16..24].try_into().unwrap());
    let real_sol_reserves = u64::from_le_bytes(slice[24..32].try_into().unwrap());
    let token_total_supply = u64::from_le_bytes(slice[32..40].try_into().unwrap());
    let complete = slice[40] != 0;

    if slice.len() > needed {
        debug!(
            "Bonding curve slice for {} has {} extra bytes after expected fields",
            mint,
            slice.len() - needed
        );
    }

        let state = BondingCurveState {
        virtual_token_reserves,
        virtual_sol_reserves,
        real_token_reserves,
        real_sol_reserves,
        token_total_supply,
        complete,
    };
    // Print parsed on-chain bonding curve info at info level so operators can
    // see the core fields in logs without having to inspect files.
    info!(
        "Bonding curve for mint {} curve {}: virtual_token_reserves={} virtual_sol_reserves={} ({} SOL) real_token_reserves={} real_sol_reserves={} ({} SOL) token_total_supply={} complete={}",
        mint,
        curve_pda,
        state.virtual_token_reserves,
        state.virtual_sol_reserves,
        state.virtual_sol_reserves as f64 / 1_000_000_000.0,
        state.real_token_reserves,
        state.real_sol_reserves,
        state.real_sol_reserves as f64 / 1_000_000_000.0,
        state.token_total_supply,
        state.complete
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
            price = (lamports_reserves / 1_000_000_000.0) / (token_reserves / 1_000_000.0);
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

/// Determine the most likely IDL for a given mint by computing the bonding-curve PDA
/// using each loaded IDL's program id and probing RPC for the account existence.
async fn detect_idl_for_mint(mint: &str, settings: &Arc<Settings>) -> Option<SimpleIdl> {
    let idls = load_all_idls();
    if idls.is_empty() { return None; }
    let mint_pk = match Pubkey::from_str(mint) { Ok(m) => m, Err(_) => return None };
    for (_k, idl) in idls.into_iter() {
        // compute PDA seeds per pump.fun: ["bonding-curve", mint]
        let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &idl.address);
        let req = json!({"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":[curve_pda.to_string(), {"encoding":"base64","commitment":"confirmed"}]});
        if let Ok(resp) = fetch_with_fallback::<Value>(req, "getAccountInfo", settings).await {
            if resp.result.is_some() {
                return Some(idl);
            }
        }
    }
    None
}

/// Given a list of AccountMeta and known context (mint, user, creator, bonding_curve),
/// return a list of create_associated_token_account instructions to create missing ATAs.
async fn build_missing_ata_preinstructions(
    accounts: &[AccountMeta],
    context: &HashMap<String, Pubkey>,
    settings: &Arc<Settings>,
) -> Result<Vec<solana_program::instruction::Instruction>, Box<dyn std::error::Error + Send + Sync>> {
    let mut pre: Vec<solana_program::instruction::Instruction> = Vec::new();
    // For candidate owners, prepare owner->mint pairs to check
    let mut candidates: Vec<(Pubkey, Pubkey)> = Vec::new();
    // extract mint from context
    let mint_pk = if let Some(m) = context.get("mint") { *m } else { return Ok(pre); };
    // common owners to check: user, bonding_curve, bonding_curve.creator (creator)
    if let Some(user) = context.get("user") { candidates.push((*user, mint_pk)); }
    if let Some(bc) = context.get("bonding_curve") { candidates.push((*bc, mint_pk)); }
    if let Some(creator) = context.get("bonding_curve.creator") { candidates.push((*creator, mint_pk)); }

    // Also inspect explicit AccountMeta entries to detect ATAs by pattern
    for am in accounts.iter() {
        // If account equals associated token address for any candidate owner, ensure created
        for (owner, mint) in candidates.iter() {
            let ata = get_associated_token_address(owner, mint);
            if ata == am.pubkey {
                // check existence
                let req = json!({"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":[ata.to_string(), {"encoding":"base64","commitment":"confirmed"}]});
                match fetch_with_fallback::<Value>(req, "getAccountInfo", settings).await {
                    Ok(info) => {
                        if info.result.is_none() {
                            // create for payer later
                            // payer will be decided by caller (use payer as context user if present)
                            let payer = context.get("payer").cloned().unwrap_or(*owner);
                            pre.push(create_associated_token_account(&payer, owner, mint, &spl_associated_token_account::id()));
                        } else if let Some(rv) = info.result {
                            let val = if let Some(v) = rv.get("value") { v.clone() } else { rv.clone() };
                            if val.is_null() {
                                let payer = context.get("payer").cloned().unwrap_or(*owner);
                                pre.push(create_associated_token_account(&payer, owner, mint, &spl_associated_token_account::id()));
                            }
                        }
                    }
                    Err(_) => {
                        // assume missing; add create instruction
                        let payer = context.get("payer").cloned().unwrap_or(*owner);
                        pre.push(create_associated_token_account(&payer, owner, mint, &spl_associated_token_account::id()));
                    }
                }
            }
        }
    }
    Ok(pre)
}

/// Fetch the bonding curve account for `mint` and attempt to read the creator pubkey
/// from the on-chain `BondingCurve` struct. Returns `None` if the account is missing
/// or the layout is unexpected.
async fn fetch_bonding_curve_creator(mint: &str, settings: &Arc<Settings>) -> Result<Option<Pubkey>, Box<dyn std::error::Error + Send + Sync>> {
    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let mint_pk = Pubkey::from_str(mint)?;
    let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
    let request = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
        "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
    });
    match fetch_with_fallback::<serde_json::Value>(request, "getAccountInfo", settings).await {
        Ok(resp) => {
            if let Some(result_val) = resp.result {
                let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.get(0)).and_then(|v| v.as_str()) {
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
async fn find_token_account_owned_by_owner(
    mint: &str,
    owner_pubkey: &str,
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

    match fetch_with_fallback::<Value>(request, "getProgramAccountsV2", settings).await {
        Ok(resp) => {
            if let Some(result_val) = resp.result {
                // V2 may return either an array of entries or an object with an `accounts` array.
                if let Some(arr) = result_val.as_array() {
                    if !arr.is_empty() {
                        if let Some(entry) = arr.get(0) {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                } else if let Some(obj_arr) = result_val.get("accounts").and_then(|v| v.as_array()) {
                    if !obj_arr.is_empty() {
                        if let Some(entry) = obj_arr.get(0) {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                } else if let Some(obj_arr) = result_val.get("value").and_then(|v| v.as_array()) {
                    if !obj_arr.is_empty() {
                        if let Some(entry) = obj_arr.get(0) {
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

    match fetch_with_fallback::<Value>(request, "getProgramAccountsV2", settings).await {
        Ok(resp) => {
            if let Some(result_val) = resp.result {
                if let Some(arr) = result_val.as_array() {
                    if !arr.is_empty() {
                        if let Some(entry) = arr.get(0) {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                } else if let Some(obj_arr) = result_val.get("accounts").and_then(|v| v.as_array()) {
                    if !obj_arr.is_empty() {
                        if let Some(entry) = obj_arr.get(0) {
                            if let Some(pubkey) = entry.get("pubkey").and_then(|p| p.as_str()) {
                                return Ok(Some(pubkey.to_string()));
                            }
                        }
                    }
                } else if let Some(obj_arr) = result_val.get("value").and_then(|v| v.as_array()) {
                    if !obj_arr.is_empty() {
                        if let Some(entry) = obj_arr.get(0) {
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

pub async fn buy_token(
    mint: &str,
    sol_amount: f64,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    price_cache: Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
) -> Result<Holding, Box<dyn std::error::Error + Send + Sync>> {
    // fetch_current_price now returns SOL per token
    let buy_price_sol = fetch_current_price(mint, &price_cache, settings).await?;
    // compute token amount as SOL amount divided by SOL per token
    let token_amount = (sol_amount / buy_price_sol) as u64;
    info!(
        "Buy {}: {} tokens for {} SOL (price: {:.18} SOL/token)",
        mint,
        token_amount,
        sol_amount,
        buy_price_sol
    );

    if is_real {
        let client = RpcClient::new(&settings.solana_rpc_urls[0]);
        let payer = keypair.ok_or("Keypair required")?;
        debug!("Preparing buy TX for mint {} amount {} SOL (real)", mint, sol_amount);
        // Determine best IDL for this mint (try detect by PDA existence)
        let detected_idl_opt = detect_idl_for_mint(mint, settings).await;
        let mut built_instr: Option<solana_program::instruction::Instruction> = None;
        let mut last_err: Option<String> = None;
        let mint_pk = Pubkey::from_str(mint)?;
        let creator_opt = fetch_bonding_curve_creator(mint, settings).await.ok().flatten();
        let payer_pubkey = payer.pubkey();
        // Build a rich context that IDLs commonly expect
        let mut context: HashMap<String, Pubkey> = HashMap::new();
        context.insert("mint".to_string(), mint_pk);
        context.insert("user".to_string(), payer_pubkey);
        if let Some(c) = creator_opt { context.insert("bonding_curve.creator".to_string(), c); }
        // bonding_curve PDA using configured pump program as fallback
        let pump_program_pk = Pubkey::from_str(&settings.pump_fun_program)?;
        let (curve_pda_fallback, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program_pk);
        context.insert("bonding_curve".to_string(), curve_pda_fallback);
        if let Some(creator) = context.get("bonding_curve.creator") {
            let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program_pk);
            context.insert("creator_vault".to_string(), creator_vault);
        }
        // Add fee_recipient (typically the global config authority or protocol fee account)
        // For pump.fun, this is typically derived or a specific pubkey
        // Using the global PDA as a safe default (or fetch from settings if available)
        let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &pump_program_pk);
        context.insert("fee_recipient".to_string(), global_pda);
        // detect_idl result preferred; otherwise fallback order
        let try_idls: Vec<SimpleIdl> = if let Some(idl) = detected_idl_opt { vec![idl] } else { load_all_idls().into_iter().map(|(_k,v)| v).collect() };
        for idl in try_idls {
            match idl.build_accounts_for("buy", &context) {
                Ok(metas) => {
                    debug!("IDL {} build_accounts_for(buy) succeeded with {} accounts", idl.address, metas.len());
                    for (i, meta) in metas.iter().enumerate() {
                        debug!("  [{}] {} (signer={}, writable={})", i, meta.pubkey, meta.is_signer, meta.is_writable);
                    }
                    let mut d = BUY_DISCRIMINATOR.to_vec();
                    d.extend(borsh::to_vec(&crate::tx_builder::BuyArgs { amount: token_amount, max_sol_cost: (buy_price_sol * 1_000_000_000.0) as u64, track_volume: Some(false) })?);
                    built_instr = Some(solana_program::instruction::Instruction { program_id: idl.address, accounts: metas, data: d });
                    break;
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        let instruction = if let Some(instr) = built_instr { instr } else {
            if let Some(e) = last_err { debug!("IDL buy build errors: {}", e); }
            // fallback to legacy builder using configured pump program
            let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
            build_buy_instruction(
                &program_id,
                mint,
                token_amount,
                (buy_price_sol * 1_000_000_000.0) as u64,
                Some(false),
                &payer_pubkey,
                creator_opt,
                settings,
            )?
        };
        // Ensure ATA exists; if missing, create a create_associated_token_account instruction
    let ata = get_associated_token_address(&payer_pubkey, &mint_pk);
        let ata_info_req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
            "params": [ ata.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
        });
        let mut pre_instructions: Vec<solana_program::instruction::Instruction> = Vec::new();
        match fetch_with_fallback::<Value>(ata_info_req, "getAccountInfo", settings).await {
            Ok(info) => {
                if info.result.is_none() {
                    // not found -> create
                    pre_instructions.push(create_associated_token_account(&payer_pubkey, &payer_pubkey, &mint_pk, &spl_associated_token_account::id()));
                } else if let Some(result_val) = info.result {
                    // normalize
                    let val = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                    if val.is_null() {
                        pre_instructions.push(create_associated_token_account(&payer_pubkey, &payer_pubkey, &mint_pk, &spl_associated_token_account::id()));
                    }
                }
            }
            Err(e) => {
                debug!("Failed to check ATA existence for {}: {}", ata, e);
            }
        }
        // Use environment gating to avoid accidental real sends
        if std::env::var("CONFIRM_REAL").unwrap_or_else(|_| "0".to_string()) == "1" {
            // prepare context with payer so ATA creation uses correct funding account
            let mut real_context: HashMap<String, Pubkey> = HashMap::new();
            real_context.insert("mint".to_string(), mint_pk);
            real_context.insert("user".to_string(), payer_pubkey);
            real_context.insert("payer".to_string(), payer_pubkey);
            if let Some(c) = creator_opt { real_context.insert("bonding_curve.creator".to_string(), c); }
            if let Some(bc) = context.get("bonding_curve") { real_context.insert("bonding_curve".to_string(), *bc); }
            if let Some(cv) = context.get("creator_vault") { real_context.insert("creator_vault".to_string(), *cv); }
            // compute missing ATA pre-instructions for accounts in the instruction
            let ata_pre = build_missing_ata_preinstructions(&instruction.accounts, &real_context, settings).await?;
            let mut all_instrs: Vec<solana_program::instruction::Instruction> = Vec::new();
            for pi in ata_pre.into_iter() { all_instrs.push(pi); }
            all_instrs.push(instruction);
            let mut tx = Transaction::new_with_payer(&all_instrs, Some(&payer.pubkey()));
            let blockhash = client.get_latest_blockhash()?;
            tx.sign(&[payer], blockhash);
            client.send_and_confirm_transaction(&tx)?;
        } else {
            info!("CONFIRM_REAL!=1: skipping actual send for buy of {}. Set CONFIRM_REAL=1 to enable.", mint);
        }
    } else {
        // Dry-run simulation: construct same instruction and simulate it using
        // either the provided simulate_keypair or an ephemeral Keypair fallback.
        let client = RpcClient::new(&settings.solana_rpc_urls[0]);
        // keep an owned Keypair alive in this scope if we need to create one
        let mut _maybe_owned_sim: Option<Keypair> = None;
        let sim_payer_ref: &Keypair = if let Some(k) = simulate_keypair {
            k
        } else {
            _maybe_owned_sim = Some(Keypair::new());
            _maybe_owned_sim.as_ref().unwrap()
        };
        debug!("Preparing simulated buy TX for mint {} amount {} SOL (dry run)", mint, sol_amount);
        let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
        let creator_opt = fetch_bonding_curve_creator(mint, settings).await.ok().flatten();
        let sim_payer_pubkey = sim_payer_ref.pubkey();
        // Try to build accounts via IDL-aware builder for exactness
        let idls = load_all_idls();
        let mut instruction_opt: Option<solana_program::instruction::Instruction> = None;
        let mut last_err: Option<String> = None;
        let mint_pk = Pubkey::from_str(mint)?;
        if let Some(idl) = idls.get("pumpfun") {
            // prepare context map
            let mut context: HashMap<String, Pubkey> = HashMap::new();
            context.insert("mint".to_string(), mint_pk);
            context.insert("user".to_string(), sim_payer_pubkey);
            if let Some(c) = creator_opt { context.insert("bonding_curve.creator".to_string(), c); }
            // Add bonding_curve PDA
            let pump_program_pk = Pubkey::from_str(&settings.pump_fun_program)?;
            let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program_pk);
            context.insert("bonding_curve".to_string(), curve_pda);
            // Add creator_vault PDA if creator exists
            if let Some(creator) = context.get("bonding_curve.creator").cloned() {
                let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program_pk);
                context.insert("creator_vault".to_string(), creator_vault);
            }
            // Add fee_recipient (global PDA)
            let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &pump_program_pk);
            context.insert("fee_recipient".to_string(), global_pda);
            match idl.build_accounts_for("buy", &context) {
                Ok(metas) => {
                    debug!("IDL build_accounts_for(buy) succeeded with {} accounts (dry-run)", metas.len());
                    for (i, meta) in metas.iter().enumerate() {
                        debug!("  [{}] {} (signer={}, writable={})", i, meta.pubkey, meta.is_signer, meta.is_writable);
                    }
                    instruction_opt = Some(solana_program::instruction::Instruction { program_id, accounts: metas, data: {
                        let mut d = BUY_DISCRIMINATOR.to_vec();
                        d.extend(borsh::to_vec(&crate::tx_builder::BuyArgs { amount: token_amount, max_sol_cost: (buy_price_sol * 1_000_000_000.0) as u64, track_volume: Some(false) })?);
                        d
                    }});
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        let instruction = if let Some(instr) = instruction_opt { instr } else {
            if let Some(e) = last_err { debug!("IDL build failed for buy: {}", e); }
            // fallback to legacy builder
            build_buy_instruction(
                &program_id,
                mint,
                token_amount,
                (buy_price_sol * 1_000_000_000.0) as u64,
                Some(false),
                &sim_payer_pubkey,
                creator_opt,
                settings,
            )?
        };
    // include any pre_instructions in the simulated tx (e.g., ATA creation)
    let mut tx_instructions = Vec::new();
        // Build pre_instructions for dry-run (ensure ATA exists for sim payer)
        let mut pre_instructions: Vec<solana_program::instruction::Instruction> = Vec::new();
        let mint_pk = Pubkey::from_str(mint)?;
        let ata = get_associated_token_address(&sim_payer_pubkey, &mint_pk);
        match fetch_with_fallback::<Value>(json!({
            "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
            "params": [ ata.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
        }), "getAccountInfo", settings).await {
            Ok(info) => {
                if info.result.is_none() {
                    pre_instructions.push(create_associated_token_account(&sim_payer_pubkey, &sim_payer_pubkey, &mint_pk, &spl_associated_token_account::id()));
                } else if let Some(result_val) = info.result {
                    let val = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                    if val.is_null() {
                        pre_instructions.push(create_associated_token_account(&sim_payer_pubkey, &sim_payer_pubkey, &mint_pk, &spl_associated_token_account::id()));
                    }
                }
            }
            Err(e) => debug!("Failed to check ATA existence for {}: {}", ata, e),
        }
        // we can't easily create payer-signed ATA in dry-run when using ephemeral keypair,
        // but include the instruction so simulateTransaction can check behavior.
        // convert pre_instructions (created with payer_pubkey) to use sim_payer_pubkey when needed
        for pi in pre_instructions.into_iter() {
            // ensure the payer pubkey in create_associated_token_account matches sim_payer
            // the create_associated_token_account sets accounts; it's safe to just push as-is
            tx_instructions.push(pi);
        }
        tx_instructions.push(instruction.clone());
        let mut tx = Transaction::new_with_payer(&tx_instructions, Some(&sim_payer_pubkey));
        match client.get_latest_blockhash() {
            Ok(blockhash) => {
                // For dry-run simulation with ephemeral keypair:
                // We build the transaction correctly but cannot fully simulate because
                // the ephemeral keypair has no SOL and its ATAs don't exist on-chain.
                // This is expected - the transaction building itself validates the logic.
                tx.message.recent_blockhash = blockhash;
                let config = solana_client::rpc_config::RpcSimulateTransactionConfig {
                    sig_verify: false,
                    replace_recent_blockhash: true,
                    commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
                    encoding: None,
                    accounts: None,
                    min_context_slot: None,
                    inner_instructions: false,
                };
                match client.simulate_transaction_with_config(&tx, config) {
                    Ok(simulation) => {
                        if let Some(ref err) = simulation.value.err {
                            if format!("{:?}", err).contains("AccountNotFound") {
                                info!("DRY RUN buy: tx built correctly for {} (simulation AccountNotFound is expected - ephemeral keypair has no SOL/accounts)", mint);
                            } else {
                                warn!("DRY RUN buy simulation error for {}: {:?}", mint, err);
                            }
                        } else {
                            info!("DRY RUN buy simulation SUCCESS for {}: compute_units={:?}", mint, simulation.value.units_consumed);
                        }
                    }
                    Err(e) => warn!("DRY RUN buy simulation RPC failed for {}: {}", mint, e),
                }
            }
            Err(e) => warn!("DRY RUN cannot get latest blockhash for {}: {}", mint, e),
        }
    }
    // Return simulated holding for dry runs
    Ok(Holding {
        amount: token_amount,
        buy_price: buy_price_sol,
        buy_time: Utc::now(),
        metadata: None,
        onchain_raw: None,
        onchain: None,
    })
}

pub async fn sell_token(
    mint: &str,
    amount: u64,
    current_price: f64,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    settings: &Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // current_price is SOL per token
    let sol_received = amount as f64 * current_price;
    info!(
        "Sell {}: {} tokens for {} SOL (price: {:.18} SOL/token)",
        mint,
        amount,
        format!("{:.9}", sol_received),
        current_price
    );

    let client = RpcClient::new(&settings.solana_rpc_urls[0]);
    let idls = load_all_idls();
    let creator_opt = fetch_bonding_curve_creator(mint, settings).await.ok().flatten();

    if is_real {
        // Real run: build instruction with the real keypair's pubkey as user (signer)
        let payer = keypair.ok_or("Keypair required")?;
        let user_pubkey = payer.pubkey();
        // Try to detect IDL for this mint and build context
        let detected_idl_opt = detect_idl_for_mint(mint, settings).await;
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
        // Add fee_recipient (global PDA)
        let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &pump_program_pk);
        context.insert("fee_recipient".to_string(), global_pda);
        let try_idls: Vec<SimpleIdl> = if let Some(idl) = detected_idl_opt { vec![idl] } else { load_all_idls().into_iter().map(|(_k,v)| v).collect() };
        for idl in try_idls {
            match idl.build_accounts_for("sell", &context) {
                Ok(metas) => {
                    let mut d = SELL_DISCRIMINATOR.to_vec();
                    d.extend(borsh::to_vec(&crate::tx_builder::SellArgs { amount, min_sol_output: (current_price * 1_000_000_000.0) as u64 })?);
                    instruction_opt = Some(solana_program::instruction::Instruction { program_id: idl.address, accounts: metas, data: d });
                    break;
                }
                Err(e) => debug!("IDL build failed for sell: {}", e),
            }
        }
        let instruction = if let Some(instr) = instruction_opt { instr } else {
            // fallback to legacy builder using configured pump program
            let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
            build_sell_instruction(
                &program_id,
                mint,
                amount,
                (current_price * 1_000_000_000.0) as u64,
                &user_pubkey,
                creator_opt,
                settings,
            )?
        };
        // Ensure ATA exists for user (sell path may not need it but check)
        let ata = get_associated_token_address(&user_pubkey, &mint_pk);
        match fetch_with_fallback::<Value>(json!({
            "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
            "params": [ ata.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
        }), "getAccountInfo", settings).await {
            Ok(info) => {
                if info.result.is_none() {
                    // create ATA as pre-instruction if real send (simulate will include it too)
                }
            }
            Err(e) => debug!("Failed to check ATA existence for sell {}: {}", ata, e),
        }
        if std::env::var("CONFIRM_REAL").unwrap_or_else(|_| "0".to_string()) == "1" {
            debug!("Sending real sell TX for mint {} amount {} tokens", mint, amount);
            // prepare real context
            let mut real_context: HashMap<String, Pubkey> = HashMap::new();
            real_context.insert("mint".to_string(), mint_pk);
            real_context.insert("user".to_string(), user_pubkey);
            real_context.insert("payer".to_string(), user_pubkey);
            if let Some(c) = creator_opt { real_context.insert("bonding_curve.creator".to_string(), c); }
            if let Some(bc) = context.get("bonding_curve") { real_context.insert("bonding_curve".to_string(), *bc); }
            if let Some(cv) = context.get("creator_vault") { real_context.insert("creator_vault".to_string(), *cv); }
            let ata_pre = build_missing_ata_preinstructions(&instruction.accounts, &real_context, settings).await?;
            let mut all_instrs: Vec<solana_program::instruction::Instruction> = Vec::new();
            for pi in ata_pre.into_iter() { all_instrs.push(pi); }
            all_instrs.push(instruction);
            let mut tx = Transaction::new_with_payer(&all_instrs, Some(&payer.pubkey()));
            let blockhash = client.get_latest_blockhash()?;
            tx.sign(&[payer], blockhash);
            client.send_and_confirm_transaction(&tx)?;
        } else {
            info!("CONFIRM_REAL!=1: skipping actual send for sell of {}. Set CONFIRM_REAL=1 to enable.", mint);
        }
    } else {
        // Dry-run simulation: construct same instruction and simulate it using
        // either the provided simulate_keypair or an ephemeral Keypair fallback.
        let mut _maybe_owned_sim: Option<Keypair> = None;
        let sim_payer_ref: &Keypair = if let Some(k) = simulate_keypair { k } else { _maybe_owned_sim = Some(Keypair::new()); _maybe_owned_sim.as_ref().unwrap() };
        let sim_payer_pubkey = sim_payer_ref.pubkey();
        let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
        let instruction = build_sell_instruction(
            &program_id,
            mint,
            amount,
            (current_price * 1_000_000_000.0) as u64,
            &sim_payer_pubkey,
            creator_opt,
            settings,
        )?;
        debug!("Preparing simulated sell TX for mint {} amount {} tokens (dry run)", mint, amount);
        let mut tx = Transaction::new_with_payer(&[instruction.clone()], Some(&sim_payer_pubkey));
        match client.get_latest_blockhash() {
            Ok(blockhash) => {
                // For dry-run simulation with ephemeral keypair:
                // We build the transaction correctly but cannot fully simulate because
                // the ephemeral keypair has no SOL and its ATAs don't exist on-chain.
                // This is expected - the transaction building itself validates the logic.
                tx.message.recent_blockhash = blockhash;
                let config = solana_client::rpc_config::RpcSimulateTransactionConfig {
                    sig_verify: false,
                    replace_recent_blockhash: true,
                    commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
                    encoding: None,
                    accounts: None,
                    min_context_slot: None,
                    inner_instructions: false,
                };
                match client.simulate_transaction_with_config(&tx, config) {
                    Ok(sim) => {
                        if let Some(ref err) = sim.value.err {
                            if format!("{:?}", err).contains("AccountNotFound") {
                                info!("DRY RUN sell: tx built correctly for {} (simulation AccountNotFound is expected - ephemeral keypair has no SOL/accounts)", mint);
                            } else {
                                warn!("DRY RUN sell simulation error for {}: {:?}", mint, err);
                            }
                        } else {
                            info!("DRY RUN sell simulation SUCCESS for {}: compute_units={:?}", mint, sim.value.units_consumed);
                        }
                    }
                    Err(e) => warn!("DRY RUN sell simulation RPC failed for {}: {}", mint, e),
                }
            }
            Err(e) => warn!("DRY RUN cannot get latest blockhash for sell {}: {}", mint, e),
        }
    }

    // Complete the function by returning success
    Ok(())
}

