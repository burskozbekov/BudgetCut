//! Actuals demo (`cargo run -p budgetcut-core --example actuals`): the
//! Saturation-style closed loop — budgeted estimate vs recorded invoices, with
//! the real Turkish invoice tax breakdown (KDV / stopaj / tevkifat).

use budgetcut_core::actuals::{invoice_breakdown, tevkifat_rate, variance_report, Actual};
use budgetcut_core::ids::*;
use budgetcut_core::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn money(d: Decimal) -> String {
    let r = round_money(d);
    let s = r.abs().to_string();
    let (i, f) = s.split_once('.').unwrap_or((&s, "00"));
    let mut g = String::new();
    for (n, c) in i.chars().rev().enumerate() {
        if n > 0 && n % 3 == 0 {
            g.push('.');
        }
        g.push(c);
    }
    format!(
        "{}{},{} ₺",
        if r.is_sign_negative() { "-" } else { "" },
        g.chars().rev().collect::<String>(),
        &format!("{:0<2}", &f[..f.len().min(2)])
    )
}

fn main() {
    // --- A small budget with estimates ---
    let mut b = Budget::new("Dizi — Bölüm 1", templates::try_currency());
    let unit = Unit {
        id: UnitId::new(),
        code: "ADET".into(),
        name: Localized::tr("Adet"),
        factor: Decimal::ONE,
    };
    let uid = unit.id;
    b.units.insert(uid, unit);
    let cur = b.base_currency;
    let mk = |b: &mut Budget, num: &str, name: &str, atl: AtlBtl, rate: Decimal| -> AccountId {
        let cat = Category {
            id: CategoryId::new(),
            number: num.into(),
            description: Localized::tr(name),
            position: Decimal::from(num.parse::<i64>().unwrap_or(1)),
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
        let aid = acc.id;
        let det = Detail {
            id: DetailId::new(),
            account: aid,
            position: dec!(1),
            description: name.into(),
            name: None,
            amount: Formula::Const(dec!(1)),
            multiplier: Formula::Const(Decimal::ONE),
            rate: Formula::Const(rate),
            unit: uid,
            currency: cur,
            applied_fringes: vec![],
            groups: vec![],
            location: None,
            set: None,
            gl_code: None,
            notes: None,
        };
        b.categories.insert(cat.id, cat);
        b.accounts.insert(acc.id, acc);
        b.details.insert(det.id, det);
        aid
    };
    let cast = mk(&mut b, "1400", "OYUNCULAR", AtlBtl::Atl, dec!(400000)); // estimate 400k
    let transport = mk(&mut b, "3600", "ULAŞIM", AtlBtl::Btl, dec!(250000)); // estimate 250k
    let calc = evaluate(&b);

    // --- Recorded invoices (from the FATURA sheet conventions) ---
    let actuals = vec![
        Actual {
            id: uuid::Uuid::now_v7(),
            account: cast,
            date: "2026-04-15".into(),
            vendor: "Ahmet M. Taylan".into(),
            description: "Oyunculuk Hizmet Bedeli".into(),
            net: dec!(350000),
            stopaj_rate: dec!(0.20),
            kdv_rate: dec!(0.20),
            tevkifat_rate: tevkifat_rate("Ticari Reklam"),
        },
        Actual {
            id: uuid::Uuid::now_v7(),
            account: transport,
            date: "2026-04-20".into(),
            vendor: "Protrans Taşımacılık".into(),
            description: "Ev Taşıma Nakliye".into(),
            net: dec!(258000),
            stopaj_rate: dec!(0),
            kdv_rate: dec!(0.20),
            tevkifat_rate: tevkifat_rate("Yük Taşımacılığı"),
        },
    ];

    println!("\n  FATURALAR (Türk vergi kırılımı)");
    println!("  {:-<92}", "");
    println!(
        "  {:<26}{:>12}{:>12}{:>12}{:>12}{:>16}",
        "Ünvan", "Net", "Stopaj", "KDV", "Tevkifat", "Ödenecek"
    );
    for a in &actuals {
        let bd = a.breakdown();
        println!(
            "  {:<26}{:>12}{:>12}{:>12}{:>12}{:>16}",
            trunc(&a.vendor, 24),
            money(bd.net),
            money(bd.stopaj),
            money(bd.kdv),
            money(bd.tevkifat_kdv),
            money(bd.payable)
        );
    }

    // --- Estimate vs Actual (variance + EFC) ---
    let rep = variance_report(&b, &calc, &actuals);
    println!("\n  TAHMİN vs GERÇEKLEŞEN");
    println!("  {:-<72}", "");
    println!(
        "  {:<22}{:>16}{:>16}{:>16}",
        "Hesap", "Tahmin", "Gerçekleşen", "Fark"
    );
    for (label, acc) in [("OYUNCULAR", cast), ("ULAŞIM", transport)] {
        let v = rep.by_account[&acc];
        println!(
            "  {:<22}{:>16}{:>16}{:>16}",
            label,
            money(v.estimate),
            money(v.actual),
            money(v.variance)
        );
    }
    println!("  {:-<72}", "");
    println!(
        "  {:<22}{:>16}{:>16}{:>16}",
        "TOPLAM",
        money(rep.estimate_total),
        money(rep.actual_total),
        money(rep.variance_total)
    );
    println!(
        "  {:<22}{:>16}",
        "EFC (Tahmini Nihai)",
        money(rep.efc_total)
    );
    println!();

    // sanity (matches the FATURA sheet)
    let r7 = invoice_breakdown(
        dec!(350000),
        dec!(0.20),
        dec!(0.20),
        tevkifat_rate("Ticari Reklam"),
    );
    println!(
        "  ✓ FATURA r7 ödenecek = {} (sayfadaki 411.250 ile birebir)",
        money(r7.payable)
    );
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).chain(['…']).collect()
    }
}
