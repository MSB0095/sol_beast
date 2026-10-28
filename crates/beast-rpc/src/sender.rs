//! Helius Sender client.
//!
//! Helius Sender is Helius's low-latency transaction submission service: it
//! accepts a standard `sendTransaction` JSON-RPC call and forwards it over
//! Helius's own validator connections instead of the public gossip network.
//! For a transaction that must land inside one or two slots — which is every
//! arbitrage — the difference between the default gossip path and a staked
//! forwarding path is the difference between landing and not landing.
//!
//! Two properties are used here:
//!
//! 1. **It is plain JSON-RPC.** Same `sendTransaction` request and response
//!    shape as any Solana RPC node, so a client needs no new code to talk to
//!    it. That also means an existing RPC client can be pointed at it and
//!    nothing else changes.
//! 2. **Multiple regions can be broadcast to at once.** Sending the same
//!    transaction to every configured region costs nothing extra and removes
//!    the regional-latency coin flip.
//!
//! What this deliberately does **not** do: bundles. Helius Sender transmits
//! individual transactions; it has no atomic-bundle mode, so a cycle that
//! depends on a separate guard transaction landing after it keeps using Jito.

use crate::{Result, RpcClient, RpcError};
use base64::Engine as _;
use serde::Deserialize;
use solana_sdk::transaction::VersionedTransaction;
use std::sync::Arc;
use std::time::Duration;

/// Default Helius Sender region. Operators add more with `helius_sender_urls`.
pub const DEFAULT_SENDER_URL: &str = "https://sender.helius-rpc.com/fast";

/// Helius Sender accepts the same commitment set as a normal RPC node.
#[derive(Debug, Deserialize)]
struct SendResponse {
    result: Option<String>,
    error: Option<RpcErrorBody>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    code: i64,
    message: String,
}

/// A client for one or more Helius Sender regions.
#[derive(Debug, Clone)]
pub struct HeliusSenderClient {
    urls: Vec<String>,
    api_key: Option<String>,
    timeout: Duration,
}

impl HeliusSenderClient {
    /// Build a client with one or more endpoints.
    ///
    /// `api_key` is optional because the endpoint can also carry the key as a
    /// `?api-key=` query parameter; the header form always wins when both are
    /// set, since it keeps the key out of logs and error messages.
    pub fn new(urls: Vec<String>, api_key: Option<String>, timeout_ms: u64) -> Result<Self> {
        let mut urls: Vec<String> = urls
            .into_iter()
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .collect();
        if urls.is_empty() {
            urls.push(DEFAULT_SENDER_URL.to_string());
        }
        let mut unique = Vec::with_capacity(urls.len());
        for url in urls {
            if !unique.contains(&url) {
                unique.push(url);
            }
        }
        Ok(Self {
            urls: unique,
            api_key,
            timeout: Duration::from_millis(timeout_ms.max(1_000)),
        })
    }

    /// Every configured endpoint, in order.
    pub fn urls(&self) -> &[String] {
        &self.urls
    }

    /// Submit one already-serialized, signed transaction.
    ///
    /// The transaction is sent to **every** configured region concurrently and
    /// the first accepted signature wins. That is safe: a duplicate submission
    /// of an already-signed transaction is either accepted by one validator or
    /// ignored as a duplicate, so the only failure mode is a fee paid once.
    pub async fn send_transaction(
        &self,
        encoded_transaction: &str,
        skip_preflight: bool,
        max_retries: u32,
    ) -> Result<String> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                encoded_transaction,
                {
                    "encoding": "base64",
                    "skipPreflight": skip_preflight,
                    "preflightCommitment": "confirmed",
                    "maxRetries": max_retries,
                }
            ],
        });
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| RpcError::Http(error.to_string()))?;
        // Fan out to every region; the first success returns.
        let mut tasks = Vec::with_capacity(self.urls.len());
        for url in &self.urls {
            let url = url.clone();
            let body = request.clone();
            let client = client.clone();
            let key = self.api_key.clone();
            tasks.push(tokio::spawn(async move {
                send_one(&client, &url, body, key.as_deref()).await
            }));
        }
        let mut last_error: Option<RpcError> = None;
        for task in tasks {
            match task.await {
                Ok(Ok(signature)) if !signature.is_empty() => return Ok(signature),
                Ok(Ok(_)) => {
                    last_error = Some(RpcError::Transaction(
                        "sender returned an empty signature".into(),
                    ))
                }
                Ok(Err(error)) => last_error = Some(error),
                Err(error) => {
                    last_error = Some(RpcError::Transaction(format!(
                        "sender task failed: {error}"
                    )))
                }
            }
        }
        Err(last_error
            .unwrap_or_else(|| RpcError::Transaction("no sender region succeeded".into())))
    }

    /// Submit a signed, already-built transaction.
    pub async fn send_versioned_transaction(
        &self,
        transaction: &VersionedTransaction,
    ) -> Result<String> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            bincode::serialize(transaction)
                .map_err(|error| RpcError::Transaction(error.to_string()))?,
        );
        self.send_transaction(&encoded, false, 0).await
    }

    /// Whether a cycle transaction can be sent without a bundle.
    ///
    /// Helius Sender transmits single transactions. A plan whose safety depends
    /// on a *separate* guard transaction landing afterwards cannot use it,
    /// because the guard would be an independent, possibly reordered
    /// transaction. A self-contained cycle — one whose profitability is enforced
    /// by its own final minimum and whose revert is atomic — can.
    pub fn supports(&self, encoded_transactions: &[String]) -> bool {
        let transaction_count = encoded_transactions.len();
        // One transaction, or several identical copies of the same one.
        !encoded_transactions.is_empty()
            && (transaction_count == 1
                || encoded_transactions
                    .iter()
                    .all(|tx| tx == &encoded_transactions[0]))
    }
}

