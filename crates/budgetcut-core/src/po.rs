//! Purchase orders + approval workflow (§ commitments). Pure, I/O-free.
//!
//! A PO commits budget against an account before the invoice arrives:
//! **Draft → Approved → Converted** (to an actual). Committed cost (approved +
//! converted) is what a producer watches against the estimate. Op-logged like
//! every other entity; status changes are a last-write-wins whole-record update.

use crate::ids::AccountId;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum POStatus {
    Draft,
    Approved,
    /// Realized into an [`crate::actuals::Actual`].
    Converted,
}

impl POStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            POStatus::Draft => "draft",
            POStatus::Approved => "approved",
            POStatus::Converted => "converted",
        }
    }
}

/// A purchase order committed against a budget account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrder {
    pub id: uuid::Uuid,
    pub account: AccountId,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub description: String,
    /// Committed amount (net).
    pub amount: Decimal,
    pub status: POStatus,
}
