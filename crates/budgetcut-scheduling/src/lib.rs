//! Production scheduling (§16 Phase 2): a stripboard and its Day-Out-of-Days.
//!
//! This is the **budget ↔ schedule seam** the spec calls for: scheduling
//! determines how many days each element (cast member, vehicle, set, …) works,
//! and [`element_work_days`] hands those counts back so a budget detail's
//! quantity ("ADET") can be driven by the schedule rather than typed by hand.
//! Pure and I/O-free, like the rest of the engine.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// When a scene shoots (affects lighting/turnaround; carried for reports).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Day,
    Night,
    Dawn,
    Dusk,
}

/// One stripboard strip — a scene with its set, length (page eighths) and the
/// elements that appear in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strip {
    pub scene: String,
    pub set: String,
    pub time: TimeOfDay,
    /// Page length in eighths (8 = one page).
    pub eighths: u32,
    /// Element keys appearing in the scene (e.g. character names, vehicles).
    pub elements: Vec<String>,
}

/// A shooting day: the ordered strips scheduled that day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShootDay {
    pub strips: Vec<Strip>,
}

/// The schedule: an ordered list of shooting days (the stripboard arrangement).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub days: Vec<ShootDay>,
}

/// A Day-Out-of-Days row for one element: when it starts/finishes and how many
/// days it works vs. is on hold (idle between first and last day).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoodRow {
    pub start_day: usize,
    pub finish_day: usize,
    pub work_days: usize,
    /// Idle days inside the span (`span − work`), i.e. paid-but-not-shooting.
    pub hold_days: usize,
}

impl Schedule {
    #[must_use]
    pub fn total_days(&self) -> usize {
        self.days.len()
    }

    /// Total page eighths across the schedule (a common pacing metric).
    #[must_use]
    pub fn total_eighths(&self) -> u32 {
        self.days
            .iter()
            .flat_map(|d| d.strips.iter())
            .map(|s| s.eighths)
            .sum()
    }

    /// Day-Out-of-Days for every element, keyed by element (deterministic order).
    #[must_use]
    pub fn day_out_of_days(&self) -> BTreeMap<String, DoodRow> {
        // Collect 1-based day numbers each element appears on.
        let mut appears: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, day) in self.days.iter().enumerate() {
            let day_no = i + 1;
            let mut seen_today = std::collections::BTreeSet::new();
            for strip in &day.strips {
                for el in &strip.elements {
                    if seen_today.insert(el.clone()) {
                        appears.entry(el.clone()).or_default().push(day_no);
                    }
                }
            }
        }
        appears
            .into_iter()
            .map(|(el, days)| {
                let start = *days.first().unwrap();
                let finish = *days.last().unwrap();
                let work = days.len();
                let span = finish - start + 1;
                (
                    el,
                    DoodRow {
                        start_day: start,
                        finish_day: finish,
                        work_days: work,
                        hold_days: span - work,
                    },
                )
            })
            .collect()
    }

    /// The budget seam: element → number of **work days**. Feed these into the
    /// matching budget detail quantities so the schedule drives the budget.
    #[must_use]
    pub fn element_work_days(&self) -> BTreeMap<String, usize> {
        self.day_out_of_days()
            .into_iter()
            .map(|(el, row)| (el, row.work_days))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(scene: &str, els: &[&str]) -> Strip {
        Strip {
            scene: scene.into(),
            set: "İÇ. EV".into(),
            time: TimeOfDay::Day,
            eighths: 8,
            elements: els.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn schedule() -> Schedule {
        // 3 shoot days. ANA KARAKTER works days 1 & 3 (hold on day 2).
        Schedule {
            days: vec![
                ShootDay {
                    strips: vec![strip("1", &["ANA KARAKTER", "YARDIMCI"])],
                },
                ShootDay {
                    strips: vec![strip("2", &["YARDIMCI"])],
                },
                ShootDay {
                    strips: vec![
                        strip("3", &["ANA KARAKTER"]),
                        strip("3B", &["ANA KARAKTER"]),
                    ],
                },
            ],
        }
    }

    #[test]
    fn dood_computes_work_and_hold() {
        let s = schedule();
        assert_eq!(s.total_days(), 3);
        assert_eq!(s.total_eighths(), 32); // 4 strips × 8

        let dood = s.day_out_of_days();
        let lead = dood["ANA KARAKTER"];
        assert_eq!(lead.start_day, 1);
        assert_eq!(lead.finish_day, 3);
        assert_eq!(lead.work_days, 2); // days 1 and 3 (not double-counted on day 3)
        assert_eq!(lead.hold_days, 1); // day 2 idle within the span

        let support = dood["YARDIMCI"];
        assert_eq!(support.work_days, 2); // days 1 and 2
        assert_eq!(support.hold_days, 0);
    }

    #[test]
    fn element_work_days_feeds_budget_quantities() {
        // The seam: a cast member budgeted at their scheduled work-day count.
        let days = schedule().element_work_days();
        assert_eq!(days["ANA KARAKTER"], 2);
        assert_eq!(days["YARDIMCI"], 2);
        // e.g. budget detail amount for "ANA KARAKTER" = 2 days × daily rate.
    }
}
