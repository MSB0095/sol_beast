//! Optimal trade sizing for every arbitrage route the executor supports.
//!
//! A fixed amount ladder is the wrong shape for this problem. Profit as a
//! function of borrow size is zero at dust (costs dominate), rises while the
//! price edge pays for the impact, peaks, and then falls as the legs push the
//! pools further against the trader. Picking from a handful of sizes either
//! overshoots the peak and reverts the cycle or undershoots it and leaves
//! profit behind.
//!
//! Every route is priced through the caller's exact quote function, so the
//! search sees the numbers the programs will use. For two constant-product
//! pools the optimum has a closed form, which the tests use as an oracle; the
//! implementation intentionally does *not* assume that form. AMM v4, CPMM,
//! Whirlpool and CLMM have different fee, rounding and tick-crossing rules,
//! so the generic curve is optimized rather than special-cased and functions
//! for all venues remain valid inputs to the same solver.
//!
//! Nothing in this module reads the chain. It is pure arithmetic over reserves
//! the caller already fetched, so it can be unit-tested against the closed form.

/// The best input size found, with the profit it produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptimalSize {
    pub amount: u64,
    pub profit: i128,
}

/// Maximise `profit(x)` over `1..=upper`.
///
/// `profit` must be the caller's *exact* profit function, including fees, the
/// flash fee and all costs. It is allowed to be negative everywhere; the
/// returned maximum is then simply the least-bad size, and the caller decides
/// whether that is worth executing.
///
/// The search is deliberately three-stage rather than a plain ternary search:
/// integer flooring in the fee and output rounding makes the curve step rather
/// than curve, and a global ternary search can settle on a step boundary. A
/// coarse geometric sweep finds the right basin, the ternary search narrows it,
/// and the local scan removes the last lamport of error.
pub fn maximize_profit(
    upper: u64,
    mut profit: impl FnMut(u64) -> i128,
) -> Option<OptimalSize> {
    if upper == 0 {
        return None;
    }

    // Stage 1: geometric sweep. Log spacing is right because the useful range
    // spans orders of magnitude: a profitable size can be 1e6 or 1e12 lamports
    // and a linear grid would spend every sample in one end.
    //
    // The sweep is only a basin finder. Profit is concave in size (the marginal
    // price of both legs rises with size, so the marginal profit falls), which
    // means the peak is unique; the sweep exists to bracket it cheaply, not to
    // locate it. 48 samples bracket a 1e12 range to within a factor of ~3,
    // which the ternary stage then closes to a single lamport. Fewer samples
    // mean fewer exact quotes, and an exact quote can cross dozens of
    // concentrated-liquidity ticks.
    const SWEEP: u32 = 48;
    let mut samples: Vec<(u64, i128)> = Vec::with_capacity(SWEEP as usize + 1);
    for index in 0..=SWEEP {
        let fraction = f64::from(index) / f64::from(SWEEP);
        let amount = if index == 0 {
            1
        } else {
            let value = (upper as f64).powf(fraction);
            value.round().clamp(1.0, upper as f64) as u64
        };
        samples.push((amount, profit(amount)));
    }

    // Ties resolve to the smallest size: the same profit with less capital at
    // risk and less price impact is strictly better, and `max_by_key` would
    // silently choose the largest instead.
    let mut best_index = 0usize;
    let mut best_profit = samples[0].1;
    for (index, (_, value)) in samples.iter().enumerate().skip(1) {
        if *value > best_profit {
            best_index = index;
            best_profit = *value;
        }
    }
    let best_amount = samples[best_index].0;

    // Stage 2: ternary search inside the bracket the sweep found.
    let lo = if best_index == 0 {
        1
    } else {
        samples[best_index - 1].0
    };
    let hi = samples
        .get(best_index + 1)
        .map(|(amount, _)| *amount)
        .unwrap_or(upper);

    let (mut best_amount, mut best_profit) = refine(lo, hi, best_amount, best_profit, &mut profit);

    // Stage 3: settle the last few lamports either side of the result.
    //
    // The window is proportional because a fixed one is meaningless across ten
    // orders of magnitude, and it is capped because an uncapped one costs
    // O(amount) quotes — a billion of them for a 1e12-lamport cycle. A coarse
    // pass steps across the window, then a fine pass steps by one lamport
    // across the best coarse step. That covers a wide neighbourhood at constant
    // cost instead of trading coverage for accuracy.
    let window = (best_amount / 2_000).clamp(16, 65_536);
    let scan_lo = best_amount.saturating_sub(window).max(1);
    let scan_hi = best_amount.saturating_add(window).min(upper);
    let step = (window / 128).max(1);

    let mut coarse_best = best_amount;
    let mut amount = scan_lo;
    while amount <= scan_hi {
        let value = profit(amount);
        if value > best_profit {
            coarse_best = amount;
            best_profit = value;
        }
        amount = amount.saturating_add(step);
    }

    let fine_lo = coarse_best.saturating_sub(step).max(scan_lo);
    let fine_hi = coarse_best.saturating_add(step).min(scan_hi);
    for amount in fine_lo..=fine_hi {
        let value = profit(amount);
        if value > best_profit {
            best_amount = amount;
            best_profit = value;
        }
    }

    Some(OptimalSize {
        amount: best_amount,
        profit: best_profit,
    })
}

