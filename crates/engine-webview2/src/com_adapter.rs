//! COM Single-Threaded Apartment (STA) adapter and thread assertions.

use common::errors::{Result, RustBrowseError};
use std::thread::ThreadId;
use windows::core::HRESULT;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

const RPC_E_CHANGED_MODE: HRESULT = HRESULT(0x80010106u32 as i32);

pub struct ComApartmentGuard {
    owning_thread: ThreadId,
}

impl ComApartmentGuard {
    /// Initializes COM STA for current thread.
    pub fn new() -> Result<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr == RPC_E_CHANGED_MODE {
            return Err(RustBrowseError::Windows(
                "COM apartment mismatch: UI thread must be single-threaded apartment (STA)."
                    .to_string(),
            ));
        }

        Ok(Self {
            owning_thread: std::thread::current().id(),
        })
    }

    /// Asserts that caller is running on the STA UI thread.
    #[inline]
    pub fn assert_on_sta_thread(&self) {
        debug_assert_eq!(
            std::thread::current().id(),
            self.owning_thread,
            "CRITICAL: WebView2 COM objects must NEVER be called from non-STA background threads!"
        );
    }
}
