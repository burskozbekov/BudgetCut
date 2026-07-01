//! Runnable demo of the BudgetCut engine (`cargo run -p budgetcut-core --example topsheet`).
//!
//! There is no desktop GUI yet (that's the next milestone), so this exercises
//! the real core end-to-end: it builds a Turkish dizi episode budget, prints
//! the topsheet with `tr-TR` formatting, then edits a Global *through an Op*
//! (the same LWW path the sync server uses) to show the dependency graph
//! recalc — and finally proves two clients applying the edits in opposite
//! orders converge to identical numbers.

use budgetcut_core::ids::*;
use budgetcut_core::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

// ----------------------------------------------------------------------------
// tr-TR money formatting (presentation only — all math stays in the core)
// ----------------------------------------------------------------------------

fn fmt_try(d: Decimal) -> String {
    let r = round_money(d);
    let neg = r.is_sign_negative();
    let s = r.abs().to_string(); // scale 2 → "2614070.17"
    let (int_part, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), format!("{:0<2}", &f[..f.len().min(2)])),
        None => (s, "00".into()),
    };
    let mut grouped = String::new();
    for (n, ch) in int_part.chars().rev().enumerate() {
        if n > 0 && n % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let int_fmt: String = grouped.chars().rev().collect();
    format!("{}{},{} ₺", if neg { "-" } else { "" }, int_fmt, frac)
}

fn pad_left(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{}{}", " ".repeat(width - len), s)
    }
}

fn pad_right(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.chars().take(width).collect()
    } else {
        format!("{}{}", s, " ".repeat(width - len))
    }
}

const AMT_W: usize = 18;

// ----------------------------------------------------------------------------
// Build a realistic Turkish dizi episode budget
// ----------------------------------------------------------------------------

struct Build {
    budget: Budget,
    shoot_days: GlobalId,
}

