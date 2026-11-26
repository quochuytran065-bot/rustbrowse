//! Unlimited Multi-Webview Tab Manager, Lazy Session Restore, Closed-Tab Stack & Crash Recovery
//! Integrated with ActiveShield, Watchdog real RAM monitoring, VideoDownloadManager, and SQLite session persistence.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, WebviewBuilder, WebviewUrl, Window};
use tracing::{info, warn};

use crate::shield::ActiveShield;
use crate::storage::SessionStorage;
use crate::watchdog::{TabTrackInput, Watchdog};

pub const TOPBAR_HEIGHT: f64 = 74.0;
const MAX_CLOSED_STACK: usize = 50;

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

#[derive(Debug, Clone)]
struct ClosedTab {
    url: String,
    title: String,
}

pub struct TabManager {
    next_id: AtomicU32,
    active_tab_id: Mutex<Option<u32>>,
    tabs: Mutex<HashMap<u32, TabInfo>>,
    last_active: Mutex<HashMap<u32, Instant>>,
    closed_stack: Mutex<Vec<ClosedTab>>,
    spare_tab: Mutex<Option<(u32, String)>>,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1),
            active_tab_id: Mutex::new(None),
            tabs: Mutex::new(HashMap::new()),
            last_active: Mutex::new(HashMap::new()),
            closed_stack: Mutex::new(Vec::new()),
            spare_tab: Mutex::new(None),
        }
    }

    pub fn next_tab_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
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

    fn friendly_title(url: &str) -> String {
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
        } else if let Ok(parsed) = url::Url::parse(url) {
            parsed.host_str().unwrap_or(url).to_string()
        } else {
            url.to_string()
        }
    }

    /// Builds a child Webview with ActiveShield scripts, target=_blank/window.open interception,
    /// live title/URL updates, and WebView2 ProcessFailed crash recovery.
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

        let parsed = url::Url::parse(url_str)
            .unwrap_or_else(|_| url::Url::parse("rustbrowse://home").unwrap());

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

        // Hook WebView2 ProcessFailed crash recovery & capture initial PID on Windows
        Self::attach_crash_recovery_and_pid(&webview, window.app_handle().clone(), tab_id);

        Ok(webview)
    }

    /// Attaches WebView2 `ProcessFailed` handler to isolate and auto-recover crashed tabs without crashing the app.
    #[cfg(windows)]
    fn attach_crash_recovery_and_pid(webview: &tauri::Webview, app: AppHandle, tab_id: u32) {
        let app_for_pid = app.clone();
        let _ = webview.with_webview(move |platform_wv| {
            use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PROCESS_FAILED_KIND;
            use webview2_com::ProcessFailedEventHandler;

            unsafe {
                let controller = platform_wv.controller();
                if let Ok(core) = controller.CoreWebView2() {
                    // Capture WebView2 browser/host PID as initial process group anchor
                    let mut pid: u32 = 0;
                    if core.BrowserProcessId(&mut pid).is_ok() && pid > 0 {
                        if let Some(tm) = app_for_pid.try_state::<TabManager>() {
                            tm.set_tab_pid(tab_id, pid);
                        }
                    }

                    let app_crash = app.clone();
                    let handler = ProcessFailedEventHandler::create(Box::new(
                        move |_sender, args| {
                            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(0);
                            if let Some(a) = args {
                                let _ = a.ProcessFailedKind(&mut kind);
                            }
                            warn!(
                                tab_id = tab_id,
                                failure_kind = kind.0,
                                "WebView2 ProcessFailed caught — isolating & auto-recovering tab."
                            );

                            let app_recover = app_crash.clone();
                            tauri::async_runtime::spawn(async move {
                                // Notify Topbar Dynamic Island
                                if let Some(main_wv) = app_recover.get_webview_window("main") {
                                    let _ = main_wv.emit(
                                        "island-event",
                                        serde_json::json!({
                                            "kind": "crash-recovered",
                                            "title": "Đã khôi phục Tab",
                                            "detail": format!("Tab #{} gặp sự cố renderer và đã tự động tải lại an toàn", tab_id)
                                        }),
                                    );
                                }
                                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                                if let Some(main_win) = app_recover.get_window("main") {
                                    if let Some(tm) = app_recover.try_state::<TabManager>() {
                                        tm.recover_crashed_tab(&main_win, tab_id);
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

    /// Auto-recovers a tab whose renderer process crashed by recreating its child Webview cleanly.
    pub fn recover_crashed_tab(&self, window: &Window, tab_id: u32) {
        let tab_opt = {
            let tabs = self.tabs.lock().unwrap();
            tabs.get(&tab_id).cloned()
        };
        let Some(tab) = tab_opt else { return };

        let is_active = self.active_id() == Some(tab_id);
        if let Some(old_wv) = window.get_webview(&tab.label) {
            let _ = old_wv.close();
        }

        let new_label = format!("tab-{}-r{}", tab_id, self.next_tab_id());
        if let Ok(new_wv) =
            self.spawn_child_webview(window, tab_id, &new_label, &tab.url, is_active)
        {
            if is_active {
                let _ = new_wv.set_focus();
            }
            if let Some(entry) = self.tabs.lock().unwrap().get_mut(&tab_id) {
                entry.label = new_label;
                entry.is_sleeping = false;
            }
            info!(tab_id, url = %tab.url, "Crashed tab recovered cleanly.");
            self.sync_to_topbar(window.app_handle());
        }
    }

    pub fn set_tab_pid(&self, tab_id: u32, pid: u32) {
        if let Some(tab) = self.tabs.lock().unwrap().get_mut(&tab_id) {
            tab.pid = pid;
            if tab.process_group.is_empty() || tab.process_group == "lazy" {
                tab.process_group = format!("wv2-{}", pid);
            }
        }
    }

    fn update_tab_title(&self, tab_id: u32, title: String, app: &AppHandle) {
        let changed = {
            let mut tabs = self.tabs.lock().unwrap();
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
            let mut tabs = self.tabs.lock().unwrap();
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

    /// Creates and attaches the initial tab (eager) + restores remaining saved tabs lazily (0 RAM until clicked).
    pub fn setup_initial_tabs(&self, window: &Window) -> Result<(), Box<dyn std::error::Error>> {
        let saved_session = SessionStorage::load_session();

        let (initial_url, initial_title) = if let Some((url, title)) = saved_session.first() {
            (url.clone(), title.clone())
        } else {
            ("rustbrowse://home".to_string(), "Trang chủ".to_string())
        };

        // 1. Eagerly create Tab 1
        let tab1_id = self.next_tab_id();
        let tab1_label = format!("tab-{}", tab1_id);
        let _webview =
            self.spawn_child_webview(window, tab1_id, &tab1_label, &initial_url, true)?;

        let tab1_info = TabInfo {
            id: tab1_id,
            label: tab1_label,
            title: initial_title,
            url: initial_url,
            is_playing_audio: false,
            is_muted: false,
            ram_mb: 0,
            is_sleeping: false,
            pid: 0,
            process_group: "active".to_string(),
        };

        self.tabs.lock().unwrap().insert(tab1_id, tab1_info);
        self.last_active
            .lock()
            .unwrap()
            .insert(tab1_id, Instant::now());
        *self.active_tab_id.lock().unwrap() = Some(tab1_id);

        // 2. Lazy-restore remaining saved tabs without spawning WebViews (0 RAM penalty)
        if saved_session.len() > 1 {
            let mut tabs = self.tabs.lock().unwrap();
            let mut last_active = self.last_active.lock().unwrap();
            for (url, title) in saved_session.iter().skip(1) {
                let id = self.next_tab_id();
                let label = format!("tab-{}", id);
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

        // 3. Pre-warm spare tab in background (about:blank)
        self.prewarm_spare_tab(window);

        self.sync_to_topbar(window.app_handle());
        Ok(())
    }

    /// Spawns a background spare tab (about:blank) with 0ms promotion readiness.
    pub fn prewarm_spare_tab(&self, window: &Window) {
        if self.spare_tab.lock().unwrap().is_some() {
            return;
        }
        let spare_id = self.next_tab_id();
        let spare_label = format!("tab-{}", spare_id);

        if self
            .spawn_child_webview(window, spare_id, &spare_label, "about:blank", false)
            .is_ok()
        {
            *self.spare_tab.lock().unwrap() = Some((spare_id, spare_label));
            info!(spare_id, "Pre-warmed spare tab ready in background.");
        }
    }

    /// Opens a new tab immediately (unlimited tabs, 0ms latency via pre-warmed pool).
    pub fn create_tab(
        &self,
        window: &Window,
        raw_url: Option<String>,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let url = raw_url
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "rustbrowse://home".to_string());
        let (pos, sz) = Self::content_bounds(window);

        let prev_active_label = {
            let active_id = self.active_tab_id.lock().unwrap();
            active_id.and_then(|id| self.tabs.lock().unwrap().get(&id).map(|t| t.label.clone()))
        };

        // 1. Promote spare tab if available, else build on-demand
        let spare_opt = self.spare_tab.lock().unwrap().take();
        let (new_id, new_label, webview) = if let Some((id, label)) = spare_opt {
            if let Some(wv) = window.get_webview(&label) {
                let _ = wv.set_bounds(tauri::Rect {
                    position: pos.into(),
                    size: sz.into(),
                });
                Self::navigate_webview(&wv, &url);
                (id, label, wv)
            } else {
                let fresh_id = self.next_tab_id();
                let fresh_label = format!("tab-{}", fresh_id);
                let wv = self.spawn_child_webview(window, fresh_id, &fresh_label, &url, false)?;
                (fresh_id, fresh_label, wv)
            }
        } else {
            let id = self.next_tab_id();
            let label = format!("tab-{}", id);
            let wv = self.spawn_child_webview(window, id, &label, &url, false)?;
            (id, label, wv)
        };

        // 2. Show new tab FIRST, then hide previous to prevent any flicker
        let _ = webview.show();
        let _ = webview.set_focus();
        if let Some(prev_label) = prev_active_label {
            if let Some(prev_wv) = window.get_webview(&prev_label) {
                let _ = prev_wv.hide();
            }
        }

        let tab_info = TabInfo {
            id: new_id,
            label: new_label,
            title: Self::friendly_title(&url),
            url: url.clone(),
            is_playing_audio: false,
            is_muted: false,
            ram_mb: 0,
            is_sleeping: false,
            pid: 0,
            process_group: "active".to_string(),
        };

        self.tabs.lock().unwrap().insert(new_id, tab_info);
        self.last_active
            .lock()
            .unwrap()
            .insert(new_id, Instant::now());
        *self.active_tab_id.lock().unwrap() = Some(new_id);

        // 3. Replenish pre-warmed pool in background after 500ms
        let window_clone = window.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            if let Some(tm) = window_clone.app_handle().try_state::<TabManager>() {
                tm.prewarm_spare_tab(&window_clone);
            }
        });

        self.persist_session();
        self.sync_to_topbar(window.app_handle());
        Ok(new_id)
    }

    /// Switches active tab instantly (materializes lazy tabs on-demand if needed).
    pub fn switch_tab(
        &self,
        window: &Window,
        target_id: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut active_lock = self.active_tab_id.lock().unwrap();
        if *active_lock == Some(target_id) {
            return Ok(());
        }

        let (target_label, target_url, old_label) = {
            let mut tabs = self.tabs.lock().unwrap();
            let target_info = tabs.get_mut(&target_id).ok_or("Target tab not found")?;
            target_info.is_sleeping = false;
            let t_label = target_info.label.clone();
            let t_url = target_info.url.clone();
            let o_label = active_lock.and_then(|id| tabs.get(&id).map(|t| t.label.clone()));
            (t_label, t_url, o_label)
        };

        // Show target webview FIRST (or materialize if lazy-restored)
        if let Some(target_wv) = window.get_webview(&target_label) {
            let (pos, sz) = Self::content_bounds(window);
            let _ = target_wv.set_bounds(tauri::Rect {
                position: pos.into(),
                size: sz.into(),
            });
            let _ = target_wv.show();
            let _ = target_wv.set_focus();
        } else {
            let wv =
                self.spawn_child_webview(window, target_id, &target_label, &target_url, true)?;
            let _ = wv.set_focus();
        }

        // Hide old webview SECOND
        if let Some(prev_label) = old_label {
            if let Some(old_wv) = window.get_webview(&prev_label) {
                let _ = old_wv.hide();
            }
        }

        *active_lock = Some(target_id);
        drop(active_lock);

        self.last_active
            .lock()
            .unwrap()
            .insert(target_id, Instant::now());
        self.sync_to_topbar(window.app_handle());
        Ok(())
    }

    /// Closes a tab, pushes it to the closed-tab stack for Ctrl+Shift+T, and switches to an adjacent tab.
    pub fn close_tab(
        &self,
        window: &Window,
        target_id: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Clear any detected videos for this tab
        if let Some(vdm) = window
            .app_handle()
            .try_state::<crate::video_downloader::VideoDownloadManager>()
        {
            vdm.clear_tab(target_id);
        }

        let mut tabs = self.tabs.lock().unwrap();
        if tabs.len() <= 1 {
            // Keep at least one tab open, push closed URL to stack if non-home, and navigate to home
            if let Some(t) = tabs.get_mut(&target_id) {
                if t.url != "rustbrowse://home" && t.url != "about:blank" {
                    self.push_closed_tab(t.url.clone(), t.title.clone());
                }
                t.url = "rustbrowse://home".to_string();
                t.title = "Trang chủ".to_string();
                t.is_sleeping = false;
                if let Some(wv) = window.get_webview(&t.label) {
                    Self::navigate_webview(&wv, &t.url);
                }
            }
            drop(tabs);
            self.persist_session();
            self.sync_to_topbar(window.app_handle());
            return Ok(());
        }

        // Sort IDs to pick adjacent neighbor cleanly
        let mut sorted_ids: Vec<u32> = tabs.keys().copied().collect();
        sorted_ids.sort_unstable();
        let pos_idx = sorted_ids.iter().position(|&id| id == target_id);

        let removed = tabs.remove(&target_id);
        self.last_active.lock().unwrap().remove(&target_id);

        if let Some(tab) = removed {
            if tab.url != "about:blank" {
                self.push_closed_tab(tab.url, tab.title);
            }
            if let Some(wv) = window.get_webview(&tab.label) {
                let _ = wv.close();
            }
        }

        let mut active_lock = self.active_tab_id.lock().unwrap();
        let mut need_materialize: Option<(u32, String, String)> = None;

        if *active_lock == Some(target_id) {
            // Pick adjacent tab (prefer right neighbor, else left neighbor)
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
                    if let Some(wv) = window.get_webview(&t.label) {
                        let (pos, sz) = Self::content_bounds(window);
                        let _ = wv.set_bounds(tauri::Rect {
                            position: pos.into(),
                            size: sz.into(),
                        });
                        let _ = wv.show();
                        let _ = wv.set_focus();
                    } else {
                        need_materialize = Some((id, t.label.clone(), t.url.clone()));
                    }
                }
            }
        }

        drop(tabs);
        drop(active_lock);

        if let Some((id, label, url)) = need_materialize {
            if let Ok(wv) = self.spawn_child_webview(window, id, &label, &url, true) {
                let _ = wv.set_focus();
            }
        }

        self.persist_session();
        self.sync_to_topbar(window.app_handle());
        Ok(())
    }

    fn push_closed_tab(&self, url: String, title: String) {
        let mut stack = self.closed_stack.lock().unwrap();
        stack.push(ClosedTab { url, title });
        if stack.len() > MAX_CLOSED_STACK {
            let overflow = stack.len() - MAX_CLOSED_STACK;
            stack.drain(0..overflow);
        }
    }

    /// Reopens the most recently closed tab (Ctrl+Shift+T).
    pub fn reopen_closed_tab(
        &self,
        window: &Window,
    ) -> Result<Option<u32>, Box<dyn std::error::Error>> {
        let popped = self.closed_stack.lock().unwrap().pop();
        if let Some(closed) = popped {
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
        let active_id = self.active_tab_id.lock().unwrap().ok_or("No active tab")?;
        let mut tabs = self.tabs.lock().unwrap();
        let tab = tabs.get_mut(&active_id).ok_or("Active tab info missing")?;

        let cleaned = ActiveShield::clean_url(raw_url.trim());

        let destination = if cleaned.starts_with("http://")
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
        };

        tab.url = destination.clone();
        tab.title = Self::friendly_title(&destination);
        tab.is_sleeping = false;

        let label = tab.label.clone();
        let exists = window.get_webview(&label).is_some();
        if let Some(wv) = window.get_webview(&label) {
            Self::navigate_webview(&wv, &destination);
        }

        drop(tabs);

        if !exists {
            let _ = self.spawn_child_webview(window, active_id, &label, &destination, true);
        }

        self.persist_session();
        self.sync_to_topbar(window.app_handle());
        Ok(())
    }

    /// Puts a background tab into soft sleep (hides it; never sleeps active or audio tabs).
    pub fn sleep_tab(&self, window: &Window, tab_id: u32) {
        let active_id = self.active_id();
        if active_id == Some(tab_id) {
            return;
        }

        let mut tabs = self.tabs.lock().unwrap();
        if let Some(tab) = tabs.get_mut(&tab_id) {
            if tab.is_playing_audio {
                return;
            }
            tab.is_sleeping = true;
            if let Some(wv) = window.get_webview(&tab.label) {
                let _ = wv.hide();
            }
            info!(tab_id = tab_id, "Tab entered soft sleep.");
        }
        drop(tabs);
        self.sync_to_topbar(window.app_handle());
    }

    /// Puts all eligible background tabs into soft sleep and runs safe background trim.
    pub fn sleep_all_tabs(&self, window: &Window) {
        let active_id = self.active_id();
        let mut tabs = self.tabs.lock().unwrap();
        for (id, tab) in tabs.iter_mut() {
            if Some(*id) != active_id && !tab.is_playing_audio {
                tab.is_sleeping = true;
                if let Some(wv) = window.get_webview(&tab.label) {
                    let _ = wv.hide();
                }
            }
        }
        drop(tabs);
        if let Some(wd) = window.app_handle().try_state::<Arc<Watchdog>>() {
            let _ = wd.clean_all_background_memory();
        }
        self.sync_to_topbar(window.app_handle());
    }

    pub fn toggle_tab_mute(&self, window: &Window, tab_id: u32) {
        let mut tabs = self.tabs.lock().unwrap();
        if let Some(tab) = tabs.get_mut(&tab_id) {
            tab.is_muted = !tab.is_muted;
            let mute_state = tab.is_muted;
            if let Some(wv) = window.get_webview(&tab.label) {
                let js = format!(
                    "document.querySelectorAll('video, audio').forEach(el => el.muted = {});",
                    mute_state
                );
                let _ = wv.eval(&js);
            }
        }
        drop(tabs);
        self.sync_to_topbar(window.app_handle());
    }

    /// Evaluates a JS snippet across all currently materialized content tab webviews.
    pub fn eval_all_tabs(&self, window: &Window, js: &str) {
        let labels: Vec<String> = self
            .tabs
            .lock()
            .unwrap()
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
        let last_active_map = self.last_active.lock().unwrap().clone();

        let track_inputs: Vec<TabTrackInput> = {
            let tabs = self.tabs.lock().unwrap();
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

        // Pull back real per-tab memory info from Watchdog's cached report
        {
            let mut tabs = self.tabs.lock().unwrap();
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
        self.tabs
            .lock()
            .unwrap()
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

    /// Syncs state to Topbar UI via JS evaluation.
    pub fn sync_to_topbar(&self, app: &AppHandle) {
        if let Some(window) = app.get_webview_window("main") {
            let tabs_json =
                serde_json::to_string(&self.list_tabs()).unwrap_or_else(|_| "[]".to_string());
            let active_str = self
                .active_id()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "null".to_string());
            let ram_mb = if let Some(wd) = app.try_state::<Arc<Watchdog>>() {
                let report = wd.get_memory_report();
                if report.total_private_mb > 0 {
                    report.total_private_mb
                } else {
                    report.total_working_set_mb
                }
            } else {
                Watchdog::total_browser_ram_mb()
            };
            let js = format!(
                "if (window.updateTabs) window.updateTabs({}, {}, {});",
                tabs_json, active_str, ram_mb
            );
            let _ = window.eval(&js);
        }
    }

    pub fn list_tabs(&self) -> Vec<TabInfo> {
        let tabs = self.tabs.lock().unwrap();
        let mut list: Vec<TabInfo> = tabs.values().cloned().collect();
        list.sort_by_key(|t| t.id);
        list
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.lock().unwrap().len()
    }

    pub fn active_id(&self) -> Option<u32> {
        *self.active_tab_id.lock().unwrap()
    }
}
