//! Amort & pattern budgets (§5 Versions/Phases; MMB "amort and pattern
//! budgets"). Episodic TV is budgeted as a **pattern** episode repeated across
//! the season, plus **amortized** season-wide costs (a set build, a series-long
//! deal, the pilot) spread evenly over a number of episodes. Pure, I/O-free.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A sane upper bound on episode counts — a multiplier, so left unbounded it
/// lets attacker-chosen values drive `Decimal` arithmetic toward overflow. No
/// real series order approaches this.
pub const MAX_EPISODES: u32 = 10_000;

/// Saturating money arithmetic, matching the calc engine's no-panic guarantee
/// (totals must never panic on overflow — `evaluate` is total). `rust_decimal`'s
/// `*`/`+` panic on overflow, so these wrappers are mandatory on any path fed by
/// client-controlled multipliers.
fn sat_mul(a: Decimal, b: Decimal) -> Decimal {
    a.checked_mul(b).unwrap_or(Decimal::MAX)
}
fn sat_add(a: Decimal, b: Decimal) -> Decimal {
    a.checked_add(b).unwrap_or(Decimal::MAX)
}

/// A season-wide cost amortized over `over_episodes` episodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmortItem {
    pub label: String,
    /// Total cost to spread.
    pub total: Decimal,
    /// Number of episodes to amortize across (clamped to ≥ 1).
    pub over_episodes: u32,
}

impl AmortItem {
    /// Per-episode share (`total / over_episodes`).
    #[must_use]
    pub fn per_episode(&self) -> Decimal {
        let n = Decimal::from(self.over_episodes.max(1));
        self.total / n
    }

    /// What *this* season bears: the per-episode share for `episodes` episodes
    /// (`total × episodes / over_episodes`). When a cost is amortized over more
    /// episodes than the current order (a multi-season set built once), this
    /// season carries only its slice; when `over_episodes == episodes` it bears
    /// the whole `total`. Multiply-before-divide minimises rounding drift.
    #[must_use]
    pub fn season_share(&self, episodes: u32) -> Decimal {
        let over = Decimal::from(self.over_episodes.max(1));
        sat_mul(self.total, Decimal::from(episodes.max(1))) / over
    }
}

/// Rolled-up series figures from a pattern episode + amortized items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesSummary {
    pub episodes: u32,
    /// Net of one pattern episode (the repeated base).
    pub pattern_episode: Decimal,
    /// `pattern_episode × episodes`.
    pub pattern_total: Decimal,
    /// Sum of all amortized item totals (season-wide).
    pub amort_total: Decimal,
    /// `pattern_total + amort_total` — the whole-season direct cost.
    pub series_total: Decimal,
    /// All-in cost charged to each episode (`series_total / episodes`).
    pub per_episode_all_in: Decimal,
}

/// Compute a series budget: a `pattern_episode_net` repeated across `episodes`,
/// plus `amortized` season-wide costs (each spread over its own window).
///
/// `episodes` is clamped to `1..=MAX_EPISODES` and all money arithmetic
/// saturates, so this is **total** (never panics) even for hostile inputs.
#[must_use]
pub fn series_budget(
    pattern_episode_net: Decimal,
    episodes: u32,
    amortized: &[AmortItem],
) -> SeriesSummary {
    let eps = episodes.clamp(1, MAX_EPISODES);
    let n = Decimal::from(eps);
    let pattern_total = sat_mul(pattern_episode_net, n);
    let amort_total = amortized
        .iter()
        .fold(Decimal::ZERO, |acc, a| sat_add(acc, a.season_share(eps)));
    let series_total = sat_add(pattern_total, amort_total);
    SeriesSummary {
        episodes: eps,
        pattern_episode: pattern_episode_net,
        pattern_total,
        amort_total,
        series_total,
        per_episode_all_in: series_total / n, // n ≥ 1, cannot overflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::round_money;
    use rust_decimal_macros::dec;

    #[test]
    fn amort_item_per_episode() {
        let pilot = AmortItem {
            label: "Pilot fazlası".into(),
            total: dec!(1200000),
            over_episodes: 8,
        };
        assert_eq!(pilot.per_episode(), dec!(150000));
    }

    #[test]
    fn series_total_pattern_plus_amort() {
        // 8-episode dizi: each episode patterns at ₺2.500.000; a ₺1.200.000
        // season-wide set build amortized across all 8.
        let amort = vec![AmortItem {
            label: "Ana dekor inşası".into(),
            total: dec!(1200000),
            over_episodes: 8,
        }];
        let s = series_budget(dec!(2500000), 8, &amort);
        assert_eq!(s.pattern_total, dec!(20000000)); // 2.5M × 8
        assert_eq!(s.amort_total, dec!(1200000));
        assert_eq!(s.series_total, dec!(21200000));
        assert_eq!(round_money(s.per_episode_all_in), dec!(2650000.00)); // 21.2M / 8
    }

    #[test]
    fn zero_episodes_is_clamped_not_a_panic() {
        let s = series_budget(dec!(100000), 0, &[]);
        assert_eq!(s.episodes, 1);
        assert_eq!(s.series_total, dec!(100000));
    }

    #[test]
    fn over_episodes_window_is_honored() {
        // A ₺1.200.000 cost amortized over 24 episodes, but this season is 8 →
        // this season bears 1.200.000 × 8 / 24 = 400.000 (not the whole cost).
        let amort = vec![AmortItem {
            label: "Çok sezonluk dekor".into(),
            total: dec!(1200000),
            over_episodes: 24,
        }];
        let s = series_budget(Decimal::ZERO, 8, &amort);
        assert_eq!(s.amort_total, dec!(400000));
        assert_eq!(s.series_total, dec!(400000));
    }

    #[test]
    fn saturates_instead_of_panicking_on_overflow() {
        // Hostile inputs (huge net, max episodes) must NOT panic — they saturate.
        let s = series_budget(Decimal::MAX, MAX_EPISODES, &[]);
        assert_eq!(s.pattern_total, Decimal::MAX);
        assert_eq!(s.series_total, Decimal::MAX);
        // Episode count is clamped to the domain max.
        let s2 = series_budget(dec!(1), u32::MAX, &[]);
        assert_eq!(s2.episodes, MAX_EPISODES);
    }
}
