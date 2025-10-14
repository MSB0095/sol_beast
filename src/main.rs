// Rust
mod settings;
pub use settings::Settings;
mod ws;
mod rpc;

use log::{error, info};
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    env_logger::init();
    let settings = Settings::from_file("config.toml");
    let mut seen_signatures = HashSet::new();

    loop {
        match ws::run_ws(&settings, &mut seen_signatures).await {
            Ok(_) => info!("WebSocket connection closed normally"),
            Err(e) => error!("WebSocket error: {}. Reconnecting in 5000ms...", e),
        }
        sleep(Duration::from_millis(5000)).await;
    }
}
