//! Adaptive resource monitoring scheduler with dynamic frequency throttling.

use common::config::{MemoryConfig, MonitorConfig};
use common::messages::TabPolicyAction;
use common::types::{MemoryPressureTier, SystemMemorySnapshot, TabId, TabMetadata};
use std::time::{Duration, Instant};

use crate::budget::MemoryBudget;
use crate::cpu_monitor::CpuMonitor;
use crate::memory_monitor::MemoryMonitor;
use crate::pressure::evaluate_memory_pressure;

#[derive(Debug, Clone)]
pub struct ResourceTickReport {
    pub snapshot: SystemMemorySnapshot,
    pub host_cpu_percent: f64,
    pub pressure_tier: MemoryPressureTier,
    pub pressure_changed: bool,
    pub budget: MemoryBudget,
    pub suggested_actions: Vec<TabPolicyAction>,
}

#[derive(Debug)]
pub struct ResourceScheduler {
    monitor_config: MonitorConfig,
    memory_config: MemoryConfig,
    is_minimized: bool,
    last_tick: Instant,
    current_pressure: MemoryPressureTier,
}

impl ResourceScheduler {
    pub fn new(monitor_config: MonitorConfig, memory_config: MemoryConfig) -> Self {
        Self {
            monitor_config,
            memory_config,
            is_minimized: false,
            // Initialize in past so first tick fires immediately
            last_tick: Instant::now() - Duration::from_secs(60),
            current_pressure: MemoryPressureTier::Normal,
        }
    }

    pub fn set_minimized(&mut self, minimized: bool) {
        if self.is_minimized != minimized {
            self.is_minimized = minimized;
            tracing::debug!(
                minimized = self.is_minimized,
                interval_secs = self.current_interval().as_secs(),
                "Resource scheduler interval adapted"
            );
        }
    }

    pub fn is_minimized(&self) -> bool {
        self.is_minimized
    }

    pub fn current_interval(&self) -> Duration {
        if self.is_minimized {
            Duration::from_secs(self.monitor_config.idle_interval_secs)
        } else {
            Duration::from_secs(self.monitor_config.visible_interval_secs)
        }
    }

    pub fn should_tick(&self, now: Instant) -> bool {
        now.duration_since(self.last_tick) >= self.current_interval()
    }

    pub fn force_tick(&mut self) {
        self.last_tick = Instant::now() - Duration::from_secs(60);
    }

    /// Executes an adaptive resource evaluation tick if the scheduled interval has elapsed.
    pub fn tick(
        &mut self,
        now: Instant,
        cpu_monitor: &mut CpuMonitor,
        active_tab_id: Option<TabId>,
        tabs: &[TabMetadata],
    ) -> Option<ResourceTickReport> {
        if !self.should_tick(now) {
            return None;
        }

        self.last_tick = now;

        let snapshot = MemoryMonitor::sample_system_memory()?;
        let host_cpu = cpu_monitor.sample_host_cpu_percent();

        let new_pressure = evaluate_memory_pressure(
            snapshot.avail_phys_mb,
            snapshot.memory_load_percent,
            &self.memory_config,
        );

        let pressure_changed = new_pressure != self.current_pressure;
        self.current_pressure = new_pressure;

        let budget =
            MemoryBudget::calculate(snapshot.total_phys_mb, new_pressure, &self.memory_config);

        let actions = budget.evaluate_budget_actions(active_tab_id, tabs);

        Some(ResourceTickReport {
            snapshot,
            host_cpu_percent: host_cpu,
            pressure_tier: new_pressure,
            pressure_changed,
            budget,
            suggested_actions: actions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_interval_adaptation() {
        let monitor_cfg = MonitorConfig {
            visible_interval_secs: 5,
            idle_interval_secs: 20,
        };
        let memory_cfg = MemoryConfig::default();

        let mut scheduler = ResourceScheduler::new(monitor_cfg, memory_cfg);
        assert_eq!(scheduler.current_interval(), Duration::from_secs(5));

        scheduler.set_minimized(true);
        assert_eq!(scheduler.current_interval(), Duration::from_secs(20));

        scheduler.set_minimized(false);
        assert_eq!(scheduler.current_interval(), Duration::from_secs(5));
    }
}
