use crate::{core::models::BondingCurveState, config::settings::Settings};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use log::{info, warn, error, debug};
use mpl_token_metadata::accounts::Metadata;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;
use crate::rpc_client::RpcClient as CoreRpcClient;
use crate::idl::load_all_idls;
use solana_program::pubkey::Pubkey;
use spl_associated_token_account::instruction::create_associated_token_account;
use crate::core::models::{PriceCache, RpcResponse};
use std::str::FromStr;
use tokio::sync::Mutex;
use std::collections::HashMap;

// Many of the helper functions originally in top-level `src/rpc.rs` were moved here
// to centralize logic for both wasm and native builds.

// Example: fetch_transaction_details function moved here.

pub async fn fetch_transaction_details(
    signature: &str,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<(String, String, String, String), Box<dyn std::error::Error + Send + Sync>> {
    // Implementation moved from top-level rpc.rs; kept intact but updated to use `crate` imports.
    let mut attempts = 0u8;
    let data_value: serde_json::Value = loop {
        attempts += 1;
        let resp: Result<RpcResponse<Value>, Box<dyn std::error::Error + Send + Sync>> = crate::rpc::fetch_with_fallback::<Value>(
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "getTransaction",
                "params": [ signature, { "encoding": "json", "commitment": "confirmed", "maxSupportedTransactionVersion": 0 } ]
            }),
            "getTransaction",
            rpc_client,
            settings,
        ).await;

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
    debug!("Transaction JSON (debug only): {}", data_value);

    // ... Keep the rest of the function as in top-level, adjusted to `crate` types ...

    unimplemented!("This function should be implemented in core::rpc_helpers based on the original top-level code.");
}

