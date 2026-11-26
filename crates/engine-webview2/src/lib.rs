//! Engine WebView2 Crate (Clean Architecture Phase 2).
//! Encapsulates all COM interactions, event wiring, and webview manipulation.

mod com_adapter;
mod controller;
mod environment;
mod events;
mod internal_pages;
mod navigation;
mod page_injection;
mod request_interceptor;
mod response_handler;
pub mod shield;
mod suspension;

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use adblock_engine::AdblockEngine;
pub use com_adapter::ComApartmentGuard;
use common::config::BrowserConfig;
use common::engine::Engine;
use common::errors::{Result, RustBrowseError};
use common::messages::EngineEvent;
use common::types::{Rect, TabId};
use controller::TabController;
use environment::WebView2EnvironmentHolder;
use events::TabEventSubscriptions;
use navigation::NavigationHandler;
use page_injection::PageInjection;
use request_interceptor::RequestInterceptor;
pub use shield::ActiveShield;
use suspension::TabSuspensionManager;
use tokio::sync::mpsc::UnboundedSender;
use windows_webview::{EventRegistration, WebView};

pub struct WebView2Engine {
    _com_guard: ComApartmentGuard,
    environment: WebView2EnvironmentHolder,
    controllers: HashMap<TabId, TabController>,
    spare_tab: Option<TabController>,
    _event_subscriptions: HashMap<TabId, TabEventSubscriptions>,
    _interceptor_subscriptions: HashMap<TabId, EventRegistration>,
    guarded_tabs: HashSet<TabId>,
    adblock: Arc<AdblockEngine>,
    shield: Arc<ActiveShield>,
    lite_mode: Arc<AtomicBool>,
    parent_hwnd: *mut c_void,
    event_sender: UnboundedSender<EngineEvent>,
}

impl WebView2Engine {
    pub fn new(
        parent_hwnd: *mut c_void,
        config: &BrowserConfig,
        event_sender: UnboundedSender<EngineEvent>,
    ) -> Result<Self> {
        let com_guard = ComApartmentGuard::new()?;
        let environment = WebView2EnvironmentHolder::create(config)?;
        let cache_path = std::path::Path::new(&config.user_data_folder).join("adblock_cache.bin");
        let adblock = Arc::new(AdblockEngine::load_or_create(cache_path));
        let shield = Arc::new(ActiveShield::new());
        let lite_mode = Arc::new(AtomicBool::new(false));

        let mut engine = Self {
            _com_guard: com_guard,
            environment,
            controllers: HashMap::new(),
            spare_tab: None,
            _event_subscriptions: HashMap::new(),
            _interceptor_subscriptions: HashMap::new(),
            guarded_tabs: HashSet::new(),
            adblock,
            shield,
            lite_mode,
            parent_hwnd,
            event_sender,
        };
        engine.prewarm_spare_tab();
        Ok(engine)
    }

