// Rust
use log::warn;
use serde::Deserialize;
use mpl_token_metadata::accounts::Metadata as MplMetadata;
use solana_program::pubkey::Pubkey;
use base64::engine::general_purpose::STANDARD as Base64Engine;
use base64::Engine;
use std::str::FromStr;

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
const METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

pub async fn fetch_transaction_details(
    signature: &str,
    https_url: &str,
) -> Result<(String, String, Option<MplMetadata>), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    // First, get the transaction to extract the mint
    let tx_request = serde_json::json!({
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

    for _ in 1..=MAX_RETRIES {
        let tx_response = client
            .post(https_url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&tx_request)?)
            .send()
            .await?;

        let tx_bytes = tx_response.bytes().await?;
        let data: RpcResponse = serde_json::from_slice(&tx_bytes)?;
        if let Some(error) = data.error {
            return Err(format!("RPC error: {}", error).into());
        }
        if let Some(result) = data.result {
            let account_keys = result.transaction.message.account_keys;
            if account_keys.len() >= 2 && account_keys[0].pubkey != account_keys[1].pubkey {
                let creator = account_keys[0].pubkey.clone();
                let mint = account_keys[1].pubkey.clone();

                // Compute Metadata PDA
                let metadata_program = Pubkey::from_str(METADATA_PROGRAM)?;
                let mint_pubkey = Pubkey::from_str(&mint)?;
                let seeds = &[
                    b"metadata",
                    metadata_program.as_ref(),
                    mint_pubkey.as_ref(),
                ];
                let (metadata_pda, _) = Pubkey::find_program_address(seeds, &metadata_program);

                // Batch fetch: getMultipleAccounts for metadata_pda
                let batch_request = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "getMultipleAccounts",
                    "params": [
                        [metadata_pda.to_string()],
                        {
                            "encoding": "base64",
                            "commitment": "confirmed"
                        }
                    ]
                });

                let batch_response = client
                    .post(https_url)
                    .header("Content-Type", "application/json")
                    .body(serde_json::to_vec(&batch_request)?)
                    .send()
                    .await?;

                let batch_bytes = batch_response.bytes().await?;
                let batch_data: serde_json::Value = serde_json::from_slice(&batch_bytes)?;
                let mut metadata: Option<MplMetadata> = None;
                if let Some(result) = batch_data.get("result") {
                    if let Some(value_arr) = result.get("value").and_then(|v| v.as_array()) {
                        if let Some(value) = value_arr.get(0).and_then(|v| v.as_object()) {
                            if let Some(data_arr) = value.get("data").and_then(|d| d.as_array()) {
                                if let Some(base64_data) = data_arr.get(0).and_then(|v| v.as_str()) {
                                    let decoded = Base64Engine.decode(base64_data)?;
                                    if let Ok(meta) = MplMetadata::from_bytes(&decoded) {
                                        metadata = Some(meta);
                                    }
                                }
                            }
                        }
                    }
                }
                return Ok((creator, mint, metadata));
            } else {
                warn!("Unexpected account keys for {}: {:?}", signature, account_keys);
                return Err("Invalid account keys".into());
            }
        }
        return Err("No transaction data".into());
    }

    Err("Max retries exceeded".into())
}
