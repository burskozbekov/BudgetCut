//! The domain model (§5). Pure data: `serde`-serializable structs and enums,
//! no I/O, no platform deps. Mirrors Movie Magic Budgeting's hierarchy
//! (Topsheet → Category → Account → Detail) plus the reusable Setup Tools
//! (Fringes, Globals, Units, Groups, Locations, Sets, Currencies).
//!
//! Ordering within a parent is carried by a fractional [`Position`] on each
//! entity rather than a parent-held vector. Inserting "between" two siblings is
//! then just picking a position in the gap — which is also LWW-friendly, since
//! position is just another last-write-wins field (§8) and reordering never
//! contends on a shared list.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::ids::*;

/// Fractional sort key. Children sort by `(position, id)`.
pub type Position = Decimal;

/// A user-facing string with locale variants. Turkish is primary (§12); other
/// locales are optional and fall back to `tr`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Localized {
    pub tr: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub en: Option<String>,
}

impl Localized {
    pub fn tr(s: impl Into<String>) -> Self {
        Self {
            tr: s.into(),
            en: None,
        }
    }

    pub fn bilingual(tr: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            tr: tr.into(),
            en: Some(en.into()),
        }
    }

    /// Resolve for a locale code ("tr", "en"); falls back to Turkish.
    #[must_use]
    pub fn get(&self, locale: &str) -> &str {
        match locale {
            "en" => self.en.as_deref().unwrap_or(&self.tr),
            _ => &self.tr,
        }
    }
}

/// A numeric input that is either a literal or an expression referencing
/// [`Global`]s (e.g. `"SHOOT_DAYS * 1.2"`). Used for a detail's amount /
/// multiplier / rate and for a global's own value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Formula {
    Const(Decimal),
    Expr(String),
}

impl Formula {
    #[must_use]
    pub fn constant(d: Decimal) -> Self {
        Formula::Const(d)
    }

    #[must_use]
    pub fn expr(s: impl Into<String>) -> Self {
        Formula::Expr(s.into())
    }
}

impl Default for Formula {
    fn default() -> Self {
        Formula::Const(Decimal::ZERO)
    }
}

impl From<Decimal> for Formula {
    fn from(d: Decimal) -> Self {
        Formula::Const(d)
    }
}

// ---------------------------------------------------------------------------
// Setup Tools
// ---------------------------------------------------------------------------

/// Whether a fringe rate is a percentage of the base or a flat amount per unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FringeKind {
    /// `rate` is a fraction (0.17 == 17%).
    Percent,
    /// `rate` is a flat money amount (in the fringe's currency) per applied line.
    Flat,
}

/// How a fringe combines with its base. This distinction comes straight from
/// real Turkish budgets: **stopaj** (income-tax withholding) is a *gross-up*
/// (`brüt = net / (1 − r)`), whereas SGK / agency commission is *additive*
/// (`kom = net × r`). MMB only models the additive case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FringeMode {
    /// Fringe amount = `base × rate` (or flat). Added on top of the base.
    Additive,
    /// Fringe amount = `base / (1 − rate) − base`. The net is grossed up so the
    /// recipient nets `base` after the withholding is deducted.
    GrossUp,
}

/// Where in the rollup a fringe's total is posted/aggregated (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostingLevel {
    Budget,
    Production,
    Category,
    Account,
    Detail,
}

/// A reusable fringe tool (payroll taxes, insurance, pension, union dues,
/// withholding, commission…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fringe {
    pub id: FringeId,
    /// Language-neutral stable code, e.g. `"TR_STOPAJ"`, `"TR_SGK_ISVEREN"`.
    pub code: String,
    pub name: Localized,
    pub kind: FringeKind,
    pub mode: FringeMode,
    /// Default rate (fraction for [`FringeKind::Percent`], money for `Flat`).
    pub rate: Decimal,
    pub posting_level: PostingLevel,
    /// Base is capped at this amount (in **base currency**) before the rate is
    /// applied. Applies to [`FringeKind::Percent`] only — it is meaningless for
    /// a flat amount and is ignored there. `None` = no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cutoff: Option<Decimal>,
    /// The resulting fringe amount is capped at this value, in **base
    /// currency** (the fringe amount is already converted to base before the
    /// comparison). `None` = no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<Decimal>,
    /// Currency for [`FringeKind::Flat`] amounts (defaults to base when None).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<CurrencyId>,
}

/// A fringe applied to a specific detail/account/category, optionally with a
/// per-line rate override. The Turkish budget's per-row `VERGİ ORANI` and
/// `KOM. ORANI` columns are exactly these overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedFringe {
    pub fringe_id: FringeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_override: Option<Decimal>,
}

impl AppliedFringe {
    #[must_use]
    pub fn new(fringe_id: FringeId) -> Self {
        Self {
            fringe_id,
            rate_override: None,
        }
    }

