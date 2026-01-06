// PumpPortal WebSocket client for new token detection
// Connects to wss://pumpportal.fun/api/data and subscribes to new token events

use futures_util::{stream::StreamExt, SinkExt};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Message types from PumpPortal WebSocket
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PumpPortalMessage {
    /// New token creation event
    NewToken(NewTokenEvent),
    /// Token trade event (not used for detection, but may be useful for monitoring)
    TokenTrade(TokenTradeEvent),
    /// Migration event
    Migration(MigrationEvent),
    /// Generic message (for unrecognized formats)
    Unknown(serde_json::Value),
}

/// New token creation event from PumpPortal
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTokenEvent {
    /// The signature of the creation transaction
    pub signature: String,
    /// Token mint address
    pub mint: String,
    /// Trader/creator address
    #[serde(default)]
    pub trader_public_key: Option<String>,
    /// Token name
    #[serde(default)]
    pub name: Option<String>,
    /// Token symbol
    #[serde(default)]
    pub symbol: Option<String>,
    /// Token description
    #[serde(default)]
    pub description: Option<String>,
    /// Image URI
    #[serde(default)]
    pub image_uri: Option<String>,
    /// Metadata URI
    #[serde(default)]
    pub metadata_uri: Option<String>,
    /// Twitter link
    #[serde(default)]
    pub twitter: Option<String>,
    /// Telegram link
    #[serde(default)]
    pub telegram: Option<String>,
    /// Bonding curve address
    #[serde(default)]
    pub bonding_curve_key: Option<String>,
    /// Virtual token reserves
    #[serde(default)]
    pub v_tokens_in_bonding_curve: Option<f64>,
    /// Virtual SOL reserves
    #[serde(default)]
    pub v_sol_in_bonding_curve: Option<f64>,
    /// Market cap in SOL
    #[serde(default)]
    pub market_cap_sol: Option<f64>,
    /// Initial buy amount (if any)
    #[serde(default)]
    pub initial_buy: Option<f64>,
    /// Transaction type (usually "create")
    #[serde(default, rename = "txType")]
    pub tx_type: Option<String>,
}

/// Token trade event from PumpPortal
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTradeEvent {
    pub signature: String,
    pub mint: String,
    #[serde(default)]
    pub trader_public_key: Option<String>,
    #[serde(default, rename = "txType")]
    pub tx_type: Option<String>,
    #[serde(default)]
    pub token_amount: Option<f64>,
    #[serde(default)]
    pub sol_amount: Option<f64>,
    #[serde(default)]
    pub v_tokens_in_bonding_curve: Option<f64>,
    #[serde(default)]
    pub v_sol_in_bonding_curve: Option<f64>,
    #[serde(default)]
    pub market_cap_sol: Option<f64>,
}

/// Migration event from PumpPortal
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationEvent {
    pub signature: String,
    pub mint: String,
}

/// Detected token info from PumpPortal event
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PumpPortalDetectedToken {
    pub signature: String,
    pub mint: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub image_uri: Option<String>,
    pub metadata_uri: Option<String>,
    pub creator: Option<String>,
    pub bonding_curve: Option<String>,
    pub virtual_token_reserves: Option<f64>,
    pub virtual_sol_reserves: Option<f64>,
    pub market_cap_sol: Option<f64>,
}

impl From<NewTokenEvent> for PumpPortalDetectedToken {
    fn from(event: NewTokenEvent) -> Self {
        Self {
            signature: event.signature,
            mint: event.mint,
            name: event.name,
            symbol: event.symbol,
            description: event.description,
            image_uri: event.image_uri,
            metadata_uri: event.metadata_uri,
            creator: event.trader_public_key,
            bonding_curve: event.bonding_curve_key,
            virtual_token_reserves: event.v_tokens_in_bonding_curve,
            virtual_sol_reserves: event.v_sol_in_bonding_curve,
            market_cap_sol: event.market_cap_sol,
        }
    }
}

