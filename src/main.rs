use chrono::{DateTime, Utc};
use log::{error, info};
use lru::LruCache;
use solana_sdk::signature::Keypair;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

mod settings;
mod ws;
mod rpc;

pub const CACHE_CAPACITY: usize = 10000;

#[derive(Clone)]
pub struct Holding {
    pub mint: String,
    pub amount: u64,
    pub buy_price: f64,
    pub buy_time: DateTime<Utc>,
}
pub type PriceCache = LruCache<String, (Instant, f64)>;


#[tokio::main(worker_threads = 4)]
async fn main() {
    env_logger::init();
    let settings = Arc::new(settings::Settings::from_file("config.toml"));

    let seen = Arc::new(Mutex::new(LruCache::new(CACHE_CAPACITY.try_into().unwrap())));
    let holdings = Arc::new(Mutex::new(HashMap::new()));
    let price_cache = Arc::new(Mutex::new(LruCache::new(CACHE_CAPACITY.try_into().unwrap())));
    let is_real = std::env::args().any(|arg| arg == "--real");
    let keypair = if is_real {
        let bytes = fs::read(&settings.wallet_keypair_path).expect("Keypair file missing");
        Some(Keypair::try_from(bytes.as_slice()).expect("Invalid keypair"))
    } else {
        None
    };

    // Spawn price monitoring
    let holdings_clone = holdings.clone();
    let price_cache_clone = price_cache.clone();
    let keypair_clone = keypair.as_ref().map(|kp| Keypair::try_from(kp.to_bytes().as_slice()).unwrap());
    let settings_clone = settings.clone();
    tokio::spawn(async move { monitor_holdings(holdings_clone, price_cache_clone, is_real, keypair_clone.as_ref(), settings_clone).await });

    // Spawn WSS tasks
    let holdings_clone = holdings.clone();
    let price_cache_clone = price_cache.clone();
    let keypair_clone = keypair.as_ref().map(|kp| Keypair::try_from(kp.to_bytes().as_slice()).unwrap());
    let seen_clone = seen.clone();
    let settings_clone = settings.clone();
    tokio::spawn(async move {
        ws::monitor_pump_fun_tokens(
            settings_clone.solana_ws_urls.clone(),
            seen_clone,
            holdings_clone,
            is_real,
            keypair_clone,
            price_cache_clone,
            settings_clone,
        ).await;
    }).await.expect("monitor_pump_fun_tokens task failed");
}


async fn monitor_holdings(
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    is_real: bool,
    keypair: Option<&Keypair>,
    settings: Arc<settings::Settings>,
) {
    info!("Starting holdings monitor...");
    loop {
        sleep(Duration::from_secs(10)).await;
        let mut holdings_guard = holdings.lock().await;
        let mut sold_mints = Vec::new();

        for (mint, holding) in holdings_guard.iter() {
            match rpc::fetch_current_price(mint, &price_cache, &settings).await {
                Ok(price) => {
                    let pnl = (price - holding.buy_price) / holding.buy_price * 100.0;
                    info!("Holding: {} | PnL: {:.2}% | Buy Price: {} | Current Price: {}", mint, pnl, holding.buy_price, price);

                    if pnl >= settings.tp_percent || pnl <= settings.sl_percent || Utc::now().signed_duration_since(holding.buy_time).num_seconds() > settings.timeout_secs {
                        info!("Triggering sell for {}: PnL {:.2}%", mint, pnl);
                        let keypair_clone = keypair.map(|k| Keypair::try_from(k.to_bytes().as_slice()).unwrap());
                        if let Err(e) = rpc::sell_token(mint, holding.amount, is_real, keypair_clone.as_ref()).await {
                            error!("Failed to sell {}: {}", mint, e);
                        } else {
                            sold_mints.push(mint.clone());
                        }
                    }
                }
                Err(e) => {
                    error!("Could not fetch price for {}: {}. Maybe migrated.", mint, e);
                    if e.to_string().contains("Token migrated") {
                        sold_mints.push(mint.clone());
                    }
                }
            }
        }
        for mint in sold_mints {
            holdings_guard.remove(&mint);
        }
    }
}