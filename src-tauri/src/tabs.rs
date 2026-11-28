//! Unlimited Multi-Webview Tab Manager, Lazy Session Restore, Closed-Tab Stack & Crash Isolation
//! Every tab has a unique monotonic TabId strictly paired 1:1 with `label = format!("tab-{}", id)`
//! and its own independent child WebView. Zero pre-warmed spare-tab overhead on startup.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, WebviewBuilder, WebviewUrl, Window};
use tracing::{info, warn};

use crate::shield::ActiveShield;
use crate::storage::SessionStorage;
use crate::watchdog::{TabTrackInput, Watchdog};

pub const TOPBAR_HEIGHT: f64 = 74.0;
const MAX_CLOSED_STACK: usize = 50;
const MAX_CRASH_LOG: usize = 50;

#[derive(Debug, Clone, serde::Serialize)]
pub struct TabInfo {
    pub id: u32,
    pub label: String,
    pub title: String,
    pub url: String,
    #[serde(rename = "isPlayingAudio")]
    pub is_playing_audio: bool,
    #[serde(rename = "isMuted")]
    pub is_muted: bool,
    #[serde(rename = "ramMb")]
    pub ram_mb: u32,
    #[serde(rename = "isSleeping")]
    pub is_sleeping: bool,
    pub pid: u32,
    #[serde(rename = "processGroup")]
    pub process_group: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum CrashKind {
    RendererExited,
    RendererUnresponsive,
    GpuProcessExited,
    BrowserProcessExited,
    Other,
}

impl CrashKind {
    pub fn from_webview2_kind(raw: i32) -> Self {
        match raw {
            0 => Self::BrowserProcessExited,
            1 => Self::RendererExited,
            2 => Self::RendererUnresponsive,
            5 => Self::GpuProcessExited,
            _ => Self::Other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RendererExited => "RendererExited",
            Self::RendererUnresponsive => "RendererUnresponsive",
            Self::GpuProcessExited => "GpuProcessExited",
            Self::BrowserProcessExited => "BrowserProcessExited",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashEvent {
    pub timestamp_ms: u64,
    pub tab_id: u32,
    pub kind: CrashKind,
    pub url_domain: String,
    pub recovered: bool,
}

#[derive(Debug, Clone)]
struct ClosedTab {
    url: String,
    title: String,
}

#[derive(Debug, Clone)]
struct SwitchOutcome {
    target_label: String,
    target_url: String,
    old_label: Option<String>,
}

#[derive(Debug, Clone)]
struct CloseOutcome {
    closed_label: Option<String>,
    reset_last_tab: Option<(String, String)>,
    activate_next: Option<(u32, String, String)>,
}

pub struct TabManager {
    next_id: AtomicU32,
    active_tab_id: Mutex<Option<u32>>,
    tabs: Mutex<HashMap<u32, TabInfo>>,
    last_active: Mutex<HashMap<u32, Instant>>,
    closed_stack: Mutex<Vec<ClosedTab>>,
    crash_log: Mutex<VecDeque<CrashEvent>>,
}

fn lock_or_recover<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Returns `%LOCALAPPDATA%\RustBrowse\User Data\crash.log` (creating parent directories if needed).
pub fn crash_log_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Local"))
        })
        .unwrap_or_else(|_| PathBuf::from("."));
    let dir = base.join("RustBrowse").join("User Data");
    let _ = fs::create_dir_all(&dir);
    dir.join("crash.log")
}

/// Appends a sanitized single-line entry to `crash.log` (never includes cookies, passwords, or query tokens).
pub fn append_crash_log_line(line: &str) {
    let path = crash_log_path();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{}] {}", now_epoch_ms(), line);
    }
}

