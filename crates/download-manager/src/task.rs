//! Individual download task state and progress calculation.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    InProgress,
    Paused,
    Completed,
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: u64,
    pub url: String,
    pub target_path: PathBuf,
    pub total_bytes: Option<u64>,
    pub bytes_received: u64,
    pub state: DownloadState,
    pub start_epoch_ms: u64,
    pub last_update_epoch_ms: u64,
}

impl DownloadTask {
    pub fn new(
        id: u64,
        url: String,
        target_path: PathBuf,
        total_bytes: Option<u64>,
        now_ms: u64,
    ) -> Self {
        Self {
            id,
            url,
            target_path,
            total_bytes,
            bytes_received: 0,
            state: DownloadState::InProgress,
            start_epoch_ms: now_ms,
            last_update_epoch_ms: now_ms,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, DownloadState::InProgress)
    }

    pub fn is_finished(&self) -> bool {
        matches!(
            self.state,
            DownloadState::Completed | DownloadState::Cancelled | DownloadState::Failed(_)
        )
    }

    /// Progress percentage from 0.0 to 100.0, or None if Content-Length unknown.
    pub fn progress_percent(&self) -> Option<f32> {
        let total = self.total_bytes?;
        if total == 0 {
            return Some(100.0);
        }
        let pct = (self.bytes_received as f64 / total as f64) * 100.0;
        Some(pct.clamp(0.0, 100.0) as f32)
    }

    /// Approximate download speed in bytes per second.
    pub fn speed_bps(&self, now_ms: u64) -> f64 {
        let elapsed_ms = now_ms.saturating_sub(self.start_epoch_ms);
        if elapsed_ms < 500 {
            return 0.0;
        }
        (self.bytes_received as f64) / (elapsed_ms as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_task_progress() {
        let task = DownloadTask {
            id: 1,
            url: "https://example.com/file.zip".to_string(),
            target_path: PathBuf::from("D:\\file.zip"),
            total_bytes: Some(1000),
            bytes_received: 450,
            state: DownloadState::InProgress,
            start_epoch_ms: 0,
            last_update_epoch_ms: 1000,
        };

        assert_eq!(task.progress_percent(), Some(45.0));
        assert!(task.is_active());
        assert!(!task.is_finished());

        let speed = task.speed_bps(2000);
        assert_eq!(speed, 225.0); // 450 bytes / 2.0 s
    }
}
