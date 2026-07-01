//! Expense settlement / petty-cash closing (§16 — "Hesap Kapama"). Pure,
//! I/O-free.
//!
//! Models the Turkish production "perakende satış vesikaları ile tevsik edilen
//! giderler icmali": a crew member's cash **advance** (avans) is closed out
//! against retail receipts (fiş). Receipts are **KDV-inclusive**, so VAT is
//! *extracted backwards* (unlike a vendor invoice, which grosses up from net):
//! `net = gross / (1 + kdv_rate)`, `kdv = gross − net`. Lines roll up by expense
//! category, and the advance is reconciled (`advance − spent = balance`).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn sat_sub(a: Decimal, b: Decimal) -> Decimal {
    a.checked_sub(b).unwrap_or(Decimal::ZERO)
}
fn sat_add(a: Decimal, b: Decimal) -> Decimal {
    a.checked_add(b).unwrap_or(Decimal::MAX)
}

/// Extract VAT from a **KDV-inclusive** total. `kdv_rate` is a fraction
/// (0.10 == 10%). Returns `(kdv, net)`. Total function: a non-positive or
/// degenerate rate yields no VAT (`kdv = 0`, `net = gross`); arithmetic
/// saturates so an absurd `gross` can never panic.
#[must_use]
pub fn extract_vat(gross: Decimal, kdv_rate: Decimal) -> (Decimal, Decimal) {
    // checked: a rate near Decimal::MAX would overflow `1 + rate` and panic.
    let denom = match Decimal::ONE.checked_add(kdv_rate) {
        Some(d) => d,
        None => return (Decimal::ZERO, gross),
    };
    if kdv_rate <= Decimal::ZERO || denom <= Decimal::ZERO {
        return (Decimal::ZERO, gross);
    }
    let net = gross.checked_div(denom).unwrap_or(gross);
    let kdv = sat_sub(gross, net);
    (kdv, net)
}

/// A single settlement receipt (fiş) costed to an expense category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub id: uuid::Uuid,
    #[serde(default)]
    pub date: String,
    /// Who it was bought from (KİMDEN ALINDIĞI).
    #[serde(default)]
    pub vendor: String,
    /// Receipt / invoice number (FİŞ / FATURA NO).
    #[serde(default)]
    pub receipt_no: String,
    /// Expense category (GİDER) — the rollup key, e.g. "YEMEK%10".
    pub category: String,
    #[serde(default)]
    pub description: String,
    /// KDV-inclusive total (KDV-Lİ TUTAR).
    pub gross: Decimal,
    /// VAT rate as a fraction (KDV ORANI; 0.10 == %10).
    #[serde(default)]
    pub kdv_rate: Decimal,
}

impl Receipt {
    /// `(kdv, net)` extracted from the inclusive gross.
    #[must_use]
    pub fn breakdown(&self) -> (Decimal, Decimal) {
        extract_vat(self.gross, self.kdv_rate)
    }
}

/// One expense category's rolled-up totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryTotal {
    pub gross: Decimal,
    pub kdv: Decimal,
    pub net: Decimal,
}

/// The whole settlement: per-category rollup, grand totals, and the advance
/// reconciliation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettlementReport {
    /// Expense category → totals (deterministic order).
    pub by_category: BTreeMap<String, CategoryTotal>,
    pub gross_total: Decimal,
    pub kdv_total: Decimal,
    pub net_total: Decimal,
    /// Cash advance handed to the holder (avans).
    pub advance: Decimal,
    /// `advance − gross_total`. Positive ⇒ holder refunds the company (iade);
    /// negative ⇒ company reimburses the holder.
    pub balance: Decimal,
}

