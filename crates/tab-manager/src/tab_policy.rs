//! Pure tab evaluation policy for RAM optimization.

use common::config::MemoryConfig;
use common::messages::TabPolicyAction;
use common::types::{MemoryPressureTier, TabLifecycleState, TabMetadata};

/// Pure evaluation function: computes required lifecycle transitions for a tab based on inactivity and memory pressure.
pub fn evaluate_tab_transition(
    tab: &TabMetadata,
    current_time_ms: u64,
    pressure: MemoryPressureTier,
    config: &MemoryConfig,
) -> TabPolicyAction {
    // If tab is Active, do not transition away via background idle policy
    if tab.state == TabLifecycleState::Active {
        return TabPolicyAction::DoNothing;
    }

    // Protected tabs (audio, downloading, pinned, user-exempt) never sleep or discard
    if !tab.can_sleep_or_discard() {
        return TabPolicyAction::DoNothing;
    }

    let inactive_secs = if current_time_ms >= tab.last_active_epoch_ms {
        (current_time_ms - tab.last_active_epoch_ms) / 1000
    } else {
        0
    };

    // Pressure multiplier accelerates transitions
    let (_warm_limit, sleep_limit, discard_limit) = match pressure {
        MemoryPressureTier::Normal => (
            config.warm_threshold_secs,
            config.sleep_threshold_secs,
            config.discard_threshold_secs,
        ),
        MemoryPressureTier::Warning => (
            config.warm_threshold_secs / 2,
            config.sleep_threshold_secs / 2,
            config.discard_threshold_secs / 2,
        ),
        MemoryPressureTier::Critical => (5, 15, 60),
    };

    match tab.state {
        TabLifecycleState::Active => TabPolicyAction::DoNothing,
        TabLifecycleState::Warm => {
            if inactive_secs >= discard_limit {
                TabPolicyAction::SetDiscarded(tab.id)
            } else if inactive_secs >= sleep_limit {
                TabPolicyAction::SetSleeping(tab.id)
            } else {
                TabPolicyAction::DoNothing
            }
        }
        TabLifecycleState::Sleeping => {
            if inactive_secs >= discard_limit {
                TabPolicyAction::SetDiscarded(tab.id)
            } else {
                TabPolicyAction::DoNothing
            }
        }
        TabLifecycleState::Discarded => TabPolicyAction::DoNothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::clock::{Clock, MockClock};
    use common::types::TabId;

    #[test]
    fn test_active_and_protected_tabs_never_transition() {
        let clock = MockClock::new(10_000_000);
        let config = MemoryConfig::default();

        let mut tab = TabMetadata::new(TabId::new(1), "https://example.com".to_string());
        tab.last_active_epoch_ms = 1_000;
        tab.state = TabLifecycleState::Active;

        let action =
            evaluate_tab_transition(&tab, clock.now_ms(), MemoryPressureTier::Critical, &config);
        assert_eq!(action, TabPolicyAction::DoNothing);

        // Protected with audio
        tab.state = TabLifecycleState::Warm;
        tab.is_audible = true;
        let action =
            evaluate_tab_transition(&tab, clock.now_ms(), MemoryPressureTier::Critical, &config);
        assert_eq!(action, TabPolicyAction::DoNothing);
    }

    #[test]
    fn test_warm_to_sleeping_transition() {
        let clock = MockClock::new(200_000);
        let config = MemoryConfig::default(); // sleep threshold: 180s

        let mut tab = TabMetadata::new(TabId::new(2), "https://example.com".to_string());
        tab.state = TabLifecycleState::Warm;
        tab.last_active_epoch_ms = 5_000; // 195s inactive

        let action =
            evaluate_tab_transition(&tab, clock.now_ms(), MemoryPressureTier::Normal, &config);
        assert_eq!(action, TabPolicyAction::SetSleeping(TabId::new(2)));
    }

    #[test]
    fn test_critical_pressure_accelerates_discard() {
        let clock = MockClock::new(70_000);
        let config = MemoryConfig::default();

        let mut tab = TabMetadata::new(TabId::new(3), "https://example.com".to_string());
        tab.state = TabLifecycleState::Sleeping;
        tab.last_active_epoch_ms = 5_000; // 65s inactive

        let action =
            evaluate_tab_transition(&tab, clock.now_ms(), MemoryPressureTier::Critical, &config);
        assert_eq!(action, TabPolicyAction::SetDiscarded(TabId::new(3)));
    }
}
