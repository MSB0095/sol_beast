use crate::error::CoreError;
use crate::models::{BondingCurveState, Holding, StrategyConfig};
use chrono::Utc;

/// Trading strategy implementation
pub struct TradingStrategy {
    config: StrategyConfig,
}

impl TradingStrategy {
    pub fn new(config: StrategyConfig) -> Self {
        Self { config }
    }

    /// Check if we should buy this token based on strategy filters
    pub fn should_buy(
        &self,
        curve_state: &BondingCurveState,
        current_price: f64,
    ) -> Result<bool, CoreError> {
        if !self.config.enable_safer_sniping {
            return Ok(true);
        }

        // Check max price per token
        if current_price > self.config.max_sol_per_token {
            log::debug!(
                "Rejected: price {} exceeds max {}",
                current_price,
                self.config.max_sol_per_token
            );
            return Ok(false);
        }

        // Check minimum liquidity (real SOL reserves)
        let liquidity_sol = curve_state.real_sol_reserves as f64 / 1e9;
        if liquidity_sol < 0.01 {
            log::debug!("Rejected: liquidity {} SOL too low", liquidity_sol);
            return Ok(false);
        }

        // Check if bonding curve is complete
        if curve_state.complete {
            log::debug!("Rejected: bonding curve already complete");
            return Ok(false);
        }

        Ok(true)
    }

    /// Check if we should sell this holding (TP, SL, or timeout)
    pub fn should_sell(
        &self,
        holding: &Holding,
        current_price: f64,
    ) -> Result<Option<String>, CoreError> {
        let price_change_percent = ((current_price - holding.buy_price) / holding.buy_price) * 100.0;

        // Take profit
        if price_change_percent >= self.config.tp_percent {
            return Ok(Some(format!(
                "Take profit at +{:.2}%",
                price_change_percent
            )));
        }

        // Stop loss
        if price_change_percent <= self.config.sl_percent {
            return Ok(Some(format!(
                "Stop loss at {:.2}%",
                price_change_percent
            )));
        }

        // Timeout
        let holding_duration = Utc::now().signed_duration_since(holding.buy_time);
        if holding_duration.num_seconds() >= self.config.timeout_secs {
            return Ok(Some(format!(
                "Timeout after {} seconds ({:.2}% P/L)",
                holding_duration.num_seconds(),
                price_change_percent
            )));
        }

        Ok(None)
    }

    /// Calculate profit/loss for a holding
    pub fn calculate_profit_loss(
        &self,
        holding: &Holding,
        sell_price: f64,
        buy_amount_sol: f64,
    ) -> (f64, f64) {
        let price_change_percent = ((sell_price - holding.buy_price) / holding.buy_price) * 100.0;
        let profit_loss = buy_amount_sol * (price_change_percent / 100.0);
        
        (profit_loss, price_change_percent)
    }

    pub fn get_config(&self) -> &StrategyConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: StrategyConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::OnchainFullMetadata;

    #[test]
    fn test_should_sell_take_profit() {
        let strategy = TradingStrategy::new(StrategyConfig::default());
        
        let holding = Holding {
            mint: "test".to_string(),
            amount: 1_000_000,
            buy_price: 0.00001,
            buy_time: Utc::now(),
            metadata: None,
            onchain_raw: None,
            onchain: None,
        };

        // Price up more than 30% to definitely trigger TP
        // 0.00001 * 1.31 = 0.0000131
        let current_price = 0.0000131;
        let result = strategy.should_sell(&holding, current_price).unwrap();
        assert!(result.is_some(), "Expected TP to trigger at 31% gain");
        let reason = result.unwrap();
        assert!(reason.contains("Take profit"), "Expected 'Take profit' message, got: {}", reason);
    }

    #[test]
    fn test_should_sell_stop_loss() {
        let strategy = TradingStrategy::new(StrategyConfig::default());
        
        let holding = Holding {
            mint: "test".to_string(),
            amount: 1_000_000,
            buy_price: 0.00001,
            buy_time: Utc::now(),
            metadata: None,
            onchain_raw: None,
            onchain: None,
        };

        // Price down 20% - should trigger SL
        let current_price = 0.000008;
        let result = strategy.should_sell(&holding, current_price).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Stop loss"));
    }

    #[test]
    fn test_should_buy_price_filter() {
        let strategy = TradingStrategy::new(StrategyConfig::default());
        
        let curve = BondingCurveState {
            virtual_token_reserves: 1_000_000_000_000,
            virtual_sol_reserves: 30_000_000_000,
            real_token_reserves: 0,
            real_sol_reserves: 100_000_000, // 0.1 SOL
            token_total_supply: 1_000_000_000_000,
            complete: false,
            creator: None,
        };

        // Price too high
        let high_price = 0.001;
        assert!(!strategy.should_buy(&curve, high_price).unwrap());

        // Price acceptable
        let good_price = 0.00005;
        assert!(strategy.should_buy(&curve, good_price).unwrap());
    }
}
