//! Central download manager tracking tasks, states, and protection signals.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::filename::resolve_unique_path;
use crate::task::{DownloadState, DownloadTask};

pub struct DownloadManager {
    tasks: HashMap<u64, DownloadTask>,
    next_task_id: u64,
    default_download_dir: PathBuf,
}

impl DownloadManager {
    pub fn new<P: AsRef<Path>>(default_dir: P) -> Self {
        Self {
            tasks: HashMap::new(),
            next_task_id: 1,
            default_download_dir: default_dir.as_ref().to_path_buf(),
        }
    }

    pub fn default_dir(&self) -> &Path {
        &self.default_download_dir
    }

    /// Registers a new incoming download task with a sanitized, collision-free filename.
    pub fn register_download(
        &mut self,
        url: &str,
        suggested_filename: &str,
        total_bytes: Option<u64>,
        now_ms: u64,
    ) -> u64 {
        let task_id = self.next_task_id;
        self.next_task_id += 1;

        let target_path = resolve_unique_path(&self.default_download_dir, suggested_filename);

        let task = DownloadTask::new(task_id, url.to_string(), target_path, total_bytes, now_ms);

        self.tasks.insert(task_id, task);
        task_id
    }

    pub fn update_progress(&mut self, id: u64, bytes_received: u64, now_ms: u64) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.bytes_received = bytes_received;
            task.last_update_epoch_ms = now_ms;
        }
    }

    pub fn complete_download(&mut self, id: u64) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.state = DownloadState::Completed;
        }
    }

    pub fn fail_download(&mut self, id: u64, reason: String) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.state = DownloadState::Failed(reason);
        }
    }

    pub fn cancel_download(&mut self, id: u64) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.state = DownloadState::Cancelled;
        }
    }

    pub fn pause_download(&mut self, id: u64) {
        if let Some(task) = self.tasks.get_mut(&id) {
            if task.state == DownloadState::InProgress {
                task.state = DownloadState::Paused;
            }
        }
    }

    pub fn resume_download(&mut self, id: u64) {
        if let Some(task) = self.tasks.get_mut(&id) {
            if task.state == DownloadState::Paused {
                task.state = DownloadState::InProgress;
            }
        }
    }

    pub fn get_task(&self, id: u64) -> Option<&DownloadTask> {
        self.tasks.get(&id)
    }

    pub fn all_tasks(&self) -> impl Iterator<Item = &DownloadTask> {
        self.tasks.values()
    }

    /// Checks if any downloads are currently active.
    /// Used by memory management to protect tabs from sleep/discard.
    pub fn has_active_downloads(&self) -> bool {
        self.tasks.values().any(|t| t.is_active())
    }

    pub fn active_download_count(&self) -> usize {
        self.tasks.values().filter(|t| t.is_active()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_manager_lifecycle() {
        let temp_dir = std::env::temp_dir().join("rustbrowse_dl_test");
        std::fs::create_dir_all(&temp_dir).ok();

        let mut dm = DownloadManager::new(&temp_dir);
        assert!(!dm.has_active_downloads());

        let id = dm.register_download(
            "https://example.com/test.iso",
            "test.iso",
            Some(1_000_000),
            1000,
        );

        assert!(dm.has_active_downloads());
        assert_eq!(dm.active_download_count(), 1);

        dm.update_progress(id, 500_000, 2000);
        let task = dm.get_task(id).unwrap();
        assert_eq!(task.bytes_received, 500_000);
        assert_eq!(task.progress_percent(), Some(50.0));

        dm.complete_download(id);
        assert!(!dm.has_active_downloads());
        assert_eq!(dm.get_task(id).unwrap().state, DownloadState::Completed);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
