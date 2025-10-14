// Rust
use log::{info, warn};
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Deserialize, Debug)]
pub struct RpcResponse {
    pub result: Option<TransactionResult>,
    pub error: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
pub struct TransactionResult {
    pub transaction: TransactionData,
}

#[derive(Deserialize, Debug)]
pub struct TransactionData {
    pub message: MessageData,
}

#[derive(Deserialize, Debug)]
pub struct MessageData {
    #[serde(rename = "accountKeys")]
    pub account_keys: Vec<AccountKey>,
}

#[derive(Deserialize, Debug)]
pub struct AccountKey {
    pub pubkey: String,
}

const MAX_RETRIES: u32 = 5;

pub async fn fetch_transaction_details(signature: &str, https_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getTransaction",
        "params": [
            signature,
            {
                "encoding": "jsonParsed",
                "commitment": "confirmed",
                "maxSupportedTransactionVersion": 0
            }
        ]
    });

    for attempt in 1..=MAX_RETRIES {
        let response = client
            .post(https_url)
            .json(&request)
            .send()
            .await?;

        match response.json::<RpcResponse>().await {
            Ok(data) => {
                if let Some(error) = data.error {
                    return Err(format!("RPC error: {}", error).into());
                }
                if let Some(result) = data.result {
                    let account_keys = result.transaction.message.account_keys;
                    if account_keys.len() >= 2 && account_keys[0].pubkey != account_keys[1].pubkey {
                        info!("Creator Address: {}", account_keys[0].pubkey);
                        info!("New Token Mint Address: {}", account_keys[1].pubkey);
                        info!("---");
                    } else {
                        warn!("Unexpected account keys for {}: {:?}", signature, account_keys);
                    }
                    return Ok(());
                }
                return Err("No transaction data".into());
            }
            Err(e) => {
                if attempt == MAX_RETRIES {
                    return Err(format!("Failed after {} retries: {}", MAX_RETRIES, e).into());
                }
                let delay = Duration::from_millis(2u64.pow(attempt) * 1000);
                warn!("Retry {} for {} after {}ms: {}", attempt, signature, delay.as_millis(), e);
                sleep(delay).await;
            }
        }
    }

    Ok(())
}