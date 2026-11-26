#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod internal_pages;
mod optimizer;
mod overlay;
mod shield;
mod storage;
mod tabs;
mod updater_engine;
mod video_downloader;
mod watchdog;

use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, State, WindowEvent};
use tracing::info;

use shield::{ActiveShield, ShieldStats};
use tabs::{TabInfo, TabManager};
use updater_engine::{SuperBoostUpdateEngine, UpdateChannel, UpdateDashboardState};
use video_downloader::VideoDownloadManager;
use watchdog::{AutoCleanLevel, ProcessMemoryReport, Watchdog};

pub const BROWSER_FLAGS: &str = "--use-angle=d3d11 --enable-gpu-rasterization --enable-zero-copy --enable-oop-rasterization --ignore-gpu-blocklist --enable-smooth-scrolling --force-color-profile=srgb --js-flags=\"--optimize-for-size\" --disable-features=BackForwardCache,Prerender2,OptimizationHints,MediaRouter,PreloadMediaEngagementData,InterestFeedContentSuggestions,AutofillServerCommunication,Translate --enable-features=IntensiveWakeUpThrottling,AsyncDns,ParallelDownloading --enable-quic --disable-background-networking --disable-component-update --disable-sync --disable-metrics --disable-default-apps --no-first-run --disk-cache-size=67108864 --media-cache-size=33554432";

fn get_main_window(app: &AppHandle) -> Result<tauri::Window, String> {
    app.get_window("main")
        .ok_or_else(|| "Main browser window not found".to_string())
}

// ============================================================================
// Tab Lifecycle Commands
// ============================================================================

