//! Golden-file test (§4/§20.8): reproduce the real Turkish dizi episode budget.
//!
//! Every expected figure below is taken directly from the source workbook
//! `BOŞ BÜTÇE.xlsx` (sheet `01.Bölüm`). If the engine's math ever drifts from
//! how Turkish production accounting actually computes payroll, this test fails.
//!
//! The two fringe modes under test (then `G.TOPLAM = brüt + kom`):
//!   * **Stopaj** (income-tax withholding) — GROSS-UP: `brüt = net / (1 − r)`.
//!   * **Komisyon / SGK / Ajans** — ADDITIVE: `kom = net × r`.

use budgetcut_core::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Build a one-account budget and apply the two Turkish fringes with per-line
/// rate overrides, returning the budget plus the ids needed to assert.
struct Fixture {
    budget: Budget,
    account: budgetcut_core::ids::AccountId,
    category: budgetcut_core::ids::CategoryId,
    stopaj: budgetcut_core::ids::FringeId,
    komisyon: budgetcut_core::ids::FringeId,
    unit: budgetcut_core::ids::UnitId,
    currency: budgetcut_core::ids::CurrencyId,
}

fn fixture(category_name: &str) -> Fixture {
    use budgetcut_core::ids::*;
    let mut budget = Budget::new("Dizi - Bölüm 1", templates::try_currency());
    let currency = budget.base_currency;

    let unit = Unit {
        id: UnitId::new(),
        code: "ADET".into(),
        name: Localized::tr("Adet"),
        factor: Decimal::ONE,
    };
    let unit_id = unit.id;
    budget.units.insert(unit_id, unit);

    // Stopaj: gross-up withholding. Komisyon: additive. Default rates are
    // overridden per line, exactly like the spreadsheet's VERGİ/KOM columns.
    let stopaj = Fringe {
        id: FringeId::new(),
        code: "TR_STOPAJ".into(),
        name: Localized::tr("Gelir Vergisi Stopajı"),
        kind: FringeKind::Percent,
        mode: FringeMode::GrossUp,
        rate: dec!(0),
        posting_level: PostingLevel::Detail,
        cutoff: None,
        cap: None,
        currency: None,
    };
    let komisyon = Fringe {
        id: FringeId::new(),
        code: "TR_KOMISYON".into(),
        name: Localized::tr("Komisyon"),
        kind: FringeKind::Percent,
        mode: FringeMode::Additive,
        rate: dec!(0),
        posting_level: PostingLevel::Detail,
        cutoff: None,
        cap: None,
        currency: None,
    };
    let stopaj_id = stopaj.id;
    let kom_id = komisyon.id;
    budget.fringes.insert(stopaj_id, stopaj);
    budget.fringes.insert(kom_id, komisyon);

    let category = Category {
        id: CategoryId::new(),
        number: "1100".into(),
        description: Localized::tr(category_name),
        position: dec!(1),
        atl_btl: Some(AtlBtl::Atl),
        applied_fringes: vec![],
    };
    let cat_id = category.id;
    let account = Account {
        id: AccountId::new(),
        category: cat_id,
        number: "1101".into(),
        description: Localized::tr(category_name),
        position: dec!(1),
        show_subtotal: true,
        applied_fringes: vec![],
    };
    let acc_id = account.id;
    budget.categories.insert(cat_id, category);
    budget.accounts.insert(acc_id, account);

    Fixture {
        budget,
        account: acc_id,
        category: cat_id,
        stopaj: stopaj_id,
        komisyon: kom_id,
        unit: unit_id,
        currency,
    }
}

