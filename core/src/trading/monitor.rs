use crate::models::{Holding, PriceCache};
use crate::Settings;
use crate::{api::{TradeRecord, BotControl}, state::BuyRecord};
use crate::rpc_client::RpcClient as CoreRpcClient;
use std::{collections::HashMap, sync::Arc};
use std::sync::atomic::AtomicUsize;
use tokio::sync::{Mutex, mpsc};
use crate::ws::WsRequest;
use lru::LruCache;
use log::{info, debug, error};
use std::num::NonZeroUsize;
use crate::core_mod::error::CoreError;
use crate::connectivity::api::ApiState;
use tokio::task::JoinHandle;
use tokio::sync::oneshot;

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
    info!("Starting holdings monitor - real trading: {}, active holdings: {}", is_real, holdings.lock().await.len());
    
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

/// Handle to the background monitor tasks (WS connections and monitor loop)
pub struct MonitorHandle {
    stop_tx: Option<oneshot::Sender<()>>,
    join_handles: Vec<JoinHandle<()>>,
}

impl MonitorHandle {
    pub async fn stop(mut self) -> Result<(), CoreError> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        for h in self.join_handles {
            if let Err(e) = h.await {
                return Err(CoreError::Internal(format!("Monitor join error: {}", e)));
            }
        }
        Ok(())
    }
}

/// Start monitor tasks: WSS subscriptions and holdings monitor.
/// Returns a MonitorHandle to allow stopping the tasks.
pub async fn start_monitor_tasks(
    api_state: ApiState,
    rpc_client: Arc<dyn CoreRpcClient>,
    keypair: Option<Arc<dyn crate::Signer>>,
    simulate_keypair: Option<Arc<dyn crate::Signer>>,
    price_cache: Arc<Mutex<PriceCache>>,
    settings: Arc<Settings>,
) -> Result<MonitorHandle, CoreError> {
    // Create common shared structures
    let holdings: Arc<Mutex<std::collections::HashMap<String, Holding>>> = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let trades_map: Arc<Mutex<std::collections::HashMap<String, crate::state::BuyRecord>>> = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let sub_map: Arc<Mutex<std::collections::HashMap<String, (usize, u64)>>> = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let trades_list: Arc<tokio::sync::Mutex<Vec<crate::api::TradeRecord>>> = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let next_wss_sender = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut join_handles: Vec<JoinHandle<()>> = Vec::new();
    let mut ws_senders: Vec<tokio::sync::mpsc::Sender<WsRequest>> = Vec::new();

    let (stop_tx, stop_rx) = oneshot::channel::<()>();

    // For each WSS URL, spawn a run_ws task
    let (detected_tx, mut detected_rx) = tokio::sync::mpsc::channel::<String>(256);
    for wss_url in settings.solana_ws_urls.clone().into_iter() {
        let (tx, rx) = tokio::sync::mpsc::channel::<WsRequest>(64);
        ws_senders.push(tx.clone());
        let (tx_string_sender, rx_string_receiver) = tokio::sync::mpsc::channel::<crate::connectivity::ws::OutgoingMessage>(64);
        let holdings_clone = holdings.clone();
        let price_cache_clone = price_cache.clone();
        let settings_clone = settings.clone();
        let rx_clone = rx;

        // run_ws returns Result - spawn and ignore result inside
        let seen = Arc::new(Mutex::new(LruCache::<String, ()>::new(NonZeroUsize::new(100).unwrap())));
        let detected_tx_clone = detected_tx.clone();
        let handle = tokio::spawn(async move {
            let _ = crate::connectivity::ws::run_ws(
                wss_url.as_str(),
                tx_string_sender,
                rx_string_receiver,
                seen,
                holdings_clone,
                price_cache_clone,
                rx_clone,
                settings_clone,
                Some(detected_tx_clone),
            ).await;
        });
        join_handles.push(handle);
    }

    // Create ws_control_senders Arc
    let ws_controls_arc = Arc::new(ws_senders);

    // Spawn detector task: listens for tokens detected by WS and auto-subscribes
    let ws_controls_clone2 = ws_controls_arc.clone();
    let sub_map_clone2 = sub_map.clone();
    let settings_clone2 = settings.clone();
    let next_wss_sender_clone2 = next_wss_sender.clone();
    // We don't need rpc_client in the detector currently. Keep a clone ready if future extension needed.
    let detector_handle = tokio::spawn(async move {
        while let Some(mint) = detected_rx.recv().await {
            debug!("Detector received new mint: {}", mint);
            if !settings_clone2.auto_subscribe_on_mint {
                continue;
            }
            // Do not subscribe if we already have it
            {
                let sub_map_guard = sub_map_clone2.lock().await;
                if sub_map_guard.contains_key(&mint) {
                    continue;
                }
            }
            // Pick a WSS sender to use
            if ws_controls_clone2.len() == 0 {
                continue;
            }
            let idx = next_wss_sender_clone2.fetch_add(1, std::sync::atomic::Ordering::SeqCst) % ws_controls_clone2.len();
            if let Some(sender) = ws_controls_clone2.get(idx) {
                let (resp_tx, resp_rx) = oneshot::channel::<Result<u64, String>>();
                if let Err(e) = sender.send(crate::ws::WsRequest::Subscribe { account: mint.clone(), mint: mint.clone(), resp: resp_tx }).await {
                    error!("Detector failed to send subscribe request for {}: {}", mint, e);
                    continue;
                }
                match resp_rx.await {
                    Ok(Ok(sub_id)) => {
                        let mut sub_map_guard = sub_map_clone2.lock().await;
                        sub_map_guard.insert(mint.clone(), (idx, sub_id));
                        info!("Detector subscribed {} on wss index {}: sub_id {}", mint, idx, sub_id);
                    }
                    Ok(Err(e)) => error!("Detector subscribe failed for {}: {}", mint, e),
                    Err(e) => error!("Detector oneshot canceled for {}: {}", mint, e),
                }
            }
        }
    });
    join_handles.push(detector_handle);

    // Spawn the monitor_holdings loop
    let holdings_clone_for_monitor = holdings.clone();
    let price_cache_clone_for_monitor = price_cache.clone();
    let rpc_client_for_monitor = rpc_client.clone();
    let settings_clone_for_monitor = settings.clone();
    let trades_map_clone = trades_map.clone();
    let ws_controls_clone = ws_controls_arc.clone();
    let sub_map_clone = sub_map.clone();
    let next_wss_sender_clone = next_wss_sender.clone();
    let trades_list_clone = trades_list.clone();
    let bot_control_clone = api_state.bot_control.clone();

    let monitor_handle = tokio::spawn(async move {
        tokio::select! {
            _ = monitor_holdings(
                holdings_clone_for_monitor,
                price_cache_clone_for_monitor,
                rpc_client_for_monitor,
                !settings_clone_for_monitor.buy_amount.is_nan() && settings_clone_for_monitor.buy_amount > 0.0, // is_real heuristic (could be improved)
                keypair,
                simulate_keypair,
                settings_clone_for_monitor,
                trades_map_clone,
                ws_controls_clone,
                sub_map_clone,
                next_wss_sender_clone,
                trades_list_clone,
                bot_control_clone,
            ) => {},
            _ = stop_rx => {
                // Received stop signal
            }
        }
    });
    join_handles.push(monitor_handle);

    Ok(MonitorHandle { stop_tx: Some(stop_tx), join_handles })
}