    /// Pre-warms 1 spare hidden WebView in background (about:blank) for 0ms tab creation.
    pub fn prewarm_spare_tab(&mut self) {
        if self.spare_tab.is_some() {
            return;
        }
        let raw_controller = unsafe {
            match self.environment.inner().create_controller_for_hwnd(self.parent_hwnd) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(error = ?e, "Failed to pre-warm spare WebView controller");
                    return;
                }
            }
        };

        match TabController::new(raw_controller) {
            Ok(ctrl) => {
                let _ = ctrl.set_visible(false);
                let _ = ctrl.set_bounds(Rect { left: 0, top: 0, right: 100, bottom: 100 });
                let _ = TabSuspensionManager::set_memory_target(ctrl.webview(), true);
                let _ = NavigationHandler::navigate(ctrl.webview(), "about:blank");
                self.spare_tab = Some(ctrl);
                tracing::info!("Pre-warmed spare Child WebView ready in background pool (0ms ready).");
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to wrap pre-warmed spare controller");
            }
        }
    }

    pub fn adblock(&self) -> &Arc<AdblockEngine> {
        &self.adblock
    }

    pub fn shield(&self) -> &Arc<ActiveShield> {
        &self.shield
    }

    pub fn set_lite_mode(&self, enabled: bool) {
        self.lite_mode.store(enabled, Ordering::Relaxed);
        tracing::info!(enabled, "Lite Mode toggled in WebView2Engine");
    }

    pub fn is_lite_mode(&self) -> bool {
        self.lite_mode.load(Ordering::Relaxed)
    }

    pub fn lite_mode_handle(&self) -> Arc<AtomicBool> {
        self.lite_mode.clone()
    }

    pub fn trigger_soft_gc(&self, id: TabId) {
        if let Some(ctrl) = self.controllers.get(&id) {
            let _ = ctrl.webview().post_web_message_as_string("SOFT_GC");
        }
    }

    pub fn toggle_reader_mode(&self, id: TabId) {
        if let Some(ctrl) = self.controllers.get(&id) {
            let _ = ctrl.webview().post_web_message_as_string("TOGGLE_READER_MODE");
        }
    }

    pub fn start_element_zapper(&self, id: TabId) {
        if let Some(ctrl) = self.controllers.get(&id) {
            let _ = ctrl.webview().post_web_message_as_string("START_ELEMENT_ZAPPER");
        }
    }

    pub fn inject_custom_css(&self, id: TabId, css: &str) {
        if let Some(ctrl) = self.controllers.get(&id) {
            let script = format!(
                r#"(() => {{
                    const s = document.createElement('style');
                    s.textContent = `{}`;
                    (document.head || document.documentElement).appendChild(s);
                }})();"#,
                css.replace('`', "\\`")
            );
            let _ = ctrl.webview().execute_script(&script, |_| {});
        }
    }

    /// Attaches the complete multi-layer security guard to a tab's WebView.
    /// Guaranteed fail-closed: registrations are validated and stored.
    pub fn attach_guard(&mut self, id: TabId, webview: &WebView, initial_url: &str) -> Result<()> {
        // 1. Wire event dispatchers with ActiveShield protection, NavigationStarting, and NewWindowRequested
        let subs = TabEventSubscriptions::wire(
            id,
            webview,
            self.event_sender.clone(),
            self.shield.clone(),
        );
        self._event_subscriptions.insert(id, subs);

        // 2. Attach AdBlock & Lite Mode network interceptor (ICoreWebView2_22 with request source kinds ALL)
        let reg = RequestInterceptor::attach(
            webview,
            self.adblock.clone(),
            self.shield.clone(),
            self.lite_mode.clone(),
        )?;
        self._interceptor_subscriptions.insert(id, reg);

        // 3. Inject scripts on document created for ALL frames and iframes
        // Universal Zero-Tolerance Ad Nuker
        let _ = PageInjection::inject_ad_nuker(webview);
        // Extreme RAM & Anti-Freeze Web Optimizer (H.264, DOM Pruning, Video Pause, GPU Effects)
        let _ = PageInjection::inject_web_optimizer(webview);
        // Catfish ad removal & heuristic scanner
        let _ = PageInjection::inject_ad_sweeper(webview);
        // Global Popup & Click-Jacking Guard
        let _ = PageInjection::inject_popup_guard(webview);
        // Media AdBlock (YouTube & Spotify)
        let _ = PageInjection::inject_media_adblock(webview);
        // Smart Reader Mode
        let _ = PageInjection::inject_reader_mode(webview);
        // Element Zapper
        let _ = PageInjection::inject_element_zapper(webview);
        // Zero Artificial Delay CSS
        let _ = PageInjection::inject_zero_delay(webview);
        // Lenis Smooth Scroll Engine (0% CPU at idle)
        let _ = PageInjection::inject_lenis_scroll(webview);

        // Cosmetic element-hiding CSS
        let host = url::Url::parse(initial_url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_default();
        let _ = PageInjection::inject_cosmetic_filters(webview, &host);

        self.guarded_tabs.insert(id);
        tracing::info!(%id, "Security Guard successfully attached (fail-closed verified).");
        Ok(())
    }
}

