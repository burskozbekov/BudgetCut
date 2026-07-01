//! Budget comparison (MMB "compare budgets across multiple locations / compare
//! complex cost scenarios"). Diffs two computed budgets by **category number**
//! so versions or location scenarios that share the Netflix CoA line up. Pure.

use crate::calc::CalcResult;
use crate::Budget;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryDiff {
    pub number: String,
    pub name: String,
    pub a_total: Decimal,
    pub b_total: Decimal,
    /// `b_total − a_total` (change going from A to B).
    pub diff: Decimal,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetComparison {
    pub rows: Vec<CategoryDiff>,
    pub a_total: Decimal,
    pub b_total: Decimal,
    pub diff: Decimal,
}

fn totals_by_number(budget: &Budget, calc: &CalcResult) -> BTreeMap<String, (String, Decimal)> {
    let mut m = BTreeMap::new();
    for c in budget.categories_sorted() {
        let total = calc
            .categories
            .get(&c.id)
            .map(|r| r.total)
            .unwrap_or(Decimal::ZERO);
        // Sum across same-numbered categories (defensive; numbers are unique normally).
        let e = m
            .entry(c.number.clone())
            .or_insert((c.description.tr.clone(), Decimal::ZERO));
        e.1 += total;
    }
    m
}

/// Compare budget A against budget B, category-by-category (by number).
#[must_use]
pub fn compare(
    a: &Budget,
    a_calc: &CalcResult,
    b: &Budget,
    b_calc: &CalcResult,
) -> BudgetComparison {
    let ta = totals_by_number(a, a_calc);
    let tb = totals_by_number(b, b_calc);

    let mut numbers: Vec<String> = ta.keys().chain(tb.keys()).cloned().collect();
    numbers.sort();
    numbers.dedup();

    let mut out = BudgetComparison::default();
    for num in numbers {
        let (name_a, a_total) = ta.get(&num).cloned().unwrap_or_default();
        let (name_b, b_total) = tb.get(&num).cloned().unwrap_or_default();
        let name = if !name_a.is_empty() { name_a } else { name_b };
        if a_total.is_zero() && b_total.is_zero() {
            continue;
        }
        out.rows.push(CategoryDiff {
            number: num,
            name,
            a_total,
            b_total,
            diff: b_total - a_total,
        });
        out.a_total += a_total;
        out.b_total += b_total;
    }
    out.diff = out.b_total - out.a_total;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::*;
    use crate::*;
    use rust_decimal_macros::dec;

    fn budget_with(num: &str, rate: Decimal) -> Budget {
        let mut b = Budget::new("v", templates::try_currency());
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
            description: Localized::tr("OYUNCULAR"),
            position: dec!(1),
            atl_btl: Some(AtlBtl::Atl),
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

    #[test]
    fn compares_two_versions_by_category_number() {
        let a = budget_with("1400", dec!(400000)); // İstanbul
        let bb = budget_with("1400", dec!(500000)); // Kapadokya (pahalı)
        let cmp = compare(&a, &evaluate(&a), &bb, &evaluate(&bb));
        assert_eq!(cmp.rows.len(), 1);
        let row = &cmp.rows[0];
        assert_eq!(row.a_total, dec!(400000));
        assert_eq!(row.b_total, dec!(500000));
        assert_eq!(row.diff, dec!(100000)); // +100k going A→B
        assert_eq!(cmp.diff, dec!(100000));
    }
}
