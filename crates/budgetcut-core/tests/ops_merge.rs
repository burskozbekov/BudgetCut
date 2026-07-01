//! Op serialization + merge regressions for the review findings (6, 7, 8, 19).

use budgetcut_core::ids::*;
use budgetcut_core::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn unit() -> Unit {
    Unit {
        id: UnitId::new(),
        code: "F".into(),
        name: Localized::tr(""),
        factor: Decimal::ONE,
    }
}
fn category() -> Category {
    Category {
        id: CategoryId::new(),
        number: "1".into(),
        description: Localized::tr(""),
        position: dec!(1),
        atl_btl: None,
        applied_fringes: vec![],
    }
}
fn account(cat: CategoryId) -> Account {
    Account {
        id: AccountId::new(),
        category: cat,
        number: "1".into(),
        description: Localized::tr(""),
        position: dec!(1),
        show_subtotal: true,
        applied_fringes: vec![],
    }
}
fn detail(acc: AccountId, unit: UnitId, cur: CurrencyId) -> Detail {
    Detail {
        id: DetailId::new(),
        account: acc,
        position: dec!(1),
        description: "x".into(),
        name: None,
        amount: Formula::Const(Decimal::ONE),
        multiplier: Formula::Const(Decimal::ONE),
        rate: Formula::Const(dec!(100)),
        unit,
        currency: cur,
        applied_fringes: vec![],
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    }
}

/// Finding 6: every OpKind variant — especially the `Remove*` newtypes wrapping
/// a bare-string id — must JSON round-trip. (Internal tagging silently broke
/// these; adjacent tagging fixes it.)
#[test]
fn serializes_every_opkind_variant() {
    let cat = category();
    let acc = account(cat.id);
    let cur = CurrencyId::new();
    let u = unit();
    let det = detail(acc.id, u.id, cur);
    let author = UserId::new();
    let mut clk = HlcClock::new(author);

    let kinds = vec![
        OpKind::InsertCategory(cat.clone()),
        OpKind::RemoveCategory(cat.id),
        OpKind::InsertAccount(acc.clone()),
        OpKind::RemoveAccount(acc.id),
        OpKind::InsertDetail(det.clone()),
        OpKind::RemoveDetail(det.id),
        OpKind::InsertGlobal(Global {
            id: GlobalId::new(),
            name: "G".into(),
            description: Localized::tr(""),
            value: Formula::Const(dec!(3)),
            in_budget_total: true,
        }),
        OpKind::RemoveGlobal(GlobalId::new()),
        OpKind::InsertFringe(Fringe {
            id: FringeId::new(),
            code: "F".into(),
            name: Localized::tr(""),
            kind: FringeKind::Percent,
            mode: FringeMode::GrossUp,
            rate: dec!(0.2),
            posting_level: PostingLevel::Detail,
            cutoff: None,
            cap: None,
            currency: None,
        }),
        OpKind::RemoveFringe(FringeId::new()),
        OpKind::InsertUnit(u.clone()),
        OpKind::RemoveUnit(u.id),
        OpKind::InsertGroup(Group {
            id: GroupId::new(),
            code: "G".into(),
            name: Localized::tr(""),
            in_budget_total: true,
            color: None,
            priority: 0,
        }),
        OpKind::RemoveGroup(GroupId::new()),
        OpKind::InsertCurrency(templates::try_currency()),
        OpKind::RemoveCurrency(cur),
        OpKind::InsertProductionTotal(ProductionTotal {
            id: ProductionTotalId::new(),
            label: Localized::tr(""),
            position: dec!(1),
        }),
        OpKind::RemoveProductionTotal(ProductionTotalId::new()),
        OpKind::SetDetailField {
            detail: det.id,
            field: DetailField::Rate(Formula::Const(dec!(7))),
        },
        OpKind::SetGlobalValue {
            global: GlobalId::new(),
            value: Formula::expr("A + 1"),
        },
    ];

    for kind in kinds {
        let op = Op::new(clk.tick(1), author, kind);
        let json = serde_json::to_string(&op).expect("op must serialize");
        let back: Op = serde_json::from_str(&json).expect("op must deserialize");
        assert_eq!(op, back, "round-trip mismatch for {}", json);
    }
}

// ---- merge regressions ----

