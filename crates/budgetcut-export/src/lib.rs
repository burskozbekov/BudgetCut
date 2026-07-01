//! Reports & export (§14). Generates Excel (XLSX) and CSV from
//! `budgetcut-core`'s computed [`CalcResult`], so exported figures match the
//! on-screen budget to the kuruş (§20.6).

#![forbid(unsafe_code)]

use budgetcut_core::calc::CalcResult;
use budgetcut_core::{evaluate, round_money, AtlBtl, Budget, Formula};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_xlsxwriter::{Format, FormatBorder, Workbook, XlsxError};

fn f64_money(d: Decimal) -> f64 {
    round_money(d).to_f64().unwrap_or(0.0)
}

fn atl_btl(a: Option<AtlBtl>) -> &'static str {
    match a {
        Some(AtlBtl::Atl) => "ATL",
        Some(AtlBtl::Btl) => "BTL",
        None => "",
    }
}

/// Build a styled XLSX workbook (Topsheet + Account Details sheets) as bytes.
pub fn workbook_bytes(budget: &Budget) -> Result<Vec<u8>, XlsxError> {
    let calc = evaluate(budget);
    let mut wb = Workbook::new();

    let title = Format::new().set_bold().set_font_size(14.0);
    let header = Format::new()
        .set_bold()
        .set_background_color(0x1C2230)
        .set_font_color(0xFFFFFF)
        .set_border(FormatBorder::Thin);
    let money = Format::new().set_num_format("#,##0.00\\ \"₺\"");
    let money_bold = Format::new().set_num_format("#,##0.00\\ \"₺\"").set_bold();
    let label_bold = Format::new().set_bold();

    // --- Topsheet ---
    {
        let ws = wb.add_worksheet().set_name("Topsheet")?;
        ws.set_column_width(0, 8.0)?;
        ws.set_column_width(1, 38.0)?;
        ws.set_column_width(3, 16.0)?;
        ws.set_column_width(4, 16.0)?;
        ws.set_column_width(5, 18.0)?;

        ws.write_with_format(0, 0, format!("BÜTÇE ÖZETİ — {}", budget.name), &title)?;
        let hdr = ["Kod", "Kategori", "ATL/BTL", "Net", "Yansıtma", "Toplam"];
        for (c, h) in hdr.iter().enumerate() {
            ws.write_with_format(2, c as u16, *h, &header)?;
        }

        let mut r = 3u32;
        for cat in budget.categories_sorted() {
            let roll = calc.categories.get(&cat.id).copied().unwrap_or_default();
            ws.write_string(r, 0, &cat.number)?;
            ws.write_string(r, 1, &cat.description.tr)?;
            ws.write_string(r, 2, atl_btl(cat.atl_btl))?;
            ws.write_number_with_format(r, 3, f64_money(roll.subtotal), &money)?;
            ws.write_number_with_format(r, 4, f64_money(roll.fringe_total), &money)?;
            ws.write_number_with_format(r, 5, f64_money(roll.total), &money)?;
            r += 1;
        }

        r += 1;
        let totals = [
            ("ÇİZGİ ÜSTÜ (ATL)", calc.atl.total),
            ("ÇİZGİ ALTI (BTL)", calc.btl.total),
            ("Toplam Yansıtmalar", calc.total.fringe_total),
            ("DİREKT MALİYET", calc.grand_total),
            ("(−) Krediler", calc.credits_total),
            ("NET TOPLAM", calc.net_total),
        ];
        for (label, value) in totals {
            ws.write_with_format(r, 1, label, &label_bold)?;
            ws.write_number_with_format(r, 5, f64_money(value), &money_bold)?;
            r += 1;
        }
    }

    // --- Account Details ---
    {
        let ws = wb.add_worksheet().set_name("Hesap Detayları")?;
        for (c, w) in [22.0, 26.0, 30.0, 8.0, 8.0, 14.0, 14.0, 14.0, 16.0]
            .iter()
            .enumerate()
        {
            ws.set_column_width(c as u16, *w)?;
        }
        let hdr = [
            "Kategori",
            "Hesap",
            "Açıklama",
            "Adet",
            "Birim",
            "Birim Tutar",
            "Net",
            "Yansıtma",
            "G. Toplam",
        ];
        for (c, h) in hdr.iter().enumerate() {
            ws.write_with_format(0, c as u16, *h, &header)?;
        }
        let unit_code = |id| {
            budget
                .units
                .get(id)
                .map(|u| u.code.clone())
                .unwrap_or_default()
        };
        let mut r = 1u32;
        for cat in budget.categories_sorted() {
            for acc in budget.accounts_of(cat.id) {
                for d in budget.details_of(acc.id) {
                    let dc = calc.detail(d.id);
                    ws.write_string(r, 0, &cat.description.tr)?;
                    ws.write_string(r, 1, &acc.description.tr)?;
                    ws.write_string(r, 2, &d.description)?;
                    ws.write_string(r, 3, formula_text(&d.amount))?;
                    ws.write_string(r, 4, unit_code(&d.unit))?;
                    ws.write_string(r, 5, formula_text(&d.rate))?;
                    ws.write_number_with_format(r, 6, f64_money(dc.subtotal), &money)?;
                    ws.write_number_with_format(r, 7, f64_money(dc.fringe_total), &money)?;
                    ws.write_number_with_format(r, 8, f64_money(dc.line_total), &money)?;
                    r += 1;
                }
            }
        }
    }

    // --- Fringe Breakdown ---
    {
        let ws = wb.add_worksheet().set_name("Yansıtmalar")?;
        ws.set_column_width(0, 22.0)?;
        ws.set_column_width(1, 18.0)?;
        ws.write_with_format(0, 0, "Yansıtma Düzeyi", &header)?;
        ws.write_with_format(0, 1, "Tutar", &header)?;
        let fb = &calc.fringe_breakdown;
        let rows = [
            ("Bütçe", fb.budget),
            ("Prodüksiyon", fb.production),
            ("Kategori", fb.category),
            ("Hesap", fb.account),
            ("Satır (Detail)", fb.detail),
        ];
        let mut r = 1u32;
        for (label, amt) in rows {
            ws.write_string(r, 0, label)?;
            ws.write_number_with_format(r, 1, f64_money(amt), &money)?;
            r += 1;
        }
        ws.write_with_format(r, 0, "TOPLAM", &label_bold)?;
        ws.write_number_with_format(r, 1, f64_money(fb.total()), &money_bold)?;
    }

    wb.save_to_buffer()
}

