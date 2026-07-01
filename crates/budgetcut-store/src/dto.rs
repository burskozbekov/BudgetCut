//! View DTOs now live in `budgetcut-core::view` (pure projections shared by the
//! desktop store and the sync server). Re-exported here for backward-compatible
//! `budgetcut_store::dto::*` paths.

pub use budgetcut_core::view::*;
