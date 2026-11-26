//! Runtime browser performance and health metrics.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct BrowserMetrics {
    page_loads_count: AtomicU64,
    tab_switch_count: AtomicU64,
    total_tab_switch_latency_ms: AtomicU64,
    peak_private_working_set_mb: AtomicU64,
    adblock_blocked_count: AtomicU64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub page_loads: u64,
    pub tab_switches: u64,
    pub avg_tab_switch_latency_ms: f64,
    pub peak_pws_mb: u64,
    pub adblock_blocked: u64,
}

impl BrowserMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_page_load(&self) {
        self.page_loads_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tab_switch(&self, latency_ms: u64) {
        self.tab_switch_count.fetch_add(1, Ordering::Relaxed);
        self.total_tab_switch_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
    }

    pub fn record_memory_sample(&self, pws_mb: u64) {
        self.peak_private_working_set_mb
            .fetch_max(pws_mb, Ordering::Relaxed);
    }

    pub fn record_adblock_blocked(&self) {
        self.adblock_blocked_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let switches = self.tab_switch_count.load(Ordering::Relaxed);
        let total_lat = self.total_tab_switch_latency_ms.load(Ordering::Relaxed);
        let avg_lat = if switches > 0 {
            total_lat as f64 / switches as f64
        } else {
            0.0
        };

        MetricsSnapshot {
            page_loads: self.page_loads_count.load(Ordering::Relaxed),
            tab_switches: switches,
            avg_tab_switch_latency_ms: avg_lat,
            peak_pws_mb: self.peak_private_working_set_mb.load(Ordering::Relaxed),
            adblock_blocked: self.adblock_blocked_count.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_recording() {
        let metrics = BrowserMetrics::new();
        metrics.record_page_load();
        metrics.record_tab_switch(40);
        metrics.record_tab_switch(60);
        metrics.record_memory_sample(120);
        metrics.record_memory_sample(150);
        metrics.record_adblock_blocked();

        let snap = metrics.snapshot();
        assert_eq!(snap.page_loads, 1);
        assert_eq!(snap.tab_switches, 2);
        assert_eq!(snap.avg_tab_switch_latency_ms, 50.0);
        assert_eq!(snap.peak_pws_mb, 150);
        assert_eq!(snap.adblock_blocked, 1);
    }
}
