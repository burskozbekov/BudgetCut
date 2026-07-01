//! Actuals / cost tracking (§16 Phase 3). Pure, I/O-free.
//!
//! Two pieces, both validated against the real invoice tax math from the source
//! `FATURA` sheet:
//!
//! 1. [`invoice_breakdown`] — the Turkish invoice calculation: income-tax
//!    withholding (**stopaj**, a gross-up for serbest-meslek), **KDV** (VAT) on
//!    the gross, and VAT withholding (**tevkifat**). Payable to the vendor is
//!    `brüt + KDV − stopaj − tevkifat`.
//! 2. [`variance_report`] — estimate (the budgeted [`crate::calc`] total) vs
//!    actual (committed cost) per account → variance and EFC (estimated final
//!    cost), the Saturation-style closed loop.

use crate::calc::CalcResult;
use crate::ids::AccountId;
use crate::Budget;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Saturating money arithmetic — `rust_decimal`'s `+`/`-` PANIC on overflow, so
/// any path fed by user-supplied amounts must use these to stay **total** (the
/// invoice/variance math must never panic on an absurd `net`). Mirrors the calc
/// engine's sat_add/sat_sub guarantee.
fn sat_add(a: Decimal, b: Decimal) -> Decimal {
    a.checked_add(b).unwrap_or(Decimal::MAX)
}
fn sat_sub(a: Decimal, b: Decimal) -> Decimal {
    a.checked_sub(b).unwrap_or(Decimal::ZERO)
}

/// The computed tax/payable breakdown of a single invoice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceBreakdown {
    /// Net (KDV hariç) — the agreed fee before taxes.
    pub net: Decimal,
    /// Gross (brüt) — net grossed up for stopaj: `net / (1 − stopaj_rate)`.
    pub brut: Decimal,
    /// Income-tax withholding (`brüt − net`).
    pub stopaj: Decimal,
    /// VAT on the gross (`brüt × kdv_rate`).
    pub kdv: Decimal,
    /// VAT withheld at source (`KDV × tevkifat_rate`).
    pub tevkifat_kdv: Decimal,
    /// Amount actually paid to the vendor: `brüt + KDV − stopaj − tevkifat_kdv`.
    pub payable: Decimal,
}

/// Compute a Turkish invoice's tax breakdown. All rates are fractions
/// (0.20 == 20%). `stopaj_rate = 0` means no withholding (`brüt = net`).
///
/// Total function: a `stopaj_rate` outside `[0, 1)` (which would divide by a
/// non-positive denominator and panic, or produce a negative gross) is treated
/// as **no withholding** rather than panicking — invalid user input degrades
/// gracefully. Validate rates upstream if you want to reject them.
#[must_use]
pub fn invoice_breakdown(
    net: Decimal,
    stopaj_rate: Decimal,
    kdv_rate: Decimal,
    tevkifat_rate: Decimal,
) -> InvoiceBreakdown {
    let denom = Decimal::ONE - stopaj_rate;
    let brut = if stopaj_rate > Decimal::ZERO && denom > Decimal::ZERO {
        net.checked_div(denom).unwrap_or(net)
    } else {
        net
    };
    let stopaj = sat_sub(brut, net);
    let kdv = brut.checked_mul(kdv_rate).unwrap_or(Decimal::ZERO);
    let tevkifat_kdv = kdv.checked_mul(tevkifat_rate).unwrap_or(Decimal::ZERO);
    // brüt + KDV − stopaj − tevkifat, saturating so a hostile net can't panic.
    let payable = sat_sub(sat_sub(sat_add(brut, kdv), stopaj), tevkifat_kdv);
    InvoiceBreakdown {
        net,
        brut,
        stopaj,
        kdv,
        tevkifat_kdv,
        payable,
    }
}

