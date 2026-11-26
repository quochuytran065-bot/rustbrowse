//! RustBrowse main application entry point (Phase 3 & Advanced Upgrade).

mod bootstrap;
mod lifecycle;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use bootstrap::Bootstrap;
use common::engine::Engine;
use common::errors::Result;
use common::messages::{EngineEvent, TabPolicyAction};
use common::types::{Rect, TabId, TabLifecycleState};
use download_manager::DownloadManager;
use engine_webview2::WebView2Engine;
use lifecycle::AppLifecycle;
use privacy_manager::TrackerFilter;
use profile_manager::ProfileManager;
use resource_manager::{CpuMonitor, ResourceScheduler};
use session_manager::{RecoveryStatus, SessionService};
use shell::{
    AppShortcut, BrowserToolbar, BrowserWindow, ShellUiEvent, ShortcutRouter, TabContextMenu,
    TabContextMenuAction, TabStrip, TabStripHit, ToolbarAction,
};
use storage::DbWorkerHandle;
use tab_manager::TabManager;
use telemetry::BrowserMetrics;
use tokio::sync::mpsc;

const HEADER_HEIGHT: i32 = 72;

#[allow(dead_code)]
struct ShellContext<'a> {
    tab_manager: &'a mut TabManager,
    engine: &'a mut WebView2Engine,
    toolbar: &'a BrowserToolbar,
    window: &'a BrowserWindow,
    tab_strip: &'a Rc<RefCell<TabStrip>>,
    session_service: &'a SessionService,
    scheduler: &'a mut ResourceScheduler,
    cpu_monitor: &'a mut CpuMonitor,
    download_manager: &'a mut DownloadManager,
    profile_manager: &'a mut ProfileManager,
    tracker_filter: &'a mut TrackerFilter,
    metrics: &'a BrowserMetrics,
    db_sender: &'a tokio::sync::mpsc::UnboundedSender<storage::DbCommand>,
}

