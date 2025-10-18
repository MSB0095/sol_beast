use borsh::{BorshDeserialize, BorshSerialize};
use chrono::{DateTime, Utc};
use lru::LruCache;
use serde::Deserialize;
use serde_json::Value;
use std::time::Instant;

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

// Holdings and Price Cache
#[derive(Clone, Debug)]
pub struct Holding {
    pub amount: u64,
    pub buy_price: f64,
    pub buy_time: DateTime<Utc>,
}
pub type PriceCache = LruCache<String, (Instant, f64)>;

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
    pub program_id_index: u8,
    pub accounts: Vec<u8>,
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
#[derive(Deserialize, Debug)]
pub struct AccountInfoResult {
    pub data: Vec<String>,
}