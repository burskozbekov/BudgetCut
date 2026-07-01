use thiserror::Error;

/// Errors from the local store.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("no budget in this store yet")]
    NoBudget,
    #[error("a budget already exists in this store")]
    BudgetExists,
}

pub type Result<T> = std::result::Result<T, StoreError>;
