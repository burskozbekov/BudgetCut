//! Importers (§13). Pure functions over [`budgetcut_core`] types so they're
//! testable in isolation and validated against source totals.
//!
//! v1 ships a **generic / AICP-style CSV** importer. Movie Magic XML/Excel is
//! Phase 2 (the brief defaults import to Phase 2); its entry point is stubbed
//! below and maps onto the same builder.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::str::FromStr;

use budgetcut_core::ids::*;
use budgetcut_core::*;
use rust_decimal::Decimal;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ImportError {
    #[error("line {line}: expected {expected} columns, found {found}")]
    BadColumns {
        line: usize,
        expected: usize,
        found: usize,
    },
    #[error("line {line}: invalid number {value:?}")]
    BadNumber { line: usize, value: String },
    #[error("empty input")]
    Empty,
    #[error("xml error: {0}")]
    Xml(String),
}

/// Non-fatal import diagnostics.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub categories: usize,
    pub accounts: usize,
    pub details: usize,
    pub warnings: Vec<String>,
}

/// Import a generic budget CSV into a fresh [`Budget`].
///
/// Expected header + rows (comma-delimited; fields may be wrapped in quotes):
/// `category_number,category_name,account_number,account_name,description,qty,rate`
///
/// Categories and accounts are de-duplicated by number; each row becomes a
/// detail line (`qty` × `rate`, no fringes — the basic profile). Account
/// numbers `< 2000` are tagged ATL, matching the Netflix CoA convention.
pub fn import_generic_csv(csv: &str, name: &str) -> Result<(Budget, ImportReport), ImportError> {
    let mut budget = Budget::new(name, templates::try_currency());
    let unit = Unit {
        id: UnitId::new(),
        code: "ADET".into(),
        name: Localized::tr("Adet"),
        factor: Decimal::ONE,
    };
    let unit_id = unit.id;
    budget.units.insert(unit_id, unit);
    let currency = budget.base_currency;

    let mut report = ImportReport::default();
    let mut cat_by_num: HashMap<String, CategoryId> = HashMap::new();
    let mut acc_by_num: HashMap<String, AccountId> = HashMap::new();
    let mut pos = Decimal::ZERO;

    let mut lines = csv.lines().filter(|l| !l.trim().is_empty()).enumerate();
    // Skip header if present (first cell non-numeric).
    let rows: Vec<(usize, String)> = match lines.next() {
        None => return Err(ImportError::Empty),
        Some((i, first)) => {
            let looks_header = first
                .split(',')
                .next()
                .map(|c| Decimal::from_str(c.trim().trim_matches('"')).is_err())
                .unwrap_or(true);
            let mut v = Vec::new();
            if !looks_header {
                v.push((i, first.to_string()));
            }
            v.extend(lines.map(|(i, l)| (i, l.to_string())));
            v
        }
    };

    for (i, line) in rows {
        let cols: Vec<String> = split_csv(&line);
        if cols.len() < 7 {
            return Err(ImportError::BadColumns {
                line: i + 1,
                expected: 7,
                found: cols.len(),
            });
        }
        let cat_num = cols[0].clone();
        let cat_name = cols[1].clone();
        let acc_num = cols[2].clone();
        let acc_name = cols[3].clone();
        let desc = cols[4].clone();
        let qty = parse_num(&cols[5], i + 1)?;
        let rate = parse_num(&cols[6], i + 1)?;

        let cat_id = *cat_by_num.entry(cat_num.clone()).or_insert_with(|| {
            pos += Decimal::ONE;
            let atl =
                acc_num
                    .parse::<u32>()
                    .ok()
                    .map(|n| if n < 2000 { AtlBtl::Atl } else { AtlBtl::Btl });
            let c = Category {
                id: CategoryId::new(),
                number: cat_num.clone(),
                description: Localized::tr(&cat_name),
                position: pos,
                atl_btl: atl,
                applied_fringes: vec![],
            };
            let id = c.id;
            budget.categories.insert(id, c);
            report.categories += 1;
            id
        });

        let acc_id = *acc_by_num.entry(acc_num.clone()).or_insert_with(|| {
            let a = Account {
                id: AccountId::new(),
                category: cat_id,
                number: acc_num.clone(),
                description: Localized::tr(&acc_name),
                position: Decimal::from(report.accounts as i64 + 1),
                show_subtotal: true,
                applied_fringes: vec![],
            };
            let id = a.id;
            budget.accounts.insert(id, a);
            report.accounts += 1;
            id
        });

        let detail = Detail {
            id: DetailId::new(),
            account: acc_id,
            position: Decimal::from(report.details as i64 + 1),
            description: desc,
            name: None,
            amount: Formula::Const(qty),
            multiplier: Formula::Const(Decimal::ONE),
            rate: Formula::Const(rate),
            unit: unit_id,
            currency,
            applied_fringes: vec![],
            groups: vec![],
            location: None,
            set: None,
            gl_code: None,
            notes: None,
        };
        budget.details.insert(detail.id, detail);
        report.details += 1;
    }

    Ok((budget, report))
}

