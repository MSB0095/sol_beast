mod models;
mod rpc;
mod settings;

use crate::{
    models::{PriceCache, Holding, TradeEvent},
    settings::Settings,
};
use chrono::Utc;
use log::{error, info, warn, trace};
use lru::LruCache;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal_macros::dec;
use solana_sdk::signature::Keypair;
use std::{fs, sync::Arc};
use tokio::sync::{broadcast, Mutex};
use serde_json::{json, Value};
use borsh::BorshDeserialize;
use base64::Engine;
use futures_util::{StreamExt, SinkExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let settings = Arc::new(Settings::from_file("config.toml"));

    let (trade_event_tx, _) = broadcast::channel::<TradeEvent>(1000);

    let wss_urls = settings.solana_ws_urls.clone();
    let max_conns = settings.max_wss_connections.unwrap_or(1);
    let rotate = settings.rotate_wss_connections.unwrap_or(false);

    let urls_to_connect: Vec<String> = if rotate {
        wss_urls.into_iter().cycle().take(max_conns).collect()
    } else {
        vec![wss_urls[0].clone()]
    };

    let mut streams = Vec::new();
    for url in urls_to_connect {
        match connect_async(&url).await {
            Ok((stream, _)) => {
                info!("Connected to {}", url);
                streams.push(stream);
            }
            Err(e) => {
                error!("Failed to connect to {}: {}", url, e);
            }
        }
    }

    if streams.is_empty() {
        error!("No WebSocket connections established. Exiting.");
        return;
    }

    let (mut writers, readers): (Vec<_>, Vec<_>) = streams.into_iter().map(|s| s.split()).unzip();
    
    for writer in &mut writers {
        let subscribe_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "logsSubscribe",
            "params": [
                { "mentions": [&settings.pump_fun_program] },
                { "commitment": "confirmed" }
            ]
        });
        if let Err(e) = writer.send(Message::Text(subscribe_request.to_string())).await {
            error!("Failed to subscribe: {}", e);
        }
    }

    info!("Listening for new tokens and trades...");

    let mut read_stream = futures_util::stream::select_all(readers);

    let seen_signatures = Arc::new(Mutex::new(LruCache::new(settings.cache_capacity.try_into().unwrap())));
    let price_cache = Arc::new(Mutex::new(LruCache::new(settings.cache_capacity.try_into().unwrap())));
    let holdings = Arc::new(Mutex::new(Vec::<Holding>::new()));
    let is_real = std::env::args().any(|arg| arg == "--real");
    let keypair = if is_real {
        let home_dir = std::env::var("HOME").unwrap();
        let keypair_path = settings.wallet_keypair_path.replace("~", &home_dir);
        let bytes = fs::read(keypair_path).expect("Keypair file not found");
        Some(Arc::new(Keypair::try_from(bytes.as_slice()).expect("Invalid keypair")))
    } else {
        None
    };

    while let Some(msg) = read_stream.next().await {
        let seen_clone = seen_signatures.clone();
        let price_cache_clone = price_cache.clone();
        let settings_clone = settings.clone();
        let keypair_clone = keypair.clone();
        let trade_event_tx_clone = trade_event_tx.clone();
        let holdings_clone = holdings.clone();

        tokio::spawn(async move {
            if let Ok(Message::Text(text)) = msg {
                if let Err(e) = process_message(
                    text,
                    seen_clone,
                    is_real,
                    keypair_clone,
                    price_cache_clone,
                    settings_clone,
                    trade_event_tx_clone,
                    holdings_clone,
                ).await {
                    warn!("Failed to process message: {}", e);
                }
            }
        });
    }
}

