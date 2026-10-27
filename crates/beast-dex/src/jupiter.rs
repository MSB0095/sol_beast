//! Jupiter Quote/Swap API client.
//!
//! Jupiter is the fallback for venues we deliberately do not model locally
//! (prop AMMs, RFQ, CLOBs, launchpads and long-tail pools). It is also the
//! independent quote used to reject stale local Radar models before an attempt.

use crate::address::Address;
use crate::error::DexError;
use base64::Engine as _;
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

/// One wire-format instruction as returned by `/swap-instructions`.
///
/// Jupiter serialises these as plain JSON objects rather than Borsh, so the
/// caller converts them into `solana_sdk::instruction::Instruction` when the
/// full transaction is assembled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawInstruction {
    pub program_id: Address,
    pub accounts: Vec<RawAccountMeta>,
    pub data: String,
}

impl RawInstruction {
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Decode the base64 instruction payload.
    pub fn data_bytes(&self) -> std::result::Result<Vec<u8>, base64::DecodeError> {
        base64::engine::general_purpose::STANDARD.decode(&self.data)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAccountMeta {
    pub pubkey: Address,
    #[serde(default)]
    pub is_signer: bool,
    #[serde(default)]
    pub is_writable: bool,
}

/// The instruction decomposition of one swap.
///
/// `setup_instructions` create the token accounts the swap needs,
/// `swap_instruction` performs the exchange, and `cleanup_instruction` unwraps
/// or closes what the swap created. When several legs are composed into one
/// transaction only the *final* leg's cleanup may run, because earlier legs
/// consume the accounts the next leg needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapInstructions {
    #[serde(default)]
    pub token_ledger_instruction: Option<RawInstruction>,
    #[serde(default)]
    pub compute_budget_instructions: Vec<RawInstruction>,
    #[serde(default)]
    pub setup_instructions: Vec<RawInstruction>,
    pub swap_instruction: RawInstruction,
    #[serde(default)]
    pub cleanup_instruction: Option<RawInstruction>,
    #[serde(default)]
    pub other_instructions: Vec<RawInstruction>,
    #[serde(default)]
    pub address_lookup_table_addresses: Vec<Address>,
    #[serde(default)]
    pub prioritization_fee_lamports: Value,
    #[serde(default)]
    pub compute_unit_limit: Option<u64>,
    #[serde(default)]
    pub transaction_version: Value,
    #[serde(default)]
    pub simulation_slot: Option<u64>,
    #[serde(default)]
    pub dynamic_slippage_report: Option<Value>,
    #[serde(default)]
    pub simulation_error: Value,
    #[serde(default)]
    pub blockhash_with_metadata: Option<Value>,
}

impl SwapInstructions {
    /// Every instruction that must precede the actual swap exchange.
    pub fn preamble(&self) -> Vec<&RawInstruction> {
        let mut out = Vec::new();
        if let Some(instruction) = &self.token_ledger_instruction {
            out.push(instruction);
        }
        out.extend(self.compute_budget_instructions.iter());
        out.extend(self.setup_instructions.iter());
        out
    }

    /// Rough serialized size of all instructions, used to stay inside the
    /// 1232-byte transaction limit before anything is signed.
    pub fn payload_bytes(&self) -> usize {
        let mut total = self.swap_instruction.data.len().div_ceil(2) + 1;
        for instruction in self
            .compute_budget_instructions
            .iter()
            .chain(self.setup_instructions.iter())
            .chain(self.other_instructions.iter())
        {
            total += instruction.data.len().div_ceil(2) + 1 + instruction.accounts.len() * 33;
        }
        if let Some(instruction) = &self.token_ledger_instruction {
            total += instruction.data.len().div_ceil(2) + 1;
        }
        total
    }
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

    /// Request the *instruction set* for a swap instead of a signed transaction.
    ///
    /// This is the primitive behind multi-leg atomic arbitrage: several legs are
    /// normally separate transactions because a Jito bundle caps at five
    /// transactions. Concatenating instruction sets from multiple quotes lets a
    /// whole cycle run inside **one** transaction, which removes the per-leg
    /// transaction from the bundle and turns the 5-transaction cap into a
    /// 1232-byte size limit instead.
    pub async fn swap_instructions(
        &self,
        request: &SwapRequest,
    ) -> Result<SwapInstructions, DexError> {
        let body = serde_json::json!({
            "userPublicKey": request.user_public_key,
            "quoteResponse": request.quote,
            "wrapAndUnwrapSol": request.wrap_and_unwrap_sol,
            "dynamicComputeUnitLimit": request.dynamic_compute_unit_limit,
        });
        let mut call = self
            .http
            .post(format!("{}/swap-instructions", self.base_url))
            .json(&body);
        if let Some(api_key) = &self.api_key {
            call = call.header("x-api-key", api_key);
        }
        let response = call.send().await.map_err(http_error)?;
        let status = response.status();
        let text = response.text().await.map_err(http_error)?;
        if !status.is_success() {
            return Err(DexError::Http(format!(
                "Jupiter swap-instructions HTTP {status}: {}",
                truncate(&text)
            )));
        }
        let instructions: SwapInstructions = serde_json::from_str(&text).map_err(DexError::from)?;
        if !instructions.simulation_error.is_null() {
            return Err(DexError::Http(format!(
                "Jupiter swap-instructions simulation failed: {}",
                truncate(&instructions.simulation_error.to_string())
            )));
        }
        if instructions.swap_instruction.data.is_empty() {
            return Err(DexError::Http(
                "Jupiter swap-instructions returned no swap instruction".into(),
            ));
        }
        Ok(instructions)
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