    #[must_use]
    pub fn with_rate(fringe_id: FringeId, rate: Decimal) -> Self {
        Self {
            fringe_id,
            rate_override: Some(rate),
        }
    }
}

/// A named variable usable inside any [`Formula`] (§5/§6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Global {
    pub id: GlobalId,
    /// The identifier used in expressions, e.g. `SHOOT_DAYS`. Case-sensitive.
    pub name: String,
    pub description: Localized,
    pub value: Formula,
    /// Movie Magic "InBT" flag. Currently **informational metadata** — the calc
    /// engine always resolves a global so any formula referencing it stays
    /// valid; this flag does not (yet) suppress anything. (Group suppression,
    /// which *does* affect rollups, lives on [`Group::in_budget_total`].)
    #[serde(default = "default_true")]
    pub in_budget_total: bool,
}

/// Defines the unit-factor used in calc (DAY / WEEK / HOUR / FLAT / % / custom).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub code: String,
    pub name: Localized,
    /// Multiplier applied in the detail formula (DAY/FLAT = 1, WEEK = 7, …).
    pub factor: Decimal,
}

/// Tags detail lines for filtering, color-coding, priority, and suppression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub code: String,
    pub name: Localized,
    /// "InBT" off ⇒ lines in this group are excluded from all rollups (§6).
    #[serde(default = "default_true")]
    pub in_budget_total: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub id: LocationId,
    pub code: String,
    pub name: Localized,
}

/// A filming set. Named `SetEntity` to avoid clashing with `std::collections`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetEntity {
    pub id: SetId,
    pub code: String,
    pub name: Localized,
}

/// A currency with a conversion rate into the budget's base currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Currency {
    pub id: CurrencyId,
    /// ISO 4217-ish code, e.g. `"TRY"`, `"USD"`.
    pub code: String,
    pub name: Localized,
    pub symbol: String,
    /// `amount_in_base = amount × rate_to_base`. Base currency has `1`.
    pub rate_to_base: Decimal,
    #[serde(default)]
    pub is_base: bool,
}

// ---------------------------------------------------------------------------
// Budget body
// ---------------------------------------------------------------------------

/// Top-level topsheet row; rolls up its accounts (§5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: CategoryId,
    /// Account number, e.g. `"1100"` (Netflix CoA).
    pub number: String,
    pub description: Localized,
    pub position: Position,
    /// Optional explicit ATL/BTL hint (the Netflix CoA tags categories
    /// directly). The authoritative split is by [`ProductionTotal`] position,
    /// but this is used when no production-total divider exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atl_btl: Option<AtlBtl>,
    /// Fringes applied to every detail in this category (cascade).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_fringes: Vec<AppliedFringe>,
}

/// The Above-/Below-the-line classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AtlBtl {
    Atl,
    Btl,
}

/// Belongs to a category; rolls up its detail lines (§5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub category: CategoryId,
    pub number: String,
    pub description: Localized,
    pub position: Position,
    #[serde(default = "default_true")]
    pub show_subtotal: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_fringes: Vec<AppliedFringe>,
}

/// The atomic cost row (§5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detail {
    pub id: DetailId,
    pub account: AccountId,
    pub position: Position,
    pub description: String,
    /// Free-text "İSİM" column (cast/crew name) from Turkish budgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Quantity ("ADET"). May reference globals.
    pub amount: Formula,
    /// "X" multiplier column. May reference globals.
    pub multiplier: Formula,
    /// Per-unit rate ("BİRİM TUTAR"). May reference globals.
    pub rate: Formula,
    pub unit: UnitId,
    pub currency: CurrencyId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_fringes: Vec<AppliedFringe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<SetId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gl_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Divider row. The **first** Production Total defines the ATL/BTL boundary (§5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionTotal {
    pub id: ProductionTotalId,
    pub label: Localized,
    pub position: Position,
}

/// Contractual charge listed below the last Production Total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Charge {
    pub id: OpScopedId,
    pub label: Localized,
    pub amount: Formula,
    pub position: Position,
}

/// Applied credit listed between Grand Total and Net Total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credit {
    pub id: OpScopedId,
    pub label: Localized,
    pub amount: Formula,
    pub position: Position,
}

/// Generic id for charges/credits (kept simple; not separately typed).
pub type OpScopedId = uuid::Uuid;

// ---------------------------------------------------------------------------
// Budget aggregate root
// ---------------------------------------------------------------------------

