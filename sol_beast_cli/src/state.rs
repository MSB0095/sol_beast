use serde::{Deserialize, Serialize};

use chrono::Utc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuyRecord {
    pub mint: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub uri: Option<String>,
    pub image: Option<String>,
    pub creator: String,
    pub detect_time: chrono::DateTime<Utc>,
    pub buy_time: chrono::DateTime<Utc>,
    pub buy_amount_sol: f64,
    pub buy_amount_tokens: u64,
    pub buy_price: f64,
    pub buy_signature: Option<String>,
}