fn formula_text(f: &budgetcut_core::Formula) -> String {
    match f {
        budgetcut_core::Formula::Const(d) => d.normalize().to_string(),
        budgetcut_core::Formula::Expr(s) => format!("={s}"),
    }
}

/// Write the workbook to `path`.
pub fn save_xlsx(path: impl AsRef<std::path::Path>, budget: &Budget) -> Result<(), XlsxError> {
    let bytes = workbook_bytes(budget)?;
    std::fs::write(path, bytes).map_err(XlsxError::IoError)
}

/// Topsheet as CSV (comma-delimited, decimal point, 2dp) — round-trip friendly.
pub fn topsheet_csv(budget: &Budget) -> String {
    let calc = evaluate(budget);
    csv_from_calc(budget, &calc)
}

/// Encode one free-text CSV cell safely: RFC-4180 quoting (wrap, double embedded
/// quotes) **plus** spreadsheet formula-injection neutralization — a leading
/// `= + - @` (or tab/CR) is the trigger Excel/Sheets evaluate, so we prefix an
/// apostrophe. Numbers we format ourselves are safe and skip this.
fn csv_cell(s: &str) -> String {
    let escaped = s.replace('"', "\"\"");
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("\"'{escaped}\"")
    } else {
        format!("\"{escaped}\"")
    }
}

fn csv_from_calc(budget: &Budget, calc: &CalcResult) -> String {
    let m = |d: Decimal| format!("{:.2}", round_money(d));
    let mut out = String::from("number,category,atl_btl,subtotal,fringe,total\n");
    for cat in budget.categories_sorted() {
        let roll = calc.categories.get(&cat.id).copied().unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_cell(&cat.number),
            csv_cell(&cat.description.tr),
            atl_btl(cat.atl_btl),
            m(roll.subtotal),
            m(roll.fringe_total),
            m(roll.total),
        ));
    }
    out.push_str(&format!(",ATL,,,,{}\n", m(calc.atl.total)));
    out.push_str(&format!(",BTL,,,,{}\n", m(calc.btl.total)));
    out.push_str(&format!(",GRAND_TOTAL,,,,{}\n", m(calc.grand_total)));
    out.push_str(&format!(",NET_TOTAL,,,,{}\n", m(calc.net_total)));
    out
}

