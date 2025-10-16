// Rust
use log::error;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;

mod settings;
pub use settings::Settings;
mod ws;
mod rpc;

const CACHE_CAPACITY: usize = 10_000;

#[tokio::main(worker_threads = 4)]
async fn main() {
    env_logger::init();
    let settings = Settings::from_file("config.toml");
// Fetch all token metadata at startup
if let Err(e) = rpc::fetch_all_token_metadata().await {
    error!("Failed to fetch token metadata: {}", e);
}
    let seen_signatures = Arc::new(Mutex::new(LruCache::new(
        NonZeroUsize::new(CACHE_CAPACITY).unwrap(),
    )));
    let (tx, mut rx) = mpsc::channel::<String>(1000);

    // Use solana_ws_urls/rpc_urls from config
    let ws_urls = settings.solana_ws_urls.clone();
    for wss_url in ws_urls {
        let tx = tx.clone();
        let seen = seen_signatures.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = ws::run_ws(&wss_url, tx.clone(), seen.clone()).await {
                    error!("WSS {} error: {}. Reconnecting...", wss_url, e);
                }
                sleep(Duration::from_millis(5000)).await;
            }
        });
    }

    // Process earliest messages
    while let Some(msg) = rx.recv().await {
        let seen = seen_signatures.clone();
        tokio::spawn(async move {
            if let Err(e) = ws::process_message(&msg, &seen).await {
                error!("Message process error: {}", e);
            }
        });
    }
}
