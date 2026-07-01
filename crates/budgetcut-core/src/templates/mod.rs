//! Library presets and templates (§5/§12).
//!
//! Ships the **Netflix global Chart of Accounts** (Turkish + English, ATL/BTL
//! tagged) as a ready-to-clone budget skeleton, plus the **Turkish
//! production-accounting fringe presets** (Stopaj gross-up, SGK, agency
//! commission). These are the default Library tools the brief calls for.
//!
//! The CoA is embedded at compile time via `include_str!`, so the crate stays
//! I/O-free (§4) — there is no filesystem read at runtime.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;

use crate::ids::*;
use crate::model::*;

/// The embedded Netflix CoA (curated `Cost Kodlar` sheet: categories with
/// ATL/BTL classification and Turkish + English descriptions).
const NETFLIX_COA_JSON: &str = include_str!("data/netflix_coa.json");

/// The real "BOŞ BÜTÇE" Turkish dizi episode budget (Mayadrom Bölüm 1),
/// parsed from the source `.xlsx` — every line with its net, stopaj (gross-up)
/// and commission (additive) rates. Reproduces the sheet's DİREKT MALİYET of
/// ₺32.488.843,87 to the kuruş when computed by the engine.
const DIZI_FULL_JSON: &str = include_str!("data/dizi_full.json");

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoaRow {
    Category {
        code: String,
        #[serde(default)]
        atl_btl: Option<String>,
        desc_en: Option<String>,
        desc_tr: Option<String>,
    },
    Detail {
        // `category` is present in the JSON but nesting is derived from row
        // order (a detail belongs to the most recent category), so we ignore it.
        code: String,
        desc_en: Option<String>,
        desc_tr: Option<String>,
    },
}

/// A parsed Chart-of-Accounts entry, decoupled from id minting so callers can
/// clone the CoA into a fresh budget with fresh ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoaCategory {
    pub number: String,
    pub name: Localized,
    pub atl_btl: Option<AtlBtl>,
    pub accounts: Vec<CoaAccount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoaAccount {
    pub number: String,
    pub name: Localized,
}

fn localized(tr: Option<String>, en: Option<String>) -> Localized {
    match (tr, en) {
        (Some(tr), Some(en)) => Localized::bilingual(tr, en),
        (Some(tr), None) => Localized::tr(tr),
        (None, Some(en)) => Localized::tr(en),
        (None, None) => Localized::tr(""),
    }
}

/// Parse the embedded Netflix CoA into categories with nested accounts.
#[must_use]
pub fn netflix_coa() -> Vec<CoaCategory> {
    let rows: Vec<CoaRow> =
        serde_json::from_str(NETFLIX_COA_JSON).expect("embedded netflix_coa.json is valid");
    let mut out: Vec<CoaCategory> = Vec::new();
    for row in rows {
        match row {
            CoaRow::Category {
                code,
                atl_btl,
                desc_en,
                desc_tr,
            } => {
                let atl = match atl_btl.as_deref() {
                    Some("ATL") => Some(AtlBtl::Atl),
                    Some("BTL") => Some(AtlBtl::Btl),
                    _ => None,
                };
                out.push(CoaCategory {
                    number: code,
                    name: localized(desc_tr, desc_en),
                    atl_btl: atl,
                    accounts: Vec::new(),
                });
            }
            CoaRow::Detail {
                code,
                desc_en,
                desc_tr,
                ..
            } => {
                if let Some(cat) = out.last_mut() {
                    cat.accounts.push(CoaAccount {
                        number: code,
                        name: localized(desc_tr, desc_en),
                    });
                }
            }
        }
    }
    out
}

/// The Turkish Lira, used as the default base currency (§12).
#[must_use]
pub fn try_currency() -> Currency {
    Currency {
        id: CurrencyId::new(),
        code: "TRY".into(),
        name: Localized::bilingual("Türk Lirası", "Turkish Lira"),
        symbol: "₺".into(),
        rate_to_base: Decimal::ONE,
        is_base: true,
    }
}

