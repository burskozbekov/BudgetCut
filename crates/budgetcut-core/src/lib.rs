//! # budgetcut-core
//!
//! The keystone crate (§4). It holds, with **zero I/O and zero platform
//! dependencies** (so it compiles to native today and WASM/mobile later):
//!
//! 1. [`model`] — the domain model (Movie Magic-aligned: Budget → Category →
//!    Account → Detail, plus the Setup Tools).
//! 2. [`calc`] — a deterministic calculation engine (globals + dependency graph
//!    with cycle detection, dual-mode fringes, rollups, ATL/BTL, currency).
//! 3. [`ops`] — the mutation [`ops::Op`] log, [`hlc`] timestamps, and the
//!    per-field Last-Write-Wins reducer ([`ops::Document::apply`]).
//! 4. [`validation`] — referential-integrity invariants.
//! 5. [`templates`] — the seeded Netflix CoA and Turkish fringe presets.
//!
//! Both the desktop client and the sync server compile this exact crate, so a
//! budget computed offline on a laptop equals the same budget computed on the
//! server equals the same budget on a collaborator's machine.
//!
//! Money is fixed-point [`rust_decimal::Decimal`] ([`money`]); ids are UUIDv7
//! ([`ids`]); both per spec §18.

#![doc(html_no_source)]

pub mod actuals;
pub mod calc;
pub mod compare;
pub mod expr;
pub mod hlc;
pub mod ids;
pub mod incentives;
pub mod library;
pub mod model;
pub mod money;
pub mod ops;
pub mod po;
pub mod scheduling;
pub mod series;
pub mod settlement;
pub mod templates;
pub mod validation;
pub mod view;

// Curated re-exports for ergonomic downstream use.
pub use calc::{evaluate, CalcResult, CellError, CellTarget, DetailCalc, Rollup};
pub use hlc::{Hlc, HlcClock, NodeId};
pub use model::{
    Account, AppliedFringe, AtlBtl, Budget, Category, Charge, Credit, Currency, Detail, Formula,
    Fringe, FringeKind, FringeMode, Global, Group, Localized, Location, PostingLevel,
    ProductionTotal, SetEntity, Unit,
};
pub use money::{round_money, Money};
pub use ops::{ApplyResult, DetailField, Document, Op, OpKind};
pub use validation::{validate, ValidationError};
