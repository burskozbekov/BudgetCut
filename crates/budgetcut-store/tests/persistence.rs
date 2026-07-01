//! Offline-first persistence round-trip (§7/§20.3): edits survive a close and
//! reopen, the budget is rebuilt by replaying the op log, totals recompute, and
//! the outbox carries unacknowledged ops.

use budgetcut_core::ids::*;
use budgetcut_core::*;
use budgetcut_store::{Session, Store};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// A tiny base budget: TRY, one ADET unit, a gross-up stopaj fringe, and one
/// ATL category+account to hang details on.
fn base() -> (Budget, UnitId, FringeId, AccountId) {
    let mut b = Budget::new("Persist Test", templates::try_currency());
    let unit = Unit {
        id: UnitId::new(),
        code: "ADET".into(),
        name: Localized::tr("Adet"),
        factor: Decimal::ONE,
    };
    let uid = unit.id;
    b.units.insert(uid, unit);
    let stopaj = Fringe {
        id: FringeId::new(),
        code: "TR_STOPAJ".into(),
        name: Localized::tr("Stopaj"),
        kind: FringeKind::Percent,
        mode: FringeMode::GrossUp,
        rate: dec!(0),
        posting_level: PostingLevel::Detail,
        cutoff: None,
        cap: None,
        currency: None,
    };
    let fid = stopaj.id;
    b.fringes.insert(fid, stopaj);
    let cat = Category {
        id: CategoryId::new(),
        number: "1300".into(),
        description: Localized::tr("YÖNETMEN"),
        position: dec!(1),
        atl_btl: Some(AtlBtl::Atl),
        applied_fringes: vec![],
    };
    let acc = Account {
        id: AccountId::new(),
        category: cat.id,
        number: "1301".into(),
        description: Localized::tr("YÖNETMEN"),
        position: dec!(1),
        show_subtotal: true,
        applied_fringes: vec![],
    };
    let acc_id = acc.id;
    b.categories.insert(cat.id, cat);
    b.accounts.insert(acc.id, acc);
    (b, uid, fid, acc_id)
}

fn director_line(acc: AccountId, unit: UnitId, cur: CurrencyId, stopaj: FringeId) -> Detail {
    Detail {
        id: DetailId::new(),
        account: acc,
        position: dec!(1),
        description: "Yönetmen".into(),
        name: None,
        amount: Formula::Const(dec!(1)),
        multiplier: Formula::Const(Decimal::ONE),
        rate: Formula::Const(dec!(660000)),
        unit,
        currency: cur,
        applied_fringes: vec![AppliedFringe::with_rate(stopaj, dec!(0.17))],
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    }
}

#[test]
fn edits_persist_across_reopen_and_recompute() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("budget.db");
    let author = UserId::new();
    let (b, unit, stopaj, acc) = base();
    let cur = b.base_currency;

    // Create + edit, then drop the session (closing the db).
    {
        let store = Store::open(&path).unwrap();
        let mut s = Session::create(store, b, author).unwrap();
        assert_eq!(
            s.insert_detail(director_line(acc, unit, cur, stopaj))
                .unwrap(),
            ApplyResult::Applied
        );

        let top = s.topsheet();
        // 660000 grossed up by /(1-0.17) => 795180.72
        assert_eq!(top.net_total, "795180.72");
        assert_eq!(top.atl_total, "795180.72");
        assert_eq!(s.outbox().unwrap().len(), 1);
    }

    // Reopen from disk: the op log is replayed and totals match.
    {
        let store = Store::open(&path).unwrap();
        let s = Session::open(store, author).unwrap();
        let top = s.topsheet();
        assert_eq!(top.net_total, "795180.72");
        assert_eq!(s.budget().details.len(), 1);
        assert_eq!(s.outbox().unwrap().len(), 1, "unacked op still in outbox");
    }
}

#[test]
fn global_edit_recalculates_dependent_lines_after_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("b.db");
    let author = UserId::new();
    let (mut b, unit, _stopaj, acc) = base();
    let cur = b.base_currency;

    // Seed a global the detail references, in the base snapshot.
    let g = Global {
        id: GlobalId::new(),
        name: "GUN".into(),
        description: Localized::tr(""),
        value: Formula::Const(dec!(10)),
        in_budget_total: true,
    };
    let gid = g.id;
    b.globals.insert(g.id, g);

    {
        let store = Store::open(&path).unwrap();
        let mut s = Session::create(store, b, author).unwrap();
        // A line costing GUN × 1000 (no fringe).
        let mut d = director_line(acc, unit, cur, FringeId::new());
        d.applied_fringes.clear();
        d.amount = Formula::expr("GUN");
        d.rate = Formula::Const(dec!(1000));
        s.insert_detail(d).unwrap();
        assert_eq!(s.topsheet().net_total, "10000.00");

        // Edit the global -> dependent line recalculates.
        s.set_global(gid, Formula::Const(dec!(25))).unwrap();
        assert_eq!(s.topsheet().net_total, "25000.00");
    }

    // The global edit persisted: reopen yields the recomputed total.
    {
        let store = Store::open(&path).unwrap();
        let s = Session::open(store, author).unwrap();
        assert_eq!(s.topsheet().net_total, "25000.00");
        assert_eq!(s.outbox().unwrap().len(), 2); // insert + set-global
    }
}

#[test]
fn reopen_is_idempotent_no_duplicate_ops() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("b.db");
    let author = UserId::new();
    let (b, unit, stopaj, acc) = base();
    let cur = b.base_currency;

    {
        let store = Store::open(&path).unwrap();
        let mut s = Session::create(store, b, author).unwrap();
        s.insert_detail(director_line(acc, unit, cur, stopaj))
            .unwrap();
    }
    // Open and close several times; the op count must stay stable.
    for _ in 0..3 {
        let store = Store::open(&path).unwrap();
        let s = Session::open(store, author).unwrap();
        assert_eq!(s.budget().details.len(), 1);
        assert_eq!(s.outbox().unwrap().len(), 1);
    }
}

#[test]
fn reseed_replaces_budget_and_persists() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("budget.db");
    let author = UserId::new();
    let (b, unit, stopaj, acc) = base();
    let cur = b.base_currency;

    {
        let store = Store::open(&path).unwrap();
        let mut s = Session::create(store, b, author).unwrap();
        s.insert_detail(director_line(acc, unit, cur, stopaj))
            .unwrap();
        assert_eq!(s.outbox().unwrap().len(), 1);

        // Reload the real BOŞ BÜTÇE sample → whole budget replaced, log cleared.
        s.reseed(templates::dizi_full_template("BOŞ BÜTÇE"))
            .unwrap();
        assert_eq!(s.budget().details.len(), 258);
        assert_eq!(
            round_money(evaluate(s.budget()).grand_total),
            dec!(32488843.87)
        );
        assert_eq!(s.outbox().unwrap().len(), 0, "op log discarded on reseed");
    }

    // The reseeded budget survives a reopen.
    {
        let store = Store::open(&path).unwrap();
        let s = Session::open(store, author).unwrap();
        assert_eq!(s.budget().details.len(), 258);
        assert_eq!(
            round_money(evaluate(s.budget()).grand_total),
            dec!(32488843.87)
        );
    }
}
