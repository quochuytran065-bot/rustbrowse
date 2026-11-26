//! SQLite-backed Lazy Session Restore Manager for RustBrowse
//! Saves open tabs and restores session with zero RAM penalty on startup.

use rusqlite::{params, Connection};
use std::fs;
use std::path::PathBuf;
use tracing::info;

pub struct SessionStorage;

impl SessionStorage {
    fn db_path() -> PathBuf {
        let dir = if let Ok(local) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local)
        } else if let Ok(userprofile) = std::env::var("USERPROFILE") {
            PathBuf::from(userprofile).join("AppData").join("Local")
        } else {
            PathBuf::from("D:\\RustBrowse\\UserData")
        };
        let target = dir.join("RustBrowse").join("User Data");
        let _ = fs::create_dir_all(&target);
        target.join("session.db")
    }

    fn open_db() -> Result<Connection, rusqlite::Error> {
        let path = Self::db_path();
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS session_tabs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                tab_order INTEGER NOT NULL
            );",
            [],
        )?;
        Ok(conn)
    }

    /// Saves list of active tabs to SQLite database.
    pub fn save_session(tabs: &[crate::tabs::TabInfo]) {
        if let Ok(mut conn) = Self::open_db() {
            let tx = match conn.transaction() {
                Ok(t) => t,
                Err(_) => return,
            };

            let _ = tx.execute("DELETE FROM session_tabs;", []);
            for (idx, tab) in tabs.iter().enumerate() {
                // Don't save internal diagnostic suite or blank tabs
                if tab.url.starts_with("rustbrowse://") || tab.url == "about:blank" {
                    continue;
                }
                let _ = tx.execute(
                    "INSERT INTO session_tabs (url, title, tab_order) VALUES (?1, ?2, ?3);",
                    params![tab.url, tab.title, idx as i64],
                );
            }
            let _ = tx.commit();
            info!("Session state saved to SQLite database.");
        }
    }

    /// Loads saved session tabs from SQLite database.
    pub fn load_session() -> Vec<(String, String)> {
        let mut results = Vec::new();
        if let Ok(conn) = Self::open_db() {
            if let Ok(mut stmt) =
                conn.prepare("SELECT url, title FROM session_tabs ORDER BY tab_order ASC;")
            {
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                });
                if let Ok(mapped) = rows {
                    for item in mapped.flatten() {
                        results.push(item);
                    }
                }
            }
        }
        results
    }
}