fn build() -> Build {
    let mut b = Budget::new("Dizi — Bölüm 1", templates::try_currency());
    let try_cur = b.base_currency;

    // Units.
    let adet = Unit {
        id: UnitId::new(),
        code: "ADET".into(),
        name: Localized::tr("Adet"),
        factor: Decimal::ONE,
    };
    let gun = Unit {
        id: UnitId::new(),
        code: "GUN".into(),
        name: Localized::tr("Gün"),
        factor: Decimal::ONE,
    };
    let (adet_id, gun_id) = (adet.id, gun.id);
    b.units.insert(adet.id, adet);
    b.units.insert(gun.id, gun);

    // Global: shoot days — BTL crew lines reference it, so editing it ripples.
    let shoot = Global {
        id: GlobalId::new(),
        name: "CEKIM_GUN".into(),
        description: Localized::tr("Çekim günü sayısı"),
        value: Formula::Const(dec!(30)),
        in_budget_total: true,
    };
    let shoot_days = shoot.id;
    b.globals.insert(shoot.id, shoot);

    // Fringes: stopaj (gross-up) + komisyon (additive).
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
    let kom = Fringe {
        id: FringeId::new(),
        code: "TR_KOMISYON".into(),
        name: Localized::tr("Komisyon / SGK"),
        kind: FringeKind::Percent,
        mode: FringeMode::Additive,
        rate: dec!(0),
        posting_level: PostingLevel::Detail,
        cutoff: None,
        cap: None,
        currency: None,
    };
    let (stopaj_id, kom_id) = (stopaj.id, kom.id);
    b.fringes.insert(stopaj.id, stopaj);
    b.fringes.insert(kom.id, kom);

    let mut pos = dec!(0);
    let mut next = || {
        pos += dec!(1);
        pos
    };

    // Helper to add a category + one account, returning the account id.
    let add_cat = |b: &mut Budget, num: &str, name: &str, atl: AtlBtl, p: Decimal| -> AccountId {
        let cat = Category {
            id: CategoryId::new(),
            number: num.into(),
            description: Localized::tr(name),
            position: p,
            atl_btl: Some(atl),
            applied_fringes: vec![],
        };
        let acc = Account {
            id: AccountId::new(),
            category: cat.id,
            number: format!("{num}1"),
            description: Localized::tr(name),
            position: dec!(1),
            show_subtotal: true,
            applied_fringes: vec![],
        };
        let acc_id = acc.id;
        b.categories.insert(cat.id, cat);
        b.accounts.insert(acc.id, acc);
        acc_id
    };

    // line builder
    #[allow(clippy::too_many_arguments)]
    fn line(
        b: &mut Budget,
        acc: AccountId,
        unit: UnitId,
        cur: CurrencyId,
        p: Decimal,
        desc: &str,
        amount: Formula,
        rate: Decimal,
        fringes: Vec<AppliedFringe>,
    ) {
        let d = Detail {
            id: DetailId::new(),
            account: acc,
            position: p,
            description: desc.into(),
            name: None,
            amount,
            multiplier: Formula::Const(Decimal::ONE),
            rate: Formula::Const(rate),
            unit,
            currency: cur,
            applied_fringes: fringes,
            groups: vec![],
            location: None,
            set: None,
            gl_code: None,
            notes: None,
        };
        b.details.insert(d.id, d);
    }

    // ── ATL: Yönetmen ve Metin Yazarı (account numbers from Netflix CoA 1300/1100)
    let atl = add_cat(
        &mut b,
        "1300",
        "YÖNETMEN VE METİN YAZARI",
        AtlBtl::Atl,
        next(),
    );
    line(
        &mut b,
        atl,
        adet_id,
        try_cur,
        dec!(1),
        "Yönetmen",
        Formula::Const(dec!(1)),
        dec!(660000),
        vec![AppliedFringe::with_rate(stopaj_id, dec!(0.17))],
    );
    line(
        &mut b,
        atl,
        adet_id,
        try_cur,
        dec!(2),
        "Senarist",
        Formula::Const(dec!(1)),
        dec!(1320000),
        vec![AppliedFringe::with_rate(stopaj_id, dec!(0.17))],
    );
    line(
        &mut b,
        atl,
        adet_id,
        try_cur,
        dec!(3),
        "Yönetmen 2. Ekip",
        Formula::Const(dec!(1)),
        dec!(165600),
        vec![AppliedFringe::with_rate(kom_id, dec!(0.38))],
    );

    // Production-total divider sets the ATL/BTL boundary.
    let pt = ProductionTotal {
        id: ProductionTotalId::new(),
        label: Localized::tr("ABOVE-THE-LINE TOPLAM"),
        position: next(),
    };
    b.production_totals.insert(pt.id, pt);

    // ── BTL: Yapım Ekibi — paid per shoot day (references the global).
    let crew = add_cat(&mut b, "2000", "YAPIM EKİBİ", AtlBtl::Btl, next());
    for (who, daily) in [
        ("Yapım Amiri", 1500),
        ("Yönetmen Yardımcısı", 1200),
        ("Set Amiri", 1000),
    ] {
        line(
            &mut b,
            crew,
            gun_id,
            try_cur,
            dec!(1),
            who,
            Formula::expr("CEKIM_GUN"),
            Decimal::from(daily),
            vec![AppliedFringe::with_rate(kom_id, dec!(0.38))],
        );
    }

    // ── BTL: Ekipman — camera package per shoot day, no fringe.
    let gear = add_cat(&mut b, "2700", "KAMERA & EKİPMAN", AtlBtl::Btl, next());
    line(
        &mut b,
        gear,
        gun_id,
        try_cur,
        dec!(1),
        "Kamera paketi",
        Formula::expr("CEKIM_GUN"),
        dec!(8000),
        vec![],
    );

    // A credit (production incentive) reduces the net.
    let credit_id = uuid::Uuid::now_v7();
    b.credits.insert(
        credit_id,
        Credit {
            id: credit_id,
            label: Localized::tr("Yapım Teşviki"),
            amount: Formula::Const(dec!(250000)),
            position: dec!(1),
        },
    );

    Build {
        budget: b,
        shoot_days,
    }
}

// ----------------------------------------------------------------------------
// Render the topsheet
// ----------------------------------------------------------------------------

const LABEL_W: usize = 42;
const FR_W: usize = 16;

