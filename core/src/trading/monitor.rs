use crate::models::{Holding, PriceCache};
use crate::Settings;
use crate::{api::{TradeRecord, BotControl}, state::BuyRecord};
use crate::rpc_client::RpcClient as CoreRpcClient;
use std::{collections::HashMap, sync::Arc};
use std::sync::atomic::AtomicUsize;
use tokio::sync::{Mutex, mpsc};
use crate::ws::WsRequest;
use log::{info, debug, error};

pub async fn monitor_holdings(
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: Arc<dyn CoreRpcClient>,
    is_real: bool,
    keypair: Option<Arc<dyn crate::Signer>>, // Core signer
    simulate_keypair: Option<Arc<dyn crate::Signer>>,
    settings: Arc<Settings>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
    _next_wss_sender: Arc<AtomicUsize>,
    trades_list: Arc<tokio::sync::Mutex<Vec<TradeRecord>>>,
    bot_control: Arc<BotControl>,
) {
    info!("Starting holdings monitor - real trading: {}, active holdings: {}",
          is_real, holdings.lock().await.len());
    
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let max_retries = 3;
    let retry_delay = tokio::time::Duration::from_secs(2);
    
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Check if monitoring should stop by checking running state
                let running_state = bot_control.running_state.lock().await;
                if *running_state != crate::api::BotRunningState::Running {
                    info!("Bot not running, stopping holdings monitor");
                    drop(running_state);
                    break;
                }
                drop(running_state);
                
                // Get current holdings snapshot
                let holdings_snapshot = {
                    let holdings_guard = holdings.lock().await;
                    holdings_guard.clone()
                };
                
                debug!("Monitoring {} holdings", holdings_snapshot.len());
                
                // Monitor each holding
                for (mint, holding) in holdings_snapshot {
                    if let Err(e) = monitor_single_holding(
                        &mint,
                        &holding,
                        &price_cache,
                        &rpc_client,
                        &settings,
                        &trades_map,
                        &ws_control_senders,
                        &sub_map,
                        max_retries,
                        retry_delay,
                        is_real,
                        &keypair,
                        &simulate_keypair,
                        &trades_list,
                    ).await {
                        error!("Failed to monitor holding {}: {}", mint, e);
                    }
                }
            }
        }
    }
    
    info!("Holdings monitor stopped");
}

async fn monitor_single_holding(
    mint: &str,
    holding: &Holding,
    price_cache: &Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
    _trades_map: &Arc<Mutex<HashMap<String, BuyRecord>>>,
    _ws_control_senders: &Arc<Vec<mpsc::Sender<WsRequest>>>,
    _sub_map: &Arc<Mutex<HashMap<String, (usize, u64)>>>,
    _max_retries: usize,
    _retry_delay: tokio::time::Duration,
    _is_real: bool,
    _keypair: &Option<Arc<dyn crate::Signer>>,
    _simulate_keypair: &Option<Arc<dyn crate::Signer>>,
    trades_list: &Arc<tokio::sync::Mutex<Vec<TradeRecord>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _signer = if _is_real { _keypair } else { _simulate_keypair };
    
    // Update price cache for this mint
    if let Ok(price) = crate::rpc_helpers::fetch_current_price(
        mint,
        price_cache,
        rpc_client,
        settings,
    ).await {
        debug!("Current price for {}: {}", mint, price);
        
        // Calculate current value
        let amount_tokens = holding.amount as f64 / 1_000_000.0; // Convert to tokens (assuming 6 decimals)
        let current_value = amount_tokens * price;
        let cost_basis = holding.buy_price * amount_tokens; // Calculate cost basis from buy price
        let pnl = current_value - cost_basis;
        let pnl_percent = if cost_basis > 0.0 {
            (pnl / cost_basis) * 100.0
        } else {
            0.0
        };
        
        debug!("Holdings update for {}: amount={}, price={}, value={}, pnl={}",
               mint, amount_tokens, price, current_value, pnl);
        
        // Add to trades list for UI updates
        if let Ok(mut trades_guard) = trades_list.try_lock() {
            let symbol = holding.metadata.as_ref().and_then(|m| m.symbol.clone());
            let name = holding.metadata.as_ref().and_then(|m| m.name.clone());
            let image = holding.metadata.as_ref().and_then(|m| m.image.clone());
            let amount_tokens = holding.amount as f64 / 1_000_000.0; // Assuming 6 decimal places
            let amount_sol = amount_tokens * price;
            
            trades_guard.push(TradeRecord {
                mint: mint.to_string(),
                symbol,
                name,
                image,
                trade_type: "hold".to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                tx_signature: None,
                amount_sol,
                amount_tokens,
                price_per_token: price,
                profit_loss: Some(pnl),
                profit_loss_percent: Some(pnl_percent),
                reason: Some("Monitoring update".to_string()),
            });
            
            // Keep only recent trades (limit to 1000)
            if trades_guard.len() > 1000 {
                let excess = trades_guard.len() - 1000;
                trades_guard.drain(0..excess);
            }
        }
    }
    
    Ok(())
}
