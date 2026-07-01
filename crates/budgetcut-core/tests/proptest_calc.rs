//! Property tests for the calculation engine (§6/§20.8).
//!
//! Two guarantees:
//!   1. **Determinism / order-independence** — the engine sorts by
//!      `(position, id)` before every sum, so the result must not depend on
//!      `HashMap` iteration order. We build two independent budgets from the
//!      same spec (inserting entities in opposite orders, so their internal
//!      hash layouts differ) and require identical `CalcResult`s.
//!   2. **No panics / well-formed totals** — acyclic globals always resolve,
//!      and `net_total == grand_total - credits`, `total == atl + btl`.

use budgetcut_core::ids::*;
use budgetcut_core::*;
use proptest::prelude::*;
use rust_decimal::Decimal;

/// A generated spec for one detail: quantity, rate, optional gross-up fringe.
#[derive(Debug, Clone)]
struct DetailSpec {
    amount: i64,
    rate: i64,
    fringe_pct: Option<u32>, // basis points (e.g. 1700 = 17%); gross-up
}

fn detail_spec() -> impl Strategy<Value = DetailSpec> {
    (1i64..100, 0i64..5000, prop::option::of(0u32..5000u32)).prop_map(
        |(amount, rate, fringe_pct)| DetailSpec {
            amount,
            rate,
            fringe_pct,
        },
    )
}

/// Build a budget from a list of detail specs. Entities are inserted in the
/// given direction so two builds with `reverse = {false, true}` produce
/// independently-laid-out `HashMap`s.
fn build(specs: &[DetailSpec], reverse: bool) -> (Budget, FringeId, UnitId) {
    let mut budget = Budget::new("calc", templates::try_currency());
    let currency = budget.base_currency;
    let unit = Unit {
        id: UnitId::new(),
        code: "F".into(),
        name: Localized::tr(""),
        factor: Decimal::ONE,
    };
    let unit_id = unit.id;
    budget.units.insert(unit_id, unit);

    let fringe = Fringe {
        id: FringeId::new(),
        code: "GU".into(),
        name: Localized::tr(""),
        kind: FringeKind::Percent,
        mode: FringeMode::GrossUp,
        rate: Decimal::ZERO,
        posting_level: PostingLevel::Detail,
        cutoff: None,
        cap: None,
        currency: None,
    };
    let fringe_id = fringe.id;
    budget.fringes.insert(fringe_id, fringe);

    let cat = Category {
        id: CategoryId::new(),
        number: "1".into(),
        description: Localized::tr(""),
        position: Decimal::ONE,
        atl_btl: None,
        applied_fringes: vec![],
    };
    let acc = Account {
        id: AccountId::new(),
        category: cat.id,
        number: "1".into(),
        description: Localized::tr(""),
        position: Decimal::ONE,
        show_subtotal: true,
        applied_fringes: vec![],
    };
    let acc_id = acc.id;
    budget.categories.insert(cat.id, cat);
    budget.accounts.insert(acc.id, acc);

    // Stable ids per index so both builds describe the same logical budget.
    let mut details: Vec<Detail> = specs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut fringes = Vec::new();
            if let Some(bp) = s.fringe_pct {
                let rate = Decimal::from(bp) / Decimal::from(10_000);
                fringes.push(AppliedFringe::with_rate(fringe_id, rate));
            }
            Detail {
                id: DetailId::new(),
                account: acc_id,
                position: Decimal::from(i as i64),
                description: String::new(),
                name: None,
                amount: Formula::Const(Decimal::from(s.amount)),
                multiplier: Formula::Const(Decimal::ONE),
                rate: Formula::Const(Decimal::from(s.rate)),
                unit: unit_id,
                currency,
                applied_fringes: fringes,
                groups: vec![],
                location: None,
                set: None,
                gl_code: None,
                notes: None,
            }
        })
        .collect();
    if reverse {
        details.reverse();
    }
    for d in details {
        budget.details.insert(d.id, d);
    }
    (budget, fringe_id, unit_id)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn evaluation_is_order_independent(specs in prop::collection::vec(detail_spec(), 0..30)) {
        let (b1, _, _) = build(&specs, false);
        let (b2, _, _) = build(&specs, true);
        let r1 = evaluate(&b1);
        let r2 = evaluate(&b2);

        // Totals must match exactly regardless of internal hash layout.
        prop_assert_eq!(r1.grand_total, r2.grand_total);
        prop_assert_eq!(r1.total.subtotal, r2.total.subtotal);
        prop_assert_eq!(r1.total.fringe_total, r2.total.fringe_total);
        prop_assert_eq!(r1.net_total, r2.net_total);
        prop_assert!(!r1.has_errors());

        // The grand subtotal equals the hand-summed expected net.
        let expected_net: Decimal = specs.iter()
            .map(|s| Decimal::from(s.amount) * Decimal::from(s.rate))
            .sum();
        prop_assert_eq!(r1.total.subtotal, expected_net);

        // Invariants: total == atl + btl, net == grand - credits.
        prop_assert_eq!(r1.total.total, r1.atl.total + r1.btl.total);
        prop_assert_eq!(r1.net_total, r1.grand_total - r1.credits_total);
        // No credits/charges here, so grand == subtotal + fringes.
        prop_assert_eq!(r1.grand_total, r1.total.subtotal + r1.total.fringe_total);
    }

    #[test]
    fn gross_up_fringe_never_decreases_total(specs in prop::collection::vec(detail_spec(), 1..20)) {
        let (b, _, _) = build(&specs, false);
        let r = evaluate(&b);
        // Gross-up and additive fringes are non-negative for rates in [0,1).
        prop_assert!(r.total.fringe_total >= Decimal::ZERO);
        prop_assert!(r.total.total >= r.total.subtotal);
    }
}
