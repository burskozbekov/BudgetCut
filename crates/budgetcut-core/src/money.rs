//! Money is fixed-point decimal, never `f64` (§18).
//!
//! All amounts are [`rust_decimal::Decimal`]. We standardise on **2 decimal
//! places (kuruş / cents)** for display and stored totals, while keeping full
//! precision through intermediate calculations so chained operations (e.g. a
//! gross-up division followed by a rollup sum) don't accumulate rounding error.

use rust_decimal::{Decimal, RoundingStrategy};

/// The money scalar used everywhere in the engine.
pub type Money = Decimal;

/// Standard money precision: 2 decimal places (kuruş).
pub const MONEY_DP: u32 = 2;

/// Round a money value to kuruş using banker's-neutral *half-up* rounding,
/// matching how Turkish production accounting rounds invoice lines.
#[must_use]
pub fn round_money(value: Money) -> Money {
    value.round_dp_with_strategy(MONEY_DP, RoundingStrategy::MidpointAwayFromZero)
}

/// Round to an arbitrary number of decimal places (half-up).
#[must_use]
pub fn round_dp(value: Money, dp: u32) -> Money {
    value.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero)
}

/// Zero, spelled out for readability at call sites.
#[must_use]
pub fn zero() -> Money {
    Decimal::ZERO
}

/// Build a money value from an integer.
#[must_use]
pub fn from_int(i: i64) -> Money {
    Decimal::from(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn rounds_half_up_to_kurus() {
        assert_eq!(round_money(dec!(135180.72289156634)), dec!(135180.72));
        assert_eq!(round_money(dec!(2614070.1686746988)), dec!(2614070.17));
        assert_eq!(round_money(dec!(0.005)), dec!(0.01));
    }

    #[test]
    fn full_precision_division_is_deterministic() {
        // The stopaj gross-up: net / (1 - rate). Decimal keeps ~28 sig digits,
        // so two evaluations are bit-identical (no f64 nondeterminism, §6).
        let net = dec!(660000);
        let rate = dec!(0.17);
        let a = net / (Decimal::ONE - rate);
        let b = net / (Decimal::ONE - rate);
        assert_eq!(a, b);
        assert_eq!(round_money(a - net), dec!(135180.72));
    }
}