fn main() -> Result<()> {
    // 1. Bootstrap logging, DPI awareness, and configuration
    let config = Bootstrap::init()?;

    // 1.0 Safe startup cache cleanup (removes Code Cache / GPUCache, preserves cookies & sessions)
    common::StartupOptimizer::safe_startup_cache_clean(&config.user_data_folder);

    // 1.1 Export extreme memory-saving Chromium flags to WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
    config.chromium_flags.apply_to_environment();

    // 1.2 Elevate UI process to ABOVE_NORMAL_PRIORITY_CLASS (Anti-Freeze)
    resource_manager::Watchdog::elevate_ui_priority();

    // 2. Start Embedded SQLite WAL Storage Worker
    let storage_dir = std::path::Path::new(&config.user_data_folder);
    std::fs::create_dir_all(storage_dir).ok();
    let db_path = storage_dir.join("rustbrowse.db");
    let mut db_worker = DbWorkerHandle::start(&db_path)?;
    let db_sender = db_worker.sender();
    let session_service = SessionService::new(db_sender.clone());

    // 3. Prepare TabStrip GDI model and UI action channel
    let tab_strip = Rc::new(RefCell::new(TabStrip::new()));
    let (ui_tx, mut ui_rx) = mpsc::unbounded_channel::<ShellUiEvent>();

    // 4. Create Native Win32 Shell Window
    let window = BrowserWindow::new("RustBrowse", 1280, 800, tab_strip.clone(), ui_tx)?;
    let hwnd = window.hwnd();

    // 5. Create Native Navigation & Memory Toolbar
    let toolbar = BrowserToolbar::new(hwnd, window.client_size().0, tab_strip.borrow().height())?;

    // 6. Initialize TabManager and WebView2 Engine
    let mut tab_manager = TabManager::new(config.memory.clone());
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<EngineEvent>();
    let mut engine = WebView2Engine::new(hwnd, &config, event_tx)?;

    // 7. Initialize Adaptive Resource Scheduler, CPU Monitor, Downloads, Profiles, Privacy & Metrics
    let mut scheduler = ResourceScheduler::new(config.monitor.clone(), config.memory.clone());
    let mut cpu_monitor = CpuMonitor::new();

    let download_dir = storage_dir
        .parent()
        .unwrap_or(storage_dir)
        .join("Downloads");
    std::fs::create_dir_all(&download_dir).ok();
    let mut download_manager = DownloadManager::new(&download_dir);
    let mut profile_manager = ProfileManager::new(&config.user_data_folder);
    let mut tracker_filter = TrackerFilter::new(&[
        "||doubleclick.net^".to_string(),
        "||adservice.google.com^".to_string(),
        "||google-analytics.com^".to_string(),
        "||scorecardresearch.com^".to_string(),
    ]);
    let metrics = BrowserMetrics::new();

    // 8. Check for Crash Recovery or Clean Session Startup
    let initial_bounds = window.compute_webview_bounds(HEADER_HEIGHT);
    let recovery = session_service
        .check_recovery()
        .unwrap_or(RecoveryStatus::CleanStartup);

    match recovery {
        RecoveryStatus::CrashRecovered(snapshot) => {
            tracing::warn!(
                tab_count = snapshot.tabs.len(),
                "Crash recovery active! Restoring tabs from prior session..."
            );
            for (i, tab) in snapshot.tabs.iter().enumerate() {
                let is_active = snapshot.active_tab_id == Some(TabId::new(tab.tab_id))
                    || (snapshot.active_tab_id.is_none() && i == 0);

                let meta = tab_manager.allocate_new_tab(tab.url.clone());
                if tab.is_pinned {
                    tab_manager.store_mut().set_pinned(meta.id, true);
                }

                if is_active {
                    engine.create_tab(meta.id, &tab.url, initial_bounds, is_active)?;
                    tab_manager.activate_tab(meta.id);
                    toolbar.address_bar().set_text(&tab.url);
                } else {
                    // Lazy Session Restore: Background tabs consume 0 MB RAM until explicitly clicked
                    tab_manager
                        .store_mut()
                        .set_state(meta.id, TabLifecycleState::Discarded);
                }
            }
            refresh_tab_strip(&tab_manager, &tab_strip, &window, window.client_size().0);
        }
        RecoveryStatus::CleanStartup => {
            let mut ctx = ShellContext {
                tab_manager: &mut tab_manager,
                engine: &mut engine,
                toolbar: &toolbar,
                window: &window,
                tab_strip: &tab_strip,
                session_service: &session_service,
                scheduler: &mut scheduler,
                cpu_monitor: &mut cpu_monitor,
                download_manager: &mut download_manager,
                profile_manager: &mut profile_manager,
                tracker_filter: &mut tracker_filter,
                metrics: &metrics,
                db_sender: &db_sender,
            };
            // Automatic self-test dashboard on initial startup
            open_new_tab(&mut ctx, "rustbrowse://selftest", initial_bounds)?;
        }
    }

    tracing::info!(url = %config.home_url, "RustBrowse Full Modular Suite initialized.");

    // 9. Native Win32 Event Loop (STA UI Thread)
    let mut last_size = window.client_size();

    while windows_window::pump() {
        let mut ctx = ShellContext {
            tab_manager: &mut tab_manager,
            engine: &mut engine,
            toolbar: &toolbar,
            window: &window,
            tab_strip: &tab_strip,
            session_service: &session_service,
            scheduler: &mut scheduler,
            cpu_monitor: &mut cpu_monitor,
            download_manager: &mut download_manager,
            profile_manager: &mut profile_manager,
            tracker_filter: &mut tracker_filter,
            metrics: &metrics,
            db_sender: &db_sender,
        };

        // A. Handle window resizing
        let current_size = ctx.window.client_size();
        if current_size != last_size {
            last_size = current_size;
            ctx.toolbar
                .layout(current_size.0, ctx.tab_strip.borrow().height());
            refresh_tab_strip(ctx.tab_manager, ctx.tab_strip, ctx.window, current_size.0);

            if let Some(active_id) = ctx.tab_manager.store().active_id() {
                let new_bounds = ctx.window.compute_webview_bounds(HEADER_HEIGHT);
                let _ = ctx.engine.set_tab_bounds(active_id, new_bounds);
            }
        }

        // B. Drain and process Native Shell UI events
        while let Ok(action) = ui_rx.try_recv() {
            let bounds = ctx.window.compute_webview_bounds(HEADER_HEIGHT);
            match action {
                ShellUiEvent::TabClick { x, y } => {
                    let hit = ctx.tab_strip.borrow().hit_test(x, y);
                    match hit {
                        TabStripHit::SelectTab(id) => {
                            let _ = switch_to_tab(&mut ctx, id, bounds);
                        }
                        TabStripHit::CloseTab(id) => {
                            let _ = close_tab_item(&mut ctx, id, bounds, &config.home_url);
                        }
                        TabStripHit::NewTab => {
                            let _ = open_new_tab(&mut ctx, &config.home_url, bounds);
                        }
                        TabStripHit::None => {}
                    }
                }
                ShellUiEvent::TabRightClick {
                    x,
                    y,
                    screen_x,
                    screen_y,
                } => {
                    let hit = ctx.tab_strip.borrow().hit_test(x, y);
                    if let TabStripHit::SelectTab(id) = hit {
                        let is_pinned = ctx
                            .tab_manager
                            .store()
                            .get(id)
                            .map(|t| t.is_pinned)
                            .unwrap_or(false);

                        if let Some(menu_act) =
                            TabContextMenu::show(hwnd, screen_x, screen_y, is_pinned)
                        {
                            match menu_act {
                                TabContextMenuAction::CloseTab => {
                                    let _ = close_tab_item(&mut ctx, id, bounds, &config.home_url);
                                }
                                TabContextMenuAction::CloseOtherTabs => {
                                    let others: Vec<TabId> = ctx
                                        .tab_manager
                                        .store()
                                        .tab_order()
                                        .iter()
                                        .copied()
                                        .filter(|&t| t != id)
                                        .collect();
                                    for other_id in others {
                                        let _ = close_tab_item(
                                            &mut ctx,
                                            other_id,
                                            bounds,
                                            &config.home_url,
                                        );
                                    }
                                }
                                TabContextMenuAction::SleepTab => {
                                    let _ = ctx.engine.set_tab_memory_target(id, true);
                                    let _ = ctx.engine.suspend_tab(id);
                                    ctx.tab_manager
                                        .store_mut()
                                        .set_state(id, TabLifecycleState::Sleeping);
                                    refresh_tab_strip(
                                        ctx.tab_manager,
                                        ctx.tab_strip,
                                        ctx.window,
                                        ctx.window.client_size().0,
                                    );
                                }
                                TabContextMenuAction::DiscardTab => {
                                    let _ = ctx.engine.close_tab(id);
                                    ctx.tab_manager
                                        .store_mut()
                                        .set_state(id, TabLifecycleState::Discarded);
                                    refresh_tab_strip(
                                        ctx.tab_manager,
                                        ctx.tab_strip,
                                        ctx.window,
                                        ctx.window.client_size().0,
                                    );
                                }
                                TabContextMenuAction::TogglePin => {
                                    ctx.tab_manager.store_mut().set_pinned(id, !is_pinned);
                                    refresh_tab_strip(
                                        ctx.tab_manager,
                                        ctx.tab_strip,
                                        ctx.window,
                                        ctx.window.client_size().0,
                                    );
                                }
                            }
                        }
                    }
                }
                ShellUiEvent::ToolbarCommand(cmd_id) => {
                    if let Some(tb_act) = ctx.toolbar.handle_command(cmd_id) {
                        if let Some(active_id) = ctx.tab_manager.store().active_id() {
                            match tb_act {
                                ToolbarAction::Back => {
                                    let _ = ctx.engine.go_back(active_id);
                                }
                                ToolbarAction::Forward => {
                                    let _ = ctx.engine.go_forward(active_id);
                                }
                                ToolbarAction::Reload => {
                                    let _ = ctx.engine.reload(active_id);
                                }
                                ToolbarAction::Home => {
                                    let _ = ctx.engine.navigate(active_id, "rustbrowse://home");
                                }
                                ToolbarAction::Navigate(raw_input) => {
                                    ctx.toolbar.address_bar().set_text(&raw_input);
                                    let _ = ctx.engine.navigate(active_id, &raw_input);
                                }
                                ToolbarAction::ToggleShield => {
                                    let _ = ctx.engine.navigate(active_id, "rustbrowse://privacy");
                                }
                                ToolbarAction::OpenDownloads => {
                                    let _ = ctx.engine.navigate(active_id, "rustbrowse://downloads");
                                }
                                ToolbarAction::OpenPerformance => {
                                    let _ = ctx.engine.navigate(active_id, "rustbrowse://performance");
                                }
                                ToolbarAction::OpenSettings => {
                                    let _ = ctx.engine.navigate(active_id, "rustbrowse://settings");
                                }
                                ToolbarAction::ToggleLiteMode => {
                                    let new_state = !ctx.engine.is_lite_mode();
                                    ctx.engine.set_lite_mode(new_state);
                                    ctx.toolbar.set_lite_mode_active(new_state);
                                    tracing::info!(new_state, "Lite Mode toggled from Topbar");
                                }
                                ToolbarAction::CleanRam => {
                                    let root_pid = std::process::id();
                                    resource_manager::Watchdog::safe_background_trim(root_pid);
                                    let (total_ram, _) = resource_manager::Watchdog::sample_tree_ram_mb();
                                    ctx.toolbar.update_ram_badge(total_ram);
                                    tracing::info!(total_ram, "Clean RAM executed from Topbar");
                                }
                                ToolbarAction::SleepTabs => {
                                    let sleepable = ctx.tab_manager.store().eligible_tabs_for_sleep();
                                    for tab_id in sleepable {
                                        let _ = ctx.engine.set_tab_memory_target(tab_id, true);
                                        let _ = ctx.engine.suspend_tab(tab_id);
                                        ctx.tab_manager
                                            .store_mut()
                                            .set_state(tab_id, TabLifecycleState::Sleeping);
                                    }
                                    refresh_tab_strip(
                                        ctx.tab_manager,
                                        ctx.tab_strip,
                                        ctx.window,
                                        ctx.window.client_size().0,
                                    );
                                    tracing::info!("Sleeping background tabs triggered from Topbar");
                                }
                                ToolbarAction::ToggleGamingMode => {
                                    let current = resource_manager::Watchdog::is_gaming_mode();
                                    let next = !current;
                                    resource_manager::Watchdog::set_gaming_mode(next);
                                    if next {
                                        let sleepable = ctx.tab_manager.store().eligible_tabs_for_sleep();
                                        for tab_id in sleepable {
                                            let _ = ctx.engine.set_tab_memory_target(tab_id, true);
                                            let _ = ctx.engine.suspend_tab(tab_id);
                                            ctx.tab_manager
                                                .store_mut()
                                                .set_state(tab_id, TabLifecycleState::Sleeping);
                                        }
                                        refresh_tab_strip(
                                            ctx.tab_manager,
                                            ctx.tab_strip,
                                            ctx.window,
                                            ctx.window.client_size().0,
                                        );
                                    }
                                    let (total_ram, _) = resource_manager::Watchdog::sample_tree_ram_mb();
                                    ctx.toolbar.update_ram_badge(total_ram);
                                    tracing::info!(gaming_mode = next, "Gaming Mode toggled from Topbar");
                                }
                                ToolbarAction::ToggleReaderMode => {
                                    ctx.engine.toggle_reader_mode(active_id);
                                    tracing::info!(%active_id, "Reader Mode toggled from Topbar");
                                }
                                ToolbarAction::StartElementZapper => {
                                    ctx.engine.start_element_zapper(active_id);
                                    tracing::info!(%active_id, "Element Zapper activated from Topbar");
                                }
                                ToolbarAction::CopyCleanUrl => {
                                    if let Some(tab) = ctx.tab_manager.store().get(active_id) {
                                        let clean = engine_webview2::ActiveShield::clean_url(&tab.url);
                                        set_clipboard_text(&clean);
                                        tracing::info!(clean_url = %clean, "Clean URL copied to clipboard");
                                    }
                                }
                                ToolbarAction::OpenRamInspector => {
                                    let _ = ctx.engine.navigate(active_id, "rustbrowse://performance");
                                }
                            }
                        }
                    }
                }
                ShellUiEvent::KeyDown {
                    key,
                    is_ctrl,
                    is_alt,
                    is_shift,
                } => {
                    if let Some(sc) = ShortcutRouter::match_shortcut(key, is_ctrl, is_alt, is_shift)
                    {
                        match sc {
                            AppShortcut::NewTab => {
                                let _ = open_new_tab(&mut ctx, &config.home_url, bounds);
                            }
                            AppShortcut::CloseTab => {
                                if let Some(active_id) = ctx.tab_manager.store().active_id() {
                                    let _ = close_tab_item(
                                        &mut ctx,
                                        active_id,
                                        bounds,
                                        &config.home_url,
                                    );
                                }
                            }
                            AppShortcut::Reload => {
                                if let Some(active_id) = ctx.tab_manager.store().active_id() {
                                    let _ = ctx.engine.reload(active_id);
                                }
                            }
                            AppShortcut::GoBack => {
                                if let Some(active_id) = ctx.tab_manager.store().active_id() {
                                    let _ = ctx.engine.go_back(active_id);
                                }
                            }
                            AppShortcut::GoForward => {
                                if let Some(active_id) = ctx.tab_manager.store().active_id() {
                                    let _ = ctx.engine.go_forward(active_id);
                                }
                            }
                            AppShortcut::FocusAddressBar => {
                                ctx.toolbar.address_bar().focus_and_select_all();
                            }
                            AppShortcut::NextTab => {
                                if let Some(active_id) = ctx.tab_manager.store().active_id() {
                                    if let Some(next) =
                                        ctx.tab_manager.store().next_tab_id(active_id)
                                    {
                                        let _ = switch_to_tab(&mut ctx, next, bounds);
                                    }
                                }
                            }
                            AppShortcut::PrevTab => {
                                if let Some(active_id) = ctx.tab_manager.store().active_id() {
                                    if let Some(prev) =
                                        ctx.tab_manager.store().prev_tab_id(active_id)
                                    {
                                        let _ = switch_to_tab(&mut ctx, prev, bounds);
                                    }
                                }
                            }
                            AppShortcut::SelectTab(idx) => {
                                if let Some(target) = ctx.tab_manager.store().tab_at_index(idx) {
                                    let _ = switch_to_tab(&mut ctx, target, bounds);
                                }
                            }
                            AppShortcut::SleepBackgroundTabs => {
                                let sleepable = ctx.tab_manager.store().eligible_tabs_for_sleep();
                                for tab_id in sleepable {
                                    let _ = ctx.engine.set_tab_memory_target(tab_id, true);
                                    let _ = ctx.engine.suspend_tab(tab_id);
                                    ctx.tab_manager
                                        .store_mut()
                                        .set_state(tab_id, TabLifecycleState::Sleeping);
                                }
                                refresh_tab_strip(
                                    ctx.tab_manager,
                                    ctx.tab_strip,
                                    ctx.window,
                                    ctx.window.client_size().0,
                                );
                            }
                            AppShortcut::FreeRam => {
                                let discardable =
                                    ctx.tab_manager.store().eligible_tabs_for_discard();
                                for tab_id in discardable {
                                    let _ = ctx.engine.close_tab(tab_id);
                                    ctx.tab_manager
                                        .store_mut()
                                        .set_state(tab_id, TabLifecycleState::Discarded);
                                }
                                refresh_tab_strip(
                                    ctx.tab_manager,
                                    ctx.tab_strip,
                                    ctx.window,
                                    ctx.window.client_size().0,
                                );
                            }
                            AppShortcut::GamingMode => {
                                let current = resource_manager::Watchdog::is_gaming_mode();
                                let next = !current;
                                resource_manager::Watchdog::set_gaming_mode(next);
                                if next {
                                    let sleepable = ctx.tab_manager.store().eligible_tabs_for_sleep();
                                    for tab_id in sleepable {
                                        let _ = ctx.engine.set_tab_memory_target(tab_id, true);
                                        let _ = ctx.engine.suspend_tab(tab_id);
                                        ctx.tab_manager
                                            .store_mut()
                                            .set_state(tab_id, TabLifecycleState::Sleeping);
                                    }
                                    refresh_tab_strip(
                                        ctx.tab_manager,
                                        ctx.tab_strip,
                                        ctx.window,
                                        ctx.window.client_size().0,
                                    );
                                }
                                let (total_ram, _) = resource_manager::Watchdog::sample_tree_ram_mb();
                                ctx.toolbar.update_ram_badge(total_ram);
                                tracing::info!(gaming_mode = next, "Gaming Mode toggled via shortcut Ctrl+Shift+G");
                            }
                            AppShortcut::CommandPalette => {
                                ctx.toolbar.address_bar().focus_and_select_all();
                            }
                            AppShortcut::ReaderMode => {
                                if let Some(active_id) = ctx.tab_manager.store().active_id() {
                                    ctx.engine.toggle_reader_mode(active_id);
                                    tracing::info!(%active_id, "Reader Mode toggled via shortcut Alt+R");
                                }
                            }
                            AppShortcut::ElementZapper => {
                                if let Some(active_id) = ctx.tab_manager.store().active_id() {
                                    ctx.engine.start_element_zapper(active_id);
                                    tracing::info!(%active_id, "Element Zapper activated via shortcut Alt+Z");
                                }
                            }
                        }
                    }
                }
                ShellUiEvent::WindowMinimized => {
                    ctx.scheduler.set_minimized(true);
                    let root_pid = std::process::id();
                    let freed = resource_manager::Watchdog::execute_tier3_clean(root_pid);
                    let (total_ram, _) = resource_manager::Watchdog::sample_tree_ram_mb();
                    ctx.toolbar.update_ram_badge(total_ram);
                    tracing::info!(freed_mb = freed, "Window minimized: Tier 3 trim & EcoQoS executed");
                }
                ShellUiEvent::WindowRestored => {
                    ctx.scheduler.set_minimized(false);
                    if !resource_manager::Watchdog::is_gaming_mode() {
                        if let Some(active_id) = ctx.tab_manager.store().active_id() {
                            let _ = ctx.engine.set_tab_memory_target(active_id, false);
                        }
                    }
                }
                ShellUiEvent::TimerTick => {
                    let active_id = ctx.tab_manager.store().active_id();
                    let all_meta = ctx.tab_manager.all_tab_metadata();
                    if let Some(report) = ctx.scheduler.tick(
                        std::time::Instant::now(),
                        ctx.cpu_monitor,
                        active_id,
                        &all_meta,
                    ) {
                        tracing::debug!(
                            avail_ram_mb = report.snapshot.avail_phys_mb,
                            load_percent = report.snapshot.memory_load_percent,
                            tree_pws_mb = report.snapshot.tree_private_working_set_mb,
                            cpu_percent = format!("{:.1}%", report.host_cpu_percent),
                            pressure = ?report.pressure_tier,
                            "Resource scheduler sample"
                        );

                        ctx.metrics
                            .record_memory_sample(report.snapshot.tree_private_working_set_mb);

                        if report.pressure_changed {
                            tracing::warn!(
                                pressure = ?report.pressure_tier,
                                "Memory pressure tier transitioned"
                            );
                        }

                        let mut changed = false;
                        for action in report.suggested_actions {
                            match action {
                                TabPolicyAction::SetSleeping(tab_id) => {
                                    tracing::info!(%tab_id, "Adaptive policy: putting background tab to sleep");
                                    let _ = ctx.engine.set_tab_memory_target(tab_id, true);
                                    let _ = ctx.engine.suspend_tab(tab_id);
                                    ctx.tab_manager
                                        .store_mut()
                                        .set_state(tab_id, TabLifecycleState::Sleeping);
                                    changed = true;
                                }
                                TabPolicyAction::SetDiscarded(tab_id) => {
                                    tracing::warn!(%tab_id, "Adaptive policy: discarding background tab under memory pressure");
                                    let _ = ctx.engine.close_tab(tab_id);
                                    ctx.tab_manager
                                        .store_mut()
                                        .set_state(tab_id, TabLifecycleState::Discarded);
                                    changed = true;
                                }
                                _ => {}
                            }
                        }

                        // Enforce strict live tab quota (1 active + max 1 sleeping, rest discarded)
                        let quota_actions = ctx.tab_manager.enforce_live_tab_quota(report.pressure_tier);
                        for action in quota_actions {
                            match action {
                                TabPolicyAction::SetSleeping(tab_id) => {
                                    let _ = ctx.engine.suspend_tab(tab_id);
                                    changed = true;
                                }
                                TabPolicyAction::SetDiscarded(tab_id) => {
                                    let _ = ctx.engine.close_tab(tab_id);
                                    changed = true;
                                }
                                _ => {}
                            }
                        }

                        // Update toolbar RAM badge and shield badge with live real-time metrics
                        ctx.toolbar.update_ram_badge(report.snapshot.tree_private_working_set_mb);
                        let total_blocked = ctx.engine.adblock().stats().total_ads_blocked()
                            + ctx.engine.adblock().stats().total_trackers_blocked()
                            + ctx.engine.shield().blocked_count();
                        ctx.toolbar.update_shield_badge(total_blocked);

                        // Smart Per-Tab Soft Trim Watchdog (checks individual background tabs against 300MB budget)
                        // Never trims GPU, main UI, active foreground tab, or tabs playing audio/video
                        let root_pid = std::process::id();
                        if let Some((trimmed_pid, freed_mb)) = resource_manager::Watchdog::scan_and_soft_trim_per_tab(root_pid) {
                            tracing::info!(trimmed_pid, freed_mb, "Smart Per-Tab Trim softly reduced background renderer RAM");
                        }

                        if changed {
                            refresh_tab_strip(
                                ctx.tab_manager,
                                ctx.tab_strip,
                                ctx.window,
                                ctx.window.client_size().0,
                            );
                        }
                    }
                }
            }
        }

        // C. Drain and process WebView2 Engine background events
        while let Ok(event) = event_rx.try_recv() {
            match event {
                EngineEvent::TitleChanged { id, title } => {
                    ctx.tab_manager.store_mut().set_title(id, title);
                    refresh_tab_strip(
                        ctx.tab_manager,
                        ctx.tab_strip,
                        ctx.window,
                        ctx.window.client_size().0,
                    );
                }
                EngineEvent::NavigationCompleted {
                    id,
                    url,
                    is_success,
                } => {
                    ctx.tab_manager.store_mut().set_url(id, url.clone());
                    if ctx.tab_manager.store().active_id() == Some(id) {
                        ctx.toolbar.address_bar().set_text(&url);
                    }
                    if is_success {
                        ctx.metrics.record_page_load();
                        let title = ctx
                            .tab_manager
                            .store()
                            .get(id)
                            .map(|t| t.title.as_str())
                            .unwrap_or("");
                        ctx.session_service.record_history_visit(&url, title);
                        persist_session_snapshot(&ctx);
                    }
                    refresh_tab_strip(
                        ctx.tab_manager,
                        ctx.tab_strip,
                        ctx.window,
                        ctx.window.client_size().0,
                    );
                    tracing::info!(%id, %url, is_success, "Navigation completed");
                }
                EngineEvent::AudioStateChanged { id, is_audible } => {
                    ctx.tab_manager.store_mut().set_audible(id, is_audible);
                    refresh_tab_strip(
                        ctx.tab_manager,
                        ctx.tab_strip,
                        ctx.window,
                        ctx.window.client_size().0,
                    );
                    tracing::info!(%id, is_audible, "Tab audio state updated");
                }
                EngineEvent::ProcessFailed {
                    id,
                    exit_code,
                    description,
                } => {
                    tracing::error!(%id, exit_code, %description, "WebView2 process failure!");
                }
                EngineEvent::BlockedPopup { id, url } => {
                    tracing::warn!(%id, %url, "Popup blocked by Global Popup Guard");
                }
                EngineEvent::NewWindowRequested {
                    id,
                    target_url,
                    is_user_initiated,
                } => {
                    if is_user_initiated && !target_url.is_empty() {
                        let bounds = ctx.window.compute_webview_bounds(HEADER_HEIGHT);
                        let _ = open_new_tab(&mut ctx, &target_url, bounds);
                        tracing::info!(%id, %target_url, "User-initiated popup opened as new tab");
                    } else {
                        tracing::warn!(%id, %target_url, "Popunder / uninitiated window prevented");
                    }
                }
                EngineEvent::ZapperRuleCreated { domain, selector } => {
                    if let Some(active_id) = ctx.tab_manager.store().active_id() {
                        let css = format!("{} {{ display: none !important; }}", selector);
                        ctx.engine.inject_custom_css(active_id, &css);
                    }
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let _ = ctx.db_sender.send(storage::DbCommand::AddZapperRule {
                        domain: domain.clone(),
                        selector: selector.clone(),
                        time_ms: now_ms,
                    });
                    tracing::info!(%domain, %selector, "Zapper rule saved and applied");
                }
                _ => {}
            }
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    // 9. Clean shutdown: commit session state, log metrics & mark clean exit
    let metrics_snap = metrics.snapshot();
    tracing::info!(
        page_loads = metrics_snap.page_loads,
        tab_switches = metrics_snap.tab_switches,
        avg_switch_latency_ms = format!("{:.2}", metrics_snap.avg_tab_switch_latency_ms),
        peak_pws_mb = metrics_snap.peak_pws_mb,
        adblock_blocked = metrics_snap.adblock_blocked,
        "RustBrowse Session Metrics Summary"
    );
    session_service.mark_clean_shutdown();
    db_worker.shutdown();
    AppLifecycle::shutdown();
    Ok(())
}

fn set_clipboard_text(text: &str) {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND};

    let wide: Vec<u16> = std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let size_bytes = wide.len() * std::mem::size_of::<u16>();

    unsafe {
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();
            if let Ok(h_global) = GlobalAlloc(GHND, size_bytes) {
                let ptr = GlobalLock(h_global);
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(
                        wide.as_ptr() as *const u8,
                        ptr as *mut u8,
                        size_bytes,
                    );
                    let _ = GlobalUnlock(h_global);
                    let _ = SetClipboardData(
                        13, // CF_UNICODETEXT
                        Some(windows::Win32::Foundation::HANDLE(h_global.0 as _)),
                    );
                }
            }
            let _ = CloseClipboard();
        }
    }
}