pub async fn fetch_token_metadata(
    mint: &str,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<(Option<Metadata>, Option<crate::models::OffchainMetadata>, Option<Vec<u8>>), Box<dyn std::error::Error + Send + Sync>> {
    let metadata_program_pk = Pubkey::from_str(&settings.metadata_program)?;
    let mint_pk = Pubkey::from_str(mint)?;
    let metadata_pda = Pubkey::find_program_address(
        &[b"metadata", metadata_program_pk.as_ref(), mint_pk.as_ref()],
        &metadata_program_pk,
    )
    .0;
    debug!("Fetching token metadata for mint {} -> metadata PDA {}", mint, metadata_pda);
    let data: RpcResponse<Value> = crate::rpc::fetch_with_fallback::<Value>(
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
        let account_obj = if let Some(v) = r.get("value") { v.clone() } else { r.clone() };
        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
            match Base64Engine.decode(base64_str) {
                Ok(decoded) => match Metadata::safe_deserialize(&decoded) {
                    Ok(meta) => {
                        let uri = meta.uri.trim_end_matches('\u{0}').to_string();
                        if !uri.is_empty() && (uri.starts_with("http://") || uri.starts_with("https://")) {
                            #[cfg(feature = "native-rpc")]
                            {
                                let client = reqwest::Client::new();
                                match client.get(&uri).send().await {
                                    Ok(resp) => match resp.text().await {
                                        Ok(body) => match serde_json::from_str::<crate::models::OffchainMetadata>(&body) {
                                            Ok(off) => {
                                                debug!("Fetched off-chain metadata for {}: {:?}", mint, off);
                                                Ok((Some(meta), Some(off), Some(decoded)))
                                            }
                                            Err(e) => {
                                                warn!("Failed to parse off-chain metadata JSON for {}: {}", uri, e);
                                                Ok((Some(meta), None, Some(decoded)))
                                            }
                                        },
                                        Err(e) => { warn!("Failed to read off-chain metadata body for {}: {}", uri, e); Ok((Some(meta), None, Some(decoded))) }
                                    },
                                    Err(e) => { warn!("HTTP error fetching off-chain metadata {}: {}", uri, e); Ok((Some(meta), None, Some(decoded))) }
                                }
                            }
                            #[cfg(not(feature = "native-rpc"))]
                            {
                                // When native-rpc feature is not available (e.g., wasm), skip fetching off-chain metadata.
                                debug!("Skipping off-chain metadata HTTP fetch for {} because native-rpc feature is disabled.", mint);
                                Ok((Some(meta), None, Some(decoded)))
                            }
                        } else {
                            Ok((Some(meta), None, Some(decoded)))
                        }
                    }
                    Err(e) => { error!("Failed to deserialize metadata for mint {}: {}", mint, e); Ok((None, None, Some(decoded))) }
                },
                Err(e) => { error!("Base64 decode error for metadata PDA {} mint {}: {}", metadata_pda, mint, e); Ok((None, None, None)) }
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

pub async fn fetch_current_price(
    mint: &str,
    price_cache: &Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<dyn CoreRpcClient>,
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
    let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pubkey.as_ref()], &pump_program);
    debug!("Fetching bonding curve account for mint {} -> curve PDA {}", mint, curve_pda);
    let commitments = ["finalized", "confirmed", "processed"];
    let mut last_err: Option<String> = None;
    let mut decoded_opt: Option<Vec<u8>> = None;
    for c in &commitments {
        for attempt in 0..3 {
            let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo", "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": c } ] });
            match crate::rpc::fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await {
                Ok(data) => {
                    if let Some(result_val) = data.result {
                        let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
                            match Base64Engine.decode(base64_str) {
                                Ok(decoded) => { decoded_opt = Some(decoded); break; }
                                Err(e) => { last_err = Some(format!("Decode error for bonding curve {} mint {}: {}", curve_pda, mint, e)); }
                            }
                        } else { last_err = Some(format!("No data field in account object for curve PDA {} at commitment {} (attempt {})", curve_pda, c, attempt)); }
                    } else { last_err = Some(format!("getAccountInfo returned no result for curve PDA {} at commitment {} (attempt {})", curve_pda, c, attempt)); }
                }
                Err(e) => { last_err = Some(format!("RPC error fetching curve PDA {} at commitment {} (attempt {}): {}", curve_pda, c, attempt, e)); }
            }
            tokio::time::sleep(std::time::Duration::from_millis(150 * (attempt as u64 + 1))).await;
        }
        if decoded_opt.is_some() { break; }
    }
    if decoded_opt.is_none() {
        if let Ok(Some(found_curve)) = crate::rpc::find_curve_account_by_mint(mint, rpc_client, settings).await {
            let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo", "params": [ found_curve, { "encoding": "base64", "commitment": "confirmed" } ] });
            if let Ok(data) = crate::rpc::fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await {
                if let Some(result_val) = data.result { let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() }; if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) { if let Ok(decoded) = Base64Engine.decode(base64_str) { decoded_opt = Some(decoded); } } }
            }
        }
    }
    if decoded_opt.is_none() {
        for http in &settings.solana_rpc_urls {
            #[cfg(not(feature = "native-rpc"))]
            {
                // If native RPC support is disabled, skip an HTTP client fallback
                debug!("Skipping HTTP RPC endpoint fallback because native-rpc feature is disabled");
                break;
            }
            #[cfg(feature = "native-rpc")]
            {
                let client = reqwest::Client::new();
                let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo", "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "finalized" } ] });
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
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("JSON parse error from {}: {} -- body: {}", http, e, text);
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        debug!("HTTP endpoint failure {}: {}", http, e);
                        continue;
                    }
                }
            }
        }
    }
    let decoded = match decoded_opt { Some(d) => d, None => return Err(last_err.unwrap_or_else(|| "Failed to fetch bonding curve state".to_string()).into()), };
    let slice = &decoded[..];
    if slice.len() < 41 { return Err("Bonding curve data too short".into()); }
    let virtual_token_reserves = u64::from_le_bytes(slice[0..8].try_into().map_err(|_| "Failed to parse virtual_token_reserves")?);
    let virtual_sol_reserves = u64::from_le_bytes(slice[8..16].try_into().map_err(|_| "Failed to parse virtual_sol_reserves")?);
    let real_token_reserves = u64::from_le_bytes(slice[16..24].try_into().map_err(|_| "Failed to parse real_token_reserves")?);
    let real_sol_reserves = u64::from_le_bytes(slice[24..32].try_into().map_err(|_| "Failed to parse real_sol_reserves")?);
    let token_total_supply = u64::from_le_bytes(slice[32..40].try_into().map_err(|_| "Failed to parse token_total_supply")?);
    let complete = slice[40] != 0;
    let creator = if slice.len() >= 73 { Pubkey::try_from(&slice[41..73]).ok().map(|p| p.to_string()) } else { None };
    let state = BondingCurveState { virtual_token_reserves, virtual_sol_reserves, real_token_reserves, real_sol_reserves, token_total_supply, complete, creator };
    if state.complete { return Err(format!("Token {} migrated to Raydium", mint).into()); }
    let token_reserves = state.virtual_token_reserves as f64; let lamports_reserves = state.virtual_sol_reserves as f64; if token_reserves > 0.0 { let price = (lamports_reserves / 1_000_000_000.0) / (token_reserves / 1_000_000.0); cache.put(mint.to_string(), (Instant::now(), price)); Ok(price) } else { return Err(format!("Invalid reserves for {}: zero tokens", mint).into()); }
}

pub async fn find_curve_account_by_mint(
    mint: &str,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    let pump_program = &settings.pump_fun_program;
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
    match crate::rpc::fetch_with_fallback::<Value>(request, "getProgramAccountsV2", rpc_client, settings).await {
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
            debug!("getProgramAccountsV2 not supported or failed for mint {}: {}", mint, e);
            Ok(None)
        }
    }
}

