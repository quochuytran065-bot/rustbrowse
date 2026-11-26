//! Dedicated background database worker thread with debounced batching.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use common::errors::{Result, RustBrowseError};
use tokio::sync::{mpsc, oneshot};

use crate::database::Database;
use crate::repositories::{
    BookmarkEntry, BookmarkRepository, HistoryEntry, HistoryRepository, SessionRepository,
    SessionTabEntry,
};

pub enum DbCommand {
    RecordVisit {
        url: String,
        title: String,
        time_ms: u64,
    },
    SaveSessionSnapshot {
        tabs: Vec<SessionTabEntry>,
        clean_shutdown: bool,
    },
    AddBookmark {
        url: String,
        title: String,
        time_ms: u64,
    },
    RemoveBookmark {
        url: String,
        reply: oneshot::Sender<bool>,
    },
    IsBookmarked {
        url: String,
        reply: oneshot::Sender<bool>,
    },
    QueryRecentHistory {
        limit: usize,
        reply: oneshot::Sender<Vec<HistoryEntry>>,
    },
    SearchHistory {
        query: String,
        limit: usize,
        reply: oneshot::Sender<Vec<HistoryEntry>>,
    },
    QueryBookmarks {
        reply: oneshot::Sender<Vec<BookmarkEntry>>,
    },
    QueryLastSession {
        reply: oneshot::Sender<(Vec<SessionTabEntry>, bool)>,
    },
    SetCleanShutdown {
        clean: bool,
    },
    GetSetting {
        key: String,
        reply: oneshot::Sender<Option<String>>,
    },
    SetSetting {
        key: String,
        value: String,
    },
    AddZapperRule {
        domain: String,
        selector: String,
        time_ms: u64,
    },
    GetZapperRules {
        domain: String,
        reply: oneshot::Sender<Vec<String>>,
    },
    Flush,
    Shutdown,
}

pub struct DbWorkerHandle {
    sender: mpsc::UnboundedSender<DbCommand>,
    is_running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl DbWorkerHandle {
    pub fn start<P: Into<PathBuf>>(db_path: P) -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel::<DbCommand>();
        let path: PathBuf = db_path.into();
        let is_running = Arc::new(AtomicBool::new(true));
        let running_clone = is_running.clone();

        let handle = thread::Builder::new()
            .name("RustBrowse-DbWorker".to_string())
            .spawn(move || {
                run_db_worker(path, rx, running_clone);
            })
            .map_err(|e| {
                RustBrowseError::Storage(format!("Failed to spawn DB worker thread: {:?}", e))
            })?;

        Ok(Self {
            sender: tx,
            is_running,
            thread_handle: Some(handle),
        })
    }

    /// Starts an in-memory worker (ideal for unit testing).
    pub fn start_in_memory() -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel::<DbCommand>();
        let is_running = Arc::new(AtomicBool::new(true));
        let running_clone = is_running.clone();

        let handle = thread::Builder::new()
            .name("RustBrowse-DbWorker-Mem".to_string())
            .spawn(move || {
                run_in_memory_db_worker(rx, running_clone);
            })
            .map_err(|e| {
                RustBrowseError::Storage(format!("Failed to spawn in-memory DB worker: {:?}", e))
            })?;

        Ok(Self {
            sender: tx,
            is_running,
            thread_handle: Some(handle),
        })
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<DbCommand> {
        self.sender.clone()
    }

    pub fn shutdown(&mut self) {
        if self.is_running.swap(false, Ordering::SeqCst) {
            let _ = self.sender.send(DbCommand::Shutdown);
            if let Some(h) = self.thread_handle.take() {
                let _ = h.join();
            }
        }
    }
}

impl Drop for DbWorkerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_db_worker(
    path: PathBuf,
    mut rx: mpsc::UnboundedReceiver<DbCommand>,
    is_running: Arc<AtomicBool>,
) {
    let mut db = match Database::open(&path) {
        Ok(db) => db,
        Err(e) => {
            tracing::error!(error = ?e, "DbWorker failed to open database file");
            return;
        }
    };

    worker_loop(&mut db, &mut rx, is_running);
}

fn run_in_memory_db_worker(
    mut rx: mpsc::UnboundedReceiver<DbCommand>,
    is_running: Arc<AtomicBool>,
) {
    let mut db = match Database::open_in_memory() {
        Ok(db) => db,
        Err(e) => {
            tracing::error!(error = ?e, "DbWorker failed to open in-memory database");
            return;
        }
    };

    worker_loop(&mut db, &mut rx, is_running);
}

