//! Per-Monitor DPI awareness and scaling helpers.

use common::types::DpiScale;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

pub struct DpiManager;

impl DpiManager {
    /// Enables Per-Monitor v2 DPI awareness for the process.
    pub fn enable_per_monitor_v2() {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    /// Queries the DPI scale for a specific window.
    pub fn get_dpi_scale_for_hwnd(hwnd: HWND) -> DpiScale {
        unsafe {
            let dpi = GetDpiForWindow(hwnd);
            let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };
            DpiScale(scale)
        }
    }

    /// Scales physical pixels by DPI factor.
    pub fn scale_pixels(pixels: i32, scale: DpiScale) -> i32 {
        (pixels as f64 * scale.0).round() as i32
    }
}
