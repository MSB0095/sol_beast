use crate::{
    models::{
        AccountInfoResult,
        BondingCurveState,
        Holding,
        PriceCache,
        RpcResponse,
        TransactionResult,
    },
    settings::Settings,
};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use borsh::BorshDeserialize;
use chrono::Utc;
use futures_util::future::select_ok;
use log::info;
use mpl_token_metadata::accounts::Metadata;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::{str::FromStr, sync::Arc, time::Instant};
use tokio::sync::Mutex;

pub async fn fetch_transaction_details(
    signature: &str,
    settings: &Arc<Settings>,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let data: RpcResponse<TransactionResult> = fetch_with_fallback(
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "getTransaction",
            "params": [ signature, { "encoding": "jsonParsed", "commitment": "confirmed", "maxSupportedTransactionVersion": 0 } ]
        }),
        "getTransaction",
        settings,
    )
    .await?;

    let result = data.result.ok_or("No transaction data")?;
    info!("Transaction data: {:?}", result);
    let account_keys = &result.transaction.message.account_keys;
    let pump_fun_program_id = &settings.pump_fun_program;

    if let Some(meta) = result.meta {
        if let Some(inner_instructions) = meta.inner_instructions {
            for inner_instruction in inner_instructions {
                for instruction in &inner_instruction.instructions {
                    let program_id_index = instruction.program_id_index as usize;
                    if let Some(program_id_key) = account_keys.get(program_id_index) {
                        if program_id_key.pubkey == *pump_fun_program_id {
                            // Assuming the order of accounts in the instruction is: mint, bonding curve, associated token account, creator
                            if instruction.accounts.len() >= 4 {
                                let mint_index = instruction.accounts[0] as usize;
                                let creator_index = instruction.accounts[3] as usize;
                                if let (Some(mint_key), Some(creator_key)) = (account_keys.get(mint_index), account_keys.get(creator_index)) {
                                    return Ok((creator_key.pubkey.clone(), mint_key.pubkey.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err("Could not find pump.fun instruction or extract details".into())
}

pub async fn fetch_token_metadata(
    mint: &str,
    settings: &Arc<Settings>,
) -> Result<Option<Metadata>, Box<dyn std::error::Error>> {
    let metadata_program_pk = Pubkey::from_str(&settings.metadata_program)?;
    let mint_pk = Pubkey::from_str(mint)?;
    let metadata_pda = Pubkey::find_program_address(
        &[b"metadata", metadata_program_pk.as_ref(), mint_pk.as_ref()],
        &metadata_program_pk,
    )
    .0;
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
        .and_then(|r| r.data.get(0).and_then(|d| Base64Engine.decode(d).ok()))
        .and_then(|d| Metadata::safe_deserialize(&d).ok()))
}

pub async fn fetch_with_fallback<T: for<'de> Deserialize<'de> + Send + 'static>(
    request: Value,
    _method: &str,
    settings: &Arc<Settings>,
) -> Result<RpcResponse<T>, Box<dyn std::error::Error>> {
    let client = Arc::new(Client::new());
    let futures = settings.solana_rpc_urls.iter().map(|http| {
        let client = client.clone();
        let request = request.clone();
        Box::pin(async move {
            client
                .post(http)
                .json(&request)
                .send()
                .await?
                .json::<RpcResponse<T>>()
                .await
                .map_err(|e| Into::<Box<dyn std::error::Error + Send + Sync>>::into(e))
        })
    });
    let (data, _) = select_ok(futures).await.map_err(|e| format!("{}", e))?;
    if data.error.is_some() {
        Err("RPC error".into())
    } else {
        Ok(data)
    }
}

pub async fn fetch_current_price(
    mint: &str,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
) -> Result<f64, Box<dyn std::error::Error>> {
    let mut cache = price_cache.lock().await;
    if let Some((timestamp, price)) = cache.get(mint) {
        if Instant::now().duration_since(*timestamp) < std::time::Duration::from_secs(settings.price_cache_ttl_secs) {
            return Ok(*price);
        }
    }

    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let mint_pubkey = Pubkey::from_str(mint)?;
    let (curve_pda, _) = Pubkey::find_program_address(
        &[b"bonding_curve", pump_program.as_ref(), mint_pubkey.as_ref()],
        &pump_program,
    );
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
        "params": [ curve_pda.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
    });
    let data: RpcResponse<AccountInfoResult> =
        fetch_with_fallback(request, "getAccountInfo", settings).await?;

    let price = if let Some(result) = data.result {
        let base64_data = &result.data;
        let decoded = Base64Engine.decode(base64_data.get(0).ok_or("Missing data string")?).map_err(|e| format!("Decode error for {}: {}", mint, e))?;
        let state = BondingCurveState::try_from_slice(&decoded)
            .map_err(|e| format!("Deserialize error for {}: {}", mint, e))?;
        if state.complete {
            return Err(format!("Token {} migrated to Raydium", mint).into());
        }
        let token_reserves = state.virtual_token_reserves as f64;
        let sol_reserves = state.virtual_sol_reserves as f64 / 1_000_000_000.0;
        if token_reserves > 0.0 {
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
) -> Result<Holding, Box<dyn std::error::Error>> {
    let buy_price = fetch_current_price(mint, &price_cache, settings).await?;
    let token_amount = (sol_amount / buy_price) as u64;
    info!(
        "Buy {}: {} tokens for {} SOL (price: {} SOL/token)",
        mint, token_amount, sol_amount, buy_price
    );

    if is_real {
        let client = RpcClient::new(&settings.solana_rpc_urls[0]);
        let payer = keypair.ok_or("Keypair required")?;
        let instruction = Instruction {
            program_id: Pubkey::from_str(&settings.pump_fun_program)?,
            accounts: vec![], // TODO: Add payer, mint, bonding curve, ATA
            data: vec![], // Buy discriminator
        };
        let mut tx = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
        let blockhash = client.get_latest_blockhash()?;
        tx.sign(&[payer], blockhash);
        client.send_and_confirm_transaction(&tx)?;
    }

    Ok(Holding {
        amount: token_amount,
        buy_price,
        buy_time: Utc::now(),
    })
}

pub async fn sell_token(
    mint: &str,
    amount: u64,
    current_price: f64,
    is_real: bool,
    keypair: Option<&Keypair>,
    settings: &Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error>> {
    let sol_received = amount as f64 * current_price;
    info!(
        "Sell {}: {} tokens for {} SOL (price: {} SOL/token)",
        mint, amount, sol_received, current_price
    );

    if is_real {
        let client = RpcClient::new(&settings.solana_rpc_urls[0]);
        let payer = keypair.ok_or("Keypair required")?;
        let instruction = Instruction {
            program_id: Pubkey::from_str(&settings.pump_fun_program)?,
            accounts: vec![], // TODO: Add payer, mint, bonding curve, ATA
            data: vec![], // Sell discriminator
        };
        let mut tx = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
        let blockhash = client.get_latest_blockhash()?;
        tx.sign(&[payer], blockhash);
        client.send_and_confirm_transaction(&tx)?;
    }
    Ok(())
}
