//! Structured crash report generation and persistence.

use crate::diagnostics::SystemDiagnostics;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    pub timestamp_epoch_ms: u64,
    pub reason: String,
    pub active_url: Option<String>,
    pub diagnostics: SystemDiagnostics,
}

impl CrashReport {
    pub fn new(
        reason: String,
        active_url: Option<String>,
        diagnostics: SystemDiagnostics,
        now_ms: u64,
    ) -> Self {
        Self {
            timestamp_epoch_ms: now_ms,
            reason,
            active_url,
            diagnostics,
        }
    }

    /// Persists the crash report as a JSON file in the target crash directory.
    pub fn save_to_dir<P: AsRef<Path>>(&self, dir: P) -> std::io::Result<PathBuf> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;

        let filename = format!("crash_{}.json", self.timestamp_epoch_ms);
        let path = dir.join(filename);

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, json)?;

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_report_serialization() {
        let temp_dir = std::env::temp_dir().join("rustbrowse_crash_test");
        let diag = SystemDiagnostics::gather(4096);
        let report = CrashReport::new(
            "Renderer process crashed".to_string(),
            Some("https://crash.com".to_string()),
            diag,
            1234567890,
        );

        let path = report.save_to_dir(&temp_dir).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Renderer process crashed"));
        assert!(content.contains("https://crash.com"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
