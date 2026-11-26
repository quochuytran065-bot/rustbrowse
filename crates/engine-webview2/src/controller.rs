//! Tab WebView2 Controller wrapper.

use common::errors::{Result, RustBrowseError};
use common::types::Rect;
use windows_webview::{Color, Controller, WebView};

pub struct TabController {
    controller: Controller,
    webview: WebView,
}

impl TabController {
    pub fn new(controller: Controller) -> Result<Self> {
        // Enforce dark background #0d0f14 to eliminate white flash
        let _ = controller.set_default_background_color(Color {
            r: 13,
            g: 15,
            b: 20,
            a: 255,
        });

        let webview = controller.webview().map_err(|e| {
            RustBrowseError::Engine(format!(
                "Failed to acquire WebView from Controller: {:?}",
                e
            ))
        })?;

        Ok(Self {
            controller,
            webview,
        })
    }

    pub fn set_bounds(&self, bounds: Rect) -> Result<()> {
        self.controller
            .set_bounds(bounds.left, bounds.top, bounds.right, bounds.bottom)
            .map_err(|e| RustBrowseError::Engine(format!("SetBounds failed: {:?}", e)))
    }

    pub fn set_visible(&self, visible: bool) -> Result<()> {
        self.controller
            .set_visible(visible)
            .map_err(|e| RustBrowseError::Engine(format!("SetIsVisible failed: {:?}", e)))
    }

    pub fn webview(&self) -> &WebView {
        &self.webview
    }

    pub fn close(&self) -> Result<()> {
        self.controller
            .close()
            .map_err(|e| RustBrowseError::Engine(format!("Close Controller failed: {:?}", e)))
    }
}
