use crate::{
    models::{
        AccountInfoResult, BondingCurveState, Holding, PriceCache, RpcResponse, TransactionResult,
    },
    settings::Settings,
};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use borsh::BorshDeserialize;
use chrono::Utc;
use log::{info, warn};
use mpl_token_metadata::accounts::Metadata;
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal_macros::dec;
use serde::Deserialize;
use serde_json::{json, Value};
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use solana_sdk::{
    hash::hashv,
    instruction::{AccountMeta, Instruction},
    signature::{Keypair, Signer},
    sysvar,
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address, instruction::create_associated_token_account,
};
use std::{str::FromStr, sync::Arc, time::Instant};
use tokio::sync::Mutex;

pub async fn fetch_transaction_details(
    signature: &str,
    settings: &Arc<Settings>,
) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let mut retries = 5;
    while retries > 0 {
        let data: RpcResponse<TransactionResult> = fetch_with_fallback(
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "getTransaction",
                "params": [ signature, { "encoding": "jsonParsed", "commitment": "confirmed", "maxSupportedTransactionVersion": 0 } ]
            }),
            "getTransaction",
            settings,
        )
        .await?;

        if let Some(result) = data.result {
            let account_keys = &result.transaction.as_ref().ok_or("Missing transaction data")?.message.account_keys;
            if account_keys.len() >= 2 {
                let creator = account_keys[0].pubkey.clone();
                let mint = account_keys[1].pubkey.clone();
                return Ok((creator, mint));
            }
        }

        retries -= 1;
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    Err("Could not extract transaction details".into())
}

pub async fn fetch_token_metadata(
    mint: &str,
    settings: &Arc<Settings>,
) -> Result<Option<Metadata>, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let metadata_program_pk = Pubkey::from_str(&settings.metadata_program)?;
    let mint_pk = Pubkey::from_str(mint)?;
    let (metadata_pda, _) = Pubkey::find_program_address(
        &[
            b"metadata",
            metadata_program_pk.as_ref(),
            mint_pk.as_ref(),
        ],
        &metadata_program_pk,
    );
    let data: RpcResponse<AccountInfoResult> = fetch_with_fallback(
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
            "params": [ metadata_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
        }),
        "getAccountInfo",
        settings,
    )
    .await?;
    Ok(data
        .result
        .and_then(|r| r.value.and_then(|v| v.data.first().and_then(|d| Base64Engine.decode(d).ok())))
        .and_then(|d| Metadata::safe_deserialize(&d).ok()))
}

pub async fn fetch_with_fallback<T: for<'de> Deserialize<'de> + Send + 'static>(
    request: Value,
    method: &str,
    settings: &Arc<Settings>,
) -> Result<RpcResponse<T>, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let client = Arc::new(Client::new());
    for http_url in settings.solana_rpc_urls.iter() {
        let response = client.post(http_url).json(&request).send().await;

        match response {
            Ok(res) => {
                if res.status().is_success() {
                    let body_bytes = match res.bytes().await {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            warn!("Failed to read response body from {}: {}", http_url, e);
                            continue;
                        }
                    };

                    match serde_json::from_slice::<RpcResponse<T>>(&body_bytes) {
                        Ok(data) => {
                            if data.error.is_some() {
                                warn!("RPC error for {}: {:?}", http_url, data.error);
                                continue;
                            }
                            return Ok(data);
                        }
                        Err(e) => {
                            let body_text = String::from_utf8_lossy(&body_bytes);
                            warn!(
                                "Failed to decode response from {}: {}. Body: {}",
                                http_url, e, body_text
                            );
                            continue;
                        }
                    }
                } else {
                    warn!("HTTP error from {}: {}", http_url, res.status());
                    continue;
                }
            }
            Err(e) => {
                warn!("Request to {} failed: {}", http_url, e);
                continue;
            }
        }
    }
    Err(format!("All RPCs failed for method {}", method).into())
}


pub async fn fetch_current_price(
    mint: &str,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
) -> Result<Decimal, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let mut cache = price_cache.lock().await;
    if let Some((timestamp, price)) = cache.get(mint) {
        if Instant::now().duration_since(*timestamp)
            < std::time::Duration::from_secs(settings.price_cache_ttl_secs)
        {
            return Ok(*price);
        }
    }

    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let mint_pubkey = Pubkey::from_str(mint)?;
    let (curve_pda, _) = Pubkey::find_program_address(
        &[
            b"bonding-curve",
            mint_pubkey.as_ref(),
        ],
        &pump_program,
    );
    info!("Bonding curve PDA for {}: {}", mint, curve_pda);
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
        "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
    });
    let data: RpcResponse<AccountInfoResult> =
        fetch_with_fallback(request, "getAccountInfo", settings).await?;

    let price = if let Some(result) = data.result {
        let base64_data = &result.value.as_ref().ok_or("Missing value in account info")?.data;
        let decoded = Base64Engine
            .decode(base64_data.first().ok_or("Missing data string")?)
            .map_err(|e| format!("Decode error for {}: {}", mint, e))?;
        info!("Decoded data length: {}", decoded.len());
        let mut decoded_slice = &decoded[8..];
        let state = BondingCurveState::deserialize(&mut decoded_slice)
            .map_err(|e| format!("Deserialize error for {}: {}", mint, e))?;
        if state.complete {
            return Err(format!("Token {} migrated to Raydium", mint).into());
        }
        let token_reserves = Decimal::from(state.virtual_token_reserves);
        let sol_reserves = Decimal::from(state.virtual_sol_reserves) / dec!(1_000_000_000);
        if !token_reserves.is_zero() {
            sol_reserves / token_reserves
        } else {
            return Err(format!("Invalid reserves for {}: zero tokens", mint).into());
        }
    } else {
        return Err(format!("Bonding curve account not found or empty for {}", mint).into());
    };

    cache.put(mint.to_string(), (Instant::now(), price));
    Ok(price)
}




