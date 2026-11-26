//! Engine trait abstraction decoupling WebView2 implementation from the browser shell and managers.

use crate::errors::Result;
use crate::types::{Rect, TabId};

pub trait Engine {
    /// Creates a new tab view within the parent window.
    fn create_tab(&mut self, id: TabId, url: &str, bounds: Rect, is_visible: bool) -> Result<()>;

    /// Navigates an existing tab to a new URL.
    fn navigate(&mut self, id: TabId, url: &str) -> Result<()>;

    /// Sets the bounds for a tab viewport.
    fn set_tab_bounds(&mut self, id: TabId, bounds: Rect) -> Result<()>;

    /// Sets visibility of a tab controller.
    fn set_tab_visible(&mut self, id: TabId, visible: bool) -> Result<()>;

    /// Hints memory usage target level (true = Low, false = Normal).
    fn set_tab_memory_target(&mut self, id: TabId, low: bool) -> Result<()>;

    /// Attempts to suspend the tab (best-effort).
    fn suspend_tab(&mut self, id: TabId) -> Result<()>;

    /// Resumes a suspended tab.
    fn resume_tab(&mut self, id: TabId) -> Result<()>;

    /// Destroys the tab's underlying WebView2 controller.
    fn close_tab(&mut self, id: TabId) -> Result<()>;

    /// Navigates backwards in tab history.
    fn go_back(&mut self, id: TabId) -> Result<()>;

    /// Navigates forwards in tab history.
    fn go_forward(&mut self, id: TabId) -> Result<()>;

    /// Reloads the tab.
    fn reload(&mut self, id: TabId) -> Result<()>;

    /// Stops any ongoing loading in the tab.
    fn stop(&mut self, id: TabId) -> Result<()>;
}
