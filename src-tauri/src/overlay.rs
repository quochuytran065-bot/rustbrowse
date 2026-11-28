//! Floating Transparent Always-On-Top Overlay Window ("overlay")
//! Reuses a single cached WebviewWindow cleanly (never recreates WebView per click)
//! so Topbar popovers, Performance panel, and Ctrl+K Command Palette render above all tab WebViews.

use std::sync::Arc;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder,
    Window, WindowEvent,
};
use tracing::{info, warn};

use crate::watchdog::Watchdog;

pub const OVERLAY_LABEL: &str = "overlay";

fn sync_perf_panel_state(app: &AppHandle, panel: Option<&str>) {
    if let Some(wd) = app.try_state::<Arc<Watchdog>>() {
        let is_perf = matches!(panel, Some("ram") | Some("perf") | Some("performance"));
        wd.set_perf_panel_open(is_perf);
    }
}

/// Opens (or repositions) the single cached floating overlay window relative to the main window.
/// `x`, `y`, `width`, `height` are logical pixels relative to the top-left of the main window's inner area.
/// Supports `"ram"`, `"shield"`, `"videos"`, `"menu"`, `"palette"`, etc.
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

    let safe_w = width.clamp(220.0, (win_logical_w - 16.0).max(220.0));
    let safe_h = height.clamp(120.0, (win_logical_h - 32.0).max(120.0));

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
            "Creating cached floating transparent overlay window."
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

        // Auto-hide overlay when it loses focus and relax Watchdog perf-panel sampling
        let win_for_blur = win.clone();
        let app_for_blur = app.clone();
        win.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                let _ = win_for_blur.hide();
                sync_perf_panel_state(&app_for_blur, None);
            }
        });

        win
    };

    sync_perf_panel_state(app, Some(panel.as_str()));

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

/// Hides the floating overlay window if open and relaxes perf-panel sampling.
pub fn close_overlay(app: &AppHandle) -> Result<(), String> {
    sync_perf_panel_state(app, None);
    if let Some(overlay_win) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = overlay_win.hide();
    }
    Ok(())
}

/// Permanently closes the floating overlay window (called when main window closes).
pub fn destroy_overlay(app: &AppHandle) {
    sync_perf_panel_state(app, None);
    if let Some(overlay_win) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = overlay_win.close();
    }
}
