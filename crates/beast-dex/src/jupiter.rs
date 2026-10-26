//! Jupiter Quote/Swap API client.
//!
//! Jupiter is the fallback for venues we deliberately do not model locally
//! (prop AMMs, RFQ, CLOBs, launchpads and long-tail pools). It is also the
//! independent quote used to reject stale local Radar models before an attempt.

use crate::address::Address;
use crate::error::DexError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const JUPITER_LITE_BASE: &str = "https://lite-api.jup.ag/swap/v1";

#[derive(Debug, Clone)]
pub struct JupiterClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteRequest {
    pub input_mint: Address,
    pub output_mint: Address,
    pub amount: u64,
    pub slippage_bps: u16,
    pub only_direct_routes: bool,
    pub restrict_intermediate_tokens: bool,
    pub max_accounts: Option<u8>,
}

impl QuoteRequest {
    pub fn new(input_mint: Address, output_mint: Address, amount: u64) -> Self {
        Self {
            input_mint,
            output_mint,
            amount,
            slippage_bps: 50,
            only_direct_routes: true,
            restrict_intermediate_tokens: true,
            max_accounts: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupiterQuote {
    pub input_mint: String,
    pub in_amount: String,
    pub output_mint: String,
    pub out_amount: String,
    pub other_amount_threshold: String,
    pub swap_mode: String,
    #[serde(default)]
    pub slippage_bps: u16,
    #[serde(default)]
    pub price_impact_pct: Value,
    #[serde(default)]
    pub route_plan: Vec<RoutePlanStep>,
    #[serde(default)]
    pub context_slot: u64,
    #[serde(default)]
    pub time_taken: f64,
}

impl JupiterQuote {
    pub fn amount_in(&self) -> Result<u64, DexError> {
        parse_u64(&self.in_amount, "inAmount")
    }

    pub fn amount_out(&self) -> Result<u64, DexError> {
        parse_u64(&self.out_amount, "outAmount")
    }

    pub fn minimum_out(&self) -> Result<u64, DexError> {
        parse_u64(&self.other_amount_threshold, "otherAmountThreshold")
    }

    pub fn price_impact_bps(&self) -> u32 {
        let value = match &self.price_impact_pct {
            Value::String(text) => text.parse::<f64>().ok(),
            Value::Number(number) => number.as_f64(),
            _ => None,
        };
        value
            .filter(|value| value.is_finite())
            .map(|value| (value.abs() * 10_000.0).round().min(u32::MAX as f64) as u32)
            .unwrap_or(0)
    }

    pub fn route_labels(&self) -> Vec<&str> {
        self.route_plan
            .iter()
            .map(|step| step.swap_info.label.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePlanStep {
    pub percent: Option<u16>,
    pub swap_info: SwapInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapInfo {
    pub amm_key: String,
    pub label: String,
    pub input_mint: String,
    pub output_mint: String,
    pub in_amount: String,
    pub out_amount: String,
    /// Jupiter omits these on routes whose fee is not reported in the quote.
    #[serde(default)]
    pub fee_amount: Option<String>,
    #[serde(default)]
    pub fee_mint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapRequest {
    pub user_public_key: String,
    pub quote: JupiterQuote,
    pub wrap_and_unwrap_sol: bool,
    pub dynamic_compute_unit_limit: bool,
    pub as_legacy_transaction: bool,
}

impl SwapRequest {
    pub fn new(user_public_key: impl Into<String>, quote: JupiterQuote) -> Self {
        Self {
            user_public_key: user_public_key.into(),
            quote,
            wrap_and_unwrap_sol: true,
            dynamic_compute_unit_limit: true,
            as_legacy_transaction: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupiterSwap {
    #[serde(default)]
    pub swap_transaction: String,
    #[serde(default)]
    pub last_valid_block_height: u64,
    #[serde(default)]
    pub prioritization_fee_lamports: Value,
    #[serde(default)]
    pub simulation_error: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

impl JupiterClient {
    pub fn new() -> Result<Self, DexError> {
        Self::with_base_url(JUPITER_LITE_BASE, None)
    }

    pub fn with_api_key(api_key: impl Into<String>) -> Result<Self, DexError> {
        Self::with_base_url(JUPITER_LITE_BASE, Some(api_key.into()))
    }

    pub fn with_base_url(
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Result<Self, DexError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .user_agent(concat!("sol-beast/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| DexError::Http(e.to_string()))?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
        })
    }

    pub async fn quote(&self, request: &QuoteRequest) -> Result<JupiterQuote, DexError> {
        if request.amount == 0 {
            return Err(DexError::Math("zero Jupiter amount".into()));
        }
        let mut query = vec![
            ("inputMint", request.input_mint.to_string()),
            ("outputMint", request.output_mint.to_string()),
            ("amount", request.amount.to_string()),
            ("slippageBps", request.slippage_bps.to_string()),
            ("onlyDirectRoutes", request.only_direct_routes.to_string()),
            (
                "restrictIntermediateTokens",
                request.restrict_intermediate_tokens.to_string(),
            ),
        ];
        if let Some(max_accounts) = request.max_accounts {
            query.push(("maxAccounts", max_accounts.to_string()));
        }
        let mut call = self
            .http
            .get(format!("{}/quote", self.base_url))
            .query(&query);
        if let Some(api_key) = &self.api_key {
            call = call.header("x-api-key", api_key);
        }
        let response = call.send().await.map_err(http_error)?;
        let status = response.status();
        let body = response.text().await.map_err(http_error)?;
        if !status.is_success() {
            return Err(DexError::Http(format!(
                "Jupiter quote HTTP {status}: {}",
                truncate(&body)
            )));
        }
        serde_json::from_str(&body).map_err(DexError::from)
    }

    pub async fn swap(&self, request: &SwapRequest) -> Result<JupiterSwap, DexError> {
        let body = serde_json::json!({
            "userPublicKey": request.user_public_key,
            "quoteResponse": request.quote,
            "wrapAndUnwrapSol": request.wrap_and_unwrap_sol,
            "dynamicComputeUnitLimit": request.dynamic_compute_unit_limit,
            "asLegacyTransaction": request.as_legacy_transaction,
            "prioritizationFeeLamports": 0,
        });
        let mut call = self
            .http
            .post(format!("{}/swap", self.base_url))
            .json(&body);
        if let Some(api_key) = &self.api_key {
            call = call.header("x-api-key", api_key);
        }
        let response = call.send().await.map_err(http_error)?;
        let status = response.status();
        let text = response.text().await.map_err(http_error)?;
        if !status.is_success() {
            return Err(DexError::Http(format!(
                "Jupiter swap HTTP {status}: {}",
                truncate(&text)
            )));
        }
        let swap: JupiterSwap = serde_json::from_str(&text).map_err(DexError::from)?;
        validate_swap(swap)
    }
}

fn validate_swap(swap: JupiterSwap) -> Result<JupiterSwap, DexError> {
    if let Some(error) = swap
        .simulation_error
        .as_ref()
        .filter(|value| !value.is_null())
    {
        return Err(DexError::Http(format!(
            "Jupiter swap simulation failed: {}",
            truncate(&error.to_string())
        )));
    }
    if let Some(error) = swap.error.as_ref().filter(|value| !value.is_empty()) {
        return Err(DexError::Http(format!("Jupiter swap error: {error}")));
    }
    if swap.swap_transaction.is_empty() {
        return Err(DexError::Http(
            "Jupiter swap response has no swapTransaction".into(),
        ));
    }
    Ok(swap)
}

fn http_error(error: reqwest::Error) -> DexError {
    DexError::Http(error.to_string())
}

fn truncate(text: &str) -> String {
    const MAX: usize = 512;
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= MAX {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn parse_u64(value: &str, field: &str) -> Result<u64, DexError> {
    value
        .parse()
        .map_err(|_| DexError::Math(format!("invalid Jupiter {field}: {value:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_quote_fixture() {
        let raw = r#"{
            "inputMint":"So11111111111111111111111111111111111111112",
            "inAmount":"1000000",
            "outputMint":"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "outAmount":"1500000",
            "otherAmountThreshold":"1490000",
            "swapMode":"ExactIn",
            "slippageBps":50,
            "priceImpactPct":"0.0012",
            "routePlan":[{"percent":100,"swapInfo":{
                "ammKey":"CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C",
                "label":"Raydium CPMM",
                "inputMint":"So11111111111111111111111111111111111111112",
                "outputMint":"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                "inAmount":"1000000","outAmount":"1500000","feeAmount":"250","feeMint":"So11111111111111111111111111111111111111112"
            }}],
            "contextSlot":123,
            "timeTaken":0.004
        }"#;
        let quote: JupiterQuote = serde_json::from_str(raw).unwrap();
        assert_eq!(quote.amount_in().unwrap(), 1_000_000);
        assert_eq!(quote.amount_out().unwrap(), 1_500_000);
        assert_eq!(quote.minimum_out().unwrap(), 1_490_000);
        assert_eq!(quote.price_impact_bps(), 12);
        assert_eq!(quote.route_labels(), vec!["Raydium CPMM"]);
    }

    #[test]
    fn parses_current_quote_shape_without_fee_fields() {
        let raw = r#"{
            "inputMint":"So11111111111111111111111111111111111111112",
            "inAmount":"10000000",
            "outputMint":"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "outAmount":"1015360",
            "otherAmountThreshold":"1010284",
            "swapMode":"ExactIn",
            "slippageBps":50,
            "priceImpactPct":"0",
            "routePlan":[{"swapInfo":{
                "ammKey":"sZxb9vrxJBpFiJBogovhfkYqfapVzveLEU4TmzWv4GN",
                "label":"Meteora DLMM",
                "inputMint":"So11111111111111111111111111111111111111112",
                "outputMint":"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                "inAmount":"10000000",
                "outAmount":"1015360",
                "updateContextSlot":"461671632"
            },"percent":100,"bps":null}],
            "contextSlot":461671635,
            "timeTaken":0.00037
        }"#;
        let quote: JupiterQuote = serde_json::from_str(raw).expect("quote parses");
        assert_eq!(quote.amount_out().unwrap(), 1_015_360);
        assert_eq!(quote.minimum_out().unwrap(), 1_010_284);
        assert_eq!(quote.route_labels(), vec!["Meteora DLMM"]);
        assert!(quote.route_plan[0].swap_info.fee_amount.is_none());
        assert!(quote.route_plan[0].swap_info.fee_mint.is_none());
    }

    #[test]
    fn rejects_swap_simulation_error() {
        let raw = r#"{
            "swapTransaction":"",
            "lastValidBlockHeight":1,
            "simulationError":{"errorCode":"FAILED_TO_SIMULATE_SWAP","error":"rpc failed"}
        }"#;
        let swap: JupiterSwap = serde_json::from_str(raw).unwrap();
        let error = validate_swap(swap).unwrap_err();
        assert!(error.to_string().contains("simulation failed"));
    }

    #[test]
    fn rejects_empty_swap_transaction() {
        let swap = JupiterSwap {
            swap_transaction: String::new(),
            last_valid_block_height: 1,
            prioritization_fee_lamports: Value::Null,
            simulation_error: None,
            error: None,
        };
        assert!(validate_swap(swap).is_err());
    }

    #[test]
    fn quote_request_rejects_zero_amount() {
        let request = QuoteRequest::new(Address::ZERO, Address::ZERO, 0);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let client = JupiterClient::new().unwrap();
        let error = runtime.block_on(client.quote(&request)).unwrap_err();
        assert!(matches!(error, DexError::Math(_)));
    }
}
