use crate::models::Holding;
use crate::settings::Settings;
use chrono::Utc;

#[derive(Debug, Clone, PartialEq)]
pub enum TradeAction {
    Hold,
    Sell(SellReason),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SellReason {
    TakeProfit(f64), // profit_percent
    StopLoss(f64),   // profit_percent
    Timeout(i64),    // elapsed_seconds
}

impl std::fmt::Display for SellReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SellReason::TakeProfit(pct) => write!(f, "TP ({:.2}%)", pct),
            SellReason::StopLoss(pct) => write!(f, "SL ({:.2}%)", pct),
            SellReason::Timeout(secs) => write!(f, "TIMEOUT ({}s)", secs),
        }
    }
}

/// Evaluate a holding against current price and settings to determine if it should be sold.
pub fn evaluate_position(
    holding: &Holding,
    current_price: f64,
    settings: &Settings,
) -> TradeAction {
    // Calculate profit/loss
    let profit_percent = if holding.buy_price != 0.0 {
        ((current_price - holding.buy_price) / holding.buy_price) * 100.0
    } else {
        0.0
    };

    // Calculate elapsed time
    let elapsed_secs = Utc::now()
        .signed_duration_since(holding.buy_time)
        .num_seconds();

    // Check conditions
    if profit_percent >= settings.tp_percent {
        TradeAction::Sell(SellReason::TakeProfit(profit_percent))
    } else if profit_percent <= settings.sl_percent {
        TradeAction::Sell(SellReason::StopLoss(profit_percent))
    } else if elapsed_secs >= settings.timeout_secs {
        TradeAction::Sell(SellReason::Timeout(elapsed_secs))
    } else {
        TradeAction::Hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Holding;
    use crate::settings::Settings;
    use chrono::Utc;

    fn mock_holding(buy_price: f64, buy_time_secs_ago: i64) -> Holding {
        Holding {
            mint: "test_mint".to_string(),
            amount: 1000,
            buy_price,
            buy_time: Utc::now() - chrono::Duration::seconds(buy_time_secs_ago),
            creator: None,
            metadata: None,
            onchain_raw: None,
            onchain: None,
        }
    }

    fn mock_settings() -> Settings {
        // Create a minimal settings struct for testing
        // We have to construct the full struct, so we'll use a helper or default if available
        // Since Settings doesn't implement Default in the snippet I saw, I'll construct it manually
        // with dummy values for irrelevant fields.
        // Actually, let's just assume we can construct it.
        // For this test, I'll use a simplified construction if possible, or just fill fields.
        // Since I can't easily construct full Settings without all fields, I'll rely on the fact
        // that I'm in the same crate.
        
        // HACK: I'll just use serde_json to create it from default-like JSON to avoid filling all fields manually
        let json = r#"{
            "solana_ws_urls": [],
            "solana_rpc_urls": [],
            "pump_fun_program": "",
            "metadata_program": "",
            "tp_percent": 50.0,
            "sl_percent": -20.0,
            "timeout_secs": 60,
            "cache_capacity": 100,
            "price_cache_ttl_secs": 60
        }"#;
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn test_take_profit() {
        let settings = mock_settings();
        let holding = mock_holding(1.0, 10);
        let current_price = 1.6; // 60% profit
        
        match evaluate_position(&holding, current_price, &settings) {
            TradeAction::Sell(SellReason::TakeProfit(pct)) => assert!(pct > 59.9),
            _ => panic!("Should be TakeProfit"),
        }
    }

    #[test]
    fn test_stop_loss() {
        let settings = mock_settings();
        let holding = mock_holding(1.0, 10);
        let current_price = 0.7; // -30% loss
        
        match evaluate_position(&holding, current_price, &settings) {
            TradeAction::Sell(SellReason::StopLoss(pct)) => assert!(pct < -29.9),
            _ => panic!("Should be StopLoss"),
        }
    }

    #[test]
    fn test_timeout() {
        let settings = mock_settings();
        let holding = mock_holding(1.0, 61); // 61 secs ago
        let current_price = 1.0; // No change
        
        match evaluate_position(&holding, current_price, &settings) {
            TradeAction::Sell(SellReason::Timeout(secs)) => assert!(secs >= 61),
            _ => panic!("Should be Timeout"),
        }
    }

    #[test]
    fn test_hold() {
        let settings = mock_settings();
        let holding = mock_holding(1.0, 10);
        let current_price = 1.1; // 10% profit (below TP)
        
        assert_eq!(evaluate_position(&holding, current_price, &settings), TradeAction::Hold);
    }
}