/// Roll receipts up by category and reconcile against an `advance`. Saturating,
/// so a hostile/absurd `gross` can never panic.
#[must_use]
pub fn settlement_report(receipts: &[Receipt], advance: Decimal) -> SettlementReport {
    let mut report = SettlementReport {
        advance,
        ..Default::default()
    };
    for r in receipts {
        let (kdv, net) = r.breakdown();
        let e = report
            .by_category
            .entry(r.category.clone())
            .or_insert(CategoryTotal {
                gross: Decimal::ZERO,
                kdv: Decimal::ZERO,
                net: Decimal::ZERO,
            });
        e.gross = sat_add(e.gross, r.gross);
        e.kdv = sat_add(e.kdv, kdv);
        e.net = sat_add(e.net, net);
        report.gross_total = sat_add(report.gross_total, r.gross);
        report.kdv_total = sat_add(report.kdv_total, kdv);
        report.net_total = sat_add(report.net_total, net);
    }
    report.balance = sat_sub(advance, report.gross_total);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::round_money;
    use rust_decimal_macros::dec;

    #[test]
    fn extract_vat_matches_the_sheet() {
        // 790 incl. %10 → KDV 71.82, net 718.18 (sheet row 8).
        let (kdv, net) = extract_vat(dec!(790), dec!(0.10));
        assert_eq!(round_money(kdv), dec!(71.82));
        assert_eq!(round_money(net), dec!(718.18));
        // 5900 incl. %20 → KDV 983.33, net 4916.67 (sheet row 20).
        let (kdv, net) = extract_vat(dec!(5900), dec!(0.20));
        assert_eq!(round_money(kdv), dec!(983.33));
        assert_eq!(round_money(net), dec!(4916.67));
        // 1500 incl. %1 → KDV 14.85, net 1485.15 (sheet row 18).
        let (kdv, net) = extract_vat(dec!(1500), dec!(0.01));
        assert_eq!(round_money(kdv), dec!(14.85));
        assert_eq!(round_money(net), dec!(1485.15));
        // 0% → no VAT.
        assert_eq!(extract_vat(dec!(500), dec!(0)), (dec!(0), dec!(500)));
    }

    fn r(cat: &str, gross: Decimal, rate: Decimal) -> Receipt {
        Receipt {
            id: uuid::Uuid::now_v7(),
            date: String::new(),
            vendor: String::new(),
            receipt_no: String::new(),
            category: cat.into(),
            description: String::new(),
            gross,
            kdv_rate: rate,
        }
    }

    #[test]
    fn reproduces_the_uber_settlement_grand_totals() {
        // All 16 receipts from "uber - Levend Çağıl Hesap Kapama.xlsx".
        let ten = dec!(0.10);
        let receipts = vec![
            r("YEMEK%10", dec!(790), ten),
            r("YEMEK%10", dec!(350), ten),
            r("YEMEK%10", dec!(8195), ten),
            r("YEMEK%10", dec!(1030), ten),
            r("YEMEK%10", dec!(260), ten),
            r("YEMEK%10", dec!(1220), ten),
            r("YEMEK%10", dec!(1500), ten),
            r("YEMEK%10", dec!(1080), ten),
            r("YEMEK%10", dec!(600), ten),
            r("YEMEK%10", dec!(650), ten),
            r("YEMEK%1", dec!(1500), dec!(0.01)),
            r("YEMEK%10", dec!(1575), ten),
            r("SANAT-DEKOR%20", dec!(5900), dec!(0.20)),
            r("YEMEK%10", dec!(395), ten),
            r("YEMEK%1", dec!(239.99), dec!(0.01)),
            r("YEMEK%10", dec!(560), ten),
        ];
        let rep = settlement_report(&receipts, dec!(30000));

        // GENEL TOPLAM from the sheet.
        assert_eq!(round_money(rep.gross_total), dec!(25844.99));
        assert_eq!(round_money(rep.net_total), dec!(23189.43));
        assert_eq!(round_money(rep.kdv_total), dec!(2655.56));

        // Category rollup: YEMEK%10 group gross = 18205 (sheet row 43).
        assert_eq!(
            round_money(rep.by_category["YEMEK%10"].gross),
            dec!(18205.00)
        );
        // SANAT-DEKOR%20 net = 4916.67 (sheet row 70).
        assert_eq!(
            round_money(rep.by_category["SANAT-DEKOR%20"].net),
            dec!(4916.67)
        );

        // Advance reconciliation: 30000 − 25844.99 = 4155.01 to refund.
        assert_eq!(round_money(rep.balance), dec!(4155.01));
    }

    #[test]
    fn absurd_gross_saturates_not_panics() {
        let big = dec!(50000000000000000000000000000); // ~5e28
        let rep = settlement_report(
            &[r("X", big, dec!(0.20)), r("X", big, dec!(0.20))],
            Decimal::ZERO,
        );
        assert!(rep.gross_total <= Decimal::MAX); // no panic
    }

    #[test]
    fn absurd_kdv_rate_does_not_panic() {
        // A near-MAX rate once overflowed `1 + rate` and PANICked; must degrade
        // to "no VAT" instead.
        let (kdv, net) = extract_vat(dec!(100), Decimal::MAX);
        assert_eq!(kdv, Decimal::ZERO);
        assert_eq!(net, dec!(100));
        // And via a stored receipt → report build.
        let rep = settlement_report(&[r("X", dec!(100), Decimal::MAX)], Decimal::ZERO);
        assert_eq!(round_money(rep.kdv_total), dec!(0.00));
    }
}
