#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod internal_pages;
mod optimizer;
mod overlay;
mod shield;
mod storage;
mod tabs;
mod video_downloader;
mod watchdog;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

use shield::{ActiveShield, ShieldStats};
use tabs::{CrashEvent, TabInfo, TabManager};
use video_downloader::VideoDownloadManager;
use watchdog::{AutoCleanLevel, ProcessMemoryReport, Watchdog};

pub const BROWSER_FLAGS: &str = "--use-angle=d3d11 --enable-gpu-rasterization --enable-zero-copy --enable-oop-rasterization --ignore-gpu-blocklist --enable-smooth-scrolling --force-color-profile=srgb --js-flags=\"--optimize-for-size\" --disable-features=BackForwardCache,Prerender2,OptimizationHints,MediaRouter,PreloadMediaEngagementData,InterestFeedContentSuggestions,AutofillServerCommunication,Translate --enable-features=IntensiveWakeUpThrottling,AsyncDns,ParallelDownloading --enable-quic --disable-background-networking --disable-component-update --disable-sync --disable-metrics --disable-default-apps --no-first-run --disk-cache-size=67108864 --media-cache-size=33554432";

pub struct StartupMetrics {
    pub boot_instant: Instant,
    pub t_shell_ready_ms: AtomicU64,
    pub t_tab1_ready_ms: AtomicU64,
}

