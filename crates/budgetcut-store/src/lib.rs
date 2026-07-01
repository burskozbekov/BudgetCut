//! # budgetcut-store
//!
//! Offline-first local persistence (§7) for BudgetCut. SQLite holds an
//! append-only **op log** (the source of truth, replayed through
//! `budgetcut-core`) plus the initial template snapshot; unacknowledged ops are
//! the **outbox** for the future sync server. This crate is Tauri-free so it is
//! testable and runnable headlessly; the desktop app is a thin wrapper over
//! [`Session`].

#![forbid(unsafe_code)]

mod error;
mod store;

pub mod dto;
pub mod session;

pub use error::{Result, StoreError};
pub use session::Session;
pub use store::{now_ms, Store};
