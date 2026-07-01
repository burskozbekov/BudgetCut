//! Coverage for calc behaviors the review flagged as untested, plus regression
//! tests for the overflow/cascade fixes. (Findings 13–22, 1, 3.)

use budgetcut_core::calc::CellTarget;
use budgetcut_core::ids::*;
use budgetcut_core::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

struct B {
    budget: Budget,
    unit: UnitId,
    cur: CurrencyId,
    cat: CategoryId,
    acc: AccountId,
}

fn base() -> B {
    let mut budget = Budget::new("t", templates::try_currency());
    let cur = budget.base_currency;
    let unit = Unit {
        id: UnitId::new(),
        code: "ADET".into(),
        name: Localized::tr(""),
        factor: Decimal::ONE,
    };
    let unit_id = unit.id;
    budget.units.insert(unit_id, unit);
    let cat = Category {
        id: CategoryId::new(),
        number: "1".into(),
        description: Localized::tr(""),
        position: dec!(1),
        atl_btl: Some(AtlBtl::Atl),
        applied_fringes: vec![],
    };
    let acc = Account {
        id: AccountId::new(),
        category: cat.id,
        number: "1".into(),
        description: Localized::tr(""),
        position: dec!(1),
        show_subtotal: true,
        applied_fringes: vec![],
    };
    let (cat_id, acc_id) = (cat.id, acc.id);
    budget.categories.insert(cat.id, cat);
    budget.accounts.insert(acc.id, acc);
    B {
        budget,
        unit: unit_id,
        cur,
        cat: cat_id,
        acc: acc_id,
    }
}

impl B {
    fn fringe(
        &mut self,
        mode: FringeMode,
        level: PostingLevel,
        rate: Decimal,
        cutoff: Option<Decimal>,
        cap: Option<Decimal>,
    ) -> FringeId {
        let f = Fringe {
            id: FringeId::new(),
            code: "F".into(),
            name: Localized::tr(""),
            kind: FringeKind::Percent,
            mode,
            rate,
            posting_level: level,
            cutoff,
            cap,
            currency: None,
        };
        let id = f.id;
        self.budget.fringes.insert(id, f);
        id
    }

