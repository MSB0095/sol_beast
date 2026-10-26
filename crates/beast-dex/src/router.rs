//! Route graph and cycle search.
//!
//! Radar only uses pools with a local model. The router searches 2-hop and
//! 3-hop cycles starting from configured quote mints, optimises the input with
//! a bounded golden-section search, and returns *gross* profit. Transaction
//! fees, tips, rent and the on-chain guard are applied by the strategy layer.

use crate::address::Address;
use crate::model::{PoolSnapshot, RouteLeg, RouteQuote};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    pub max_hops: usize,
    /// Cap on price impact *per leg*.
    pub max_impact_bps: u32,
    /// Absolute cap in the start mint's raw units.
    pub max_input_raw: u64,
    /// Fraction of the first pool's reserve to risk, in bps (100 = 1%).
    pub pool_fraction_bps: u32,
    /// Routes with less gross profit are discarded.
    pub min_gross_lamports: i64,
    pub max_routes: usize,
    pub start_mints: Vec<Address>,
    /// Empty means any intermediate mint.
    pub intermediate_mints: Vec<Address>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            max_hops: 3,
            max_impact_bps: 150,
            max_input_raw: 5_000_000_000,
            pool_fraction_bps: 100,
            min_gross_lamports: 0,
            max_routes: 20_000,
            start_mints: Vec::new(),
            intermediate_mints: Vec::new(),
        }
    }
}

pub struct Router {
    pools: Vec<PoolSnapshot>,
    index: HashMap<Address, Vec<usize>>,
    config: RouterConfig,
}

impl Router {
    pub fn new(pools: Vec<PoolSnapshot>, config: RouterConfig) -> Self {
        let mut index: HashMap<Address, Vec<usize>> = HashMap::new();
        for (position, pool) in pools.iter().enumerate() {
            if !pool.is_locally_quotable() {
                continue;
            }
            index.entry(pool.mint_a).or_default().push(position);
            if pool.mint_b != pool.mint_a {
                index.entry(pool.mint_b).or_default().push(position);
            }
        }
        Self {
            pools,
            index,
            config,
        }
    }

    pub fn pool_count(&self) -> usize {
        self.pools.len()
    }

    pub fn pools(&self) -> &[PoolSnapshot] {
        &self.pools
    }

    pub fn config(&self) -> &RouterConfig {
        &self.config
    }

    pub fn opportunities(&self) -> Vec<RouteQuote> {
        let starts: Vec<Address> = if self.config.start_mints.is_empty() {
            self.index.keys().copied().collect()
        } else {
            self.config.start_mints.clone()
        };
        let mut best: HashMap<String, RouteQuote> = HashMap::new();
        for start in starts {
            for route in self.routes_for(start) {
                if let Some(quote) = self.optimize(&route, start) {
                    if quote.gross_profit <= self.config.min_gross_lamports {
                        continue;
                    }
                    let key = route_key(start, &route, &self.pools);
                    match best.get(&key) {
                        Some(existing) if existing.gross_profit >= quote.gross_profit => {}
                        _ => {
                            best.insert(key, quote);
                        }
                    }
                }
            }
        }
        let mut opportunities: Vec<_> = best.into_values().collect();
        opportunities.sort_by_key(|opportunity| std::cmp::Reverse(opportunity.gross_profit));
        opportunities
    }