/// Import a Movie Magic-style XML export into a fresh [`Budget`].
///
/// Targets the normalized MMB export shape (attributes shown):
/// ```xml
/// <MovieMagicBudget>
///   <Category Number="1300" Title="DIRECTION">
///     <Account Number="1301" Title="DIRECTOR">
///       <Detail Description="Director" Amount="1" Units="Flat" Rate="660000"/>
///     </Account>
///   </Category>
/// </MovieMagicBudget>
/// ```
/// Real MMB `.mbb`/XML dialects vary; mapping a specific export onto this shape
/// is a thin pre-pass. Account numbers `< 2000` are tagged ATL (Netflix CoA
/// convention). Fringes/globals are not carried (Phase 2 follow-up).
pub fn import_mmb_xml(xml: &str, name: &str) -> Result<(Budget, ImportReport), ImportError> {
    use quick_xml::events::{BytesStart, Event};
    use quick_xml::Reader;

    fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
        e.attributes()
            .flatten()
            .find(|a| a.key.as_ref() == key)
            .and_then(|a| a.unescape_value().ok().map(|c| c.into_owned()))
    }

    let mut budget = Budget::new(name, templates::try_currency());
    let unit = Unit {
        id: UnitId::new(),
        code: "ADET".into(),
        name: Localized::tr("Adet"),
        factor: Decimal::ONE,
    };
    let unit_id = unit.id;
    budget.units.insert(unit_id, unit);
    let currency = budget.base_currency;

    let mut report = ImportReport::default();
    let mut cur_cat: Option<CategoryId> = None;
    let mut cur_acc: Option<AccountId> = None;
    let mut cat_pos = Decimal::ZERO;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"Category" => {
                    cat_pos += Decimal::ONE;
                    let number = attr(&e, b"Number").unwrap_or_default();
                    let atl = number.parse::<u32>().ok().map(|n| {
                        if n < 2000 {
                            AtlBtl::Atl
                        } else {
                            AtlBtl::Btl
                        }
                    });
                    let c = Category {
                        id: CategoryId::new(),
                        number,
                        description: Localized::tr(attr(&e, b"Title").unwrap_or_default()),
                        position: cat_pos,
                        atl_btl: atl,
                        applied_fringes: vec![],
                    };
                    cur_cat = Some(c.id);
                    budget.categories.insert(c.id, c);
                    report.categories += 1;
                }
                b"Account" => {
                    if let Some(cat) = cur_cat {
                        let a = Account {
                            id: AccountId::new(),
                            category: cat,
                            number: attr(&e, b"Number").unwrap_or_default(),
                            description: Localized::tr(attr(&e, b"Title").unwrap_or_default()),
                            position: Decimal::from(report.accounts as i64 + 1),
                            show_subtotal: true,
                            applied_fringes: vec![],
                        };
                        cur_acc = Some(a.id);
                        budget.accounts.insert(a.id, a);
                        report.accounts += 1;
                    }
                }
                b"Detail" => {
                    if let Some(acc) = cur_acc {
                        let amount = attr(&e, b"Amount")
                            .and_then(|s| Decimal::from_str(s.trim()).ok())
                            .unwrap_or(Decimal::ONE);
                        let rate = attr(&e, b"Rate")
                            .and_then(|s| Decimal::from_str(s.trim()).ok())
                            .unwrap_or(Decimal::ZERO);
                        let d = Detail {
                            id: DetailId::new(),
                            account: acc,
                            position: Decimal::from(report.details as i64 + 1),
                            description: attr(&e, b"Description").unwrap_or_default(),
                            name: None,
                            amount: Formula::Const(amount),
                            multiplier: Formula::Const(Decimal::ONE),
                            rate: Formula::Const(rate),
                            unit: unit_id,
                            currency,
                            applied_fringes: vec![],
                            groups: vec![],
                            location: None,
                            set: None,
                            gl_code: None,
                            notes: None,
                        };
                        budget.details.insert(d.id, d);
                        report.details += 1;
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"Category" => cur_cat = None,
                b"Account" => cur_acc = None,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(ImportError::Xml(e.to_string())),
            _ => {}
        }
    }

    if report.details == 0 {
        report.warnings.push("hiç satır bulunamadı".into());
    }
    Ok((budget, report))
}