impl StartupMetrics {
    pub fn new(boot_instant: Instant) -> Self {
        Self {
            boot_instant,
            t_shell_ready_ms: AtomicU64::new(0),
            t_tab1_ready_ms: AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub current_version: String,
    pub body: Option<String>,
    pub configured: bool,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub version: String,
    pub t_shell_ready_ms: u64,
    pub t_tab1_ready_ms: u64,
    pub uptime_secs: u64,
    pub tab_count: usize,
    pub crash_count: usize,
    pub crashes: Vec<CrashEvent>,
    pub crash_log_path: String,
    pub memory_report: ProcessMemoryReport,
}

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
// RAM Watchdog, Health Diagnostics, Shield & Super Boost Commands
// ============================================================================

#[tauri::command]
fn set_perf_panel_open(watchdog: State<'_, Arc<Watchdog>>, open: bool) -> Result<bool, String> {
    watchdog.set_perf_panel_open(open);
    Ok(watchdog.is_perf_panel_open())
}

#[tauri::command]
fn get_health_report(
    app: AppHandle,
    startup: State<'_, StartupMetrics>,
    tab_manager: State<'_, TabManager>,
    watchdog: State<'_, Arc<Watchdog>>,
) -> HealthReport {
    HealthReport {
        version: app.package_info().version.to_string(),
        t_shell_ready_ms: startup.t_shell_ready_ms.load(Ordering::Relaxed),
        t_tab1_ready_ms: startup.t_tab1_ready_ms.load(Ordering::Relaxed),
        uptime_secs: startup.boot_instant.elapsed().as_secs(),
        tab_count: tab_manager.tab_count(),
        crash_count: tab_manager.crash_count(),
        crashes: tab_manager.list_crashes(),
        crash_log_path: tabs::crash_log_path().to_string_lossy().to_string(),
        memory_report: watchdog.get_memory_report(),
    }
}

#[tauri::command]
fn export_diagnostics_report(
    app: AppHandle,
    startup: State<'_, StartupMetrics>,
    tab_manager: State<'_, TabManager>,
    watchdog: State<'_, Arc<Watchdog>>,
) -> String {
    let mem = watchdog.get_memory_report();
    let payload = serde_json::json!({
        "app": "RustBrowse",
        "version": app.package_info().version.to_string(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "startup": {
            "tShellReadyMs": startup.t_shell_ready_ms.load(Ordering::Relaxed),
            "tTab1ReadyMs": startup.t_tab1_ready_ms.load(Ordering::Relaxed),
            "uptimeSecs": startup.boot_instant.elapsed().as_secs(),
        },
        "tabs": {
            "count": tab_manager.tab_count(),
        },
        "memory": {
            "totalPrivateMb": mem.total_private_mb,
            "totalWorkingSetMb": mem.total_working_set_mb,
            "browserMb": mem.browser_mb,
            "gpuMb": mem.gpu_mb,
            "rendererMb": mem.renderer_mb,
            "utilityMb": mem.utility_mb,
            "processCount": mem.process_count,
            "rendererCount": mem.renderer_count,
            "totalCpuPercent": mem.total_cpu_percent,
            "systemMemoryLoadPct": mem.system_memory_load_pct,
            "systemTotalRamMb": mem.system_total_ram_mb,
            "systemAvailRamMb": mem.system_avail_ram_mb,
            "memoryPressureLevel": mem.memory_pressure_level,
            "autoCleanLevel": mem.auto_clean_level,
            "totalCleanedMb": mem.total_cleaned_mb,
        },
        "crashes": tab_manager.list_crashes(),
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
}

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
// Auto-Updater Commands (Safe when pubkey/endpoint is unconfigured)
// ============================================================================

async fn run_update_check(app: &AppHandle, emit_island: bool) -> UpdateCheckResult {
    let current_version = app.package_info().version.to_string();

    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            warn!(error = %e, "Auto-updater not configured or unavailable.");
            return UpdateCheckResult {
                available: false,
                version: None,
                current_version,
                body: None,
                configured: false,
                message: format!("Updater chưa cấu hình khóa ký: {}", e),
            };
        }
    };

    match updater.check().await {
        Ok(Some(update)) => {
            let ver = update.version.clone();
            let body = update.body.clone();
            info!(version = %ver, "New RustBrowse update available.");
            if emit_island {
                if let Some(main_wv) = app.get_webview_window("main") {
                    let _ = main_wv.emit(
                        "island-event",
                        serde_json::json!({
                            "kind": "update-available",
                            "title": format!("Bản cập nhật v{}", ver),
                            "detail": "Sẵn sàng cài đặt trong Menu"
                        }),
                    );
                }
            }
            UpdateCheckResult {
                available: true,
                version: Some(ver),
                current_version,
                body,
                configured: true,
                message: "Đã tìm thấy bản cập nhật mới".to_string(),
            }
        }
        Ok(None) => UpdateCheckResult {
            available: false,
            version: None,
            current_version,
            body: None,
            configured: true,
            message: "RustBrowse đang ở phiên bản mới nhất".to_string(),
        },
        Err(e) => {
            warn!(error = %e, "Update check failed.");
            UpdateCheckResult {
                available: false,
                version: None,
                current_version,
                body: None,
                configured: false,
                message: format!("Không thể kiểm tra cập nhật: {}", e),
            }
        }
    }
}

#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Result<UpdateCheckResult, String> {
    Ok(run_update_check(&app, true).await)
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<String, String> {
    let updater = app
        .updater()
        .map_err(|e| format!("Updater chưa cấu hình: {}", e))?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("Kiểm tra cập nhật thất bại: {}", e))?;

    let Some(update) = update else {
        return Ok("Đã ở phiên bản mới nhất".to_string());
    };

    let version = update.version.clone();
    let app_for_progress = app.clone();
    let mut downloaded: usize = 0;

    update
        .download_and_install(
            move |chunk_len, content_len| {
                downloaded += chunk_len;
                let pct = content_len
                    .filter(|&total| total > 0)
                    .map(|total| ((downloaded as f64 / total as f64) * 100.0).min(100.0) as u32)
                    .unwrap_or(0);
                if let Some(main_wv) = app_for_progress.get_webview_window("main") {
                    let _ = main_wv.emit(
                        "island-event",
                        serde_json::json!({
                            "kind": "update-progress",
                            "title": format!("Đang tải v{}", version),
                            "detail": format!("{}%", pct)
                        }),
                    );
                }
            },
            || {
                info!("Update downloaded and ready to install.");
            },
        )
        .await
        .map_err(|e| format!("Tải/cài đặt cập nhật thất bại: {}", e))?;

    app.restart();
}

// ============================================================================
// Application Entrypoint
// ============================================================================

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        tabs::append_crash_log_line(&format!("PANIC at {} :: {}", loc, msg));
        default_hook(info);
    }));
}