/// US Dollar with an example conversion rate (multi-currency demo).
#[must_use]
pub fn usd_currency(rate_to_try: Decimal) -> Currency {
    Currency {
        id: CurrencyId::new(),
        code: "USD".into(),
        name: Localized::bilingual("ABD Doları", "US Dollar"),
        symbol: "$".into(),
        rate_to_base: rate_to_try,
        is_base: false,
    }
}

/// Standard units (FLAT/ADET, DAY, WEEK, HOUR, %).
#[must_use]
pub fn standard_units() -> Vec<Unit> {
    vec![
        Unit {
            id: UnitId::new(),
            code: "ADET".into(),
            name: Localized::bilingual("Adet", "Flat"),
            factor: Decimal::ONE,
        },
        Unit {
            id: UnitId::new(),
            code: "GUN".into(),
            name: Localized::bilingual("Gün", "Day"),
            factor: Decimal::ONE,
        },
        Unit {
            id: UnitId::new(),
            code: "HAFTA".into(),
            name: Localized::bilingual("Hafta", "Week"),
            factor: dec!(7),
        },
        Unit {
            id: UnitId::new(),
            code: "SAAT".into(),
            name: Localized::bilingual("Saat", "Hour"),
            factor: Decimal::ONE,
        },
    ]
}

/// Turkish production-accounting fringe presets (§12).
///
/// * **Gelir Vergisi Stopajı** — income-tax withholding, modelled as a
///   *gross-up* (`brüt = net / (1 − r)`), matching real budgets. Default 20%.
/// * **SGK İşveren Payı** — employer social-security contribution, *additive*.
/// * **İşsizlik Sigortası İşveren Payı** — unemployment insurance, *additive*.
/// * **Ajans / SGK Komisyonu** — agency/commission, *additive*; the real budget
///   applies this with per-line rate overrides (e.g. 0.38, 0.20, 0.15).
#[must_use]
pub fn turkish_fringes() -> Vec<Fringe> {
    vec![
        Fringe {
            id: FringeId::new(),
            code: "TR_STOPAJ".into(),
            name: Localized::bilingual("Gelir Vergisi Stopajı", "Income Tax Withholding"),
            kind: FringeKind::Percent,
            mode: FringeMode::GrossUp,
            rate: dec!(0.20),
            posting_level: PostingLevel::Detail,
            cutoff: None,
            cap: None,
            currency: None,
        },
        Fringe {
            id: FringeId::new(),
            code: "TR_SGK_ISVEREN".into(),
            name: Localized::bilingual("SGK İşveren Payı", "Employer Social Security"),
            kind: FringeKind::Percent,
            mode: FringeMode::Additive,
            rate: dec!(0.205),
            posting_level: PostingLevel::Detail,
            cutoff: None,
            cap: None,
            currency: None,
        },
        Fringe {
            id: FringeId::new(),
            code: "TR_ISSIZLIK_ISVEREN".into(),
            name: Localized::bilingual(
                "İşsizlik Sigortası İşveren Payı",
                "Employer Unemployment Insurance",
            ),
            kind: FringeKind::Percent,
            mode: FringeMode::Additive,
            rate: dec!(0.02),
            posting_level: PostingLevel::Detail,
            cutoff: None,
            cap: None,
            currency: None,
        },
        Fringe {
            id: FringeId::new(),
            code: "TR_KOMISYON".into(),
            name: Localized::bilingual("Ajans / SGK Komisyonu", "Agency / Commission"),
            kind: FringeKind::Percent,
            mode: FringeMode::Additive,
            rate: dec!(0.0),
            posting_level: PostingLevel::Detail,
            cutoff: None,
            cap: None,
            currency: None,
        },
    ]
}