/// Fetch the bonding curve state for a given mint, returning a BondingCurveState
pub async fn fetch_bonding_curve_state(
    mint: &str,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<BondingCurveState, Box<dyn std::error::Error + Send + Sync>> {
    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let mint_pubkey = Pubkey::from_str(mint)?;
    let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pubkey.as_ref()], &pump_program);
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo", "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ] });
    let data = crate::rpc::fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await?;
    if let Some(result_val) = data.result {
        let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
            let decoded = Base64Engine.decode(base64_str)?;
            const PUMP_CURVE_DISCRIMINATOR: [u8; 8] = [0x17, 0xb7, 0xf8, 0x37, 0x60, 0xd8, 0xac, 0x60];
            if decoded.len() >= 49 && decoded[..8] == PUMP_CURVE_DISCRIMINATOR {
                let slice = &decoded[8..];
                let creator_pk = if slice.len() >= 73 { Pubkey::try_from(&slice[41..73]).ok() } else { None };
                let creator = creator_pk.map(|p| p.to_string());
                let state = BondingCurveState {
                    virtual_token_reserves: u64::from_le_bytes(slice[0..8].try_into().map_err(|_| "Failed to read virtual_token_reserves")?),
                    virtual_sol_reserves: u64::from_le_bytes(slice[8..16].try_into().map_err(|_| "Failed to read virtual_sol_reserves")?),
                    real_token_reserves: u64::from_le_bytes(slice[16..24].try_into().map_err(|_| "Failed to read real_token_reserves")?),
                    real_sol_reserves: u64::from_le_bytes(slice[24..32].try_into().map_err(|_| "Failed to read real_sol_reserves")?),
                    token_total_supply: u64::from_le_bytes(slice[32..40].try_into().map_err(|_| "Failed to read token_total_supply")?),
                    complete: slice[40] != 0,
                    creator,
                };
                return Ok(state);
            }
        }
    }
    Err("Failed to fetch bonding curve state".into())
}

pub async fn detect_idl_for_mint(mint: &str, rpc_client: &Arc<dyn CoreRpcClient>, settings: &Arc<Settings>) -> Option<crate::idl::SimpleIdl> {
    let idls = match load_all_idls() {
        Ok(m) => m,
        Err(e) => { log::debug!("Failed to load IDLs: {}", e); return None; }
    };
    if idls.is_empty() { return None; }
    let mint_pk = match Pubkey::from_str(mint) { Ok(m) => m, Err(_) => return None };
    for (_k, idl) in idls.into_iter() {
        let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &idl.address);
        let req = json!({"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":[curve_pda.to_string(), {"encoding":"base64","commitment":"confirmed"}]});
        if let Ok(resp) = crate::rpc::fetch_with_fallback::<Value>(req, "getAccountInfo", rpc_client, settings).await {
            if resp.result.is_some() { return Some(idl); }
        }
    }
    None
}

pub async fn build_missing_ata_preinstructions(
    context: &HashMap<String, Pubkey>,
) -> Result<Vec<solana_program::instruction::Instruction>, Box<dyn std::error::Error + Send + Sync>> {
    let mut pre: Vec<solana_program::instruction::Instruction> = Vec::new();
    let mut candidates: Vec<(Pubkey, Pubkey)> = Vec::new();
    let mint_pk = if let Some(m) = context.get("mint") { *m } else { return Ok(pre); };
    if let Some(user) = context.get("user") { candidates.push((*user, mint_pk)); }
    for (owner, mint) in candidates.iter() {
        let payer = context.get("payer").cloned().unwrap_or(*owner);
        pre.push(create_associated_token_account(&payer, owner, mint, &spl_token::id()));
        debug!("Adding create ATA instruction for owner {} mint {}", owner, mint);
    }
    Ok(pre)
}

pub async fn fetch_global_fee_recipient(rpc_client: &Arc<dyn CoreRpcClient>, settings: &Arc<Settings>) -> Result<Pubkey, Box<dyn std::error::Error + Send + Sync>> {
    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &pump_program);
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo", "params": [ global_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ] });
    let data = crate::rpc::fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await?;
    if let Some(result_val) = data.result {
        let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
        if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
            let decoded = Base64Engine.decode(base64_str)?;
            const GLOBAL_DISCRIMINATOR: [u8; 8] = [0xa7, 0xe8, 0xe8, 0xb1, 0xc8, 0x6c, 0x72, 0x7f];
            if decoded.len() >= 73 && decoded[..8] == GLOBAL_DISCRIMINATOR {
                let slice = &decoded[8..];
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

pub async fn fetch_bonding_curve_creator(mint: &str, rpc_client: &Arc<dyn CoreRpcClient>, settings: &Arc<Settings>) -> Result<Option<Pubkey>, Box<dyn std::error::Error + Send + Sync>> {
    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let mint_pk = Pubkey::from_str(mint)?;
    let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo", "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ] });
    match crate::rpc::fetch_with_fallback::<Value>(request, "getAccountInfo", rpc_client, settings).await {
        Ok(resp) => {
            if let Some(result_val) = resp.result {
                let account_obj = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                if let Some(base64_str) = account_obj.get("data").and_then(|d| d.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
                    if let Ok(decoded) = Base64Engine.decode(base64_str) {
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
        Err(e) => { warn!("Failed to fetch curve PDA for creator extraction: {}", e); Ok(None) }
    }
}
