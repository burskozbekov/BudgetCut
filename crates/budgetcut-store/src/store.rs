//! SQLite persistence (§7). The **op log is the source of truth**; the budget
//! is rebuilt by replaying it through `budgetcut-core` (the same reducer the
//! server uses), so local and remote can never diverge. The initial template
//! is stored once as a `base snapshot`; edits are appended as ops. Unacked ops
//! are the outbox for the future sync server.

use budgetcut_core::{Budget, Document, Hlc, Op};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, StoreError};

/// A local, on-disk (or in-memory) budget store.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if needed) a store at `path`, in WAL mode, migrated.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// An ephemeral in-memory store (tests).
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        // WAL: concurrent readers + a writer, good for an offline app cache.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS ops (
                seq     INTEGER PRIMARY KEY AUTOINCREMENT,
                op_id   TEXT NOT NULL UNIQUE,
                payload TEXT NOT NULL,
                acked   INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;
        Ok(Self { conn })
    }

    /// Whether a budget has been created in this store.
    pub fn has_budget(&self) -> Result<bool> {
        let v: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'budget_base'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.is_some())
    }

    /// Persist the initial budget snapshot. Fails if one already exists.
    pub fn create_budget(&mut self, base: &Budget) -> Result<()> {
        if self.has_budget()? {
            return Err(StoreError::BudgetExists);
        }
        let json = serde_json::to_string(base)?;
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES ('budget_base', ?1)",
            params![json],
        )?;
        Ok(())
    }

    /// Replace the whole local budget with a fresh `base`, discarding the op
    /// log. Used by "load sample / reset" — unlike [`create_budget`] it does not
    /// fail when a budget already exists.
    pub fn reset(&mut self, base: &Budget) -> Result<()> {
        let json = serde_json::to_string(base)?;
        self.conn.execute("DELETE FROM ops", [])?;
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES ('budget_base', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    /// Append an op to the log (idempotent by op id).
    pub fn append_op(&mut self, op: &Op) -> Result<()> {
        let payload = serde_json::to_string(op)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO ops (op_id, payload) VALUES (?1, ?2)",
            params![op.id.to_string(), payload],
        )?;
        Ok(())
    }

    /// All ops in log order.
    pub fn all_ops(&self) -> Result<Vec<Op>> {
        self.ops_query("SELECT payload FROM ops ORDER BY seq")
    }

    /// Ops not yet acknowledged by the server (the outbox).
    pub fn outbox(&self) -> Result<Vec<Op>> {
        self.ops_query("SELECT payload FROM ops WHERE acked = 0 ORDER BY seq")
    }

    fn ops_query(&self, sql: &str) -> Result<Vec<Op>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut ops = Vec::new();
        for row in rows {
            ops.push(serde_json::from_str::<Op>(&row?)?);
        }
        Ok(ops)
    }

    /// Mark ops up to and including `op_id` acknowledged by the server.
    pub fn ack_op(&mut self, op_id: &str) -> Result<()> {
        self.conn
            .execute("UPDATE ops SET acked = 1 WHERE op_id = ?1", params![op_id])?;
        Ok(())
    }

    /// Rebuild the [`Document`] by replaying the log onto the base snapshot.
    /// Returns the document and the maximum HLC seen (to re-seed the clock so
    /// new local ops strictly post-date the persisted ones).
    pub fn load_document(&self) -> Result<(Document, Option<Hlc>)> {
        let base_json: String = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'budget_base'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(StoreError::NoBudget)?;
        let base: Budget = serde_json::from_str(&base_json)?;
        let mut doc = Document::new(base);
        let mut max_hlc: Option<Hlc> = None;
        for op in self.all_ops()? {
            max_hlc = Some(match max_hlc {
                Some(h) if h >= op.hlc => h,
                _ => op.hlc,
            });
            doc.apply(&op);
        }
        Ok((doc, max_hlc))
    }
}

/// Wall-clock milliseconds for HLC ticks. The store is a shell, so reading the
/// clock here keeps `budgetcut-core` itself I/O-free (§4).
pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