fn print_topsheet(budget: &Budget) {
    let r = evaluate(budget);
    let rule: String = "─".repeat(LABEL_W + FR_W + AMT_W);

    println!("  BÜTÇE ÖZETİ — {}", budget.name);
    println!("  {}", rule);
    println!(
        "  {}{}{}",
        pad_right("HESAP / KATEGORİ", LABEL_W),
        pad_left("FRINGE", FR_W),
        pad_left("TOPLAM", AMT_W)
    );
    println!("  {}", rule);

    for cat in budget.categories_sorted() {
        let roll = r.categories.get(&cat.id).copied().unwrap_or_default();
        let tag = match cat.atl_btl {
            Some(AtlBtl::Atl) => "ATL",
            Some(AtlBtl::Btl) => "BTL",
            None => "—",
        };
        println!(
            "  {}{}{}",
            pad_right(
                &format!("{} {} [{}]", cat.number, cat.description.tr, tag),
                LABEL_W
            ),
            pad_left(&fmt_try(roll.fringe_total), FR_W),
            pad_left(&fmt_try(roll.total), AMT_W)
        );
    }

    let total_w = LABEL_W + FR_W;
    println!("  {}", rule);
    println!(
        "  {}{}",
        pad_right("ABOVE-THE-LINE (ATL)", total_w),
        pad_left(&fmt_try(r.atl.total), AMT_W)
    );
    println!(
        "  {}{}",
        pad_right("BELOW-THE-LINE (BTL)", total_w),
        pad_left(&fmt_try(r.btl.total), AMT_W)
    );
    println!(
        "  {}{}",
        pad_right("Toplam Yansıtmalar (Fringe)", total_w),
        pad_left(&fmt_try(r.total.fringe_total), AMT_W)
    );
    println!("  {}", rule);
    println!(
        "  {}{}",
        pad_right("DİREKT MALİYET (Grand Total)", total_w),
        pad_left(&fmt_try(r.grand_total), AMT_W)
    );
    if r.credits_total != Decimal::ZERO {
        println!(
            "  {}{}",
            pad_right("(−) Krediler / Teşvikler", total_w),
            pad_left(&fmt_try(r.credits_total), AMT_W)
        );
    }
    println!(
        "  {}{}",
        pad_right("NET TOPLAM", total_w),
        pad_left(&fmt_try(r.net_total), AMT_W)
    );
    if r.has_errors() {
        println!("  ⚠  {} hücre #ERR", r.errors.len());
    }
}

// ----------------------------------------------------------------------------
// main
// ----------------------------------------------------------------------------

fn main() {
    let Build { budget, shoot_days } = build();

    // Sanity: the model is referentially valid.
    assert!(validate(&budget).is_empty(), "seeded budget must be valid");

    println!();
    println!("════════ 1) İlk hesaplama (Çekim günü = 30) ════════\n");
    print_topsheet(&budget);

    // --- Edit a Global through an Op (the real sync/LWW path) ---
    let author = UserId::new();
    let mut clock = HlcClock::new(author);
    let mut doc = Document::new(budget.clone());

    let op = Op::new(
        clock.tick(1_700_000_000_000),
        author,
        OpKind::SetGlobalValue {
            global: shoot_days,
            value: Formula::Const(dec!(45)),
        },
    );
    let outcome = doc.apply(&op);
    println!(
        "\n════════ 2) Global düzenlemesi (Op uygulandı: {:?}) ════════",
        outcome
    );
    println!("    CEKIM_GUN: 30 → 45  →  bağımlı BTL satırları yeniden hesaplandı\n");
    print_topsheet(&doc.budget);

    // --- Convergence: a second client applies the same set of ops in REVERSE
    //     order and must reach byte-identical numbers (offline-first, §20.3). ---
    let op_a = op.clone();
    let op_b = Op::new(
        clock.tick(1_700_000_000_001),
        author,
        OpKind::SetGlobalValue {
            global: shoot_days,
            value: Formula::Const(dec!(45)),
        },
    );
    let mut client1 = Document::new(budget.clone());
    let mut client2 = Document::new(budget.clone());
    client1.apply(&op_a);
    client1.apply(&op_b);
    client2.apply(&op_b);
    client2.apply(&op_a);
    let t1 = evaluate(&client1.budget).net_total;
    let t2 = evaluate(&client2.budget).net_total;
    println!("\n════════ 3) Yakınsama (iki istemci, ters sıra) ════════");
    println!("    İstemci 1 NET: {}", fmt_try(t1));
    println!("    İstemci 2 NET: {}", fmt_try(t2));
    println!(
        "    {}",
        if t1 == t2 && client1.budget == client2.budget {
            "✓ Aynı duruma yakınsadılar (LWW + HLC)"
        } else {
            "✗ AYRIŞTILAR"
        }
    );
    println!();
}
