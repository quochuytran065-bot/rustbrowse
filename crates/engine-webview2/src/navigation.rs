//! Navigation execution on tab webviews with internal rustbrowse:// routing.

use common::errors::{Result, RustBrowseError};
use windows_webview::WebView;

use crate::internal_pages::InternalPages;
use adblock_engine::AdblockStats;

pub struct NavigationHandler;

impl NavigationHandler {
    /// Navigates webview to URL, or renders internal pages if `rustbrowse://` schema is used.
    pub fn navigate_with_stats(
        webview: &WebView,
        raw_url: &str,
        stats: Option<&AdblockStats>,
        ram_mb: f64,
        active_tabs: usize,
        sleeping_tabs: usize,
        discarded_tabs: usize,
    ) -> Result<()> {
        let trimmed = raw_url.trim();

        if trimmed.is_empty() || trimmed == "about:blank" || trimmed == "rustbrowse://home" || trimmed == "about:home" {
            let dummy_stats = AdblockStats::new();
            let s = stats.unwrap_or(&dummy_stats);
            let html = InternalPages::render_home(ram_mb, active_tabs, s);
            return webview
                .navigate_to_string(&html)
                .map_err(|e| RustBrowseError::Engine(format!("Navigate to string failed: {:?}", e)));
        }

        if trimmed == "rustbrowse://privacy" {
            let dummy_stats = AdblockStats::new();
            let s = stats.unwrap_or(&dummy_stats);
            let html = InternalPages::render_privacy(s);
            return webview
                .navigate_to_string(&html)
                .map_err(|e| RustBrowseError::Engine(format!("Navigate to string failed: {:?}", e)));
        }

        if trimmed == "rustbrowse://performance" {
            let html = InternalPages::render_performance(ram_mb, active_tabs, sleeping_tabs, discarded_tabs);
            return webview
                .navigate_to_string(&html)
                .map_err(|e| RustBrowseError::Engine(format!("Navigate to string failed: {:?}", e)));
        }

        if trimmed == "rustbrowse://downloads" {
            let html = InternalPages::render_downloads();
            return webview
                .navigate_to_string(&html)
                .map_err(|e| RustBrowseError::Engine(format!("Navigate to string failed: {:?}", e)));
        }

        if trimmed == "rustbrowse://bookmarks" {
            let html = InternalPages::render_bookmarks();
            return webview
                .navigate_to_string(&html)
                .map_err(|e| RustBrowseError::Engine(format!("Navigate to string failed: {:?}", e)));
        }

        if trimmed == "rustbrowse://history" {
            let html = InternalPages::render_history();
            return webview
                .navigate_to_string(&html)
                .map_err(|e| RustBrowseError::Engine(format!("Navigate to string failed: {:?}", e)));
        }

        if trimmed == "rustbrowse://settings" {
            let html = InternalPages::render_settings();
            return webview
                .navigate_to_string(&html)
                .map_err(|e| RustBrowseError::Engine(format!("Navigate to string failed: {:?}", e)));
        }

        if trimmed == "rustbrowse://selftest" {
            let html = InternalPages::render_selftest();
            return webview
                .navigate_to_string(&html)
                .map_err(|e| RustBrowseError::Engine(format!("Navigate to string failed: {:?}", e)));
        }

        if trimmed.starts_with("rustbrowse://blocked") {
            let target_url = if let Some(idx) = trimmed.find("url=") {
                &trimmed[idx + 4..]
            } else {
                "Unknown URL"
            };
            let html = InternalPages::render_blocked(target_url);
            return webview
                .navigate_to_string(&html)
                .map_err(|e| RustBrowseError::Engine(format!("Navigate to string failed: {:?}", e)));
        }

        // Smart omnibox: if text doesn't look like a URL, treat as search query
        let destination = if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("file:///")
            || trimmed.starts_with("edge://")
            || trimmed.starts_with("chrome://")
        {
            trimmed.to_string()
        } else if trimmed.contains('.') && !trimmed.contains(' ') {
            format!("https://{}", trimmed)
        } else {
            let encoded = trimmed.replace(' ', "+");
            format!("https://www.google.com/search?q={}", encoded)
        };

        webview
            .navigate(&destination)
            .map_err(|e| RustBrowseError::Engine(format!("Navigate to '{}' failed: {:?}", destination, e)))
    }

    pub fn navigate(webview: &WebView, url: &str) -> Result<()> {
        Self::navigate_with_stats(webview, url, None, 35.0, 1, 0, 0)
    }

    pub fn reload(webview: &WebView) -> Result<()> {
        webview
            .reload()
            .map_err(|e| RustBrowseError::Engine(format!("Reload failed: {:?}", e)))
    }

    pub fn stop(webview: &WebView) -> Result<()> {
        webview
            .stop()
            .map_err(|e| RustBrowseError::Engine(format!("Stop failed: {:?}", e)))
    }

    pub fn go_back(webview: &WebView) -> Result<()> {
        webview
            .go_back()
            .map_err(|e| RustBrowseError::Engine(format!("GoBack failed: {:?}", e)))
    }

    pub fn go_forward(webview: &WebView) -> Result<()> {
        webview
            .go_forward()
            .map_err(|e| RustBrowseError::Engine(format!("GoForward failed: {:?}", e)))
    }
}
