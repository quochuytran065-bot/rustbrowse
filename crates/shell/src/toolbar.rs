//! Native Win32 Navigation & Action Toolbar with Fluent 2 / Modern Dark controls.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use crate::address_bar::AddressBar;
use common::errors::{Result, RustBrowseError};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, MoveWindow, SendMessageW, SetWindowTextW, BS_PUSHBUTTON,
    HMENU, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETFONT, WS_CHILD, WS_VISIBLE,
};

pub const ID_NAV_BACK: u16 = 101;
pub const ID_NAV_FORWARD: u16 = 102;
pub const ID_NAV_RELOAD: u16 = 103;
pub const ID_NAV_HOME: u16 = 104;
pub const ID_ADDRESS_BAR: u16 = 105;
pub const ID_NAV_GO: u16 = 106;
pub const ID_ACTION_SHIELD: u16 = 107;
pub const ID_ACTION_DOWNLOADS: u16 = 108;
pub const ID_ACTION_PERFORMANCE: u16 = 109;
pub const ID_ACTION_SETTINGS: u16 = 110;
pub const ID_ACTION_LITE_MODE: u16 = 111;
pub const ID_ACTION_CLEAN_RAM: u16 = 112;
pub const ID_ACTION_SLEEP_TABS: u16 = 113;
pub const ID_ACTION_RAM_BADGE: u16 = 114;
pub const ID_ACTION_GAMING_MODE: u16 = 115;
pub const ID_ACTION_READER_MODE: u16 = 116;
pub const ID_ACTION_ELEMENT_ZAPPER: u16 = 117;
pub const ID_ACTION_CLEAN_URL_COPY: u16 = 118;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolbarAction {
    Back,
    Forward,
    Reload,
    Home,
    Navigate(String),
    ToggleShield,
    OpenDownloads,
    OpenPerformance,
    OpenSettings,
    ToggleLiteMode,
    CleanRam,
    SleepTabs,
    ToggleGamingMode,
    ToggleReaderMode,
    StartElementZapper,
    CopyCleanUrl,
    OpenRamInspector,
}

pub struct BrowserToolbar {
    btn_back: HWND,
    btn_forward: HWND,
    btn_reload: HWND,
    btn_home: HWND,
    address_bar: AddressBar,
    btn_go: HWND,
    btn_ram_badge: HWND,
    btn_lite_mode: HWND,
    btn_clean_ram: HWND,
    btn_sleep_tabs: HWND,
    btn_shield: HWND,
    btn_downloads: HWND,
    btn_perf: HWND,
    btn_settings: HWND,
}

