//! Network request interception wiring between WebView2, AdblockEngine, ActiveShield, and Lite Mode.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use adblock_engine::{AdblockEngine, FilterDecision};
use common::errors::{Result, RustBrowseError};
use windows_webview::{EventRegistration, WebResourceRequest, WebView};

use crate::response_handler::ResponseHandler;
use crate::shield::ActiveShield;

pub struct RequestInterceptor;

impl RequestInterceptor {
    /// Binds `on_web_resource_requested` to the tab's `WebView`.
    /// `windows-webview` 0.100.0 automatically registers `AddWebResourceRequestedFilterWithRequestSourceKinds`
    /// with `COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL` covering top documents, iframes, and workers.
    pub fn attach(
        webview: &WebView,
        adblock: Arc<AdblockEngine>,
        shield: Arc<ActiveShield>,
        lite_mode: Arc<AtomicBool>,
    ) -> Result<EventRegistration> {
        webview
            .on_web_resource_requested("*", move |req: WebResourceRequest| {
                let uri = req.uri();
                let method = req.method();
                let lower = uri.to_lowercase();

                // 0. Whitelist essential CDN icon fonts and video stream chunks first
                if ActiveShield::is_essential_resource(&uri) {
                    return None;
                }

                // 1. Layer A: Active Shield inspection for gambling, scam, popunder, and malvertising
                if shield.is_blocked(&uri) {
                    shield.log_blocked_request(&uri);
                    return Some(ResponseHandler::blocked_response());
                }

                // 2. Lite Mode Inspection: block decorative web fonts (.woff2, .woff, .ttf)
                if lite_mode.load(Ordering::Relaxed) {
                    if lower.ends_with(".woff2")
                        || lower.ends_with(".woff")
                        || lower.ends_with(".ttf")
                        || lower.ends_with(".otf")
                        || lower.contains("fonts.gstatic.com")
                        || lower.contains("fonts.googleapis.com")
                    {
                        shield.log_blocked_request(&uri);
                        return Some(ResponseHandler::blocked_response());
                    }
                }

                // 3. Infer resource type from URI heuristics
                let resource_type = if lower.ends_with(".js") || lower.contains("/js/") {
                    "script"
                } else if lower.ends_with(".png")
                    || lower.ends_with(".jpg")
                    || lower.ends_with(".webp")
                    || lower.ends_with(".gif")
                    || lower.ends_with(".svg")
                {
                    "image"
                } else if lower.ends_with(".css") {
                    "stylesheet"
                } else if lower.ends_with(".woff2")
                    || lower.ends_with(".woff")
                    || lower.ends_with(".ttf")
                {
                    "font"
                } else if lower.contains("/ping") || lower.contains("/tr") || lower.contains("/collect") {
                    "ping"
                } else {
                    "other"
                };

                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);

                let decision = adblock.check_request(&uri, &uri, resource_type, &method, now_ms);

                match decision {
                    FilterDecision::Block => {
                        shield.log_blocked_request(&uri);
                        Some(ResponseHandler::blocked_response())
                    }
                    FilterDecision::Allow | FilterDecision::Whitelisted => None,
                }
            })
            .map_err(|e| RustBrowseError::Engine(format!("Failed to register on_web_resource_requested: {:?}", e)))
    }
}
