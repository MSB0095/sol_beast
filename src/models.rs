#![allow(dead_code)]

// We no longer rely on Borsh for bonding-curve parsing; manual parsing is used
// to tolerate trailing bytes and layout variations.
use chrono::{DateTime, Utc};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;
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
// - complete: bool (1 byte)
// - creator: Pubkey (32 bytes)
#[derive(Debug, PartialEq, Clone)]
pub struct BondingCurveState {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
    pub creator: Option<Pubkey>,
}

impl BondingCurveState {
    /// Compute the spot price in SOL per token using the virtual reserves.
    /// Returns None if token reserve is zero.
    pub fn spot_price_sol_per_token(&self) -> Option<f64> {
        if self.virtual_token_reserves == 0 {
            return None;
        }
        // Formula: (virtual_sol_lamports / 1e9) / (virtual_token_base_units / 1e6)
        // Simplifies to (virtual_sol_lamports / virtual_token_base_units) * 1e-3
        let vsol = self.virtual_sol_reserves as f64;
        let vtok = self.virtual_token_reserves as f64;
        Some((vsol / vtok) * 1e-3)
    }
}

// Holdings and Price Cache
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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

#[derive(Deserialize, Serialize, Debug, Clone)]
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



#[cfg(test)]
mod tests {
    use super::BondingCurveState;

    #[test]
    fn test_spot_price_formula_unit() {
        let state = BondingCurveState {
            virtual_sol_reserves: 30_000_000_000u64, // 30 SOL in lamports
            virtual_token_reserves: 1_073_000_191_000_000u64, // 1.073B tokens with 6 decimals
            real_token_reserves: 0,
            real_sol_reserves: 0,
            token_total_supply: 0,
            complete: false,
            creator: None,
        };
        let price_opt = state.spot_price_sol_per_token();
        assert!(price_opt.is_some(), "spot_price_sol_per_token should not be None");
        let price = price_opt.unwrap();
        let expected = 30.0 / 1_073_000_191.0_f64; // ~2.795e-8
        let diff = (price - expected).abs();
        assert!(diff < 1e-15, "price mismatch: got {} expected {} diff {}", price, expected, diff);
    }
}