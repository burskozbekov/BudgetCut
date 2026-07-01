//! Production scheduling (§16 Phase 2): a flat, op-loggable stripboard and its
//! Day-Out-of-Days. Pure, I/O-free.
//!
//! The standalone `budgetcut-scheduling` crate models a schedule as nested
//! ordered days (good for the pure algorithm); this in-core model is **flat and
//! keyed** ([`Strip`] has its own id + a `day` number) so each strip is a
//! first-class op-logged Budget entity that syncs and merges like any other.
//! [`day_out_of_days`] is the budget↔schedule seam: per-element working-day
//! counts can drive a budget detail's quantity.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One stripboard strip — a scene scheduled on a shooting day, with the
/// elements (cast/vehicles/sets) that appear in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strip {
    pub id: uuid::Uuid,
    /// 1-based shooting day number.
    pub day: u32,
    pub scene: String,
    #[serde(default)]
    pub set: String,
    /// Page length in eighths (8 = one page).
    #[serde(default)]
    pub eighths: u32,
    /// Element keys appearing in the scene (e.g. character names).
    #[serde(default)]
    pub elements: Vec<String>,
}

/// A Day-Out-of-Days row for one element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoodRow {
    pub element: String,
    pub start_day: u32,
    pub finish_day: u32,
    /// Days the element actually works.
    pub work_days: u32,
    /// Idle days inside the span (`span − work`) — paid-but-not-shooting.
    pub hold_days: u32,
}

/// Distinct shooting days across the strips.
#[must_use]
pub fn total_days(strips: &[Strip]) -> u32 {
    strips.iter().map(|s| s.day).collect::<BTreeSet<_>>().len() as u32
}

/// Total page eighths across the strips (saturating — never panics/wraps on
/// absurd inputs).
#[must_use]
pub fn total_eighths(strips: &[Strip]) -> u32 {
    strips
        .iter()
        .map(|s| s.eighths)
        .fold(0u32, u32::saturating_add)
}

/// Day-Out-of-Days for every element (deterministic order).
#[must_use]
pub fn day_out_of_days(strips: &[Strip]) -> Vec<DoodRow> {
    let mut by_el: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for s in strips {
        for e in &s.elements {
            by_el.entry(e.clone()).or_default().insert(s.day);
        }
    }
    by_el
        .into_iter()
        .map(|(element, days)| {
            let start = *days.iter().next().unwrap();
            let finish = *days.iter().next_back().unwrap();
            let work = days.len() as u32;
            // saturating: hostile day numbers (0 .. u32::MAX) must not overflow.
            let span = finish.saturating_sub(start).saturating_add(1);
            DoodRow {
                element,
                start_day: start,
                finish_day: finish,
                work_days: work,
                hold_days: span - work,
            }
        })
        .collect()
}

/// The budget↔schedule seam: element → number of **work days**. Feed these into
/// the matching budget detail quantity so the schedule drives the budget.
#[must_use]
pub fn element_work_days(strips: &[Strip]) -> BTreeMap<String, u32> {
    day_out_of_days(strips)
        .into_iter()
        .map(|r| (r.element, r.work_days))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(day: u32, els: &[&str]) -> Strip {
        Strip {
            id: uuid::Uuid::now_v7(),
            day,
            scene: format!("S{day}"),
            set: "İÇ. EV".into(),
            eighths: 8,
            elements: els.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn dood_computes_work_and_hold() {
        // ANA KARAKTER on days 1 & 3 (hold on day 2); YARDIMCI on 1 & 2.
        let strips = vec![
            strip(1, &["ANA KARAKTER", "YARDIMCI"]),
            strip(2, &["YARDIMCI"]),
            strip(3, &["ANA KARAKTER"]),
            strip(3, &["ANA KARAKTER"]), // same day, not double-counted
        ];
        assert_eq!(total_days(&strips), 3);
        assert_eq!(total_eighths(&strips), 32);

        let dood = day_out_of_days(&strips);
        let lead = dood.iter().find(|r| r.element == "ANA KARAKTER").unwrap();
        assert_eq!(
            (
                lead.start_day,
                lead.finish_day,
                lead.work_days,
                lead.hold_days
            ),
            (1, 3, 2, 1)
        );
        let sup = dood.iter().find(|r| r.element == "YARDIMCI").unwrap();
        assert_eq!((sup.work_days, sup.hold_days), (2, 0));

        let wd = element_work_days(&strips);
        assert_eq!(wd["ANA KARAKTER"], 2);
    }

    #[test]
    fn extreme_day_numbers_do_not_overflow() {
        // An element spanning day 0 .. u32::MAX once overflowed `finish-start+1`.
        let strips = vec![
            strip(0, &["X"]),
            strip(u32::MAX, &["X"]),
            Strip {
                id: uuid::Uuid::now_v7(),
                day: 5,
                scene: "big".into(),
                set: String::new(),
                eighths: u32::MAX,
                elements: vec![],
            },
        ];
        let dood = day_out_of_days(&strips); // must not panic
        let x = dood.iter().find(|r| r.element == "X").unwrap();
        assert_eq!(x.work_days, 2);
        assert!(x.hold_days >= 1); // saturated span − work
        assert_eq!(total_eighths(&strips), u32::MAX); // saturated, no panic
    }
}
