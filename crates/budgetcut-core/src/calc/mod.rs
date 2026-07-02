//! The calculation engine (§6).
//!
//! Deterministic, side-effect-free evaluation of a [`Budget`] into computed
//! totals. The pipeline:
//!
//! 1. **Resolve globals** topologically, detecting reference cycles → `#ERR`.
//! 2. **Evaluate each detail**: `amount × unit_factor × multiplier × rate`,
//!    converted to the base currency.
//! 3. **Apply fringes** (additive *and* gross-up; per-line rate overrides;
//!    cutoff/cap), cascading budget→category→account→detail.
//! 4. **Roll up** Detail → Account → Category → ATL/BTL → Grand → −Credits → Net.
//!
//! Money is `Decimal` throughout; nothing is rounded until a caller asks for a
//! display value, so chained operations don't drift. Iteration order is always
//! sorted by `(position, id)` so two runs on identical input produce identical
//! output (§6 determinism, §18).

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;

use crate::expr::{self, Ast, EvalError};
use crate::ids::*;
use crate::model::*;

/// Identifies the cell that produced an evaluation error (for `#ERR` display).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellTarget {
    Global(GlobalId),
    DetailAmount(DetailId),
    DetailMultiplier(DetailId),
    DetailRate(DetailId),
    /// The line's computed value (e.g. the product overflowed).
    DetailValue(DetailId),
    DetailFringe(DetailId, FringeId),
    Charge(OpScopedId),
    Credit(OpScopedId),
}

/// Saturating decimal add — totals must never panic on overflow (§6: `evaluate`
/// is total). Per-line values are already bounded (an overflowing line degrades
/// to `#ERR` and contributes 0), so this only guards pathological aggregates.
fn sat_add(a: Decimal, b: Decimal) -> Decimal {
    a.checked_add(b).unwrap_or(if b.is_sign_negative() {
        Decimal::MIN
    } else {
        Decimal::MAX
    })
}

/// Saturating decimal subtract (see [`sat_add`]).
fn sat_sub(a: Decimal, b: Decimal) -> Decimal {
    a.checked_sub(b).unwrap_or(if b.is_sign_negative() {
        Decimal::MAX
    } else {
        Decimal::MIN
    })
}

/// An evaluation error pinned to the cell that caused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellError {
    pub target: CellTarget,
    pub error: EvalError,
}

/// Per-detail computed values, in base currency, full precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DetailCalc {
    /// `amount × unit_factor × multiplier × rate`, converted to base.
    pub subtotal: Decimal,
    /// Evaluated multiplier (the national sheet's *ADET* / quantity column).
    pub multiplier: Decimal,
    /// Evaluated per-unit rate (the national sheet's *BİRİM TUTAR* column).
    pub rate: Decimal,
    /// Sum of all fringes applied to this line.
    pub fringe_total: Decimal,
    /// `subtotal + fringe_total` (the Turkish `G.TOPLAM`).
    pub line_total: Decimal,
    /// `false` when a suppressed group (InBT off) excludes this line.
    pub included: bool,
    /// `true` when any formula/fringe on this line errored.
    pub error: bool,
}

/// A subtotal / fringe / total triple used at every rollup level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rollup {
    pub subtotal: Decimal,
    pub fringe_total: Decimal,
    pub total: Decimal,
}

impl Rollup {
    /// Accumulate a line's components. `total` is intentionally *not*
    /// accumulated from per-line `line_total`s — it is derived as
    /// `subtotal + fringe_total` in [`Rollup::finalize`]. Summing components
    /// separately and deriving the total keeps `total == subtotal + fringe`
    /// exact: when a running sum exceeds `Decimal`'s 28 significant digits it
    /// rounds, and `Σ(subtotalᵢ + fringeᵢ)` would re-associate differently from
    /// `Σ subtotalᵢ + Σ fringeᵢ`, disagreeing in the last digit. Sums are
    /// saturating ([`sat_add`]) so a magnitude overflow clamps instead of
    /// panicking.
    fn add_detail(&mut self, d: &DetailCalc) {
        if d.included {
            self.subtotal = sat_add(self.subtotal, d.subtotal);
            self.fringe_total = sat_add(self.fringe_total, d.fringe_total);
        }
    }

