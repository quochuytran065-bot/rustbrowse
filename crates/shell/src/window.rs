//! Native Win32 browser shell window.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use common::errors::{Result, RustBrowseError};
use common::types::Rect;
use tokio::sync::mpsc::UnboundedSender;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, SetTimer, WM_COMMAND, WM_KEYDOWN, WM_LBUTTONDBLCLK,
    WM_LBUTTONDOWN, WM_PAINT, WM_RBUTTONUP, WM_SIZE, WM_TIMER,
};
use windows_window::Window;

use crate::tab_strip::TabStrip;

#[derive(Debug, Clone)]
pub enum ShellUiEvent {
    TabClick {
        x: i32,
        y: i32,
    },
    TabRightClick {
        x: i32,
        y: i32,
        screen_x: i32,
        screen_y: i32,
    },
    ToolbarCommand(u16),
    KeyDown {
        key: usize,
        is_ctrl: bool,
        is_alt: bool,
        is_shift: bool,
    },
    WindowMinimized,
    WindowRestored,
    TimerTick,
}

pub struct BrowserWindow {
    window: Window,
    tab_strip: Rc<RefCell<TabStrip>>,
}

impl BrowserWindow {
    pub fn new(
        title: &str,
        initial_width: i32,
        initial_height: i32,
        tab_strip: Rc<RefCell<TabStrip>>,
        event_tx: UnboundedSender<ShellUiEvent>,
    ) -> Result<Self> {
        let ts_clone = tab_strip.clone();
        let tx = event_tx;

        let window = Window::new(title)
            .size(initial_width, initial_height)
            .on_message(move |hwnd, msg, wparam, lparam| match msg {
                WM_PAINT => unsafe {
                    let mut ps = PAINTSTRUCT::default();
                    let hdc = BeginPaint(HWND(hwnd as *mut _), &mut ps);
                    let mut rc = RECT::default();
                    let _ = GetClientRect(HWND(hwnd as *mut _), &mut rc);
                    let client_width = rc.right - rc.left;
                    ts_clone.borrow().paint(hdc, client_width);
                    let _ = EndPaint(HWND(hwnd as *mut _), &ps);
                    Some(0)
                },
                WM_COMMAND => {
                    let cmd_id = (wparam & 0xFFFF) as u16;
                    let _ = tx.send(ShellUiEvent::ToolbarCommand(cmd_id));
                    Some(0)
                }
                WM_LBUTTONDOWN => {
                    let x = (lparam & 0xFFFF) as i16 as i32;
                    let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
                    if y < ts_clone.borrow().height() {
                        let _ = tx.send(ShellUiEvent::TabClick { x, y });
                        return Some(0);
                    }
                    None
                }
                WM_LBUTTONDBLCLK => {
                    let x = (lparam & 0xFFFF) as i16 as i32;
                    let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
                    if y < ts_clone.borrow().height() {
                        let hit = ts_clone.borrow().hit_test(x, y);
                        if hit == crate::tab_strip::TabStripHit::None {
                            let _ = tx.send(ShellUiEvent::TabClick { x: -1, y: -1 });
                            return Some(0);
                        }
                    }
                    None
                }
                WM_RBUTTONUP => {
                    let x = (lparam & 0xFFFF) as i16 as i32;
                    let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
                    if y < ts_clone.borrow().height() {
                        unsafe {
                            let mut pt = POINT::default();
                            let _ = GetCursorPos(&mut pt);
                            let _ = tx.send(ShellUiEvent::TabRightClick {
                                x,
                                y,
                                screen_x: pt.x,
                                screen_y: pt.y,
                            });
                        }
                        return Some(0);
                    }
                    None
                }
                WM_KEYDOWN => unsafe {
                    let is_ctrl = (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
                    let is_alt = (GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
                    let is_shift = (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
                    let _ = tx.send(ShellUiEvent::KeyDown {
                        key: wparam,
                        is_ctrl,
                        is_alt,
                        is_shift,
                    });
                    None
                },
                WM_SIZE => {
                    if wparam == 1 {
                        let _ = tx.send(ShellUiEvent::WindowMinimized);
                    } else if wparam == 0 || wparam == 2 {
                        let _ = tx.send(ShellUiEvent::WindowRestored);
                    }
                    None
                }
                WM_TIMER => {
                    let _ = tx.send(ShellUiEvent::TimerTick);
                    Some(0)
                }
                _ => None,
            })
            .create()
            .map_err(|e| {
                RustBrowseError::Windows(format!("Failed to create native Win32 window: {:?}", e))
            })?;

        unsafe {
            let _ = SetTimer(Some(HWND(window.hwnd() as *mut _)), 1001, 1000, None);
        }

        Ok(Self { window, tab_strip })
    }

    pub fn hwnd(&self) -> *mut c_void {
        self.window.hwnd()
    }

    pub fn client_size(&self) -> (i32, i32) {
        self.window.client_size()
    }

    /// Computes the viewport rectangle for the active tab (e.g. below toolbar).
    pub fn compute_webview_bounds(&self, toolbar_height: i32) -> Rect {
        let (width, height) = self.client_size();
        Rect::new(0, toolbar_height, width, height)
    }

    /// Triggers an immediate GDI repaint for the tab strip bar.
    pub fn invalidate_tab_strip(&self) {
        unsafe {
            let (width, _) = self.client_size();
            let rc = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: self.tab_strip.borrow().height(),
            };
            let _ = InvalidateRect(Some(HWND(self.hwnd() as *mut _)), Some(&rc), false);
        }
    }
}
