use crate::models::PriceCache;
use crate::shyft_monitor::ShyftControlMessage;
use sol_beast_core::error::CoreError;
use sol_beast_core::price_subscriber::PriceSubscriber;
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{mpsc, Mutex};
use std::time::Instant;

/// CLI-specific PriceSubscriber that bridges the Shyft control channel and shared price cache.
/// It implements the Core PriceSubscriber trait so monitor logic can stay thin.
pub struct CliPriceSubscriber {
    price_cache: Arc<Mutex<PriceCache>>,
    shyft_control_tx: mpsc::Sender<ShyftControlMessage>,
    subscribed: StdMutex<HashSet<String>>,
    price_cache_ttl_secs: u64,
}

impl CliPriceSubscriber {
    pub fn new(
        price_cache: Arc<Mutex<PriceCache>>,
        shyft_control_tx: mpsc::Sender<ShyftControlMessage>,
        price_cache_ttl_secs: u64,
    ) -> Self {
        Self {
            price_cache,
            shyft_control_tx,
            subscribed: StdMutex::new(HashSet::new()),
            price_cache_ttl_secs,
        }
    }

    /// Expose the underlying cache for RPC fallbacks.
    pub fn price_cache(&self) -> Arc<Mutex<PriceCache>> {
        self.price_cache.clone()
    }

    /// Prime the cache with a fresh price (used when falling back to RPC).
    pub async fn prime_price(&self, mint: &str, price: f64) {
        let mut cache = self.price_cache.lock().await;
        cache.put(mint.to_string(), (Instant::now(), price));
    }

    /// Subscribe to price updates (idempotent) using interior mutability.
    pub async fn subscribe_mint(&self, mint: &str) -> Result<(), CoreError> {
        let should_send = {
            let mut set = self.subscribed.lock().unwrap();
            if set.contains(mint) {
                false
            } else {
                set.insert(mint.to_string());
                true
            }
        };
        if should_send {
            let _ = self
                .shyft_control_tx
                .send(ShyftControlMessage::SubscribePrice(mint.to_string()))
                .await;
        }
        Ok(())
    }

    /// Unsubscribe and clear cached price.
    pub async fn unsubscribe_mint(&self, mint: &str) -> Result<(), CoreError> {
        let should_send = {
            let mut set = self.subscribed.lock().unwrap();
            set.remove(mint)
        };
        if should_send {
            let _ = self
                .shyft_control_tx
                .send(ShyftControlMessage::UnsubscribePrice(mint.to_string()))
                .await;
        }
        let mut cache = self.price_cache.lock().await;
        cache.pop(mint);
        Ok(())
    }

    /// Get cached price respecting TTL.
    pub async fn cached_price(&self, mint: &str) -> Option<f64> {
        let cache = self.price_cache.lock().await;
        if let Some((ts, price)) = cache.peek(mint) {
            if Instant::now().duration_since(*ts).as_secs() < self.price_cache_ttl_secs {
                return Some(*price);
            }
        }
        None
    }
}

#[async_trait(?Send)]
impl PriceSubscriber for CliPriceSubscriber {
    async fn subscribe(&mut self, mint: &str) -> Result<(), CoreError> {
        let should_send = {
            let mut set = self.subscribed.lock().unwrap();
            if set.contains(mint) {
                false
            } else {
                set.insert(mint.to_string());
                true
            }
        };
        if should_send {
            let _ = self
                .shyft_control_tx
                .send(ShyftControlMessage::SubscribePrice(mint.to_string()))
                .await;
        }
        Ok(())
    }

    async fn unsubscribe(&mut self, mint: &str) -> Result<(), CoreError> {
        let should_send = {
            let mut set = self.subscribed.lock().unwrap();
            if set.remove(mint) { true } else { false }
        };
        if should_send {
            let _ = self
                .shyft_control_tx
                .send(ShyftControlMessage::UnsubscribePrice(mint.to_string()))
                .await;
        }
        // Also drop cached price
        let mut cache = self.price_cache.lock().await;
        cache.pop(mint);
        Ok(())
    }

    async fn get_price(&self, mint: &str) -> Option<f64> {
        self.cached_price(mint).await
    }

    async fn is_running(&self) -> bool {
        true
    }

    fn subscribed_mints(&self) -> Vec<String> {
        self.subscribed
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }
}