    /// Derive `total` from the accumulated components.
    fn finalize(&mut self) {
        self.total = sat_add(self.subtotal, self.fringe_total);
    }

    fn add_components(&mut self, other: &Rollup) {
        self.subtotal = sat_add(self.subtotal, other.subtotal);
        self.fringe_total = sat_add(self.fringe_total, other.fringe_total);
    }
}

/// Fringe totals grouped by posting level (for the topsheet breakdown, §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FringeBreakdown {
    pub budget: Decimal,
    pub production: Decimal,
    pub category: Decimal,
    pub account: Decimal,
    pub detail: Decimal,
}

impl FringeBreakdown {
    fn add(&mut self, level: PostingLevel, amt: Decimal) {
        match level {
            PostingLevel::Budget => self.budget += amt,
            PostingLevel::Production => self.production += amt,
            PostingLevel::Category => self.category += amt,
            PostingLevel::Account => self.account += amt,
            PostingLevel::Detail => self.detail += amt,
        }
    }

    #[must_use]
    pub fn total(&self) -> Decimal {
        self.budget + self.production + self.category + self.account + self.detail
    }
}

/// The full computed state of a budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalcResult {
    pub global_values: HashMap<String, Result<Decimal, EvalError>>,
    pub details: HashMap<DetailId, DetailCalc>,
    pub accounts: HashMap<AccountId, Rollup>,
    pub categories: HashMap<CategoryId, Rollup>,
    pub fringe_breakdown: FringeBreakdown,
    pub atl: Rollup,
    pub btl: Rollup,
    /// Grand rollup of all categories (subtotal + fringes).
    pub total: Rollup,
    pub charges_total: Decimal,
    /// `total.total + charges_total`.
    pub grand_total: Decimal,
    pub credits_total: Decimal,
    /// `grand_total - credits_total`.
    pub net_total: Decimal,
    pub errors: Vec<CellError>,
}

impl CalcResult {
    /// Convenience: the computed result for a detail (zeroed if absent).
    #[must_use]
    pub fn detail(&self, id: DetailId) -> DetailCalc {
        self.details.get(&id).copied().unwrap_or_default()
    }

    /// Whether any cell errored (`#ERR`).
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Evaluate a budget into its computed totals. Pure and deterministic.
#[must_use]
pub fn evaluate(budget: &Budget) -> CalcResult {
    let mut errors = Vec::new();
    let global_values = resolve_globals(budget, &mut errors);

    // Resolver for detail formulas: globals only (a detail can't reference
    // another detail). Missing names surface as #ERR.
    let resolve = |name: &str| -> Result<Decimal, EvalError> {
        match global_values.get(name) {
            Some(Ok(v)) => Ok(*v),
            Some(Err(e)) => Err(e.clone()),
            None => Err(EvalError::UnknownRef(name.to_string())),
        }
    };

    let mut details: HashMap<DetailId, DetailCalc> = HashMap::new();
    let mut accounts: HashMap<AccountId, Rollup> = HashMap::new();
    let mut categories: HashMap<CategoryId, Rollup> = HashMap::new();
    let mut fringe_breakdown = FringeBreakdown::default();
    // The grand rollup is accumulated directly from the detail leaves, so it
    // always reflects every included line — even one whose account/category id
    // is dangling (which validation flags but the engine must total robustly).
    let mut grand = Rollup::default();

    // Evaluate details in deterministic order.
    let mut detail_ids: Vec<&Detail> = budget.details.values().collect();
    detail_ids.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));

    for d in detail_ids {
        let calc = eval_detail(budget, d, &resolve, &mut fringe_breakdown, &mut errors);
        details.insert(d.id, calc);
        grand.add_detail(&calc);

        // Roll into account / category (best-effort topsheet view).
        accounts.entry(d.account).or_default().add_detail(&calc);
        if let Some(acc) = budget.accounts.get(&d.account) {
            categories
                .entry(acc.category)
                .or_default()
                .add_detail(&calc);
        }
    }
    grand.finalize();

    // Ensure every account/category has an entry (even if empty), then derive
    // each rollup's total from its accumulated components.
    for a in budget.accounts.keys() {
        accounts.entry(*a).or_default();
    }
    for c in budget.categories.keys() {
        categories.entry(*c).or_default();
    }
    for r in accounts.values_mut() {
        r.finalize();
    }
    for r in categories.values_mut() {
        r.finalize();
    }

    // ATL / BTL split (best-effort, from the category rollups). The grand
    // `total` is taken from the detail-derived `grand` so it can't be skewed by
    // a dangling category; for a well-formed budget `atl + btl == total`.
    let (atl, btl) = split_atl_btl(budget, &categories);
    let total = grand;

    // Charges (added after grand total) and credits (subtracted to net).
    let mut charges_total = Decimal::ZERO;
    for ch in sorted_by_pos(budget.charges.values(), |c| (c.position, c.id)) {
        match eval_formula(&ch.amount, &resolve) {
            Ok(v) => charges_total = sat_add(charges_total, v),
            Err(e) => errors.push(CellError {
                target: CellTarget::Charge(ch.id),
                error: e,
            }),
        }
    }
    let mut credits_total = Decimal::ZERO;
    for cr in sorted_by_pos(budget.credits.values(), |c| (c.position, c.id)) {
        match eval_formula(&cr.amount, &resolve) {
            Ok(v) => credits_total = sat_add(credits_total, v),
            Err(e) => errors.push(CellError {
                target: CellTarget::Credit(cr.id),
                error: e,
            }),
        }
    }

    let grand_total = sat_add(total.total, charges_total);
    let net_total = sat_sub(grand_total, credits_total);

    CalcResult {
        global_values,
        details,
        accounts,
        categories,
        fringe_breakdown,
        atl,
        btl,
        total,
        charges_total,
        grand_total,
        credits_total,
        net_total,
        errors,
    }
}