/// Find the largest size whose exact profit remains at least `min_profit`.
///
/// Profit for these routes is increasing up to one peak and then decreases as
/// the pools push price further against the trader. This first locates the
/// unimodal peak with the exact quote function, then binary-searches only
/// [peak, upper] for the last size that clears the threshold. If the function
/// is not unimodal, the same search still returns the largest evaluated size
/// that clears `min_profit`; it never invents an edge above the observed one.
pub fn maximize_amount(
    upper: u64,
    mut profit: impl FnMut(u64) -> i128,
    min_profit: i128,
) -> Option<OptimalSize> {
    let peak = maximize_profit(upper, |amount| profit(amount))?;
    if peak.profit < min_profit {
        return None;
    }

    let upper = upper.max(peak.amount);
    let mut best_amount = peak.amount;
    let mut best_profit = peak.profit;
    let mut lo = peak.amount;
    let mut hi = upper;

    while lo < hi {
        let mid = lo.saturating_add(hi.saturating_sub(lo).saturating_add(1) / 2);
        let value = profit(mid);
        if value >= min_profit {
            if mid > best_amount {
                best_amount = mid;
                best_profit = value;
            }
            lo = mid;
        } else {
            hi = mid.saturating_sub(1);
        }
    }

    // Rounding and tick boundaries can make a tiny, legitimate edge sit in the
    // threshold gap. Check the narrow region around the crossing rather than
    // assume a perfectly smooth integer curve.
    let window = best_amount.saturating_div(100_000).max(4);
    let lo_window = best_amount.saturating_sub(window);
    let hi_window = best_amount.saturating_add(window).min(upper);
    for amount in lo_window..=hi_window {
        let value = profit(amount);
        if value >= min_profit && amount > best_amount {
            best_amount = amount;
            best_profit = value;
        }
    }

    Some(OptimalSize {
        amount: best_amount,
        profit: best_profit,
    })
}

