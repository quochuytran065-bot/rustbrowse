//! Application bootstrap: logging, DPI, environment prep.

use common::config::BrowserConfig;
use common::errors::Result;
use shell::DpiManager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct Bootstrap;

impl Bootstrap {
    pub fn init() -> Result<BrowserConfig> {
        // 1. Initialize tracing
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .compact();

        tracing_subscriber::registry().with(fmt_layer).init();

        tracing::info!("=== Starting RustBrowse Desktop Browser ===");

        // 2. Enable Per-Monitor v2 DPI awareness
        DpiManager::enable_per_monitor_v2();
        tracing::info!("Per-Monitor v2 DPI awareness enabled.");

        // 3. Load or generate default config
        let config = BrowserConfig::default();
        tracing::info!(user_data = %config.user_data_folder, "Configuration initialized.");

        // Ensure user data folder directory exists on disk
        if let Err(e) = std::fs::create_dir_all(&config.user_data_folder) {
            tracing::warn!(error = %e, "Could not pre-create user data directory; WebView2 will attempt creation.");
        }

        Ok(config)
    }
}