/// Evaluate one detail line into base-currency values, applying its fringes.
fn eval_detail<F>(
    budget: &Budget,
    d: &Detail,
    resolve: &F,
    breakdown: &mut FringeBreakdown,
    errors: &mut Vec<CellError>,
) -> DetailCalc
where
    F: Fn(&str) -> Result<Decimal, EvalError>,
{
    let mut had_error = false;

    let amount = match eval_formula(&d.amount, resolve) {
        Ok(v) => v,
        Err(e) => {
            errors.push(CellError {
                target: CellTarget::DetailAmount(d.id),
                error: e,
            });
            had_error = true;
            Decimal::ZERO
        }
    };
    let multiplier = match eval_formula(&d.multiplier, resolve) {
        Ok(v) => v,
        Err(e) => {
            errors.push(CellError {
                target: CellTarget::DetailMultiplier(d.id),
                error: e,
            });
            had_error = true;
            Decimal::ONE
        }
    };
    let rate = match eval_formula(&d.rate, resolve) {
        Ok(v) => v,
        Err(e) => {
            errors.push(CellError {
                target: CellTarget::DetailRate(d.id),
                error: e,
            });
            had_error = true;
            Decimal::ZERO
        }
    };

    let unit_factor = budget.units.get(&d.unit).map_or(Decimal::ONE, |u| u.factor);
    let fx = budget
        .currencies
        .get(&d.currency)
        .map_or(Decimal::ONE, |c| c.rate_to_base);

    // Checked product: an overflowing line degrades to #ERR (contributing 0)
    // rather than panicking the whole calc (§6: `evaluate` is total).
    let subtotal = match amount
        .checked_mul(unit_factor)
        .and_then(|x| x.checked_mul(multiplier))
        .and_then(|x| x.checked_mul(rate))
        .and_then(|x| x.checked_mul(fx))
    {
        Some(v) => v,
        None => {
            errors.push(CellError {
                target: CellTarget::DetailValue(d.id),
                error: EvalError::Overflow,
            });
            had_error = true;
            Decimal::ZERO
        }
    };

    // Suppressed if any of the detail's groups has InBT off.
    let included = !d.groups.iter().any(|g| {
        budget
            .groups
            .get(g)
            .map(|grp| !grp.in_budget_total)
            .unwrap_or(false)
    });

    // Effective fringes: budget → category → account → detail (cascade).
    let mut fringe_total = Decimal::ZERO;
    for af in effective_fringes(budget, d) {
        // A fringe referenced (at any cascade level) but no longer in the
        // budget is an error, not a silent skip.
        let Some(fringe) = budget.fringes.get(&af.fringe_id) else {
            errors.push(CellError {
                target: CellTarget::DetailFringe(d.id, af.fringe_id),
                error: EvalError::UnknownRef(format!("fringe {}", af.fringe_id)),
            });
            had_error = true;
            continue;
        };
        match fringe_amount(fringe, &af, subtotal, budget) {
            Ok(amt) => {
                fringe_total = sat_add(fringe_total, amt);
                if included {
                    breakdown.add(fringe.posting_level, amt);
                }
            }
            Err(e) => {
                errors.push(CellError {
                    target: CellTarget::DetailFringe(d.id, fringe.id),
                    error: e,
                });
                had_error = true;
            }
        }
    }

    DetailCalc {
        subtotal,
        multiplier,
        rate,
        fringe_total,
        line_total: sat_add(subtotal, fringe_total),
        included,
        error: had_error,
    }
}