/// Build a fresh budget seeded from the Netflix CoA with TRY base currency,
/// standard units, and the Turkish fringe presets in its Library. Account
/// numbers and ATL/BTL come straight from Netflix's CoA; ids are freshly minted.
#[must_use]
pub fn turkish_dizi_template(name: impl Into<String>) -> Budget {
    let mut budget = Budget::new(name, try_currency());

    for unit in standard_units() {
        budget.units.insert(unit.id, unit);
    }
    for fringe in turkish_fringes() {
        budget.fringes.insert(fringe.id, fringe);
    }

    // Materialize the CoA. Position increments preserve CoA order.
    let mut cat_pos = dec!(0);
    for coa_cat in netflix_coa() {
        cat_pos += dec!(1);
        let category = Category {
            id: CategoryId::new(),
            number: coa_cat.number,
            description: coa_cat.name,
            position: cat_pos,
            atl_btl: coa_cat.atl_btl,
            applied_fringes: vec![],
        };
        let cat_id = category.id;
        budget.categories.insert(cat_id, category);

        let mut acc_pos = dec!(0);
        for coa_acc in coa_cat.accounts {
            acc_pos += dec!(1);
            let account = Account {
                id: AccountId::new(),
                category: cat_id,
                number: coa_acc.number,
                description: coa_acc.name,
                position: acc_pos,
                show_subtotal: true,
                applied_fringes: vec![],
            };
            budget.accounts.insert(account.id, account);
        }
    }

    budget
}

// ---------------------------------------------------------------------------
// "BOŞ BÜTÇE" — the real Turkish dizi episode budget as a seed template.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DiziTemplate {
    // The JSON's "name" is ignored — the budget name comes from the caller.
    categories: Vec<DiziCat>,
}
#[derive(Debug, Deserialize)]
struct DiziCat {
    name: String,
    atl: bool,
    details: Vec<DiziDetail>,
}
#[derive(Debug, Deserialize)]
struct DiziDetail {
    desc: String,
    name: String,
    adet: String,
    birim: String,
    vergi: String,
    kom: String,
}

fn dec_or_zero(s: &str) -> Decimal {
    use std::str::FromStr;
    Decimal::from_str(s.trim()).unwrap_or(Decimal::ZERO)
}

