// Native WebSocket-based price subscriber

use crate::error::CoreError;
use crate::price_subscriber::PriceSubscriber;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::time::Instant;

pub struct NativeWebSocketSubscriber {
    prices: Arc<RwLock<HashMap<String, (Instant, f64)>>>,
    subscribed_mints: Arc<RwLock<Vec<String>>>,
    is_running: Arc<RwLock<bool>>,
    price_cache_ttl_secs: u64,
}

impl NativeWebSocketSubscriber {
    pub fn new(price_cache_ttl_secs: u64) -> Self {
        Self {
            prices: Arc::new(RwLock::new(HashMap::new())),
            subscribed_mints: Arc::new(RwLock::new(Vec::new())),
            is_running: Arc::new(RwLock::new(true)),
            price_cache_ttl_secs,
        }
    }

    /// Update price for a mint (called by WebSocket handler)
    pub async fn update_price(&self, mint: &str, price: f64) {
        let mut prices = self.prices.write().await;
        prices.insert(mint.to_string(), (Instant::now(), price));
    }
}

#[async_trait::async_trait(?Send)]
impl PriceSubscriber for NativeWebSocketSubscriber {
    async fn subscribe(&mut self, mint: &str) -> Result<(), CoreError> {
        let mut mints = self.subscribed_mints.write().await;
        if !mints.contains(&mint.to_string()) {
            mints.push(mint.to_string());
        }
        Ok(())
    }

    async fn unsubscribe(&mut self, mint: &str) -> Result<(), CoreError> {
        let mut mints = self.subscribed_mints.write().await;
        mints.retain(|m| m != mint);
        
        let mut prices = self.prices.write().await;
        prices.remove(mint);
        
        Ok(())
    }

    async fn get_price(&self, mint: &str) -> Option<f64> {
        let prices = self.prices.read().await;
        if let Some((timestamp, price)) = prices.get(mint) {
            // Check if price is still valid
            if Instant::now().duration_since(*timestamp).as_secs() < self.price_cache_ttl_secs {
                return Some(*price);
            }
        }
        None
    }

    async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    fn subscribed_mints(&self) -> Vec<String> {
        // Note: This is not async in the trait, so we can't use .await here
        // In a real implementation, we'd need to refactor this or use blocking API
        Vec::new()
    }
}