/// Send one copy to one region.
async fn send_one(
    client: &reqwest::Client,
    url: &str,
    body: serde_json::Value,
    api_key: Option<&str>,
) -> Result<String> {
    let mut request = client.post(url).json(&body);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|error| RpcError::Http(format!("{url}: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        let text = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".into());
        return Err(RpcError::Http(format!("{url}: HTTP {status}: {text}")));
    }
    let parsed: SendResponse = response
        .json()
        .await
        .map_err(|error| RpcError::Decode(format!("{url}: {error}")))?;
    if let Some(error) = parsed.error {
        return Err(RpcError::Transaction(format!(
            "{url}: {}: {}",
            error.code, error.message
        )));
    }
    parsed
        .result
        .ok_or_else(|| RpcError::Decode(format!("{url}: sendTransaction returned no result")))
}

/// Sender plus the RPC client it can fall back to.
#[derive(Debug, Clone)]
pub struct SenderRouter {
    sender: Arc<HeliusSenderClient>,
    rpc: Arc<RpcClient>,
}

impl SenderRouter {
    pub fn new(sender: Arc<HeliusSenderClient>, rpc: Arc<RpcClient>) -> Self {
        Self { sender, rpc }
    }

    /// Send through Helius Sender, falling back to the default RPC path.
    ///
    /// The fallback exists because a sender outage must not stop the bot: a
    /// transaction that loses the low-latency race still lands, just slower.
    pub async fn send(&self, encoded_transaction: &str, skip_preflight: bool) -> Result<String> {
        match self
            .sender
            .send_transaction(encoded_transaction, skip_preflight, 3)
            .await
        {
            Ok(signature) => Ok(signature),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "Helius Sender failed; falling back to the default RPC send path"
                );
                self.rpc
                    .send_transaction(encoded_transaction, skip_preflight)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_one_region() {
        let client = HeliusSenderClient::new(Vec::new(), None, 30_000).unwrap();
        assert_eq!(client.urls(), [DEFAULT_SENDER_URL]);
    }

    #[test]
    fn trims_trailing_slashes_and_dedupes() {
        let client = HeliusSenderClient::new(
            vec![
                "https://a.example/fast/".into(),
                "https://a.example/fast".into(),
                "https://b.example/fast".into(),
                "   ".into(),
            ],
            None,
            1_000,
        )
        .unwrap();
        assert_eq!(
            client.urls(),
            ["https://a.example/fast", "https://b.example/fast"]
        );
    }

    #[test]
    fn supports_single_transactions_only() {
        let client = HeliusSenderClient::new(Vec::new(), None, 30_000).unwrap();
        assert!(client.supports(&["AAA".into()]));
        // Several copies of one transaction broadcast is fine.
        assert!(client.supports(&["AAA".into(), "AAA".into()]));
        // A cycle plus a distinct guard transaction is not a sender job.
        assert!(!client.supports(&["AAA".into(), "BBB".into()]));
        assert!(!client.supports(&[]));
    }

    #[test]
    fn a_bad_timeout_is_clamped() {
        let client = HeliusSenderClient::new(Vec::new(), None, 10).unwrap();
        assert!(client.timeout >= Duration::from_millis(1_000));
    }
}