/// Build a budget from the embedded "BOŞ BÜTÇE" dizi template: TRY base, ADET
/// unit, Turkish fringes, and every category → account → line from the real
/// sheet, with per-line stopaj (gross-up) and commission (additive) applied via
/// rate overrides. Ids are freshly minted. The engine recomputes the same
/// DİREKT MALİYET the source spreadsheet shows.
#[must_use]
pub fn dizi_full_template(name: impl Into<String>) -> Budget {
    let tpl: DiziTemplate =
        serde_json::from_str(DIZI_FULL_JSON).expect("embedded dizi_full.json is valid");
    let mut budget = Budget::new(name, try_currency());

    for unit in standard_units() {
        budget.units.insert(unit.id, unit);
    }
    for fringe in turkish_fringes() {
        budget.fringes.insert(fringe.id, fringe);
    }
    let by_code = |code: &str| {
        budget
            .fringes
            .values()
            .find(|f| f.code == code)
            .map(|f| f.id)
    };
    let stopaj = by_code("TR_STOPAJ").expect("TR_STOPAJ preset");
    let komisyon = by_code("TR_KOMISYON").expect("TR_KOMISYON preset");
    let adet = budget
        .units
        .values()
        .find(|u| u.code == "ADET")
        .map(|u| u.id)
        .expect("ADET unit");
    let cur = budget.base_currency;

    let mut cat_pos = dec!(0);
    for (i, c) in tpl.categories.iter().enumerate() {
        cat_pos += dec!(1);
        let category = Category {
            id: CategoryId::new(),
            number: format!("{}", 1000 + i * 100),
            description: Localized::tr(&c.name),
            position: cat_pos,
            atl_btl: Some(if c.atl { AtlBtl::Atl } else { AtlBtl::Btl }),
            applied_fringes: vec![],
        };
        let cat_id = category.id;
        // One account per section (the sheet is section → lines).
        let account = Account {
            id: AccountId::new(),
            category: cat_id,
            number: format!("{}", 1000 + i * 100 + 1),
            description: Localized::tr(&c.name),
            position: dec!(1),
            show_subtotal: true,
            applied_fringes: vec![],
        };
        let acc_id = account.id;
        budget.categories.insert(cat_id, category);
        budget.accounts.insert(acc_id, account);

        let mut pos = dec!(0);
        for d in &c.details {
            pos += dec!(1);
            let vergi = dec_or_zero(&d.vergi);
            let kom = dec_or_zero(&d.kom);
            let mut fringes = Vec::new();
            if vergi > Decimal::ZERO {
                fringes.push(AppliedFringe::with_rate(stopaj, vergi));
            }
            if kom > Decimal::ZERO {
                fringes.push(AppliedFringe::with_rate(komisyon, kom));
            }
            let detail = Detail {
                id: DetailId::new(),
                account: acc_id,
                position: pos,
                description: d.desc.clone(),
                name: if d.name.is_empty() {
                    None
                } else {
                    Some(d.name.clone())
                },
                amount: Formula::Const(dec_or_zero(&d.adet)),
                multiplier: Formula::Const(Decimal::ONE),
                rate: Formula::Const(dec_or_zero(&d.birim)),
                unit: adet,
                currency: cur,
                applied_fringes: fringes,
                groups: vec![],
                location: None,
                set: None,
                gl_code: None,
                notes: None,
            };
            budget.details.insert(detail.id, detail);
        }
    }
    budget
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coa_parses_with_categories_and_accounts() {
        let coa = netflix_coa();
        assert!(coa.len() >= 30, "expected the full Netflix category set");
        // First category is SCRIPT / SENARYO, ATL.
        let first = &coa[0];
        assert_eq!(first.number, "1100");
        assert_eq!(first.name.tr, "SENARYO");
        assert_eq!(first.atl_btl, Some(AtlBtl::Atl));
        assert!(!first.accounts.is_empty());
        // Some category must be BTL (e.g. 2000 PRODUCTION STAFF).
        assert!(coa.iter().any(|c| c.atl_btl == Some(AtlBtl::Btl)));
    }

    #[test]
    fn template_seeds_a_full_budget() {
        let b = turkish_dizi_template("Test Dizi");
        assert!(b.categories.len() >= 30);
        assert!(b.accounts.len() > 100);
        assert_eq!(b.fringes.len(), 4);
        assert!(b.units.len() >= 4);
        assert!(b.fringes.values().any(|f| f.mode == FringeMode::GrossUp));
    }

    #[test]
    fn dizi_full_reproduces_the_source_sheet_total() {
        use crate::{evaluate, round_money};
        let b = dizi_full_template("BOŞ BÜTÇE");
        assert_eq!(b.categories.len(), 22);
        assert_eq!(b.details.len(), 258);
        let r = evaluate(&b);
        // DİREKT MALİYET straight from the source .xlsx (O2 / row 385), to kuruş.
        assert_eq!(round_money(r.grand_total), dec!(32488843.87));
        // ATL boundary from the sheet (ABOVE-THE-LINE TOPLAM row 28).
        assert_eq!(round_money(r.atl.total), dec!(10854170.17));
        assert_eq!(round_money(r.btl.total), dec!(21634673.70));
        assert_eq!(r.errors.len(), 0);

        // Per-category subtotals must match the sheet's "TOPLAM / X" rows too —
        // this rules out compensating errors that a grand-total-only check would
        // miss.
        let cat_total = |name: &str| -> Decimal {
            let id = b
                .categories
                .values()
                .find(|c| c.description.tr == name)
                .unwrap_or_else(|| panic!("category {name} not found"))
                .id;
            round_money(r.categories[&id].total)
        };
        assert_eq!(cat_total("PERSONEL"), dec!(3827196.60)); // sheet row 131
        assert_eq!(cat_total("TEKNİK EKİP"), dec!(2263291.20)); // row 176
        assert_eq!(cat_total("YİYECEK İÇECEK"), dec!(1740000.00)); // row 233
        assert_eq!(cat_total("YÖNETMEN VE METİN YAZARI"), dec!(2614070.17)); // row 9
    }
}
