//! Native Win32 Address Bar edit control with Subclassed Enter-Key Navigation.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::errors::{Result, RustBrowseError};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetParent, GetWindowTextLengthW, GetWindowTextW, MoveWindow,
    SendMessageW, SetWindowLongPtrW, SetWindowTextW, ES_AUTOHSCROLL, ES_LEFT, GWLP_WNDPROC, HMENU,
    WINDOW_STYLE, WM_CHAR, WM_COMMAND, WM_KEYDOWN, WM_SETFONT, WS_CHILD, WS_EX_CLIENTEDGE,
    WS_TABSTOP, WS_VISIBLE,
};

pub const EM_SETSEL: u32 = 0x00B1;
pub const ID_NAV_GO: u16 = 106;

static ORIG_EDIT_PROC: AtomicUsize = AtomicUsize::new(0);

/// Subclass window procedure for the AddressBar EDIT control.
/// Intercepts the Enter key (VK_RETURN) so pressing Enter immediately navigates to the typed URL.
/// Also suppresses the default Windows error ding/beep sound on Enter.
unsafe extern "system" fn address_bar_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_KEYDOWN => {
            if wparam.0 == VK_RETURN.0 as usize {
                let parent = GetParent(hwnd);
                if let Ok(p) = parent {
                    if !p.is_invalid() {
                        let _ = SendMessageW(
                            p,
                            WM_COMMAND,
                            Some(WPARAM(ID_NAV_GO as usize)),
                            Some(LPARAM(hwnd.0 as isize)),
                        );
                    }
                }
                return LRESULT(0);
            }
        }
        WM_CHAR => {
            if wparam.0 == VK_RETURN.0 as usize {
                // Suppress Windows ding on Enter
                return LRESULT(0);
            }
        }
        _ => {}
    }

    let orig = ORIG_EDIT_PROC.load(Ordering::SeqCst);
    if orig != 0 {
        let orig_fn: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT =
            std::mem::transmute(orig);
        orig_fn(hwnd, msg, wparam, lparam)
    } else {
        windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

pub struct AddressBar {
    hwnd: HWND,
}

impl AddressBar {
    pub fn new(
        parent_hwnd: HWND,
        control_id: u16,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<Self> {
        unsafe {
            let style = WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WINDOW_STYLE(ES_AUTOHSCROLL as u32 | ES_LEFT as u32);

            let hwnd = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                w!("EDIT"),
                PCWSTR::null(),
                style,
                x,
                y,
                width,
                height,
                Some(parent_hwnd),
                Some(HMENU(control_id as usize as *mut _)),
                None,
                None,
            )
            .map_err(|e| {
                RustBrowseError::Windows(format!(
                    "Failed to create AddressBar edit control: {:?}",
                    e
                ))
            })?;

            // Subclass the edit control to handle Enter key navigation cleanly
            let orig = SetWindowLongPtrW(
                hwnd,
                GWLP_WNDPROC,
                address_bar_subclass_proc as *const () as usize as isize,
            );
            ORIG_EDIT_PROC.store(orig as usize, Ordering::SeqCst);

            // Set default GUI font for clean readability
            let h_font = GetStockObject(DEFAULT_GUI_FONT);
            let _ = SendMessageW(
                hwnd,
                WM_SETFONT,
                Some(WPARAM(h_font.0 as usize)),
                Some(LPARAM(1)),
            );

            Ok(Self { hwnd })
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Sets the URL or query text displayed in the address bar.
    pub fn set_text(&self, text: &str) {
        unsafe {
            let wide: Vec<u16> = OsStr::new(text)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let _ = SetWindowTextW(self.hwnd, PCWSTR(wide.as_ptr()));
        }
    }

    /// Retrieves the text currently typed into the address bar.
    pub fn get_text(&self) -> String {
        unsafe {
            let len = GetWindowTextLengthW(self.hwnd);
            if len <= 0 {
                return String::new();
            }
            let mut buf = vec![0u16; (len + 1) as usize];
            let read = GetWindowTextW(self.hwnd, &mut buf);
            String::from_utf16_lossy(&buf[..read as usize])
        }
    }

    /// Moves and resizes the address bar control.
    pub fn set_bounds(&self, x: i32, y: i32, width: i32, height: i32) {
        unsafe {
            let _ = MoveWindow(self.hwnd, x, y, width, height, true);
        }
    }

    /// Focuses the address bar and selects its content.
    pub fn focus_and_select_all(&self) {
        unsafe {
            let _ = SetFocus(Some(self.hwnd));
            let _ = SendMessageW(self.hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
        }
    }
}

impl Drop for AddressBar {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
