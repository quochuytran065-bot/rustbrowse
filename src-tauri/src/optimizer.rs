//! Startup Cache Optimizer & Safe Chromium GPU Pipeline Config (Google-Optimizer Philosophy)
//! Safely cleans bloated caches while strictly preserving user sessions, logins, and cookies.

use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub struct StartupOptimizer;

impl StartupOptimizer {
    /// Launches a low-priority background thread to clean temporary caches without blocking UI startup.
    pub fn launch_background_cache_clean() {
        std::thread::Builder::new()
            .name("cache-cleaner".to_string())
            .spawn(|| {
                // Find local app data user data folder for RustBrowse / WebView2
                if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                    let user_data = PathBuf::from(&local_app_data)
                        .join("RustBrowse")
                        .join("User Data");
                    Self::safe_startup_cache_clean(&user_data);

                    // Also clean EBWebView cache under app data if present
                    let eb_webview = PathBuf::from(&local_app_data)
                        .join("com.rustbrowse.app")
                        .join("EBWebView");
                    Self::safe_startup_cache_clean(&eb_webview);
                }
            })
            .ok();
    }

    /// Safely purges temporary caches, shader bloat, and crashpad dumps on browser startup.
    /// STRICTLY PRESERVES: Cookies, Local Storage, IndexedDB, Login Data, and Preferences!
    pub fn safe_startup_cache_clean<P: AsRef<Path>>(user_data_dir: P) {
        let base = user_data_dir.as_ref();
        if !base.exists() {
            return;
        }

        let purge_targets = [
            base.join("Default").join("Code Cache"),
            base.join("Default").join("GPUCache"),
            base.join("Default").join("ShaderCache"),
            base.join("Default")
                .join("Service Worker")
                .join("CacheStorage"),
            base.join("Crashpad"),
            base.join("BrowserMetrics"),
        ];

        let mut purged_count = 0;
        for path in &purge_targets {
            if path.exists() {
                if let Err(e) = std::fs::remove_dir_all(path) {
                    warn!(path = ?path, error = ?e, "Could not remove cache directory (may be locked by another process)");
                } else {
                    purged_count += 1;
                }
            }
        }

        if purged_count > 0 {
            info!(
                purged_directories = purged_count,
                "Startup cache clean completed safely without touching Cookies or Logins."
            );
        }
    }
}
