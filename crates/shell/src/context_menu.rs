//! Win32 popup context menu for tabs.

use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, TrackPopupMenu, MF_SEPARATOR, MF_STRING,
    TPM_LEFTALIGN, TPM_RETURNCMD, TPM_TOPALIGN,
};

pub const IDM_CLOSE_TAB: u32 = 1001;
pub const IDM_CLOSE_OTHER_TABS: u32 = 1002;
pub const IDM_SLEEP_TAB: u32 = 1003;
pub const IDM_DISCARD_TAB: u32 = 1004;
pub const IDM_TOGGLE_PIN: u32 = 1005;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabContextMenuAction {
    CloseTab,
    CloseOtherTabs,
    SleepTab,
    DiscardTab,
    TogglePin,
}

pub struct TabContextMenu;

impl TabContextMenu {
    /// Displays the context menu at the specified screen coordinates and returns the user's choice.
    pub fn show(
        hwnd: *mut std::ffi::c_void,
        screen_x: i32,
        screen_y: i32,
        is_pinned: bool,
    ) -> Option<TabContextMenuAction> {
        let hwnd = HWND(hwnd);
        unsafe {
            let menu = match CreatePopupMenu() {
                Ok(m) => m,
                Err(_) => return None,
            };

            let pin_label = if is_pinned {
                w!("Bỏ ghim tab")
            } else {
                w!("Ghim tab")
            };

            let _ = AppendMenuW(menu, MF_STRING, IDM_CLOSE_TAB as usize, w!("Đóng tab"));
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                IDM_CLOSE_OTHER_TABS as usize,
                w!("Đóng các tab khác"),
            );
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                IDM_SLEEP_TAB as usize,
                w!("Cho tab ngủ (Sleep)"),
            );
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                IDM_DISCARD_TAB as usize,
                w!("Giải phóng RAM tab này (Discard)"),
            );
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            let _ = AppendMenuW(menu, MF_STRING, IDM_TOGGLE_PIN as usize, pin_label);

            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN,
                screen_x,
                screen_y,
                Some(0),
                hwnd,
                None,
            );

            let _ = DestroyMenu(menu);

            match cmd.0 as u32 {
                IDM_CLOSE_TAB => Some(TabContextMenuAction::CloseTab),
                IDM_CLOSE_OTHER_TABS => Some(TabContextMenuAction::CloseOtherTabs),
                IDM_SLEEP_TAB => Some(TabContextMenuAction::SleepTab),
                IDM_DISCARD_TAB => Some(TabContextMenuAction::DiscardTab),
                IDM_TOGGLE_PIN => Some(TabContextMenuAction::TogglePin),
                _ => None,
            }
        }
    }
}
