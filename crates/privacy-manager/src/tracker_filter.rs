//! High-level tracker filter coordinator.

use crate::permissions::PermissionManager;
use crate::rule_engine::{AdblockRuleEngine, FilterDecision};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct TrackerFilter {
    engine: AdblockRuleEngine,
    permissions: PermissionManager,
    blocked_requests_count: AtomicU64,
    allowed_requests_count: AtomicU64,
}

impl TrackerFilter {
    pub fn new(rules: &[String]) -> Self {
        Self {
            engine: AdblockRuleEngine::new(rules),
            permissions: PermissionManager::new(),
            blocked_requests_count: AtomicU64::new(0),
            allowed_requests_count: AtomicU64::new(0),
        }
    }

    pub fn empty() -> Self {
        Self {
            engine: AdblockRuleEngine::empty(),
            permissions: PermissionManager::new(),
            blocked_requests_count: AtomicU64::new(0),
            allowed_requests_count: AtomicU64::new(0),
        }
    }

    pub fn engine(&self) -> &AdblockRuleEngine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut AdblockRuleEngine {
        &mut self.engine
    }

    pub fn permissions(&self) -> &PermissionManager {
        &self.permissions
    }

    pub fn permissions_mut(&mut self) -> &mut PermissionManager {
        &mut self.permissions
    }

    /// Evaluates if an outgoing web resource should be blocked.
    pub fn should_block_resource(&self, url: &str, source_url: &str, resource_type: &str) -> bool {
        let decision = self.engine.check_request(url, source_url, resource_type);
        match decision {
            FilterDecision::Block => {
                self.blocked_requests_count.fetch_add(1, Ordering::Relaxed);
                true
            }
            FilterDecision::Allow | FilterDecision::Whitelisted => {
                self.allowed_requests_count.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    pub fn blocked_count(&self) -> u64 {
        self.blocked_requests_count.load(Ordering::Relaxed)
    }

    pub fn allowed_count(&self) -> u64 {
        self.allowed_requests_count.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_filter_counting() {
        let rules = vec!["||tracker.com^".to_string()];
        let filter = TrackerFilter::new(&rules);

        assert_eq!(filter.blocked_count(), 0);
        assert_eq!(filter.allowed_count(), 0);

        let blocked = filter.should_block_resource(
            "https://tracker.com/pixel.gif",
            "https://test.com",
            "image",
        );
        assert!(blocked);
        assert_eq!(filter.blocked_count(), 1);

        let allowed = filter.should_block_resource(
            "https://cdn.test.com/app.js",
            "https://test.com",
            "script",
        );
        assert!(!allowed);
        assert_eq!(filter.allowed_count(), 1);
    }
}
