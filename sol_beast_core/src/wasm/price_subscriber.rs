// WASM price subscriber using browser state/events

use crate::error::CoreError;
use crate::price_subscriber::PriceSubscriber;
use std::collections::HashMap;

/// WASM price subscriber - uses browser state for prices
pub struct WasmPriceSubscriber {
    prices: HashMap<String, f64>,
    subscribed_mints: Vec<String>,
}

impl WasmPriceSubscriber {
    pub fn new() -> Self {
        Self {
            prices: HashMap::new(),
            subscribed_mints: Vec::new(),
        }
    }

    /// Update price from browser state (called by JavaScript)
    pub fn update_price(&mut self, mint: &str, price: f64) {
        self.prices.insert(mint.to_string(), price);
    }

    /// Batch update prices from browser state
    pub fn update_prices(&mut self, prices: HashMap<String, f64>) {
        self.prices.extend(prices);
    }
}

impl Default for WasmPriceSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl PriceSubscriber for WasmPriceSubscriber {
    async fn subscribe(&mut self, mint: &str) -> Result<(), CoreError> {
        if !self.subscribed_mints.contains(&mint.to_string()) {
            self.subscribed_mints.push(mint.to_string());
        }
        Ok(())
    }

    async fn unsubscribe(&mut self, mint: &str) -> Result<(), CoreError> {
        self.subscribed_mints.retain(|m| m != mint);
        self.prices.remove(mint);
        Ok(())
    }

    async fn get_price(&self, mint: &str) -> Option<f64> {
        self.prices.get(mint).copied()
    }

    async fn is_running(&self) -> bool {
        true // Browser always running while app is open
    }

    fn subscribed_mints(&self) -> Vec<String> {
        self.subscribed_mints.clone()
    }
}

