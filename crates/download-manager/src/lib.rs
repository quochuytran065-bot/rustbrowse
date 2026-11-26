//! Download management for RustBrowse.

pub mod filename;
pub mod manager;
pub mod task;

pub use filename::{resolve_unique_path, sanitize_filename};
pub use manager::DownloadManager;
pub use task::{DownloadState, DownloadTask};
