//! Demo for the new MMB-parity analytics:
//! `cargo run -p budgetcut-core --example series_compare`
//!   • amort & pattern (series) budget
//!   • version/location comparison
//!   • incentive estimation

use budgetcut_core::compare::compare;
use budgetcut_core::ids::*;
use budgetcut_core::incentives::turkish_presets;
use budgetcut_core::series::{series_budget, AmortItem};
use budgetcut_core::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn money(d: Decimal) -> String {
    let r = round_money(d);
    let neg = r.is_sign_negative();
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
        if neg { "-" } else { "" },
        g.chars().rev().collect::<String>(),
        &format!("{:0<2}", &f[..f.len().min(2)])
    )
}

/// One-category budget for a location/version, returning (budget, calc net).
fn location_budget(name: &str, num: &str, rate: Decimal) -> Budget {
    let mut b = Budget::new(name, templates::try_currency());
    let unit = Unit {
        id: UnitId::new(),
        code: "ADET".into(),
        name: Localized::tr(""),
        factor: Decimal::ONE,
    };
    let uid = unit.id;
    b.units.insert(uid, unit);
    let cat = Category {
        id: CategoryId::new(),
        number: num.into(),
        description: Localized::tr("MEKAN/OYUNCULAR"),
        position: dec!(1),
        atl_btl: Some(AtlBtl::Btl),
        applied_fringes: vec![],
    };
    let acc = Account {
        id: AccountId::new(),
        category: cat.id,
        number: format!("{num}1"),
        description: Localized::tr(""),
        position: dec!(1),
        show_subtotal: true,
        applied_fringes: vec![],
    };
    let det = Detail {
        id: DetailId::new(),
        account: acc.id,
        position: dec!(1),
        description: "".into(),
        name: None,
        amount: Formula::Const(dec!(1)),
        multiplier: Formula::Const(Decimal::ONE),
        rate: Formula::Const(rate),
        unit: uid,
        currency: b.base_currency,
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
    b
}

fn main() {
    // ---- A) Amort & pattern (8-episode dizi) ----
    let amort = vec![
        AmortItem {
            label: "Ana dekor inşası".into(),
            total: dec!(1200000),
            over_episodes: 8,
        },
        AmortItem {
            label: "Sezon müzik telifi".into(),
            total: dec!(800000),
            over_episodes: 8,
        },
    ];
    let s = series_budget(dec!(2500000), 8, &amort);
    println!("\n  A) AMORT & PATTERN (Dizi, {} bölüm)", s.episodes);
    println!("  {:-<56}", "");
    println!(
        "  {:<34}{:>20}",
        "Pattern bölüm (net)",
        money(s.pattern_episode)
    );
    println!(
        "  {:<34}{:>20}",
        "Pattern toplam (×8)",
        money(s.pattern_total)
    );
    for a in &amort {
        println!(
            "  {:<34}{:>20}",
            format!("Amort: {} (/{})", a.label, a.over_episodes),
            money(a.total)
        );
    }
    println!("  {:<34}{:>20}", "SEZON TOPLAM", money(s.series_total));
    println!(
        "  {:<34}{:>20}",
        "Bölüm başına (amortili)",
        money(s.per_episode_all_in)
    );

    // ---- B) Version/location comparison ----
    let ist = location_budget("İstanbul", "3400", dec!(2000000));
    let kapa = location_budget("Kapadokya", "3400", dec!(2600000));
    let cmp = compare(&ist, &evaluate(&ist), &kapa, &evaluate(&kapa));
    println!("\n  B) KARŞILAŞTIRMA (İstanbul ↔ Kapadokya)");
    println!("  {:-<70}", "");
    println!(
        "  {:<24}{:>14}{:>14}{:>16}",
        "Kategori", "İstanbul", "Kapadokya", "Fark"
    );
    for r in &cmp.rows {
        println!(
            "  {:<24}{:>14}{:>14}{:>16}",
            r.name,
            money(r.a_total),
            money(r.b_total),
            money(r.diff)
        );
    }
    println!("  {:-<70}", "");
    println!(
        "  {:<24}{:>14}{:>14}{:>16}",
        "TOPLAM",
        money(cmp.a_total),
        money(cmp.b_total),
        money(cmp.diff)
    );

    // ---- C) Incentive estimation ----
    let qualifying = s.series_total; // e.g. qualifying Turkish spend
    println!(
        "\n  C) TEŞVİK TAHMİNİ (nitelikli harcama {})",
        money(qualifying)
    );
    println!("  {:-<60}", "");
    for inc in turkish_presets() {
        println!(
            "  {:<40}{:>18}",
            inc.jurisdiction,
            money(inc.estimate(qualifying))
        );
    }
    println!();
}