fn worker_loop(
    db: &mut Database,
    rx: &mut mpsc::UnboundedReceiver<DbCommand>,
    is_running: Arc<AtomicBool>,
) {
    let mut pending_history: Vec<(String, String, u64)> = Vec::new();
    let mut pending_session: Option<(Vec<SessionTabEntry>, bool)> = None;
    let mut last_flush = Instant::now();
    let debounce_interval = Duration::from_secs(2);

    while is_running.load(Ordering::SeqCst) {
        // Drain incoming commands without blocking indefinitely
        let mut got_command = false;
        while let Ok(cmd) = rx.try_recv() {
            got_command = true;
            match cmd {
                DbCommand::RecordVisit {
                    url,
                    title,
                    time_ms,
                } => {
                    pending_history.push((url, title, time_ms));
                }
                DbCommand::SaveSessionSnapshot {
                    tabs,
                    clean_shutdown,
                } => {
                    pending_session = Some((tabs, clean_shutdown));
                }
                DbCommand::AddBookmark {
                    url,
                    title,
                    time_ms,
                } => {
                    let _ = BookmarkRepository::add(db.conn(), &url, &title, time_ms);
                }
                DbCommand::RemoveBookmark { url, reply } => {
                    let res = BookmarkRepository::remove(db.conn(), &url).unwrap_or(false);
                    let _ = reply.send(res);
                }
                DbCommand::IsBookmarked { url, reply } => {
                    let res = BookmarkRepository::is_bookmarked(db.conn(), &url).unwrap_or(false);
                    let _ = reply.send(res);
                }
                DbCommand::QueryRecentHistory { limit, reply } => {
                    flush_pending(db, &mut pending_history, &mut pending_session);
                    let res = HistoryRepository::get_recent(db.conn(), limit).unwrap_or_default();
                    let _ = reply.send(res);
                }
                DbCommand::SearchHistory {
                    query,
                    limit,
                    reply,
                } => {
                    flush_pending(db, &mut pending_history, &mut pending_session);
                    let res =
                        HistoryRepository::search(db.conn(), &query, limit).unwrap_or_default();
                    let _ = reply.send(res);
                }
                DbCommand::QueryBookmarks { reply } => {
                    let res = BookmarkRepository::get_all(db.conn()).unwrap_or_default();
                    let _ = reply.send(res);
                }
                DbCommand::QueryLastSession { reply } => {
                    flush_pending(db, &mut pending_history, &mut pending_session);
                    let res = SessionRepository::load_session(db.conn())
                        .unwrap_or_else(|_| (Vec::new(), true));
                    let _ = reply.send(res);
                }
                DbCommand::SetCleanShutdown { clean } => {
                    let _ = SessionRepository::set_clean_shutdown(db.conn(), clean);
                }
                DbCommand::GetSetting { key, reply } => {
                    let res = crate::repositories::SettingsRepository::get(db.conn(), &key).unwrap_or(None);
                    let _ = reply.send(res);
                }
                DbCommand::SetSetting { key, value } => {
                    let _ = crate::repositories::SettingsRepository::set(db.conn(), &key, &value);
                }
                DbCommand::AddZapperRule { domain, selector, time_ms } => {
                    let _ = crate::repositories::ZapperRepository::add(db.conn(), &domain, &selector, time_ms);
                }
                DbCommand::GetZapperRules { domain, reply } => {
                    let res = crate::repositories::ZapperRepository::get_rules_for_domain(db.conn(), &domain).unwrap_or_default();
                    let _ = reply.send(res);
                }
                DbCommand::Flush => {
                    flush_pending(db, &mut pending_history, &mut pending_session);
                    last_flush = Instant::now();
                }
                DbCommand::Shutdown => {
                    flush_pending(db, &mut pending_history, &mut pending_session);
                    is_running.store(false, Ordering::SeqCst);
                    return;
                }
            }
        }

        // Debounced flush (every 2s if pending items exist)
        if last_flush.elapsed() >= debounce_interval
            && (!pending_history.is_empty() || pending_session.is_some())
        {
            flush_pending(db, &mut pending_history, &mut pending_session);
            last_flush = Instant::now();
        }

        if !got_command {
            thread::sleep(Duration::from_millis(50));
        }
    }

    // Final flush on exit
    flush_pending(db, &mut pending_history, &mut pending_session);
}

fn flush_pending(
    db: &mut Database,
    pending_history: &mut Vec<(String, String, u64)>,
    pending_session: &mut Option<(Vec<SessionTabEntry>, bool)>,
) {
    if !pending_history.is_empty() {
        if let Ok(tx) = db.conn_mut().transaction() {
            for (url, title, time_ms) in pending_history.drain(..) {
                let _ = HistoryRepository::record_visit(&tx, &url, &title, time_ms);
            }
            let _ = tx.commit();
        }
    }

    if let Some((tabs, clean)) = pending_session.take() {
        let _ = SessionRepository::save_session(db.conn_mut(), &tabs, clean);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_async_record_and_query() {
        let mut handle = DbWorkerHandle::start_in_memory().unwrap();
        let sender = handle.sender();

        // 1. Send visits
        sender
            .send(DbCommand::RecordVisit {
                url: "https://site-a.com".to_string(),
                title: "Site A".to_string(),
                time_ms: 1000,
            })
            .unwrap();

        sender
            .send(DbCommand::RecordVisit {
                url: "https://site-b.com".to_string(),
                title: "Site B".to_string(),
                time_ms: 2000,
            })
            .unwrap();

        // 2. Query recent (forces flush)
        let (tx, rx) = oneshot::channel();
        sender
            .send(DbCommand::QueryRecentHistory {
                limit: 10,
                reply: tx,
            })
            .unwrap();

        let recent = rx.blocking_recv().unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].url, "https://site-b.com");

        // 3. Save session
        let tabs = vec![SessionTabEntry {
            tab_id: 1,
            url: "https://site-a.com".to_string(),
            title: "Site A".to_string(),
            is_pinned: false,
            tab_order: 0,
            is_active: true,
        }];
        sender
            .send(DbCommand::SaveSessionSnapshot {
                tabs,
                clean_shutdown: false,
            })
            .unwrap();

        let (s_tx, s_rx) = oneshot::channel();
        sender
            .send(DbCommand::QueryLastSession { reply: s_tx })
            .unwrap();
        let (loaded_tabs, clean) = s_rx.blocking_recv().unwrap();
        assert_eq!(loaded_tabs.len(), 1);
        assert!(!clean);

        handle.shutdown();
    }
}
