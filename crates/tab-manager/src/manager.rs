//! High-level TabManager coordinating tab states, memory policies, and transitions.

use crate::tab_policy::evaluate_tab_transition;
use crate::tab_state::TabStateStore;
use common::config::MemoryConfig;
use common::messages::TabPolicyAction;
use common::types::{MemoryPressureTier, TabId, TabLifecycleState, TabMetadata};

pub struct TabManager {
    store: TabStateStore,
    next_tab_id: u64,
    memory_config: MemoryConfig,
}

impl TabManager {
    pub fn new(memory_config: MemoryConfig) -> Self {
        Self {
            store: TabStateStore::new(),
            next_tab_id: 1,
            memory_config,
        }
    }

    /// Allocates and registers a new tab metadata.
    pub fn allocate_new_tab(&mut self, url: String) -> TabMetadata {
        let id = TabId::new(self.next_tab_id);
        self.next_tab_id += 1;
        let meta = TabMetadata::new(id, url);
        self.store.insert(meta.clone());
        meta
    }

    pub fn store(&self) -> &TabStateStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut TabStateStore {
        &mut self.store
    }

    pub fn all_tab_metadata(&self) -> Vec<TabMetadata> {
        self.store.all_metadata()
    }

    /// Evaluates all background tabs and returns the actions to be executed on the engine.
    pub fn evaluate_pressure_policy(
        &self,
        current_time_ms: u64,
        pressure: MemoryPressureTier,
    ) -> Vec<TabPolicyAction> {
        let mut actions = Vec::new();
        for tab in self.store.all_tabs() {
            let action =
                evaluate_tab_transition(tab, current_time_ms, pressure, &self.memory_config);
            if action != TabPolicyAction::DoNothing {
                actions.push(action);
            }
        }
        actions
    }

    /// Handles tab closure, returning the next tab to activate if any.
    pub fn close_tab(&mut self, id: TabId) -> (Option<TabMetadata>, Option<TabId>) {
        let removed = self.store.remove(id);
        let next_active = self.store.active_id();
        (removed, next_active)
    }

    /// Handles tab activation, transitioning the new tab to Active and previous to Warm.
    pub fn activate_tab(&mut self, id: TabId) -> Option<TabLifecycleState> {
        let previous_state = self.store.get(id).map(|t| t.state);
        self.store.set_active(id);
        previous_state
    }

    /// Enforces the strict live tab quota (max 1 Active + max 1 Sleeping/Warm).
    /// All other background tabs are automatically scheduled for Discard.
    /// Protected tabs (playing audio, active downloads, pinned) are never discarded.
    pub fn enforce_live_tab_quota(&mut self, pressure: MemoryPressureTier) -> Vec<TabPolicyAction> {
        let max_live = match pressure {
            MemoryPressureTier::Normal => self.memory_config.normal_max_live_tabs,
            MemoryPressureTier::Warning | MemoryPressureTier::Critical => {
                self.memory_config.warning_max_live_tabs
            }
        };

        let active_id = self.store.active_id();
        let mut actions = Vec::new();

        // Collect all non-discarded background tabs that can be discarded
        let mut candidates: Vec<TabMetadata> = self
            .store
            .all_tabs()
            .filter(|t| {
                Some(t.id) != active_id
                    && t.can_sleep_or_discard()
                    && t.state != TabLifecycleState::Discarded
            })
            .cloned()
            .collect();

        // Sort by last_active_epoch_ms ascending (oldest background tabs first)
        candidates.sort_by_key(|t| t.last_active_epoch_ms);

        let total_live = 1 + candidates.len();

        if total_live > max_live {
            let to_discard_count = total_live - max_live;
            for tab in candidates.iter().take(to_discard_count) {
                self.store.set_state(tab.id, TabLifecycleState::Discarded);
                actions.push(TabPolicyAction::SetDiscarded(tab.id));
            }
        }

        // For remaining live background tab(s) that fit in quota, transition Warm -> Sleeping
        let skip_count = if total_live > max_live {
            total_live - max_live
        } else {
            0
        };
        for tab in candidates.iter().skip(skip_count) {
            if tab.state == TabLifecycleState::Warm {
                self.store.set_state(tab.id, TabLifecycleState::Sleeping);
                actions.push(TabPolicyAction::SetSleeping(tab.id));
            }
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_manager_allocation_and_activation() {
        let config = MemoryConfig::default();
        let mut manager = TabManager::new(config);

        let t1 = manager.allocate_new_tab("https://site1.com".to_string());
        let t2 = manager.allocate_new_tab("https://site2.com".to_string());

        assert_eq!(t1.id, TabId::new(1));
        assert_eq!(t2.id, TabId::new(2));
        assert_eq!(manager.store().tab_count(), 2);
        assert_eq!(manager.store().active_id(), Some(TabId::new(1)));

        let prev = manager.activate_tab(TabId::new(2));
        assert_eq!(prev, Some(TabLifecycleState::Active)); // t2 initial was Active
        assert_eq!(manager.store().active_id(), Some(TabId::new(2)));
    }

    #[test]
    fn test_enforce_live_tab_quota_ten_tabs() {
        let mut config = MemoryConfig::default();
        config.normal_max_live_tabs = 2;
        let mut manager = TabManager::new(config);

        // Open 10 tabs
        for i in 1..=10 {
            let tab = manager.allocate_new_tab(format!("https://site{}.com", i));
            manager.activate_tab(tab.id);
            // Simulate progression of time so earlier tabs have smaller last_active_epoch_ms
            if let Some(t) = manager.store_mut().get_mut(tab.id) {
                t.last_active_epoch_ms = i * 1000;
            }
        }

        assert_eq!(manager.store().tab_count(), 10);
        assert_eq!(manager.store().active_id(), Some(TabId::new(10)));

        let actions = manager.enforce_live_tab_quota(MemoryPressureTier::Normal);

        // Under 2 live tabs max:
        // Tab 10 is active (1)
        // Tab 9 is sleeping (1)
        // Tabs 1..=8 are discarded (8 actions)
        let discarded_count = actions
            .iter()
            .filter(|a| matches!(a, TabPolicyAction::SetDiscarded(_)))
            .count();
        let sleeping_count = actions
            .iter()
            .filter(|a| matches!(a, TabPolicyAction::SetSleeping(_)))
            .count();

        assert_eq!(discarded_count, 8);
        assert_eq!(sleeping_count, 1);

        // Tab 10 is still Active
        assert_eq!(
            manager.store().get(TabId::new(10)).unwrap().state,
            TabLifecycleState::Active
        );
        // Tab 9 is Sleeping
        assert_eq!(
            manager.store().get(TabId::new(9)).unwrap().state,
            TabLifecycleState::Sleeping
        );
        // Tab 1 is Discarded
        assert_eq!(
            manager.store().get(TabId::new(1)).unwrap().state,
            TabLifecycleState::Discarded
        );
    }
}
