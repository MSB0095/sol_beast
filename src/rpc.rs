// Rust
use log::info;
use mpl_token_metadata::accounts::Metadata as MplMetadata;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use solana_program::pubkey::Pubkey;
use std::str::FromStr;
use base64::engine::general_purpose::STANDARD as Base64Engine;
use base64::Engine;

// Fetch all token metadata accounts in a single HTTPS RPC call at startup
pub async fn fetch_all_token_metadata() -> Result<Vec<MplMetadata>, Box<dyn std::error::Error>> {
    use serde_json::json;
    let accounts_data: RpcResponse<MultipleAccountsResult> = fetch_with_fallback(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getProgramAccounts",
        "params": [
            METADATA_PROGRAM,
            {
                "encoding": "base64",
                "commitment": "confirmed"
            }
        ]
    }), "getProgramAccounts").await?;

    let mut tokens = Vec::new();
    if let Some(result) = accounts_data.result {
        for acc_opt in result.value {
            if let Some(acc) = acc_opt {
                let base64_data = &acc.data.0;
                if let Ok(decoded) = Base64Engine.decode(base64_data) {
                    if let Ok(meta) = MplMetadata::from_bytes(&decoded) {
                        tokens.push(meta);
                    }
                }
            }
        }
    }
    Ok(tokens)
}
const METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

#[derive(Deserialize)]
pub struct RpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<serde_json::Value>,
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
}

#[derive(Deserialize)]
pub struct AccountKey {
    pub pubkey: String,
}

#[derive(Deserialize)]
pub struct MultipleAccountsResult {
    pub value: Vec<Option<AccountInfo>>,
}

#[derive(Deserialize)]
pub struct AccountInfo {
    pub data: (String, String), // (base64, encoding)
}

pub async fn handle_new_token(signature: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Fetch transaction and metadata in a single HTTPS batch call
    let (creator, mint, metadata) = fetch_transaction_and_metadata(signature).await?;
    // All info is obtained in one HTTPS call above

    if let Some(meta) = &metadata {
        info!(
            "Token {}: Name={}, Symbol={}, URI={}, SellerFee={}, Creator={}",
            mint,
            meta.name.trim(),
            meta.symbol.trim(),
            meta.uri.trim(),
            meta.seller_fee_basis_points,
            creator
        );
    } else {
        info!("Token {}: Creator={}, No metadata found", mint, creator);
    }
    execute_trade(&mint, metadata.as_ref(), &creator).await?;
    Ok(())
}

/// Fetch both transaction and metadata in one getMultipleAccounts request.
pub async fn fetch_transaction_and_metadata(signature: &str) -> Result<(String, String, Option<MplMetadata>), Box<dyn std::error::Error>> {
    // 1. Fetch transaction to get account keys (mint)
    // 1. Fetch transaction to get account keys (mint)
    // 1. Fetch transaction to get account keys (mint)
    let tx_data: RpcResponse<TransactionResult> = fetch_with_fallback(json!({
        "jsonrpc": "2.0", "id": 1, "method": "getTransaction",
        "params": [ signature, { "encoding": "jsonParsed", "commitment": "confirmed", "maxSupportedTransactionVersion": 0 } ]
    }), "getTransaction").await?;
    let result = tx_data.result.ok_or("No transaction data")?;
    let keys = result.transaction.message.account_keys;
    if keys.len() < 2 || keys[0].pubkey == keys[1].pubkey {
        return Err("Invalid account keys".into());
    }
    let creator = keys[0].pubkey.clone();
    let mint = keys[1].pubkey.clone();

    // 2. Compute Metadata PDA
    let metadata_pda = Pubkey::find_program_address(
        &[b"metadata", &Pubkey::from_str(METADATA_PROGRAM)?.to_bytes(), &Pubkey::from_str(&mint)?.to_bytes()],
        &Pubkey::from_str(METADATA_PROGRAM)?,
    ).0;

    // 3. Batch fetch: getMultipleAccounts for [metadata_pda]
    let batch_data: RpcResponse<MultipleAccountsResult> = fetch_with_fallback(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "getMultipleAccounts",
        "params": [
            [metadata_pda.to_string()],
            { "encoding": "base64", "commitment": "confirmed" }
        ]
    }), "getMultipleAccounts").await?;

    let mut metadata: Option<MplMetadata> = None;
    if let Some(result) = batch_data.result {
        if let Some(Some(account)) = result.value.get(0) {
            let base64_data = &account.data.0;
            let decoded = Base64Engine.decode(base64_data)?;
            if let Ok(meta) = MplMetadata::from_bytes(&decoded) {
                metadata = Some(meta);
            }
        }
    }
    Ok((creator, mint, metadata))
}

pub async fn fetch_with_fallback<T: for<'de> Deserialize<'de> + Send + 'static>(
    request: serde_json::Value,
    _method: &str,
) -> Result<RpcResponse<T>, Box<dyn std::error::Error>> {
    use crate::settings::Settings;
    use tokio::sync::OnceCell;
    static SETTINGS: OnceCell<Settings> = OnceCell::const_new();

    let settings = SETTINGS
        .get_or_init(|| async { Settings::from_file("config.toml") })
        .await;

    let client = std::sync::Arc::new(Client::new());
    let rpc_urls = settings.solana_rpc_urls.clone();
    let futures = rpc_urls.into_iter().map(|http| {
        let client = client.clone();
        let request = request.clone();
        Box::pin(async move {
            let resp = client.post(&http).header("Content-Type", "application/json").body(request.to_string()).send().await?;
            let bytes = resp.bytes().await?;
            let data = serde_json::from_slice::<RpcResponse<T>>(&bytes)?;
            Ok::<_, Box<dyn std::error::Error>>(data)
        })
    });
    let (data, _) = futures_util::future::select_ok(futures).await?;
    if data.error.is_some() {
        Err("RPC error".into())
    } else {
        Ok(data)
    }
}

pub async fn execute_trade(
    mint: &str,
    metadata: Option<&MplMetadata>,
    _creator: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(m) = metadata {
        if !m.uri.is_empty() && m.seller_fee_basis_points < 500 {
            info!(
                "Buy {}: Name={}, Symbol={}, URI={}, Royalties={}",
                mint,
                m.name.trim(),
                m.symbol.trim(),
                m.uri.trim(),
                m.seller_fee_basis_points
            );
            // buy_token(mint, 1000).await?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn buy_token(_mint: &str, _amount: u64) -> Result<(), Box<dyn std::error::Error>> {
    todo!("Implement buy")
}

#[allow(dead_code)]
pub async fn sell_token(_mint: &str, _amount: u64) -> Result<(), Box<dyn std::error::Error>> {
    todo!("Implement sell")
}
