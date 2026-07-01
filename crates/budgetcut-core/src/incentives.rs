//! Production-incentive estimation (MMB "estimate incentives"). A rebate is a
//! fraction of *qualifying* spend, optionally capped. Pure, I/O-free.
//!
//! Ships illustrative Turkish presets (the T.C. yapım destekleri / cash-rebate
//! scheme). Rates/caps change by regulation and project — treat presets as a
//! starting point and override per project.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// An incentive program: a rebate `rate` on qualifying spend, with an optional
/// `cap` on the rebate amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incentive {
    pub jurisdiction: String,
    /// Rebate fraction (0.30 == 30%).
    pub rate: Decimal,
    /// Maximum rebate amount, if the program is capped.
    pub cap: Option<Decimal>,
}

/// Estimate the rebate for a given qualifying spend.
#[must_use]
pub fn estimate(qualifying_spend: Decimal, rate: Decimal, cap: Option<Decimal>) -> Decimal {
    let raw = (qualifying_spend.max(Decimal::ZERO)) * rate;
    match cap {
        Some(c) if raw > c => c,
        _ => raw,
    }
}

impl Incentive {
    #[must_use]
    pub fn estimate(&self, qualifying_spend: Decimal) -> Decimal {
        estimate(qualifying_spend, self.rate, self.cap)
    }
}

/// Illustrative Turkish production-incentive presets.
#[must_use]
pub fn turkish_presets() -> Vec<Incentive> {
    use rust_decimal_macros::dec;
    vec![
        Incentive {
            jurisdiction: "T.C. Yapım Desteği (nakit iade)".into(),
            rate: dec!(0.30),
            cap: None,
        },
        Incentive {
            jurisdiction: "Bölgesel teşvik (örnek)".into(),
            rate: dec!(0.20),
            cap: Some(dec!(5000000)),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn rebate_and_cap() {
        // 30% of ₺10.000.000 qualifying spend, no cap.
        assert_eq!(estimate(dec!(10000000), dec!(0.30), None), dec!(3000000));
        // 20% of ₺40.000.000 = 8M, capped at 5M.
        assert_eq!(
            estimate(dec!(40000000), dec!(0.20), Some(dec!(5000000))),
            dec!(5000000)
        );
        // negative spend can't yield a rebate
        assert_eq!(estimate(dec!(-100), dec!(0.30), None), dec!(0));
    }

    #[test]
    fn presets_apply() {
        let p = &turkish_presets()[0];
        assert_eq!(p.estimate(dec!(2000000)), dec!(600000));
    }
}
