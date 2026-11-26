//! Error hierarchy for RustBrowse.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RustBrowseError {
    #[error("COM or Windows API error: {0}")]
    Windows(String),

    #[error("WebView2 engine error: {0}")]
    Engine(String),

    #[error("Tab not found: {0}")]
    TabNotFound(u64),

    #[error("Invalid URL format: {0}")]
    InvalidUrl(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Internal channel error: {0}")]
    Channel(String),
}

pub type Result<T> = std::result::Result<T, RustBrowseError>;
