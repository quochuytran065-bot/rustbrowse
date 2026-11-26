//! Common types across the RustBrowse workspace.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Strongly typed unique identifier for a browser tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub u64);

impl TabId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tab-{}", self.0)
    }
}

/// The 4-state lifecycle of a browser tab for strict RAM optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabLifecycleState {
    /// Actively visible on screen. Controller bound, MemoryUsageTargetLevel::Normal.
    Active,
    /// Hidden in background (`IsVisible = false`). Controller preserved for instant switch.
    Warm,
    /// Hidden, MemoryUsageTargetLevel::Low, and TrySuspend invoked (best-effort).
    Sleeping,
    /// Controller destroyed to free full WebView2 renderer RAM.
    /// URL and lightweight state preserved on client; rebuilt lazily on focus.
    Discarded,
}

impl TabLifecycleState {
    pub fn is_alive(&self) -> bool {
        matches!(self, Self::Active | Self::Warm | Self::Sleeping)
    }
}

/// Lightweight snapshot of a tab used when discarded or serialized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabMetadata {
    pub id: TabId,
    pub url: String,
    pub title: String,
    pub favicon_url: Option<String>,
    pub last_active_epoch_ms: u64,
    pub is_pinned: bool,
    pub is_audible: bool,
    pub is_downloading: bool,
    pub user_protected: bool,
    pub state: TabLifecycleState,
}

impl TabMetadata {
    pub fn new(id: TabId, url: String) -> Self {
        Self {
            id,
            url,
            title: "New Tab".to_string(),
            favicon_url: None,
            last_active_epoch_ms: 0,
            is_pinned: false,
            is_audible: false,
            is_downloading: false,
            user_protected: false,
            state: TabLifecycleState::Active,
        }
    }

    /// Policy gate: checks if tab is eligible for Sleep or Discard.
    pub fn can_sleep_or_discard(&self) -> bool {
        !self.is_pinned && !self.is_audible && !self.is_downloading && !self.user_protected
    }
}

/// Bounding rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// DPI Scale factor for HiDPI displays.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DpiScale(pub f64);

impl Default for DpiScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// System memory pressure tier based on available RAM and usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryPressureTier {
    /// Available RAM > ~1 GB. Default policy.
    Normal,
    /// Available RAM < ~1 GB. Sleep older background tabs.
    Warning,
    /// Available RAM < ~600 MB or Load > 85%. Discard eligible tabs aggressively.
    Critical,
}

/// Global system memory status snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemMemorySnapshot {
    pub total_phys_mb: u64,
    pub avail_phys_mb: u64,
    pub memory_load_percent: u32,
    pub tree_private_working_set_mb: u64,
}