async fn process_message(
    text: String,
    seen: Arc<Mutex<LruCache<String, ()>>>,
    is_real: bool,
    keypair: Option<Arc<Keypair>>,
    price_cache: Arc<Mutex<PriceCache>>,
    settings: Arc<Settings>,
    trade_event_tx: broadcast::Sender<TradeEvent>,
    holdings: Arc<Mutex<Vec<Holding>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let msg: Value = serde_json::from_str(&text)?;

    if let Some(result) = msg.get("params").and_then(|p| p.get("result")) {
        if let Some(value_obj) = result.get("value") {
            if let Some(logs) = value_obj.get("logs").and_then(|l| l.as_array()) {
                for log_item in logs {
                    if let Some(log) = log_item.as_str() {
                        if log.starts_with("Program data: ") {
                            let data_str = log.replace("Program data: ", "");
                            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(data_str) {
                                if decoded.len() > 8 && decoded[0..8] == [189, 219, 127, 211, 78, 230, 97, 238] {
                                    if let Ok(trade_event) = TradeEvent::deserialize(&mut &decoded[8..]) {
                                        trace!("Trade event for mint {}: {} tokens for {} SOL", trade_event.mint, trade_event.token_amount, trade_event.sol_amount as f64 / 1e9);
                                        let _ = trade_event_tx.send(trade_event);
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(signature_str) = value_obj.get("signature").and_then(|s| s.as_str()) {
                    if logs.iter().any(|log_item| log_item.as_str() == Some("Program log: Instruction: InitializeMint2")) {
                        if seen.lock().await.put(signature_str.to_string(), ()).is_none() {
                            info!("New pump.fun token: {}", signature_str);
                            handle_new_token(signature_str, is_real, keypair, price_cache, settings, trade_event_tx.subscribe(), holdings).await?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_new_token(
    signature: &str,
    is_real: bool,
    keypair: Option<Arc<Keypair>>,
    price_cache: Arc<Mutex<PriceCache>>,
    settings: Arc<Settings>,
    trade_event_rx: broadcast::Receiver<TradeEvent>,
    holdings: Arc<Mutex<Vec<Holding>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (creator, mint) = rpc::fetch_transaction_details(signature, &settings).await?;
    if let Some(metadata) = rpc::fetch_token_metadata(&mint, &settings).await? {
        info!("New token found: {} ({}) | Mint: {} | Creator: {}", metadata.name.trim_end_matches('\0'), metadata.symbol.trim_end_matches('\0'), mint, creator);
        let should_buy = !metadata.uri.trim_end_matches('\0').is_empty() && metadata.seller_fee_basis_points < 500;
        
        let mut holdings_guard = holdings.lock().await;
        if should_buy && holdings_guard.len() < settings.max_held_tokens.unwrap_or(10) as usize {
            let buy_amount = settings.buy_amount.unwrap_or(0.0005);
            match rpc::buy_token(&mint, buy_amount, is_real, keypair.as_deref(), price_cache.clone(), &settings).await {
                Ok(holding) => {
                    info!("Successfully bought {}. Monitoring...", mint);
                    holdings_guard.push(holding.clone());
                    price_cache.lock().await.pop(&mint.to_string());
                    
                    let holdings_clone = holdings.clone();
                    let mint_clone = mint.clone();
                    tokio::spawn(async move {
                        monitor_token(holding, is_real, keypair, settings, trade_event_rx).await;
                        let mut holdings_guard = holdings_clone.lock().await;
                        holdings_guard.retain(|h| h.mint != mint_clone.to_string());
                        info!("Removed {} from holdings. Current holdings: {}", mint_clone, holdings_guard.len());
                    });
                }
                Err(e) => warn!("Failed to buy {}: {}", mint, e),
            }
        }
    }
    Ok(())
}

async fn monitor_token(
    holding: Holding,
    is_real: bool,
    keypair: Option<Arc<Keypair>>,
    settings: Arc<Settings>,
    mut trade_event_rx: broadcast::Receiver<TradeEvent>,
) {
    info!("Monitoring token: {}", holding.mint);
    while let Ok(trade_event) = trade_event_rx.recv().await {
        if trade_event.mint.to_string() == holding.mint {
            let token_reserves = Decimal::from(trade_event.virtual_token_reserves);
            let sol_reserves = Decimal::from(trade_event.virtual_sol_reserves) / dec!(1_000_000_000);
            let current_price = if !token_reserves.is_zero() {
                sol_reserves / token_reserves
            } else {
                continue;
            };

            let profit_percent = if !holding.buy_price.is_zero() {
                ((current_price - holding.buy_price) / holding.buy_price) * dec!(100)
            } else {
                Decimal::ZERO
            };
            let elapsed = Utc::now().signed_duration_since(holding.buy_time).num_seconds();
            info!("Price for {}: {} SOL/token | Profit: {}%", holding.mint, current_price.round_dp(18), profit_percent.round_dp(8));

            let tp_percent_decimal = Decimal::from_f64(settings.tp_percent).unwrap_or_default();
            let sl_percent_decimal = Decimal::from_f64(settings.sl_percent).unwrap_or_default();

            let should_sell = if profit_percent >= tp_percent_decimal {
                info!("TP hit for {}: +{:.2} ({} SOL/token)", holding.mint, profit_percent, current_price);
                true
            } else if profit_percent <= sl_percent_decimal {
                info!("SL hit for {}: {:.2} ({} SOL/token)", holding.mint, profit_percent, current_price);
                true
            } else if elapsed >= settings.timeout_secs {
                info!("Timeout for {}: {}s", holding.mint, elapsed);
                true
            } else {
                false
            };

            if should_sell {
                if let Err(e) = rpc::sell_token(&holding.mint, holding.amount, current_price, is_real, keypair.as_deref(), &settings).await {
                    error!("Sell error for {}: {}", holding.mint, e);
                }
                break;
            }
        }
    }
    info!("Stopped monitoring {}.", holding.mint);
}
