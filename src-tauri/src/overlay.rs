//! Floating Transparent Always-On-Top Overlay Window ("overlay")
//! Renders Topbar popovers/dropdowns (RAM, Shield, Videos, Menu) above all child tab WebViews
//! without clipping at the 74px Topbar boundary.

use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder,
    Window, WindowEvent,
};
use tracing::{info, warn};

pub const OVERLAY_LABEL: &str = "overlay";

/// Opens (or repositions) the floating overlay window relative to the main window's client area.
/// `x`, `y`, `width`, `height` are logical pixels relative to the top-left of the main window's inner area.
pub fn open_overlay(
    app: &AppHandle,
    main_window: &Window,
    panel: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let scale = main_window.scale_factor().unwrap_or(1.0);
    let inner_pos = main_window
        .inner_position()
        .unwrap_or(tauri::PhysicalPosition::new(0, 0));
    let inner_size = main_window
        .inner_size()
        .unwrap_or(tauri::PhysicalSize::new(1280, 800));

    let win_logical_x = inner_pos.x as f64 / scale;
    let win_logical_y = inner_pos.y as f64 / scale;
    let win_logical_w = inner_size.width as f64 / scale;
    let win_logical_h = inner_size.height as f64 / scale;

    let safe_w = width.clamp(200.0, (win_logical_w - 16.0).max(200.0));
    let safe_h = height.clamp(120.0, (win_logical_h - 40.0).max(120.0));

    // Clamp relative coordinates inside main window bounds
    let clamped_x = x.clamp(8.0, (win_logical_w - safe_w - 8.0).max(8.0));
    let clamped_y = y.clamp(0.0, (win_logical_h - safe_h - 8.0).max(0.0));

    let screen_x = win_logical_x + clamped_x;
    let screen_y = win_logical_y + clamped_y;

    let overlay_win = if let Some(existing) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = existing.set_size(tauri::Size::Logical(LogicalSize::new(safe_w, safe_h)));
        let _ = existing.set_position(tauri::Position::Logical(LogicalPosition::new(
            screen_x, screen_y,
        )));
        existing
    } else {
        info!(
            panel = %panel,
            x = screen_x,
            y = screen_y,
            width = safe_w,
            height = safe_h,
            "Creating floating transparent overlay window."
        );
        let builder =
            WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
                .title("RustBrowse Overlay")
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .shadow(false)
                .visible(false)
                .inner_size(safe_w, safe_h)
                .position(screen_x, screen_y);

        let win = builder.build().map_err(|e| {
            warn!(error = %e, "Failed to create overlay window");
            e.to_string()
        })?;

        // Auto-hide overlay when it loses focus (clicking outside)
        let win_for_blur = win.clone();
        win.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                let _ = win_for_blur.hide();
            }
        });

        win
    };

    let _ = overlay_win.show();
    let _ = overlay_win.set_focus();

    // Deliver requested panel name via both Tauri event and direct JS callback
    let _ = overlay_win.emit("overlay-panel", &panel);
    let panel_json = serde_json::to_string(&panel).unwrap_or_else(|_| "\"ram\"".to_string());
    let js = format!(
        "window.__RUSTBROWSE_OVERLAY_PANEL = {0}; if (window.setOverlayPanel) window.setOverlayPanel({0});",
        panel_json
    );
    let _ = overlay_win.eval(&js);

    Ok(())
}

/// Hides the floating overlay window if it is currently open.
pub fn close_overlay(app: &AppHandle) -> Result<(), String> {
    if let Some(overlay_win) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = overlay_win.hide();
    }
    Ok(())
}

/// Permanently closes the floating overlay window (called when main window closes).
pub fn destroy_overlay(app: &AppHandle) {
    if let Some(overlay_win) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = overlay_win.close();
    }
}