/// VAT-withholding (tevkifat) rate for a common service type. Returns 0 for an
/// unknown/misspelled type or "Yok" (none) — callers needing to distinguish
/// "no tevkifat" from "unrecognised" should validate upstream. Exact decimal
/// literals, no `f64` in the money path (§18).
#[must_use]
pub fn tevkifat_rate(kind: &str) -> Decimal {
    use rust_decimal_macros::dec;
    match kind {
        "Yük Taşımacılığı" => dec!(0.20), // 2/10
        "Ticari Reklam" => dec!(0.30),    // 3/10
        "Temizlik" | "İşgücü Temini" | "Danışmanlık" | "Etüt-Proje" => dec!(0.90), // 9/10
        "Yapım İşleri" | "Yapım" => dec!(0.40), // 4/10
        _ => Decimal::ZERO,
    }
}

/// A recorded actual cost (an invoice/expense), costed against a budget account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actual {
    pub id: uuid::Uuid,
    pub account: AccountId,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub description: String,
    pub net: Decimal,
    #[serde(default)]
    pub stopaj_rate: Decimal,
    #[serde(default)]
    pub kdv_rate: Decimal,
    #[serde(default)]
    pub tevkifat_rate: Decimal,
}

impl Actual {
    #[must_use]
    pub fn breakdown(&self) -> InvoiceBreakdown {
        invoice_breakdown(
            self.net,
            self.stopaj_rate,
            self.kdv_rate,
            self.tevkifat_rate,
        )
    }

    /// The cost charged against the budget — the gross (brüt), since VAT is a
    /// pass-through and stopaj is part of the gross cost of engaging the vendor.
    #[must_use]
    pub fn cost(&self) -> Decimal {
        self.breakdown().brut
    }
}

/// Estimate-vs-actual for one account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountVariance {
    pub estimate: Decimal,
    pub actual: Decimal,
    /// `estimate − actual` (positive = remaining/under, negative = over).
    pub variance: Decimal,
    /// Estimated final cost: `max(estimate, actual)`.
    pub efc: Decimal,
}

/// The whole estimate-vs-actual report, rolled up.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VarianceReport {
    pub by_account: std::collections::HashMap<AccountId, AccountVariance>,
    pub estimate_total: Decimal,
    pub actual_total: Decimal,
    pub variance_total: Decimal,
    pub efc_total: Decimal,
}