    fn detail(&mut self, pos: i64, rate: Decimal, fringes: Vec<AppliedFringe>) -> DetailId {
        self.detail_in(
            self.acc,
            self.cur,
            pos,
            Formula::Const(dec!(1)),
            Formula::Const(rate),
            fringes,
            vec![],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn detail_in(
        &mut self,
        acc: AccountId,
        cur: CurrencyId,
        pos: i64,
        amount: Formula,
        rate: Formula,
        fringes: Vec<AppliedFringe>,
        groups: Vec<GroupId>,
    ) -> DetailId {
        let d = Detail {
            id: DetailId::new(),
            account: acc,
            position: Decimal::from(pos),
            description: String::new(),
            name: None,
            amount,
            multiplier: Formula::Const(Decimal::ONE),
            rate,
            unit: self.unit,
            currency: cur,
            applied_fringes: fringes,
            groups,
            location: None,
            set: None,
            gl_code: None,
            notes: None,
        };
        let id = d.id;
        self.budget.details.insert(id, d);
        id
    }
}

#[test]
fn cutoff_caps_the_base_additive_and_grossup() {
    // Additive: base 100000 capped at 60000 -> 60000 * 0.20 = 12000.
    let mut b = base();
    let add = b.fringe(
        FringeMode::Additive,
        PostingLevel::Detail,
        dec!(0.20),
        Some(dec!(60000)),
        None,
    );
    let capped = b.detail(1, dec!(100000), vec![AppliedFringe::new(add)]);
    let under = b.detail(2, dec!(50000), vec![AppliedFringe::new(add)]); // below cutoff, unaffected
    let r = evaluate(&b.budget);
    assert_eq!(r.detail(capped).fringe_total, dec!(12000));
    assert_eq!(r.detail(under).fringe_total, dec!(10000));

    // GrossUp: base 100000 capped at 60000 -> 60000/0.75 - 60000 = 20000.
    let mut b2 = base();
    let gu = b2.fringe(
        FringeMode::GrossUp,
        PostingLevel::Detail,
        dec!(0.25),
        Some(dec!(60000)),
        None,
    );
    let d = b2.detail(1, dec!(100000), vec![AppliedFringe::new(gu)]);
    let r2 = evaluate(&b2.budget);
    assert_eq!(r2.detail(d).fringe_total, dec!(20000));
}

#[test]
fn cap_limits_the_fringe_amount_and_composes_with_cutoff() {
    // raw 20000 capped to 15000.
    let mut b = base();
    let f = b.fringe(
        FringeMode::Additive,
        PostingLevel::Detail,
        dec!(0.20),
        None,
        Some(dec!(15000)),
    );
    let d = b.detail(1, dec!(100000), vec![AppliedFringe::new(f)]);
    assert_eq!(evaluate(&b.budget).detail(d).fringe_total, dec!(15000));

    // cutoff 60000 then rate 0.20 = 12000, then cap 10000 -> 10000.
    let mut b2 = base();
    let f2 = b2.fringe(
        FringeMode::Additive,
        PostingLevel::Detail,
        dec!(0.20),
        Some(dec!(60000)),
        Some(dec!(10000)),
    );
    let d2 = b2.detail(1, dec!(100000), vec![AppliedFringe::new(f2)]);
    assert_eq!(evaluate(&b2.budget).detail(d2).fringe_total, dec!(10000));
}

#[test]
fn fringe_cascade_budget_category_account_detail_and_breakdown() {
    let mut b = base();
    let fb = b.fringe(
        FringeMode::Additive,
        PostingLevel::Budget,
        dec!(0.10),
        None,
        None,
    );
    let fc = b.fringe(
        FringeMode::Additive,
        PostingLevel::Category,
        dec!(0.05),
        None,
        None,
    );
    let fa = b.fringe(
        FringeMode::Additive,
        PostingLevel::Account,
        dec!(0.02),
        None,
        None,
    );
    let fd = b.fringe(
        FringeMode::Additive,
        PostingLevel::Detail,
        dec!(0.01),
        None,
        None,
    );
    b.budget.applied_fringes.push(AppliedFringe::new(fb));
    b.budget
        .categories
        .get_mut(&b.cat)
        .unwrap()
        .applied_fringes
        .push(AppliedFringe::new(fc));
    b.budget
        .accounts
        .get_mut(&b.acc)
        .unwrap()
        .applied_fringes
        .push(AppliedFringe::new(fa));
    let d = b.detail(1, dec!(100000), vec![AppliedFringe::new(fd)]);

    let r = evaluate(&b.budget);
    // 10000 + 5000 + 2000 + 1000
    assert_eq!(r.detail(d).fringe_total, dec!(18000));
    assert_eq!(r.fringe_breakdown.budget, dec!(10000));
    assert_eq!(r.fringe_breakdown.category, dec!(5000));
    assert_eq!(r.fringe_breakdown.account, dec!(2000));
    assert_eq!(r.fringe_breakdown.detail, dec!(1000));
    assert_eq!(r.fringe_breakdown.total(), r.total.fringe_total);
}

#[test]
fn suppressed_group_excluded_from_atl_fringes_breakdown_and_net() {
    let mut b = base();
    let grp = Group {
        id: GroupId::new(),
        code: "HIDE".into(),
        name: Localized::tr(""),
        in_budget_total: false,
        color: None,
        priority: 0,
    };
    let gid = grp.id;
    b.budget.groups.insert(gid, grp);
    let f = b.fringe(
        FringeMode::Additive,
        PostingLevel::Detail,
        dec!(0.20),
        None,
        None,
    );

    let normal = b.detail(1, dec!(100000), vec![AppliedFringe::new(f)]);
    // Suppressed line in the same ATL account, carrying a fringe.
    let hidden = b.detail_in(
        b.acc,
        b.cur,
        2,
        Formula::Const(dec!(1)),
        Formula::Const(dec!(500000)),
        vec![AppliedFringe::new(f)],
        vec![gid],
    );

    let r = evaluate(&b.budget);
    // Only the normal line counts everywhere.
    assert_eq!(r.atl.subtotal, dec!(100000));
    assert_eq!(r.atl.fringe_total, dec!(20000));
    assert_eq!(r.total.subtotal, dec!(100000));
    assert_eq!(r.fringe_breakdown.detail, dec!(20000)); // hidden line's fringe absent
    assert_eq!(r.net_total, dec!(120000));
    // The hidden line still computes its own value but is flagged excluded.
    assert!(!r.detail(hidden).included);
    assert_eq!(r.detail(hidden).line_total, dec!(600000)); // 500000 + 20% = 600000
    assert!(r.detail(normal).included);
}

#[test]
fn charges_add_to_grand_total_and_bad_formula_errors() {
    let mut b = base();
    let d = b.detail(1, dec!(100000), vec![]);
    let charge_id = uuid::Uuid::now_v7();
    b.budget.charges.insert(
        charge_id,
        Charge {
            id: charge_id,
            label: Localized::tr("Completion Bond"),
            amount: Formula::Const(dec!(5000)),
            position: dec!(1),
        },
    );
    let r = evaluate(&b.budget);
    assert_eq!(r.detail(d).subtotal, dec!(100000));
    assert_eq!(r.charges_total, dec!(5000));
    assert_eq!(r.grand_total, dec!(105000));
    assert_eq!(r.net_total, dec!(105000));

    // A charge with an unresolved reference surfaces a Charge cell error.
    let bad_id = uuid::Uuid::now_v7();
    b.budget.charges.insert(
        bad_id,
        Charge {
            id: bad_id,
            label: Localized::tr("Bad"),
            amount: Formula::expr("NONEXISTENT"),
            position: dec!(2),
        },
    );
    let r2 = evaluate(&b.budget);
    assert!(r2
        .errors
        .iter()
        .any(|e| matches!(e.target, CellTarget::Charge(id) if id == bad_id)));
}

#[test]
fn credits_subtract_from_net_total() {
    let mut b = base();
    b.detail(1, dec!(100000), vec![]);
    let charge_id = uuid::Uuid::now_v7();
    b.budget.charges.insert(
        charge_id,
        Charge {
            id: charge_id,
            label: Localized::tr(""),
            amount: Formula::Const(dec!(10000)),
            position: dec!(1),
        },
    );
    let credit_id = uuid::Uuid::now_v7();
    b.budget.credits.insert(
        credit_id,
        Credit {
            id: credit_id,
            label: Localized::tr("Tax Rebate"),
            amount: Formula::Const(dec!(30000)),
            position: dec!(1),
        },
    );
    let r = evaluate(&b.budget);
    // grand = 100000 + 10000 charge; net = grand - 30000 credit
    assert_eq!(r.grand_total, dec!(110000));
    assert_eq!(r.net_total, dec!(80000));
}

#[test]
fn multi_currency_propagates_through_rollups_and_fringe_uses_base() {
    let mut b = base();
    let usd = templates::usd_currency(dec!(34.5));
    let usd_id = usd.id;
    b.budget.currencies.insert(usd_id, usd);
    let f = b.fringe(
        FringeMode::Additive,
        PostingLevel::Detail,
        dec!(0.10),
        None,
        None,
    );
    // $10,000 @ 34.5 = ₺345,000; fringe 10% computed on the base-converted figure.
    let d = b.detail_in(
        b.acc,
        usd_id,
        1,
        Formula::Const(dec!(1)),
        Formula::Const(dec!(10000)),
        vec![AppliedFringe::new(f)],
        vec![],
    );
    let r = evaluate(&b.budget);
    assert_eq!(r.detail(d).subtotal, dec!(345000));
    assert_eq!(r.detail(d).fringe_total, dec!(34500));
    assert_eq!(r.categories[&b.cat].subtotal, dec!(345000));
    assert_eq!(r.atl.subtotal, dec!(345000));
    assert_eq!(r.total.subtotal, dec!(345000));
    assert_eq!(r.grand_total, dec!(379500)); // 345000 + 34500
    assert_eq!(r.net_total, dec!(379500));
}

#[test]
fn cyclic_global_propagates_err_into_dependent_detail() {
    let mut b = base();
    for (n, v) in [("A", "B + 1"), ("B", "A + 1")] {
        let g = Global {
            id: GlobalId::new(),
            name: n.into(),
            description: Localized::tr(""),
            value: Formula::expr(v),
            in_budget_total: true,
        };
        b.budget.globals.insert(g.id, g);
    }
    let d = b.detail_in(
        b.acc,
        b.cur,
        1,
        Formula::expr("A"),
        Formula::Const(dec!(100)),
        vec![],
        vec![],
    );
    let r = evaluate(&b.budget);
    assert!(r.has_errors());
    assert!(r.detail(d).error);
    assert_eq!(r.detail(d).subtotal, dec!(0));
    assert!(r
        .errors
        .iter()
        .any(|e| matches!(e.target, CellTarget::DetailAmount(id) if id == d)));
}

#[test]
fn overflow_degrades_to_err_without_panicking() {
    // amount 1e15 × rate 1e15 = 1e30, beyond Decimal's ~7.9e28 ceiling.
    let mut b = base();
    let big = Formula::Const(dec!(1000000000000000));
    let d = b.detail_in(b.acc, b.cur, 1, big.clone(), big, vec![], vec![]);
    let normal = b.detail(2, dec!(100), vec![]);
    let r = evaluate(&b.budget); // must not panic
    assert!(r.detail(d).error);
    assert_eq!(r.detail(d).subtotal, dec!(0));
    assert!(r
        .errors
        .iter()
        .any(|e| matches!(e.target, CellTarget::DetailValue(id) if id == d)));
    // The well-formed line still contributes.
    assert_eq!(r.detail(normal).subtotal, dec!(100));
    assert_eq!(r.total.subtotal, dec!(100));
}

#[test]
fn missing_fringe_at_detail_level_is_an_error_not_silent() {
    let mut b = base();
    let ghost = FringeId::new(); // never inserted into budget.fringes
    let d = b.detail(1, dec!(100000), vec![AppliedFringe::new(ghost)]);
    let r = evaluate(&b.budget);
    assert!(r.detail(d).error);
    assert!(r.errors.iter().any(
        |e| matches!(e.target, CellTarget::DetailFringe(did, fid) if did == d && fid == ghost)
    ));
    // validation also catches it.
    assert!(validate(&b.budget)
        .iter()
        .any(|e| matches!(e, ValidationError::DetailMissingFringe(_, _))));
}

#[test]
fn validation_catches_cascade_fringe_references() {
    let mut b = base();
    let ghost = FringeId::new();
    b.budget.applied_fringes.push(AppliedFringe::new(ghost));
    b.budget
        .categories
        .get_mut(&b.cat)
        .unwrap()
        .applied_fringes
        .push(AppliedFringe::new(ghost));
    b.budget
        .accounts
        .get_mut(&b.acc)
        .unwrap()
        .applied_fringes
        .push(AppliedFringe::new(ghost));
    let errs = validate(&b.budget);
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::BudgetMissingFringe(_))));
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::CategoryMissingFringe(_, _))));
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::AccountMissingFringe(_, _))));
}

#[test]
fn detail_under_dangling_category_still_counts_in_grand_total() {
    // The account's category id is absent: validation flags it, but the grand
    // total must still include the line (account vs grand must not disagree).
    let mut b = base();
    b.budget.categories.clear(); // orphan the account's category
    let d = b.detail(1, dec!(50000), vec![]);
    let r = evaluate(&b.budget);
    assert_eq!(r.detail(d).subtotal, dec!(50000));
    assert_eq!(r.total.subtotal, dec!(50000));
    assert_eq!(r.grand_total, dec!(50000));
    assert!(validate(&b.budget)
        .iter()
        .any(|e| matches!(e, ValidationError::AccountMissingCategory(_, _))));
}

#[test]
fn ultra_small_literal_underflow_is_an_error_not_silent_zero() {
    let mut b = base();
    let d = b.detail_in(
        b.acc,
        b.cur,
        1,
        Formula::Const(dec!(1)),
        Formula::expr("0.00000000000000000000000000000001"),
        vec![],
        vec![],
    );
    let r = evaluate(&b.budget);
    assert!(r.detail(d).error);
    assert!(r
        .errors
        .iter()
        .any(|e| matches!(e.target, CellTarget::DetailRate(id) if id == d)));
}
