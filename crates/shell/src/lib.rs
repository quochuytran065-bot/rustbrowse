//! Native Win32 desktop shell for RustBrowse.

pub mod address_bar;
pub mod context_menu;
pub mod dpi;
pub mod shortcuts;
pub mod tab_strip;
pub mod toolbar;
pub mod window;

pub use address_bar::AddressBar;
pub use context_menu::{TabContextMenu, TabContextMenuAction};
pub use dpi::DpiManager;
pub use shortcuts::{AppShortcut, ShortcutRouter};
pub use tab_strip::{TabStrip, TabStripHit, TabVisualItem};
pub use toolbar::{BrowserToolbar, ToolbarAction};
pub use window::{BrowserWindow, ShellUiEvent};