/// Build the estimate-vs-actual / EFC report from the budgeted [`CalcResult`]
/// and a set of recorded [`Actual`]s.
///
/// Semantics & caveats:
/// * **EFC = `max(estimate, actual)`** treats actuals as *open commitments* — an
///   upper bound on the final cost. It deliberately won't forecast an *underrun*
///   on a still-open account; closing an account (final cost = actual) is a
///   future per-account signal.
/// * **Base alignment:** an actual's cost is its **brüt** (grossed-up). For the
///   comparison to be apples-to-apples, the budgeted estimate for a
///   withheld-tax (E-SMM) account must carry the matching stopaj gross-up
///   fringe — which is exactly how the Library presets model those lines.
/// * The `*_total` fields are **unrounded** (full precision) for correct
///   accumulation; round at a single point on display (round the total, or
///   round rows then re-sum — pick one) to keep rows reconciling with the total.
#[must_use]
pub fn variance_report(budget: &Budget, calc: &CalcResult, actuals: &[Actual]) -> VarianceReport {
    use std::collections::HashMap;

    // Sum committed cost per account (saturating — never panic on overflow).
    let mut actual_by_account: HashMap<AccountId, Decimal> = HashMap::new();
    for a in actuals {
        let e = actual_by_account.entry(a.account).or_default();
        *e = sat_add(*e, a.cost());
    }

    let mut report = VarianceReport::default();
    // Every account that has either an estimate or an actual.
    let mut account_ids: Vec<AccountId> = budget.accounts.keys().copied().collect();
    for id in actual_by_account.keys() {
        if !budget.accounts.contains_key(id) {
            account_ids.push(*id);
        }
    }
    account_ids.sort();
    account_ids.dedup();

    for id in account_ids {
        let estimate = calc
            .accounts
            .get(&id)
            .map(|r| r.total)
            .unwrap_or(Decimal::ZERO);
        let actual = actual_by_account.get(&id).copied().unwrap_or(Decimal::ZERO);
        if estimate.is_zero() && actual.is_zero() {
            continue;
        }
        let efc = if actual > estimate { actual } else { estimate };
        report.by_account.insert(
            id,
            AccountVariance {
                estimate,
                actual,
                variance: estimate - actual,
                efc,
            },
        );
        report.estimate_total = sat_add(report.estimate_total, estimate);
        report.actual_total = sat_add(report.actual_total, actual);
        report.efc_total = sat_add(report.efc_total, efc);
    }
    report.variance_total = sat_sub(report.estimate_total, report.actual_total);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::round_money;
    use rust_decimal_macros::dec;

    /// Reproduce the real `FATURA` sheet rows to the kuruş.
    #[test]
    fn invoice_math_matches_fatura_sheet() {
        // r2 — E-SMM, stopaj 20%, KDV 20%, no tevkifat.
        let r2 = invoice_breakdown(dec!(40000), dec!(0.20), dec!(0.20), dec!(0));
        assert_eq!(round_money(r2.brut), dec!(50000.00));
        assert_eq!(round_money(r2.stopaj), dec!(10000.00));
        assert_eq!(round_money(r2.kdv), dec!(10000.00));
        assert_eq!(round_money(r2.payable), dec!(50000.00));

        // r3 — E-Arşiv, no stopaj/tevkifat, KDV 20%.
        let r3 = invoice_breakdown(dec!(8000), dec!(0), dec!(0.20), dec!(0));
        assert_eq!(round_money(r3.kdv), dec!(1600.00));
        assert_eq!(round_money(r3.payable), dec!(9600.00));

        // r6 — E-Fatura, no stopaj, tevkifat "Yük Taşımacılığı" (2/10), KDV 20%.
        let r6 = invoice_breakdown(
            dec!(258000),
            dec!(0),
            dec!(0.20),
            tevkifat_rate("Yük Taşımacılığı"),
        );
        assert_eq!(round_money(r6.kdv), dec!(51600.00));
        assert_eq!(round_money(r6.tevkifat_kdv), dec!(10320.00));
        assert_eq!(round_money(r6.payable), dec!(299280.00));

        // r7 — E-SMM, stopaj 20%, tevkifat "Ticari Reklam" (3/10), KDV 20%.
        let r7 = invoice_breakdown(
            dec!(350000),
            dec!(0.20),
            dec!(0.20),
            tevkifat_rate("Ticari Reklam"),
        );
        assert_eq!(round_money(r7.brut), dec!(437500.00));
        assert_eq!(round_money(r7.stopaj), dec!(87500.00));
        assert_eq!(round_money(r7.kdv), dec!(87500.00));
        assert_eq!(round_money(r7.tevkifat_kdv), dec!(26250.00));
        assert_eq!(round_money(r7.payable), dec!(411250.00));

        // r8 — E-Arşiv, KDV 10%.
        let r8 = invoice_breakdown(dec!(90909.09), dec!(0), dec!(0.10), dec!(0));
        assert_eq!(round_money(r8.kdv), dec!(9090.91));
        assert_eq!(round_money(r8.payable), dec!(100000.00));
    }

    #[test]
    fn invalid_stopaj_rate_does_not_panic() {
        // rate == 1 would divide by zero; rate > 1 would go negative — both
        // must degrade to "no withholding" rather than panic (total function).
        for r in [dec!(1), dec!(1.5), dec!(-0.2)] {
            let bd = invoice_breakdown(dec!(100000), r, dec!(0.20), dec!(0));
            assert_eq!(bd.brut, dec!(100000), "rate {r} should fall back to net");
            assert_eq!(bd.stopaj, dec!(0));
            assert_eq!(round_money(bd.payable), dec!(120000.00));
        }
        // a valid 20% still grosses up
        let ok = invoice_breakdown(dec!(40000), dec!(0.20), dec!(0.20), dec!(0));
        assert_eq!(round_money(ok.brut), dec!(50000.00));
    }

    #[test]
    fn absurd_net_saturates_rather_than_panicking() {
        // A near-Decimal::MAX net (a pasted/typo'd or hostile figure) once made
        // brüt + KDV overflow and PANIC. invoice_breakdown must be total now.
        let huge = dec!(39000000000000000000000000000); // ~3.9e28
        let bd = invoice_breakdown(huge, dec!(0.5), dec!(0.2), dec!(0));
        assert!(bd.payable <= Decimal::MAX); // no panic; saturates
                                             // variance_report accumulation must not panic either.
        let b = Budget::new("t", crate::templates::try_currency());
        let calc = crate::evaluate(&b);
        let many: Vec<Actual> = (0..40)
            .map(|_| Actual {
                id: uuid::Uuid::now_v7(),
                account: crate::ids::AccountId::new(),
                date: String::new(),
                vendor: String::new(),
                description: String::new(),
                net: dec!(5000000000000000000000000000),
                stopaj_rate: dec!(0),
                kdv_rate: dec!(0),
                tevkifat_rate: dec!(0),
            })
            .collect();
        let r = variance_report(&b, &calc, &many);
        assert!(r.actual_total <= Decimal::MAX); // saturated, no panic
    }

    #[test]
    fn variance_and_efc() {
        use crate::ids::*;
        use crate::*;
        let mut b = Budget::new("t", templates::try_currency());
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
            number: "1300".into(),
            description: Localized::tr(""),
            position: dec!(1),
            atl_btl: Some(AtlBtl::Atl),
            applied_fringes: vec![],
        };
        let acc = Account {
            id: AccountId::new(),
            category: cat.id,
            number: "1301".into(),
            description: Localized::tr(""),
            position: dec!(1),
            show_subtotal: true,
            applied_fringes: vec![],
        };
        let acc_id = acc.id;
        let det = Detail {
            id: DetailId::new(),
            account: acc_id,
            position: dec!(1),
            description: "".into(),
            name: None,
            amount: Formula::Const(dec!(1)),
            multiplier: Formula::Const(Decimal::ONE),
            rate: Formula::Const(dec!(100000)),
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
        let calc = evaluate(&b);

        // Estimate 100000; one actual costing 90000 brüt (net 90000, no taxes).
        let actuals = vec![Actual {
            id: uuid::Uuid::now_v7(),
            account: acc_id,
            date: "".into(),
            vendor: "".into(),
            description: "".into(),
            net: dec!(90000),
            stopaj_rate: dec!(0),
            kdv_rate: dec!(0.20),
            tevkifat_rate: dec!(0),
        }];
        let r = variance_report(&b, &calc, &actuals);
        let v = r.by_account[&acc_id];
        assert_eq!(v.estimate, dec!(100000));
        assert_eq!(v.actual, dec!(90000)); // brüt cost (KDV excluded)
        assert_eq!(v.variance, dec!(10000)); // under budget
        assert_eq!(v.efc, dec!(100000)); // EFC = estimate (not over)
        assert_eq!(r.actual_total, dec!(90000));
    }

    #[test]
    fn actual_op_roundtrips_through_the_reducer() {
        use crate::hlc::HlcClock;
        use crate::ids::{AccountId, UserId};
        use crate::ops::{ApplyResult, Document, Op, OpKind};

        let acc = AccountId::new();
        let mut clock = HlcClock::new(UserId::new());
        let author = UserId::new();
        let mut doc = Document::new(Budget::new("t", crate::templates::try_currency()));

        let a = Actual {
            id: uuid::Uuid::now_v7(),
            account: acc,
            date: "2026-06-30".into(),
            vendor: "Tedarikçi".into(),
            description: "Kamera kirası".into(),
            net: dec!(40000),
            stopaj_rate: dec!(0.20),
            kdv_rate: dec!(0.20),
            tevkifat_rate: dec!(0),
        };
        let aid = a.id;

        let insert = Op::new(clock.tick(1), author, OpKind::InsertActual(a));
        assert!(matches!(doc.apply(&insert), ApplyResult::Applied));
        assert_eq!(doc.budget.actuals.len(), 1);
        assert_eq!(doc.budget.actuals[&aid].cost(), dec!(50000)); // brüt = 40000/0.8

        // Idempotent re-delivery doesn't double-insert.
        assert!(matches!(doc.apply(&insert), ApplyResult::Idempotent));
        assert_eq!(doc.budget.actuals.len(), 1);

        let remove = Op::new(clock.tick(2), author, OpKind::RemoveActual(aid));
        assert!(matches!(doc.apply(&remove), ApplyResult::Applied));
        assert!(doc.budget.actuals.is_empty());
    }
}