fn parse_num(s: &str, line: usize) -> Result<Decimal, ImportError> {
    let cleaned = s
        .trim()
        .trim_matches('"')
        .replace('.', "")
        .replace(',', ".");
    // Try the cleaned (tr-style "1.234,56") form first, then the raw form.
    Decimal::from_str(s.trim().trim_matches('"'))
        .or_else(|_| Decimal::from_str(&cleaned))
        .map_err(|_| ImportError::BadNumber {
            line,
            value: s.to_string(),
        })
}

/// Minimal CSV field splitter handling double-quoted fields.
fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    for ch in line.chars() {
        match ch {
            '"' => in_q = !in_q,
            ',' if !in_q => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    out.push(cur.trim().to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
category_number,category_name,account_number,account_name,description,qty,rate
1300,YÖNETMENLER,1301,Yönetmen,Yönetmen ücreti,1,660000
1300,YÖNETMENLER,1301,Yönetmen,2. ekip yönetmen,1,165600
2000,YAPIM EKİBİ,2001,Yapım Amiri,Sezon boyu,30,1500";

    #[test]
    fn imports_and_totals_match_source() {
        let (budget, report) = import_generic_csv(SAMPLE, "Imported").unwrap();
        assert_eq!(report.categories, 2);
        assert_eq!(report.accounts, 2);
        assert_eq!(report.details, 3);

        // Validate against the hand-summed source total (§13 total-validation).
        let expected = Decimal::from(660000) + Decimal::from(165600) + Decimal::from(30 * 1500);
        let calc = evaluate(&budget);
        assert!(!calc.has_errors());
        assert_eq!(calc.total.subtotal, expected);
        assert_eq!(round_money(calc.grand_total), expected); // no fringes
                                                             // ATL/BTL split inferred from account numbers.
        assert_eq!(calc.atl.subtotal, Decimal::from(660000 + 165600));
        assert_eq!(calc.btl.subtotal, Decimal::from(30 * 1500));
    }

    const MMB_XML: &str = r#"<MovieMagicBudget>
      <Category Number="1300" Title="DIRECTION">
        <Account Number="1301" Title="DIRECTOR">
          <Detail Description="Director" Amount="1" Units="Flat" Rate="660000"/>
        </Account>
      </Category>
      <Category Number="2000" Title="PRODUCTION STAFF">
        <Account Number="2001" Title="UPM">
          <Detail Description="Unit Production Manager" Amount="30" Units="Day" Rate="1500"/>
        </Account>
      </Category>
    </MovieMagicBudget>"#;

    #[test]
    fn imports_mmb_xml_and_totals_match() {
        let (budget, report) = import_mmb_xml(MMB_XML, "From MMB").unwrap();
        assert_eq!(report.categories, 2);
        assert_eq!(report.accounts, 2);
        assert_eq!(report.details, 2);
        let calc = evaluate(&budget);
        assert!(!calc.has_errors());
        // 1×660000 (ATL) + 30×1500 (BTL) = 660000 + 45000
        assert_eq!(calc.total.subtotal, Decimal::from(660000 + 45000));
        assert_eq!(calc.atl.subtotal, Decimal::from(660000));
        assert_eq!(calc.btl.subtotal, Decimal::from(45000));
    }

    #[test]
    fn rejects_short_rows() {
        let bad = "1300,Y,1301,Y,desc,1"; // 6 cols
        assert!(matches!(
            import_generic_csv(bad, "x"),
            Err(ImportError::BadColumns { .. })
        ));
    }
}
