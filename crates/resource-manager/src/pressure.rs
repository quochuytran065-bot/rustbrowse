//! Pure memory pressure evaluation function.

use common::config::MemoryConfig;
use common::types::MemoryPressureTier;

/// Pure evaluation function mapping available RAM and load percentage to a pressure tier.
pub fn evaluate_memory_pressure(
    avail_phys_mb: u64,
    memory_load_percent: u32,
    config: &MemoryConfig,
) -> MemoryPressureTier {
    if avail_phys_mb < config.critical_available_ram_mb
        || memory_load_percent >= config.critical_load_percent
    {
        MemoryPressureTier::Critical
    } else if avail_phys_mb < config.warning_available_ram_mb {
        MemoryPressureTier::Warning
    } else {
        MemoryPressureTier::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pressure_tiers() {
        let config = MemoryConfig::default(); // warning: 1024, critical: 600, load: 85

        // Normal: 1500 MB free, 50% load
        assert_eq!(
            evaluate_memory_pressure(1500, 50, &config),
            MemoryPressureTier::Normal
        );

        // Warning: 800 MB free, 70% load
        assert_eq!(
            evaluate_memory_pressure(800, 70, &config),
            MemoryPressureTier::Warning
        );

        // Critical: 500 MB free, 70% load
        assert_eq!(
            evaluate_memory_pressure(500, 70, &config),
            MemoryPressureTier::Critical
        );

        // Critical by load: 1200 MB free, 90% load
        assert_eq!(
            evaluate_memory_pressure(1200, 90, &config),
            MemoryPressureTier::Critical
        );
    }
}
