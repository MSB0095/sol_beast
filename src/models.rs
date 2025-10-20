use borsh::{BorshDeserialize, BorshSerialize};
use chrono::{DateTime, Utc};
use lru::LruCache;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use solana_program::pubkey::Pubkey;
use std::time::Instant;

// Bonding Curve State
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct BondingCurveState {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
    pub creator: Pubkey,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct TradeEvent {
    pub mint: Pubkey,
    pub sol_amount: u64,
    pub token_amount: u64,
    pub is_buy: bool,
    pub user: Pubkey,
    pub timestamp: i64,
    pub virtual_sol_reserves: u64,
    pub virtual_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub fee_recipient: Pubkey,
    pub fee_basis_points: u64,
    pub fee: u64,
    pub creator: Pubkey,
    pub creator_fee_basis_points: u64,
    pub creator_fee: u64,
}

// Holdings and Price Cache
#[derive(Clone, Debug, Deserialize)]
pub struct Holding {
    pub mint: String,
    pub amount: u64,
    pub buy_price: Decimal,
    pub buy_time: DateTime<Utc>,
}
pub type PriceCache = LruCache<String, (Instant, Decimal)>;

// RPC structures
#[derive(Deserialize, Debug)]
pub struct RpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<Value>,
}
#[derive(Deserialize, Debug)]
pub struct TransactionResult {
    pub transaction: Option<TransactionData>,
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
#[derive(Deserialize, Debug)]
pub struct AccountInfoResult {
    pub value: Option<AccountInfoValue>,
}

#[derive(Deserialize, Debug)]
pub struct AccountInfoValue {
    pub data: Vec<String>,
}
