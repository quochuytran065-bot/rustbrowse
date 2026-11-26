//! Data access repositories for History, Bookmarks, and Session Tabs.

use common::errors::{Result, RustBrowseError};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub visit_time_epoch_ms: u64,
    pub visit_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub created_at_epoch_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTabEntry {
    pub tab_id: u64,
    pub url: String,
    pub title: String,
    pub is_pinned: bool,
    pub tab_order: i32,
    pub is_active: bool,
}

pub struct HistoryRepository;

impl HistoryRepository {
    /// Records a URL visit: updates visit count and time if already exists, or inserts new record.
    pub fn record_visit(conn: &Connection, url: &str, title: &str, time_ms: u64) -> Result<()> {
        let mut stmt = conn
            .prepare("SELECT id, visit_count FROM history WHERE url = ?1 LIMIT 1")
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        let existing = stmt
            .query_row(params![url], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, u32>(1)?))
            })
            .ok();

        if let Some((id, count)) = existing {
            conn.execute(
                "UPDATE history SET title = ?1, visit_time_epoch_ms = ?2, visit_count = ?3 WHERE id = ?4",
                params![title, time_ms, count + 1, id],
            )
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        } else {
            conn.execute(
                "INSERT INTO history (url, title, visit_time_epoch_ms, visit_count) VALUES (?1, ?2, ?3, 1)",
                params![url, title, time_ms],
            )
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    /// Queries the most recent history entries.
    pub fn get_recent(conn: &Connection, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut stmt = conn
            .prepare(
                "SELECT id, url, title, visit_time_epoch_ms, visit_count 
                 FROM history 
                 ORDER BY visit_time_epoch_ms DESC 
                 LIMIT ?1",
            )
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    visit_time_epoch_ms: row.get(3)?,
                    visit_count: row.get(4)?,
                })
            })
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        let mut entries = Vec::new();
        for r in rows {
            entries.push(r.map_err(|e| RustBrowseError::Storage(e.to_string()))?);
        }
        Ok(entries)
    }

    /// Searches history by URL or title substring.
    pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        let pattern = format!("%{}%", query);
        let mut stmt = conn
            .prepare(
                "SELECT id, url, title, visit_time_epoch_ms, visit_count 
                 FROM history 
                 WHERE url LIKE ?1 OR title LIKE ?1 
                 ORDER BY visit_time_epoch_ms DESC 
                 LIMIT ?2",
            )
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        let rows = stmt
            .query_map(params![pattern, limit as i64], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    visit_time_epoch_ms: row.get(3)?,
                    visit_count: row.get(4)?,
                })
            })
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        let mut entries = Vec::new();
        for r in rows {
            entries.push(r.map_err(|e| RustBrowseError::Storage(e.to_string()))?);
        }
        Ok(entries)
    }

    pub fn clear(conn: &Connection) -> Result<()> {
        conn.execute("DELETE FROM history", [])
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        Ok(())
    }
}

pub struct BookmarkRepository;

impl BookmarkRepository {
    pub fn add(conn: &Connection, url: &str, title: &str, time_ms: u64) -> Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO bookmarks (url, title, created_at_epoch_ms) VALUES (?1, ?2, ?3)",
            params![url, title, time_ms],
        )
        .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn remove(conn: &Connection, url: &str) -> Result<bool> {
        let changed = conn
            .execute("DELETE FROM bookmarks WHERE url = ?1", params![url])
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        Ok(changed > 0)
    }

    pub fn is_bookmarked(conn: &Connection, url: &str) -> Result<bool> {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM bookmarks WHERE url = ?1",
                params![url],
                |row| row.get(0),
            )
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        Ok(count > 0)
    }

    pub fn get_all(conn: &Connection) -> Result<Vec<BookmarkEntry>> {
        let mut stmt = conn
            .prepare("SELECT id, url, title, created_at_epoch_ms FROM bookmarks ORDER BY id ASC")
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(BookmarkEntry {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    created_at_epoch_ms: row.get(3)?,
                })
            })
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        let mut entries = Vec::new();
        for r in rows {
            entries.push(r.map_err(|e| RustBrowseError::Storage(e.to_string()))?);
        }
        Ok(entries)
    }
}

pub struct SessionRepository;

impl SessionRepository {
    /// Saves the active tabs and sets the clean shutdown flag.
    pub fn save_session(
        conn: &mut Connection,
        tabs: &[SessionTabEntry],
        clean_shutdown: bool,
    ) -> Result<()> {
        let tx = conn
            .transaction()
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        // 1. Clear previous session tabs
        tx.execute("DELETE FROM session_tabs", [])
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        // 2. Insert current tabs in transaction
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO session_tabs (tab_id, url, title, is_pinned, tab_order, is_active) 
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

            for tab in tabs {
                stmt.execute(params![
                    tab.tab_id as i64,
                    tab.url,
                    tab.title,
                    if tab.is_pinned { 1 } else { 0 },
                    tab.tab_order,
                    if tab.is_active { 1 } else { 0 },
                ])
                .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
            }
        }

