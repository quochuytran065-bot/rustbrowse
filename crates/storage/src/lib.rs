//! Storage crate providing embedded SQLite in WAL mode with debounced background writing.

pub mod database;
pub mod repositories;
pub mod worker;

pub use database::Database;
pub use repositories::{
    BookmarkEntry, BookmarkRepository, HistoryEntry, HistoryRepository, SessionRepository,
    SessionTabEntry, SettingsRepository, ZapperRepository,
};
pub use worker::{DbCommand, DbWorkerHandle};
