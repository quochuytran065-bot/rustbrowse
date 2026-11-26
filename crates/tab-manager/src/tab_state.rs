//! Tab state storage and indices.

use common::types::{TabId, TabLifecycleState, TabMetadata};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct TabStateStore {
    tabs: HashMap<TabId, TabMetadata>,
    active_tab_id: Option<TabId>,
    tab_order: Vec<TabId>,
}

impl TabStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, metadata: TabMetadata) {
        let id = metadata.id;
        self.tabs.insert(id, metadata);
        if !self.tab_order.contains(&id) {
            self.tab_order.push(id);
        }
        if self.active_tab_id.is_none() {
            self.active_tab_id = Some(id);
        }
    }

    pub fn get(&self, id: TabId) -> Option<&TabMetadata> {
        self.tabs.get(&id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut TabMetadata> {
        self.tabs.get_mut(&id)
    }

    pub fn remove(&mut self, id: TabId) -> Option<TabMetadata> {
        let index = self.tab_order.iter().position(|&x| x == id);
        if let Some(idx) = index {
            self.tab_order.remove(idx);
            if self.active_tab_id == Some(id) {
                // Select adjacent tab: either at the same index, or the previous one
                if !self.tab_order.is_empty() {
                    let new_idx = if idx < self.tab_order.len() {
                        idx
                    } else {
                        self.tab_order.len() - 1
                    };
                    self.active_tab_id = Some(self.tab_order[new_idx]);
                } else {
                    self.active_tab_id = None;
                }
            }
        }
        self.tabs.remove(&id)
    }

    pub fn set_active(&mut self, id: TabId) {
        if self.tabs.contains_key(&id) {
            // Update previous active tab state to Warm if it was Active
            if let Some(prev_id) = self.active_tab_id {
                if prev_id != id {
                    if let Some(prev_tab) = self.tabs.get_mut(&prev_id) {
                        if prev_tab.state == TabLifecycleState::Active {
                            prev_tab.state = TabLifecycleState::Warm;
                        }
                    }
                }
            }

            if let Some(curr_tab) = self.tabs.get_mut(&id) {
                curr_tab.state = TabLifecycleState::Active;
            }
            self.active_tab_id = Some(id);
        }
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active_tab_id
    }

    pub fn tab_order(&self) -> &[TabId] {
        &self.tab_order
    }

    pub fn tab_at_index(&self, index: usize) -> Option<TabId> {
        self.tab_order.get(index).copied()
    }

    pub fn next_tab_id(&self, current: TabId) -> Option<TabId> {
        let idx = self.tab_order.iter().position(|&x| x == current)?;
        if idx + 1 < self.tab_order.len() {
            Some(self.tab_order[idx + 1])
        } else {
            self.tab_order.first().copied() // Wrap around
        }
    }

    pub fn prev_tab_id(&self, current: TabId) -> Option<TabId> {
        let idx = self.tab_order.iter().position(|&x| x == current)?;
        if idx > 0 {
            Some(self.tab_order[idx - 1])
        } else {
            self.tab_order.last().copied() // Wrap around
        }
    }

    pub fn set_title(&mut self, id: TabId, title: String) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.title = title;
        }
    }

    pub fn set_url(&mut self, id: TabId, url: String) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.url = url;
        }
    }

    pub fn set_state(&mut self, id: TabId, state: TabLifecycleState) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.state = state;
        }
    }

    pub fn set_pinned(&mut self, id: TabId, pinned: bool) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.is_pinned = pinned;
        }
    }

    pub fn set_audible(&mut self, id: TabId, audible: bool) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.is_audible = audible;
        }
    }

    pub fn all_tabs(&self) -> impl Iterator<Item = &TabMetadata> {
        self.tabs.values()
    }

    pub fn all_metadata(&self) -> Vec<TabMetadata> {
        self.tab_order
            .iter()
            .filter_map(|id| self.tabs.get(id).cloned())
            .collect()
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Finds all background tabs that are eligible to be suspended (sleep).
    pub fn eligible_tabs_for_sleep(&self) -> Vec<TabId> {
        let active = self.active_tab_id;
        self.tabs
            .values()
            .filter(|tab| {
                Some(tab.id) != active
                    && tab.can_sleep_or_discard()
                    && tab.state == TabLifecycleState::Warm
            })
            .map(|tab| tab.id)
            .collect()
    }

    /// Finds all background tabs that are eligible to be discarded (free RAM).
    pub fn eligible_tabs_for_discard(&self) -> Vec<TabId> {
        let active = self.active_tab_id;
        self.tabs
            .values()
            .filter(|tab| {
                Some(tab.id) != active
                    && tab.can_sleep_or_discard()
                    && tab.state != TabLifecycleState::Discarded
            })
            .map(|tab| tab.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_store_lifecycle_and_navigation() {
        let mut store = TabStateStore::new();
        let t1 = TabMetadata::new(TabId::new(1), "https://example.com/1".to_string());
        let t2 = TabMetadata::new(TabId::new(2), "https://example.com/2".to_string());
        let t3 = TabMetadata::new(TabId::new(3), "https://example.com/3".to_string());

        store.insert(t1);
        store.insert(t2);
        store.insert(t3);

        assert_eq!(store.tab_count(), 3);
        assert_eq!(store.active_id(), Some(TabId::new(1)));

        // Navigation next / prev
        assert_eq!(store.next_tab_id(TabId::new(1)), Some(TabId::new(2)));
        assert_eq!(store.next_tab_id(TabId::new(3)), Some(TabId::new(1))); // wrap
        assert_eq!(store.prev_tab_id(TabId::new(1)), Some(TabId::new(3))); // wrap

        // Set active tab 2
        store.set_active(TabId::new(2));
        assert_eq!(store.active_id(), Some(TabId::new(2)));
        assert_eq!(
            store.get(TabId::new(1)).unwrap().state,
            TabLifecycleState::Warm
        );
        assert_eq!(
            store.get(TabId::new(2)).unwrap().state,
            TabLifecycleState::Active
        );

        // Remove active tab 2 -> should activate tab 3
        let removed = store.remove(TabId::new(2));
        assert!(removed.is_some());
        assert_eq!(store.active_id(), Some(TabId::new(3)));
        assert_eq!(store.tab_order(), &[TabId::new(1), TabId::new(3)]);
    }

    #[test]
    fn test_eligibility_for_sleep_and_discard() {
        let mut store = TabStateStore::new();
        let mut t1 = TabMetadata::new(TabId::new(1), "https://example.com/1".to_string());
        t1.state = TabLifecycleState::Active;
        let mut t2 = TabMetadata::new(TabId::new(2), "https://example.com/2".to_string());
        t2.state = TabLifecycleState::Warm;
        let mut t3 = TabMetadata::new(TabId::new(3), "https://example.com/3".to_string());
        t3.state = TabLifecycleState::Warm;
        t3.is_audible = true; // Playing sound -> protected

        store.insert(t1);
        store.insert(t2);
        store.insert(t3);
        store.set_active(TabId::new(1));

        let sleepable = store.eligible_tabs_for_sleep();
        assert_eq!(sleepable, vec![TabId::new(2)]);

        let discardable = store.eligible_tabs_for_discard();
        assert_eq!(discardable, vec![TabId::new(2)]);
    }
}
