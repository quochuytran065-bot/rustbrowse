//! Tab memory optimization using Microsoft WebView2 TrySuspend and Resume.
//! Conforms to Microsoft recommendations: never interleave MemoryUsageTargetLevel with Suspend.

use std::ffi::c_void;
use common::errors::{Result, RustBrowseError};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
use windows_webview::{MemoryUsageTargetLevel, WebView};

// IID of ICoreWebView2_3: {A0D6DF20-3B92-416D-AA0C-437A9C727857}
const IID_ICORE_WEBVIEW2_3: [u8; 16] = [
    0x20, 0xdf, 0xd6, 0xa0, 0x92, 0x3b, 0x6d, 0x41, 0xaa, 0x0c, 0x43, 0x7a, 0x9c, 0x72, 0x78, 0x57,
];

pub struct TabSuspensionManager;

impl TabSuspensionManager {
    /// Sets memory usage target level (Low vs Normal).
    pub fn set_memory_target(webview: &WebView, low: bool) -> Result<()> {
        let level = if low {
            MemoryUsageTargetLevel::Low
        } else {
            MemoryUsageTargetLevel::Normal
        };

        webview.set_memory_usage_target_level(level).map_err(|e| {
            RustBrowseError::Engine(format!("SetMemoryUsageTargetLevel failed: {:?}", e))
        })
    }

    /// Queries `ICoreWebView2_3` directly through COM QueryInterface ABI.
    fn query_webview3(webview: &WebView) -> Option<ICoreWebView2_3> {
        unsafe {
            let raw_ptr: *mut c_void = std::mem::transmute_copy(webview);
            if raw_ptr.is_null() {
                return None;
            }

            // COM QueryInterface is slot 0 of vtable:
            // HRESULT QueryInterface(REFIID riid, void **ppvObject);
            let vtable = *(raw_ptr as *mut *const usize);
            let qi_fn: unsafe extern "system" fn(*mut c_void, *const u8, *mut *mut c_void) -> i32 =
                std::mem::transmute(*vtable);

            let mut out_ptr: *mut c_void = std::ptr::null_mut();
            let hr = qi_fn(raw_ptr, IID_ICORE_WEBVIEW2_3.as_ptr(), &mut out_ptr);
            if hr == 0 && !out_ptr.is_null() {
                Some(std::mem::transmute(out_ptr))
            } else {
                None
            }
        }
    }

    /// Suspends a hidden tab's renderer using `ICoreWebView2_3::TrySuspend`.
    /// Tells Chromium to pause background execution and release renderer working set.
    pub fn suspend_tab(webview: &WebView) -> Result<()> {
        if let Some(w3) = Self::query_webview3(webview) {
            unsafe {
                w3.TrySuspend(None).map_err(|e| {
                    RustBrowseError::Engine(format!("TrySuspend failed: {:?}", e))
                })
            }
        } else {
            // Graceful fallback for environments where ICoreWebView2_3 QI fails
            Self::set_memory_target(webview, true)
        }
    }

    /// Resumes a suspended tab using `ICoreWebView2_3::Resume`.
    pub fn resume_tab(webview: &WebView) -> Result<()> {
        if let Some(w3) = Self::query_webview3(webview) {
            unsafe {
                w3.Resume().map_err(|e| {
                    RustBrowseError::Engine(format!("Resume failed: {:?}", e))
                })
            }
        } else {
            Self::set_memory_target(webview, false)
        }
    }
}
