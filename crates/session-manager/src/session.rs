//! Session state representation and serialization.

use common::types::{TabId, TabMetadata};
use serde::{Deserialize, Serialize};
use storage::SessionTabEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTab {
    pub tab_id: u64,
    pub url: String,
    pub title: String,
    pub is_pinned: bool,
    pub tab_order: i32,
    pub is_active: bool,
}

impl SessionTab {
    pub fn from_metadata(meta: &TabMetadata, order: i32, is_active: bool) -> Self {
        Self {
            tab_id: meta.id.0,
            url: meta.url.clone(),
            title: meta.title.clone(),
            is_pinned: meta.is_pinned,
            tab_order: order,
            is_active,
        }
    }

    pub fn to_storage_entry(&self) -> SessionTabEntry {
        SessionTabEntry {
            tab_id: self.tab_id,
            url: self.url.clone(),
            title: self.title.clone(),
            is_pinned: self.is_pinned,
            tab_order: self.tab_order,
            is_active: self.is_active,
        }
    }

    pub fn from_storage_entry(entry: SessionTabEntry) -> Self {
        Self {
            tab_id: entry.tab_id,
            url: entry.url,
            title: entry.title,
            is_pinned: entry.is_pinned,
            tab_order: entry.tab_order,
            is_active: entry.is_active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub tabs: Vec<SessionTab>,
    pub active_tab_id: Option<TabId>,
    pub clean_shutdown: bool,
}

impl SessionSnapshot {
    pub fn new(tabs: Vec<SessionTab>, active_tab_id: Option<TabId>, clean_shutdown: bool) -> Self {
        Self {
            tabs,
            active_tab_id,
            clean_shutdown,
        }
    }
}
