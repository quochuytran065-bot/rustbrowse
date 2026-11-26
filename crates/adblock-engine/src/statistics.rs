//! Real-time statistics tracking for blocked ads, trackers, and saved bandwidth.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DomainStats {
    pub domain: String,
    pub ads_blocked: u64,
    pub trackers_blocked: u64,
    pub bandwidth_saved_bytes: u64,
    pub last_blocked_epoch_ms: u64,
}

#[derive(Debug, Default)]
pub struct AdblockStats {
    total_ads_blocked: AtomicU64,
    total_trackers_blocked: AtomicU64,
    total_bandwidth_saved_bytes: AtomicU64,
    domains: RwLock<HashMap<String, DomainStats>>,
}

impl AdblockStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a blocked network resource.
    pub fn record_blocked(
        &self,
        domain: &str,
        is_tracker: bool,
        estimated_size_bytes: u64,
        epoch_ms: u64,
    ) {
        if is_tracker {
            self.total_trackers_blocked.fetch_add(1, Ordering::Relaxed);
        } else {
            self.total_ads_blocked.fetch_add(1, Ordering::Relaxed);
        }
        self.total_bandwidth_saved_bytes
            .fetch_add(estimated_size_bytes, Ordering::Relaxed);

        let domain_key = domain.to_lowercase();
        if let Ok(mut map) = self.domains.write() {
            let entry = map.entry(domain_key.clone()).or_insert_with(|| DomainStats {
                domain: domain_key,
                ads_blocked: 0,
                trackers_blocked: 0,
                bandwidth_saved_bytes: 0,
                last_blocked_epoch_ms: epoch_ms,
            });

            if is_tracker {
                entry.trackers_blocked += 1;
            } else {
                entry.ads_blocked += 1;
            }
            entry.bandwidth_saved_bytes += estimated_size_bytes;
            entry.last_blocked_epoch_ms = epoch_ms;
        }
    }

    pub fn total_ads_blocked(&self) -> u64 {
        self.total_ads_blocked.load(Ordering::Relaxed)
    }

    pub fn total_trackers_blocked(&self) -> u64 {
        self.total_trackers_blocked.load(Ordering::Relaxed)
    }

    pub fn total_bandwidth_saved_bytes(&self) -> u64 {
        self.total_bandwidth_saved_bytes.load(Ordering::Relaxed)
    }

    pub fn stats_for_domain(&self, domain: &str) -> Option<DomainStats> {
        let key = domain.to_lowercase();
        self.domains.read().ok().and_then(|m| m.get(&key).cloned())
    }

    pub fn all_domain_stats(&self) -> Vec<DomainStats> {
        self.domains
            .read()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_aggregate_stats() {
        let stats = AdblockStats::new();
        stats.record_blocked("example.com", false, 50_000, 1000);
        stats.record_blocked("example.com", true, 2_000, 1005);
        stats.record_blocked("tracker.net", true, 1_500, 1010);

        assert_eq!(stats.total_ads_blocked(), 1);
        assert_eq!(stats.total_trackers_blocked(), 2);
        assert_eq!(stats.total_bandwidth_saved_bytes(), 53_500);

        let ex = stats.stats_for_domain("EXAMPLE.COM").unwrap();
        assert_eq!(ex.ads_blocked, 1);
        assert_eq!(ex.trackers_blocked, 1);
        assert_eq!(ex.bandwidth_saved_bytes, 52_000);
    }
}