impl Fixture {
    /// Add a payroll line: `adet` × `birim` net, with stopaj/komisyon rates.
    fn line(
        &mut self,
        pos: i64,
        desc: &str,
        adet: Decimal,
        birim: Decimal,
        vergi: Decimal,
        kom: Decimal,
    ) -> budgetcut_core::ids::DetailId {
        use budgetcut_core::ids::*;
        let mut fringes = Vec::new();
        if !vergi.is_zero() {
            fringes.push(AppliedFringe::with_rate(self.stopaj, vergi));
        }
        if !kom.is_zero() {
            fringes.push(AppliedFringe::with_rate(self.komisyon, kom));
        }
        let d = Detail {
            id: DetailId::new(),
            account: self.account,
            position: Decimal::from(pos),
            description: desc.into(),
            name: None,
            amount: Formula::Const(adet),
            multiplier: Formula::Const(Decimal::ONE),
            rate: Formula::Const(birim),
            unit: self.unit,
            currency: self.currency,
            applied_fringes: fringes,
            groups: vec![],
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
fn director_and_writer_section_matches_workbook() {
    // Source rows r6–r9 of sheet 01.Bölüm.
    let mut fx = fixture("YÖNETMEN VE METİN YAZARI");
    let yonetmen = fx.line(1, "YÖNETMEN", dec!(1), dec!(660000), dec!(0.17), dec!(0));
    let ikinci = fx.line(
        2,
        "YÖNETMEN 2.EKİP",
        dec!(1),
        dec!(165600),
        dec!(0),
        dec!(0.38),
    );
    let senarist = fx.line(3, "SENARİST", dec!(1), dec!(1320000), dec!(0.17), dec!(0));

    assert_eq!(validate(&fx.budget), vec![]);
    let r = evaluate(&fx.budget);
    assert!(!r.has_errors());

    // Per-line G.TOPLAM (line_total), rounded to kuruş.
    assert_eq!(round_money(r.detail(yonetmen).line_total), dec!(795180.72));
    assert_eq!(round_money(r.detail(ikinci).line_total), dec!(228528.00));
    assert_eq!(round_money(r.detail(senarist).line_total), dec!(1590361.45));

    // Section (account == category) rollup. Source r9:
    //   net 2.145.600  stopaj 405.542,17  ek 62.928  G.TOPLAM 2.614.070,17
    let cat = r.categories[&fx.category];
    assert_eq!(cat.subtotal, dec!(2145600)); // net is exact
    assert_eq!(round_money(cat.fringe_total), dec!(468470.17)); // stopaj + komisyon
    assert_eq!(round_money(cat.total), dec!(2614070.17)); // G.TOPLAM

    // This category is ATL, so it lands in the ATL rollup.
    assert_eq!(round_money(r.atl.total), dec!(2614070.17));
    assert_eq!(round_money(r.btl.total), dec!(0));
}

#[test]
fn actor_with_stopaj_and_commission_matches_workbook() {
    // Source r17: net 600.000, vergi 0,20, kom 0,15 → G.TOPLAM 840.000.
    let mut fx = fixture("OYUNCULAR");
    let actor = fx.line(
        1,
        "ANA KARAKTER",
        dec!(1),
        dec!(600000),
        dec!(0.20),
        dec!(0.15),
    );
    let r = evaluate(&fx.budget);

    let dc = r.detail(actor);
    assert_eq!(dc.subtotal, dec!(600000)); // net
                                           // stopaj 150.000 (gross-up) + ajans kom 90.000 = 240.000 fringe
    assert_eq!(round_money(dc.fringe_total), dec!(240000.00));
    assert_eq!(round_money(dc.line_total), dec!(840000.00));
}

#[test]
fn bodyguards_quantity_line_matches_workbook() {
    // Source r33 KORUMALAR: adet 6 × birim 6.480 = net 38.880,
    //   vergi 0,25, kom 0,20 → G.TOPLAM 59.616.
    let mut fx = fixture("BÖLÜM OYUNCULARI");
    let korumalar = fx.line(1, "KORUMALAR", dec!(6), dec!(6480), dec!(0.25), dec!(0.20));
    let r = evaluate(&fx.budget);

    let dc = r.detail(korumalar);
    assert_eq!(dc.subtotal, dec!(38880)); // net = 6 × 6480
                                          // gross-up stopaj 12.960 + sgk-kom 7.776 = 20.736
    assert_eq!(round_money(dc.fringe_total), dec!(20736.00));
    assert_eq!(round_money(dc.line_total), dec!(59616.00));
}

#[test]
fn atl_btl_split_respects_production_total_boundary() {
    use budgetcut_core::ids::*;
    // One ATL category (pos 1) and one BTL category (pos 3), divided by a
    // Production Total at pos 2. The first Production Total is the boundary (§5).
    let mut fx = fixture("YÖNETMEN");
    fx.line(1, "YÖNETMEN", dec!(1), dec!(660000), dec!(0.17), dec!(0)); // ATL: 795.180,72

    // BTL category with one simple line, net 100.000, no fringes.
    let btl_cat = Category {
        id: CategoryId::new(),
        number: "2000".into(),
        description: Localized::tr("YAPIM EKİBİ"),
        position: dec!(3),
        atl_btl: Some(AtlBtl::Btl),
        applied_fringes: vec![],
    };
    let btl_acc = Account {
        id: AccountId::new(),
        category: btl_cat.id,
        number: "2001".into(),
        description: Localized::tr("YAPIM AMİRİ"),
        position: dec!(1),
        show_subtotal: true,
        applied_fringes: vec![],
    };
    let btl_detail = Detail {
        id: DetailId::new(),
        account: btl_acc.id,
        position: dec!(1),
        description: "YAPIM AMİRİ".into(),
        name: None,
        amount: Formula::Const(dec!(1)),
        multiplier: Formula::Const(Decimal::ONE),
        rate: Formula::Const(dec!(100000)),
        unit: fx.unit,
        currency: fx.currency,
        applied_fringes: vec![],
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    };
    fx.budget.categories.insert(btl_cat.id, btl_cat);
    fx.budget.accounts.insert(btl_acc.id, btl_acc);
    fx.budget.details.insert(btl_detail.id, btl_detail);

    let pt = ProductionTotal {
        id: ProductionTotalId::new(),
        label: Localized::tr("ABOVE-THE-LINE TOPLAM"),
        position: dec!(2),
    };
    fx.budget.production_totals.insert(pt.id, pt);

    let r = evaluate(&fx.budget);
    assert_eq!(round_money(r.atl.total), dec!(795180.72));
    assert_eq!(round_money(r.btl.total), dec!(100000.00));
    assert_eq!(round_money(r.total.total), dec!(895180.72));
}

#[test]
fn multi_currency_converts_to_base() {
    use budgetcut_core::ids::*;
    // A USD line at 34.5 TRY/USD must roll up in TRY (§6 currency conversion).
    let mut fx = fixture("POST PRODÜKSİYON");
    let usd = templates::usd_currency(dec!(34.5));
    let usd_id = usd.id;
    fx.budget.currencies.insert(usd_id, usd);

    let d = Detail {
        id: DetailId::new(),
        account: fx.account,
        position: dec!(1),
        description: "VFX (USD)".into(),
        name: None,
        amount: Formula::Const(dec!(1)),
        multiplier: Formula::Const(Decimal::ONE),
        rate: Formula::Const(dec!(10000)), // $10,000
        unit: fx.unit,
        currency: usd_id,
        applied_fringes: vec![],
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    };
    let did = d.id;
    fx.budget.details.insert(did, d);

    let r = evaluate(&fx.budget);
    assert_eq!(r.detail(did).subtotal, dec!(345000)); // 10,000 × 34.5
}
