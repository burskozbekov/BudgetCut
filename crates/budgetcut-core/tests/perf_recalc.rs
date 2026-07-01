//! Performance smoke test (§6/§20.2): full recalc of a 5,000-line budget.
//!
//! The spec target is < 50 ms on a laptop in release. Test profiles aren't a
//! fair benchmark, so we assert a generous ceiling to catch algorithmic
//! blow-ups (e.g. accidental O(n²)) and print the real elapsed time. Run
//! `cargo test --release -p budgetcut-core --test perf_recalc -- --nocapture`
//! to see the true number.

use budgetcut_core::ids::*;
use budgetcut_core::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::time::Instant;

#[test]
fn recalc_5000_lines_is_fast_and_correct() {
    let mut budget = Budget::new("big", templates::try_currency());
    let currency = budget.base_currency;
    let unit = Unit {
        id: UnitId::new(),
        code: "F".into(),
        name: Localized::tr(""),
        factor: Decimal::ONE,
    };
    let unit_id = unit.id;
    budget.units.insert(unit_id, unit);

    // A global every detail references, to exercise the dependency graph.
    let g = Global {
        id: GlobalId::new(),
        name: "SHOOT_DAYS".into(),
        description: Localized::tr(""),
        value: Formula::Const(dec!(30)),
        in_budget_total: true,
    };
    budget.globals.insert(g.id, g);

    let fringe = Fringe {
        id: FringeId::new(),
        code: "GU".into(),
        name: Localized::tr(""),
        kind: FringeKind::Percent,
        mode: FringeMode::GrossUp,
        rate: dec!(0.17),
        posting_level: PostingLevel::Detail,
        cutoff: None,
        cap: None,
        currency: None,
    };
    let fringe_id = fringe.id;
    budget.fringes.insert(fringe_id, fringe);

    // 50 categories × 4 accounts × 25 details = 5,000 lines.
    let mut line_count = 0;
    for c in 0..50 {
        let cat = Category {
            id: CategoryId::new(),
            number: format!("{}", 1000 + c),
            description: Localized::tr(""),
            position: Decimal::from(c),
            atl_btl: None,
            applied_fringes: vec![],
        };
        let cat_id = cat.id;
        budget.categories.insert(cat_id, cat);
        for a in 0..4 {
            let acc = Account {
                id: AccountId::new(),
                category: cat_id,
                number: format!("{c}-{a}"),
                description: Localized::tr(""),
                position: Decimal::from(a),
                show_subtotal: true,
                applied_fringes: vec![],
            };
            let acc_id = acc.id;
            budget.accounts.insert(acc_id, acc);
            for d in 0..25 {
                let det = Detail {
                    id: DetailId::new(),
                    account: acc_id,
                    position: Decimal::from(d),
                    description: String::new(),
                    name: None,
                    amount: Formula::expr("SHOOT_DAYS"),
                    multiplier: Formula::Const(Decimal::ONE),
                    rate: Formula::Const(dec!(100)),
                    unit: unit_id,
                    currency,
                    applied_fringes: vec![AppliedFringe::new(fringe_id)],
                    groups: vec![],
                    location: None,
                    set: None,
                    gl_code: None,
                    notes: None,
                };
                budget.details.insert(det.id, det);
                line_count += 1;
            }
        }
    }
    assert_eq!(line_count, 5000);

    let start = Instant::now();
    let r = evaluate(&budget);
    let elapsed = start.elapsed();
    eprintln!("full recalc of 5,000 lines: {elapsed:?}");

    // Each line: 30 days × 100 = 3000 net; grossed up by /(1-0.17).
    assert_eq!(r.total.subtotal, dec!(3000) * Decimal::from(5000));
    assert!(!r.has_errors());
    // Generous ceiling for the test profile; real target is <50 ms in release.
    assert!(
        elapsed.as_millis() < 500,
        "recalc unexpectedly slow: {elapsed:?}"
    );
}
