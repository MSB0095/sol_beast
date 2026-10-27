//! Solana JSON-RPC and Jito bundle clients.
//!
//! All methods are read-only except `send_transaction` and Jito `send_bundle`.
//! The execution crate is the only caller allowed to use those two methods.

use base64::Engine;
use beast_dex::Address;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, RpcError>;

#[derive(Debug, Error, Clone)]
pub enum RpcError {
    #[error("no RPC endpoints configured")]
    NoEndpoints,
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("RPC error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("RPC decode error: {0}")]
    Decode(String),
    #[error("RPC timeout: {0}")]
    Timeout(String),
    #[error("transaction error: {0}")]
    Transaction(String),
}

impl From<reqwest::Error> for RpcError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout(error.to_string())
        } else {
            Self::Http(error.to_string())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub lamports: u64,
    pub owner: Address,
    pub data: Vec<u8>,
    pub executable: bool,
    pub rent_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub slot: u64,
    pub account: Account,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAccountBalance {
    pub amount: u64,
    pub decimals: u8,
    pub ui_amount: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureStatus {
    pub slot: u64,
    pub confirmations: Option<u64>,
    pub err: Option<Value>,
    pub confirmation_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub err: Option<Value>,
    pub logs: Vec<String>,
    pub units_consumed: Option<u64>,
    pub return_data: Option<Value>,
    /// Post-execution account snapshots, requested through
    /// `simulate_transaction_with_accounts`. Empty when not requested or when
    /// the simulation failed before accounts could be captured.
    pub post_accounts: Vec<Option<Value>>,
}

/// The subset of `getTransaction` needed for settlement reconciliation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionDetails {
    pub signature: String,
    pub slot: u64,
    pub block_time: Option<i64>,
    pub fee: u64,
    pub err: Option<Value>,
    pub pre_balances: Vec<u64>,
    pub post_balances: Vec<u64>,
    pub account_keys: Vec<String>,
    pub log_messages: Vec<String>,
}

impl TransactionDetails {
    /// Net lamport delta for one account, inclusive of fees paid by that account.
    pub fn balance_delta(&self, address: &str) -> Option<i64> {
        let index = self.account_keys.iter().position(|key| key == address)?;
        let before = *self.pre_balances.get(index)? as i128;
        let after = *self.post_balances.get(index)? as i128;
        i64::try_from(after - before).ok()
    }
}

/// What an endpoint is best at. The client routes each JSON-RPC method to a
/// matching endpoint first and only spills to other roles when none is healthy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointRole {
    Feed,
    Exec,
    Discovery,
    Rent,
    Fallback,
}

impl EndpointRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Feed => "feed",
            Self::Exec => "exec",
            Self::Discovery => "discovery",
            Self::Rent => "rent",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EndpointOptions {
    pub name: String,
    pub url: String,
    pub role: EndpointRole,
    /// Sustained requests/second allowed on this endpoint.
    pub max_rps: u32,
    /// Separate limit for heavy methods that free tiers throttle harder.
    pub heavy_rps: u32,
    /// Process-local call budget. 0 disables it. The window resets after 30
    /// days of uptime; it is a safety margin, not provider-side accounting.
    pub monthly_call_budget: u64,
    /// Lower wins when several endpoints serve the same role.
    pub priority: u8,
}