/// Ternary search over an integer bracket, seeded with the sweep's best point.
fn refine(
    mut lo: u64,
    mut hi: u64,
    mut best_amount: u64,
    mut best_profit: i128,
    profit: &mut impl FnMut(u64) -> i128,
) -> (u64, i128) {
    while hi.saturating_sub(lo) > 3 {
        let third = (hi - lo) / 3;
        let m1 = lo + third;
        let m2 = hi - third;
        let p1 = profit(m1);
        let p2 = profit(m2);
        if p1 > best_profit {
            best_amount = m1;
            best_profit = p1;
        }
        if p2 > best_profit {
            best_amount = m2;
            best_profit = p2;
        }
        if p1 < p2 {
            lo = m1 + 1;
        } else {
            hi = m2.saturating_sub(1);
        }
    }
    for amount in lo..=hi.max(lo) {
        let value = profit(amount);
        if value > best_profit {
            best_amount = amount;
            best_profit = value;
        }
    }
    (best_amount, best_profit)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two constant-product pools with a multiplicative fee, as a reference.
    fn two_pool_profit(x: u64, a1: u64, b1: u64, a2: u64, b2: u64, fee: f64) -> i128 {
        let x = x as f64;
        let g = 1.0 - fee;
        let y = (x * g * b1 as f64) / (a1 as f64 + x * g);
        let z = (y * g * a2 as f64) / (b2 as f64 + y * g);
        (z - x) as i128
    }

    /// Closed form from the module docs, used as an independent oracle.
    fn closed_form(a1: u64, b1: u64, a2: u64, b2: u64, fee: f64) -> u64 {
        let g = 1.0 - fee;
        let (a1, b1, a2, b2) = (a1 as f64, b1 as f64, a2 as f64, b2 as f64);
        let u = ((g * g * a1 * a2 * b1 * b2).sqrt() - a1 * b2) / (b2 + g * b1);
        (u / g).max(1.0) as u64
    }

    #[test]
    fn finds_the_closed_form_optimum() {
        // A 1% dislocation. Buying the intermediate in pool 1 pays more per
        // input than selling it back in pool 2, which is the direction that
        // can profit; the reverse direction always loses the two fees.
        let (a1, b1) = (1_000_000_000_000u64, 10_100_000_000_000u64);
        let (a2, b2) = (1_000_000_000_000u64, 10_000_000_000_000u64);
        let fee = 0.0025;
        let found = maximize_profit(500_000_000_000, |x| {
            two_pool_profit(x, a1, b1, a2, b2, fee)
        })
        .unwrap();
        let expected = closed_form(a1, b1, a2, b2, fee);
        assert!(found.profit > 0, "expected a profitable cycle");
        // The exact optimum and the closed form should agree to well under 1%.
        let drift = (found.amount as f64 - expected as f64).abs() / expected as f64;
        assert!(drift < 0.01, "amount {} vs closed form {expected}", found.amount);
    }

    #[test]
    fn never_exceeds_the_upper_bound() {
        let found = maximize_profit(1_000, |x| x as i128).unwrap();
        assert!(found.amount <= 1_000);
        assert_eq!(found.profit, 1_000);
    }

    #[test]
    fn reports_the_least_bad_size_when_nothing_pays() {
        // Every size loses the same fixed amount; the search must still return a
        // size rather than pretending no answer exists.
        let found = maximize_profit(1_000_000, |_| -50_000).unwrap();
        assert_eq!(found.profit, -50_000);
        assert!(found.amount >= 1 && found.amount <= 1_000_000);
    }

    #[test]
    fn handles_zero_upper_bound() {
        assert!(maximize_profit(0, |x| x as i128).is_none());
    }
}

/// Select the trade size objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeObjective {
    MaxProfit,
    MaxBorrowable,
}

/// Shared core solver for every route in the bot.
///
/// `profit` may in turn read any number of invariants (AMM, CPMM, concentrated
/// liquidity, Whirlpool, etc.); this function is deliberately ignorant of the
/// venue. It uses that function to price every candidate and returns the first
/// solution permitted by the chosen objective.
pub fn solve_size(
    upper: u64,
    profit: impl FnMut(u64) -> i128,
    objective: SizeObjective,
    min_profit: i128,
) -> Option<OptimalSize> {
    match objective {
        SizeObjective::MaxProfit => maximize_profit(upper, profit),
        SizeObjective::MaxBorrowable => maximize_amount(upper, profit, min_profit),
    }
}

#[cfg(test)]
mod max_amount_tests {
    use super::*;

    fn two_pool_profit(x: u64, a1: u64, b1: u64, a2: u64, b2: u64, fee: f64) -> i128 {
        let x = x as f64;
        let g = 1.0 - fee;
        let y = (x * g * b1 as f64) / (a1 as f64 + x * g);
        let z = (y * g * a2 as f64) / (b2 as f64 + y * g);
        (z - x) as i128
    }

    #[test]
    fn finds_the_rightmost_zero_crossing() {
        let (a1, b1) = (1_000_000_000_000u64, 10_100_000_000_000u64);
        let (a2, b2) = (1_000_000_000_000u64, 10_000_000_000_000u64);
        let fee = 0.0025;
        let upper = 500_000_000_000u64;
        let result = maximize_amount(upper, |x| two_pool_profit(x, a1, b1, a2, b2, fee), 0)
            .expect("one edge exists");
        let peak = maximize_profit(upper, |x| two_pool_profit(x, a1, b1, a2, b2, fee))
            .expect("peak exists");
        assert!(result.amount >= peak.amount);
        assert!(two_pool_profit(result.amount, a1, b1, a2, b2, fee) >= 0);
        if let Some(next) = result.amount.checked_add(1) {
            assert!(two_pool_profit(next, a1, b1, a2, b2, fee) < 0);
        }
    }

    #[test]
    fn respects_reserve_bound_and_threshold() {
        let result = maximize_amount(1_000, |_| -1, 0);
        assert!(result.is_none());
        let result = solve_size(1_000, |x| x as i128, SizeObjective::MaxBorrowable, 5_000);
        assert!(result.is_none());
        let result = solve_size(10_000, |x| x as i128, SizeObjective::MaxProfit, 0).unwrap();
        assert_eq!(result.amount, 10_000);
    }
}