/// Export the budget as a general-ledger CSV for transfer into an accounting
/// system (the open analog of MMB's SmartAccounting hand-off). One row per
/// account, keyed by account number so the chart of accounts aligns; amounts
/// match the on-screen budget to the kuruş. Free-text cells are injection-safe.
///
/// Account rows sum to the direct cost; the footer then adds production-level
/// charges and credits so the file **reconciles** to GRAND_TOTAL / NET_TOTAL
/// (mirroring the topsheet), even when charges/credits exist.
pub fn accounting_csv(budget: &Budget) -> String {
    let calc = evaluate(budget);
    let m = |d: Decimal| format!("{:.2}", round_money(d));
    let mut out =
        String::from("account_number,category_number,description,atl_btl,net,fringe,total\n");
    for cat in budget.categories_sorted() {
        for acc in budget.accounts_of(cat.id) {
            let mut net = Decimal::ZERO;
            let mut fringe = Decimal::ZERO;
            for d in budget.details_of(acc.id) {
                let dc = calc.detail(d.id);
                if !dc.included {
                    continue; // suppressed group (InBT off): excluded from the GL too
                }
                net += dc.subtotal;
                fringe += dc.fringe_total;
            }
            if net.is_zero() && fringe.is_zero() {
                continue;
            }
            out.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                csv_cell(&acc.number),
                csv_cell(&cat.number),
                csv_cell(&acc.description.tr),
                atl_btl(cat.atl_btl),
                m(net),
                m(fringe),
                m(net + fringe),
            ));
        }
    }
    // Footer reconciles account rows (= direct cost) → grand → net.
    out.push_str(&format!(",,DIRECT_COST,,,,{}\n", m(calc.total.total)));
    out.push_str(&format!(",,CHARGES,,,,{}\n", m(calc.charges_total)));
    out.push_str(&format!(",,GRAND_TOTAL,,,,{}\n", m(calc.grand_total)));
    out.push_str(&format!(",,(-)CREDITS,,,,{}\n", m(calc.credits_total)));
    out.push_str(&format!(",,NET_TOTAL,,,,{}\n", m(calc.net_total)));
    out
}

// ---------------------------------------------------------------------------
// PDF report: rendered as print-styled HTML (§14). The desktop app prints this
// via the webview; the CLI renders it to PDF with the system browser. Keeping
// the report as HTML gives perfect Turkish typography (no font-embedding) and
// one source of truth for both paths.
// ---------------------------------------------------------------------------

fn money_tr(d: Decimal) -> String {
    let r = round_money(d);
    let neg = r.is_sign_negative();
    let s = r.abs().to_string();
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), format!("{:0<2}", &f[..f.len().min(2)])),
        None => (s, "00".into()),
    };
    let mut g = String::new();
    for (n, c) in int.chars().rev().enumerate() {
        if n > 0 && n % 3 == 0 {
            g.push('.');
        }
        g.push(c);
    }
    let int: String = g.chars().rev().collect();
    format!("{}{},{}&nbsp;₺", if neg { "-" } else { "" }, int, frac)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render the budget as a print-ready HTML report (topsheet + account details).