fn open_new_tab(ctx: &mut ShellContext, url: &str, bounds: Rect) -> Result<()> {
    let old_id_opt = ctx.tab_manager.store().active_id();

    let meta = ctx.tab_manager.allocate_new_tab(url.to_string());
    // Immediately claims pre-warmed spare Child WebView in 0ms!
    ctx.engine.create_tab(meta.id, url, bounds, true)?;
    ctx.tab_manager.activate_tab(meta.id);
    ctx.toolbar.address_bar().set_text(url);

    // Hide old active tab (show before hide eliminates frame gap / white flash)
    if let Some(old_id) = old_id_opt {
        let _ = ctx.engine.set_tab_visible(old_id, false);
        // Protect tab from memory drop if playing audio/video
        let is_audible = ctx.tab_manager.store().get(old_id).map(|t| t.is_audible).unwrap_or(false);
        if !is_audible {
            let _ = ctx.engine.set_tab_memory_target(old_id, true);
        }
        ctx.tab_manager
            .store_mut()
            .set_state(old_id, TabLifecycleState::Warm);
    }

    persist_session_snapshot(ctx);
    refresh_tab_strip(
        ctx.tab_manager,
        ctx.tab_strip,
        ctx.window,
        ctx.window.client_size().0,
    );
    Ok(())
}

