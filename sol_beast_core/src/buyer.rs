// Core buyer logic - platform agnostic
// Evaluates buy heuristics and validates token purchases

use crate::settings::Settings;
use crate::models::BondingCurveState;
use crate::error::CoreError;
use log::{info, warn};

/// Result of buy heuristic evaluation
#[derive(Debug, Clone)]
pub struct BuyEvaluation {
    pub should_buy: bool,
    pub reason: String,
    pub token_amount: u64,
    pub buy_price_sol: f64,
}

/// Evaluates whether a token should be bought based on configured heuristics
pub fn evaluate_buy_heuristics(
    _mint: &str,
    sol_amount: f64,
    buy_price_sol: f64,
    bonding_curve_state: Option<&BondingCurveState>,
    settings: &Settings,
) -> BuyEvaluation {
    // Calculate token amount (pump.fun uses 6 decimals)
    let token_amount = ((sol_amount / buy_price_sol) * 1_000_000.0) as u64;
    
    // If safer sniping is disabled, always approve
    if !settings.enable_safer_sniping {
        return BuyEvaluation {
            should_buy: true,
            reason: "Safety checks disabled - auto-approve".to_string(),
            token_amount,
            buy_price_sol,
        };
    }
    
    // Check 1: Minimum tokens threshold
    if token_amount < settings.min_tokens_threshold {
        return BuyEvaluation {
            should_buy: false,
            reason: format!(
                "Token amount {} is below minimum threshold {} (price too high: {:.18} SOL/token)",
                token_amount, settings.min_tokens_threshold, buy_price_sol
            ),
            token_amount,
            buy_price_sol,
        };
    }
    
    // Check 2: Maximum SOL per token (price ceiling)
    if buy_price_sol > settings.max_sol_per_token {
        return BuyEvaluation {
            should_buy: false,
            reason: format!(
                "Token price {:.18} SOL/token exceeds maximum {:.18} SOL/token (already too expensive)",
                buy_price_sol, settings.max_sol_per_token
            ),
            token_amount,
            buy_price_sol,
        };
    }
    
    // Check 3: Liquidity checks (if bonding curve data available)
    if let Some(state) = bonding_curve_state {
        let real_sol = state.real_sol_reserves as f64 / 1_000_000_000.0;
        
        if real_sol < settings.min_liquidity_sol {
            return BuyEvaluation {
                should_buy: false,
                reason: format!(
                    "Liquidity {:.4} SOL is below minimum {:.4} SOL (too risky)",
                    real_sol, settings.min_liquidity_sol
                ),
                token_amount,
                buy_price_sol,
            };
        }
        
        if real_sol > settings.max_liquidity_sol {
            return BuyEvaluation {
                should_buy: false,
                reason: format!(
                    "Liquidity {:.4} SOL exceeds maximum {:.4} SOL (too late)",
                    real_sol, settings.max_liquidity_sol
                ),
                token_amount,
                buy_price_sol,
            };
        }
    }
    
    // All checks passed
    BuyEvaluation {
        should_buy: true,
        reason: format!(
            "All checks passed: {} tokens for {} SOL at {:.18} SOL/token",
            token_amount, sol_amount, buy_price_sol
        ),
        token_amount,
        buy_price_sol,
    }
}

/// Calculate profit/loss percentage for a holding
pub fn calculate_pnl_percent(buy_price: f64, current_price: f64) -> f64 {
    if buy_price == 0.0 {
        return 0.0;
    }
    ((current_price - buy_price) / buy_price) * 100.0
}

/// Check if take profit condition is met
pub fn check_take_profit(pnl_percent: f64, tp_percent: f64) -> bool {
    pnl_percent >= tp_percent
}

/// Check if stop loss condition is met
pub fn check_stop_loss(pnl_percent: f64, sl_percent: f64) -> bool {
    pnl_percent <= sl_percent
}

/// Check if timeout condition is met
pub fn check_timeout(buy_time: i64, current_time: i64, timeout_secs: i64) -> bool {
    (current_time - buy_time) >= timeout_secs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_pnl() {
        assert_eq!(calculate_pnl_percent(1.0, 2.0), 100.0);
        assert_eq!(calculate_pnl_percent(1.0, 0.5), -50.0);
        assert_eq!(calculate_pnl_percent(0.0, 1.0), 0.0);
    }

    #[test]
    fn test_take_profit() {
        assert!(check_take_profit(100.0, 50.0));
        assert!(check_take_profit(100.0, 100.0));
        assert!(!check_take_profit(49.0, 50.0));
    }

    #[test]
    fn test_stop_loss() {
        assert!(check_stop_loss(-60.0, -50.0));
        assert!(check_stop_loss(-50.0, -50.0));
        assert!(!check_stop_loss(-40.0, -50.0));
    }

    #[test]
    fn test_timeout() {
        assert!(check_timeout(0, 60, 50));
        assert!(check_timeout(0, 50, 50));
        assert!(!check_timeout(0, 40, 50));
    }
}
