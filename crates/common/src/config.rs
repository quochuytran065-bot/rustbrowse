//! Configuration schemas and defaults for RustBrowse.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub user_data_folder: String,
    pub home_url: String,
    pub memory: MemoryConfig,
    pub monitor: MonitorConfig,
    pub chromium_flags: ChromiumFlagsConfig,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        let local_app_data =
            std::env::var("LOCALAPPDATA").unwrap_or_else(|_| r"D:\RustBrowse\Data".to_string());
        let default_udf = format!(r"{}\RustBrowse\User Data", local_app_data);

        Self {
            user_data_folder: default_udf,
            home_url: "rustbrowse://selftest".to_string(),
            memory: MemoryConfig::default(),
            monitor: MonitorConfig::default(),
            chromium_flags: ChromiumFlagsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Seconds hidden before transitioning Active -> Warm (default: 30)
    pub warm_threshold_secs: u64,
    /// Seconds hidden before transitioning Warm -> Sleeping (default: 90)
    pub sleep_threshold_secs: u64,
    /// Seconds hidden before transitioning Sleeping -> Discarded (default: 300)
    pub discard_threshold_secs: u64,
    /// Warning pressure available RAM threshold in MB (default: 1024)
    pub warning_available_ram_mb: u64,
    /// Critical pressure available RAM threshold in MB (default: 600)
    pub critical_available_ram_mb: u64,
    /// Critical memory load percentage (default: 85)
    pub critical_load_percent: u32,
    /// Maximum live WebView2 controllers allowed when RAM is normal
    pub normal_max_live_tabs: usize,
    /// Maximum live WebView2 controllers allowed when RAM is warning
    pub warning_max_live_tabs: usize,
    /// Trim working set on minimize (default: false until benchmarked)
    pub trim_on_minimize: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            warm_threshold_secs: 60,
            sleep_threshold_secs: 180,    // 3 minutes (Soft Sleep)
            discard_threshold_secs: 1200, // 20 minutes (Hard Unload)
            warning_available_ram_mb: 1024,
            critical_available_ram_mb: 600,
            critical_load_percent: 85,
            normal_max_live_tabs: 15,     // Keep up to 15 tabs alive in Soft Sleep (0ms wake)
            warning_max_live_tabs: 8,
            trim_on_minimize: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Monitor interval when main window is visible (default: 5 s)
    pub visible_interval_secs: u64,
    /// Monitor interval when main window is minimized or idle (default: 20 s)
    pub idle_interval_secs: u64,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            visible_interval_secs: 5,
            idle_interval_secs: 20,
        }
    }
}

/// Additional chromium arguments passed to WebView2 Environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromiumFlagsConfig {
    /// Additional arguments string
    pub additional_args: Vec<String>,
    /// Whether to limit renderer process count
    pub renderer_process_limit: Option<u32>,
    /// Disable background networking
    pub disable_background_networking: bool,
}

impl Default for ChromiumFlagsConfig {
    fn default() -> Self {
        Self {
            additional_args: vec![
                r#"--js-flags="--max-old-space-size=256 --optimize-for-size""#.to_string(),
                "--use-angle=d3d11".to_string(),
                "--enable-gpu-rasterization".to_string(),
                "--enable-zero-copy".to_string(),
                "--enable-oop-rasterization".to_string(),
                "--ignore-gpu-blocklist".to_string(),
                "--force-color-profile=srgb".to_string(),
                "--enable-features=AsyncDns,IntensiveWakeUpThrottling,ParallelDownloading".to_string(),
                "--disable-features=BackForwardCache,Prerender2,OptimizationHints,MediaRouter,PreloadMediaEngagementData,InterestFeedContentSuggestions,AutofillServerCommunication,Translate,AudioServiceOutOfProcess".to_string(),
                "--aggressive-cache-discard".to_string(),
                "--disk-cache-size=52428800".to_string(),
                "--media-cache-size=33554432".to_string(),
                "--renderer-process-limit=2".to_string(),
                "--process-per-site".to_string(),
                "--disable-smooth-scrolling".to_string(),
                "--enable-quic".to_string(),
                "--disable-component-update".to_string(),
                "--disable-sync".to_string(),
                "--disable-metrics".to_string(),
                "--disable-spell-checking".to_string(),
                "--no-first-run".to_string(),
            ],
            renderer_process_limit: Some(2),
            disable_background_networking: true,
        }
    }
}

impl ChromiumFlagsConfig {
    /// Assembles verified flags string, rejecting any forbidden flags.
    pub fn build_arguments_string(&self) -> String {
        let mut flags = Vec::new();

        if let Some(limit) = self.renderer_process_limit {
            let limit_flag = format!("--renderer-process-limit={}", limit);
            if !self.additional_args.iter().any(|a| a.starts_with("--renderer-process-limit")) {
                flags.push(limit_flag);
            }
        }
        if self.disable_background_networking {
            if !self.additional_args.iter().any(|a| a == "--disable-background-networking") {
                flags.push("--disable-background-networking".to_string());
            }
        }

        for arg in &self.additional_args {
            // Strictly reject unsafe flags
            if arg.contains("--no-sandbox")
                || arg.contains("--disable-web-security")
                || arg.contains("--ignore-certificate-errors")
                || arg.contains("--disable-gpu")
            {
                tracing::warn!("Forbidden Chromium flag rejected: {}", arg);
                continue;
            }
            flags.push(arg.clone());
        }

        flags.join(" ")
    }

    /// Sets the WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS environment variable.
    pub fn apply_to_environment(&self) {
        let args = self.build_arguments_string();
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", &args);
        tracing::info!(flags = %args, "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS exported to environment.");
    }
}