fn shuffle(mut v: Vec<Op>, seed: u64) -> Vec<Op> {
    let mut s = seed ^ 0x9E37_79B9_7F4A_7C15;
    for i in (1..v.len()).rev() {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        v.swap(i, (s >> 33) as usize % (i + 1));
    }
    v
}

/// Apply `ops` in several orders against a fresh clone of `base`; assert all
/// orders converge and return the common resulting budget.
fn converged(base: &Document, ops: &[Op]) -> Budget {
    let mut orders: Vec<Vec<Op>> = vec![ops.to_vec(), {
        let mut r = ops.to_vec();
        r.reverse();
        r
    }];
    for seed in [1u64, 2, 3, 99] {
        orders.push(shuffle(ops.to_vec(), seed));
    }
    let mut result: Option<Budget> = None;
    for order in orders {
        let mut doc = base.clone();
        for op in &order {
            doc.apply(op);
        }
        match &result {
            None => result = Some(doc.budget),
            Some(prev) => assert_eq!(prev, &doc.budget, "did not converge"),
        }
    }
    result.unwrap()
}

fn base_doc() -> (Document, UnitId, CurrencyId, AccountId) {
    let mut budget = Budget::new("t", templates::try_currency());
    let cur = budget.base_currency;
    let u = unit();
    let uid = u.id;
    budget.units.insert(uid, u);
    let cat = category();
    let acc = account(cat.id);
    let acc_id = acc.id;
    budget.categories.insert(cat.id, cat);
    budget.accounts.insert(acc.id, acc);
    (Document::new(budget), uid, cur, acc_id)
}

/// Findings 7 + 19: remove-then-resurrect with a stale buffered edit. Because
/// the resurrecting insert seeds the field registers at its HLC, the stale
/// edit from the previous incarnation loses; a newer edit still wins. Must hold
/// for every application order.
#[test]
fn remove_then_resurrect_converges() {
    let (base, uid, cur, acc) = base_doc();
    let author = UserId::new();
    let n = |ms: u64| Hlc::new(ms, 0, author);
    let mut det1 = detail(acc, uid, cur);
    det1.rate = Formula::Const(dec!(1));
    let did = det1.id;
    let mut det2 = det1.clone(); // resurrection payload
    det2.rate = Formula::Const(dec!(2));

    let ops = vec![
        Op::new(n(10), author, OpKind::InsertDetail(det1)),
        Op::new(
            n(25),
            author,
            OpKind::SetDetailField {
                detail: did,
                field: DetailField::Rate(Formula::Const(dec!(999))),
            },
        ),
        Op::new(n(30), author, OpKind::RemoveDetail(did)),
        Op::new(n(40), author, OpKind::InsertDetail(det2)),
        Op::new(
            n(50),
            author,
            OpKind::SetDetailField {
                detail: did,
                field: DetailField::Rate(Formula::Const(dec!(5))),
            },
        ),
    ];

    let budget = converged(&base, &ops);
    // Entity present (insert@40 > remove@30); rate is the latest write@50, not
    // the stale @25 nor the resurrection payload.
    let d = budget.details.get(&did).expect("detail resurrected");
    assert_eq!(d.rate, Formula::Const(dec!(5)));
}

/// Finding 8: an insert claims its field registers, so a stale lower-HLC field
/// set cannot overwrite a newer insert's payload — regardless of arrival order.
#[test]
fn stale_set_does_not_overwrite_newer_insert() {
    let (base, _uid, _cur, _acc) = base_doc();
    let author = UserId::new();
    let n = |ms: u64| Hlc::new(ms, 0, author);
    let g = Global {
        id: GlobalId::new(),
        name: "G".into(),
        description: Localized::tr(""),
        value: Formula::Const(dec!(100)),
        in_budget_total: true,
    };
    let gid = g.id;

    let ops = vec![
        Op::new(n(50), author, OpKind::InsertGlobal(g)),
        Op::new(
            n(20),
            author,
            OpKind::SetGlobalValue {
                global: gid,
                value: Formula::Const(dec!(7)),
            },
        ),
    ];
    let budget = converged(&base, &ops);
    // The insert (HLC 50) post-dates the set (HLC 20), so the value stays 100.
    assert_eq!(budget.globals[&gid].value, Formula::Const(dec!(100)));
}
