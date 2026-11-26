//! Singleton WebView2 Environment management.

use common::config::BrowserConfig;
use common::errors::{Result, RustBrowseError};
use windows_webview::{Environment, EnvironmentOptions};

pub struct WebView2EnvironmentHolder {
    inner: Environment,
}

impl WebView2EnvironmentHolder {
    /// Creates the single shared Environment using configuration.
    pub fn create(config: &BrowserConfig) -> Result<Self> {
        tracing::info!(
            user_data = %config.user_data_folder,
            "Initializing shared WebView2 Environment..."
        );

        let additional_args = config.chromium_flags.build_arguments_string();

        let mut options = EnvironmentOptions::new().user_data_folder(&config.user_data_folder);

        if !additional_args.is_empty() {
            tracing::info!(flags = %additional_args, "Passing Chromium flags to Environment");
            options = options.additional_browser_arguments(additional_args);
        }

        let inner = Environment::with_options(&options).map_err(|e| {
            RustBrowseError::Engine(format!("Failed to create WebView2 Environment: {:?}", e))
        })?;

        tracing::info!("Shared WebView2 Environment successfully initialized.");
        Ok(Self { inner })
    }

    pub fn inner(&self) -> &Environment {
        &self.inner
    }
}
