//! Serializable view models (§11). Pure projections of computed
//! [`crate::calc::CalcResult`] into UI-ready DTOs: already-rounded kuruş decimal
//! strings + ids, so any client — desktop, server-rendered browser, future
//! mobile — renders without doing business math (§4/§11). I/O-free, so this
//! lives in the core crate and is shared by the store and the sync server.

use crate::calc::CalcResult;
use crate::{round_money, AtlBtl, Budget, Formula};
use rust_decimal::Decimal;
use serde::Serialize;

/// A money value rounded to kuruş and rendered with exactly 2 decimal places.
fn money(d: Decimal) -> String {
    format!("{:.2}", round_money(d))
}

fn atl_btl_str(a: Option<AtlBtl>) -> Option<String> {
    a.map(|x| match x {
        AtlBtl::Atl => "ATL".to_string(),
        AtlBtl::Btl => "BTL".to_string(),
    })
}

/// How a [`Formula`] input should be shown/edited in the grid.
#[derive(Debug, Clone, Serialize)]
pub struct FormulaDto {
    pub is_expr: bool,
    pub text: String,
}

impl From<&Formula> for FormulaDto {
    fn from(f: &Formula) -> Self {
        match f {
            Formula::Const(d) => FormulaDto {
                is_expr: false,
                text: d.normalize().to_string(),
            },
            Formula::Expr(s) => FormulaDto {
                is_expr: true,
                text: s.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TopsheetCategory {
    pub id: String,
    pub number: String,
    pub name: String,
    pub atl_btl: Option<String>,
    pub subtotal: String,
    pub fringe_total: String,
    pub total: String,
}

/// The topsheet summary (§5 derived view).
#[derive(Debug, Clone, Serialize)]
pub struct TopsheetDto {
    pub budget_name: String,
    pub base_currency: String,
    pub categories: Vec<TopsheetCategory>,
    pub atl_total: String,
    pub btl_total: String,
    pub fringes_total: String,
    pub grand_total: String,
    pub charges_total: String,
    pub credits_total: String,
    pub net_total: String,
    pub error_count: usize,
}

impl TopsheetDto {
    pub fn build(budget: &Budget, r: &CalcResult) -> Self {
        let base_currency = budget
            .currencies
            .get(&budget.base_currency)
            .map(|c| c.code.clone())
            .unwrap_or_default();
        let categories = budget
            .categories_sorted()
            .into_iter()
            .map(|c| {
                let roll = r.categories.get(&c.id).copied().unwrap_or_default();
                TopsheetCategory {
                    id: c.id.to_string(),
                    number: c.number.clone(),
                    name: c.description.tr.clone(),
                    atl_btl: atl_btl_str(c.atl_btl),
                    subtotal: money(roll.subtotal),
                    fringe_total: money(roll.fringe_total),
                    total: money(roll.total),
                }
            })
            .collect();
        TopsheetDto {
            budget_name: budget.name.clone(),
            base_currency,
            categories,
            atl_total: money(r.atl.total),
            btl_total: money(r.btl.total),
            fringes_total: money(r.total.fringe_total),
            grand_total: money(r.grand_total),
            charges_total: money(r.charges_total),
            credits_total: money(r.credits_total),
            net_total: money(r.net_total),
            error_count: r.errors.len(),
        }
    }
}

/// A fringe applied to a line, as shown on the grid (code + effective rate).
#[derive(Debug, Clone, Serialize)]
pub struct AppliedFringeView {
    pub code: String,
    pub rate: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetailRow {
    pub id: String,
    pub description: String,
    pub name: Option<String>,
    pub amount: FormulaDto,
    pub multiplier: FormulaDto,
    pub rate: FormulaDto,
    pub unit: String,
    pub currency: String,
    pub fringes: Vec<AppliedFringeView>,
    pub subtotal: String,
    pub fringe_total: String,
    pub line_total: String,
    pub error: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountNode {
    pub id: String,
    pub number: String,
    pub name: String,
    pub subtotal: String,
    pub fringe_total: String,
    pub total: String,
    pub details: Vec<DetailRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryNode {
    pub id: String,
    pub number: String,
    pub name: String,
    pub atl_btl: Option<String>,
    pub subtotal: String,
    pub fringe_total: String,
    pub total: String,
    pub accounts: Vec<AccountNode>,
}

/// The full editable budget tree for the account-details grid.
#[derive(Debug, Clone, Serialize)]
pub struct TreeDto {
    pub budget_name: String,
    pub categories: Vec<CategoryNode>,
}

impl TreeDto {
    pub fn build(budget: &Budget, r: &CalcResult) -> Self {
        let unit_code = |id| {
            budget
                .units
                .get(id)
                .map(|u| u.code.clone())
                .unwrap_or_default()
        };
        let cur_code = |id| {
            budget
                .currencies
                .get(id)
                .map(|c| c.code.clone())
                .unwrap_or_default()
        };
        let categories = budget
            .categories_sorted()
            .into_iter()
            .map(|c| {
                let croll = r.categories.get(&c.id).copied().unwrap_or_default();
                let accounts = budget
                    .accounts_of(c.id)
                    .into_iter()
                    .map(|a| {
                        let aroll = r.accounts.get(&a.id).copied().unwrap_or_default();
                        let details = budget
                            .details_of(a.id)
                            .into_iter()
                            .map(|d| {
                                let dc = r.detail(d.id);
                                let fringes = d
                                    .applied_fringes
                                    .iter()
                                    .filter_map(|af| {
                                        budget.fringes.get(&af.fringe_id).map(|f| {
                                            AppliedFringeView {
                                                code: f.code.clone(),
                                                rate: af
                                                    .rate_override
                                                    .unwrap_or(f.rate)
                                                    .normalize()
                                                    .to_string(),
                                            }
                                        })
                                    })
                                    .collect();
                                DetailRow {
                                    id: d.id.to_string(),
                                    description: d.description.clone(),
                                    name: d.name.clone(),
                                    amount: (&d.amount).into(),
                                    multiplier: (&d.multiplier).into(),
                                    rate: (&d.rate).into(),
                                    unit: unit_code(&d.unit),
                                    currency: cur_code(&d.currency),
                                    fringes,
                                    subtotal: money(dc.subtotal),
                                    fringe_total: money(dc.fringe_total),
                                    line_total: money(dc.line_total),
                                    error: dc.error,
                                }
                            })
                            .collect();
                        AccountNode {
                            id: a.id.to_string(),
                            number: a.number.clone(),
                            name: a.description.tr.clone(),
                            subtotal: money(aroll.subtotal),
                            fringe_total: money(aroll.fringe_total),
                            total: money(aroll.total),
                            details,
                        }
                    })
                    .collect();
                CategoryNode {
                    id: c.id.to_string(),
                    number: c.number.clone(),
                    name: c.description.tr.clone(),
                    atl_btl: atl_btl_str(c.atl_btl),
                    subtotal: money(croll.subtotal),
                    fringe_total: money(croll.fringe_total),
                    total: money(croll.total),
                    accounts,
                }
            })
            .collect();
        TreeDto {
            budget_name: budget.name.clone(),
            categories,
        }
    }
}

/// One row of the **Ulusal Dizi Formatı** sheet — a faithful reproduction of
/// the Turkish national TV-series budget layout (the `BOŞ BÜTÇE` workbook):
/// `AÇIKLAMA | ADET | VERGİ/STOPAJ ORANI | KOM. ORANI | BİRİM TUTAR | NET TUTAR
/// | STOPAJ | EK ÜCRET/KOMİSYON | G.TOPLAM`, with per-section `TOPLAM /` lines,
/// the `ABOVE-THE-LINE`/`BELOW-THE-LINE` subtotals, and the `DİREKT MALİYET`
/// grand total. Money is display-rounded from full-precision decimals, exactly
/// like the source workbook (so the grand total reproduces it to the kuruş).
#[derive(Debug, Clone, Serialize)]
pub struct NationalRow {
    /// `category` | `line` | `subtotal` | `section` | `grand`.
    pub kind: String,
    /// Left label (category / line description / "TOPLAM / …" / section name).
    pub label: String,
    /// Column C — the line's İSİM (proper name), when present.
    pub name: Option<String>,
    pub atl_btl: Option<String>,
    /// ADET (quantity) — blank on header rows.
    pub adet: String,
    /// VERGİ / STOPAJ ORANI as a fraction string ("0.17"), when the line has one.
    pub vergi_orani: Option<String>,
    /// KOM. ORANI as a fraction string, when the line has one.
    pub kom_orani: Option<String>,
    /// BİRİM TUTAR (unit price) — blank on header rows.
    pub birim_tutar: Option<String>,
    pub net_tutar: String,
    pub stopaj: String,
    pub ek_komisyon: String,
    pub g_toplam: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NationalSheetDto {
    pub budget_name: String,
    pub rows: Vec<NationalRow>,
    pub atl_total: String,
    pub btl_total: String,
    pub net_grand: String,
    pub stopaj_grand: String,
    pub komisyon_grand: String,
    /// DİREKT MALİYET (net + stopaj + komisyon across every section).
    pub grand_total: String,
}

/// Saturating add — user-driven aggregates must never panic on overflow
/// (rust_decimal's `+` does). Matches the convention in actuals/settlement/calc.
fn sat_add(a: Decimal, b: Decimal) -> Decimal {
    a.checked_add(b).unwrap_or(Decimal::MAX)
}

/// Running money accumulators for a section / grand total. Every field holds
/// already-kuruş-rounded values (round-first), so a total is the exact sum of
/// the rounded line cells above it — the printed columns foot with no drift.
#[derive(Default, Clone, Copy)]
struct NatAcc {
    adet: Decimal,
    net: Decimal,
    stopaj: Decimal,
    komisyon: Decimal,
    g: Decimal,
}

impl NatAcc {
    fn add(&mut self, o: &NatAcc) {
        self.adet = sat_add(self.adet, o.adet);
        self.net = sat_add(self.net, o.net);
        self.stopaj = sat_add(self.stopaj, o.stopaj);
        self.komisyon = sat_add(self.komisyon, o.komisyon);
        self.g = sat_add(self.g, o.g);
    }
}

/// Render a quantity: integer when whole, else trimmed to 2 dp.
fn qty(d: Decimal) -> String {
    let n = d.normalize();
    if n.fract().is_zero() {
        format!("{n:.0}")
    } else {
        format!("{n:.2}")
    }
}

impl NationalSheetDto {
    pub fn build(budget: &Budget, r: &CalcResult) -> Self {
        let mut rows: Vec<NationalRow> = Vec::new();
        let mut atl = NatAcc::default();
        let mut btl = NatAcc::default();

        // Emit ATL sections first, then BTL — matching the workbook order.
        for want_atl in [true, false] {
            let mut section = NatAcc::default();
            let mut any = false;
            for c in budget.categories_sorted() {
                // Share the calc engine's ATL boundary (ProductionTotal-aware) so
                // this sheet's split can never contradict the topsheet's.
                if crate::calc::category_is_atl(budget, c) != want_atl {
                    continue;
                }
                any = true;
                rows.push(NationalRow {
                    kind: "category".into(),
                    label: c.description.tr.clone(),
                    name: None,
                    atl_btl: Some(if want_atl { "ATL".into() } else { "BTL".into() }),
                    adet: String::new(),
                    vergi_orani: None,
                    kom_orani: None,
                    birim_tutar: None,
                    net_tutar: String::new(),
                    stopaj: String::new(),
                    ek_komisyon: String::new(),
                    g_toplam: String::new(),
                });
                let mut cat = NatAcc::default();
                for a in budget.accounts_of(c.id) {
                    for d in budget.details_of(a.id) {
                        let dc = r.detail(d.id);
                        if !dc.included {
                            continue;
                        }
                        let split = crate::calc::detail_fringe_split(budget, d, dc.subtotal);
                        // Round each money column to kuruş FIRST, then derive the
                        // row total + accumulate the rounded values, so every
                        // printed column foots to its subtotal (the same
                        // reconciliation discipline as the actuals/settlement
                        // reports). g_toplam is net+stopaj+komisyon by definition.
                        let net = round_money(dc.subtotal);
                        let stopaj = round_money(split.grossup);
                        let komisyon = round_money(split.additive);
                        let g = sat_add(sat_add(net, stopaj), komisyon);
                        // adet = net / birim so the row foots; fall back to the
                        // evaluated multiplier when there's no unit price.
                        let adet = if dc.rate.is_zero() {
                            dc.multiplier
                        } else {
                            dc.subtotal.checked_div(dc.rate).unwrap_or(dc.multiplier)
                        };
                        let label = if d.description.trim().is_empty() {
                            a.description.tr.clone()
                        } else {
                            d.description.clone()
                        };
                        rows.push(NationalRow {
                            kind: "line".into(),
                            label,
                            name: d.name.clone(),
                            atl_btl: None,
                            adet: qty(adet),
                            vergi_orani: split.grossup_rate.map(|x| x.normalize().to_string()),
                            kom_orani: split.additive_rate.map(|x| x.normalize().to_string()),
                            birim_tutar: Some(money(dc.rate)),
                            net_tutar: money(net),
                            stopaj: money(stopaj),
                            ek_komisyon: money(komisyon),
                            g_toplam: money(g),
                        });
                        cat.adet = sat_add(cat.adet, adet);
                        cat.net = sat_add(cat.net, net);
                        cat.stopaj = sat_add(cat.stopaj, stopaj);
                        cat.komisyon = sat_add(cat.komisyon, komisyon);
                        cat.g = sat_add(cat.g, g);
                    }
                }
                rows.push(subtotal_row(
                    "subtotal",
                    format!("TOPLAM / {}", c.description.tr),
                    &cat,
                ));
                section.add(&cat);
            }
            if any {
                let label = if want_atl {
                    "ABOVE-THE-LINE TOPLAM"
                } else {
                    "BELOW-THE-LINE TOPLAM"
                };
                rows.push(subtotal_row("section", label.to_string(), &section));
            }
            if want_atl {
                atl.add(&section);
            } else {
                btl.add(&section);
            }
        }

        let mut grand = NatAcc::default();
        grand.add(&atl);
        grand.add(&btl);
        rows.push(subtotal_row("grand", "DİREKT MALİYET".into(), &grand));

        NationalSheetDto {
            budget_name: budget.name.clone(),
            rows,
            atl_total: money(atl.g),
            btl_total: money(btl.g),
            net_grand: money(grand.net),
            stopaj_grand: money(grand.stopaj),
            komisyon_grand: money(grand.komisyon),
            grand_total: money(grand.g),
        }
    }
}

fn subtotal_row(kind: &str, label: String, a: &NatAcc) -> NationalRow {
    NationalRow {
        kind: kind.into(),
        label,
        name: None,
        atl_btl: None,
        adet: qty(a.adet),
        vergi_orani: None,
        kom_orani: None,
        birim_tutar: None,
        net_tutar: money(a.net),
        stopaj: money(a.stopaj),
        ek_komisyon: money(a.komisyon),
        g_toplam: money(a.g),
    }
}

/// Setup Tools view (§5/§11): the reusable fringes, globals and units.
#[derive(Debug, Clone, Serialize)]
pub struct FringeToolDto {
    pub code: String,
    pub name: String,
    pub kind: String,
    pub mode: String,
    pub rate: String,
    pub posting_level: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalToolDto {
    pub name: String,
    pub description: String,
    pub value: FormulaDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnitToolDto {
    pub code: String,
    pub name: String,
    pub factor: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolsDto {
    pub fringes: Vec<FringeToolDto>,
    pub globals: Vec<GlobalToolDto>,
    pub units: Vec<UnitToolDto>,
}

impl ToolsDto {
    pub fn build(budget: &Budget) -> Self {
        use crate::{FringeKind, FringeMode, PostingLevel};
        let mut fringes: Vec<_> = budget
            .fringes
            .values()
            .map(|f| FringeToolDto {
                code: f.code.clone(),
                name: f.name.tr.clone(),
                kind: match f.kind {
                    FringeKind::Percent => "yüzde",
                    FringeKind::Flat => "sabit",
                }
                .into(),
                mode: match f.mode {
                    FringeMode::Additive => "ek (additive)",
                    FringeMode::GrossUp => "brüte tamamlama (gross-up)",
                }
                .into(),
                rate: f.rate.normalize().to_string(),
                posting_level: match f.posting_level {
                    PostingLevel::Budget => "Bütçe",
                    PostingLevel::Production => "Prodüksiyon",
                    PostingLevel::Category => "Kategori",
                    PostingLevel::Account => "Hesap",
                    PostingLevel::Detail => "Satır",
                }
                .into(),
            })
            .collect();
        fringes.sort_by(|a, b| a.code.cmp(&b.code));
        let mut globals: Vec<_> = budget
            .globals
            .values()
            .map(|g| GlobalToolDto {
                name: g.name.clone(),
                description: g.description.tr.clone(),
                value: (&g.value).into(),
            })
            .collect();
        globals.sort_by(|a, b| a.name.cmp(&b.name));
        let mut units: Vec<_> = budget
            .units
            .values()
            .map(|u| UnitToolDto {
                code: u.code.clone(),
                name: u.name.tr.clone(),
                factor: u.factor.normalize().to_string(),
            })
            .collect();
        units.sort_by(|a, b| a.code.cmp(&b.code));
        ToolsDto {
            fringes,
            globals,
            units,
        }
    }
}

// ---------------------------------------------------------------------------
// Analytics view models (MMB parity): amort/pattern series, budget comparison,
// incentive estimation. Same money-as-string rule so clients never compute.
// ---------------------------------------------------------------------------

/// A season-wide cost the user wants amortized over N episodes (UI input).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AmortInput {
    pub label: String,
    /// Decimal string (e.g. "1200000").
    pub total: String,
    pub over_episodes: u32,
}

/// Series (amort + pattern) summary as kuruş strings (§5 / MMB amort+pattern).
#[derive(Debug, Clone, Serialize)]
pub struct SeriesSummaryDto {
    pub episodes: u32,
    pub pattern_episode: String,
    pub pattern_total: String,
    pub amort_total: String,
    pub series_total: String,
    pub per_episode_all_in: String,
}

impl From<crate::series::SeriesSummary> for SeriesSummaryDto {
    fn from(s: crate::series::SeriesSummary) -> Self {
        SeriesSummaryDto {
            episodes: s.episodes,
            pattern_episode: money(s.pattern_episode),
            pattern_total: money(s.pattern_total),
            amort_total: money(s.amort_total),
            series_total: money(s.series_total),
            per_episode_all_in: money(s.per_episode_all_in),
        }
    }
}

/// Build a series summary for a budget: the budget's net total is the pattern
/// episode; `amortized` are season-wide costs spread across the episodes.
pub fn series_summary_for(
    r: &CalcResult,
    episodes: u32,
    amortized: &[AmortInput],
) -> SeriesSummaryDto {
    use crate::series::{series_budget, AmortItem};
    use std::str::FromStr;
    let items: Vec<AmortItem> = amortized
        .iter()
        .map(|a| AmortItem {
            label: a.label.clone(),
            total: round_money(Decimal::from_str(a.total.trim()).unwrap_or(Decimal::ZERO)),
            over_episodes: a.over_episodes,
        })
        .collect();
    // The pattern episode is a *finalized* episode budget, so replicate the
    // kuruş-rounded net — keeps pattern_episode × episodes == pattern_total on
    // screen (rather than carrying the engine's full precision into ×N).
    series_budget(round_money(r.net_total), episodes, &items).into()
}

/// One category's A-vs-B comparison row (kuruş strings).
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonRow {
    pub number: String,
    pub name: String,
    pub a_total: String,
    pub b_total: String,
    pub diff: String,
}

/// Side-by-side budget/version/location comparison (MMB compare).
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonDto {
    pub a_name: String,
    pub b_name: String,
    pub rows: Vec<ComparisonRow>,
    pub a_total: String,
    pub b_total: String,
    pub diff: String,
}

impl ComparisonDto {
    pub fn build(a: &Budget, a_calc: &CalcResult, b: &Budget, b_calc: &CalcResult) -> Self {
        let cmp = crate::compare::compare(a, a_calc, b, b_calc);
        ComparisonDto {
            a_name: a.name.clone(),
            b_name: b.name.clone(),
            rows: cmp
                .rows
                .into_iter()
                .map(|r| ComparisonRow {
                    number: r.number,
                    name: r.name,
                    a_total: money(r.a_total),
                    b_total: money(r.b_total),
                    diff: money(r.diff),
                })
                .collect(),
            a_total: money(cmp.a_total),
            b_total: money(cmp.b_total),
            diff: money(cmp.diff),
        }
    }
}

/// One incentive program applied to a qualifying spend (rate as a fraction
/// string e.g. "0.30"; estimate in kuruş).
#[derive(Debug, Clone, Serialize)]
pub struct IncentiveLineDto {
    pub jurisdiction: String,
    pub rate: String,
    pub cap: Option<String>,
    pub estimate: String,
}

/// Estimate every Turkish incentive preset against a qualifying spend.
#[derive(Debug, Clone, Serialize)]
pub struct IncentiveReportDto {
    pub qualifying_spend: String,
    pub lines: Vec<IncentiveLineDto>,
}

impl IncentiveReportDto {
    pub fn turkish_for(qualifying_spend: Decimal) -> Self {
        let lines = crate::incentives::turkish_presets()
            .into_iter()
            .map(|inc| IncentiveLineDto {
                jurisdiction: inc.jurisdiction.clone(),
                rate: inc.rate.normalize().to_string(),
                cap: inc.cap.map(money),
                estimate: money(inc.estimate(qualifying_spend)),
            })
            .collect();
        IncentiveReportDto {
            qualifying_spend: money(qualifying_spend),
            lines,
        }
    }
}

/// One recorded actual with its Turkish invoice tax breakdown (kuruş strings).
#[derive(Debug, Clone, Serialize)]
pub struct ActualLineDto {
    pub id: String,
    pub account_number: String,
    pub account_name: String,
    pub vendor: String,
    pub description: String,
    pub net: String,
    pub brut: String,
    pub stopaj: String,
    pub kdv: String,
    pub tevkifat_kdv: String,
    pub payable: String,
}

/// One account's estimate-vs-actual / EFC line.
#[derive(Debug, Clone, Serialize)]
pub struct ActualVarianceRow {
    pub account_number: String,
    pub account_name: String,
    pub estimate: String,
    pub actual: String,
    /// `estimate − actual` (negative = over budget).
    pub variance: String,
    pub efc: String,
    pub over: bool,
}

/// Estimate-vs-actual / EFC report plus the underlying invoice lines (§16
/// Phase 3 — the Saturation-style closed loop).
#[derive(Debug, Clone, Serialize)]
pub struct ActualsReportDto {
    pub rows: Vec<ActualVarianceRow>,
    pub estimate_total: String,
    pub actual_total: String,
    pub variance_total: String,
    pub efc_total: String,
    pub lines: Vec<ActualLineDto>,
}

impl ActualsReportDto {
    pub fn build(budget: &Budget, r: &CalcResult) -> Self {
        let actuals: Vec<crate::actuals::Actual> = budget.actuals.values().cloned().collect();
        let report = crate::actuals::variance_report(budget, r, &actuals);

        let acct = |id: &crate::ids::AccountId| {
            budget
                .accounts
                .get(id)
                .map(|a| (a.number.clone(), a.description.tr.clone()))
                .unwrap_or_default()
        };

        // Round each column to kuruş *first*, then derive variance/EFC and the
        // totals from those rounded values, so the on-screen table reconciles
        // exactly (columns subtract; rows sum to the totals) — no off-by-a-kuruş.
        let mut est_total = Decimal::ZERO;
        let mut act_total = Decimal::ZERO;
        let mut efc_total = Decimal::ZERO;
        let mut rows: Vec<ActualVarianceRow> = report
            .by_account
            .iter()
            .map(|(id, v)| {
                let (number, name) = acct(id);
                let est = round_money(v.estimate);
                let act = round_money(v.actual);
                let efc = if act > est { act } else { est };
                // Saturating: a hostile/absurd actual must not panic the report.
                est_total = est_total.checked_add(est).unwrap_or(Decimal::MAX);
                act_total = act_total.checked_add(act).unwrap_or(Decimal::MAX);
                efc_total = efc_total.checked_add(efc).unwrap_or(Decimal::MAX);
                ActualVarianceRow {
                    account_number: number,
                    account_name: name,
                    estimate: money(est),
                    actual: money(act),
                    variance: money(est - act),
                    efc: money(efc),
                    over: act > est,
                }
            })
            .collect();
        rows.sort_by(|a, b| a.account_number.cmp(&b.account_number));

        let mut lines: Vec<ActualLineDto> = actuals
            .iter()
            .map(|a| {
                let (number, name) = acct(&a.account);
                let bd = a.breakdown();
                ActualLineDto {
                    id: a.id.to_string(),
                    account_number: number,
                    account_name: name,
                    vendor: a.vendor.clone(),
                    description: a.description.clone(),
                    net: money(a.net),
                    brut: money(bd.brut),
                    stopaj: money(bd.stopaj),
                    kdv: money(bd.kdv),
                    tevkifat_kdv: money(bd.tevkifat_kdv),
                    payable: money(bd.payable),
                }
            })
            .collect();
        lines.sort_by(|a, b| a.account_number.cmp(&b.account_number));

        ActualsReportDto {
            rows,
            estimate_total: money(est_total),
            actual_total: money(act_total),
            variance_total: money(est_total - act_total),
            efc_total: money(efc_total),
            lines,
        }
    }
}

/// One purchase order for the PO view.
#[derive(Debug, Clone, Serialize)]
pub struct PurchaseOrderDto {
    pub id: String,
    pub account_number: String,
    pub account_name: String,
    pub date: String,
    pub vendor: String,
    pub description: String,
    pub amount: String,
    pub status: String,
}

/// Purchase-order list + committed totals by status.
#[derive(Debug, Clone, Serialize)]
pub struct PurchaseOrdersDto {
    pub orders: Vec<PurchaseOrderDto>,
    pub draft_total: String,
    pub approved_total: String,
    pub converted_total: String,
    /// Approved + converted — the committed cost a producer watches.
    pub committed_total: String,
}

impl PurchaseOrdersDto {
    pub fn build(budget: &Budget) -> Self {
        use crate::po::POStatus;
        let acct = |id: &crate::ids::AccountId| {
            budget
                .accounts
                .get(id)
                .map(|a| (a.number.clone(), a.description.tr.clone()))
                .unwrap_or_default()
        };
        let mut orders: Vec<crate::po::PurchaseOrder> =
            budget.purchase_orders.values().cloned().collect();
        orders.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.vendor.cmp(&b.vendor)));

        let sat = |a: Decimal, b: Decimal| a.checked_add(b).unwrap_or(Decimal::MAX);
        let (mut draft, mut approved, mut converted) =
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        for o in &orders {
            let amt = round_money(o.amount);
            match o.status {
                POStatus::Draft => draft = sat(draft, amt),
                POStatus::Approved => approved = sat(approved, amt),
                POStatus::Converted => converted = sat(converted, amt),
            }
        }
        let dtos = orders
            .into_iter()
            .map(|o| {
                let (number, name) = acct(&o.account);
                PurchaseOrderDto {
                    id: o.id.to_string(),
                    account_number: number,
                    account_name: name,
                    date: o.date,
                    vendor: o.vendor,
                    description: o.description,
                    amount: money(o.amount),
                    status: o.status.as_str().to_string(),
                }
            })
            .collect();
        PurchaseOrdersDto {
            orders: dtos,
            draft_total: money(draft),
            approved_total: money(approved),
            converted_total: money(converted),
            committed_total: money(sat(approved, converted)),
        }
    }
}

/// A stripboard strip for the schedule view.
#[derive(Debug, Clone, Serialize)]
pub struct StripDto {
    pub id: String,
    pub day: u32,
    pub scene: String,
    pub set: String,
    pub eighths: u32,
    pub elements: Vec<String>,
}

/// A Day-Out-of-Days row for the schedule view.
#[derive(Debug, Clone, Serialize)]
pub struct DoodRowDto {
    pub element: String,
    pub start_day: u32,
    pub finish_day: u32,
    pub work_days: u32,
    pub hold_days: u32,
}

/// Schedule view: strips (sorted by day) + Day-Out-of-Days + totals.
#[derive(Debug, Clone, Serialize)]
pub struct ScheduleDto {
    pub strips: Vec<StripDto>,
    pub dood: Vec<DoodRowDto>,
    pub total_days: u32,
    pub total_eighths: u32,
}

impl ScheduleDto {
    pub fn build(budget: &Budget) -> Self {
        let mut strips: Vec<crate::scheduling::Strip> = budget.strips.values().cloned().collect();
        strips.sort_by(|a, b| a.day.cmp(&b.day).then_with(|| a.scene.cmp(&b.scene)));
        let dood = crate::scheduling::day_out_of_days(&strips)
            .into_iter()
            .map(|r| DoodRowDto {
                element: r.element,
                start_day: r.start_day,
                finish_day: r.finish_day,
                work_days: r.work_days,
                hold_days: r.hold_days,
            })
            .collect();
        ScheduleDto {
            total_days: crate::scheduling::total_days(&strips),
            total_eighths: crate::scheduling::total_eighths(&strips),
            strips: strips
                .into_iter()
                .map(|s| StripDto {
                    id: s.id.to_string(),
                    day: s.day,
                    scene: s.scene,
                    set: s.set,
                    eighths: s.eighths,
                    elements: s.elements,
                })
                .collect(),
            dood,
        }
    }
}

/// One settlement receipt with its extracted VAT (kuruş strings).
#[derive(Debug, Clone, Serialize)]
pub struct ReceiptLineDto {
    pub id: String,
    pub date: String,
    pub vendor: String,
    pub receipt_no: String,
    pub category: String,
    pub description: String,
    pub gross: String,
    pub kdv: String,
    pub net: String,
}

/// One expense category's settlement rollup.
#[derive(Debug, Clone, Serialize)]
pub struct SettlementCategoryDto {
    pub category: String,
    pub gross: String,
    pub kdv: String,
    pub net: String,
}

/// Expense settlement ("Hesap Kapama"): per-category rollup, grand totals, and
/// the advance reconciliation, plus the underlying receipt lines.
#[derive(Debug, Clone, Serialize)]
pub struct SettlementReportDto {
    pub categories: Vec<SettlementCategoryDto>,
    pub gross_total: String,
    pub kdv_total: String,
    pub net_total: String,
    pub advance: String,
    pub balance: String,
    /// True when the advance covers the spend (balance ≥ 0 ⇒ refund to company).
    pub refund: bool,
    pub lines: Vec<ReceiptLineDto>,
}

impl SettlementReportDto {
    pub fn build(budget: &Budget, advance: Decimal) -> Self {
        use std::collections::BTreeMap;
        let mut receipts: Vec<crate::settlement::Receipt> =
            budget.receipts.values().cloned().collect();
        receipts.sort_by(|a, b| {
            a.category
                .cmp(&b.category)
                .then_with(|| a.date.cmp(&b.date))
        });

        // Round each receipt's columns to kuruş FIRST, then accumulate the
        // category and grand totals from those rounded values — so the rendered
        // table reconciles exactly (rows sum to categories sum to the totals).
        let sat = |a: Decimal, b: Decimal| a.checked_add(b).unwrap_or(Decimal::MAX);
        let mut cats: BTreeMap<String, (Decimal, Decimal, Decimal)> = BTreeMap::new();
        let (mut gt, mut kt, mut nt) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        let mut lines = Vec::with_capacity(receipts.len());
        for r in &receipts {
            let (kdv_raw, net_raw) = r.breakdown();
            let gross = round_money(r.gross);
            let kdv = round_money(kdv_raw);
            let net = round_money(net_raw);
            let e = cats.entry(r.category.clone()).or_insert((
                Decimal::ZERO,
                Decimal::ZERO,
                Decimal::ZERO,
            ));
            e.0 = sat(e.0, gross);
            e.1 = sat(e.1, kdv);
            e.2 = sat(e.2, net);
            gt = sat(gt, gross);
            kt = sat(kt, kdv);
            nt = sat(nt, net);
            lines.push(ReceiptLineDto {
                id: r.id.to_string(),
                date: r.date.clone(),
                vendor: r.vendor.clone(),
                receipt_no: r.receipt_no.clone(),
                category: r.category.clone(),
                description: r.description.clone(),
                gross: money(gross),
                kdv: money(kdv),
                net: money(net),
            });
        }

        let categories = cats
            .into_iter()
            .map(|(category, (gross, kdv, net))| SettlementCategoryDto {
                category,
                gross: money(gross),
                kdv: money(kdv),
                net: money(net),
            })
            .collect();

        let advance = round_money(advance);
        let balance = advance.checked_sub(gt).unwrap_or(Decimal::ZERO);
        SettlementReportDto {
            categories,
            gross_total: money(gt),
            kdv_total: money(kt),
            net_total: money(nt),
            advance: money(advance),
            balance: money(balance),
            refund: balance >= Decimal::ZERO,
            lines,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{evaluate, templates::dizi_full_template};

    #[test]
    fn national_sheet_reproduces_the_source_workbook() {
        let b = dizi_full_template("BOŞ BÜTÇE");
        let r = evaluate(&b);
        let sheet = NationalSheetDto::build(&b, &r);

        // DİREKT MALİYET + ATL/BTL straight from the BOŞ BÜTÇE workbook. With
        // round-first columns the grand is within a kuruş of the full-precision
        // ₺32,488,843.87 (they coincide here; the difference, if any, is one
        // kuruş, the price of on-screen columns that foot).
        assert_eq!(sheet.grand_total, "32488843.87");
        assert_eq!(sheet.atl_total, "10854170.17");
        assert_eq!(sheet.btl_total, "21634673.70");

        // The grand row and the two section rows are present and consistent.
        let grand = rows_of(&sheet, "grand");
        assert_eq!(grand.len(), 1);
        assert_eq!(grand[0].g_toplam, "32488843.87");
        assert_eq!(rows_of(&sheet, "section").len(), 2);
        // One "TOPLAM / …" per category (22 in the workbook).
        assert_eq!(rows_of(&sheet, "subtotal").len(), 22);

        // Every subtotal/section/grand row must self-foot EXACTLY: the printed
        // NET + STOPAJ + KOMİSYON equals the printed G.TOPLAM to the kuruş.
        for row in sheet
            .rows
            .iter()
            .filter(|r| matches!(r.kind.as_str(), "subtotal" | "section" | "grand"))
        {
            let sum: Decimal = [&row.net_tutar, &row.stopaj, &row.ek_komisyon]
                .iter()
                .map(|s| s.parse::<Decimal>().unwrap())
                .sum();
            assert_eq!(
                money(sum),
                row.g_toplam,
                "row '{}' does not foot",
                row.label
            );
        }
        // …and the DTO-level grand columns foot exactly too.
        let net: Decimal = sheet.net_grand.parse().unwrap();
        let stopaj: Decimal = sheet.stopaj_grand.parse().unwrap();
        let kom: Decimal = sheet.komisyon_grand.parse().unwrap();
        assert_eq!(money(net + stopaj + kom), sheet.grand_total);
    }

    #[test]
    fn national_sheet_first_line_matches_yonetmen() {
        let b = dizi_full_template("BOŞ BÜTÇE");
        let r = evaluate(&b);
        let sheet = NationalSheetDto::build(&b, &r);
        // Row 6 of the workbook: YÖNETMEN, adet 1, %17 stopaj, 660000 → 795180.72.
        let line = sheet
            .rows
            .iter()
            .find(|r| r.kind == "line" && r.label == "YÖNETMEN")
            .expect("YÖNETMEN line");
        assert_eq!(line.adet, "1");
        assert_eq!(line.vergi_orani.as_deref(), Some("0.17"));
        assert_eq!(line.birim_tutar.as_deref(), Some("660000.00"));
        assert_eq!(line.net_tutar, "660000.00");
        assert_eq!(line.g_toplam, "795180.72");
    }

    fn rows_of<'a>(s: &'a NationalSheetDto, kind: &str) -> Vec<&'a NationalRow> {
        s.rows.iter().filter(|r| r.kind == kind).collect()
    }

    // A category of gross-up lines whose per-line stopaj is a repeating decimal
    // (100 / 0.9 − 100 = 11.111…). Under the old display-rounding the STOPAJ
    // column would not foot (7 × 11.11 = 77.77 vs money(77.777…) = 77.78); the
    // round-first accumulation must make it foot exactly.
    #[test]
    fn national_sheet_columns_foot_with_sub_kurus_lines() {
        use crate::ids::*;
        use crate::{
            AppliedFringe, AtlBtl, Budget, Category, Detail, Formula, Fringe, FringeKind,
            FringeMode, Localized, PostingLevel, Unit,
        };
        use rust_decimal_macros::dec;

        let mut b = Budget::new("T", crate::templates::try_currency());
        let unit = Unit {
            id: UnitId::new(),
            code: "ADET".into(),
            name: Localized::bilingual("Adet", "Flat"),
            factor: Decimal::ONE,
        };
        let uid = unit.id;
        b.units.insert(uid, unit);
        let fringe = Fringe {
            id: FringeId::new(),
            code: "TR_STOPAJ".into(),
            name: Localized::bilingual("Stopaj", "Withholding"),
            kind: FringeKind::Percent,
            mode: FringeMode::GrossUp,
            rate: dec!(0.10),
            posting_level: PostingLevel::Detail,
            cutoff: None,
            cap: None,
            currency: None,
        };
        let fid = fringe.id;
        b.fringes.insert(fid, fringe);

        let cat = Category {
            id: CategoryId::new(),
            number: "1000".into(),
            description: Localized::tr("PERSONEL"),
            position: dec!(1),
            atl_btl: Some(AtlBtl::Atl),
            applied_fringes: vec![],
        };
        let cid = cat.id;
        b.categories.insert(cid, cat);
        let acc = crate::Account {
            id: AccountId::new(),
            category: cid,
            number: "1001".into(),
            description: Localized::tr("PERSONEL"),
            position: dec!(1),
            show_subtotal: true,
            applied_fringes: vec![],
        };
        let aid = acc.id;
        b.accounts.insert(aid, acc);
        for i in 0..7 {
            let d = Detail {
                id: DetailId::new(),
                account: aid,
                position: Decimal::from(i),
                description: format!("Kişi {i}"),
                name: None,
                amount: Formula::Const(dec!(100)),
                multiplier: Formula::Const(Decimal::ONE),
                rate: Formula::Const(Decimal::ONE),
                unit: uid,
                currency: b.base_currency,
                applied_fringes: vec![AppliedFringe::with_rate(fid, dec!(0.10))],
                groups: vec![],
                location: None,
                set: None,
                gl_code: None,
                notes: None,
            };
            b.details.insert(d.id, d);
        }

        let r = evaluate(&b);
        let sheet = NationalSheetDto::build(&b, &r);
        let sub = sheet
            .rows
            .iter()
            .find(|row| row.kind == "subtotal")
            .expect("subtotal row");

        // The subtotal foots to its own G.TOPLAM…
        let sum: Decimal = [&sub.net_tutar, &sub.stopaj, &sub.ek_komisyon]
            .iter()
            .map(|s| s.parse::<Decimal>().unwrap())
            .sum();
        assert_eq!(money(sum), sub.g_toplam);
        // …and the displayed line STOPAJ cells sum to the subtotal STOPAJ cell.
        let line_stopaj: Decimal = sheet
            .rows
            .iter()
            .filter(|row| row.kind == "line")
            .map(|row| row.stopaj.parse::<Decimal>().unwrap())
            .sum();
        assert_eq!(money(line_stopaj), sub.stopaj);
        // Per-line gross-up 100/0.9−100 = 11.11; 7 × 11.11 = 77.77.
        assert_eq!(sub.stopaj, "77.77");
    }
}
