//! Startup Maintenance Optimizer for RustBrowse (Windows 11)
//! Strictly preserves HTTP Cache, Code Cache, GPUCache, ShaderCache, Cookies,
//! Local Storage, IndexedDB, Login Data, and Preferences so cold starts and
//! progressive image loading remain instant. Only cleans stale crash dumps and
//! telemetry metrics asynchronously after a 15-second boot grace period.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{info, warn};

const STARTUP_DEFER_SECS: u64 = 15;
const STALE_CRASH_AGE_SECS: u64 = 7 * 24 * 3600; // 7 days

pub struct StartupOptimizer;

impl StartupOptimizer {
    /// Launches a low-priority background thread that waits 15 seconds after boot
    /// before cleaning stale crash dumps / metrics, never blocking startup or
    /// invalidating HTTP/Code/GPU caches.
    pub fn launch_background_cache_clean() {
        let _ = std::thread::Builder::new()
            .name("deferred-maintenance".to_string())
            .spawn(|| {
                std::thread::sleep(Duration::from_secs(STARTUP_DEFER_SECS));
                if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                    let user_data = PathBuf::from(&local_app_data)
                        .join("RustBrowse")
                        .join("User Data");
                    Self::safe_startup_cache_clean(&user_data);

                    let eb_webview = PathBuf::from(&local_app_data)
                        .join("com.rustbrowse.app")
                        .join("EBWebView");
                    Self::safe_startup_cache_clean(&eb_webview);
                }
            });
    }

    /// Safely cleans ONLY stale Crashpad dumps (>7 days old) and BrowserMetrics.
    /// STRICTLY PRESERVES: HTTP Cache, Code Cache, GPUCache, ShaderCache,
    /// Service Worker CacheStorage, Cookies, Local Storage, IndexedDB, and Logins!
    pub fn safe_startup_cache_clean<P: AsRef<Path>>(user_data_dir: P) {
        let base = user_data_dir.as_ref();
        if !base.exists() {
            return;
        }

        let mut cleaned_items = 0usize;

        // 1. Clean BrowserMetrics (telemetry scratch files only)
        let metrics_dir = base.join("BrowserMetrics");
        if metrics_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&metrics_dir) {
                warn!(path = ?metrics_dir, error = ?e, "Could not clean BrowserMetrics");
            } else {
                cleaned_items += 1;
            }
        }

        // 2. Clean stale Crashpad reports older than 7 days (preserve recent crash dumps)
        let crash_reports = base.join("Crashpad").join("reports");
        if let Ok(entries) = std::fs::read_dir(&crash_reports) {
            let now = SystemTime::now();
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let is_stale = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| now.duration_since(t).ok())
                    .map(|age| age.as_secs() > STALE_CRASH_AGE_SECS)
                    .unwrap_or(false);
                if is_stale && std::fs::remove_file(&path).is_ok() {
                    cleaned_items += 1;
                }
            }
        }

        if cleaned_items > 0 {
            info!(
                cleaned_items,
                "Deferred maintenance completed safely (HTTP/Code/GPU caches, Cookies, and Logins preserved)."
            );
        }
    }
}