/// Split a detail's fringe total into gross-up (Turkish *stopaj*) and additive
/// (*ek ücret / komisyon*) buckets, with the first effective rate seen for each
/// — the two right-hand columns of the national dizi sheet. Reuses the same
/// [`effective_fringes`] + [`fringe_amount`] path as the rollup, so the split
/// always sums back to `DetailCalc::fringe_total`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FringeSplit {
    pub grossup: Decimal,
    pub additive: Decimal,
    pub grossup_rate: Option<Decimal>,
    pub additive_rate: Option<Decimal>,
}

#[must_use]
pub fn detail_fringe_split(budget: &Budget, d: &Detail, subtotal: Decimal) -> FringeSplit {
    let mut s = FringeSplit::default();
    for af in effective_fringes(budget, d) {
        let Some(fringe) = budget.fringes.get(&af.fringe_id) else {
            continue;
        };
        let Ok(amt) = fringe_amount(fringe, &af, subtotal, budget) else {
            continue;
        };
        let rate = af.rate_override.unwrap_or(fringe.rate);
        match fringe.mode {
            FringeMode::GrossUp => {
                s.grossup = sat_add(s.grossup, amt);
                if s.grossup_rate.is_none() {
                    s.grossup_rate = Some(rate);
                }
            }
            FringeMode::Additive => {
                s.additive = sat_add(s.additive, amt);
                if s.additive_rate.is_none() {
                    s.additive_rate = Some(rate);
                }
            }
        }
    }
    s
}

/// Gather every fringe that applies to a detail, from all cascade levels.
fn effective_fringes(budget: &Budget, d: &Detail) -> Vec<AppliedFringe> {
    let mut out = Vec::new();
    out.extend(budget.applied_fringes.iter().cloned());
    if let Some(acc) = budget.accounts.get(&d.account) {
        if let Some(cat) = budget.categories.get(&acc.category) {
            out.extend(cat.applied_fringes.iter().cloned());
        }
        out.extend(acc.applied_fringes.iter().cloned());
    }
    out.extend(d.applied_fringes.iter().cloned());
    out
}

/// Compute a single fringe's contribution on `base` (already in base currency).
fn fringe_amount(
    fringe: &Fringe,
    applied: &AppliedFringe,
    base: Decimal,
    budget: &Budget,
) -> Result<Decimal, EvalError> {
    // Base capped at cutoff before the rate applies.
    let effective_base = match fringe.cutoff {
        Some(c) if base > c => c,
        _ => base,
    };

    let raw = match fringe.kind {
        FringeKind::Percent => {
            let rate = applied.rate_override.unwrap_or(fringe.rate);
            match fringe.mode {
                FringeMode::Additive => effective_base
                    .checked_mul(rate)
                    .ok_or(EvalError::Overflow)?,
                FringeMode::GrossUp => {
                    let denom = Decimal::ONE - rate;
                    if denom <= Decimal::ZERO {
                        return Err(EvalError::DivByZero);
                    }
                    effective_base
                        .checked_div(denom)
                        .and_then(|brut| brut.checked_sub(effective_base))
                        .ok_or(EvalError::Overflow)?
                }
            }
        }
        FringeKind::Flat => {
            // Flat amount in the fringe's currency, converted to base. (`cutoff`
            // caps the *base* and is meaningless for a flat amount, so it is
            // intentionally not applied here — see ADR 0004 / Fringe docs.)
            let amt = applied.rate_override.unwrap_or(fringe.rate);
            let fx = fringe
                .currency
                .and_then(|c| budget.currencies.get(&c))
                .map_or(Decimal::ONE, |c| c.rate_to_base);
            amt.checked_mul(fx).ok_or(EvalError::Overflow)?
        }
    };

    Ok(match fringe.cap {
        Some(cap) if raw > cap => cap,
        _ => raw,
    })
}

