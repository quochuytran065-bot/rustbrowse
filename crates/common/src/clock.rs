//! Clock abstraction to enable 100% deterministic testing of memory & tab policies.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    /// Returns current timestamp in epoch milliseconds.
    fn now_ms(&self) -> u64;
}

/// Real system clock.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Controllable mock clock for golden tests.
#[derive(Debug, Default)]
pub struct MockClock {
    current_ms: AtomicU64,
}

impl MockClock {
    pub fn new(initial_ms: u64) -> Self {
        Self {
            current_ms: AtomicU64::new(initial_ms),
        }
    }

    pub fn set_ms(&self, ms: u64) {
        self.current_ms.store(ms, Ordering::SeqCst);
    }

    pub fn advance_secs(&self, secs: u64) {
        self.current_ms.fetch_add(secs * 1000, Ordering::SeqCst);
    }
}

impl Clock for MockClock {
    fn now_ms(&self) -> u64 {
        self.current_ms.load(Ordering::SeqCst)
    }
}