pub fn report_html(budget: &Budget) -> String {
    let calc = evaluate(budget);
    let mut h = String::new();
    h.push_str(r#"<!doctype html><html lang="tr"><head><meta charset="utf-8"><style>
*{box-sizing:border-box} body{font-family:-apple-system,'Segoe UI',Roboto,sans-serif;color:#111;margin:0;font-size:11px}
.page{padding:28px 34px}
h1{font-size:20px;margin:0 0 2px} .muted{color:#666}
.hd{display:flex;justify-content:space-between;align-items:flex-end;border-bottom:2px solid #111;padding-bottom:10px;margin-bottom:14px}
.big{font-size:22px;font-weight:700} .big .lbl{font-size:10px;color:#666;font-weight:400;display:block;text-transform:uppercase;letter-spacing:.05em}
.assume{display:flex;gap:18px;flex-wrap:wrap;margin:10px 0 18px;font-size:10px}
.assume span{background:#f3f4f6;border:1px solid #e5e7eb;border-radius:5px;padding:3px 8px}
table{width:100%;border-collapse:collapse;margin-bottom:18px}
th{text-align:left;font-size:9px;text-transform:uppercase;letter-spacing:.04em;color:#666;border-bottom:1px solid #999;padding:6px 8px}
td{padding:5px 8px;border-bottom:1px solid #eee}
.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
.tag{font-size:8px;font-weight:700;padding:1px 4px;border-radius:3px;border:1px solid #ccc}
.sec{font-weight:700;background:#f7f7f9}
.tot td{border-top:2px solid #111;font-weight:700}
.net td{color:#5b2bd6}
h2{font-size:13px;margin:18px 0 6px;border-bottom:1px solid #ccc;padding-bottom:3px}
@page{size:A4;margin:14mm}
</style></head><body><div class="page">"#);

    // Header
    h.push_str(&format!(
        r#"<div class="hd"><div><h1>{}</h1><div class="muted">Bütçe Özeti — Direkt Maliyet Raporu</div></div>
<div class="big"><span class="lbl">Net Toplam</span>{}</div></div>"#,
        esc(&budget.name),
        money_tr(calc.net_total)
    ));

    // Assumptions (globals)
    let mut globals: Vec<_> = budget.globals.values().collect();
    globals.sort_by(|a, b| a.name.cmp(&b.name));
    if !globals.is_empty() {
        h.push_str(r#"<div class="assume">"#);
        for g in globals {
            let val = match &g.value {
                Formula::Const(d) => d.normalize().to_string(),
                Formula::Expr(s) => format!("={s}"),
            };
            h.push_str(&format!(
                "<span><b>{}</b> = {}</span>",
                esc(&g.name),
                esc(&val)
            ));
        }
        h.push_str("</div>");
    }

    // Topsheet
    h.push_str(r#"<table><thead><tr><th>Kod</th><th>Kategori</th><th>ATL/BTL</th>
<th class="num">Net</th><th class="num">Yansıtma</th><th class="num">Toplam</th></tr></thead><tbody>"#);
    for cat in budget.categories_sorted() {
        let r = calc.categories.get(&cat.id).copied().unwrap_or_default();
        if r.total.is_zero() {
            continue; // skip empty categories on the printed topsheet
        }
        h.push_str(&format!(
            r#"<tr><td>{}</td><td>{}</td><td><span class="tag">{}</span></td>
<td class="num">{}</td><td class="num">{}</td><td class="num">{}</td></tr>"#,
            esc(&cat.number),
            esc(&cat.description.tr),
            atl_btl(cat.atl_btl),
            money_tr(r.subtotal),
            money_tr(r.fringe_total),
            money_tr(r.total),
        ));
    }
    let row = |label: &str, v: Decimal, cls: &str| {
        format!(
            r#"<tr class="{cls}"><td colspan="5">{}</td><td class="num">{}</td></tr>"#,
            label,
            money_tr(v)
        )
    };
    h.push_str(&row("ÇİZGİ ÜSTÜ (ATL)", calc.atl.total, "sec"));
    h.push_str(&row("ÇİZGİ ALTI (BTL)", calc.btl.total, "sec"));
    h.push_str(&row("Toplam Yansıtmalar", calc.total.fringe_total, "sec"));
    h.push_str(&row("DİREKT MALİYET", calc.grand_total, "tot"));
    if !calc.credits_total.is_zero() {
        h.push_str(&row("(−) Krediler / Teşvikler", calc.credits_total, "sec"));
    }
    h.push_str(&row("NET TOPLAM", calc.net_total, "tot net"));
    h.push_str("</tbody></table>");

    // Account details
    h.push_str("<h2>Hesap Detayları</h2>");
    h.push_str(r#"<table><thead><tr><th>Hesap</th><th>Açıklama</th>
<th class="num">Adet</th><th>Birim</th><th class="num">Birim Tutar</th>
<th class="num">Net</th><th class="num">Yansıtma</th><th class="num">G. Toplam</th></tr></thead><tbody>"#);
    let unit_code = |id| {
        budget
            .units
            .get(id)
            .map(|u| u.code.clone())
            .unwrap_or_default()
    };
    for cat in budget.categories_sorted() {
        if calc
            .categories
            .get(&cat.id)
            .map(|r| r.total.is_zero())
            .unwrap_or(true)
        {
            continue;
        }
        h.push_str(&format!(
            r#"<tr class="sec"><td colspan="8">{} {}</td></tr>"#,
            esc(&cat.number),
            esc(&cat.description.tr)
        ));
        for acc in budget.accounts_of(cat.id) {
            for d in budget.details_of(acc.id) {
                let dc = calc.detail(d.id);
                h.push_str(&format!(
                    r#"<tr><td>{}</td><td>{}</td><td class="num">{}</td><td>{}</td>
<td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td></tr>"#,
                    esc(&acc.description.tr),
                    esc(&d.description),
                    esc(&formula_text(&d.amount)),
                    esc(&unit_code(&d.unit)),
                    esc(&formula_text(&d.rate)),
                    money_tr(dc.subtotal),
                    money_tr(dc.fringe_total),
                    money_tr(dc.line_total),
                ));
            }
        }
    }
    h.push_str("</tbody></table>");

    // Fringe breakdown by posting level (§14).
    let fb = &calc.fringe_breakdown;
    if !fb.total().is_zero() {
        h.push_str("<h2>Yansıtma Dağılımı (Fringe Breakdown)</h2>");
        h.push_str(r#"<table><thead><tr><th>Yansıtma Düzeyi</th><th class="num">Tutar</th></tr></thead><tbody>"#);
        for (label, amt) in [
            ("Bütçe", fb.budget),
            ("Prodüksiyon", fb.production),
            ("Kategori", fb.category),
            ("Hesap", fb.account),
            ("Satır (Detail)", fb.detail),
        ] {
            if !amt.is_zero() {
                h.push_str(&format!(
                    r#"<tr><td>{}</td><td class="num">{}</td></tr>"#,
                    label,
                    money_tr(amt)
                ));
            }
        }
        h.push_str(&format!(
            r#"<tr class="tot"><td>TOPLAM YANSITMA</td><td class="num">{}</td></tr>"#,
            money_tr(fb.total())
        ));
        h.push_str("</tbody></table>");
    }

    h.push_str("</div></body></html>");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use budgetcut_core::ids::*;
    use budgetcut_core::*;
    use rust_decimal_macros::dec;

    fn demo() -> Budget {
        let mut b = Budget::new("Test", templates::try_currency());
        let unit = Unit {
            id: UnitId::new(),
            code: "ADET".into(),
            name: Localized::tr(""),
            factor: Decimal::ONE,
        };
        let uid = unit.id;
        b.units.insert(uid, unit);
        let st = Fringe {
            id: FringeId::new(),
            code: "S".into(),
            name: Localized::tr(""),
            kind: FringeKind::Percent,
            mode: FringeMode::GrossUp,
            rate: dec!(0),
            posting_level: PostingLevel::Detail,
            cutoff: None,
            cap: None,
            currency: None,
        };
        let sid = st.id;
        b.fringes.insert(sid, st);
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
        let det = Detail {
            id: DetailId::new(),
            account: acc.id,
            position: dec!(1),
            description: "Yönetmen".into(),
            name: None,
            amount: Formula::Const(dec!(1)),
            multiplier: Formula::Const(Decimal::ONE),
            rate: Formula::Const(dec!(660000)),
            unit: uid,
            currency: b.base_currency,
            applied_fringes: vec![AppliedFringe::with_rate(sid, dec!(0.17))],
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

    #[test]
    fn xlsx_bytes_are_a_valid_zip() {
        let bytes = workbook_bytes(&demo()).unwrap();
        // XLSX is a zip archive: starts with "PK".
        assert_eq!(&bytes[0..2], b"PK");
        assert!(bytes.len() > 1000);
    }

    #[test]
    fn report_html_is_wellformed_and_carries_totals() {
        let h = report_html(&demo());
        assert!(h.starts_with("<!doctype html>"));
        assert!(h.contains("NET TOPLAM"));
        assert!(h.contains("795.180,72")); // grossed-up director, tr-TR formatted
        assert!(h.ends_with("</html>"));
    }

    #[test]
    fn csv_carries_the_engine_totals() {
        let csv = topsheet_csv(&demo());
        // Director grossed up: 660000/(1-0.17) = 795180.72
        assert!(csv.contains("NET_TOTAL,,,,795180.72"), "csv was:\n{csv}");
        assert!(csv.contains("\"1300\",\"YÖNETMEN\",ATL,660000.00,135180.72,795180.72"));
    }

    #[test]
    fn accounting_csv_posts_account_level_gl_rows() {
        let csv = accounting_csv(&demo());
        // GL row keyed by account number 1301 under category 1300 (cells quoted).
        assert!(
            csv.contains("\"1301\",\"1300\",\"YÖNETMEN\",ATL,660000.00,135180.72,795180.72"),
            "csv was:\n{csv}"
        );
        // No charges/credits → DIRECT_COST == GRAND_TOTAL == NET_TOTAL.
        assert!(csv.contains("DIRECT_COST,,,,795180.72"));
        assert!(csv.contains("GRAND_TOTAL,,,,795180.72"));
        assert!(csv.contains("NET_TOTAL,,,,795180.72"));
    }

    #[test]
    fn csv_cell_neutralizes_formula_injection() {
        // A leading formula trigger is defused with a prefixed apostrophe.
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_cell("@SUM(A1)"), "\"'@SUM(A1)\"");
        assert_eq!(csv_cell("-2+3"), "\"'-2+3\"");
        // Ordinary text is just RFC-4180 quoted.
        assert_eq!(csv_cell("Yönetmen"), "\"Yönetmen\"");
    }
}