    fn routes_for(&self, start: Address) -> Vec<Vec<usize>> {
        let mut routes = Vec::new();
        let mut seen = HashSet::new();
        let Some(first_pools) = self.index.get(&start) else {
            return routes;
        };
        for &p1 in first_pools {
            if routes.len() >= self.config.max_routes {
                break;
            }
            if let Some(mid) = self.pools[p1].other_mint(start) {
                if !self.allowed_intermediate(mid) {
                    continue;
                }
                if self.config.max_hops >= 2 {
                    if let Some(second_pools) = self.index.get(&mid) {
                        for &p2 in second_pools {
                            if p2 == p1 || routes.len() >= self.config.max_routes {
                                continue;
                            }
                            if self.pools[p2].other_mint(mid) == Some(start) {
                                let route = vec![p1, p2];
                                let key = route_key(start, &route, &self.pools);
                                if seen.insert(key) {
                                    routes.push(route);
                                }
                            }
                        }
                    }
                }
                if self.config.max_hops >= 3 {
                    if let Some(second_pools) = self.index.get(&mid) {
                        for &p2 in second_pools {
                            if p2 == p1 || routes.len() >= self.config.max_routes {
                                continue;
                            }
                            let Some(mid2) = self.pools[p2].other_mint(mid) else {
                                continue;
                            };
                            if mid2 == start || mid2 == mid || !self.allowed_intermediate(mid2) {
                                continue;
                            }
                            let Some(third_pools) = self.index.get(&mid2) else {
                                continue;
                            };
                            for &p3 in third_pools {
                                if p3 == p1 || p3 == p2 || routes.len() >= self.config.max_routes {
                                    continue;
                                }
                                if self.pools[p3].other_mint(mid2) == Some(start) {
                                    let route = vec![p1, p2, p3];
                                    let key = route_key(start, &route, &self.pools);
                                    if seen.insert(key) {
                                        routes.push(route);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        routes
    }

    fn allowed_intermediate(&self, mint: Address) -> bool {
        self.config.intermediate_mints.is_empty() || self.config.intermediate_mints.contains(&mint)
    }

    fn evaluate(&self, route: &[usize], start: Address, amount_in: u64) -> Option<RouteQuote> {
        if amount_in == 0 || route.is_empty() {
            return None;
        }
        let mut mint = start;
        let mut amount = amount_in;
        let mut legs = Vec::with_capacity(route.len());
        let mut min_confidence = 1.0f32;
        let mut max_impact = 0u32;
        for &pool_index in route {
            let pool = self.pools.get(pool_index)?;
            let max_leg_input = pool.max_input_raw(mint, self.config.pool_fraction_bps);
            if max_leg_input == 0 || amount > max_leg_input {
                return None;
            }
            let quote = pool.quote(mint, amount).ok()?;
            if quote.price_impact_bps > self.config.max_impact_bps {
                return None;
            }
            max_impact = max_impact.max(quote.price_impact_bps);
            min_confidence = min_confidence.min(quote.confidence);
            legs.push(RouteLeg {
                pool: quote.pool,
                venue: quote.venue,
                mint_in: quote.mint_in,
                mint_out: quote.mint_out,
                amount_in: quote.amount_in,
                amount_out: quote.amount_out,
                fee_paid: quote.fee_paid,
                price_impact_bps: quote.price_impact_bps,
            });
            amount = quote.amount_out;
            mint = quote.mint_out;
        }
        if mint != start {
            return None;
        }
        let gross = amount as i128 - amount_in as i128;
        Some(RouteQuote {
            start_mint: start,
            end_mint: mint,
            amount_in,
            amount_out: amount,
            gross_profit: gross.clamp(i64::MIN as i128, i64::MAX as i128) as i64,
            legs,
            min_confidence,
            max_price_impact_bps: max_impact,
        })
    }

    fn optimize(&self, route: &[usize], start: Address) -> Option<RouteQuote> {
        let first = self.pools.get(*route.first()?)?;
        let upper = first
            .max_input_raw(start, self.config.pool_fraction_bps)
            .min(self.config.max_input_raw);
        if upper == 0 {
            return None;
        }
        let mut best: Option<RouteQuote> = None;
        let mut consider = |amount: u64| {
            if let Some(quote) = self.evaluate(route, start, amount) {
                if best
                    .as_ref()
                    .map(|current| quote.gross_profit > current.gross_profit)
                    .unwrap_or(true)
                {
                    best = Some(quote);
                }
            }
        };

        // Coarse grid catches routes whose objective is not perfectly unimodal
        // because of stable/CLMM approximations.
        let points = 16u64;
        for i in 1..=points {
            consider((upper / points).saturating_mul(i).max(1).min(upper));
        }

        // Golden-section search around [1, upper].
        let phi = 0.618_033_988_749_895;
        let mut low = 1.0f64;
        let mut high = upper as f64;
        let mut x1 = high - phi * (high - low);
        let mut x2 = low + phi * (high - low);
        let mut f1 = self.objective(route, start, x1);
        let mut f2 = self.objective(route, start, x2);
        for _ in 0..56 {
            if f1 < f2 {
                low = x1;
                x1 = x2;
                f1 = f2;
                x2 = low + phi * (high - low);
                f2 = self.objective(route, start, x2);
            } else {
                high = x2;
                x2 = x1;
                f2 = f1;
                x1 = high - phi * (high - low);
                f1 = self.objective(route, start, x1);
            }
        }
        let best_point = ((low + high) / 2.0).round().clamp(1.0, upper as f64) as u64;
        consider(best_point);
        for delta in [1u64, 10, 100, 1_000] {
            if best_point > delta {
                consider(best_point - delta);
            }
            consider(best_point.saturating_add(delta).min(upper));
        }
        consider(upper);
        best
    }

    fn objective(&self, route: &[usize], start: Address, amount: f64) -> f64 {
        if !amount.is_finite() || amount < 1.0 {
            return f64::NEG_INFINITY;
        }
        self.evaluate(route, start, amount.round() as u64)
            .map(|quote| quote.gross_profit as f64)
            .unwrap_or(f64::NEG_INFINITY)
    }
}

pub fn route_key(start: Address, route: &[usize], pools: &[PoolSnapshot]) -> String {
    let mut key = String::with_capacity(16 + route.len() * 45);
    key.push_str(&start.to_string());
    for pool_index in route {
        key.push('|');
        if let Some(pool) = pools.get(*pool_index) {
            key.push_str(&pool.address.to_string());
        }
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PoolModel;

    fn address(n: u8) -> Address {
        Address([n; 32])
    }

    fn pool(
        id: u8,
        mint_a: Address,
        mint_b: Address,
        reserve_a: u64,
        reserve_b: u64,
        fee_bps: u32,
    ) -> PoolSnapshot {
        PoolSnapshot {
            address: address(id),
            venue: "test".into(),
            program_id: address(99),
            mint_a,
            mint_b,
            vault_a: address(100),
            vault_b: address(101),
            decimals_a: 9,
            decimals_b: 6,
            fee_bps,
            model: PoolModel::ConstantProduct {
                reserve_a,
                reserve_b,
            },
            slot: 1,
            confidence: 1.0,
        }
    }

    #[test]
    fn finds_profitable_two_hop_cycle() {
        let sol = address(1);
        let usdc = address(2);
        let pools = vec![
            pool(10, sol, usdc, 1_000_000_000, 100_000_000_000, 0),
            pool(11, sol, usdc, 1_000_000_000, 101_000_000_000, 0),
        ];
        let router = Router::new(
            pools,
            RouterConfig {
                max_hops: 2,
                max_impact_bps: 500,
                max_input_raw: 1_000_000_000,
                pool_fraction_bps: 1_000,
                min_gross_lamports: 0,
                max_routes: 100,
                start_mints: vec![sol],
                intermediate_mints: vec![usdc],
            },
        );
        let opportunities = router.opportunities();
        assert_eq!(opportunities.len(), 1);
        assert!(opportunities[0].gross_profit > 0);
        assert_eq!(opportunities[0].legs.len(), 2);
        assert_eq!(opportunities[0].start_mint, sol);
        assert_eq!(opportunities[0].end_mint, sol);
    }

    #[test]
    fn no_opportunity_when_prices_match() {
        let sol = address(1);
        let usdc = address(2);
        let pools = vec![
            pool(10, sol, usdc, 1_000_000_000, 100_000_000_000, 30),
            pool(11, sol, usdc, 1_000_000_000, 100_000_000_000, 30),
        ];
        let router = Router::new(
            pools,
            RouterConfig {
                max_hops: 2,
                start_mints: vec![sol],
                intermediate_mints: vec![usdc],
                ..RouterConfig::default()
            },
        );
        assert!(router.opportunities().is_empty());
    }

    #[test]
    fn ignores_external_pools() {
        let sol = address(1);
        let usdc = address(2);
        let mut external = pool(12, sol, usdc, 0, 0, 0);
        external.model = PoolModel::External {
            note: "prop amm".into(),
        };
        let router = Router::new(
            vec![external],
            RouterConfig {
                start_mints: vec![sol],
                ..RouterConfig::default()
            },
        );
        assert_eq!(router.pool_count(), 1);
        assert!(router.opportunities().is_empty());
    }
}
