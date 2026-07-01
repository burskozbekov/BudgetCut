//! Property tests for the op reducer (§4/§8/§20.3).
//!
//! The core convergence claim: **for a given set of ops, the materialized
//! budget is independent of application order**, and re-applying any op is a
//! no-op (idempotent). We assert this by applying the same ops in many random
//! orders and checking every result equals the canonical (HLC-sorted) result.

use budgetcut_core::ids::*;
use budgetcut_core::*;
use proptest::prelude::*;
use rust_decimal::Decimal;

const POOL: usize = 4; // number of distinct detail ids ops may target

/// A deterministic Fisher–Yates shuffle seeded by `seed` (no RNG crate needed).
fn shuffle<T>(mut v: Vec<T>, seed: u64) -> Vec<T> {
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    let n = v.len();
    for i in (1..n).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = ((state >> 33) as usize) % (i + 1);
        v.swap(i, j);
    }
    v
}

/// A minimal base document with a category/account/unit already present, plus a
/// pool of detail ids the generated ops will insert/edit/remove.
fn base() -> (Document, Vec<DetailId>, UnitId, CurrencyId, AccountId) {
    let mut budget = Budget::new("conv", templates::try_currency());
    let currency = budget.base_currency;
    let unit = Unit {
        id: UnitId::new(),
        code: "F".into(),
        name: Localized::tr(""),
        factor: Decimal::ONE,
    };
    let unit_id = unit.id;
    budget.units.insert(unit_id, unit);
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
    let ids: Vec<DetailId> = (0..POOL).map(|_| DetailId::new()).collect();
    (Document::new(budget), ids, unit_id, currency, acc_id)
}

fn fresh_detail(id: DetailId, account: AccountId, unit: UnitId, currency: CurrencyId) -> Detail {
    Detail {
        id,
        account,
        position: Decimal::ONE,
        description: "init".into(),
        name: None,
        amount: Formula::Const(Decimal::ONE),
        multiplier: Formula::Const(Decimal::ONE),
        rate: Formula::Const(Decimal::ZERO),
        unit,
        currency,
        applied_fringes: vec![],
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn ops_converge_regardless_of_order(
        actions in prop::collection::vec((0u8..POOL as u8, 0u8..4, -50i64..50), 0..40),
        seeds in prop::collection::vec(any::<u64>(), 1..6),
    ) {
        let (base_doc, ids, unit, currency, account) = base();
        let author = UserId::new();
        let mut clk = HlcClock::new(author);
        let mut wall = 1u64;

        // 1) Insert ops first so each detail's Insert is causally first (lowest HLC).
        let mut ops: Vec<Op> = Vec::new();
        for id in &ids {
            wall += 1;
            ops.push(Op::new(
                clk.tick(wall),
                author,
                OpKind::InsertDetail(fresh_detail(*id, account, unit, currency)),
            ));
        }
        // 2) Field/remove ops, all with higher HLCs.
        for (idx, kind, val) in &actions {
            wall += 1;
            let hlc = clk.tick(wall);
            let id = ids[*idx as usize];
            let kind = match kind {
                0 => OpKind::SetDetailField { detail: id, field: DetailField::Description(format!("d{val}")) },
                1 => OpKind::SetDetailField { detail: id, field: DetailField::Rate(Formula::Const(Decimal::from(*val))) },
                2 => OpKind::SetDetailField { detail: id, field: DetailField::Amount(Formula::Const(Decimal::from(*val))) },
                _ => OpKind::RemoveDetail(id),
            };
            ops.push(Op::new(hlc, author, kind));
        }

        // Canonical result: apply in HLC order.
        let mut canonical_ops = ops.clone();
        canonical_ops.sort_by_key(|o| o.hlc);
        let mut canonical = base_doc.clone();
        for op in &canonical_ops {
            canonical.apply(op);
        }
        let canonical_budget = canonical.budget.clone();

        // Every shuffled order must converge to the same budget.
        let mut orders: Vec<Vec<Op>> = vec![ops.clone()];
        {
            let mut rev = ops.clone();
            rev.reverse();
            orders.push(rev);
        }
        for s in &seeds {
            orders.push(shuffle(ops.clone(), *s));
        }
        for order in orders {
            let mut doc = base_doc.clone();
            for op in &order {
                doc.apply(op);
            }
            prop_assert_eq!(&doc.budget, &canonical_budget,
                "ops did not converge for a permutation");
        }

        // Idempotency: re-applying every op changes nothing.
        let mut again = canonical;
        for op in &ops {
            prop_assert_eq!(again.apply(op), ApplyResult::Idempotent);
        }
        prop_assert_eq!(&again.budget, &canonical_budget);
    }
}