pub async fn buy_token(
    mint: &str,
    sol_amount: f64,
    is_real: bool,
    keypair: Option<&Keypair>,
    price_cache: Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
) -> Result<Holding, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let buy_price = fetch_current_price(mint, &price_cache, settings).await?;
    let sol_amount_decimal = Decimal::from_f64(sol_amount).ok_or("Invalid SOL amount")?;
    let token_amount = (sol_amount_decimal / buy_price).floor().to_u64().ok_or("Invalid token amount")?;
    let lamports = (sol_amount_decimal * dec!(1_000_000_000)).to_u64().ok_or("Invalid lamports amount")?;

    info!(
        "Preparing buy for {}: {} tokens for {} SOL (price: {} SOL/token)",
        mint, token_amount, sol_amount, buy_price
    );

    if is_real {
        let payer = keypair.ok_or("Keypair required for real trade")?;
        let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
        let mint_pubkey = Pubkey::from_str(mint)?;

        let discriminator = &hashv(&["global:buy".as_bytes()]).to_bytes()[0..8];
        let instruction_data = [discriminator.to_vec(), lamports.to_le_bytes().to_vec()].concat();

        let (curve_pda, _) = Pubkey::find_program_address(
            &[
                b"bonding-curve",
                mint_pubkey.as_ref(),
            ],
            &pump_program,
        );
        let ata = get_associated_token_address(&payer.pubkey(), &mint_pubkey);

        let instructions = vec![
            create_associated_token_account(
                &payer.pubkey(),
                &payer.pubkey(),
                &mint_pubkey,
                &spl_token::id(),
            ),
            Instruction {
                program_id: pump_program,
                accounts: vec![
                    AccountMeta::new_readonly(payer.pubkey(), true),
                    AccountMeta::new(mint_pubkey, false),
                    AccountMeta::new(curve_pda, false),
                    AccountMeta::new(ata, false),
                    AccountMeta::new_readonly(spl_token::id(), false),
                    AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
                    AccountMeta::new_readonly(spl_associated_token_account::ID, false),
                    AccountMeta::new_readonly(sysvar::rent::id(), false),
                ],
                data: instruction_data,
            },
        ];

        let client = RpcClient::new(&settings.solana_rpc_urls[0]);
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
        let blockhash = client.get_latest_blockhash()?;
        tx.sign(&[payer], blockhash);
        client.send_and_confirm_transaction(&tx)?;
        info!("Buy executed for {}", mint);
    }

    Ok(Holding {
        mint: mint.to_string(),
        amount: token_amount,
        buy_price,
        buy_time: Utc::now(),
    })
}

pub async fn sell_token(
    mint: &str,
    amount: u64,
    current_price: Decimal,
    is_real: bool,
    keypair: Option<&Keypair>,
    settings: &Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sol_received = Decimal::from(amount) * current_price;
    info!(
        "Preparing sell for {}: {} tokens for {} SOL (price: {} SOL/token)",
        mint, amount, sol_received, current_price
    );

    if is_real {
        let payer = keypair.ok_or("Keypair required for real trade")?;
        let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
        let mint_pubkey = Pubkey::from_str(mint)?;

        let discriminator = &hashv(&["global:sell".as_bytes()]).to_bytes()[0..8];
        let instruction_data = [discriminator.to_vec(), amount.to_le_bytes().to_vec()].concat();

        let (curve_pda, _) = Pubkey::find_program_address(
            &[
                b"bonding-curve",
                mint_pubkey.as_ref(),
            ],
            &pump_program,
        );
        let ata = get_associated_token_address(&payer.pubkey(), &mint_pubkey);

        let instruction = Instruction {
            program_id: pump_program,
            accounts: vec![
                AccountMeta::new_readonly(payer.pubkey(), true),
                AccountMeta::new(mint_pubkey, false),
                AccountMeta::new(curve_pda, false),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(spl_token::id(), false),
                AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
                AccountMeta::new_readonly(spl_associated_token_account::ID, false),
                AccountMeta::new_readonly(sysvar::rent::id(), false),
            ],
            data: instruction_data,
        };

        let client = RpcClient::new(&settings.solana_rpc_urls[0]);
        let mut tx = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
        let blockhash = client.get_latest_blockhash()?;
        tx.sign(&[payer], blockhash);
        client.send_and_confirm_transaction(&tx)?;
        info!("Sell executed for {}", mint);
    }
    Ok(())
}
