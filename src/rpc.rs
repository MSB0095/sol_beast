use crate::settings::Settings;
use base64::engine::general_purpose::STANDARD as Base64Engine;
use base64::Engine;
use borsh::{BorshDeserialize, BorshSerialize};
use futures_util::future::select_ok;
use log::{info, error};
use mpl_token_metadata::accounts::Metadata;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use solana_program::pubkey::Pubkey;
use solana_sdk::{
    signature::Keypair,
};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use crate::{Holding, PriceCache};
use chrono::Utc;

pub const PRICE_CACHE_TTL: Duration = Duration::from_secs(300);

// Bonding Curve State
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct BondingCurveState {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub complete: bool,
    pub fee_basis_points: u16,
}

// RPC structures
#[derive(Deserialize)]
pub struct RpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<Value>,
}
#[derive(Deserialize)]
pub struct TransactionResult {
    pub transaction: TransactionData,
}
#[derive(Deserialize)]
pub struct TransactionData {
    pub message: MessageData,
}
#[derive(Deserialize)]
pub struct MessageData {
    #[serde(rename = "accountKeys")]
    pub account_keys: Vec<AccountKey>,
    pub instructions: Vec<Value>,
}
#[derive(Deserialize)]
pub struct AccountKey {
    pub pubkey: String,
}
#[derive(Deserialize)]
pub struct AccountInfoResult {
    #[serde(default)]
    pub value: Option<AccountData>,
}
#[derive(Deserialize)]
pub struct AccountData {
    pub data: Vec<String>,
}


pub async fn handle_new_token(
    signature: &str,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    is_real: bool,
    keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (_creator, mint) = fetch_transaction_details(signature, &settings).await?;
    info!("Handling token: {}", mint);

    if holdings.lock().await.contains_key(&mint) {
        info!("Already hold token {}", mint);
        return Ok(());
    }

    let metadata = fetch_token_metadata(&mint, &settings).await?;

    if let Some(meta) = &metadata {
        info!("Token {}: Name={}, Symbol={}, URI={}", mint, meta.name.trim(), meta.symbol.trim(), meta.uri.trim());
        if !meta.name.is_empty() && !meta.uri.is_empty() {
            let buy_amount_sol = 0.01; // Example: buy for 0.01 SOL
            buy_token(&mint, buy_amount_sol, is_real, keypair, holdings, price_cache, &settings).await?;
        }
    }
    else {
        info!("No metadata found for token {}", mint);
    }

    Ok(())
}

pub async fn fetch_transaction_details(signature: &str, settings: &Settings) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    let tx_data: RpcResponse<TransactionResult> = fetch_with_fallback(json!({
        "jsonrpc": "2.0", "id": 1, "method": "getTransaction",
        "params": [ signature, { "encoding": "jsonParsed", "commitment": "confirmed", "maxSupportedTransactionVersion": 0 } ]
    }), "getTransaction", settings).await?;

    let result = tx_data.result.ok_or("No transaction data")?;
    let instructions = &result.transaction.message.instructions;
    let pump_fun_program_id = &settings.pump_fun_program;

    for instruction_value in instructions {
        if let Some(instruction) = instruction_value.as_object() {
            if let Some(program_id) = instruction.get("programId").and_then(|v| v.as_str()) {
                if program_id == pump_fun_program_id {
                    // This is a pump.fun instruction.
                    // The log filter in ws.rs ensures this is a 'create' transaction.
                    // The mint is the first account.
                    if let Some(accounts) = instruction.get("accounts").and_then(|v| v.as_array()) {
                        if let Some(mint_value) = accounts.get(0) {
                            if let Some(mint) = mint_value.as_str() {
                                // Creator is the first account in the overall transaction, which is the fee payer and signer.
                                let creator = result.transaction.message.account_keys[0].pubkey.clone();
                                return Ok((creator, mint.to_string()));
                            }
                        }
                    }
                }
            }
        }
    }

    Err("Could not find pump.fun create instruction or mint account.".into())
}

