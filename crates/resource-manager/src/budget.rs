//! Memory budget and tab count constraints based on RAM tiers and pressure levels.

use common::config::MemoryConfig;
use common::messages::TabPolicyAction;
use common::types::{MemoryPressureTier, TabId, TabMetadata};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryBudget {
    pub max_live_tabs: usize,
    pub pressure_tier: MemoryPressureTier,
}

impl MemoryBudget {
    /// Determines the maximum allowed concurrent live tabs (Active, Warm, Sleeping).
    pub fn calculate(
        total_phys_mb: u64,
        pressure: MemoryPressureTier,
        config: &MemoryConfig,
    ) -> Self {
        let is_4gb_or_less = total_phys_mb <= 4600;

        let max_live = match (is_4gb_or_less, pressure) {
            // 4 GB Primary Tier
            (true, MemoryPressureTier::Normal) => config.normal_max_live_tabs.max(1),
            (true, MemoryPressureTier::Warning) => config.warning_max_live_tabs.max(1),
            (true, MemoryPressureTier::Critical) => 1, // Only active tab alive

            // 8 GB+ Tier
            (false, MemoryPressureTier::Normal) => (config.normal_max_live_tabs * 2).max(6),
            (false, MemoryPressureTier::Warning) => config.warning_max_live_tabs.max(3),
            (false, MemoryPressureTier::Critical) => 2,
        };

        Self {
            max_live_tabs: max_live,
            pressure_tier: pressure,
        }
    }

    /// Evaluates all open tabs against the memory budget and pressure tier.
    /// Returns a list of pure `TabPolicyAction` directives.
    pub fn evaluate_budget_actions(
        &self,
        active_tab_id: Option<TabId>,
        tabs: &[TabMetadata],
    ) -> Vec<TabPolicyAction> {
        let mut actions = Vec::new();

        let live_tabs: Vec<&TabMetadata> = tabs.iter().filter(|t| t.state.is_alive()).collect();

        let excess_count = live_tabs.len().saturating_sub(self.max_live_tabs);

        // Candidates eligible for Discard (must not be active, must pass eligibility check)
        let mut discard_candidates: Vec<&TabMetadata> = live_tabs
            .iter()
            .copied()
            .filter(|t| Some(t.id) != active_tab_id && t.can_sleep_or_discard())
            .collect();

        // Sort candidates by LRU (oldest last_active first)
        discard_candidates.sort_by_key(|t| t.last_active_epoch_ms);

        let mut discarded_ids = std::collections::HashSet::new();

        if excess_count > 0 {
            for tab in discard_candidates.iter().take(excess_count) {
                actions.push(TabPolicyAction::SetDiscarded(tab.id));
                discarded_ids.insert(tab.id);
            }
        }

        // Under Critical pressure, discard ALL eligible background live tabs, not just excess
        if self.pressure_tier == MemoryPressureTier::Critical {
            for tab in &discard_candidates {
                if !discarded_ids.contains(&tab.id) {
                    actions.push(TabPolicyAction::SetDiscarded(tab.id));
                    discarded_ids.insert(tab.id);
                }
            }
        }

        // Under Warning pressure, force sleep on any surviving background live tab
        if self.pressure_tier == MemoryPressureTier::Warning {
            for tab in tabs {
                if Some(tab.id) != active_tab_id
                    && tab.state == common::types::TabLifecycleState::Warm
                    && tab.can_sleep_or_discard()
                    && !discarded_ids.contains(&tab.id)
                {
                    actions.push(TabPolicyAction::SetSleeping(tab.id));
                }
            }
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::types::TabLifecycleState;

    fn create_test_tab(id: u64, last_active: u64, state: TabLifecycleState) -> TabMetadata {
        TabMetadata {
            id: TabId::new(id),
            url: format!("https://site{}.com", id),
            title: format!("Tab {}", id),
            favicon_url: None,
            last_active_epoch_ms: last_active,
            is_pinned: false,
            is_audible: false,
            is_downloading: false,
            user_protected: false,
            state,
        }
    }

    #[test]
    fn test_budget_calculation() {
        let mut config = MemoryConfig::default();
        config.normal_max_live_tabs = 2;
        config.warning_max_live_tabs = 1;
        let budget_4gb = MemoryBudget::calculate(4000, MemoryPressureTier::Normal, &config);
        assert_eq!(budget_4gb.max_live_tabs, 2);

        let budget_4gb_warn = MemoryBudget::calculate(4000, MemoryPressureTier::Warning, &config);
        assert_eq!(budget_4gb_warn.max_live_tabs, 1);

        let budget_4gb_crit = MemoryBudget::calculate(4000, MemoryPressureTier::Critical, &config);
        assert_eq!(budget_4gb_crit.max_live_tabs, 1);
    }

    #[test]
    fn test_lru_excess_discard_actions() {
        let mut config = MemoryConfig::default();
        config.normal_max_live_tabs = 2;
        let budget = MemoryBudget::calculate(4000, MemoryPressureTier::Normal, &config); // max 2 live

        // 4 live tabs: tab 1 (active), tab 2 (oldest), tab 3 (newer), tab 4 (newest)
        let tabs = vec![
            create_test_tab(1, 1000, TabLifecycleState::Active),
            create_test_tab(2, 100, TabLifecycleState::Warm),
            create_test_tab(3, 500, TabLifecycleState::Warm),
            create_test_tab(4, 800, TabLifecycleState::Warm),
        ];

        let actions = budget.evaluate_budget_actions(Some(TabId::new(1)), &tabs);

        // Excess count is 4 - 2 = 2. Oldest are tab 2 (100) and tab 3 (500)
        assert_eq!(
            actions,
            vec![
                TabPolicyAction::SetDiscarded(TabId::new(2)),
                TabPolicyAction::SetDiscarded(TabId::new(3))
            ]
        );
    }

    #[test]
    fn test_critical_pressure_discards_all_eligible_background() {
        let config = MemoryConfig::default();
        let budget = MemoryBudget::calculate(4000, MemoryPressureTier::Critical, &config);

        let mut tab_pinned = create_test_tab(2, 200, TabLifecycleState::Warm);
        tab_pinned.is_pinned = true;

        let tabs = vec![
            create_test_tab(1, 1000, TabLifecycleState::Active),
            tab_pinned,
            create_test_tab(3, 300, TabLifecycleState::Warm),
            create_test_tab(4, 400, TabLifecycleState::Sleeping),
        ];

        let actions = budget.evaluate_budget_actions(Some(TabId::new(1)), &tabs);

        // Tab 1 is active (safe), Tab 2 is pinned (protected), Tabs 3 & 4 must be discarded
        assert_eq!(
            actions,
            vec![
                TabPolicyAction::SetDiscarded(TabId::new(3)),
                TabPolicyAction::SetDiscarded(TabId::new(4)),
            ]
        );
    }
}
