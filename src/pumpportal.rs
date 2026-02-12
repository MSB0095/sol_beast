use crate::settings::Settings;
use futures_util::{stream::StreamExt, SinkExt};
use log::{debug, error, info};
use serde_json::{json, Value};
use std::sync::Arc;
use std::str::FromStr;
use tokio::sync::mpsc;
use solana_program::pubkey::Pubkey;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Minimal PumpPortal websocket client.
/// Connects to the given `wss_url`, subscribes to new-token events and forwards
/// a normalized JSON string into `tx` compatible with existing Solana WSS
/// notification shape so the rest of the pipeline can reuse `process_message`.
pub async fn run_pumpportal_ws(
    wss_url: &str,
    tx: mpsc::Sender<String>,
    _settings: Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        info!("Connecting to PumpPortal WSS {}", wss_url);
        let (ws_stream, _) = match connect_async(wss_url).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to connect to PumpPortal {}: {}. Retrying in 2s", wss_url, e);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to new token creation events
        let sub_payload = json!({ "method": "subscribeNewToken" }).to_string();
        if let Err(e) = write.send(Message::Text(sub_payload)).await {
            error!("Failed to send subscribeNewToken: {}", e);
        } else {
            info!("Subscribed to PumpPortal new-token stream");
        }

        // Read loop
        loop {
            let msg = match read.next().await {
                Some(Ok(m)) => m,
                Some(Err(e)) => {
                    error!("PumpPortal read error: {}", e);
                    break;
                }
                None => {
                    error!("PumpPortal stream ended");
                    break;
                }
            };

            let text = match msg {
                Message::Text(t) => t,
                Message::Binary(b) => match String::from_utf8(b) {
                    Ok(s) => s,
                    Err(e) => {
                        debug!("PumpPortal binary->utf8 error: {}", e);
                        continue;
                    }
                },
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => { error!("PumpPortal close frame"); break; },
                Message::Frame(_) => continue,
            };

            // Log raw incoming PumpPortal message (trimmed) to observe real format
            info!("PumpPortal raw: {}", text.chars().take(200).collect::<String>());

            let v: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    debug!("PumpPortal JSON parse error: {}", e);
                    continue;
                }
            };
            // Extract common fields PumpPortal provides when available
            let sig_opt = v.get("tx_signature")
                .and_then(|s| s.as_str())
                .or_else(|| v.get("signature").and_then(|s| s.as_str()))
                .or_else(|| v.get("txSig").and_then(|s| s.as_str()))
                .map(|s| s.to_string());

            // Build a normalized pumpportal object with tolerant parsing for field name variants.
            let mut pumpobj = serde_json::Map::new();

            // Helper closures
            let get_str = |obj: &Value, keys: &[&str]| -> Option<String> {
                for k in keys {
                    if let Some(vv) = obj.get(*k) {
                        if let Some(s) = vv.as_str() { return Some(s.to_string()); }
                        // sometimes PumpPortal emits numbers as strings
                        if let Some(n) = vv.as_u64() { return Some(n.to_string()); }
                    }
                }
                None
            };

            let get_num_u64 = |obj: &Value, keys: &[&str]| -> Option<u64> {
                for k in keys {
                    if let Some(vv) = obj.get(*k) {
                        if let Some(n) = vv.as_u64() { return Some(n); }
                        if let Some(s) = vv.as_str() { if let Ok(p) = s.parse::<u64>() { return Some(p); } }
                        if let Some(f) = vv.as_f64() { return Some(f as u64); }
                    }
                }
                None
            };



            // Mint: tolerate noisy PumpPortal values and trim whitespace
            if let Some(raw_mint) = get_str(&v, &["mint", "mintAddress", "tokenMint", "mintAddr", "mint_addr", "mintAddrStr", "mintPubkey", "mint_pubkey", "token_mint"]) {
                // Normalize: trim whitespace first
                let mut mint = raw_mint.trim().to_string();
                // strip trailing non-alphanumeric characters
                while mint.ends_with(|c: char| !c.is_ascii_alphanumeric()) { mint.pop(); }

                // First, try the mint as-is (some valid pubkeys can end with "pump")
                if Pubkey::from_str(&mint).is_ok() {
                    pumpobj.insert("mint".to_string(), Value::String(mint));
                } else {
                    // If invalid, try a single trailing "pump" trim (case-insensitive)
                    let mut trimmed = mint.clone();
                    let low = trimmed.to_lowercase();
                    if low.ends_with("pump") {
                        trimmed.truncate(trimmed.len() - 4);
                        trimmed = trimmed.trim().to_string();
                        while trimmed.ends_with(|c: char| !c.is_ascii_alphanumeric()) { trimmed.pop(); }
                    }

                    if Pubkey::from_str(&trimmed).is_ok() {
                        pumpobj.insert("mint".to_string(), Value::String(trimmed));
                    } else {
                        debug!("PumpPortal mint is not valid pubkey, skipping: {}", mint);
                    }
                }
            }

            // Creator / trader public key
            if let Some(creator) = get_str(&v, &["traderPublicKey", "creator", "creatorPubkey", "creatorAddress", "trader"]) {
                pumpobj.insert("creator".to_string(), Value::String(creator));
            }

            // Bonding curve identifier
            if let Some(curve) = get_str(&v, &["bondingCurveKey", "bonding_curve", "bondingCurve", "bondingCurvePDA", "bonding_curve_pda", "curve"]) {
                pumpobj.insert("bonding_curve".to_string(), Value::String(curve));
            }

            // Metadata: PumpPortal sometimes provides a nested `metadata`, or top-level name/symbol/uri
            let mut meta_map = serde_json::Map::new();
            if let Some(m) = v.get("metadata") {
                if m.is_object() {
                    for (k, val) in m.as_object().unwrap().iter() {
                        meta_map.insert(k.clone(), val.clone());
                    }
                }
            }
            // Top-level variants
            if let Some(name) = get_str(&v, &["name", "tokenName"]) {
                meta_map.insert("name".to_string(), Value::String(name));
            }
            if let Some(symbol) = get_str(&v, &["symbol", "tokenSymbol"]) {
                meta_map.insert("symbol".to_string(), Value::String(symbol));
            }
            if let Some(uri) = get_str(&v, &["uri", "metadataUri", "uriStr", "tokenUri"]) {
                meta_map.insert("uri".to_string(), Value::String(uri));
            }
            if let Some(image) = get_str(&v, &["image", "imageUrl"]) {
                meta_map.insert("image".to_string(), Value::String(image));
            }
            // Additional useful fields from PumpPortal events
            if let Some(mcap) = get_str(&v, &["marketCapSol", "market_cap_sol"]) {
                meta_map.insert("marketCapSol".to_string(), Value::String(mcap));
            }
            if let Some(is_mayhem) = v.get("is_mayhem_mode").and_then(|b| b.as_bool()) {
                meta_map.insert("is_mayhem_mode".to_string(), Value::Bool(is_mayhem));
            }
            if let Some(pool) = v.get("pool") {
                meta_map.insert("pool".to_string(), pool.clone());
            }

            if !meta_map.is_empty() {
                pumpobj.insert("metadata".to_string(), Value::Object(meta_map));
            }

            // Bonding state / reserves normalization
            // Accept multiple naming conventions and normalize to virtual_token_reserves / virtual_sol_reserves
            let mut bstate_map = serde_json::Map::new();
            // Robust parsing for vTokens: PumpPortal sends token counts in human-readable
            // units (e.g. 1_073_000_000 for ~1.073B tokens), NOT in base units (which would
            // be 1_073_000_000_000_000 for a 6-decimal token).  We convert to base units
            // (multiply by 1e6) the same way we convert vSol from SOL to lamports (×1e9).
            // pump.fun tokens always have 6 decimals.
            if let Some(vv) = v.get("vTokensInBondingCurve").or_else(|| v.get("v_tokens_in_bonding_curve")).or_else(|| v.get("v_tokens")).or_else(|| v.get("virtual_token_reserves")).or_else(|| v.get("vTokens")) {
                let vtok_base_opt: Option<u64> = match vv {
                    Value::Number(n) => {
                        if let Some(f) = n.as_f64() {
                            // Float → always human-readable tokens, convert to base units
                            Some((f * 1_000_000.0).round() as u64)
                        } else if let Some(u) = n.as_u64() {
                            // Integer: if < 1e12, likely human-readable; if >= 1e12, already base units
                            if u < 1_000_000_000_000 {
                                Some(u * 1_000_000)
                            } else {
                                Some(u)
                            }
                        } else {
                            None
                        }
                    }
                    Value::String(s) => {
                        if s.contains('.') || s.to_lowercase().contains('e') {
                            s.parse::<f64>().ok().map(|f| (f * 1_000_000.0).round() as u64)
                        } else {
                            s.parse::<u64>().ok().map(|u| {
                                if u < 1_000_000_000_000 { u * 1_000_000 } else { u }
                            })
                        }
                    }
                    _ => None,
                };
                if let Some(vtok_base) = vtok_base_opt {
                    bstate_map.insert("virtual_token_reserves".to_string(), Value::Number(serde_json::Number::from(vtok_base)));
                }
            }
            // Parse mint decimals if PumpPortal provides them (avoid RPC lookup)
            if let Some(dec) = get_num_u64(&v, &["decimals", "mintDecimals", "mint_decimals", "tokenDecimals"]) {
                bstate_map.insert("decimals".to_string(), Value::Number(serde_json::Number::from(dec)));
            }
            // Robust parsing for vSol: decide whether value is SOL (float/string with decimal) or lamports (integer)
            if let Some(vv) = v.get("vSolInBondingCurve").or_else(|| v.get("v_sol_in_bonding_curve")).or_else(|| v.get("v_sol")).or_else(|| v.get("virtual_sol_reserves")).or_else(|| v.get("vSol")) {
                let vsol_lamports_opt: Option<u64> = match vv {
                    Value::Number(n) => {
                        if n.is_f64() {
                            // treat as SOL float
                            n.as_f64().map(|f| (f * 1_000_000_000.0).round() as u64)
                        } else if let Some(u) = n.as_u64() {
                            // Integer: if < 1e9 (< 1 SOL in lamports), it's human-readable SOL;
                            // pump.fun virtual_sol_reserves always starts at ~30 SOL.
                            if u < 1_000_000_000 {
                                Some(u * 1_000_000_000)
                            } else {
                                Some(u)
                            }
                        } else {
                            None
                        }
                    }
                    Value::String(s) => {
                        if s.contains('.') || s.to_lowercase().contains('e') {
                            s.parse::<f64>().ok().map(|f| (f * 1_000_000_000.0).round() as u64)
                        } else {
                            s.parse::<u64>().ok().map(|u| {
                                if u < 1_000_000_000 { u * 1_000_000_000 } else { u }
                            })
                        }
                    }
                    _ => None,
                };
                if let Some(vsol_lamports) = vsol_lamports_opt {
                    bstate_map.insert("virtual_sol_reserves".to_string(), Value::Number(serde_json::Number::from(vsol_lamports)));
                }
            }
            // Complete / migrated flag
            if let Some(complete) = v.get("complete").and_then(|c| c.as_bool()).or_else(|| v.get("migrated").and_then(|c| c.as_bool())) {
                bstate_map.insert("complete".to_string(), Value::Bool(complete));
            }
            if !bstate_map.is_empty() {
                pumpobj.insert("bonding_state".to_string(), Value::Object(bstate_map));
            }

            // Skip non-pump.fun pool tokens (e.g. "bonk") early to avoid wasted RPC calls
            if let Some(meta) = pumpobj.get("metadata").and_then(|m| m.as_object()) {
                if let Some(pool) = meta.get("pool").and_then(|p| p.as_str()) {
                    if pool != "pump" {
                        debug!("Skipping non-pump.fun token (pool={})", pool);
                        continue;
                    }
                }
            }

            // Build normalized message
            let mut value_map = serde_json::Map::new();
            if let Some(sig) = sig_opt.clone() {
                value_map.insert("signature".to_string(), Value::String(sig));
            }
            // Clone pumpobj before inserting into value_map to avoid move
            value_map.insert("pumpportal".to_string(), Value::Object(pumpobj.clone()));

            // Log the normalized pumpportal object before sending
            info!("Sending PumpPortal object: {:?}", pumpobj);

            // Build the properly formatted message with params/result/value structure
            let out = json!({
                "params": {
                    "result": {
                        "value": Value::Object(value_map)
                    }
                }
            })
            .to_string();

            // Send the single properly-formatted message to the channel
            if let Err(e) = tx.send(out).await {
                error!("Failed to forward PumpPortal event into main channel: {}", e);
            }
        }

        // Reconnect after a short delay
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