impl EndpointOptions {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            role: EndpointRole::Fallback,
            max_rps: 5,
            heavy_rps: 2,
            monthly_call_budget: 0,
            priority: 10,
        }
    }

    pub fn with_role(mut self, role: EndpointRole) -> Self {
        self.role = role;
        self
    }

    /// Set the role from its configuration spelling (`feed`, `exec`, ...).
    pub fn with_role_name(mut self, role: &str) -> Self {
        self.role = match role {
            "feed" => EndpointRole::Feed,
            "exec" => EndpointRole::Exec,
            "discovery" => EndpointRole::Discovery,
            "rent" => EndpointRole::Rent,
            _ => EndpointRole::Fallback,
        };
        self
    }

    pub fn with_rps(mut self, max_rps: u32, heavy_rps: u32) -> Self {
        self.max_rps = max_rps.max(1);
        self.heavy_rps = heavy_rps.max(1);
        self
    }

    pub fn with_budget(mut self, monthly_call_budget: u64) -> Self {
        self.monthly_call_budget = monthly_call_budget;
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Live health of one endpoint, refreshed on every request.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointHealth {
    pub name: String,
    pub url: String,
    pub role: &'static str,
    pub max_rps: u32,
    pub heavy_rps: u32,
    pub total_calls: u64,
    pub budget_calls: u64,
    pub monthly_call_budget: u64,
    pub priority: u8,
    pub rate_limited: u64,
    pub circuit_open: bool,
    /// Milliseconds until the circuit breaker closes. 0 when healthy.
    pub circuit_remaining_ms: u64,
}

#[derive(Debug, Default)]
struct EndpointState {
    calls: AtomicU64,
    /// Calls counted against `monthly_call_budget` in the current window.
    budget_calls: AtomicU64,
    /// Unix millis at which the current budget window started.
    budget_window_start: AtomicU64,
    rate_limited: AtomicU64,
    /// Unix millis until which this endpoint is skipped after a rate limit.
    cooldown_until: AtomicU64,
}

/// Token-bucket state for one rate-limit key (endpoint or endpoint+method).
#[derive(Debug, Default)]
struct Bucket {
    /// Next instant at which a slot becomes free, in millis.
    next_free_ms: AtomicU64,
}

#[derive(Debug)]
pub struct RpcClient {
    http: reqwest::Client,
    endpoints: Vec<EndpointOptions>,
    states: Vec<EndpointState>,
    buckets: Vec<Bucket>,
    /// Maps `endpoint_index * 2 + heavy_slot` into `buckles`.
    heavy_buckets: Vec<Bucket>,
    cursor: AtomicUsize,
    request_id: AtomicU64,
    commitment: String,
    max_retries: u32,
    circuit_breaker_ms: u64,
}

/// Methods that free tiers throttle independently of the general RPS limit.
const HEAVY_METHODS: &[&str] = &[
    "getProgramAccounts",
    "getTokenAccountsByOwner",
    "getTokenAccountsByDelegate",
    "getTokenLargestAccounts",
];

impl RpcClient {
    pub fn new(
        urls: Vec<String>,
        commitment: impl Into<String>,
        timeout_ms: u64,
        max_retries: u32,
    ) -> Result<Self> {
        let endpoints: Vec<EndpointOptions> = urls
            .into_iter()
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .enumerate()
            .map(|(index, url)| EndpointOptions::new(format!("legacy-{index}"), url))
            .collect();
        Self::build(endpoints, commitment, timeout_ms, max_retries, 15_000)
    }

    /// Build a client from named endpoints with per-endpoint limits.
    pub fn with_endpoints(
        endpoints: Vec<EndpointOptions>,
        commitment: impl Into<String>,
        timeout_ms: u64,
        max_retries: u32,
        circuit_breaker_ms: u64,
    ) -> Result<Self> {
        Self::build(
            endpoints,
            commitment,
            timeout_ms,
            max_retries,
            circuit_breaker_ms,
        )
    }

    fn build(
        endpoints: Vec<EndpointOptions>,
        commitment: impl Into<String>,
        timeout_ms: u64,
        max_retries: u32,
        circuit_breaker_ms: u64,
    ) -> Result<Self> {
        if endpoints.is_empty() {
            return Err(RpcError::NoEndpoints);
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms.clamp(500, 60_000)))
            .user_agent(concat!("sol-beast-rpc/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let count = endpoints.len();
        Ok(Self {
            http,
            endpoints,
            states: (0..count).map(|_| EndpointState::default()).collect(),
            buckets: (0..count).map(|_| Bucket::default()).collect(),
            heavy_buckets: (0..count).map(|_| Bucket::default()).collect(),
            cursor: AtomicUsize::new(0),
            request_id: AtomicU64::new(1),
            commitment: commitment.into(),
            max_retries,
            circuit_breaker_ms,
        })
    }

    pub fn endpoints(&self) -> &[EndpointOptions] {
        &self.endpoints
    }

    /// Snapshot of per-endpoint health for the UI and the doctor command.
    pub fn health(&self) -> Vec<EndpointHealth> {
        let now = now_ms();
        (0..self.endpoints.len())
            .map(|index| {
                let state = &self.states[index];
                let cooldown = state.cooldown_until.load(Ordering::Relaxed);
                EndpointHealth {
                    name: self.endpoints[index].name.clone(),
                    url: self.endpoints[index].url.clone(),
                    role: self.endpoints[index].role.as_str(),
                    max_rps: self.endpoints[index].max_rps,
                    heavy_rps: self.endpoints[index].heavy_rps,
                    total_calls: state.calls.load(Ordering::Relaxed),
                    budget_calls: state.budget_calls.load(Ordering::Relaxed),
                    monthly_call_budget: self.endpoints[index].monthly_call_budget,
                    priority: self.endpoints[index].priority,
                    rate_limited: state.rate_limited.load(Ordering::Relaxed),
                    circuit_open: cooldown > now,
                    circuit_remaining_ms: cooldown.saturating_sub(now),
                }
            })
            .collect()
    }

    pub fn total_calls(&self) -> u64 {
        self.states
            .iter()
            .map(|state| state.calls.load(Ordering::Relaxed))
            .sum()
    }

    /// Reserve a slot in the token bucket for `(endpoint, method)`.
    ///
    /// Free tiers fail on requests-per-second, not on compute units, so the
    /// only thing that keeps a long-lived feed alive is refusing to issue a
    /// request the provider will answer with 429.
    async fn acquire(&self, index: usize, method: &str) {
        let (limit, bucket) = if HEAVY_METHODS.contains(&method) {
            (
                u64::from(self.endpoints[index].heavy_rps),
                &self.heavy_buckets[index],
            )
        } else {
            (
                u64::from(self.endpoints[index].max_rps),
                &self.buckets[index],
            )
        };
        if limit == 0 {
            return;
        }
        let spacing_ms = (1_000 / limit).max(1);
        loop {
            let now = now_ms();
            let next = bucket.next_free_ms.load(Ordering::Relaxed);
            if next <= now {
                // Claim the slot; if we lose the race, another caller claimed it.
                if bucket
                    .next_free_ms
                    .compare_exchange(next, now + spacing_ms, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    return;
                }
                continue;
            }
            let wait_ms = next.saturating_sub(now).min(spacing_ms);
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
        }
    }

    /// Pick the next healthy endpoint for `role`.
    ///
    /// Routing is strict about preference and loose about survival:
    /// 1. an endpoint whose configured role matches the call;
    /// 2. a `fallback` endpoint, which may serve any role;
    /// 3. any healthy endpoint, so a misconfigured or exhausted provider can
    ///    never make the process look dead.
    fn pick(&self, role: EndpointRole) -> Option<usize> {
        let now = now_ms();
        let count = self.endpoints.len();
        if count == 0 {
            return None;
        }
        let healthy = |index: usize| {
            if self.states[index].cooldown_until.load(Ordering::Relaxed) > now {
                return false;
            }
            let budget = self.endpoints[index].monthly_call_budget;
            if budget == 0 {
                return true;
            }
            let window = self.states[index]
                .budget_window_start
                .load(Ordering::Relaxed);
            if window == 0 || now.saturating_sub(window) >= 30 * 24 * 60 * 60 * 1_000 {
                // Start a fresh window on first use and after 30 days.
                if window == 0 {
                    let _ = self.states[index].budget_window_start.compare_exchange(
                        0,
                        now,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    );
                } else {
                    self.states[index]
                        .budget_window_start
                        .store(now, Ordering::Relaxed);
                    self.states[index].budget_calls.store(0, Ordering::Relaxed);
                }
                return true;
            }
            self.states[index].budget_calls.load(Ordering::Relaxed) < budget
        };
        for pass in 0..3 {
            let start = self.cursor.fetch_add(1, Ordering::Relaxed);
            let mut best: Option<(u8, usize)> = None;
            for offset in 0..count {
                let index = (start + offset) % count;
                if !healthy(index) {
                    continue;
                }
                let matches = match pass {
                    0 => self.endpoints[index].role == role,
                    1 => self.endpoints[index].role == EndpointRole::Fallback,
                    _ => true,
                };
                if !matches {
                    continue;
                }
                let priority = self.endpoints[index].priority;
                if best
                    .map(|(best_priority, _)| priority < best_priority)
                    .unwrap_or(true)
                {
                    best = Some((priority, index));
                }
            }
            if let Some((_, index)) = best {
                return Some(index);
            }
        }
        None
    }

    /// Which endpoint role a JSON-RPC method is routed to by default.
    fn role_for_method(method: &str) -> EndpointRole {
        match method {
            "getProgramAccounts" | "getTokenLargestAccounts" => EndpointRole::Discovery,
            "getTokenAccountsByOwner" | "getTokenAccountsByDelegate" => EndpointRole::Rent,
            "sendTransaction"
            | "sendRawTransaction"
            | "simulateTransaction"
            | "getLatestBlockhash"
            | "isBlockhashValid"
            | "getSignatureStatuses"
            | "getTransaction"
            | "getBalance"
            | "getAccountInfo" => EndpointRole::Exec,
            _ => EndpointRole::Feed,
        }
    }

    async fn call<T: for<'de> Deserialize<'de>>(&self, method: &str, params: Value) -> Result<T> {
        let mut last_error = None;
        for attempt in 0..=self.max_retries {
            let Some(index) = self.pick(Self::role_for_method(method)) else {
                let cooldown = self
                    .states
                    .iter()
                    .map(|state| state.cooldown_until.load(Ordering::Relaxed))
                    .max()
                    .unwrap_or(0);
                return Err(RpcError::Http(format!(
                    "no RPC endpoint available: {} endpoints are cooling down or over budget (next cooldown ends in {}ms)",
                    self.endpoints.len(),
                    cooldown.saturating_sub(now_ms())
                )));
            };
            self.acquire(index, method).await;
            let url = &self.endpoints[index].url;
            let body = json!({
                "jsonrpc": "2.0",
                "id": self.request_id.fetch_add(1, Ordering::Relaxed),
                "method": method,
                "params": params,
            });
            self.states[index].calls.fetch_add(1, Ordering::Relaxed);
            if self.endpoints[index].monthly_call_budget > 0 {
                self.states[index]
                    .budget_calls
                    .fetch_add(1, Ordering::Relaxed);
            }
            let result = self
                .http
                .post(url)
                .json(&body)
                .send()
                .await
                .map_err(RpcError::from)
                .and_then(|response| {
                    let status = response.status();
                    if !status.is_success() {
                        Err(RpcError::Http(format!("HTTP {status} from {url}")))
                    } else {
                        Ok(response)
                    }
                });
            let response = match result {
                Ok(response) => response,
                Err(error) => {
                    if is_rate_limited(&error) {
                        self.trip(index);
                    }
                    tracing::warn!(method, url, attempt, error = %error, "RPC request failed");
                    last_error = Some(error);
                    continue;
                }
            };
            let text = response.text().await.map_err(RpcError::from)?;
            let envelope: RpcEnvelope<T> = serde_json::from_str(&text)
                .map_err(|error| RpcError::Decode(format!("{error}: {}", snippet(&text))))?;
            if let Some(error) = envelope.error {
                // A JSON-RPC error is deterministic; retrying another endpoint
                // can still help when a provider is rate-limited.
                let rpc_error = RpcError::Rpc {
                    code: error.code,
                    message: error.message,
                };
                if is_rate_limited(&rpc_error) {
                    self.trip(index);
                }
                tracing::warn!(method, url, attempt, error = %rpc_error, "RPC returned error");
                last_error = Some(rpc_error);
                continue;
            }
            return envelope
                .result
                .ok_or_else(|| RpcError::Decode("missing result".into()));
        }
        Err(last_error.unwrap_or_else(|| RpcError::Decode("RPC retries exhausted".into())))
    }

    /// Put an endpoint on cooldown after a rate-limit or server error.
    fn trip(&self, index: usize) {
        self.states[index]
            .rate_limited
            .fetch_add(1, Ordering::Relaxed);
        self.states[index]
            .cooldown_until
            .store(now_ms() + self.circuit_breaker_ms, Ordering::Relaxed);
        tracing::debug!(
            endpoint = %self.endpoints[index].name,
            cooldown_ms = self.circuit_breaker_ms,
            "RPC endpoint circuit opened"
        );
    }

    pub async fn get_slot(&self) -> Result<u64> {
        self.call("getSlot", json!([{"commitment": self.commitment}]))
            .await
    }

    pub async fn get_health(&self) -> Result<String> {
        self.call("getHealth", json!([])).await
    }

    pub async fn get_version(&self) -> Result<Value> {
        self.call("getVersion", json!([])).await
    }

    pub async fn get_balance(&self, address: Address) -> Result<u64> {
        let result: Value = self
            .call(
                "getBalance",
                json!([address.to_string(), {"commitment": self.commitment}]),
            )
            .await?;
        result
            .get("value")
            .and_then(Value::as_u64)
            .ok_or_else(|| RpcError::Decode("getBalance missing value".into()))
    }

    pub async fn get_account_info(&self, address: Address) -> Result<Option<AccountInfo>> {
        let result: Value = self
            .call(
                "getAccountInfo",
                json!([
                    address.to_string(),
                    {"encoding": "base64", "commitment": self.commitment}
                ]),
            )
            .await?;
        let slot = result
            .pointer("/context/slot")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let value = result.get("value").cloned().unwrap_or(Value::Null);
        if value.is_null() {
            return Ok(None);
        }
        let account = parse_account(&value)?;
        Ok(Some(AccountInfo { slot, account }))
    }

    pub async fn get_multiple_accounts(
        &self,
        addresses: &[Address],
    ) -> Result<Vec<Option<Account>>> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        if addresses.len() > 100 {
            return Err(RpcError::Decode(
                "getMultipleAccounts supports at most 100 addresses".into(),
            ));
        }
        let keys: Vec<String> = addresses.iter().map(ToString::to_string).collect();
        let result: Value = self
            .call(
                "getMultipleAccounts",
                json!([
                    keys,
                    {"encoding": "base64", "commitment": self.commitment}
                ]),
            )
            .await?;
        let values = result
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(|| RpcError::Decode("getMultipleAccounts missing values".into()))?;
        values
            .iter()
            .map(|value| {
                if value.is_null() {
                    Ok(None)
                } else {
                    parse_account(value).map(Some)
                }
            })
            .collect()
    }

    pub async fn get_token_account_balance(&self, address: Address) -> Result<TokenAccountBalance> {
        let result: Value = self
            .call(
                "getTokenAccountBalance",
                json!([address.to_string(), {"commitment": self.commitment}]),
            )
            .await?;
        let value = result
            .get("value")
            .ok_or_else(|| RpcError::Decode("getTokenAccountBalance missing value".into()))?;
        let amount = value
            .get("amount")
            .and_then(Value::as_str)
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| RpcError::Decode("invalid token amount".into()))?;
        let decimals = value
            .get("decimals")
            .and_then(Value::as_u64)
            .ok_or_else(|| RpcError::Decode("invalid token decimals".into()))?
            as u8;
        Ok(TokenAccountBalance {
            amount,
            decimals,
            ui_amount: value
                .get("uiAmountString")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        })
    }

    pub async fn get_token_accounts_by_owner(
        &self,
        owner: Address,
        token_program: Address,
    ) -> Result<Vec<(Address, Account)>> {
        let result: Value = self
            .call(
                "getTokenAccountsByOwner",
                json!([
                    owner.to_string(),
                    {"programId": token_program.to_string()},
                    {"encoding": "base64", "commitment": self.commitment}
                ]),
            )
            .await?;
        let values = result
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(|| RpcError::Decode("getTokenAccountsByOwner missing value".into()))?;
        values
            .iter()
            .map(|entry| {
                let pubkey = entry
                    .get("pubkey")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RpcError::Decode("token account missing pubkey".into()))?
                    .parse::<Address>()
                    .map_err(|error| RpcError::Decode(error.to_string()))?;
                let account = entry
                    .get("account")
                    .ok_or_else(|| RpcError::Decode("token account missing account".into()))?;
                Ok((pubkey, parse_account(account)?))
            })
            .collect()
    }

    pub async fn get_transaction(&self, signature: &str) -> Result<Option<TransactionDetails>> {
        let result: Value = self
            .call(
                "getTransaction",
                json!([
                    signature,
                    {
                        "encoding": "json",
                        "commitment": self.commitment,
                        "maxSupportedTransactionVersion": 0
                    }
                ]),
            )
            .await?;
        if result.is_null() {
            return Ok(None);
        }
        let meta = result
            .get("meta")
            .ok_or_else(|| RpcError::Decode("transaction missing meta".into()))?;
        let message = result
            .pointer("/transaction/message")
            .ok_or_else(|| RpcError::Decode("transaction missing message".into()))?;
        let mut account_keys: Vec<String> = message
            .get("accountKeys")
            .and_then(Value::as_array)
            .map(|keys| {
                keys.iter()
                    .filter_map(|key| key.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(loaded) = meta.get("loadedAddresses") {
            for key in ["writable", "readonly"] {
                if let Some(addresses) = loaded.get(key).and_then(Value::as_array) {
                    account_keys.extend(
                        addresses
                            .iter()
                            .filter_map(|address| address.as_str().map(ToString::to_string)),
                    );
                }
            }
        }
        Ok(Some(TransactionDetails {
            signature: signature.to_string(),
            slot: result.get("slot").and_then(Value::as_u64).unwrap_or(0),
            block_time: result.get("blockTime").and_then(Value::as_i64),
            fee: meta.get("fee").and_then(Value::as_u64).unwrap_or(0),
            err: meta.get("err").cloned().filter(|value| !value.is_null()),
            pre_balances: meta
                .get("preBalances")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default(),
            post_balances: meta
                .get("postBalances")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default(),
            account_keys,
            log_messages: meta
                .get("logMessages")
                .and_then(Value::as_array)
                .map(|logs| {
                    logs.iter()
                        .filter_map(Value::as_str)
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        }))
    }

    pub async fn get_program_accounts(
        &self,
        program_id: Address,
        filters: Vec<Value>,
        data_slice: Option<(usize, usize)>,
    ) -> Result<Vec<(Address, Account)>> {
        let mut config = json!({
            "encoding": "base64",
            "commitment": self.commitment,
            "filters": filters,
        });
        if let Some((offset, length)) = data_slice {
            config["dataSlice"] = json!({"offset": offset, "length": length});
        }
        let result: Vec<Value> = self
            .call(
                "getProgramAccounts",
                json!([program_id.to_string(), config]),
            )
            .await?;
        result
            .iter()
            .map(|entry| {
                let pubkey = entry
                    .get("pubkey")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RpcError::Decode("program account missing pubkey".into()))?
                    .parse::<Address>()
                    .map_err(|error| RpcError::Decode(error.to_string()))?;
                let account = entry
                    .get("account")
                    .ok_or_else(|| RpcError::Decode("program account missing account".into()))?;
                Ok((pubkey, parse_account(account)?))
            })
            .collect()
    }

    pub async fn get_latest_blockhash(&self) -> Result<(String, u64)> {
        let result: Value = self
            .call(
                "getLatestBlockhash",
                json!([{"commitment": self.commitment}]),
            )
            .await?;
        let value = result
            .get("value")
            .ok_or_else(|| RpcError::Decode("missing blockhash value".into()))?;
        let blockhash = value
            .get("blockhash")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::Decode("missing blockhash".into()))?
            .to_string();
        let height = value
            .get("lastValidBlockHeight")
            .and_then(Value::as_u64)
            .ok_or_else(|| RpcError::Decode("missing lastValidBlockHeight".into()))?;
        Ok((blockhash, height))
    }

    pub async fn send_transaction(
        &self,
        encoded_transaction: &str,
        skip_preflight: bool,
    ) -> Result<String> {
        self.call(
            "sendTransaction",
            json!([
                encoded_transaction,
                {
                    "encoding": "base64",
                    "skipPreflight": skip_preflight,
                    "preflightCommitment": self.commitment,
                    "maxRetries": 0
                }
            ]),
        )
        .await
    }

    pub async fn simulate_transaction(
        &self,
        encoded_transaction: &str,
    ) -> Result<SimulationResult> {
        self.simulate_transaction_with_accounts(encoded_transaction, &[], false)
            .await
    }

    /// Simulate and additionally return post-execution snapshots of `addresses`.
    ///
    /// This is how a live probe can observe the *effect* of an instruction
    /// (for example, the exact tokens credited by a flash borrow) instead of
    /// inferring it from logs.
    pub async fn simulate_transaction_with_accounts(
        &self,
        encoded_transaction: &str,
        addresses: &[String],
        include_inner_instructions: bool,
    ) -> Result<SimulationResult> {
        let mut config = json!({
            "encoding": "base64",
            "sigVerify": false,
            "replaceRecentBlockhash": true,
            "commitment": "processed",
            "innerInstructions": include_inner_instructions,
        });
        if !addresses.is_empty() {
            config["accounts"] = json!({
                "encoding": "base64",
                "addresses": addresses,
            });
        }
        let result: Value = self
            .call("simulateTransaction", json!([encoded_transaction, config]))
            .await?;
        let value = result
            .get("value")
            .ok_or_else(|| RpcError::Decode("simulateTransaction missing value".into()))?;
        Ok(SimulationResult {
            err: value.get("err").cloned().filter(|value| !value.is_null()),
            logs: value
                .get("logs")
                .and_then(Value::as_array)
                .map(|logs| {
                    logs.iter()
                        .filter_map(Value::as_str)
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            units_consumed: value.get("unitsConsumed").and_then(Value::as_u64),
            return_data: value.get("returnData").cloned(),
            post_accounts: value
                .get("accounts")
                .and_then(Value::as_array)
                .map(|accounts| {
                    accounts
                        .iter()
                        .map(|account| (!account.is_null()).then(|| account.clone()))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub async fn get_signature_statuses(
        &self,
        signatures: &[String],
    ) -> Result<Vec<Option<SignatureStatus>>> {
        let result: Value = self
            .call(
                "getSignatureStatuses",
                json!([signatures, {"searchTransactionHistory": true}]),
            )
            .await?;
        let values = result
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(|| RpcError::Decode("missing signature statuses".into()))?;
        values
            .iter()
            .map(|value| {
                if value.is_null() {
                    return Ok(None);
                }
                Ok(Some(SignatureStatus {
                    slot: value.get("slot").and_then(Value::as_u64).unwrap_or(0),
                    confirmations: value.get("confirmations").and_then(Value::as_u64),
                    err: value.get("err").cloned().filter(|value| !value.is_null()),
                    confirmation_status: value
                        .get("confirmationStatus")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                }))
            })
            .collect()
    }
}

#[derive(Deserialize)]
struct RpcEnvelope<T> {
    result: Option<T>,
    error: Option<RpcEnvelopeError>,
}

#[derive(Deserialize)]
struct RpcEnvelopeError {
    code: i64,
    message: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn is_rate_limited(error: &RpcError) -> bool {
    match error {
        RpcError::Http(message) => message.contains("429") || message.contains("Too Many Requests"),
        // -32005 is the generic "node is behind" / overload code used by many
        // providers; -32429 is the Solana JSON-RPC rate-limit code.
        RpcError::Rpc { code, .. } => *code == -32_429 || *code == -32_005,
        _ => false,
    }
}

fn parse_account(value: &Value) -> Result<Account> {
    let lamports = value
        .get("lamports")
        .and_then(Value::as_u64)
        .ok_or_else(|| RpcError::Decode("account missing lamports".into()))?;
    let owner = value
        .get("owner")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::Decode("account missing owner".into()))?
        .parse::<Address>()
        .map_err(|error| RpcError::Decode(error.to_string()))?;
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|parts| parts.first())
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::Decode("account missing base64 data".into()))?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| RpcError::Decode(error.to_string()))?;
    Ok(Account {
        lamports,
        owner,
        data,
        executable: value
            .get("executable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        rent_epoch: value.get("rentEpoch").and_then(Value::as_u64).unwrap_or(0),
    })
}

/// SPL Token account amount at offset 64.
pub fn token_account_amount(data: &[u8]) -> Option<u64> {
    let bytes = data.get(64..72)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

/// SPL Token mint decimals at offset 44.
pub fn mint_decimals(data: &[u8]) -> Option<u8> {
    data.get(44).copied()
}

fn snippet(text: &str) -> String {
    text.chars().take(240).collect()
}

// ---------------------------------------------------------------------------
// Jito
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct JitoClient {
    http: reqwest::Client,
    urls: Vec<String>,
    auth: Option<String>,
    cursor: AtomicUsize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleStatus {
    pub bundle_id: String,
    pub status: String,
    pub landed_slot: Option<u64>,
    pub transactions: Vec<String>,
    pub confirmation_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TipFloorEntry {
    pub time: Option<String>,
    pub landed_tips_25th_percentile: Option<f64>,
    pub landed_tips_50th_percentile: Option<f64>,
    pub landed_tips_75th_percentile: Option<f64>,
    pub landed_tips_95th_percentile: Option<f64>,
    pub landed_tips_99th_percentile: Option<f64>,
    pub ema_landed_tips_50th_percentile: Option<f64>,
}

impl JitoClient {
    pub fn new(urls: Vec<String>, auth: Option<String>) -> Result<Self> {
        let urls: Vec<String> = urls
            .into_iter()
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .collect();
        if urls.is_empty() {
            return Err(RpcError::NoEndpoints);
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent(concat!("sol-beast-jito/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            urls,
            auth,
            cursor: AtomicUsize::new(0),
        })
    }

    fn next_url(&self) -> &str {
        let index = self.cursor.fetch_add(1, Ordering::Relaxed) % self.urls.len();
        &self.urls[index]
    }

    async fn call<T: for<'de> Deserialize<'de>>(&self, method: &str, params: Value) -> Result<T> {
        let mut last_error = None;
        for attempt in 0..=1 {
            let url = self.next_url();
            let body = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params,
            });
            let mut request = self.http.post(format!("{url}/api/v1/bundles")).json(&body);
            if let Some(auth) = &self.auth {
                request = request.header("x-jito-auth", auth);
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    let text = response.text().await.map_err(RpcError::from)?;
                    if !status.is_success() {
                        last_error = Some(RpcError::Http(format!(
                            "Jito HTTP {status}: {}",
                            snippet(&text)
                        )));
                        continue;
                    }
                    let envelope: RpcEnvelope<T> =
                        serde_json::from_str(&text).map_err(|error| {
                            RpcError::Decode(format!("{error}: {}", snippet(&text)))
                        })?;
                    if let Some(error) = envelope.error {
                        last_error = Some(RpcError::Rpc {
                            code: error.code,
                            message: error.message,
                        });
                        continue;
                    }
                    return envelope
                        .result
                        .ok_or_else(|| RpcError::Decode("missing Jito result".into()));
                }
                Err(error) => {
                    last_error = Some(RpcError::from(error));
                    tracing::warn!(method, url, attempt, error = %last_error.as_ref().unwrap(), "Jito request failed");
                }
            }
        }
        Err(last_error.unwrap_or_else(|| RpcError::Decode("Jito retries exhausted".into())))
    }

    pub async fn tip_accounts(&self) -> Result<Vec<Address>> {
        let accounts: Vec<String> = self.call("getTipAccounts", json!([])).await?;
        accounts
            .iter()
            .map(|account| {
                account
                    .parse::<Address>()
                    .map_err(|error| RpcError::Decode(error.to_string()))
            })
            .collect()
    }

    pub async fn send_bundle(&self, encoded_transactions: &[String]) -> Result<String> {
        if encoded_transactions.is_empty() {
            return Err(RpcError::Transaction("empty Jito bundle".into()));
        }
        self.call(
            "sendBundle",
            json!([encoded_transactions, {"encoding": "base64"}]),
        )
        .await
    }

    pub async fn get_bundle_statuses(&self, bundle_ids: &[String]) -> Result<Vec<BundleStatus>> {
        let result: Value = self.call("getBundleStatuses", json!([bundle_ids])).await?;
        let values = result
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        values
            .into_iter()
            .map(|value| {
                Ok(BundleStatus {
                    bundle_id: value
                        .get("bundle_id")
                        .or_else(|| value.get("bundleId"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    status: value
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    landed_slot: value
                        .get("landed_slot")
                        .or_else(|| value.get("landedSlot"))
                        .and_then(Value::as_u64),
                    transactions: value
                        .get("transactions")
                        .and_then(Value::as_array)
                        .map(|txs| {
                            txs.iter()
                                .filter_map(Value::as_str)
                                .map(ToString::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    confirmation_status: value
                        .get("confirmation_status")
                        .or_else(|| value.get("confirmationStatus"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                })
            })
            .collect()
    }

    pub async fn simulate_bundle(&self, encoded_transactions: &[String]) -> Result<Value> {
        let value: Value = self
            .call(
                "simulateBundle",
                json!([{"encodedTransactions": encoded_transactions}]),
            )
            .await?;
        validate_simulation(&value)?;
        Ok(value)
    }

    pub async fn tip_floor(&self, url: &str, percentile: u8) -> Result<u64> {
        let response = reqwest::Client::new()
            .get(url)
            .timeout(Duration::from_secs(8))
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(RpcError::Http(format!("tip floor HTTP {status}")));
        }
        let entries: Vec<TipFloorEntry> = serde_json::from_str(&text)
            .map_err(|error| RpcError::Decode(format!("tip floor: {error}")))?;
        let entry = entries
            .first()
            .ok_or_else(|| RpcError::Decode("empty tip floor".into()))?;
        let sol = match percentile {
            0..=25 => entry.landed_tips_25th_percentile,
            26..=50 => entry.landed_tips_50th_percentile,
            51..=75 => entry.landed_tips_75th_percentile,
            76..=95 => entry.landed_tips_95th_percentile,
            _ => entry.landed_tips_99th_percentile,
        }
        .ok_or_else(|| RpcError::Decode("missing tip percentile".into()))?;
        Ok((sol.max(0.0) * 1_000_000_000.0).round() as u64)
    }
}

fn validate_simulation(value: &Value) -> Result<()> {
    let summary = value
        .pointer("/value/summary")
        .or_else(|| value.get("summary"))
        .ok_or_else(|| RpcError::Decode("Jito simulateBundle response has no summary".into()))?;
    match summary {
        Value::String(status) if status.eq_ignore_ascii_case("succeeded") => Ok(()),
        Value::Object(map) if map.contains_key("succeeded") => Ok(()),
        Value::Object(map) => {
            let failed = map.get("failed").unwrap_or(summary);
            Err(RpcError::Transaction(format!(
                "Jito bundle simulation failed: {}",
                snippet(&failed.to_string())
            )))
        }
        Value::String(status) => Err(RpcError::Transaction(format!(
            "Jito bundle simulation did not succeed: {status}"
        ))),
        other => Err(RpcError::Decode(format!(
            "unrecognized Jito simulation summary: {}",
            snippet(&other.to_string())
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_successful_jito_simulation() {
        let value = json!({
            "context": {"slot": 1},
            "value": {"summary": "succeeded", "transactionResults": []}
        });
        assert!(validate_simulation(&value).is_ok());
    }

    #[test]
    fn rejects_failed_jito_simulation() {
        let value = json!({
            "context": {"slot": 1},
            "value": {
                "summary": {"failed": {
                    "error": {"TransactionFailure": {"InstructionError": [2, {"Custom": 1}]}},
                    "tx_signature": "abc"
                }},
                "transactionResults": [{"err": {"InstructionError": [2, {"Custom": 1}]}}]
            }
        });
        let error = validate_simulation(&value).unwrap_err();
        assert!(error.to_string().contains("simulation failed"));
        assert!(error.to_string().contains("Custom"));
    }

    #[test]
    fn parses_token_account_amount() {
        let mut data = vec![0u8; 165];
        data[64..72].copy_from_slice(&12_345_678u64.to_le_bytes());
        assert_eq!(token_account_amount(&data), Some(12_345_678));
        assert_eq!(token_account_amount(&[0u8; 10]), None);
    }

    #[test]
    fn parses_mint_decimals() {
        let mut data = vec![0u8; 82];
        data[44] = 6;
        assert_eq!(mint_decimals(&data), Some(6));
    }

    #[test]
    fn parses_base64_account() {
        let value = json!({
            "lamports": 123,
            "owner": "So11111111111111111111111111111111111111112",
            "data": [base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]), "base64"],
            "executable": false,
            "rentEpoch": 0
        });
        let account = parse_account(&value).unwrap();
        assert_eq!(account.lamports, 123);
        assert_eq!(account.data, vec![1, 2, 3]);
    }

    fn client(roles: &[EndpointRole]) -> RpcClient {
        RpcClient::with_endpoints(
            roles
                .iter()
                .enumerate()
                .map(|(index, role)| {
                    EndpointOptions::new(format!("e{index}"), "http://localhost").with_role(*role)
                })
                .collect(),
            "confirmed",
            1_000,
            0,
            0,
        )
        .unwrap()
    }

    #[test]
    fn methods_map_to_their_roles() {
        assert_eq!(
            RpcClient::role_for_method("getProgramAccounts"),
            EndpointRole::Discovery
        );
        assert_eq!(
            RpcClient::role_for_method("getTokenAccountsByOwner"),
            EndpointRole::Rent
        );
        assert_eq!(
            RpcClient::role_for_method("simulateTransaction"),
            EndpointRole::Exec
        );
        assert_eq!(
            RpcClient::role_for_method("getAccountInfo"),
            EndpointRole::Exec
        );
        assert_eq!(
            RpcClient::role_for_method("getMultipleAccounts"),
            EndpointRole::Feed
        );
    }

    #[test]
    fn routing_prefers_the_matching_role() {
        let rpc = client(&[
            EndpointRole::Feed,
            EndpointRole::Discovery,
            EndpointRole::Exec,
        ]);
        let picked = rpc.pick(EndpointRole::Discovery).unwrap();
        assert_eq!(rpc.endpoints[picked].role, EndpointRole::Discovery);
        let picked = rpc.pick(EndpointRole::Exec).unwrap();
        assert_eq!(rpc.endpoints[picked].role, EndpointRole::Exec);
    }

    #[test]
    fn routing_uses_fallback_before_crossing_roles() {
        let rpc = client(&[EndpointRole::Feed, EndpointRole::Fallback]);
        let picked = rpc.pick(EndpointRole::Exec).unwrap();
        assert_eq!(rpc.endpoints[picked].role, EndpointRole::Fallback);
    }

    #[test]
    fn routing_still_survives_without_a_fallback() {
        let rpc = client(&[EndpointRole::Feed]);
        let picked = rpc.pick(EndpointRole::Exec).unwrap();
        assert_eq!(rpc.endpoints[picked].role, EndpointRole::Feed);
    }

    #[test]
    fn parses_tip_floor() {
        let raw = r#"[{
            "time":"2026-01-01T00:00:00Z",
            "landed_tips_25th_percentile":0.000001,
            "landed_tips_50th_percentile":0.000002,
            "landed_tips_75th_percentile":0.000003,
            "landed_tips_95th_percentile":0.000004,
            "landed_tips_99th_percentile":0.000005
        }]"#;
        let entries: Vec<TipFloorEntry> = serde_json::from_str(raw).unwrap();
        assert_eq!(entries.len(), 1);
        assert!((entries[0].landed_tips_75th_percentile.unwrap() - 0.000003).abs() < 1e-12);
    }
}
