//! Internal messaging channels between UI thread, managers, and engine.

use crate::types::{Rect, TabId, TabLifecycleState};

/// Commands sent to the UI thread (STA message loop owner).
#[derive(Debug, Clone)]
pub enum UiCommand {
    /// Create and show a new tab with an initial URL.
    CreateTab { url: String, in_background: bool },
    /// Activate and bring the specified tab to foreground.
    ActivateTab(TabId),
    /// Close the specified tab.
    CloseTab(TabId),
    /// Navigate the specified tab to a new URL.
    Navigate { id: TabId, url: String },
    /// Go back in tab history.
    GoBack(TabId),
    /// Go forward in tab history.
    GoForward(TabId),
    /// Reload current tab.
    Reload(TabId),
    /// Transition tab state to Sleeping (low memory hint + suspend).
    SleepTab(TabId),
    /// Transition tab state to Discarded (destroy controller).
    DiscardTab(TabId),
    /// Quick action: Sleep all background tabs immediately.
    SleepAllBackgroundTabs,
    /// Quick action: Discard all eligible background tabs to free maximum RAM.
    FreeEligibleTabsRam,
    /// Resize the active tab viewport to bounds.
    ResizeActiveTab(Rect),
    /// Request trim working set for the host process.
    TrimWorkingSet,
}

/// Events emitted by the WebView2 engine / native shell towards managers.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// A new tab was created with its assigned controller.
    TabCreated { id: TabId, url: String },
    /// Tab navigation started.
    NavigationStarting { id: TabId, url: String },
    /// Tab navigation completed.
    NavigationCompleted {
        id: TabId,
        url: String,
        is_success: bool,
    },
    /// Document title changed.
    TitleChanged { id: TabId, title: String },
    /// Favicon URI updated.
    FaviconChanged { id: TabId, favicon_url: String },
    /// Audio state changed (playing audio prevents sleep).
    AudioStateChanged { id: TabId, is_audible: bool },
    /// Renderer or browser process failed/crashed.
    ProcessFailed {
        id: TabId,
        exit_code: i32,
        description: String,
    },
    /// Tab controller was destroyed / discarded.
    TabDiscarded { id: TabId },
    /// Tab lifecycle state transition completed.
    LifecycleStateChanged {
        id: TabId,
        new_state: TabLifecycleState,
    },
    /// A popup was blocked by Popup Guard.
    BlockedPopup { id: TabId, url: String },
    /// Page requested a new window; if initiated, host can open as tab.
    NewWindowRequested {
        id: TabId,
        target_url: String,
        is_user_initiated: bool,
    },
    /// Element Zapper created a new rule.
    ZapperRuleCreated { domain: String, selector: String },
    /// Auto clean RAM completed with freed MB.
    AutoCleanTriggered { freed_mb: u64 },
}

/// Pure policy actions emitted by tab_policy function without side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabPolicyAction {
    DoNothing,
    SetWarm(TabId),
    SetSleeping(TabId),
    SetDiscarded(TabId),
}
