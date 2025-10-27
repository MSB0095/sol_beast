#![allow(dead_code)]

use borsh::{BorshDeserialize, BorshSerialize};
use chrono::{DateTime, Utc};
use lru::LruCache;
use serde::Deserialize;
use serde_json::Value;
use std::time::Instant;

// Bonding Curve State
// New pump.fun bonding curve layout (post 2025 updates): the on-chain account
// begins with an 8-byte Anchor discriminator which we strip before deserializing
// into this struct. Fields here map to the bytes after that discriminator.
// Layout (after discriminator):
// - virtual_token_reserves: u64
// - virtual_sol_reserves: u64
// - real_token_reserves: u64
// - real_sol_reserves: u64
// - token_total_supply: u64
// - complete: bool
#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq)]
pub struct BondingCurveState {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
}

// Holdings and Price Cache
#[derive(Clone, Debug)]
pub struct Holding {
    pub amount: u64,
    pub buy_price: f64,
    pub buy_time: DateTime<Utc>,
    // Optional off-chain metadata retrieved from the token's URI (name, symbol, image, etc.)
    pub metadata: Option<OffchainTokenMetadata>,
    // Optional on-chain metadata (trimmed fields) retrieved from the token's metadata account
    // Full raw on-chain metadata account bytes (decoded from base64). This preserves the
    // entire account data so callers can deserialize later or inspect any fields.
    pub onchain_raw: Option<Vec<u8>>,
    // Parsed, convenient subset of the on-chain `Metadata` account saved for quick access
    pub onchain: Option<OnchainFullMetadata>,
}
pub type PriceCache = LruCache<String, (Instant, f64)>;

#[derive(Deserialize, Debug, Clone)]
pub struct OffchainTokenMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    #[serde(flatten)]
    pub extras: Option<serde_json::Value>,
}

// (Previously had a small trimmed `OnchainTokenMetadata`.) We now store the full
// decoded account bytes in `Holding::onchain_raw` to preserve the complete on-chain
// metadata payload.

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OnchainFullMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub uri: Option<String>,
    pub seller_fee_basis_points: Option<u16>,
    // Keep the raw bytes too so callers don't need to re-request the account
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<Vec<u8>>,
}

// RPC structures
#[derive(Deserialize, Debug)]
pub struct RpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<Value>,
}
#[derive(Deserialize, Debug)]
pub struct TransactionResult {
    pub transaction: TransactionData,
    pub meta: Option<TransactionMeta>,
}
#[derive(Deserialize, Debug)]
pub struct TransactionData {
    pub message: MessageData,
}
#[derive(Deserialize, Debug)]
pub struct TransactionMeta {
    #[serde(rename = "innerInstructions")]
    pub inner_instructions: Option<Vec<InnerInstruction>>,
}
#[derive(Deserialize, Debug)]
pub struct InnerInstruction {
    // pub index: u8,
    pub instructions: Vec<Instruction>,
}
#[derive(Deserialize, Debug)]
pub struct Instruction {
    #[serde(rename = "programIdIndex")]
    pub program_id_index: usize,
    pub accounts: Vec<usize>,
}
#[derive(Deserialize, Debug)]
pub struct MessageData {
    #[serde(rename = "accountKeys")]
    pub account_keys: Vec<AccountKey>,
}
#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum AccountKey {
    Simple(String),
    Detailed { pubkey: String },
}

impl AccountKey {
    pub fn pubkey(&self) -> &str {
        match self {
            AccountKey::Simple(s) => s.as_str(),
            AccountKey::Detailed { pubkey } => pubkey.as_str(),
        }
    }
}
#[derive(Deserialize, Debug)]
pub struct AccountInfoResult {
    pub data: Vec<String>,
}