/// Resolve all globals, detecting cycles and missing references.
fn resolve_globals(
    budget: &Budget,
    errors: &mut Vec<CellError>,
) -> HashMap<String, Result<Decimal, EvalError>> {
    // Parse every global formula into an AST keyed by name.
    let mut asts: HashMap<String, Result<Ast, EvalError>> = HashMap::new();
    let mut name_to_id: HashMap<String, GlobalId> = HashMap::new();
    for g in budget.globals.values() {
        name_to_id.insert(g.name.clone(), g.id);
        let ast = match &g.value {
            Formula::Const(d) => Ok(Ast::Num(*d)),
            Formula::Expr(s) => expr::parse(s),
        };
        asts.insert(g.name.clone(), ast);
    }

    let mut results: HashMap<String, Result<Decimal, EvalError>> = HashMap::new();
    // Deterministic resolution order.
    let mut names: Vec<String> = asts.keys().cloned().collect();
    names.sort();
    for name in &names {
        let mut visiting = HashSet::new();
        let r = eval_global(name, &asts, &mut results, &mut visiting);
        if let Err(e) = &r {
            if let Some(id) = name_to_id.get(name) {
                errors.push(CellError {
                    target: CellTarget::Global(*id),
                    error: e.clone(),
                });
            }
        }
    }
    results
}

fn eval_global(
    name: &str,
    asts: &HashMap<String, Result<Ast, EvalError>>,
    results: &mut HashMap<String, Result<Decimal, EvalError>>,
    visiting: &mut HashSet<String>,
) -> Result<Decimal, EvalError> {
    if let Some(r) = results.get(name) {
        return r.clone();
    }
    if !asts.contains_key(name) {
        return Err(EvalError::UnknownRef(name.to_string()));
    }
    if visiting.contains(name) {
        return Err(EvalError::Cycle(name.to_string()));
    }
    visiting.insert(name.to_string());
    let res = match &asts[name] {
        Err(e) => Err(e.clone()),
        Ok(ast) => eval_node(ast, asts, results, visiting),
    };
    visiting.remove(name);
    results.insert(name.to_string(), res.clone());
    res
}

fn eval_node(
    ast: &Ast,
    asts: &HashMap<String, Result<Ast, EvalError>>,
    results: &mut HashMap<String, Result<Decimal, EvalError>>,
    visiting: &mut HashSet<String>,
) -> Result<Decimal, EvalError> {
    Ok(match ast {
        Ast::Num(n) => *n,
        Ast::Ref(name) => eval_global(name, asts, results, visiting)?,
        Ast::Neg(a) => -eval_node(a, asts, results, visiting)?,
        Ast::Add(a, b) => {
            eval_node(a, asts, results, visiting)? + eval_node(b, asts, results, visiting)?
        }
        Ast::Sub(a, b) => {
            eval_node(a, asts, results, visiting)? - eval_node(b, asts, results, visiting)?
        }
        Ast::Mul(a, b) => {
            eval_node(a, asts, results, visiting)? * eval_node(b, asts, results, visiting)?
        }
        Ast::Div(a, b) => {
            let d = eval_node(b, asts, results, visiting)?;
            if d.is_zero() {
                return Err(EvalError::DivByZero);
            }
            eval_node(a, asts, results, visiting)? / d
        }
    })
}

