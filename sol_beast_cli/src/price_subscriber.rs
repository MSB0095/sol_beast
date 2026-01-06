use crate::models::PriceCache;
use crate::ws::WsRequest;
use sol_beast_core::error::CoreError;
use sol_beast_core::price_subscriber::PriceSubscriber;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{mpsc, Mutex, oneshot};
use std::time::Instant;
use solana_program::pubkey::Pubkey;
use std::str::FromStr;

/// CLI-specific PriceSubscriber that bridges the WSS control channel and shared price cache.
/// It implements the Core PriceSubscriber trait so monitor logic can stay thin.
pub struct CliPriceSubscriber {
    price_cache: Arc<Mutex<PriceCache>>,
    ws_control_tx: mpsc::Sender<WsRequest>,
    // Map mint -> subscription ID (returned by RPC)
    subscribed: StdMutex<HashMap<String, u64>>,
    price_cache_ttl_secs: u64,
    pump_fun_program: String,
}

impl CliPriceSubscriber {
    pub fn new(
        price_cache: Arc<Mutex<PriceCache>>,
        ws_control_tx: mpsc::Sender<WsRequest>,
        price_cache_ttl_secs: u64,
        pump_fun_program: String,
    ) -> Self {
        Self {
            price_cache,
            ws_control_tx,
            subscribed: StdMutex::new(HashMap::new()),
            price_cache_ttl_secs,
            pump_fun_program,
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
        let should_subscribe = {
            let map = self.subscribed.lock().unwrap();
            !map.contains_key(mint)
        };

        if should_subscribe {
            // Derive Bonding Curve PDA
            let program_id = Pubkey::from_str(&self.pump_fun_program)
                .map_err(|e| CoreError::ParseError(format!("Invalid pump program: {}", e)))?;
            let mint_pk = Pubkey::from_str(mint)
                .map_err(|e| CoreError::ParseError(format!("Invalid mint: {}", e)))?;
            let (pda, _) = Pubkey::find_program_address(
                &[b"bonding-curve", mint_pk.as_ref()],
                &program_id,
            );
            
            let (tx, rx) = oneshot::channel();
            self.ws_control_tx
                .send(WsRequest::Subscribe {
                    account: pda.to_string(),
                    mint: mint.to_string(),
                    resp: tx,
                })
                .await
                .map_err(|e| CoreError::Rpc(format!("Failed to send WSS subscribe request: {}", e)))?;
            
            match rx.await {
                Ok(Ok(sub_id)) => {
                    let mut map = self.subscribed.lock().unwrap();
                    map.insert(mint.to_string(), sub_id);
                }
                Ok(Err(e)) => return Err(CoreError::Rpc(format!("WSS subscription failed: {}", e))),
                Err(e) => return Err(CoreError::Rpc(format!("WSS response channel closed: {}", e))),
            }
        }
        Ok(())
    }

    /// Unsubscribe and clear cached price.
    pub async fn unsubscribe_mint(&self, mint: &str) -> Result<(), CoreError> {
        let sub_id = {
            let mut map = self.subscribed.lock().unwrap();
            map.remove(mint)
        };
        
        if let Some(id) = sub_id {
            let (tx, rx) = oneshot::channel();
            let _ = self.ws_control_tx
                .send(WsRequest::Unsubscribe {
                    sub_id: id,
                    resp: tx,
                })
                .await;
            // Best effort await
            let _ = rx.await;
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
        self.subscribe_mint(mint).await
    }

    async fn unsubscribe(&mut self, mint: &str) -> Result<(), CoreError> {
        self.unsubscribe_mint(mint).await?;
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
            .keys()
            .cloned()
            .collect()
    }
}
