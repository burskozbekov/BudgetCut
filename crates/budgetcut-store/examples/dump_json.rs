//! Emit the demo budget's computed view models as JSON on stdout, so the
//! frontend can render genuine engine output in dev/browser mode (where the
//! Tauri `invoke` bridge isn't available). `cargo run -q -p budgetcut-store
//! --example dump_json > apps/desktop/src/fixtures/demo.json`.

use budgetcut_core::ids::*;
use budgetcut_core::*;
use budgetcut_store::{Session, Store};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;

#[derive(Serialize)]
struct Dump {
    topsheet: budgetcut_store::dto::TopsheetDto,
    tree: budgetcut_store::dto::TreeDto,
    tools: budgetcut_store::dto::ToolsDto,
}

fn unit(b: &Budget, c: &str) -> UnitId {
    b.units.values().find(|u| u.code == c).unwrap().id
}
fn fringe(b: &Budget, c: &str) -> FringeId {
    b.fringes.values().find(|f| f.code == c).unwrap().id
}
fn acc(b: &Budget, n: &str) -> AccountId {
    b.accounts.values().find(|a| a.number == n).unwrap().id
}
/// First account in the category with the given number.
fn first_in_cat(b: &Budget, cat_num: &str) -> AccountId {
    let cat = b
        .categories
        .values()
        .find(|c| c.number == cat_num)
        .unwrap()
        .id;
    b.accounts_of(cat)[0].id
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
    let base = templates::turkish_dizi_template("Dizi — Bölüm 1");
    let adet = unit(&base, "ADET");
    let gun = unit(&base, "GUN");
    let stopaj = fringe(&base, "TR_STOPAJ");
    let kom = fringe(&base, "TR_KOMISYON");
    let cur = base.base_currency;
    let dir_acc = acc(&base, "1301");
    let writer_acc = acc(&base, "1107");
    let upm_acc = acc(&base, "2001");
    let cam_acc = first_in_cat(&base, "3100"); // KAMERA EKİBİ

    let store = Store::open_in_memory().unwrap();
    let mut s = Session::create(store, base, UserId::new()).unwrap();

    s.edit(OpKind::InsertGlobal(Global {
        id: GlobalId::new(),
        name: "CEKIM_GUN".into(),
        description: Localized::tr("Çekim günü"),
        value: Formula::Const(dec!(45)),
        in_budget_total: true,
    }))
    .unwrap();

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
        "Yapım Amiri",
        Formula::expr("CEKIM_GUN"),
        dec!(1500),
        vec![AppliedFringe::with_rate(kom, dec!(0.38))],
    ))
    .unwrap();
    s.insert_detail(line(
        upm_acc,
        gun,
        cur,
        "Yönetmen Yardımcısı",
        Formula::expr("CEKIM_GUN"),
        dec!(1200),
        vec![AppliedFringe::with_rate(kom, dec!(0.38))],
    ))
    .unwrap();
    s.insert_detail(line(
        cam_acc,
        gun,
        cur,
        "Kamera paketi",
        Formula::expr("CEKIM_GUN"),
        dec!(8000),
        vec![],
    ))
    .unwrap();

    let dump = Dump {
        topsheet: s.topsheet(),
        tree: s.tree(),
        tools: s.tools(),
    };
    println!("{}", serde_json::to_string_pretty(&dump).unwrap());
}
