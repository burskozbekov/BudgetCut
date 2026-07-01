//! Runnable offline-first demo (`cargo run -p budgetcut-store --example offline`).
//!
//! Seeds the Netflix-CoA Turkish dizi template, makes edits **through ops**
//! (persisted to a real on-disk SQLite file), shows live recalc, then **closes
//! and reopens from disk** — proving the budget is rebuilt by replaying the op
//! log and the totals are identical. This is the offline-first MVP path with no
//! GUI required.

use budgetcut_core::ids::*;
use budgetcut_core::*;
use budgetcut_store::{Session, Store};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn money(s: &str) -> String {
    // pretty tr-TR grouping for display only
    let neg = s.starts_with('-');
    let s = s.trim_start_matches('-');
    let (int, frac) = s.split_once('.').unwrap_or((s, "00"));
    let mut g = String::new();
    for (n, c) in int.chars().rev().enumerate() {
        if n > 0 && n % 3 == 0 {
            g.push('.');
        }
        g.push(c);
    }
    let int: String = g.chars().rev().collect();
    format!("{}{},{} ₺", if neg { "-" } else { "" }, int, frac)
}

fn find_account(b: &Budget, number: &str) -> AccountId {
    b.accounts
        .values()
        .find(|a| a.number == number)
        .map(|a| a.id)
        .unwrap_or_else(|| panic!("account {number} not found in template"))
}

fn unit_id(b: &Budget, code: &str) -> UnitId {
    b.units.values().find(|u| u.code == code).unwrap().id
}
fn fringe_id(b: &Budget, code: &str) -> FringeId {
    b.fringes.values().find(|f| f.code == code).unwrap().id
}

fn line(
    acc: AccountId,
    unit: UnitId,
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
        unit,
        currency: cur,
        applied_fringes: fr,
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    }
}

fn print_summary(title: &str, s: &Session) {
    let t = s.topsheet();
    println!("\n  ── {title} ──");
    for c in &t.categories {
        // only show categories that have a value
        if c.total != "0.00" {
            println!(
                "     {:<6} {:<26} {:>18}",
                c.number,
                truncate(&c.name, 26),
                money(&c.total)
            );
        }
    }
    println!("     {:-<52}", "");
    println!("     {:<33} {:>18}", "ATL", money(&t.atl_total));
    println!("     {:<33} {:>18}", "BTL", money(&t.btl_total));
    println!(
        "     {:<33} {:>18}",
        "DİREKT MALİYET",
        money(&t.grand_total)
    );
    println!("     {:<33} {:>18}", "NET TOPLAM", money(&t.net_total));
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).chain(std::iter::once('…')).collect()
    }
}

fn main() {
    // Real on-disk database in a temp dir.
    let dir = std::env::temp_dir().join("budgetcut-offline-demo");
    let _ = std::fs::create_dir_all(&dir);
    let db = dir.join("dizi.db");
    let _ = std::fs::remove_file(&db); // fresh each run
    let author = UserId::new();

    println!("📁 SQLite: {}", db.display());

    // 1) Create from the seeded Netflix CoA Turkish template.
    let base = templates::turkish_dizi_template("Dizi — Bölüm 1");
    let adet = unit_id(&base, "ADET");
    let gun = unit_id(&base, "GUN");
    let stopaj = fringe_id(&base, "TR_STOPAJ");
    let kom = fringe_id(&base, "TR_KOMISYON");
    let cur = base.base_currency;
    let dir_acc = find_account(&base, "1301"); // YÖNETMEN
    let writer_acc = find_account(&base, "1107"); // SENARİST / STAFF WRITERS
    let upm_acc = find_account(&base, "2001"); // YAPIM AMİRİ

    let store = Store::open(&db).unwrap();
    let mut s = Session::create(store, base, author).unwrap();
    println!(
        "✓ {} hesaplı Netflix CoA şablonundan bütçe oluşturuldu",
        s.budget().accounts.len()
    );

    // 2) Edits through ops (each persisted to the log).
    s.edit(OpKind::InsertGlobal(Global {
        id: GlobalId::new(),
        name: "CEKIM_GUN".into(),
        description: Localized::tr("Çekim günü"),
        value: Formula::Const(dec!(30)),
        in_budget_total: true,
    }))
    .unwrap();
    let shoot_days = s
        .budget()
        .globals
        .values()
        .find(|g| g.name == "CEKIM_GUN")
        .unwrap()
        .id;

    s.insert_detail(line(
        dir_acc,
        adet,
        cur,
        "Yönetmen",
        Formula::Const(dec!(1)),
        dec!(660000),
        vec![AppliedFringe::with_rate(stopaj, dec!(0.17))],
    ))
    .unwrap();
    s.insert_detail(line(
        writer_acc,
        adet,
        cur,
        "Senarist",
        Formula::Const(dec!(1)),
        dec!(1320000),
        vec![AppliedFringe::with_rate(stopaj, dec!(0.17))],
    ))
    .unwrap();
    s.insert_detail(line(
        upm_acc,
        gun,
        cur,
        "Yapım Amiri (gün)",
        Formula::expr("CEKIM_GUN"),
        dec!(1500),
        vec![AppliedFringe::with_rate(kom, dec!(0.38))],
    ))
    .unwrap();

    print_summary("1) İlk hesaplama (Çekim günü = 30)", &s);

    // 3) Edit the global -> dependent BTL line recalculates.
    s.set_global(shoot_days, Formula::Const(dec!(45))).unwrap();
    print_summary("2) CEKIM_GUN 30 → 45 (Op uygulandı, log'a yazıldı)", &s);

    let pending = s.outbox().unwrap().len();
    println!("\n  ✎ Outbox (senkronize edilmemiş op): {pending}");

    // 4) Close and reopen from disk: rebuild by replaying the op log.
    drop(s);
    println!("\n  ⟳ Oturum kapatıldı. Diskten yeniden açılıyor (op log replay)…");
    let store = Store::open(&db).unwrap();
    let s2 = Session::open(store, author).unwrap();
    print_summary("3) Diskten yeniden açıldı — sayılar aynı", &s2);
    println!(
        "\n  {} (offline-first: edits persisted, replayed, recomputed)",
        if s2.topsheet().net_total == "2478692.17" {
            "✓ Doğrulandı"
        } else {
            "✗ FARK VAR"
        }
    );
}
