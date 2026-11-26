//! Crash recovery detection and session restore evaluation.

use crate::session::{SessionSnapshot, SessionTab};
use common::types::TabId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// Previous shutdown was clean. Standard startup with home page.
    CleanStartup,
    /// Previous session terminated abnormally (crash/power cut). Tabs available for restore.
    CrashRecovered(SessionSnapshot),
}

pub struct CrashRecoveryDetector;

impl CrashRecoveryDetector {
    /// Evaluates whether the loaded session indicates a previous crash.
    pub fn evaluate(raw_tabs: Vec<SessionTab>, clean_shutdown: bool) -> RecoveryStatus {
        if raw_tabs.is_empty() {
            return RecoveryStatus::CleanStartup;
        }

        let active_id = raw_tabs
            .iter()
            .find(|t| t.is_active)
            .map(|t| TabId::new(t.tab_id));

        let snapshot = SessionSnapshot::new(raw_tabs, active_id, clean_shutdown);

        if !clean_shutdown {
            RecoveryStatus::CrashRecovered(snapshot)
        } else {
            RecoveryStatus::CleanStartup
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_recovery_detection() {
        let tabs = vec![SessionTab {
            tab_id: 1,
            url: "https://crashed-work.com".to_string(),
            title: "Work".to_string(),
            is_pinned: false,
            tab_order: 0,
            is_active: true,
        }];

        // Unclean shutdown -> CrashRecovered
        let status = CrashRecoveryDetector::evaluate(tabs.clone(), false);
        match status {
            RecoveryStatus::CrashRecovered(s) => {
                assert_eq!(s.tabs.len(), 1);
                assert_eq!(s.active_tab_id, Some(TabId::new(1)));
            }
            RecoveryStatus::CleanStartup => panic!("Should detect crash!"),
        }

        // Clean shutdown -> CleanStartup
        let clean_status = CrashRecoveryDetector::evaluate(tabs, true);
        assert_eq!(clean_status, RecoveryStatus::CleanStartup);
    }
}
