//! Startup Cache Optimizer & Safe Chromium GPU Pipeline Config (Google-Optimizer Philosophy)
//! Safely cleans bloated caches while strictly preserving user sessions, logins, and cookies.

use std::path::Path;
use tracing::{info, warn};

pub struct StartupOptimizer;

impl StartupOptimizer {
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
            base.join("Default").join("Service Worker").join("CacheStorage"),
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

        info!(
            purged_directories = purged_count,
            "Startup cache clean completed safely without touching Cookies or Logins."
        );
    }

    /// Returns the verified Windows 11 GPU rendering & compositor flags.
    pub fn get_gpu_rendering_flags() -> Vec<&'static str> {
        vec![
            "--use-angle=d3d11",
            "--enable-gpu-rasterization",
            "--enable-zero-copy",
            "--enable-oop-rasterization",
            "--ignore-gpu-blocklist",
            "--disable-gpu-vsync=false",
            "--force-color-profile=srgb",
            "--enable-features=AsyncDns,IntensiveWakeUpThrottling,ParallelDownloading",
            "--disable-features=BackForwardCache,Prerender2,OptimizationHints,MediaRouter,PreloadMediaEngagementData,InterestFeedContentSuggestions,AutofillServerCommunication,Translate",
            "--disable-spell-checking",
            "--no-first-run",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_startup_clean_preserves_cookies() {
        let temp_dir = std::env::temp_dir().join("rustbrowse_test_clean");
        let default_dir = temp_dir.join("Default");
        let cookies_file = default_dir.join("Cookies");
        let code_cache = default_dir.join("Code Cache");

        std::fs::create_dir_all(&code_cache).unwrap();
        std::fs::write(&cookies_file, "mock_cookies_content").unwrap();
        std::fs::write(code_cache.join("blob.bin"), "cache_content").unwrap();

        StartupOptimizer::safe_startup_cache_clean(&temp_dir);

        assert!(cookies_file.exists(), "Cookies file MUST NOT be deleted!");
        assert!(!code_cache.exists(), "Code Cache directory MUST be purged!");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
