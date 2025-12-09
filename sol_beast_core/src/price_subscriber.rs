// Price subscription and monitoring abstraction
// Allows both native (WebSocket) and WASM (browser updates) price sources

use crate::error::CoreError;
use async_trait::async_trait;
use std::collections::HashMap;

pub type PriceSubscriberResult<T> = Result<T, CoreError>;

/// Platform-agnostic price subscription trait
/// Implementations exist for:
/// - Native: WebSocket-based subscriptions (Helius, Shyft)
/// - WASM: Browser-based price updates from browser state
#[async_trait(?Send)]
pub trait PriceSubscriber {
    /// Subscribe to price updates for a mint
    async fn subscribe(&mut self, mint: &str) -> PriceSubscriberResult<()>;

    /// Unsubscribe from price updates for a mint
    async fn unsubscribe(&mut self, mint: &str) -> PriceSubscriberResult<()>;

    /// Get the current cached price for a mint (may be stale)
    async fn get_price(&self, mint: &str) -> Option<f64>;

    /// Get multiple prices at once
    async fn get_prices(&self, mints: &[&str]) -> HashMap<String, f64> {
        let mut prices = HashMap::new();
        for mint in mints {
            if let Some(price) = self.get_price(mint).await {
                prices.insert(mint.to_string(), price);
            }
        }
        prices
    }

    /// Check if this subscriber is actively running
    async fn is_running(&self) -> bool;

    /// Get list of currently subscribed mints
    fn subscribed_mints(&self) -> Vec<String>;
}