#[tauri::command]
fn create_tab(
    app: AppHandle,
    tab_manager: State<'_, TabManager>,
    url: Option<String>,
) -> Result<u32, String> {
    let window = get_main_window(&app)?;
    tab_manager
        .create_tab(&window, url)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn switch_tab(
    app: AppHandle,
    tab_manager: State<'_, TabManager>,
    tab_id: u32,
) -> Result<(), String> {
    let window = get_main_window(&app)?;
    tab_manager
        .switch_tab(&window, tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn close_tab(
    app: AppHandle,
    tab_manager: State<'_, TabManager>,
    tab_id: u32,
) -> Result<(), String> {
    let window = get_main_window(&app)?;
    tab_manager
        .close_tab(&window, tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reopen_closed_tab(
    app: AppHandle,
    tab_manager: State<'_, TabManager>,
) -> Result<Option<u32>, String> {
    let window = get_main_window(&app)?;
    tab_manager
        .reopen_closed_tab(&window)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_tabs(tab_manager: State<'_, TabManager>) -> Vec<TabInfo> {
    tab_manager.list_tabs()
}

#[tauri::command]
fn navigate(app: AppHandle, tab_manager: State<'_, TabManager>, url: String) -> Result<(), String> {
    let window = get_main_window(&app)?;
    tab_manager
        .navigate_active(&window, &url)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn go_back(app: AppHandle, tab_manager: State<'_, TabManager>) -> Result<(), String> {
    let window = get_main_window(&app)?;
    if let Some(label) = tab_manager.active_label() {
        if let Some(wv) = window.get_webview(&label) {
            let _ = wv.eval("window.history.back()");
        }
    }
    Ok(())
}

#[tauri::command]
fn go_forward(app: AppHandle, tab_manager: State<'_, TabManager>) -> Result<(), String> {
    let window = get_main_window(&app)?;
    if let Some(label) = tab_manager.active_label() {
        if let Some(wv) = window.get_webview(&label) {
            let _ = wv.eval("window.history.forward()");
        }
    }
    Ok(())
}

#[tauri::command]
fn reload(app: AppHandle, tab_manager: State<'_, TabManager>) -> Result<(), String> {
    let window = get_main_window(&app)?;
    if let Some(label) = tab_manager.active_label() {
        if let Some(wv) = window.get_webview(&label) {
            let _ = wv.eval("window.location.reload()");
        }
    }
    Ok(())
}

#[tauri::command]
fn sleep_tab(
    app: AppHandle,
    tab_manager: State<'_, TabManager>,
    tab_id: u32,
) -> Result<(), String> {
    let window = get_main_window(&app)?;
    tab_manager.sleep_tab(&window, tab_id);
    Ok(())
}

#[tauri::command]
fn sleep_all_tabs(app: AppHandle, tab_manager: State<'_, TabManager>) -> Result<(), String> {
    let window = get_main_window(&app)?;
    tab_manager.sleep_all_tabs(&window);
    Ok(())
}

#[tauri::command]
fn toggle_tab_mute(
    app: AppHandle,
    tab_manager: State<'_, TabManager>,
    tab_id: u32,
) -> Result<(), String> {
    let window = get_main_window(&app)?;
    tab_manager.toggle_tab_mute(&window, tab_id);
    Ok(())
}

// ============================================================================
// Floating Overlay Commands
// ============================================================================

#[tauri::command]
fn open_overlay(
    app: AppHandle,
    panel: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let window = get_main_window(&app)?;
    overlay::open_overlay(&app, &window, panel, x, y, width, height)
}

#[tauri::command]
fn close_overlay(app: AppHandle) -> Result<(), String> {
    overlay::close_overlay(&app)
}

// ============================================================================
// RAM Watchdog, Shield & Super Boost Commands
// ============================================================================

#[tauri::command]
fn clean_ram(
    app: AppHandle,
    watchdog: State<'_, Arc<Watchdog>>,
    tab_manager: State<'_, TabManager>,
) -> Result<u64, String> {
    info!("Manual RAM Clean triggered from Topbar/Overlay");
    let freed_mb = watchdog.clean_all_background_memory();
    tab_manager.sync_with_watchdog(&app);
    tab_manager.sync_to_topbar(&app);
    Ok(freed_mb)
}

#[tauri::command]
fn get_memory_report(watchdog: State<'_, Arc<Watchdog>>) -> ProcessMemoryReport {
    watchdog.get_memory_report()
}

#[tauri::command]
fn set_auto_clean_level(
    watchdog: State<'_, Arc<Watchdog>>,
    level: String,
) -> Result<AutoCleanLevel, String> {
    let parsed = AutoCleanLevel::from_str_lossy(&level);
    watchdog.set_auto_clean_level(parsed);
    Ok(parsed)
}

#[tauri::command]
fn set_ram_threshold(threshold_mb: u32) -> Result<(), String> {
    Watchdog::set_per_tab_threshold(threshold_mb as u64);
    Ok(())
}

#[tauri::command]
fn toggle_lite_mode(watchdog: State<'_, Arc<Watchdog>>) -> Result<bool, String> {
    let next = !Watchdog::is_gaming_mode();
    watchdog.set_gaming_mode(next);
    Ok(next)
}

#[tauri::command]
fn toggle_gaming_mode(watchdog: State<'_, Arc<Watchdog>>) -> Result<bool, String> {
    let next = !Watchdog::is_gaming_mode();
    watchdog.set_gaming_mode(next);
    Ok(next)
}

#[tauri::command]
fn get_shield_stats(shield: State<'_, Arc<ActiveShield>>) -> ShieldStats {
    shield.get_stats()
}

#[tauri::command]
fn toggle_shield(shield: State<'_, Arc<ActiveShield>>, enabled: bool) -> Result<bool, String> {
    shield.set_enabled(enabled);
    Ok(shield.is_enabled())
}

#[tauri::command]
fn toggle_super_boost(
    app: AppHandle,
    tab_manager: State<'_, TabManager>,
    enabled: bool,
) -> Result<bool, String> {
    let window = get_main_window(&app)?;
    let js = format!(
        "if (window.__RUSTBROWSE_TOGGLE_POLISH) window.__RUSTBROWSE_TOGGLE_POLISH({});",
        enabled
    );
    tab_manager.eval_all_tabs(&window, &js);
    Ok(enabled)
}

#[tauri::command]
fn toggle_smooth_scroll(
    app: AppHandle,
    tab_manager: State<'_, TabManager>,
    enabled: bool,
) -> Result<bool, String> {
    let window = get_main_window(&app)?;
    let js = format!(
        "if (window.__RUSTBROWSE_TOGGLE_SMOOTH_SCROLL) window.__RUSTBROWSE_TOGGLE_SMOOTH_SCROLL({});",
        enabled
    );
    tab_manager.eval_all_tabs(&window, &js);
    Ok(enabled)
}

#[tauri::command]
fn copy_clean_url(app: AppHandle, tab_manager: State<'_, TabManager>) -> Result<String, String> {
    let window = get_main_window(&app)?;
    if let Some(active_id) = tab_manager.active_id() {
        let tabs = tab_manager.list_tabs();
        if let Some(t) = tabs.iter().find(|item| item.id == active_id) {
            let clean = ActiveShield::clean_url(&t.url);
            if let Some(wv) = window.get_webview(&t.label) {
                let js = format!(
                    "navigator.clipboard.writeText({});",
                    serde_json::to_string(&clean).unwrap_or_else(|_| "\"\"".to_string())
                );
                let _ = wv.eval(&js);
            }
            return Ok(clean);
        }
    }
    Ok(String::new())
}

#[tauri::command]
fn toggle_reader_mode(app: AppHandle, tab_manager: State<'_, TabManager>) -> Result<(), String> {
    let window = get_main_window(&app)?;
    if let Some(label) = tab_manager.active_label() {
        if let Some(wv) = window.get_webview(&label) {
            let _ = wv.eval(
                "if (window.__RUSTBROWSE_TOGGLE_READER_MODE) window.__RUSTBROWSE_TOGGLE_READER_MODE();",
            );
        }
    }
    Ok(())
}

#[tauri::command]
fn start_element_zapper(app: AppHandle, tab_manager: State<'_, TabManager>) -> Result<(), String> {
    let window = get_main_window(&app)?;
    if let Some(label) = tab_manager.active_label() {
        if let Some(wv) = window.get_webview(&label) {
            let _ = wv
                .eval("if (window.__RUSTBROWSE_START_ZAPPER) window.__RUSTBROWSE_START_ZAPPER();");
        }
    }
    Ok(())
}

// ============================================================================
// SuperBoost Auto Update Engine Commands
// ============================================================================

#[tauri::command]
fn get_update_dashboard_state(
    engine: State<'_, Arc<SuperBoostUpdateEngine>>,
) -> UpdateDashboardState {
    engine.snapshot()
}

#[tauri::command]
async fn check_for_updates(
    app: AppHandle,
    engine: State<'_, Arc<SuperBoostUpdateEngine>>,
) -> Result<UpdateDashboardState, String> {
    Ok(engine.check_for_updates(&app, true, true).await)
}

#[tauri::command]
async fn download_update_background(
    app: AppHandle,
    engine: State<'_, Arc<SuperBoostUpdateEngine>>,
) -> Result<UpdateDashboardState, String> {
    Ok(engine.download_and_stage(&app).await)
}

#[tauri::command]
async fn install_update(
    app: AppHandle,
    engine: State<'_, Arc<SuperBoostUpdateEngine>>,
    video_mgr: State<'_, VideoDownloadManager>,
    tab_mgr: State<'_, TabManager>,
) -> Result<String, String> {
    let has_downloads = video_mgr.has_active_downloads();
    let has_media = tab_mgr.list_tabs().iter().any(|t| t.is_playing_audio);
    // Persist current open tabs into SQLite session.db before restart
    storage::SessionStorage::save_session(&tab_mgr.list_tabs());
    engine
        .install_and_restart(&app, true, has_downloads, has_media)
        .await
}

#[tauri::command]
fn set_update_channel(
    app: AppHandle,
    engine: State<'_, Arc<SuperBoostUpdateEngine>>,
    channel: String,
) -> Result<UpdateDashboardState, String> {
    let parsed = UpdateChannel::from_str_lossy(&channel);
    Ok(engine.set_channel(&app, parsed))
}

#[tauri::command]
fn set_auto_install_on_exit(
    app: AppHandle,
    engine: State<'_, Arc<SuperBoostUpdateEngine>>,
    enabled: bool,
) -> Result<UpdateDashboardState, String> {
    Ok(engine.set_auto_install_on_exit(&app, enabled))
}

#[tauri::command]
fn set_update_check_interval(
    app: AppHandle,
    engine: State<'_, Arc<SuperBoostUpdateEngine>>,
    minutes: u64,
) -> Result<UpdateDashboardState, String> {
    Ok(engine.set_check_interval_minutes(&app, minutes))
}

// ============================================================================
// Application Entrypoint
// ============================================================================

fn main() {
    // 1. Safe Chromium Low-RAM & Hardware Accelerated Flags (no OOM cap, smooth scrolling enabled)
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", BROWSER_FLAGS);

    // 2. Initialize tracing
    tracing_subscriber::fmt::init();

    // 3. Auto-Strip Cache on Startup in Background
    optimizer::StartupOptimizer::launch_background_cache_clean();

    // 4. Initialize Core Engines
    let active_shield = ActiveShield::new();
    let watchdog = Watchdog::new();
    let video_manager = VideoDownloadManager::new();
    let update_engine = SuperBoostUpdateEngine::new(env!("CARGO_PKG_VERSION").to_string());

    // 5. Build and Run Tauri v2 Application
    tauri::Builder::default()
        .manage(active_shield)
        .manage(watchdog)
        .manage(video_manager)
        .manage(update_engine)
        .register_asynchronous_uri_scheme_protocol("rustbrowse", move |ctx, request, responder| {
            let app = ctx.app_handle();
            let uri = request.uri().to_string();

            let (rules_count, blocked_count) = app
                .try_state::<Arc<ActiveShield>>()
                .map(|s| (s.get_rules_count(), s.get_blocked_ads_count()))
                .unwrap_or((54210, 0));

            let ram_mb = app
                .try_state::<Arc<Watchdog>>()
                .map(|w| {
                    let r = w.get_memory_report();
                    if r.total_private_mb > 0 {
                        r.total_private_mb
                    } else {
                        r.total_working_set_mb
                    }
                })
                .unwrap_or_else(Watchdog::total_browser_ram_mb);

            let tabs_count = app
                .try_state::<TabManager>()
                .map(|tm| tm.tab_count().max(1))
                .unwrap_or(1);

            let active_downloads = app
                .try_state::<VideoDownloadManager>()
                .map(|v| if v.has_active_downloads() { 1 } else { 0 })
                .unwrap_or(0);

            let body = if uri.contains("selftest") {
                internal_pages::InternalPages::render_selftest(rules_count, blocked_count, ram_mb)
            } else if uri.contains("privacy") {
                internal_pages::InternalPages::render_privacy(rules_count, blocked_count, ram_mb)
            } else if uri.contains("performance") {
                internal_pages::InternalPages::render_performance(ram_mb, tabs_count)
            } else if uri.contains("superboost") {
                internal_pages::InternalPages::render_superboost(ram_mb, tabs_count)
            } else if uri.contains("downloads") {
                internal_pages::InternalPages::render_downloads(active_downloads)
            } else if uri.contains("updates") {
                internal_pages::InternalPages::render_updates(env!("CARGO_PKG_VERSION"))
            } else if uri.contains("settings") {
                internal_pages::InternalPages::render_settings(ram_mb, tabs_count, blocked_count)
            } else {
                internal_pages::InternalPages::render_home(ram_mb, tabs_count, blocked_count)
            };

            let response = tauri::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Access-Control-Allow-Origin", "*")
                .body(body.into_bytes())
                .unwrap();
            responder.respond(response);
        })
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            // Tabs
            create_tab,
            switch_tab,
            close_tab,
            reopen_closed_tab,
            list_tabs,
            navigate,
            go_back,
            go_forward,
            reload,
            sleep_tab,
            sleep_all_tabs,
            toggle_tab_mute,
            // Floating Overlay
            open_overlay,
            close_overlay,
            // Watchdog, Shield & Super Boost
            clean_ram,
            get_memory_report,
            set_auto_clean_level,
            set_ram_threshold,
            toggle_lite_mode,
            toggle_gaming_mode,
            get_shield_stats,
            toggle_shield,
            toggle_super_boost,
            toggle_smooth_scroll,
            copy_clean_url,
            toggle_reader_mode,
            start_element_zapper,
            // SuperBoost Auto Update Engine
            get_update_dashboard_state,
            check_for_updates,
            download_update_background,
            install_update,
            set_update_channel,
            set_auto_install_on_exit,
            set_update_check_interval,
            // Video Downloader
            video_downloader::report_detected_videos,
            video_downloader::get_detected_videos,
            video_downloader::clear_tab_videos,
            video_downloader::start_video_download,
            video_downloader::list_downloads,
            video_downloader::pause_download,
            video_downloader::resume_download,
            video_downloader::cancel_download,
            video_downloader::open_download_folder,
            video_downloader::get_yt_dlp_status,
            video_downloader::install_yt_dlp,
            video_downloader::set_sniffer_enabled,
            video_downloader::get_sniffer_enabled,
        ])
        .setup(|app| {
            info!("RustBrowse Tauri v2 initializing shell window.");
            let window = app.get_window("main").ok_or("Failed to get main window")?;

            // Confine Topbar webview to exactly 74px height
            let size = window
                .inner_size()
                .unwrap_or(tauri::PhysicalSize::new(1280, 800));
            let scale = window.scale_factor().unwrap_or(1.0);
            let logical_w = size.width as f64 / scale;

            if let Some(main_wv) = window.get_webview("main") {
                let _ = main_wv.set_bounds(tauri::Rect {
                    position: tauri::LogicalPosition::new(0.0, 0.0).into(),
                    size: tauri::LogicalSize::new(logical_w, tabs::TOPBAR_HEIGHT).into(),
                });
            }

            let tab_mgr = TabManager::new();
            tab_mgr.setup_initial_tabs(&window)?;
            app.manage(tab_mgr);

            // Handle window resize / move / close to maintain bounds, session & overlay lifecycle
            let window_clone = window.clone();
            window.on_window_event(move |event| match event {
                WindowEvent::Resized(_) => {
                    let _ = overlay::close_overlay(window_clone.app_handle());
                    let size = window_clone
                        .inner_size()
                        .unwrap_or(tauri::PhysicalSize::new(1280, 800));
                    let scale = window_clone.scale_factor().unwrap_or(1.0);
                    let logical_w = size.width as f64 / scale;

                    if let Some(main_wv) = window_clone.get_webview("main") {
                        let _ = main_wv.set_bounds(tauri::Rect {
                            position: tauri::LogicalPosition::new(0.0, 0.0).into(),
                            size: tauri::LogicalSize::new(logical_w, tabs::TOPBAR_HEIGHT).into(),
                        });
                    }

                    let (pos, sz) = TabManager::content_bounds(&window_clone);
                    if let Some(tm) = window_clone.app_handle().try_state::<TabManager>() {
                        if let Some(label) = tm.active_label() {
                            if let Some(wv) = window_clone.get_webview(&label) {
                                let _ = wv.set_bounds(tauri::Rect {
                                    position: pos.into(),
                                    size: sz.into(),
                                });
                            }
                        }
                    }
                }
                WindowEvent::Moved(_) => {
                    let _ = overlay::close_overlay(window_clone.app_handle());
                }
                WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed => {
                    let handle = window_clone.app_handle();
                    if let Some(tm) = handle.try_state::<TabManager>() {
                        storage::SessionStorage::save_session(&tm.list_tabs());
                    }
                    if let Some(eng) = handle.try_state::<Arc<SuperBoostUpdateEngine>>() {
                        let busy_dl = handle
                            .try_state::<VideoDownloadManager>()
                            .map(|v| v.has_active_downloads())
                            .unwrap_or(false);
                        if eng.should_install_on_exit() && !busy_dl {
                            info!("Staged update ready on exit; applying safely.");
                            let app_exit = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Some(e) = app_exit.try_state::<Arc<SuperBoostUpdateEngine>>()
                                {
                                    let _ = e.install_and_restart(&app_exit, false, false, false).await;
                                }
                            });
                        }
                    }
                    overlay::destroy_overlay(handle);
                }
                _ => {}
            });

            // Background 4s live RAM & tab state sync loop (never blocks UI thread)
            let app_for_sync = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(4));
                loop {
                    interval.tick().await;
                    if let Some(tm) = app_for_sync.try_state::<TabManager>() {
                        tm.sync_with_watchdog(&app_for_sync);
                        tm.sync_to_topbar(&app_for_sync);
                    }
                }
            });

            // Post-boot 10s crash-loop rollback guard: mark running version healthy once stable
            let app_for_health = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if let Some(eng) = app_for_health.try_state::<Arc<SuperBoostUpdateEngine>>() {
                    eng.mark_boot_healthy();
                }
            });

            // Background startup + periodic auto-update check (non-blocking, respectful interval)
            let app_for_update = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(4)).await;
                loop {
                    let wait_dur = if let Some(eng) =
                        app_for_update.try_state::<Arc<SuperBoostUpdateEngine>>()
                    {
                        let _ = eng.check_for_updates(&app_for_update, true, true).await;
                        eng.check_interval_duration()
                    } else {
                        Duration::from_secs(360 * 60)
                    };
                    tokio::time::sleep(wait_dur).await;
                }
            });

            info!("RustBrowse Multi-Webview Shell ready (unlimited tabs, overlay, SuperBoost updater).");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