        // 3. Update clean shutdown metadata
        tx.execute(
            "INSERT OR REPLACE INTO session_metadata (key, value) VALUES ('clean_shutdown', ?1)",
            params![if clean_shutdown { "true" } else { "false" }],
        )
        .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        tx.commit()
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Loads the saved session and checks if the previous shutdown was clean.
    pub fn load_session(conn: &Connection) -> Result<(Vec<SessionTabEntry>, bool)> {
        // Query clean shutdown flag
        let clean_shutdown: bool = conn
            .query_row(
                "SELECT value FROM session_metadata WHERE key = 'clean_shutdown'",
                [],
                |row| {
                    let val: String = row.get(0)?;
                    Ok(val == "true")
                },
            )
            .unwrap_or(true); // Default to true if first run

        // Query saved tabs
        let mut stmt = conn
            .prepare(
                "SELECT tab_id, url, title, is_pinned, tab_order, is_active 
                 FROM session_tabs 
                 ORDER BY tab_order ASC",
            )
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(SessionTabEntry {
                    tab_id: row.get::<_, i64>(0)? as u64,
                    url: row.get(1)?,
                    title: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    is_pinned: row.get::<_, i32>(3)? != 0,
                    tab_order: row.get(4)?,
                    is_active: row.get::<_, i32>(5)? != 0,
                })
            })
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;

        let mut tabs = Vec::new();
        for r in rows {
            tabs.push(r.map_err(|e| RustBrowseError::Storage(e.to_string()))?);
        }

        Ok((tabs, clean_shutdown))
    }

    /// Marks whether the current session was closed cleanly.
    pub fn set_clean_shutdown(conn: &Connection, clean: bool) -> Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO session_metadata (key, value) VALUES ('clean_shutdown', ?1)",
            params![if clean { "true" } else { "false" }],
        )
        .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        Ok(())
    }
}

pub struct SettingsRepository;

impl SettingsRepository {
    pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        let res = stmt.query_row(params![key], |row| row.get(0)).ok();
        Ok(res)
    }

    pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )
        .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        Ok(())
    }
}

pub struct ZapperRepository;

impl ZapperRepository {
    pub fn add(conn: &Connection, domain: &str, selector: &str, time_ms: u64) -> Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO zapper_rules (domain, selector, created_at_epoch_ms) VALUES (?1, ?2, ?3)",
            params![domain, selector, time_ms],
        )
        .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_rules_for_domain(conn: &Connection, domain: &str) -> Result<Vec<String>> {
        let mut stmt = conn
            .prepare("SELECT selector FROM zapper_rules WHERE domain = ?1")
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map(params![domain], |row| row.get::<_, String>(0))
            .map_err(|e| RustBrowseError::Storage(e.to_string()))?;
        let mut rules = Vec::new();
        for r in rows.flatten() {
            rules.push(r);
        }
        Ok(rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    #[test]
    fn test_history_repository() {
        let mut db = Database::open_in_memory().unwrap();
        let conn = db.conn_mut();

        HistoryRepository::record_visit(conn, "https://example.com", "Example", 1000).unwrap();
        HistoryRepository::record_visit(conn, "https://example.com", "Example New", 2000).unwrap();
        HistoryRepository::record_visit(conn, "https://rust-lang.org", "Rust", 3000).unwrap();

        let recent = HistoryRepository::get_recent(conn, 10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].url, "https://rust-lang.org");
        assert_eq!(recent[1].url, "https://example.com");
        assert_eq!(recent[1].visit_count, 2);

        let searched = HistoryRepository::search(conn, "Rust", 10).unwrap();
        assert_eq!(searched.len(), 1);
        assert_eq!(searched[0].url, "https://rust-lang.org");
    }

    #[test]
    fn test_bookmark_repository() {
        let mut db = Database::open_in_memory().unwrap();
        let conn = db.conn_mut();

        BookmarkRepository::add(conn, "https://crates.io", "Crates", 1000).unwrap();
        assert!(BookmarkRepository::is_bookmarked(conn, "https://crates.io").unwrap());
        assert!(!BookmarkRepository::is_bookmarked(conn, "https://github.com").unwrap());

        let all = BookmarkRepository::get_all(conn).unwrap();
        assert_eq!(all.len(), 1);

        let removed = BookmarkRepository::remove(conn, "https://crates.io").unwrap();
        assert!(removed);
        assert!(!BookmarkRepository::is_bookmarked(conn, "https://crates.io").unwrap());
    }

    #[test]
    fn test_session_repository() {
        let mut db = Database::open_in_memory().unwrap();
        let conn = db.conn_mut();

        let tabs = vec![
            SessionTabEntry {
                tab_id: 1,
                url: "https://tab1.com".to_string(),
                title: "Tab 1".to_string(),
                is_pinned: true,
                tab_order: 0,
                is_active: false,
            },
            SessionTabEntry {
                tab_id: 2,
                url: "https://tab2.com".to_string(),
                title: "Tab 2".to_string(),
                is_pinned: false,
                tab_order: 1,
                is_active: true,
            },
        ];

        SessionRepository::save_session(conn, &tabs, false).unwrap();
        let (loaded, clean) = SessionRepository::load_session(conn).unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(!clean);
        assert!(loaded[0].is_pinned);
        assert!(loaded[1].is_active);

        SessionRepository::set_clean_shutdown(conn, true).unwrap();
        let (_, clean_after) = SessionRepository::load_session(conn).unwrap();
        assert!(clean_after);
    }
}