fn eval_formula<F>(f: &Formula, resolve: &F) -> Result<Decimal, EvalError>
where
    F: Fn(&str) -> Result<Decimal, EvalError>,
{
    match f {
        Formula::Const(d) => Ok(*d),
        Formula::Expr(s) => expr::parse(s)?.eval(resolve),
    }
}

/// Whether a category is Above-The-Line. The authoritative boundary is the
/// position of the first [`ProductionTotal`] (categories before it are ATL);
/// with no production total, fall back to the category's [`AtlBtl`] hint. Shared
/// by [`split_atl_btl`] and the national-sheet view so their ATL/BTL splits can
/// never disagree for the same budget.
#[must_use]
pub fn category_is_atl(budget: &Budget, cat: &Category) -> bool {
    let boundary = budget
        .production_totals
        .values()
        .map(|pt| pt.position)
        .min();
    match boundary {
        Some(b) => cat.position < b,
        None => matches!(cat.atl_btl, Some(AtlBtl::Atl)),
    }
}

/// The Netflix cost-report / topsheet reporting bucket for an account or
/// category, derived purely from its **code**. A display/rollup grouping only —
/// it does not touch the stored `atl_btl` classification or the ProductionTotal
/// boundary (those stay authoritative for the real ATL/BTL split).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetflixGroup {
    Atl,
    BtlProduction,
    Post,
    Music,
    Vfx,
    Other,
    MiscIncentives,
}

impl NetflixGroup {
    /// Stable key used in DTOs and mapped to a localized label in the UI.
    #[must_use]
    pub fn key(&self) -> &'static str {
        match self {
            NetflixGroup::Atl => "ATL",
            NetflixGroup::BtlProduction => "BTL",
            NetflixGroup::Post => "POST",
            NetflixGroup::Music => "MUSIC",
            NetflixGroup::Vfx => "VFX",
            NetflixGroup::Other => "OTHER",
            NetflixGroup::MiscIncentives => "MISC",
        }
    }
}

/// Map a Netflix CoA code (category or account number, e.g. `"1100"`, `"6100"`)
/// to its reporting group. Total and panic-free: leading non-digits/garbage and
/// unknown bands fall through to [`NetflixGroup::Other`] so no line is dropped.
/// The two 6xxx bands are checked before the generic ranges so MUSIC (6000) and
/// VFX (6100) are never conflated.
#[must_use]
pub fn nflx_group(number: &str) -> NetflixGroup {
    let digits: String = number
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let Ok(n) = digits.parse::<u32>() else {
        return NetflixGroup::Other;
    };
    match n {
        1000..=1999 => NetflixGroup::Atl,
        2000..=4999 => NetflixGroup::BtlProduction,
        5000..=5999 => NetflixGroup::Post,
        6000..=6099 => NetflixGroup::Music,
        6100..=6199 => NetflixGroup::Vfx,
        7900 | 8100 | 8000..=8199 => NetflixGroup::MiscIncentives,
        7000..=7999 => NetflixGroup::Other,
        6200..=6999 => NetflixGroup::Other,
        _ => NetflixGroup::Other,
    }
}

