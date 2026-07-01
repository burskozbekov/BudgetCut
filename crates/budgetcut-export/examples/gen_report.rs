//! Generate an XLSX + CSV topsheet from the seeded dizi budget.
//! `cargo run -p budgetcut-export --example gen_report -- <out_dir>`

use budgetcut_core::ids::*;
use budgetcut_core::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn unit(b: &Budget, c: &str) -> UnitId {
    b.units.values().find(|u| u.code == c).unwrap().id
}
fn fringe(b: &Budget, c: &str) -> FringeId {
    b.fringes.values().find(|f| f.code == c).unwrap().id
}
fn acc(b: &Budget, n: &str) -> AccountId {
    b.accounts.values().find(|a| a.number == n).unwrap().id
}
fn first_in(b: &Budget, cat: &str) -> AccountId {
    let c = b.categories.values().find(|c| c.number == cat).unwrap().id;
    b.accounts_of(c)[0].id
}

fn line(
    acc: AccountId,
    u: UnitId,
    cur: CurrencyId,
    desc: &str,
    amount: Formula,
    rate: Decimal,
    fr: Vec<AppliedFringe>,
) -> Detail {
    Detail {
        id: DetailId::new(),
        account: acc,
        position: dec!(1),
        description: desc.into(),
        name: None,
        amount,
        multiplier: Formula::Const(Decimal::ONE),
        rate: Formula::Const(rate),
        unit: u,
        currency: cur,
        applied_fringes: fr,
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let mut b = templates::turkish_dizi_template("Dizi — Bölüm 1");
    let adet = unit(&b, "ADET");
    let gun = unit(&b, "GUN");
    let stopaj = fringe(&b, "TR_STOPAJ");
    let kom = fringe(&b, "TR_KOMISYON");
    let cur = b.base_currency;

    let g = Global {
        id: GlobalId::new(),
        name: "CEKIM_GUN".into(),
        description: Localized::tr("Çekim günü"),
        value: Formula::Const(dec!(45)),
        in_budget_total: true,
    };
    b.globals.insert(g.id, g);

    let dir = acc(&b, "1301");
    let writer = acc(&b, "1107");
    let upm = acc(&b, "2001");
    let cam = first_in(&b, "3100");
    for d in [
        line(
            dir,
            adet,
            cur,
            "Yönetmen",
            Formula::Const(dec!(1)),
            dec!(660000),
            vec![AppliedFringe::with_rate(stopaj, dec!(0.17))],
        ),
        line(
            writer,
            adet,
            cur,
            "Senarist",
            Formula::Const(dec!(1)),
            dec!(1320000),
            vec![AppliedFringe::with_rate(stopaj, dec!(0.17))],
        ),
        line(
            upm,
            gun,
            cur,
            "Yapım Amiri",
            Formula::expr("CEKIM_GUN"),
            dec!(1500),
            vec![AppliedFringe::with_rate(kom, dec!(0.38))],
        ),
        line(
            upm,
            gun,
            cur,
            "Yönetmen Yardımcısı",
            Formula::expr("CEKIM_GUN"),
            dec!(1200),
            vec![AppliedFringe::with_rate(kom, dec!(0.38))],
        ),
        line(
            cam,
            gun,
            cur,
            "Kamera paketi",
            Formula::expr("CEKIM_GUN"),
            dec!(8000),
            vec![],
        ),
    ] {
        b.details.insert(d.id, d);
    }

    let xlsx = format!("{out}/BudgetCut-Topsheet.xlsx");
    let csv = format!("{out}/BudgetCut-Topsheet.csv");
    let html = format!("{out}/BudgetCut-Report.html");
    budgetcut_export::save_xlsx(&xlsx, &b).expect("write xlsx");
    std::fs::write(&csv, budgetcut_export::topsheet_csv(&b)).expect("write csv");
    std::fs::write(&html, budgetcut_export::report_html(&b)).expect("write html");
    let calc = evaluate(&b);
    println!("XLSX: {xlsx}");
    println!("CSV : {csv}");
    println!("HTML: {html}");
    println!("NET TOPLAM: {:.2} ₺", round_money(calc.net_total));
}
