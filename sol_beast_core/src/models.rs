use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Bonding Curve State for pump.fun tokens
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct BondingCurveState {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
    pub creator: Option<String>, // Store as string for WASM compatibility
}

impl BondingCurveState {
    /// Compute the spot price in SOL per token using the virtual reserves.
    /// 
    /// Returns the price in SOL per token (with token decimals).
    /// Formula: (virtual_sol_lamports / virtual_token_base_units) * 1e-3
    /// 
    /// The 1e-3 factor accounts for the unit conversion:
    /// - SOL reserves are in lamports (1 SOL = 1e9 lamports)
    /// - Token reserves are in base units (1 token = 1e6 base units for 6 decimals)
    /// - Result: (lamports / base_units) * 1e-3 = SOL per token
    pub fn spot_price_sol_per_token(&self) -> Option<f64> {
        if self.virtual_token_reserves == 0 {
            return None;
        }
        let vsol = self.virtual_sol_reserves as f64;
        let vtok = self.virtual_token_reserves as f64;
        // Price = (SOL_lamports / token_base_units) * 1e-3
        Some((vsol / vtok) * 1e-3)
    }
}

/// User account data stored per wallet address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccount {
    pub wallet_address: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub total_trades: u64,
    pub total_profit_loss: f64,
    pub settings: UserSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub tp_percent: f64,
    pub sl_percent: f64,
    pub timeout_secs: i64,
    pub buy_amount: f64,
    pub max_held_coins: usize,
    pub enable_safer_sniping: bool,
    pub min_tokens_threshold: u64,
    pub max_sol_per_token: f64,
    pub slippage_bps: u64,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            tp_percent: 30.0,
            sl_percent: -20.0,
            timeout_secs: 3600,
            buy_amount: 0.1,
            max_held_coins: 10,
            enable_safer_sniping: true,
            min_tokens_threshold: 1_000_000,
            max_sol_per_token: 0.0001,
            slippage_bps: 500,
        }
    }
}

/// Token holding information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Holding {
    pub mint: String,
    pub amount: u64,
    pub buy_price: f64,
    pub buy_time: DateTime<Utc>,
    pub metadata: Option<OffchainMetadata>,
    pub onchain_raw: Option<Vec<u8>>,
    pub onchain: Option<OnchainFullMetadata>,
}

/// Off-chain token metadata from URI
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct OffchainMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    #[serde(flatten)]
    pub extras: Option<serde_json::Value>,
}

/// On-chain metadata parsed from metadata account
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnchainFullMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub uri: Option<String>,
    pub seller_fee_basis_points: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<Vec<u8>>,
}

/// Trade record for history
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradeRecord {
    pub mint: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub image: Option<String>,
    pub trade_type: String, // "buy" or "sell"
    pub timestamp: DateTime<Utc>,
    pub tx_signature: Option<String>,
    pub amount_sol: f64,
    pub amount_tokens: f64,
    pub price_per_token: f64,
    pub profit_loss: Option<f64>,
    pub profit_loss_percent: Option<f64>,
    pub reason: Option<String>,
}

/// Strategy configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub tp_percent: f64,
    pub sl_percent: f64,
    pub timeout_secs: i64,
    pub enable_safer_sniping: bool,
    pub min_tokens_threshold: u64,
    pub max_sol_per_token: f64,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            tp_percent: 30.0,
            sl_percent: -20.0,
            timeout_secs: 3600,
            enable_safer_sniping: true,
            min_tokens_threshold: 1_000_000,
            max_sol_per_token: 0.0001,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spot_price_formula() {
        let state = BondingCurveState {
            virtual_sol_reserves: 30_000_000_000u64,
            virtual_token_reserves: 1_073_000_191_000_000u64,
            real_token_reserves: 0,
            real_sol_reserves: 0,
            token_total_supply: 0,
            complete: false,
            creator: None,
        };
        let price = state.spot_price_sol_per_token();
        assert!(price.is_some());
        let expected = 30.0 / 1_073_000_191.0_f64;
        assert!((price.unwrap() - expected).abs() < 1e-15);
    }
}