/// Run the PumpPortal WebSocket connection for new token detection
/// 
/// This connects to PumpPortal's WebSocket API and subscribes to new token events.
/// Detected tokens are sent via the provided channel for processing by the main loop.
pub async fn run_pumpportal_ws(
    ws_url: &str,
    tx: mpsc::Sender<PumpPortalDetectedToken>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Outer reconnection loop
    loop {
        info!("Connecting to PumpPortal WebSocket at {}", ws_url);
        
        match connect_async(ws_url).await {
            Ok((ws_stream, _)) => {
                let (mut write, mut read) = ws_stream.split();
                
                info!("PumpPortal WebSocket connected successfully");
                
                // Subscribe to new token creation events
                let subscribe_payload = json!({
                    "method": "subscribeNewToken"
                });
                
                if let Err(e) = write.send(Message::Text(subscribe_payload.to_string())).await {
                    error!("Failed to send subscribeNewToken to PumpPortal: {}", e);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
                
                info!("Subscribed to PumpPortal new token events");
                
                // Process incoming messages
                loop {
                    match read.next().await {
                        Some(Ok(Message::Text(text))) => {
                            debug!("PumpPortal raw message: {}", text);
                            
                            // Try to parse as new token event
                            match serde_json::from_str::<NewTokenEvent>(&text) {
                                Ok(event) => {
                                    // Check if this is a "create" transaction
                                    let is_create = event.tx_type.as_deref() == Some("create") || 
                                                   event.tx_type.is_none(); // Default to create if not specified
                                    
                                    if is_create {
                                        info!(
                                            "PumpPortal new token detected: {} ({}) - mint: {}",
                                            event.name.as_deref().unwrap_or("Unknown"),
                                            event.symbol.as_deref().unwrap_or("???"),
                                            event.mint
                                        );
                                        
                                        let detected = PumpPortalDetectedToken::from(event);
                                        if let Err(e) = tx.send(detected).await {
                                            error!("Failed to send detected token: {}", e);
                                        }
                                    } else {
                                        debug!("PumpPortal event is not a create: {:?}", event.tx_type);
                                    }
                                }
                                Err(_) => {
                                    // Could be a different message type or acknowledgment
                                    debug!("PumpPortal message not a new token event: {}", text);
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            // Respond to ping with pong
                            if let Err(e) = write.send(Message::Pong(data)).await {
                                warn!("Failed to send pong: {}", e);
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            warn!("PumpPortal WebSocket closed by server");
                            break;
                        }
                        Some(Err(e)) => {
                            error!("PumpPortal WebSocket error: {}", e);
                            break;
                        }
                        None => {
                            warn!("PumpPortal WebSocket stream ended");
                            break;
                        }
                        _ => {
                            // Ignore other message types (Binary, Pong, etc.)
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to connect to PumpPortal WebSocket: {}", e);
            }
        }
        
        // Wait before reconnecting
        info!("PumpPortal WebSocket disconnected; reconnecting in 2 seconds...");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_new_token_event() {
        let json_str = r#"{
            "signature": "test_sig_123",
            "mint": "So11111111111111111111111111111111111111112",
            "traderPublicKey": "HN7cABqLq46Es1jh92dQQisAq662SmxELLLsHHe4YWrH",
            "txType": "create",
            "name": "Test Token",
            "symbol": "TEST",
            "description": "A test token",
            "imageUri": "https://example.com/image.png",
            "metadataUri": "https://example.com/metadata.json",
            "bondingCurveKey": "3k8eKHcWD1K6c4m3L1qH2xPd8qVZJGCfwRN7VNjQ1KQj",
            "vTokensInBondingCurve": 1000000000.0,
            "vSolInBondingCurve": 30.0,
            "marketCapSol": 30.0
        }"#;

        let event: NewTokenEvent = serde_json::from_str(json_str).unwrap();
        assert_eq!(event.signature, "test_sig_123");
        assert_eq!(event.mint, "So11111111111111111111111111111111111111112");
        assert_eq!(event.name, Some("Test Token".to_string()));
        assert_eq!(event.symbol, Some("TEST".to_string()));
        assert_eq!(event.tx_type, Some("create".to_string()));
        
        let detected: PumpPortalDetectedToken = event.into();
        assert_eq!(detected.signature, "test_sig_123");
        assert_eq!(detected.name, Some("Test Token".to_string()));
    }

    #[test]
    fn test_parse_minimal_event() {
        // Sometimes events come with minimal data
        let json_str = r#"{
            "signature": "minimal_sig",
            "mint": "TokenMint123456789"
        }"#;

        let event: NewTokenEvent = serde_json::from_str(json_str).unwrap();
        assert_eq!(event.signature, "minimal_sig");
        assert_eq!(event.mint, "TokenMint123456789");
        assert!(event.name.is_none());
        assert!(event.tx_type.is_none());
    }
}