fn main() {
    let boot_instant = Instant::now();

    // 1. Install sanitized panic logger to %LOCALAPPDATA%\RustBrowse\User Data\crash.log
    install_panic_hook();

    // 2. Safe Chromium Low-RAM & Hardware Accelerated Flags
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", BROWSER_FLAGS);

    // 3. Initialize tracing
    tracing_subscriber::fmt::init();

    // 4. Initialize lightweight core engines (heavy work runs async/deferred in background)
    let startup_metrics = StartupMetrics::new(boot_instant);
    let active_shield = ActiveShield::new();
    let watchdog = Watchdog::new();
    let video_manager = VideoDownloadManager::new();

    // 5. Build and Run Tauri v2 Application
    tauri::Builder::default()
        .manage(startup_metrics)
        .manage(active_shield)
        .manage(watchdog)
        .manage(video_manager)
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
                .unwrap_or(0);

            let tabs_count = app
                .try_state::<TabManager>()
                .map(|tm| tm.tab_count().max(1))
                .unwrap_or(1);

            let active_downloads = app
                .try_state::<VideoDownloadManager>()
                .map(|v| usize::from(v.has_active_downloads()))
                .unwrap_or(0);

            let version = app.package_info().version.to_string();

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
            } else if uri.contains("settings") {
                internal_pages::InternalPages::render_settings(ram_mb, tabs_count, blocked_count)
            } else if uri.contains("health") {
                internal_pages::InternalPages::render_health(&version, ram_mb, tabs_count)
            } else if uri.contains("updates") {
                internal_pages::InternalPages::render_updates(&version)
            } else {
                internal_pages::InternalPages::render_home(ram_mb, tabs_count, blocked_count)
            };

            match tauri::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Access-Control-Allow-Origin", "*")
                .body(body.into_bytes())
            {
                Ok(response) => responder.respond(response),
                Err(e) => {
                    warn!(error = %e, "Failed to construct rustbrowse:// HTTP response");
                }
            }
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
            // Watchdog, Health Diagnostics, Shield & Super Boost
            set_perf_panel_open,
            get_health_report,
            export_diagnostics_report,
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
            // Auto-Updater
            check_for_updates,
            install_update,
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
            let t_shell_ready_ms = app
                .try_state::<StartupMetrics>()
                .map(|s| {
                    let ms = s.boot_instant.elapsed().as_millis() as u64;
                    s.t_shell_ready_ms.store(ms, Ordering::Relaxed);
                    ms
                })
                .unwrap_or(0);
            info!(
                t_shell_ready_ms = t_shell_ready_ms,
                "RustBrowse Tauri v2 shell window ready."
            );

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

            let t_tab1_ready_ms = app
                .try_state::<StartupMetrics>()
                .map(|s| {
                    let ms = s.boot_instant.elapsed().as_millis() as u64;
                    s.t_tab1_ready_ms.store(ms, Ordering::Relaxed);
                    ms
                })
                .unwrap_or(0);
            info!(
                t_tab1_ready_ms = t_tab1_ready_ms,
                "RustBrowse Tab 1 ready (zero pre-warmed spare overhead)."
            );

            // Handle window resize / move / close to maintain bounds & overlay lifecycle
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
                    overlay::destroy_overlay(window_clone.app_handle());
                }
                _ => {}
            });

            // Defer non-essential startup cache maintenance so boot stays instant
            tauri::async_runtime::spawn(async move {
                optimizer::StartupOptimizer::launch_background_cache_clean();
            });

            // Background 4s live RAM & tab state sync loop (reads O(1) cached snapshot, never blocks UI)
            let app_for_sync = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(4));
                interval.tick().await; // skip immediate tick at t=0
                loop {
                    interval.tick().await;
                    if let Some(tm) = app_for_sync.try_state::<TabManager>() {
                        tm.sync_with_watchdog(&app_for_sync);
                        tm.sync_to_topbar(&app_for_sync);
                    }
                }
            });

            // Deferred auto-update check 8 seconds after boot (graceful if unconfigured/offline)
            let app_for_update = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(8)).await;
                let _ = run_update_check(&app_for_update, true).await;
            });

            info!("RustBrowse Multi-Webview Shell ready (Repair V6).");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