fn switch_to_tab(ctx: &mut ShellContext, target_id: TabId, bounds: Rect) -> Result<()> {
    let switch_start = std::time::Instant::now();
    let current_active = ctx.tab_manager.store().active_id();
    if current_active == Some(target_id) {
        return Ok(());
    }

    let tab_meta = ctx
        .tab_manager
        .store()
        .get(target_id)
        .cloned()
        .ok_or(common::errors::RustBrowseError::TabNotFound(target_id.0))?;

    // 1. Prepare and show target tab FIRST (show before hide prevents desktop/white flash)
    let _ = ctx.engine.set_tab_bounds(target_id, bounds);
    match tab_meta.state {
        TabLifecycleState::Sleeping => {
            // Instant wake from Soft Sleep (<15ms, NO page reload, DOM/scroll preserved)
            let _ = ctx.engine.resume_tab(target_id);
            let _ = ctx.engine.set_tab_memory_target(target_id, false);
            let _ = ctx.engine.set_tab_visible(target_id, true);
        }
        TabLifecycleState::Discarded => {
            ctx.engine
                .create_tab(target_id, &tab_meta.url, bounds, true)?;
        }
        _ => {
            let _ = ctx.engine.set_tab_memory_target(target_id, false);
            let _ = ctx.engine.set_tab_visible(target_id, true);
        }
    }

    // 2. NOW hide previous active tab
    if let Some(old_id) = current_active {
        let _ = ctx.engine.set_tab_visible(old_id, false);
        // Only set memory target to Low if tab is NOT currently playing audio/video
        let is_audible = ctx.tab_manager.store().get(old_id).map(|t| t.is_audible).unwrap_or(false);
        if !is_audible {
            let _ = ctx.engine.set_tab_memory_target(old_id, true);
        }
        ctx.tab_manager
            .store_mut()
            .set_state(old_id, TabLifecycleState::Warm);
    }

    ctx.tab_manager.activate_tab(target_id);
    ctx.toolbar.address_bar().set_text(&tab_meta.url);

    persist_session_snapshot(ctx);
    refresh_tab_strip(
        ctx.tab_manager,
        ctx.tab_strip,
        ctx.window,
        ctx.window.client_size().0,
    );
    ctx.metrics
        .record_tab_switch(switch_start.elapsed().as_millis() as u64);
    Ok(())
}