impl Engine for WebView2Engine {
    fn create_tab(&mut self, id: TabId, url: &str, bounds: Rect, is_visible: bool) -> Result<()> {
        self._com_guard.assert_on_sta_thread();

        let tab_ctrl = if let Some(ctrl) = self.spare_tab.take() {
            tracing::info!(%id, url, "Claimed pre-warmed spare Child WebView in 0ms!");
            ctrl
        } else {
            tracing::info!(%id, url, "Creating new WebView2 tab controller...");
            let raw_controller = unsafe {
                self.environment
                    .inner()
                    .create_controller_for_hwnd(self.parent_hwnd)
                    .map_err(|e| {
                        RustBrowseError::Engine(format!("create_controller_for_hwnd failed: {:?}", e))
                    })?
            };
            TabController::new(raw_controller)?
        };

        tab_ctrl.set_bounds(bounds)?;
        tab_ctrl.set_visible(is_visible)?;

        if !is_visible {
            TabSuspensionManager::set_memory_target(tab_ctrl.webview(), true)?;
        }

        // Attach complete multi-layer security guard before any navigation
        self.attach_guard(id, tab_ctrl.webview(), url)?;

        if !url.is_empty() {
            NavigationHandler::navigate(tab_ctrl.webview(), url)?;
        }

        self.controllers.insert(id, tab_ctrl);
        let _ = self.event_sender.send(EngineEvent::TabCreated {
            id,
            url: url.to_string(),
        });

        // Replenish pre-warmed spare tab pool in background
        self.prewarm_spare_tab();

        Ok(())
    }

    fn navigate(&mut self, id: TabId, url: &str) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        if !self.guarded_tabs.contains(&id) {
            return Err(RustBrowseError::Engine(format!(
                "CRITICAL: Refusing to navigate tab {}: guard not attached!",
                id
            )));
        }
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        NavigationHandler::navigate(ctrl.webview(), url)
    }

    fn set_tab_bounds(&mut self, id: TabId, bounds: Rect) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        ctrl.set_bounds(bounds)
    }

    fn set_tab_visible(&mut self, id: TabId, visible: bool) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        ctrl.set_visible(visible)
    }

    fn set_tab_memory_target(&mut self, id: TabId, low: bool) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        TabSuspensionManager::set_memory_target(ctrl.webview(), low)
    }

    fn suspend_tab(&mut self, id: TabId) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        TabSuspensionManager::suspend_tab(ctrl.webview())
    }

    fn resume_tab(&mut self, id: TabId) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        TabSuspensionManager::resume_tab(ctrl.webview())
    }

    fn close_tab(&mut self, id: TabId) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        self.guarded_tabs.remove(&id);
        self._event_subscriptions.remove(&id);
        self._interceptor_subscriptions.remove(&id);
        if let Some(ctrl) = self.controllers.remove(&id) {
            ctrl.close()?;
        }
        let _ = self.event_sender.send(EngineEvent::TabDiscarded { id });
        Ok(())
    }

    fn go_back(&mut self, id: TabId) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        NavigationHandler::go_back(ctrl.webview())
    }

    fn go_forward(&mut self, id: TabId) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        NavigationHandler::go_forward(ctrl.webview())
    }

    fn reload(&mut self, id: TabId) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        NavigationHandler::reload(ctrl.webview())
    }

    fn stop(&mut self, id: TabId) -> Result<()> {
        self._com_guard.assert_on_sta_thread();
        let ctrl = self
            .controllers
            .get(&id)
            .ok_or(RustBrowseError::TabNotFound(id.0))?;

        NavigationHandler::stop(ctrl.webview())
    }
}