/// Extracts only scheme + host (or internal route) so crash logs never leak query strings, tokens, or credentials.
pub fn sanitize_url_domain(raw: &str) -> String {
    if raw.starts_with("rustbrowse://") {
        return raw.split('?').next().unwrap_or("rustbrowse://").to_string();
    }
    if let Ok(parsed) = url::Url::parse(raw) {
        if let Some(host) = parsed.host_str() {
            return host.to_string();
        }
        return parsed.scheme().to_string();
    }
    "unknown".to_string()
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1),
            active_tab_id: Mutex::new(None),
            tabs: Mutex::new(HashMap::new()),
            last_active: Mutex::new(HashMap::new()),
            closed_stack: Mutex::new(Vec::new()),
            crash_log: Mutex::new(VecDeque::with_capacity(MAX_CRASH_LOG)),
        }
    }

    pub fn next_tab_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn tab_label(id: u32) -> String {
        format!("tab-{}", id)
    }

    /// Computes the viewport bounds for content tabs (below topbar).
    pub fn content_bounds(
        window: &Window,
    ) -> (tauri::LogicalPosition<f64>, tauri::LogicalSize<f64>) {
        let size = window
            .inner_size()
            .unwrap_or(tauri::PhysicalSize::new(1280, 800));
        let scale = window.scale_factor().unwrap_or(1.0);
        let logical_w = size.width as f64 / scale;
        let logical_h = size.height as f64 / scale;
        let pos = tauri::LogicalPosition::new(0.0, TOPBAR_HEIGHT);
        let sz = tauri::LogicalSize::new(logical_w, (logical_h - TOPBAR_HEIGHT).max(100.0));
        (pos, sz)
    }

    pub fn friendly_title(url: &str) -> String {
        if url.starts_with("rustbrowse://home") {
            "Trang chủ".to_string()
        } else if url.starts_with("rustbrowse://selftest") {
            "Diagnostic Suite".to_string()
        } else if url.starts_with("rustbrowse://privacy") {
            "Bảo mật".to_string()
        } else if url.starts_with("rustbrowse://performance") {
            "Hiệu năng".to_string()
        } else if url.starts_with("rustbrowse://superboost") {
            "Super Boost".to_string()
        } else if url.starts_with("rustbrowse://downloads") {
            "Tải xuống".to_string()
        } else if url.starts_with("rustbrowse://settings") {
            "Cài đặt".to_string()
        } else if url.starts_with("rustbrowse://health") {
            "Sức khỏe hệ thống".to_string()
        } else if url.starts_with("rustbrowse://updates") {
            "Cập nhật".to_string()
        } else if let Ok(parsed) = url::Url::parse(url) {
            parsed.host_str().unwrap_or(url).to_string()
        } else {
            url.to_string()
        }
    }

    pub fn normalize_destination(raw_url: &str) -> String {
        let cleaned = ActiveShield::clean_url(raw_url.trim());
        if cleaned.starts_with("http://")
            || cleaned.starts_with("https://")
            || cleaned.starts_with("rustbrowse://")
            || cleaned.starts_with("file:///")
        {
            cleaned
        } else if cleaned.contains('.') && !cleaned.contains(' ') {
            format!("https://{}", cleaned)
        } else {
            format!(
                "https://www.google.com/search?q={}",
                cleaned.replace(' ', "+")
            )
        }
    }

    // ========================================================================
    // Pure, lock-safe state transitions (tested directly in unit tests)
    // ========================================================================

    /// Allocates a new monotonic tab ID & strictly paired `tab-{id}` label, activates it,
    /// and returns `(new_id, new_label, prev_active_label)` without mutating any existing tab.
    fn open_tab_state(
        &self,
        url: &str,
        custom_title: Option<String>,
    ) -> (u32, String, Option<String>) {
        let new_id = self.next_tab_id();
        let new_label = Self::tab_label(new_id);
        let title = custom_title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| Self::friendly_title(url));

        let tab_info = TabInfo {
            id: new_id,
            label: new_label.clone(),
            title,
            url: url.to_string(),
            is_playing_audio: false,
            is_muted: false,
            ram_mb: 0,
            is_sleeping: false,
            pid: 0,
            process_group: "active".to_string(),
        };

        let mut active_lock = lock_or_recover(&self.active_tab_id);
        let mut tabs = lock_or_recover(&self.tabs);
        let prev_label = active_lock.and_then(|id| tabs.get(&id).map(|t| t.label.clone()));

        tabs.insert(new_id, tab_info);
        *active_lock = Some(new_id);
        drop(tabs);
        drop(active_lock);

        lock_or_recover(&self.last_active).insert(new_id, Instant::now());
        (new_id, new_label, prev_label)
    }

    /// Switches active tab in state only (never mutates target or old URL).
    fn switch_tab_state(&self, target_id: u32) -> Result<Option<SwitchOutcome>, String> {
        let mut active_lock = lock_or_recover(&self.active_tab_id);
        if *active_lock == Some(target_id) {
            return Ok(None);
        }

        let mut tabs = lock_or_recover(&self.tabs);
        let old_label = active_lock.and_then(|id| tabs.get(&id).map(|t| t.label.clone()));
        let target_info = tabs
            .get_mut(&target_id)
            .ok_or_else(|| format!("Target tab {} not found", target_id))?;
        target_info.is_sleeping = false;
        let outcome = SwitchOutcome {
            target_label: target_info.label.clone(),
            target_url: target_info.url.clone(),
            old_label,
        };

        *active_lock = Some(target_id);
        drop(tabs);
        drop(active_lock);

        lock_or_recover(&self.last_active).insert(target_id, Instant::now());
        Ok(Some(outcome))
    }

    /// Closes a tab in state, pushes closed tab to bounded stack, and chooses adjacent neighbor.
    fn close_tab_state(&self, target_id: u32) -> CloseOutcome {
        let mut tabs = lock_or_recover(&self.tabs);
        if tabs.len() <= 1 {
            let mut reset = None;
            if let Some(t) = tabs.get_mut(&target_id) {
                if t.url != "rustbrowse://home" && t.url != "about:blank" {
                    self.push_closed_tab(t.url.clone(), t.title.clone());
                }
                t.url = "rustbrowse://home".to_string();
                t.title = "Trang chủ".to_string();
                t.is_sleeping = false;
                reset = Some((t.label.clone(), t.url.clone()));
            }
            return CloseOutcome {
                closed_label: None,
                reset_last_tab: reset,
                activate_next: None,
            };
        }

        let mut sorted_ids: Vec<u32> = tabs.keys().copied().collect();
        sorted_ids.sort_unstable();
        let pos_idx = sorted_ids.iter().position(|&id| id == target_id);

        let removed = tabs.remove(&target_id);
        let closed_label = removed.as_ref().map(|t| t.label.clone());
        if let Some(tab) = removed {
            if tab.url != "about:blank" {
                self.push_closed_tab(tab.url, tab.title);
            }
        }

        let mut active_lock = lock_or_recover(&self.active_tab_id);
        let mut activate_next = None;
        if *active_lock == Some(target_id) {
            let remaining: Vec<u32> = sorted_ids
                .into_iter()
                .filter(|&id| id != target_id)
                .collect();
            let next_id = if let Some(idx) = pos_idx {
                if idx < remaining.len() {
                    Some(remaining[idx])
                } else {
                    remaining.last().copied()
                }
            } else {
                remaining.last().copied()
            };

            *active_lock = next_id;
            if let Some(id) = next_id {
                if let Some(t) = tabs.get_mut(&id) {
                    t.is_sleeping = false;
                    activate_next = Some((id, t.label.clone(), t.url.clone()));
                }
            }
        }
        drop(tabs);
        drop(active_lock);

        lock_or_recover(&self.last_active).remove(&target_id);
        CloseOutcome {
            closed_label,
            reset_last_tab: None,
            activate_next,
        }
    }

    fn pop_closed_tab(&self) -> Option<ClosedTab> {
        lock_or_recover(&self.closed_stack).pop()
    }

    fn push_closed_tab(&self, url: String, title: String) {
        let mut stack = lock_or_recover(&self.closed_stack);
        stack.push(ClosedTab { url, title });
        if stack.len() > MAX_CLOSED_STACK {
            let overflow = stack.len() - MAX_CLOSED_STACK;
            stack.drain(0..overflow);
        }
    }

    /// Updates active tab URL/title in state and returns `(tab_id, label, normalized_destination)`.
    fn navigate_active_state(&self, raw_url: &str) -> Result<(u32, String, String), String> {
        let active_id =
            lock_or_recover(&self.active_tab_id).ok_or_else(|| "No active tab".to_string())?;
        let destination = Self::normalize_destination(raw_url);
        let mut tabs = lock_or_recover(&self.tabs);
        let tab = tabs
            .get_mut(&active_id)
            .ok_or_else(|| "Active tab info missing".to_string())?;
        tab.url = destination.clone();
        tab.title = Self::friendly_title(&destination);
        tab.is_sleeping = false;
        let label = tab.label.clone();
        Ok((active_id, label, destination))
    }

    /// Records a sanitized crash event into the bounded ring buffer (max 50) and crash.log.
    pub fn record_crash(
        &self,
        tab_id: u32,
        kind: CrashKind,
        raw_url: &str,
        recovered: bool,
        write_file: bool,
    ) -> CrashEvent {
        let domain = sanitize_url_domain(raw_url);
        let event = CrashEvent {
            timestamp_ms: now_epoch_ms(),
            tab_id,
            kind: kind.clone(),
            url_domain: domain.clone(),
            recovered,
        };

        {
            let mut log = lock_or_recover(&self.crash_log);
            if log.len() >= MAX_CRASH_LOG {
                log.pop_front();
            }
            log.push_back(event.clone());
        }

        if write_file {
            append_crash_log_line(&format!(
                "PROCESS_FAILED kind={} tab_id={} domain={} recovered={}",
                kind.as_str(),
                tab_id,
                domain,
                recovered
            ));
        }

        event
    }

    pub fn list_crashes(&self) -> Vec<CrashEvent> {
        lock_or_recover(&self.crash_log).iter().cloned().collect()
    }

    pub fn crash_count(&self) -> usize {
        lock_or_recover(&self.crash_log).len()
    }

    // ========================================================================
    // WebView2 Child Management & Crash Isolation
    // ========================================================================

    /// Builds an independent child Webview with ActiveShield scripts, target=_blank interception,
    /// live title/URL tracking, and WebView2 ProcessFailed crash isolation.
    fn spawn_child_webview(
        &self,
        window: &Window,
        tab_id: u32,
        label: &str,
        url_str: &str,
        show_immediately: bool,
    ) -> Result<tauri::Webview, Box<dyn std::error::Error>> {
        let (pos, sz) = Self::content_bounds(window);
        let composite_js = format!(
            "window.__RUSTBROWSE_TAB_ID = {};\n{}",
            tab_id,
            ActiveShield::composite_injection_script()
        );

        let parsed = url::Url::parse(url_str).or_else(|_| url::Url::parse("rustbrowse://home"))?;

        let app_for_new_win = window.app_handle().clone();
        let app_for_title = window.app_handle().clone();
        let app_for_load = window.app_handle().clone();

        let builder = WebviewBuilder::new(label, WebviewUrl::External(parsed))
            .auto_resize()
            .initialization_script(&composite_js)
            .on_new_window(move |new_url, _features| {
                let target_url = new_url.to_string();
                let app_handle = app_for_new_win.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(main_win) = app_handle.get_window("main") {
                        if let Some(tm) = app_handle.try_state::<TabManager>() {
                            let _ = tm.create_tab(&main_win, Some(target_url));
                        }
                    }
                });
                tauri::webview::NewWindowResponse::Deny
            })
            .on_document_title_changed(move |_wv, new_title| {
                if new_title.is_empty() {
                    return;
                }
                if let Some(tm) = app_for_title.try_state::<TabManager>() {
                    tm.update_tab_title(tab_id, new_title, &app_for_title);
                }
            })
            .on_page_load(move |wv, payload| {
                let current_url = payload.url().to_string();
                if current_url != "about:blank" {
                    if let Some(tm) = app_for_load.try_state::<TabManager>() {
                        tm.update_tab_url(tab_id, current_url, &app_for_load);
                    }
                }
                if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                    let _ = wv.eval(format!("window.__RUSTBROWSE_TAB_ID = {};", tab_id));
                }
            });

        let webview = window.add_child(builder, pos, sz)?;
        if show_immediately {
            let _ = webview.show();
        } else {
            let _ = webview.hide();
        }

        Self::attach_crash_recovery_and_pid(&webview, window.app_handle().clone(), tab_id);

        Ok(webview)
    }

    /// Attaches WebView2 `ProcessFailed` handler to classify, log, isolate, and auto-recover crashed tabs.
    #[cfg(windows)]
    fn attach_crash_recovery_and_pid(webview: &tauri::Webview, app: AppHandle, tab_id: u32) {
        let app_for_pid = app.clone();
        let _ = webview.with_webview(move |platform_wv| {
            use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PROCESS_FAILED_KIND;
            use webview2_com::ProcessFailedEventHandler;

            unsafe {
                let controller = platform_wv.controller();
                if let Ok(core) = controller.CoreWebView2() {
                    let mut pid: u32 = 0;
                    if core.BrowserProcessId(&mut pid).is_ok() && pid > 0 {
                        if let Some(tm) = app_for_pid.try_state::<TabManager>() {
                            tm.set_tab_pid(tab_id, pid);
                        }
                    }

                    let app_crash = app.clone();
                    let handler = ProcessFailedEventHandler::create(Box::new(
                        move |_sender, args| {
                            let mut raw_kind = COREWEBVIEW2_PROCESS_FAILED_KIND(0);
                            if let Some(a) = args {
                                let _ = a.ProcessFailedKind(&mut raw_kind);
                            }
                            let kind = CrashKind::from_webview2_kind(raw_kind.0);
                            warn!(
                                tab_id = tab_id,
                                crash_kind = kind.as_str(),
                                "WebView2 ProcessFailed caught — isolating & auto-recovering tab."
                            );

                            let app_recover = app_crash.clone();
                            tauri::async_runtime::spawn(async move {
                                let should_recreate = matches!(
                                    kind,
                                    CrashKind::RendererExited
                                        | CrashKind::RendererUnresponsive
                                        | CrashKind::BrowserProcessExited
                                        | CrashKind::Other
                                );

                                if let Some(tm) = app_recover.try_state::<TabManager>() {
                                    let tab_url = tm
                                        .get_tab_url(tab_id)
                                        .unwrap_or_else(|| "unknown".to_string());
                                    tm.record_crash(
                                        tab_id,
                                        kind.clone(),
                                        &tab_url,
                                        should_recreate,
                                        true,
                                    );
                                }

                                if let Some(main_wv) = app_recover.get_webview_window("main") {
                                    let _ = main_wv.emit(
                                        "island-event",
                                        serde_json::json!({
                                            "kind": "crash-recovered",
                                            "title": "Đã khôi phục Tab",
                                            "detail": format!(
                                                "Tab #{} ({}) đã được cô lập và khôi phục an toàn",
                                                tab_id,
                                                kind.as_str()
                                            )
                                        }),
                                    );
                                }

                                if should_recreate {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(200))
                                        .await;
                                    if let Some(main_win) = app_recover.get_window("main") {
                                        if let Some(tm) = app_recover.try_state::<TabManager>() {
                                            tm.recover_crashed_tab(&main_win, tab_id);
                                        }
                                    }
                                }
                            });
                            Ok(())
                        },
                    ));

                    let mut token = 0i64;
                    if let Err(e) = core.add_ProcessFailed(&handler, &mut token) {
                        warn!(tab_id, error = ?e, "Failed to register WebView2 ProcessFailed handler");
                    }
                }
            }
        });
    }

    #[cfg(not(windows))]
    fn attach_crash_recovery_and_pid(_webview: &tauri::Webview, _app: AppHandle, _tab_id: u32) {}

    /// Auto-recovers a tab whose renderer process crashed by closing the dead WebView and recreating it.
    pub fn recover_crashed_tab(&self, window: &Window, tab_id: u32) {
        let tab_opt = {
            let tabs = lock_or_recover(&self.tabs);
            tabs.get(&tab_id).cloned()
        };
        let Some(tab) = tab_opt else { return };

        let is_active = self.active_id() == Some(tab_id);
        if let Some(old_wv) = window.get_webview(&tab.label) {
            let _ = old_wv.close();
        }

        // Reuse strict `tab-{id}` label after closing dead webview; fallback if Tauri hasn't released label yet
        let primary_label = Self::tab_label(tab_id);
        let result = self
            .spawn_child_webview(window, tab_id, &primary_label, &tab.url, is_active)
            .map(|wv| (primary_label, wv))
            .or_else(|_| {
                let fallback_label = format!("tab-{}-r{}", tab_id, self.next_tab_id());
                self.spawn_child_webview(window, tab_id, &fallback_label, &tab.url, is_active)
                    .map(|wv| (fallback_label, wv))
            });

        if let Ok((new_label, new_wv)) = result {
            if is_active {
                let _ = new_wv.set_focus();
            }
            if let Some(entry) = lock_or_recover(&self.tabs).get_mut(&tab_id) {
                entry.label = new_label;
                entry.is_sleeping = false;
            }
            info!(tab_id, domain = %sanitize_url_domain(&tab.url), "Crashed tab recovered cleanly.");
            self.sync_to_topbar(window.app_handle());
        }
    }

    pub fn set_tab_pid(&self, tab_id: u32, pid: u32) {
        if let Some(tab) = lock_or_recover(&self.tabs).get_mut(&tab_id) {
            tab.pid = pid;
            if tab.process_group.is_empty() || tab.process_group == "lazy" {
                tab.process_group = format!("wv2-{}", pid);
            }
        }
    }

    pub fn get_tab_url(&self, tab_id: u32) -> Option<String> {
        lock_or_recover(&self.tabs)
            .get(&tab_id)
            .map(|t| t.url.clone())
    }

    fn update_tab_title(&self, tab_id: u32, title: String, app: &AppHandle) {
        let changed = {
            let mut tabs = lock_or_recover(&self.tabs);
            if let Some(tab) = tabs.get_mut(&tab_id) {
                if tab.title != title {
                    tab.title = title;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if changed {
            self.sync_to_topbar(app);
        }
    }

    fn update_tab_url(&self, tab_id: u32, url: String, app: &AppHandle) {
        let changed = {
            let mut tabs = lock_or_recover(&self.tabs);
            if let Some(tab) = tabs.get_mut(&tab_id) {
                if tab.url != url {
                    tab.url = url.clone();
                    if tab.title.is_empty() || tab.title.starts_with("http") {
                        tab.title = Self::friendly_title(&url);
                    }
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if changed {
            self.persist_session();
            self.sync_to_topbar(app);
        }
    }

    /// Creates Tab 1 eagerly (zero extra pre-warmed WebView at boot!) and lazy-restores remaining saved tabs.
    pub fn setup_initial_tabs(&self, window: &Window) -> Result<(), Box<dyn std::error::Error>> {
        let saved_session = SessionStorage::load_session();

        let (initial_url, initial_title) = if let Some((url, title)) = saved_session.first() {
            (url.clone(), title.clone())
        } else {
            ("rustbrowse://home".to_string(), "Trang chủ".to_string())
        };

        // 1. Eagerly create ONLY Tab 1 for fastest possible cold start
        let (tab1_id, tab1_label, _) = self.open_tab_state(&initial_url, Some(initial_title));
        let _webview =
            self.spawn_child_webview(window, tab1_id, &tab1_label, &initial_url, true)?;

        // 2. Lazy-restore remaining saved tabs without spawning WebViews (0 RAM penalty)
        if saved_session.len() > 1 {
            let mut tabs = lock_or_recover(&self.tabs);
            let mut last_active = lock_or_recover(&self.last_active);
            for (url, title) in saved_session.iter().skip(1) {
                let id = self.next_tab_id();
                let label = Self::tab_label(id);
                tabs.insert(
                    id,
                    TabInfo {
                        id,
                        label,
                        title: title.clone(),
                        url: url.clone(),
                        is_playing_audio: false,
                        is_muted: false,
                        ram_mb: 0,
                        is_sleeping: true,
                        pid: 0,
                        process_group: "lazy".to_string(),
                    },
                );
                last_active.insert(id, Instant::now());
            }
            info!(
                lazy_count = saved_session.len() - 1,
                "Lazy-restored saved session tabs with 0 RAM overhead."
            );
        }

        self.sync_to_topbar(window.app_handle());
        Ok(())
    }

    /// Opens a brand-new tab with a unique monotonic ID & independent child WebView.
    /// Hides the previous tab's WebView WITHOUT modifying the previous tab's URL/title/state.
    pub fn create_tab(
        &self,
        window: &Window,
        raw_url: Option<String>,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let url = raw_url
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "rustbrowse://home".to_string());

        let (new_id, new_label, prev_active_label) = self.open_tab_state(&url, None);

        let webview = match self.spawn_child_webview(window, new_id, &new_label, &url, true) {
            Ok(wv) => wv,
            Err(e) => {
                // Rollback state if child WebView creation failed
                lock_or_recover(&self.tabs).remove(&new_id);
                lock_or_recover(&self.last_active).remove(&new_id);
                return Err(e);
            }
        };

        let _ = webview.set_focus();

        // Hide previous tab SECOND to prevent visual flicker (never touch its URL or DOM)
        if let Some(prev_label) = prev_active_label {
            if let Some(prev_wv) = window.get_webview(&prev_label) {
                let _ = prev_wv.hide();
            }
        }

        self.persist_session();
        self.sync_to_topbar(window.app_handle());
        Ok(new_id)
    }

    /// Switches active tab instantly (shows target / materializes if lazy, hides old; NEVER reloads!).
    pub fn switch_tab(
        &self,
        window: &Window,
        target_id: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(outcome) = self.switch_tab_state(target_id)? else {
            return Ok(());
        };

        // Show target webview FIRST (or materialize if lazy-restored)
        if let Some(target_wv) = window.get_webview(&outcome.target_label) {
            let (pos, sz) = Self::content_bounds(window);
            let _ = target_wv.set_bounds(tauri::Rect {
                position: pos.into(),
                size: sz.into(),
            });
            let _ = target_wv.show();
            let _ = target_wv.set_focus();
        } else {
            let wv = self.spawn_child_webview(
                window,
                target_id,
                &outcome.target_label,
                &outcome.target_url,
                true,
            )?;
            let _ = wv.set_focus();
        }

        // Hide old webview SECOND
        if let Some(prev_label) = outcome.old_label {
            if let Some(old_wv) = window.get_webview(&prev_label) {
                let _ = old_wv.hide();
            }
        }

        self.sync_to_topbar(window.app_handle());
        Ok(())
    }

    /// Closes a tab, pushes it to the closed-tab stack for Ctrl+Shift+T, and switches to an adjacent tab.
    pub fn close_tab(
        &self,
        window: &Window,
        target_id: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(vdm) = window
            .app_handle()
            .try_state::<crate::video_downloader::VideoDownloadManager>()
        {
            vdm.clear_tab(target_id);
        }

        let outcome = self.close_tab_state(target_id);

        if let Some((label, home_url)) = outcome.reset_last_tab {
            if let Some(wv) = window.get_webview(&label) {
                Self::navigate_webview(&wv, &home_url);
            }
            self.persist_session();
            self.sync_to_topbar(window.app_handle());
            return Ok(());
        }

        if let Some(closed_label) = outcome.closed_label {
            if let Some(wv) = window.get_webview(&closed_label) {
                let _ = wv.close();
            }
        }

        if let Some((next_id, next_label, next_url)) = outcome.activate_next {
            if let Some(wv) = window.get_webview(&next_label) {
                let (pos, sz) = Self::content_bounds(window);
                let _ = wv.set_bounds(tauri::Rect {
                    position: pos.into(),
                    size: sz.into(),
                });
                let _ = wv.show();
                let _ = wv.set_focus();
            } else if let Ok(wv) =
                self.spawn_child_webview(window, next_id, &next_label, &next_url, true)
            {
                let _ = wv.set_focus();
            }
        }

        self.persist_session();
        self.sync_to_topbar(window.app_handle());
        Ok(())
    }

    /// Reopens the most recently closed tab (Ctrl+Shift+T).
    pub fn reopen_closed_tab(
        &self,
        window: &Window,
    ) -> Result<Option<u32>, Box<dyn std::error::Error>> {
        if let Some(closed) = self.pop_closed_tab() {
            let id = self.create_tab(window, Some(closed.url))?;
            if !closed.title.is_empty() {
                self.update_tab_title(id, closed.title, window.app_handle());
            }
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    /// Navigates the currently active tab.
    pub fn navigate_active(
        &self,
        window: &Window,
        raw_url: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (active_id, label, destination) = self.navigate_active_state(raw_url)?;

        if let Some(wv) = window.get_webview(&label) {
            Self::navigate_webview(&wv, &destination);
        } else {
            let _ = self.spawn_child_webview(window, active_id, &label, &destination, true);
        }

        self.persist_session();
        self.sync_to_topbar(window.app_handle());
        Ok(())
    }

    /// Puts a background tab into soft sleep (hides it; never sleeps active or audio tabs).
    pub fn sleep_tab(&self, window: &Window, tab_id: u32) {
        if self.active_id() == Some(tab_id) {
            return;
        }

        let label_to_hide = {
            let mut tabs = lock_or_recover(&self.tabs);
            tabs.get_mut(&tab_id).and_then(|tab| {
                if tab.is_playing_audio {
                    None
                } else {
                    tab.is_sleeping = true;
                    Some(tab.label.clone())
                }
            })
        };

        if let Some(label) = label_to_hide {
            if let Some(wv) = window.get_webview(&label) {
                let _ = wv.hide();
            }
            info!(tab_id = tab_id, "Tab entered soft sleep.");
            self.sync_to_topbar(window.app_handle());
        }
    }

    /// Puts all eligible background tabs into soft sleep and runs safe background trim.
    pub fn sleep_all_tabs(&self, window: &Window) {
        let active_id = self.active_id();
        let labels_to_hide: Vec<String> = {
            let mut tabs = lock_or_recover(&self.tabs);
            tabs.iter_mut()
                .filter_map(|(id, tab)| {
                    if Some(*id) != active_id && !tab.is_playing_audio {
                        tab.is_sleeping = true;
                        Some(tab.label.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        for label in labels_to_hide {
            if let Some(wv) = window.get_webview(&label) {
                let _ = wv.hide();
            }
        }

        if let Some(wd) = window.app_handle().try_state::<Arc<Watchdog>>() {
            let _ = wd.clean_all_background_memory();
        }
        self.sync_to_topbar(window.app_handle());
    }

    pub fn toggle_tab_mute(&self, window: &Window, tab_id: u32) {
        let target = {
            let mut tabs = lock_or_recover(&self.tabs);
            tabs.get_mut(&tab_id).map(|tab| {
                tab.is_muted = !tab.is_muted;
                (tab.label.clone(), tab.is_muted)
            })
        };

        if let Some((label, mute_state)) = target {
            if let Some(wv) = window.get_webview(&label) {
                let js = format!(
                    "document.querySelectorAll('video, audio').forEach(el => el.muted = {});",
                    mute_state
                );
                let _ = wv.eval(&js);
            }
            self.sync_to_topbar(window.app_handle());
        }
    }

    /// Evaluates a JS snippet across all currently materialized content tab webviews.
    pub fn eval_all_tabs(&self, window: &Window, js: &str) {
        let labels: Vec<String> = lock_or_recover(&self.tabs)
            .values()
            .map(|t| t.label.clone())
            .collect();
        for label in labels {
            if let Some(wv) = window.get_webview(&label) {
                let _ = wv.eval(js);
            }
        }
    }

    /// Feeds current tab states into Watchdog and refreshes real per-tab RAM / PID / processGroup metrics.
    pub fn sync_with_watchdog(&self, app: &AppHandle) {
        let Some(wd) = app.try_state::<Arc<Watchdog>>() else {
            return;
        };

        let is_downloading = app
            .try_state::<crate::video_downloader::VideoDownloadManager>()
            .map(|v| v.has_active_downloads())
            .unwrap_or(false);

        let active_id = self.active_id();
        let now = Instant::now();
        let last_active_map = lock_or_recover(&self.last_active).clone();

        let track_inputs: Vec<TabTrackInput> = {
            let tabs = lock_or_recover(&self.tabs);
            tabs.values()
                .map(|t| {
                    let idle_secs = if Some(t.id) == active_id {
                        0
                    } else {
                        last_active_map
                            .get(&t.id)
                            .map(|inst| now.duration_since(*inst).as_secs())
                            .unwrap_or(0)
                    };
                    TabTrackInput {
                        tab_id: t.id,
                        pid: if t.pid > 0 { Some(t.pid) } else { None },
                        is_active: Some(t.id) == active_id,
                        is_audio_or_video: t.is_playing_audio,
                        is_downloading,
                        is_sleeping: t.is_sleeping,
                        idle_secs,
                    }
                })
                .collect()
        };

        wd.update_tab_states(track_inputs);

        // Pull back real per-tab memory info from Watchdog's O(1) cached report
        {
            let mut tabs = lock_or_recover(&self.tabs);
            for (id, tab) in tabs.iter_mut() {
                if let Some(mem) = wd.get_tab_memory(*id) {
                    tab.ram_mb = mem.ram_mb as u32;
                    if let Some(pid) = mem.pid {
                        if pid > 0 {
                            tab.pid = pid;
                        }
                    }
                    if !mem.process_group.is_empty() {
                        tab.process_group = mem.process_group;
                    }
                }
            }
        }
    }

    /// Returns the label of the currently active tab.
    pub fn active_label(&self) -> Option<String> {
        let active_id = self.active_id()?;
        lock_or_recover(&self.tabs)
            .get(&active_id)
            .map(|t| t.label.clone())
    }

    fn persist_session(&self) {
        let list = self.list_tabs();
        SessionStorage::save_session(&list);
    }

    fn navigate_webview(webview: &tauri::Webview, url: &str) {
        if let Ok(parsed) = url::Url::parse(url) {
            let _ = webview.navigate(parsed);
        }
    }

    /// Syncs state to Topbar UI via JS evaluation using Watchdog's O(1) cached memory report.
    pub fn sync_to_topbar(&self, app: &AppHandle) {
        if let Some(window) = app.get_webview_window("main") {
            let tabs_json =
                serde_json::to_string(&self.list_tabs()).unwrap_or_else(|_| "[]".to_string());
            let active_str = self
                .active_id()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "null".to_string());
            let ram_mb = app
                .try_state::<Arc<Watchdog>>()
                .map(|wd| {
                    let report = wd.get_memory_report();
                    if report.total_private_mb > 0 {
                        report.total_private_mb
                    } else {
                        report.total_working_set_mb
                    }
                })
                .unwrap_or(0);
            let js = format!(
                "if (window.updateTabs) window.updateTabs({}, {}, {});",
                tabs_json, active_str, ram_mb
            );
            let _ = window.eval(&js);
        }
    }

    pub fn list_tabs(&self) -> Vec<TabInfo> {
        let tabs = lock_or_recover(&self.tabs);
        let mut list: Vec<TabInfo> = tabs.values().cloned().collect();
        list.sort_by_key(|t| t.id);
        list
    }

    pub fn tab_count(&self) -> usize {
        lock_or_recover(&self.tabs).len()
    }

    pub fn active_id(&self) -> Option<u32> {
        *lock_or_recover(&self.active_tab_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_1_to_20_tabs_preserves_monotonic_ids_labels_and_old_urls() {
        let mgr = TabManager::new();

        // Tab 1 -> Tab 2 -> Tab 12 -> Tab 20
        for idx in 1..=20u32 {
            let url = format!("https://site{}.example.com/page", idx);
            let (id, label, prev_label) = mgr.open_tab_state(&url, None);
            assert_eq!(id, idx, "Tab IDs must be strictly monotonic");
            assert_eq!(
                label,
                format!("tab-{}", idx),
                "Label must pair 1:1 with TabId"
            );
            if idx == 1 {
                assert!(prev_label.is_none());
            } else {
                assert_eq!(prev_label, Some(format!("tab-{}", idx - 1)));
            }
            assert_eq!(mgr.active_id(), Some(idx));
        }

        let all = mgr.list_tabs();
        assert_eq!(all.len(), 20);
        // Verify every single old tab kept its independent URL intact
        for (i, tab) in all.iter().enumerate() {
            let expected_id = (i + 1) as u32;
            assert_eq!(tab.id, expected_id);
            assert_eq!(tab.label, format!("tab-{}", expected_id));
            assert_eq!(
                tab.url,
                format!("https://site{}.example.com/page", expected_id)
            );
        }
    }

    #[test]
    fn switch_tab_preserves_urls_and_never_reloads_or_overwrites() {
        let mgr = TabManager::new();
        let (id1, _, _) = mgr.open_tab_state("https://google.com", None);
        let (id2, _, _) = mgr.open_tab_state("https://github.com", None);
        assert_eq!(mgr.active_id(), Some(id2));

        // Switch back to Tab 1
        let outcome = mgr
            .switch_tab_state(id1)
            .expect("switch to tab 1")
            .expect("changed");
        assert_eq!(outcome.target_label, format!("tab-{}", id1));
        assert_eq!(outcome.old_label, Some(format!("tab-{}", id2)));
        assert_eq!(mgr.active_id(), Some(id1));

        // Switching to already-active tab is a no-op
        assert!(mgr.switch_tab_state(id1).expect("noop switch").is_none());

        // Both URLs remain completely intact
        assert_eq!(mgr.get_tab_url(id1).as_deref(), Some("https://google.com"));
        assert_eq!(mgr.get_tab_url(id2).as_deref(), Some("https://github.com"));
    }

    #[test]
    fn close_active_middle_and_last_tab_with_reopen_stack() {
        let mgr = TabManager::new();
        let (id1, _, _) = mgr.open_tab_state("https://one.example", Some("One".into()));
        let (id2, _, _) = mgr.open_tab_state("https://two.example", Some("Two".into()));
        let (id3, _, _) = mgr.open_tab_state("https://three.example", Some("Three".into()));
        assert_eq!(mgr.active_id(), Some(id3));

        // Close middle inactive tab (id2) -> active stays id3
        let c_mid = mgr.close_tab_state(id2);
        assert_eq!(c_mid.closed_label, Some(format!("tab-{}", id2)));
        assert!(c_mid.activate_next.is_none());
        assert_eq!(mgr.active_id(), Some(id3));
        assert_eq!(mgr.tab_count(), 2);

        // Close active tab (id3) -> activates remaining neighbor id1
        let c_act = mgr.close_tab_state(id3);
        assert_eq!(c_act.closed_label, Some(format!("tab-{}", id3)));
        assert_eq!(c_act.activate_next.as_ref().map(|x| x.0), Some(id1));
        assert_eq!(mgr.active_id(), Some(id1));
        assert_eq!(mgr.tab_count(), 1);

        // Close last tab (id1) -> resets to rustbrowse://home instead of empty state
        let c_last = mgr.close_tab_state(id1);
        assert!(c_last.closed_label.is_none());
        assert_eq!(
            c_last.reset_last_tab,
            Some((format!("tab-{}", id1), "rustbrowse://home".to_string()))
        );
        assert_eq!(mgr.get_tab_url(id1).as_deref(), Some("rustbrowse://home"));

        // Reopen closed stack is LIFO: id1 -> id3 -> id2
        let r1 = mgr.pop_closed_tab().expect("reopen 1");
        assert_eq!(r1.url, "https://one.example");
        let r2 = mgr.pop_closed_tab().expect("reopen 2");
        assert_eq!(r2.url, "https://three.example");
        let r3 = mgr.pop_closed_tab().expect("reopen 3");
        assert_eq!(r3.url, "https://two.example");
        assert!(mgr.pop_closed_tab().is_none());
    }

    #[test]
    fn crash_ring_buffer_is_bounded_and_sanitizes_urls() {
        let mgr = TabManager::new();
        for i in 0..75u32 {
            mgr.record_crash(
                i,
                CrashKind::RendererExited,
                "https://bank.example.com/login?token=secret123&pwd=abc",
                true,
                false,
            );
        }

        let crashes = mgr.list_crashes();
        assert_eq!(crashes.len(), MAX_CRASH_LOG);
        assert_eq!(crashes[0].tab_id, 25);
        assert_eq!(crashes[MAX_CRASH_LOG - 1].tab_id, 74);
        assert_eq!(crashes[0].url_domain, "bank.example.com");
        assert!(!crashes[0].url_domain.contains("secret123"));
        assert!(!crashes[0].url_domain.contains("pwd"));
    }
}
