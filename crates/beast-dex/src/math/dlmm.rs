//! Meteora DLMM price helpers. Quotes need bin arrays and are deliberately
//! delegated to Jupiter/simulation until the full bin walker is enabled.

pub fn price_from_id(active_id: i32, bin_step: u16) -> f64 {
    let step = bin_step as f64 / 10_000.0;
    (1.0 + step).powf(active_id as f64)
}

/// Convert a UI price (token Y per token X) into an approximate active bin id.
pub fn id_from_price(price: f64, bin_step: u16) -> Option<i32> {
    if price <= 0.0 || bin_step == 0 || !price.is_finite() {
        return None;
    }
    let step = 1.0 + bin_step as f64 / 10_000.0;
    Some((price.ln() / step.ln()).round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_is_monotonic_and_roundtrips() {
        let p = price_from_id(100, 25);
        assert!(p > 1.0);
        let id = id_from_price(p, 25).unwrap();
        assert_eq!(id, 100);
    }

    #[test]
    fn zero_bin_step_is_rejected() {
        assert!(id_from_price(1.0, 0).is_none());
    }
}
