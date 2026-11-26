//! Script and style injection into pages (Cosmetic CSS + Web Optimizer + Popup Guard + Media Adblock).

use adblock_engine::CosmeticFilter;
use common::errors::{Result, RustBrowseError};
use windows_webview::WebView;

pub struct PageInjection;

impl PageInjection {
    /// Injects cosmetic element-hiding script into the page before DOM load.
    pub fn inject_cosmetic_filters(webview: &WebView, hostname: &str) -> Result<()> {
        let script = CosmeticFilter::build_injection_script(hostname);
        webview
            .add_script_to_execute_on_document_created(&script)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject cosmetic script: {:?}", e)))
    }

    /// Injects ad_nuker.js (Universal Zero-Tolerance Ad Nuker for all frames & iframes).
    pub fn inject_ad_nuker(webview: &WebView) -> Result<()> {
        const NUKER_SCRIPT: &str = include_str!("../../../scripts/ad_nuker.js");
        webview
            .add_script_to_execute_on_document_created(NUKER_SCRIPT)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject ad nuker script: {:?}", e)))
    }

    /// Injects web_optimizer.js (H.264 override, DOM pruning, offscreen video pause, GPU effect killer).
    pub fn inject_web_optimizer(webview: &WebView) -> Result<()> {
        const OPTIMIZER_SCRIPT: &str = include_str!("../../../scripts/web_optimizer.js");
        webview
            .add_script_to_execute_on_document_created(OPTIMIZER_SCRIPT)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject web optimizer script: {:?}", e)))
    }

    /// Injects ad_sweeper.js (Catfish ad removal, heuristic banner scanning, anti-clickjacking, auto-redirect preventer).
    pub fn inject_ad_sweeper(webview: &WebView) -> Result<()> {
        const SWEEPER_SCRIPT: &str = include_str!("../../../scripts/ad_sweeper.js");
        webview
            .add_script_to_execute_on_document_created(SWEEPER_SCRIPT)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject ad sweeper script: {:?}", e)))
    }

    /// Injects popup_guard.js (Global New Tab & Popup/Popunder Guard, Click-Jacking Defender).
    pub fn inject_popup_guard(webview: &WebView) -> Result<()> {
        const POPUP_SCRIPT: &str = include_str!("../../../scripts/popup_guard.js");
        webview
            .add_script_to_execute_on_document_created(POPUP_SCRIPT)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject popup guard script: {:?}", e)))
    }

    /// Injects media_adblock.js (YouTube, Facebook, Spotify Web ad blocker - SpotX Web + Brave logic).
    pub fn inject_media_adblock(webview: &WebView) -> Result<()> {
        const MEDIA_SCRIPT: &str = include_str!("../../../scripts/media_adblock.js");
        webview
            .add_script_to_execute_on_document_created(MEDIA_SCRIPT)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject media adblock script: {:?}", e)))
    }

    /// Injects reader_mode.js (Smart Reader Mode 1-click clean reading).
    pub fn inject_reader_mode(webview: &WebView) -> Result<()> {
        const READER_SCRIPT: &str = include_str!("../../../scripts/reader_mode.js");
        webview
            .add_script_to_execute_on_document_created(READER_SCRIPT)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject reader mode script: {:?}", e)))
    }

    /// Injects element_zapper.js (Element Zapper 1-click element blocker).
    pub fn inject_element_zapper(webview: &WebView) -> Result<()> {
        const ZAPPER_SCRIPT: &str = include_str!("../../../scripts/element_zapper.js");
        webview
            .add_script_to_execute_on_document_created(ZAPPER_SCRIPT)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject element zapper script: {:?}", e)))
    }

    /// Injects zero_delay.js (Strips CSS animation delays and transitions for instant click responsiveness).
    pub fn inject_zero_delay(webview: &WebView) -> Result<()> {
        const ZERO_DELAY_SCRIPT: &str = include_str!("../../../scripts/zero_delay.js");
        webview
            .add_script_to_execute_on_document_created(ZERO_DELAY_SCRIPT)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject zero delay script: {:?}", e)))
    }

    /// Injects lenis_scroll.js (Smooth, jitter-free scroll interpolation with 0% CPU at idle).
    pub fn inject_lenis_scroll(webview: &WebView) -> Result<()> {
        const LENIS_SCRIPT: &str = include_str!("../../../scripts/lenis_scroll.js");
        webview
            .add_script_to_execute_on_document_created(LENIS_SCRIPT)
            .map(|_| ())
            .map_err(|e| RustBrowseError::Engine(format!("Failed to inject Lenis smooth scroll script: {:?}", e)))
    }
}