async fn monitor_single_holding(
    mint: &str,
    holding: &Holding,
    price_cache: &Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
    _trades_map: &Arc<Mutex<HashMap<String, BuyRecord>>>,
    ws_control_senders: &Arc<Vec<mpsc::Sender<WsRequest>>>,
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

    // Ensure we have an active websocket subscription for this holding
    // If not, send a Subscribe control message requesting updates
    {
        let mut sub_map_guard = _sub_map.lock().await;
        if !sub_map_guard.contains_key(mint) {
            if let Some(sender) = ws_control_senders.get(0) {
                let (resp_tx, resp_rx) = oneshot::channel::<Result<u64, String>>();
                if let Err(e) = sender.send(WsRequest::Subscribe { account: mint.to_string(), mint: mint.to_string(), resp: resp_tx }).await {
                    error!("Failed to send subscribe request for {}: {}", mint, e);
                } else {
                    match resp_rx.await {
                        Ok(Ok(sub_id)) => {
                            sub_map_guard.insert(mint.to_string(), (0usize, sub_id));
                            info!("Subscribed {} with sub_id {}", mint, sub_id);
                        }
                        Ok(Err(err_str)) => {
                            error!("Subscribe failed for {}: {}", mint, err_str);
                        }
                        Err(e) => {
                            error!("Subscribe oneshot canceled for {}: {}", mint, e);
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}
