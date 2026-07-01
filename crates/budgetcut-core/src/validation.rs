//! Structural invariants (§4).
//!
//! Calc surfaces *value* errors (unresolved globals, cycles). Validation here
//! checks *referential integrity*: that an entity's foreign keys resolve. The
//! server runs this before persisting ops; the client can run it to warn early.

use crate::ids::*;
use crate::model::*;

/// A broken invariant, identified by the offending entity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("account {0} references missing category {1}")]
    AccountMissingCategory(AccountId, CategoryId),
    #[error("detail {0} references missing account {1}")]
    DetailMissingAccount(DetailId, AccountId),
    #[error("detail {0} references missing unit {1}")]
    DetailMissingUnit(DetailId, UnitId),
    #[error("detail {0} references missing currency {1}")]
    DetailMissingCurrency(DetailId, CurrencyId),
    #[error("detail {0} applies missing fringe {1}")]
    DetailMissingFringe(DetailId, FringeId),
    #[error("budget applies missing fringe {0}")]
    BudgetMissingFringe(FringeId),
    #[error("category {0} applies missing fringe {1}")]
    CategoryMissingFringe(CategoryId, FringeId),
    #[error("account {0} applies missing fringe {1}")]
    AccountMissingFringe(AccountId, FringeId),
    #[error("detail {0} references missing group {1}")]
    DetailMissingGroup(DetailId, GroupId),
    #[error("base currency {0} is not in the currency table")]
    MissingBaseCurrency(CurrencyId),
    #[error("duplicate global name {0:?}")]
    DuplicateGlobalName(String),
    #[error("actual {0} references missing account {1}")]
    ActualMissingAccount(uuid::Uuid, AccountId),
}

/// Check every referential invariant; returns all violations found.
#[must_use]
pub fn validate(b: &Budget) -> Vec<ValidationError> {
    let mut errs = Vec::new();

    if !b.currencies.contains_key(&b.base_currency) {
        errs.push(ValidationError::MissingBaseCurrency(b.base_currency));
    }

    // Budget-level cascade fringes must resolve.
    for af in &b.applied_fringes {
        if !b.fringes.contains_key(&af.fringe_id) {
            errs.push(ValidationError::BudgetMissingFringe(af.fringe_id));
        }
    }

    for c in b.categories.values() {
        for af in &c.applied_fringes {
            if !b.fringes.contains_key(&af.fringe_id) {
                errs.push(ValidationError::CategoryMissingFringe(c.id, af.fringe_id));
            }
        }
    }

    for a in b.accounts.values() {
        if !b.categories.contains_key(&a.category) {
            errs.push(ValidationError::AccountMissingCategory(a.id, a.category));
        }
        for af in &a.applied_fringes {
            if !b.fringes.contains_key(&af.fringe_id) {
                errs.push(ValidationError::AccountMissingFringe(a.id, af.fringe_id));
            }
        }
    }

    for d in b.details.values() {
        if !b.accounts.contains_key(&d.account) {
            errs.push(ValidationError::DetailMissingAccount(d.id, d.account));
        }
        if !b.units.contains_key(&d.unit) {
            errs.push(ValidationError::DetailMissingUnit(d.id, d.unit));
        }
        if !b.currencies.contains_key(&d.currency) {
            errs.push(ValidationError::DetailMissingCurrency(d.id, d.currency));
        }
        for af in &d.applied_fringes {
            if !b.fringes.contains_key(&af.fringe_id) {
                errs.push(ValidationError::DetailMissingFringe(d.id, af.fringe_id));
            }
        }
        for g in &d.groups {
            if !b.groups.contains_key(g) {
                errs.push(ValidationError::DetailMissingGroup(d.id, *g));
            }
        }
    }

    // Actuals must cost against an account that exists.
    for a in b.actuals.values() {
        if !b.accounts.contains_key(&a.account) {
            errs.push(ValidationError::ActualMissingAccount(a.id, a.account));
        }
    }

    // Global names must be unique (they're an expression namespace).
    let mut seen = std::collections::HashSet::new();
    for g in b.globals.values() {
        if !seen.insert(g.name.clone()) {
            errs.push(ValidationError::DuplicateGlobalName(g.name.clone()));
        }
    }

    errs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::{try_currency, turkish_dizi_template};

    #[test]
    fn seeded_template_is_valid() {
        let b = turkish_dizi_template("t");
        assert_eq!(validate(&b), vec![]);
    }

    #[test]
    fn detects_dangling_account() {
        let mut b = Budget::new("t", try_currency());
        let acc = Account {
            id: AccountId::new(),
            category: CategoryId::new(), // never inserted
            number: "1".into(),
            description: Localized::tr(""),
            position: rust_decimal::Decimal::ONE,
            show_subtotal: true,
            applied_fringes: vec![],
        };
        b.accounts.insert(acc.id, acc);
        let errs = validate(&b);
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::AccountMissingCategory(_, _))));
    }
}