fn close_tab_item(ctx: &mut ShellContext, id: TabId, bounds: Rect, home_url: &str) -> Result<()> {
    let is_active = ctx.tab_manager.store().active_id() == Some(id);

    // If active tab being closed, pick next active tab first
    if is_active && ctx.tab_manager.store().tab_count() > 1 {
        if let Some(next_id) = ctx.tab_manager.store().next_tab_id(id) {
            let _ = switch_to_tab(ctx, next_id, bounds);
        }
    }

    let _ = ctx.engine.close_tab(id);
    ctx.tab_manager.close_tab(id);

    // If all tabs were closed, spawn a clean new tab
    if ctx.tab_manager.store().tab_count() == 0 {
        open_new_tab(ctx, home_url, bounds)?;
    } else {
        persist_session_snapshot(ctx);
        refresh_tab_strip(
            ctx.tab_manager,
            ctx.tab_strip,
            ctx.window,
            ctx.window.client_size().0,
        );
    }

    Ok(())
}

fn persist_session_snapshot(ctx: &ShellContext) {
    let all_tabs: Vec<common::types::TabMetadata> =
        ctx.tab_manager.store().all_tabs().cloned().collect();
    ctx.session_service.save_session_snapshot(
        &all_tabs,
        ctx.tab_manager.store().active_id(),
        false,
    );
}

fn refresh_tab_strip(
    tab_manager: &TabManager,
    tab_strip: &Rc<RefCell<TabStrip>>,
    window: &BrowserWindow,
    client_width: i32,
) {
    let order = tab_manager.store().tab_order();
    let mut tab_refs = Vec::with_capacity(order.len());
    for &id in order {
        if let Some(tab) = tab_manager.store().get(id) {
            tab_refs.push(tab);
        }
    }

    tab_strip
        .borrow_mut()
        .update_layout(&tab_refs, tab_manager.store().active_id(), client_width);
    window.invalidate_tab_strip();
}