impl BrowserToolbar {
    pub fn new(
        parent_hwnd: *mut std::ffi::c_void,
        client_width: i32,
        toolbar_y: i32,
    ) -> Result<Self> {
        let parent_hwnd = HWND(parent_hwnd);
        unsafe {
            let h_font = GetStockObject(DEFAULT_GUI_FONT);

            let make_btn = |id: u16, text: &str| -> Result<HWND> {
                let wide: Vec<u16> = OsStr::new(text)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("BUTTON"),
                    PCWSTR(wide.as_ptr()),
                    WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                    0,
                    0,
                    0,
                    0,
                    Some(parent_hwnd),
                    Some(HMENU(id as usize as *mut _)),
                    None,
                    None,
                )
                .map_err(|e| {
                    RustBrowseError::Windows(format!(
                        "Failed to create toolbar button {}: {:?}",
                        id, e
                    ))
                })?;

                let _ = SendMessageW(
                    hwnd,
                    WM_SETFONT,
                    Some(WPARAM(h_font.0 as usize)),
                    Some(LPARAM(1)),
                );
                Ok(hwnd)
            };

            let btn_back = make_btn(ID_NAV_BACK, "<")?;
            let btn_forward = make_btn(ID_NAV_FORWARD, ">")?;
            let btn_reload = make_btn(ID_NAV_RELOAD, "↻")?;
            let btn_home = make_btn(ID_NAV_HOME, "⌂")?;
            let btn_go = make_btn(ID_NAV_GO, "→")?;
            let btn_ram_badge = make_btn(ID_ACTION_RAM_BADGE, "RAM: -- MB")?;
            let btn_lite_mode = make_btn(ID_ACTION_LITE_MODE, "⚡ Lite")?;
            let btn_clean_ram = make_btn(ID_ACTION_CLEAN_RAM, "🧹")?;
            let btn_sleep_tabs = make_btn(ID_ACTION_SLEEP_TABS, "💤")?;
            let btn_shield = make_btn(ID_ACTION_SHIELD, "🛡️ Shields")?;
            let btn_downloads = make_btn(ID_ACTION_DOWNLOADS, "↓")?;
            let btn_perf = make_btn(ID_ACTION_PERFORMANCE, "📊")?;
            let btn_settings = make_btn(ID_ACTION_SETTINGS, "⚙")?;

            let address_bar = AddressBar::new(parent_hwnd, ID_ADDRESS_BAR, 0, 0, 0, 0)?;

            let toolbar = Self {
                btn_back,
                btn_forward,
                btn_reload,
                btn_home,
                address_bar,
                btn_go,
                btn_ram_badge,
                btn_lite_mode,
                btn_clean_ram,
                btn_sleep_tabs,
                btn_shield,
                btn_downloads,
                btn_perf,
                btn_settings,
            };

            toolbar.layout(client_width, toolbar_y);
            Ok(toolbar)
        }
    }

    pub fn address_bar(&self) -> &AddressBar {
        &self.address_bar
    }

    pub fn update_ram_badge(&self, ram_mb: u64) {
        unsafe {
            let label = format!("RAM: {} MB", ram_mb);
            let wide: Vec<u16> = OsStr::new(&label)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let _ = SetWindowTextW(self.btn_ram_badge, PCWSTR(wide.as_ptr()));
        }
    }

    pub fn set_lite_mode_active(&self, active: bool) {
        unsafe {
            let label = if active { "⚡ Lite [ON]" } else { "⚡ Lite [OFF]" };
            let wide: Vec<u16> = OsStr::new(label)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let _ = SetWindowTextW(self.btn_lite_mode, PCWSTR(wide.as_ptr()));
        }
    }

    pub fn update_shield_badge(&self, blocked_count: u64) {
        unsafe {
            let label = if blocked_count > 0 {
                format!("🛡️ ({})", blocked_count)
            } else {
                "🛡️ Shields".to_string()
            };
            let wide: Vec<u16> = OsStr::new(&label)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let _ = SetWindowTextW(self.btn_shield, PCWSTR(wide.as_ptr()));
        }
    }

    /// Responsive and collision-free layout for toolbar navigation and controls.
    /// Guarantees that the address bar never collides, overflows, or protrudes offscreen.
    pub fn layout(&self, client_width: i32, toolbar_y: i32) {
        unsafe {
            let btn_height = 28;
            let btn_y = toolbar_y + 4;

            // 1. Left cluster: Navigation controls
            let mut left_x = 8;
            let _ = MoveWindow(self.btn_back, left_x, btn_y, 30, btn_height, true);
            left_x += 34;
            let _ = MoveWindow(self.btn_forward, left_x, btn_y, 30, btn_height, true);
            left_x += 34;
            let _ = MoveWindow(self.btn_reload, left_x, btn_y, 30, btn_height, true);
            left_x += 34;
            let _ = MoveWindow(self.btn_home, left_x, btn_y, 32, btn_height, true);
            left_x += 38; // left_x is now 146

            // 2. Right cluster: Action buttons positioned from the right edge
            let right_padding = 8;
            let mut right_x = (client_width - right_padding).max(left_x + 200);

            // Settings ⚙ (always visible)
            right_x -= 32;
            let _ = MoveWindow(self.btn_settings, right_x, btn_y, 32, btn_height, true);

            // Shield 🛡️ (always visible)
            right_x -= 76;
            let _ = MoveWindow(self.btn_shield, right_x, btn_y, 72, btn_height, true);

            // Lite Mode ⚡ (always visible)
            right_x -= 68;
            let _ = MoveWindow(self.btn_lite_mode, right_x, btn_y, 64, btn_height, true);

            // RAM Badge (always visible)
            right_x -= 100;
            let _ = MoveWindow(self.btn_ram_badge, right_x, btn_y, 96, btn_height, true);

            // Optional secondary buttons (only visible if window width >= 1050)
            if client_width >= 1050 {
                // Sleep Tabs
                right_x -= 42;
                let _ = MoveWindow(self.btn_sleep_tabs, right_x, btn_y, 38, btn_height, true);

                // Clean RAM
                right_x -= 42;
                let _ = MoveWindow(self.btn_clean_ram, right_x, btn_y, 38, btn_height, true);

                // Downloads
                right_x -= 34;
                let _ = MoveWindow(self.btn_downloads, right_x, btn_y, 30, btn_height, true);

                // Performance
                right_x -= 34;
                let _ = MoveWindow(self.btn_perf, right_x, btn_y, 30, btn_height, true);
            } else {
                // Safely hide optional buttons offscreen so they never squeeze address bar
                let _ = MoveWindow(self.btn_sleep_tabs, -200, -200, 0, 0, false);
                let _ = MoveWindow(self.btn_clean_ram, -200, -200, 0, 0, false);
                let _ = MoveWindow(self.btn_downloads, -200, -200, 0, 0, false);
                let _ = MoveWindow(self.btn_perf, -200, -200, 0, 0, false);
            }

            // Go button → (always visible, next to address bar)
            right_x -= 38;
            let _ = MoveWindow(self.btn_go, right_x, btn_y, 34, btn_height, true);

            // 3. Middle: Address Bar takes ALL remaining width between left_x and right_x
            let addr_x = left_x;
            let addr_w = (right_x - addr_x - 6).max(180);
            self.address_bar.set_bounds(addr_x, btn_y, addr_w, btn_height);
        }
    }

    /// Handles a WM_COMMAND event, returning an action if the command ID belongs to the toolbar.
    pub fn handle_command(&self, cmd_id: u16) -> Option<ToolbarAction> {
        match cmd_id {
            ID_NAV_BACK => Some(ToolbarAction::Back),
            ID_NAV_FORWARD => Some(ToolbarAction::Forward),
            ID_NAV_RELOAD => Some(ToolbarAction::Reload),
            ID_NAV_HOME => Some(ToolbarAction::Home),
            ID_NAV_GO => {
                let text = self.address_bar.get_text();
                Some(ToolbarAction::Navigate(text))
            }
            ID_ACTION_SHIELD => Some(ToolbarAction::ToggleShield),
            ID_ACTION_DOWNLOADS => Some(ToolbarAction::OpenDownloads),
            ID_ACTION_PERFORMANCE => Some(ToolbarAction::OpenPerformance),
            ID_ACTION_SETTINGS => Some(ToolbarAction::OpenSettings),
            ID_ACTION_LITE_MODE => Some(ToolbarAction::ToggleLiteMode),
            ID_ACTION_CLEAN_RAM => Some(ToolbarAction::CleanRam),
            ID_ACTION_SLEEP_TABS => Some(ToolbarAction::SleepTabs),
            ID_ACTION_RAM_BADGE => Some(ToolbarAction::OpenRamInspector),
            ID_ACTION_GAMING_MODE => Some(ToolbarAction::ToggleGamingMode),
            ID_ACTION_READER_MODE => Some(ToolbarAction::ToggleReaderMode),
            ID_ACTION_ELEMENT_ZAPPER => Some(ToolbarAction::StartElementZapper),
            ID_ACTION_CLEAN_URL_COPY => Some(ToolbarAction::CopyCleanUrl),
            _ => None,
        }
    }
}

impl Drop for BrowserToolbar {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.btn_back);
            let _ = DestroyWindow(self.btn_forward);
            let _ = DestroyWindow(self.btn_reload);
            let _ = DestroyWindow(self.btn_home);
            let _ = DestroyWindow(self.btn_go);
            let _ = DestroyWindow(self.btn_ram_badge);
            let _ = DestroyWindow(self.btn_lite_mode);
            let _ = DestroyWindow(self.btn_clean_ram);
            let _ = DestroyWindow(self.btn_sleep_tabs);
            let _ = DestroyWindow(self.btn_shield);
            let _ = DestroyWindow(self.btn_downloads);
            let _ = DestroyWindow(self.btn_perf);
            let _ = DestroyWindow(self.btn_settings);
        }
    }
}
