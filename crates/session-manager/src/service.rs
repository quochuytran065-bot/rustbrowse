//! Unified session, history, and bookmark service interacting with DbWorker.

use std::time::{SystemTime, UNIX_EPOCH};

use common::errors::{Result, RustBrowseError};
use common::types::{TabId, TabMetadata};
use storage::{BookmarkEntry, DbCommand, HistoryEntry, SessionTabEntry};
use tokio::sync::{mpsc, oneshot};

use crate::recovery::{CrashRecoveryDetector, RecoveryStatus};
use crate::session::SessionTab;

pub struct SessionService {
    db_sender: mpsc::UnboundedSender<DbCommand>,
}

impl SessionService {
    pub fn new(db_sender: mpsc::UnboundedSender<DbCommand>) -> Self {
        Self { db_sender }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Records a visited URL into browsing history.
    pub fn record_history_visit(&self, url: &str, title: &str) {
        let _ = self.db_sender.send(DbCommand::RecordVisit {
            url: url.to_string(),
            title: title.to_string(),
            time_ms: Self::now_ms(),
        });
    }

    /// Queries the most recent history records.
    pub fn get_recent_history(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let (tx, rx) = oneshot::channel();
        self.db_sender
            .send(DbCommand::QueryRecentHistory { limit, reply: tx })
            .map_err(|e| RustBrowseError::Channel(e.to_string()))?;

        rx.blocking_recv()
            .map_err(|e| RustBrowseError::Channel(e.to_string()))
    }

    /// Searches history records matching query.
    pub fn search_history(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        let (tx, rx) = oneshot::channel();
        self.db_sender
            .send(DbCommand::SearchHistory {
                query: query.to_string(),
                limit,
                reply: tx,
            })
            .map_err(|e| RustBrowseError::Channel(e.to_string()))?;

        rx.blocking_recv()
            .map_err(|e| RustBrowseError::Channel(e.to_string()))
    }

    /// Adds or updates a bookmark.
    pub fn add_bookmark(&self, url: &str, title: &str) {
        let _ = self.db_sender.send(DbCommand::AddBookmark {
            url: url.to_string(),
            title: title.to_string(),
            time_ms: Self::now_ms(),
        });
    }

    /// Removes a bookmark.
    pub fn remove_bookmark(&self, url: &str) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.db_sender
            .send(DbCommand::RemoveBookmark {
                url: url.to_string(),
                reply: tx,
            })
            .map_err(|e| RustBrowseError::Channel(e.to_string()))?;

        rx.blocking_recv()
            .map_err(|e| RustBrowseError::Channel(e.to_string()))
    }

    /// Checks if a URL is bookmarked.
    pub fn is_bookmarked(&self, url: &str) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.db_sender
            .send(DbCommand::IsBookmarked {
                url: url.to_string(),
                reply: tx,
            })
            .map_err(|e| RustBrowseError::Channel(e.to_string()))?;

        rx.blocking_recv()
            .map_err(|e| RustBrowseError::Channel(e.to_string()))
    }

    /// Queries all saved bookmarks.
    pub fn get_all_bookmarks(&self) -> Result<Vec<BookmarkEntry>> {
        let (tx, rx) = oneshot::channel();
        self.db_sender
            .send(DbCommand::QueryBookmarks { reply: tx })
            .map_err(|e| RustBrowseError::Channel(e.to_string()))?;

        rx.blocking_recv()
            .map_err(|e| RustBrowseError::Channel(e.to_string()))
    }

    /// Persists a snapshot of currently open tabs to database (debounced via worker).
    pub fn save_session_snapshot(
        &self,
        tabs: &[TabMetadata],
        active_id: Option<TabId>,
        clean_shutdown: bool,
    ) {
        let mut entries = Vec::with_capacity(tabs.len());
        for (i, tab) in tabs.iter().enumerate() {
            let is_active = Some(tab.id) == active_id;
            entries.push(SessionTabEntry {
                tab_id: tab.id.0,
                url: tab.url.clone(),
                title: tab.title.clone(),
                is_pinned: tab.is_pinned,
                tab_order: i as i32,
                is_active,
            });
        }

        let _ = self.db_sender.send(DbCommand::SaveSessionSnapshot {
            tabs: entries,
            clean_shutdown,
        });
    }

    /// Checks the previous session for abnormal termination / crash recovery.
    pub fn check_recovery(&self) -> Result<RecoveryStatus> {
        let (tx, rx) = oneshot::channel();
        self.db_sender
            .send(DbCommand::QueryLastSession { reply: tx })
            .map_err(|e| RustBrowseError::Channel(e.to_string()))?;

        let (entries, clean_shutdown) = rx
            .blocking_recv()
            .map_err(|e| RustBrowseError::Channel(e.to_string()))?;

        let session_tabs = entries
            .into_iter()
            .map(SessionTab::from_storage_entry)
            .collect();

        Ok(CrashRecoveryDetector::evaluate(
            session_tabs,
            clean_shutdown,
        ))
    }

    /// Marks the session as cleanly shut down on normal application exit.
    pub fn mark_clean_shutdown(&self) {
        let _ = self
            .db_sender
            .send(DbCommand::SetCleanShutdown { clean: true });
        let _ = self.db_sender.send(DbCommand::Flush);
    }
}