/// A complete budget: the materialized state that [`crate::ops`] mutate and the
/// [`crate::calc`] engine evaluates. Entities are stored flat (id-keyed) for
/// O(1) op application and tombstoning; ordering is via each entity's
/// [`Position`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub id: BudgetId,
    pub name: String,
    pub base_currency: CurrencyId,

    pub categories: HashMap<CategoryId, Category>,
    pub accounts: HashMap<AccountId, Account>,
    pub details: HashMap<DetailId, Detail>,
    #[serde(default)]
    pub production_totals: HashMap<ProductionTotalId, ProductionTotal>,
    #[serde(default)]
    pub charges: HashMap<OpScopedId, Charge>,
    #[serde(default)]
    pub credits: HashMap<OpScopedId, Credit>,
    /// Recorded actuals (invoices/expenses) costed against accounts (§16 Phase
    /// 3). Keyed by the actual's own UUID. See [`crate::actuals`].
    #[serde(default)]
    pub actuals: HashMap<uuid::Uuid, crate::actuals::Actual>,
    /// Settlement receipts (fiş) for petty-cash / advance closing ("Hesap
    /// Kapama"). Keyed by the receipt's own UUID. See [`crate::settlement`].
    #[serde(default)]
    pub receipts: HashMap<uuid::Uuid, crate::settlement::Receipt>,
    /// Stripboard strips for scheduling / Day-Out-of-Days (§16 Phase 2). Keyed
    /// by the strip's own UUID. See [`crate::scheduling`].
    #[serde(default)]
    pub strips: HashMap<uuid::Uuid, crate::scheduling::Strip>,
    /// Purchase orders (commitments). Keyed by the PO's own UUID. See
    /// [`crate::po`].
    #[serde(default)]
    pub purchase_orders: HashMap<uuid::Uuid, crate::po::PurchaseOrder>,

    // Setup tools
    pub fringes: HashMap<FringeId, Fringe>,
    pub globals: HashMap<GlobalId, Global>,
    pub units: HashMap<UnitId, Unit>,
    #[serde(default)]
    pub groups: HashMap<GroupId, Group>,
    #[serde(default)]
    pub locations: HashMap<LocationId, Location>,
    #[serde(default)]
    pub sets: HashMap<SetId, SetEntity>,
    pub currencies: HashMap<CurrencyId, Currency>,

    /// Fringes applied to the whole budget (cascade to every detail).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_fringes: Vec<AppliedFringe>,
}

impl Budget {
    /// A new, empty budget with a single base currency.
    #[must_use]
    pub fn new(name: impl Into<String>, base_currency: Currency) -> Self {
        let base_id = base_currency.id;
        let mut currencies = HashMap::new();
        let mut base = base_currency;
        base.is_base = true;
        base.rate_to_base = Decimal::ONE;
        currencies.insert(base_id, base);
        Self {
            id: BudgetId::new(),
            name: name.into(),
            base_currency: base_id,
            categories: HashMap::new(),
            accounts: HashMap::new(),
            details: HashMap::new(),
            production_totals: HashMap::new(),
            charges: HashMap::new(),
            credits: HashMap::new(),
            actuals: HashMap::new(),
            receipts: HashMap::new(),
            strips: HashMap::new(),
            purchase_orders: HashMap::new(),
            fringes: HashMap::new(),
            globals: HashMap::new(),
            units: HashMap::new(),
            groups: HashMap::new(),
            locations: HashMap::new(),
            sets: HashMap::new(),
            currencies,
            applied_fringes: Vec::new(),
        }
    }

    /// Accounts of a category, sorted deterministically by `(position, id)`.
    #[must_use]
    pub fn accounts_of(&self, category: CategoryId) -> Vec<&Account> {
        let mut v: Vec<&Account> = self
            .accounts
            .values()
            .filter(|a| a.category == category)
            .collect();
        v.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
        v
    }

    /// Details of an account, sorted deterministically by `(position, id)`.
    #[must_use]
    pub fn details_of(&self, account: AccountId) -> Vec<&Detail> {
        let mut v: Vec<&Detail> = self
            .details
            .values()
            .filter(|d| d.account == account)
            .collect();
        v.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
        v
    }

    /// Categories sorted deterministically by `(position, id)`.
    #[must_use]
    pub fn categories_sorted(&self) -> Vec<&Category> {
        let mut v: Vec<&Category> = self.categories.values().collect();
        v.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
        v
    }

    /// Resolve a global value lookup table keyed by global *name* for the
    /// expression evaluator.
    #[must_use]
    pub fn globals_by_name(&self) -> HashMap<String, &Global> {
        self.globals.values().map(|g| (g.name.clone(), g)).collect()
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::try_currency;

    #[test]
    fn localized_falls_back_to_turkish() {
        let l = Localized::tr("Yönetmen");
        assert_eq!(l.get("en"), "Yönetmen");
        let b = Localized::bilingual("Yönetmen", "Director");
        assert_eq!(b.get("en"), "Director");
        assert_eq!(b.get("tr"), "Yönetmen");
    }

    #[test]
    fn new_budget_has_base_currency() {
        let b = Budget::new("Test", try_currency());
        let base = &b.currencies[&b.base_currency];
        assert!(base.is_base);
        assert_eq!(base.rate_to_base, Decimal::ONE);
        assert_eq!(base.code, "TRY");
    }

    #[test]
    fn formula_serde_is_tagged() {
        let f = Formula::expr("SHOOT_DAYS * 1.2");
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"kind\":\"expr\""));
        let back: Formula = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
    }
}