pub async fn fetch_token_metadata(mint: &str, settings: &Settings) -> Result<Option<Metadata>, Box<dyn std::error::Error + Send + Sync>> {
    let metadata_program_id = Pubkey::from_str(&settings.metadata_program)?;
    let mint_pubkey = Pubkey::from_str(mint)?;
    let (metadata_pda, _) = Pubkey::find_program_address(
        &[b"metadata", metadata_program_id.as_ref(), mint_pubkey.as_ref()],
        &metadata_program_id,
    );

    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
        "params": [ metadata_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
    });

    let data: RpcResponse<AccountInfoResult> = fetch_with_fallback(request, "getAccountInfo", settings).await?;

    if let Some(result) = data.result {
        if let Some(account_data) = result.value {
            if !account_data.data.is_empty() {
                let base64_data = &account_data.data[0];
                match Base64Engine.decode(base64_data) {
                    Ok(decoded) => {
                        match Metadata::from_bytes(&decoded) {
                            Ok(meta) => return Ok(Some(meta)),
                            Err(e) => {
                                error!("Failed to parse metadata for {}: {}", mint, e);
                                return Err(e.into());
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to decode base64 data for {}: {}", mint, e);
                        return Err(e.into());
                    }
                }
            } else {
                info!("Empty account data for mint {}", mint);
            }
        } else {
            info!("No account info returned for mint {}", mint);
        }
    } else {
        info!("No result in RPC response for mint {}", mint);
    }
    Ok(None)
}

pub async fn fetch_with_fallback<T: for<'de> Deserialize<'de> + Send + 'static>(
    request: serde_json::Value,
    _method: &str,
    settings: &Settings,
) -> Result<RpcResponse<T>, Box<dyn std::error::Error + Send + Sync>> {
    let client = Arc::new(Client::new());
    let futures = settings.solana_rpc_urls.iter().map(|rpc_url| {
        let client = client.clone();
        let request = request.clone();
        Box::pin(async move {
            let resp = client.post(rpc_url).header("Content-Type", "application/json").body(request.to_string()).send().await?;
            let bytes = resp.bytes().await?;
            let data = serde_json::from_slice::<RpcResponse<T>>(&bytes)?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(data)
        })
    });
    let (data, _) = select_ok(futures).await?;
    if data.error.is_some() {
        Err("RPC error".into())
    } else {
        Ok(data)
    }
}

pub async fn fetch_current_price(mint: &str, price_cache: &Arc<Mutex<PriceCache>>, settings: &Settings) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let mut cache = price_cache.lock().await;
    let mint_str = mint.to_string();
    if let Some((timestamp, price)) = cache.get(&mint_str) {
        if Instant::now().duration_since(*timestamp) < PRICE_CACHE_TTL {
            return Ok(*price);
        }
    }

    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let mint_pubkey = Pubkey::from_str(mint)?;
    let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding_curve", pump_program.as_ref(), mint_pubkey.as_ref()], &pump_program);
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
        "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
    });
    let data: RpcResponse<AccountInfoResult> = fetch_with_fallback(request, "getAccountInfo", settings).await?;
    let price = if let Some(result) = data.result {
        if result.value.is_none() || result.value.as_ref().map_or(true, |d| d.data.is_empty()) {
            return Err("Bonding curve account not found or empty".into());
        }
        let base64_data = &result.value.expect("Verified non-empty in previous check").data[0];
        let decoded = Base64Engine.decode(base64_data)?;
        let state = BondingCurveState::try_from_slice(&decoded[8..])?; // Skip 8-byte discriminator
        if state.complete {
            return Err("Token migrated".into());
        }
        let token_reserves = state.virtual_token_reserves as f64;
        let sol_reserves = state.virtual_sol_reserves as f64 / 1_000_000_000.0;
        if token_reserves > 0.0 { sol_reserves / token_reserves } else { 0.0 }
    } else {
        0.0
    };

    cache.put(mint_str, (Instant::now(), price));
    Ok(price)
}

pub async fn buy_token(
    mint: &str,
    sol_amount: f64,
    is_real: bool,
    keypair: Option<&Keypair>,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Settings,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Attempting to buy {} with {} SOL", mint, sol_amount);
    if !is_real {
        info!("Dry run: Not executing buy transaction.");
        let price = fetch_current_price(mint, price_cache, settings).await.unwrap_or(0.0);
        if price > 0.0 {
            let token_amount = (sol_amount / price) as u64;
            let mut holdings_guard = holdings.lock().await;
            holdings_guard.insert(mint.to_string(), Holding {
                mint: mint.to_string(),
                amount: token_amount,
                buy_price: price,
                buy_time: Utc::now(),
            });
            info!("Simulated buy of {} tokens of {}", token_amount, mint);
        }
        return Ok(());
    }

    let _keypair = keypair.ok_or("Keypair required for real transaction")?;
    
    // Discriminator for 'buy'
    let discriminator: [u8; 8] = [109, 160, 113, 29, 33, 135, 141, 6];
    let mut instruction_data = discriminator.to_vec();
    instruction_data.extend_from_slice(&((sol_amount * 1_000_000_000.0) as u64).to_le_bytes());
    instruction_data.extend_from_slice(&0u64.to_le_bytes()); // min_token_output

    info!("Buy instruction data (not sent): {:?}", instruction_data);
    todo!("Implement full buy transaction sending with all required accounts");
}

pub async fn sell_token(
    mint: &str,
    token_amount: u64,
    is_real: bool,
    keypair: Option<&Keypair>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Attempting to sell {} of {}", token_amount, mint);
    if !is_real {
        info!("Dry run: Not executing sell transaction.");
        info!("Simulated sell of {}", mint);
        return Ok(());
    }

    let _keypair = keypair.ok_or("Keypair required for real transaction")?;

    // Discriminator for 'sell'
    let discriminator: [u8; 8] = [153, 8, 166, 57, 28, 118, 184, 128];
    let mut instruction_data = discriminator.to_vec();
    instruction_data.extend_from_slice(&token_amount.to_le_bytes());
    instruction_data.extend_from_slice(&0u64.to_le_bytes()); // min_sol_output

    info!("Sell instruction data (not sent): {:?}", instruction_data);
    todo!("Implement full sell transaction sending with all required accounts");
}
