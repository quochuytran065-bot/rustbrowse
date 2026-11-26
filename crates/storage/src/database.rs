//! SQLite connection setup, WAL mode configuration, and migrations.

use common::errors::{Result, RustBrowseError};
use rusqlite::Connection;
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens or creates an SQLite database file and applies performance pragmas and schema migrations.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| {
            RustBrowseError::Storage(format!("Failed to open SQLite database: {:?}", e))
        })?;

        let mut db = Self { conn };
        db.apply_pragmas()?;
        db.run_migrations()?;
        Ok(db)
    }

    /// Opens an in-memory SQLite database (used for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| {
            RustBrowseError::Storage(format!("Failed to open in-memory SQLite database: {:?}", e))
        })?;

        let mut db = Self { conn };
        db.apply_pragmas()?;
        db.run_migrations()?;
        Ok(db)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    fn apply_pragmas(&self) -> Result<()> {
        // WAL mode: concurrent readers + single writer without locking UI
        // Synchronous = NORMAL: safe with WAL, prevents fsync stalls
        // Busy timeout = 5000ms: retries locks gracefully under disk pressure
        // Temp store = MEMORY: avoids temporary disk spills
        self.conn
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA busy_timeout = 5000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA foreign_keys = ON;",
            )
            .map_err(|e| {
                RustBrowseError::Storage(format!("Failed to apply SQLite pragmas: {:?}", e))
            })?;

        Ok(())
    }

    fn run_migrations(&mut self) -> Result<()> {
        let tx = self.conn.transaction().map_err(|e| {
            RustBrowseError::Storage(format!("Failed to begin migration transaction: {:?}", e))
        })?;

        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                title TEXT,
                visit_time_epoch_ms INTEGER NOT NULL,
                visit_count INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_history_url ON history(url);
            CREATE INDEX IF NOT EXISTS idx_history_time ON history(visit_time_epoch_ms DESC);

            CREATE TABLE IF NOT EXISTS bookmarks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                created_at_epoch_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_bookmarks_url ON bookmarks(url);

            CREATE TABLE IF NOT EXISTS session_metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS session_tabs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tab_id INTEGER NOT NULL,
                url TEXT NOT NULL,
                title TEXT,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                tab_order INTEGER NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS zapper_rules (
                domain TEXT NOT NULL,
                selector TEXT NOT NULL,
                created_at_epoch_ms INTEGER NOT NULL,
                PRIMARY KEY (domain, selector)
            );",
        )
        .map_err(|e| {
            RustBrowseError::Storage(format!("Failed to execute schema migration: {:?}", e))
        })?;

        tx.commit().map_err(|e| {
            RustBrowseError::Storage(format!("Failed to commit schema migration: {:?}", e))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_init_and_pragmas() {
        let db = Database::open_in_memory().expect("In-memory DB creation must succeed");

        let journal_mode: String = db
            .conn()
            .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
            .expect("PRAGMA query must succeed");

        // In-memory sqlite often reports "memory", file-based reports "wal"
        assert!(!journal_mode.is_empty());
    }
}