/// Days since the civil epoch (1970-01-01) for a `YYYY-MM-DD`, `DD.MM.YYYY` or
/// `DD/MM/YYYY` date string. `None` for blank/garbage — callers route undated
/// rows to the "to-date"/spillover bucket rather than dropping them. Hand-rolled
/// (core carries no `chrono` dependency) via Howard Hinnant's `days_from_civil`.
#[must_use]
pub fn parse_iso_days(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (y, m, d) = match t.split_once('-') {
        // YYYY-MM-DD — only when the first field is a 4-digit year, so a
        // dash-separated day-first date ("08-03-2021") is rejected as None
        // rather than silently misparsed.
        Some((y, rest)) if y.len() == 4 => {
            let (mo, da) = rest.split_once('-')?;
            (
                y.parse::<i64>().ok()?,
                mo.parse::<u32>().ok()?,
                da.trim().get(..2).unwrap_or(da).parse::<u32>().ok()?,
            )
        }
        Some(_) => return None,
        // DD.MM.YYYY or DD/MM/YYYY
        None => {
            let sep = if t.contains('.') {
                '.'
            } else if t.contains('/') {
                '/'
            } else {
                return None;
            };
            let mut it = t.split(sep);
            let da = it.next()?.parse::<u32>().ok()?;
            let mo = it.next()?.parse::<u32>().ok()?;
            let y = it.next()?.trim().parse::<i64>().ok()?;
            (y, mo, da)
        }
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = ((m + 9) % 12) as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// Inverse of [`parse_iso_days`]: render a civil-epoch day count back to a
/// `YYYY-MM-DD` string (used for cash-flow week-ending column labels).
#[must_use]
pub fn iso_from_days(days: i64) -> String {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Determine the ATL/BTL split from the category rollups.
fn split_atl_btl(budget: &Budget, categories: &HashMap<CategoryId, Rollup>) -> (Rollup, Rollup) {
    let mut atl = Rollup::default();
    let mut btl = Rollup::default();

    for cat in budget.categories_sorted() {
        let r = categories.get(&cat.id).copied().unwrap_or_default();
        if category_is_atl(budget, cat) {
            atl.add_components(&r);
        } else {
            btl.add_components(&r);
        }
    }
    atl.finalize();
    btl.finalize();
    (atl, btl)
}

fn sorted_by_pos<'a, T, K: Ord, I>(iter: I, key: impl Fn(&T) -> K) -> Vec<&'a T>
where
    I: IntoIterator<Item = &'a T>,
{
    let mut v: Vec<&T> = iter.into_iter().collect();
    v.sort_by_key(|a| key(a));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::try_currency;
    use rust_decimal_macros::dec;

    fn budget_with_unit() -> (Budget, UnitId) {
        let mut b = Budget::new("T", try_currency());
        let unit = Unit {
            id: UnitId::new(),
            code: "FLAT".into(),
            name: Localized::tr("Götürü"),
            factor: Decimal::ONE,
        };
        let uid = unit.id;
        b.units.insert(uid, unit);
        (b, uid)
    }

    #[test]
    fn global_cycle_surfaces_as_error() {
        let (mut b, _u) = budget_with_unit();
        let a = Global {
            id: GlobalId::new(),
            name: "A".into(),
            description: Localized::tr(""),
            value: Formula::expr("B + 1"),
            in_budget_total: true,
        };
        let bg = Global {
            id: GlobalId::new(),
            name: "B".into(),
            description: Localized::tr(""),
            value: Formula::expr("A + 1"),
            in_budget_total: true,
        };
        b.globals.insert(a.id, a);
        b.globals.insert(bg.id, bg);
        let res = evaluate(&b);
        assert!(res.has_errors());
        assert!(res
            .errors
            .iter()
            .any(|e| matches!(e.error, EvalError::Cycle(_))));
    }

    #[test]
    fn global_chain_resolves() {
        let (mut b, uid) = budget_with_unit();
        for (n, v) in [
            ("DAYS", Formula::Const(dec!(30))),
            ("OT", Formula::expr("DAYS * 2")),
        ] {
            let g = Global {
                id: GlobalId::new(),
                name: n.into(),
                description: Localized::tr(""),
                value: v,
                in_budget_total: true,
            };
            b.globals.insert(g.id, g);
        }
        let cat = Category {
            id: CategoryId::new(),
            number: "1".into(),
            description: Localized::tr(""),
            position: dec!(1),
            atl_btl: None,
            applied_fringes: vec![],
        };
        let acc = Account {
            id: AccountId::new(),
            category: cat.id,
            number: "1".into(),
            description: Localized::tr(""),
            position: dec!(1),
            show_subtotal: true,
            applied_fringes: vec![],
        };
        let det = Detail {
            id: DetailId::new(),
            account: acc.id,
            position: dec!(1),
            description: "x".into(),
            name: None,
            amount: Formula::expr("OT"),
            multiplier: Formula::Const(Decimal::ONE),
            rate: Formula::Const(dec!(100)),
            unit: uid,
            currency: b.base_currency,
            applied_fringes: vec![],
            groups: vec![],
            location: None,
            set: None,
            gl_code: None,
            notes: None,
        };
        let did = det.id;
        b.categories.insert(cat.id, cat);
        b.accounts.insert(acc.id, acc);
        b.details.insert(det.id, det);
        let res = evaluate(&b);
        assert!(!res.has_errors());
        // OT = 60, rate 100 => 6000
        assert_eq!(res.detail(did).subtotal, dec!(6000));
    }

    #[test]
    fn suppressed_group_excluded_from_rollup() {
        let (mut b, uid) = budget_with_unit();
        let grp = Group {
            id: GroupId::new(),
            code: "HIDE".into(),
            name: Localized::tr("Gizli"),
            in_budget_total: false,
            color: None,
            priority: 0,
        };
        let gid = grp.id;
        b.groups.insert(gid, grp);
        let cat = Category {
            id: CategoryId::new(),
            number: "1".into(),
            description: Localized::tr(""),
            position: dec!(1),
            atl_btl: None,
            applied_fringes: vec![],
        };
        let acc = Account {
            id: AccountId::new(),
            category: cat.id,
            number: "1".into(),
            description: Localized::tr(""),
            position: dec!(1),
            show_subtotal: true,
            applied_fringes: vec![],
        };
        let det = Detail {
            id: DetailId::new(),
            account: acc.id,
            position: dec!(1),
            description: "x".into(),
            name: None,
            amount: Formula::Const(dec!(1)),
            multiplier: Formula::Const(Decimal::ONE),
            rate: Formula::Const(dec!(500)),
            unit: uid,
            currency: b.base_currency,
            applied_fringes: vec![],
            groups: vec![gid],
            location: None,
            set: None,
            gl_code: None,
            notes: None,
        };
        let cid = cat.id;
        b.categories.insert(cat.id, cat);
        b.accounts.insert(acc.id, acc);
        b.details.insert(det.id, det);
        let res = evaluate(&b);
        assert_eq!(res.categories[&cid].subtotal, dec!(0));
        assert_eq!(res.total.subtotal, dec!(0));
    }

    #[test]
    fn nflx_group_maps_code_bands_incl_6xxx_specials() {
        use NetflixGroup::*;
        assert_eq!(nflx_group("1100"), Atl);
        assert_eq!(nflx_group("1999"), Atl);
        assert_eq!(nflx_group("2000"), BtlProduction);
        assert_eq!(nflx_group("4999"), BtlProduction);
        assert_eq!(nflx_group("5000"), Post);
        assert_eq!(nflx_group("5999"), Post);
        assert_eq!(nflx_group("6000"), Music); // MUSIC before the 6xxx band
        assert_eq!(nflx_group("6099"), Music);
        assert_eq!(nflx_group("6100"), Vfx); // VFX, not conflated with MUSIC
        assert_eq!(nflx_group("6199"), Vfx);
        assert_eq!(nflx_group("6200"), Other);
        assert_eq!(nflx_group("7000"), Other);
        assert_eq!(nflx_group("7900"), MiscIncentives);
        assert_eq!(nflx_group("8100"), MiscIncentives);
        // Total & panic-free on junk.
        assert_eq!(nflx_group(""), Other);
        assert_eq!(nflx_group("abc"), Other);
        assert_eq!(nflx_group("1100-ATL"), Atl);
    }

    #[test]
    fn iso_date_parses_all_formats_and_roundtrips() {
        let a = parse_iso_days("2021-03-08").unwrap();
        assert_eq!(parse_iso_days("08.03.2021"), Some(a));
        assert_eq!(parse_iso_days("08/03/2021"), Some(a));
        // A week later is +7 days.
        assert_eq!(parse_iso_days("2021-03-15"), Some(a + 7));
        assert_eq!(parse_iso_days(""), None);
        assert_eq!(parse_iso_days("not a date"), None);
        assert_eq!(parse_iso_days("2021-13-01"), None); // bad month
                                                        // Dash-separated day-first is ambiguous → rejected (not misparsed).
        assert_eq!(parse_iso_days("08-03-2021"), None);
        assert_eq!(iso_from_days(a), "2021-03-08");
        assert_eq!(iso_from_days(0), "1970-01-01"); // civil epoch
    }
}
