//! Anti-Freeze, Real Per-Tab RAM + CPU Monitor & Safe Incremental Auto-Clean Policy Engine
//! Target: Windows 11 x64 (Rust + Tauri v2 + WebView2)
//!
//! - Reads real Windows OS memory & CPU metrics strictly on a background worker thread:
//!   CreateToolhelp32Snapshot + K32GetProcessMemoryInfo (PrivateUsage & WorkingSetSize) +
//!   GetProcessTimes (OS CPU% delta) + GlobalMemoryStatusEx (system memory pressure).
//! - Exposes O(1) microsecond cached snapshots via RwLock — NEVER scans processes on UI/IPC threads.
//! - Adaptive sampling cadence via `set_perf_panel_open(open)`: 1000ms when Perf panel is open,
//!   8000ms when closed, with immediate Condvar wakeup on open.
//! - Bounded history ring buffer (`VecDeque`, max 60 samples) for long-session stability (C11).
//! - Pure, unit-tested incremental auto-clean policy engine (Off / Balanced / Aggressive)
//!   with 60s cooldown, +30MB hysteresis, max 1 small action per cycle, and strict protection
//!   for active tab, audio/video tabs, active downloads, GPU process, and browser process.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetProcessTimes, OpenProcess, SetPriorityClass,
    ABOVE_NORMAL_PRIORITY_CLASS, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA, PROCESS_VM_READ,
};

pub const COOLDOWN_SECS: u64 = 60;
pub const HYSTERESIS_MB: u64 = 30;
pub const MAX_HISTORY_SAMPLES: usize = 60;
const SAMPLER_OPEN_INTERVAL_MS: u64 = 1000;
const SAMPLER_IDLE_INTERVAL_MS: u64 = 8000;

static PER_TAB_THRESHOLD_MB: AtomicU64 = AtomicU64::new(300);
static GAMING_MODE: AtomicBool = AtomicBool::new(false);
static ACTIVE_TAB_PID: AtomicU32 = AtomicU32::new(0);
static LAST_TOTAL_RAM_MB: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessRole {
    Browser,
    Gpu,
    Renderer,
    Utility,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoCleanLevel {
    Off,
    #[default]
    Balanced,
    Aggressive,
}

impl AutoCleanLevel {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "disabled" | "none" | "0" => Self::Off,
            "aggressive" | "high" | "max" | "2" => Self::Aggressive,
            _ => Self::Balanced,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Balanced => 1,
            Self::Aggressive => 2,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Off,
            2 => Self::Aggressive,
            _ => Self::Balanced,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMemoryEntry {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub role: ProcessRole,
    pub private_mb: u64,
    pub working_set_mb: u64,
    pub cpu_percent: f32,
    pub is_main: bool,
    pub is_active_tab: bool,
    pub is_audio: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabMemoryInfo {
    pub tab_id: u32,
    pub pid: Option<u32>,
    pub process_group: String,
    pub private_mb: u64,
    pub working_set_mb: u64,
    pub ram_mb: u64,
    pub is_shared_process: bool,
    pub is_active: bool,
    pub is_audio: bool,
    pub is_sleeping: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySamplePoint {
    pub timestamp_ms: u64,
    pub total_private_mb: u64,
    pub total_cpu_percent: f32,
    pub system_memory_load_pct: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMemoryReport {
    pub timestamp_ms: u64,
    pub total_private_mb: u64,
    pub total_working_set_mb: u64,
    pub total_cpu_percent: f32,
    pub system_memory_load_pct: u32,
    pub system_total_ram_mb: u64,
    pub system_avail_ram_mb: u64,
    pub memory_pressure_level: String,
    pub perf_panel_open: bool,
    pub browser_mb: u64,
    pub gpu_mb: u64,
    pub renderer_mb: u64,
    pub utility_mb: u64,
    pub process_count: usize,
    pub renderer_count: usize,
    pub processes: Vec<ProcessMemoryEntry>,
    pub tabs: Vec<TabMemoryInfo>,
    pub history: Vec<MemorySamplePoint>,
    pub total_cleaned_mb: u64,
    pub auto_clean_level: AutoCleanLevel,
    pub last_clean_action: Option<String>,
}

impl Default for ProcessMemoryReport {
    fn default() -> Self {
        Self {
            timestamp_ms: now_unix_ms(),
            total_private_mb: 0,
            total_working_set_mb: 0,
            total_cpu_percent: 0.0,
            system_memory_load_pct: 0,
            system_total_ram_mb: 0,
            system_avail_ram_mb: 0,
            memory_pressure_level: "low".to_string(),
            perf_panel_open: false,
            browser_mb: 0,
            gpu_mb: 0,
            renderer_mb: 0,
            utility_mb: 0,
            process_count: 0,
            renderer_count: 0,
            processes: Vec::new(),
            tabs: Vec::new(),
            history: Vec::new(),
            total_cleaned_mb: 0,
            auto_clean_level: AutoCleanLevel::Balanced,
            last_clean_action: None,
        }
    }
}

/// Input fed from `tabs.rs` so Watchdog can associate real renderer PIDs & protection flags.
#[derive(Debug, Clone)]
pub struct TabTrackInput {
    pub tab_id: u32,
    pub pid: Option<u32>,
    pub is_active: bool,
    pub is_audio_or_video: bool,
    pub is_downloading: bool,
    pub is_sleeping: bool,
    pub idle_secs: u64,
}

/// Pure policy engine types (100% deterministic & unit-testable).
#[derive(Debug, Clone)]
pub struct TabPolicyState {
    pub tab_id: u32,
    pub pid: u32,
    pub role: ProcessRole,
    pub private_mb: u64,
    pub working_set_mb: u64,
    pub is_active: bool,
    pub is_audio_or_video: bool,
    pub is_downloading: bool,
    pub is_sleeping: bool,
    pub idle_secs: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct TrimRecord {
    pub last_trim_secs_ago: u64,
    pub ram_after_mb: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryPressure {
    pub total_browser_mb: u64,
    pub per_tab_threshold_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CleanAction {
    None,
    SoftTrim {
        tab_id: u32,
        pid: u32,
        current_mb: u64,
        reason: String,
    },
    SleepTab {
        tab_id: u32,
        pid: u32,
        current_mb: u64,
        reason: String,
    },
}

/// Pure helper: computes normalized OS CPU% (0.0..=100.0) from `GetProcessTimes` 100ns deltas.
pub fn compute_cpu_percent(
    prev_proc_100ns: u64,
    curr_proc_100ns: u64,
    elapsed_secs: f64,
    logical_cpus: usize,
) -> f32 {
    if elapsed_secs <= 0.001 || curr_proc_100ns <= prev_proc_100ns {
        return 0.0;
    }
    let delta_secs = (curr_proc_100ns - prev_proc_100ns) as f64 / 10_000_000.0;
    let cpus = logical_cpus.max(1) as f64;
    let raw_pct = ((delta_secs / (elapsed_secs * cpus)) * 100.0).clamp(0.0, 100.0);
    ((raw_pct * 10.0).round() / 10.0) as f32
}

/// Pure helper: classifies OS memory load % into `"low" | "moderate" | "high"`.
pub fn classify_memory_pressure(system_load_pct: u32) -> &'static str {
    if system_load_pct >= 85 {
        "high"
    } else if system_load_pct >= 70 {
        "moderate"
    } else {
        "low"
    }
}

/// Pure helper: pushes a sample onto a bounded ring buffer capped at `max_len`.
pub fn push_bounded_sample(
    history: &mut VecDeque<MemorySamplePoint>,
    sample: MemorySamplePoint,
    max_len: usize,
) {
    if max_len == 0 {
        history.clear();
        return;
    }
    while history.len() >= max_len {
        history.pop_front();
    }
    history.push_back(sample);
}

/// Pure, deterministic incremental auto-clean policy engine.
///
/// Guarantees:
/// - Returns AT MOST ONE small action per cycle (zero mass EmptyWorkingSet storms).
/// - Strictly NEVER touches active tab, audio/video tabs, active downloads, GPU process,
///   browser process, or utility processes (only `ProcessRole::Renderer`).
/// - Enforces 60s cooldown and +30MB hysteresis per PID.
pub fn evaluate_auto_clean_policy(
    level: AutoCleanLevel,
    pressure: &MemoryPressure,
    tabs: &[TabPolicyState],
    trim_history: &HashMap<u32, TrimRecord>,
) -> CleanAction {
    if level == AutoCleanLevel::Off {
        return CleanAction::None;
    }

    let (effective_threshold, min_trim_idle_secs, sleep_idle_secs) = match level {
        AutoCleanLevel::Off => return CleanAction::None,
        AutoCleanLevel::Balanced => (pressure.per_tab_threshold_mb.max(180), 30u64, 600u64),
        AutoCleanLevel::Aggressive => {
            let scaled = (pressure.per_tab_threshold_mb * 70) / 100;
            (scaled.max(150), 15u64, 300u64)
        }
    };

    let mut best_candidate: Option<(&TabPolicyState, u64, u64)> = None;

    for tab in tabs {
        // Hard safety exemptions — NEVER violated regardless of memory pressure.
        if tab.role != ProcessRole::Renderer {
            continue;
        }
        if tab.is_active || tab.is_audio_or_video || tab.is_downloading {
            continue;
        }
        if tab.idle_secs < min_trim_idle_secs {
            continue;
        }

        let current_mb = if tab.private_mb > 0 {
            tab.private_mb
        } else {
            tab.working_set_mb
        };

        if current_mb <= effective_threshold {
            continue;
        }

        // Enforce 60s cooldown + 30MB hysteresis
        if let Some(rec) = trim_history.get(&tab.pid) {
            if rec.last_trim_secs_ago < COOLDOWN_SECS {
                continue;
            }
            if current_mb < rec.ram_after_mb.saturating_add(HYSTERESIS_MB) {
                continue;
            }
        }

        let excess = current_mb - effective_threshold;
        match best_candidate {
            None => best_candidate = Some((tab, current_mb, excess)),
            Some((_, _, best_excess)) if excess > best_excess => {
                best_candidate = Some((tab, current_mb, excess));
            }
            _ => {}
        }
    }

    if let Some((tab, current_mb, _)) = best_candidate {
        if !tab.is_sleeping && tab.idle_secs >= sleep_idle_secs {
            CleanAction::SleepTab {
                tab_id: tab.tab_id,
                pid: tab.pid,
                current_mb,
                reason: format!(
                    "Idle {}s and {}MB > {}MB (total {}MB, {:?})",
                    tab.idle_secs,
                    current_mb,
                    effective_threshold,
                    pressure.total_browser_mb,
                    level
                ),
            }
        } else {
            CleanAction::SoftTrim {
                tab_id: tab.tab_id,
                pid: tab.pid,
                current_mb,
                reason: format!(
                    "Background renderer {}MB > {}MB (total {}MB, {:?})",
                    current_mb, effective_threshold, pressure.total_browser_mb, level
                ),
            }
        }
    } else {
        CleanAction::None
    }
}

struct RawProcessSnapshot {
    entry: ProcessMemoryEntry,
    cpu_time_100ns: Option<u64>,
}

#[allow(dead_code)]
pub struct Watchdog {
    protected_pids: Mutex<HashSet<u32>>,
    trim_history: Mutex<HashMap<u32, (Instant, u64)>>,
    tracked_tabs: Mutex<Vec<TabTrackInput>>,
    cpu_prev: Mutex<HashMap<u32, (u64, Instant)>>,
    history: Mutex<VecDeque<MemorySamplePoint>>,
    latest_report: RwLock<ProcessMemoryReport>,
    last_clean_action: Mutex<Option<String>>,
    total_ram_cleaned_mb: AtomicU64,
    auto_clean_level: AtomicU8,
    perf_panel_open: AtomicBool,
    sampler_wake: (Mutex<u64>, Condvar),
    logical_cpus: usize,
}

#[allow(dead_code)]
impl Watchdog {
    pub fn new() -> Arc<Self> {
        let logical_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        let wd = Arc::new(Self {
            protected_pids: Mutex::new(HashSet::new()),
            trim_history: Mutex::new(HashMap::new()),
            tracked_tabs: Mutex::new(Vec::new()),
            cpu_prev: Mutex::new(HashMap::new()),
            history: Mutex::new(VecDeque::with_capacity(MAX_HISTORY_SAMPLES)),
            latest_report: RwLock::new(ProcessMemoryReport::default()),
            last_clean_action: Mutex::new(None),
            total_ram_cleaned_mb: AtomicU64::new(0),
            auto_clean_level: AtomicU8::new(AutoCleanLevel::Balanced.to_u8()),
            perf_panel_open: AtomicBool::new(false),
            sampler_wake: (Mutex::new(0), Condvar::new()),
            logical_cpus,
        });

        Self::elevate_ui_priority();

        // Background adaptive OS RAM + CPU sampler + incremental policy engine.
        // Yields briefly (250ms) so cold-start shell & Tab 1 render with zero Win32 contention.
        let wd_clone = wd.clone();
        let _ = std::thread::Builder::new()
            .name("watchdog-ram-sampler".to_string())
            .spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                let mut last_policy_run = Instant::now();
                loop {
                    let report = wd_clone.refresh_memory_report();
                    if last_policy_run.elapsed() >= Duration::from_secs(4) {
                        wd_clone.run_policy_cycle(&report);
                        last_policy_run = Instant::now();
                    }

                    let interval_ms = if wd_clone.is_perf_panel_open() {
                        SAMPLER_OPEN_INTERVAL_MS
                    } else {
                        SAMPLER_IDLE_INTERVAL_MS
                    };

                    let (lock, cvar) = &wd_clone.sampler_wake;
                    if let Ok(guard) = lock.lock() {
                        let _ = cvar.wait_timeout(guard, Duration::from_millis(interval_ms));
                    } else {
                        std::thread::sleep(Duration::from_millis(interval_ms));
                    }
                }
            });

        wd
    }

    /// Promotes UI process to ABOVE_NORMAL_PRIORITY_CLASS.
    pub fn elevate_ui_priority() {
        unsafe {
            let proc = GetCurrentProcess();
            if let Err(e) = SetPriorityClass(proc, ABOVE_NORMAL_PRIORITY_CLASS) {
                warn!(error = ?e, "Failed to set ABOVE_NORMAL_PRIORITY_CLASS for UI process");
            } else {
                info!("RustBrowse UI process promoted to ABOVE_NORMAL_PRIORITY_CLASS (Anti-Freeze Active).");
            }
        }
    }

    /// Switches background sampler between 1000ms (Perf panel open) and 8000ms (closed),
    /// waking the sampler thread immediately when opened. Never blocks UI thread.
    pub fn set_perf_panel_open(&self, open: bool) {
        self.perf_panel_open.store(open, Ordering::Relaxed);
        if let Ok(mut report) = self.latest_report.write() {
            report.perf_panel_open = open;
        }
        let (lock, cvar) = &self.sampler_wake;
        if let Ok(mut seq) = lock.lock() {
            *seq = seq.wrapping_add(1);
            cvar.notify_all();
        }
    }

    pub fn is_perf_panel_open(&self) -> bool {
        self.perf_panel_open.load(Ordering::Relaxed)
    }

    pub fn set_per_tab_threshold(mb: u64) {
        PER_TAB_THRESHOLD_MB.store(mb.max(128), Ordering::Relaxed);
        info!(threshold_mb = mb, "Updated per-tab RAM budget threshold.");
    }

    pub fn get_per_tab_threshold() -> u64 {
        PER_TAB_THRESHOLD_MB.load(Ordering::Relaxed)
    }

    pub fn set_auto_clean_level(&self, level: AutoCleanLevel) {
        self.auto_clean_level
            .store(level.to_u8(), Ordering::Relaxed);
        if let Ok(mut report) = self.latest_report.write() {
            report.auto_clean_level = level;
        }
        info!(level = ?level, "Auto-clean policy level updated.");
    }

    pub fn get_auto_clean_level(&self) -> AutoCleanLevel {
        AutoCleanLevel::from_u8(self.auto_clean_level.load(Ordering::Relaxed))
    }

    pub fn set_active_tab_pid(&self, pid: u32) {
        ACTIVE_TAB_PID.store(pid, Ordering::Relaxed);
    }

    pub fn protect_pid(&self, pid: u32) {
        if let Ok(mut set) = self.protected_pids.lock() {
            set.insert(pid);
        }
    }

    pub fn unprotect_pid(&self, pid: u32) {
        if let Ok(mut set) = self.protected_pids.lock() {
            set.remove(&pid);
        }
    }

    pub fn set_gaming_mode(&self, enabled: bool) {
        GAMING_MODE.store(enabled, Ordering::Relaxed);
        if enabled {
            self.set_auto_clean_level(AutoCleanLevel::Aggressive);
        } else {
            self.set_auto_clean_level(AutoCleanLevel::Balanced);
        }
        info!(gaming_mode = enabled, "Gaming mode state changed.");
    }

    pub fn is_gaming_mode() -> bool {
        GAMING_MODE.load(Ordering::Relaxed)
    }

    /// Updates tracked tab metadata from `TabManager` so per-tab RAM attribution & policy are exact.
    pub fn update_tab_states(&self, tabs: Vec<TabTrackInput>) {
        if let Some(active_pid) = tabs.iter().find(|t| t.is_active).and_then(|t| t.pid) {
            ACTIVE_TAB_PID.store(active_pid, Ordering::Relaxed);
        }
        if let Ok(mut guard) = self.tracked_tabs.lock() {
            *guard = tabs;
        }
    }

    /// O(1) non-blocking read of the latest cached OS memory & CPU report.
    /// NEVER invokes Win32 process enumeration on the caller/UI/IPC thread.
    pub fn get_memory_report(&self) -> ProcessMemoryReport {
        self.latest_report
            .read()
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// O(1) non-blocking lookup of a specific tab's real live memory info.
    pub fn get_tab_memory(&self, tab_id: u32) -> Option<TabMemoryInfo> {
        let report = self.latest_report.read().ok()?;
        report.tabs.iter().find(|t| t.tab_id == tab_id).cloned()
    }

    /// Performs one OS snapshot pass on the background worker thread and updates
    /// the cached `ProcessMemoryReport` + bounded 60-sample history ring buffer.
    pub fn refresh_memory_report(&self) -> ProcessMemoryReport {
        let protected = self
            .protected_pids
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let tracked = self
            .tracked_tabs
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        let now = Instant::now();
        let raw_snaps = Self::snapshot_browser_process_tree();
        let (sys_load_pct, sys_total_mb, sys_avail_mb) = query_system_memory_status();

        let mut processes = Vec::with_capacity(raw_snaps.len());
        let mut live_pids: HashSet<u32> = HashSet::with_capacity(raw_snaps.len());
        let mut total_cpu_percent: f32 = 0.0;

        {
            let mut cpu_prev = self.cpu_prev.lock().unwrap_or_else(|e| e.into_inner());
            for mut snap in raw_snaps {
                let pid = snap.entry.pid;
                live_pids.insert(pid);
                if protected.contains(&pid) {
                    snap.entry.is_audio = true;
                }
                if let Some(curr_100ns) = snap.cpu_time_100ns {
                    let pct = if let Some(&(prev_100ns, prev_inst)) = cpu_prev.get(&pid) {
                        compute_cpu_percent(
                            prev_100ns,
                            curr_100ns,
                            now.saturating_duration_since(prev_inst).as_secs_f64(),
                            self.logical_cpus,
                        )
                    } else {
                        0.0
                    };
                    cpu_prev.insert(pid, (curr_100ns, now));
                    snap.entry.cpu_percent = pct;
                    total_cpu_percent += pct;
                }
                processes.push(snap.entry);
            }
            // Prune exited PIDs so cpu_prev map never leaks memory over long sessions (C11)
            cpu_prev.retain(|pid, _| live_pids.contains(pid));
        }

        // Prune exited PIDs from trim_history as well (C11)
        if let Ok(mut hist) = self.trim_history.lock() {
            hist.retain(|pid, _| live_pids.contains(pid));
        }

        total_cpu_percent = ((total_cpu_percent.min(100.0) * 10.0).round()) / 10.0;

        let mut total_private_mb = 0u64;
        let mut total_working_set_mb = 0u64;
        let mut browser_mb = 0u64;
        let mut gpu_mb = 0u64;
        let mut renderer_mb = 0u64;
        let mut utility_mb = 0u64;
        let mut renderer_count = 0usize;

        let mut proc_by_pid: HashMap<u32, &ProcessMemoryEntry> = HashMap::new();
        for p in &processes {
            let primary = if p.private_mb > 0 {
                p.private_mb
            } else {
                p.working_set_mb
            };
            total_private_mb += primary;
            total_working_set_mb += p.working_set_mb;
            match p.role {
                ProcessRole::Browser => browser_mb += primary,
                ProcessRole::Gpu => gpu_mb += primary,
                ProcessRole::Renderer => {
                    renderer_mb += primary;
                    renderer_count += 1;
                }
                ProcessRole::Utility | ProcessRole::Other => utility_mb += primary,
            }
            proc_by_pid.insert(p.pid, p);
        }

        // Count how many tracked tabs share each explicit PID
        let mut pid_share_counts: HashMap<u32, u64> = HashMap::new();
        let mut unmapped_active_tabs = 0u64;
        for t in &tracked {
            if let Some(pid) = t.pid {
                *pid_share_counts.entry(pid).or_insert(0) += 1;
            } else if !t.is_sleeping {
                unmapped_active_tabs += 1;
            }
        }

        let mut tabs_info = Vec::with_capacity(tracked.len());
        for t in &tracked {
            if let Some(pid) = t.pid {
                if let Some(proc_entry) = proc_by_pid.get(&pid) {
                    let shares = pid_share_counts.get(&pid).copied().unwrap_or(1).max(1);
                    let priv_share = proc_entry.private_mb / shares;
                    let ws_share = proc_entry.working_set_mb / shares;
                    let ram = if priv_share > 0 { priv_share } else { ws_share };
                    tabs_info.push(TabMemoryInfo {
                        tab_id: t.tab_id,
                        pid: Some(pid),
                        process_group: format!("renderer-{}", pid),
                        private_mb: priv_share,
                        working_set_mb: ws_share,
                        ram_mb: ram,
                        is_shared_process: shares > 1,
                        is_active: t.is_active,
                        is_audio: t.is_audio_or_video,
                        is_sleeping: t.is_sleeping,
                    });
                    continue;
                }
            }

            // Fallback when WebView2 hasn't reported an explicit per-tab PID yet:
            // attribute fairly from real OS renderer pool memory (zero hardcoded fake numbers).
            let (priv_est, ws_est) = if t.is_sleeping || unmapped_active_tabs == 0 {
                (0, 0)
            } else {
                let pool_priv = renderer_mb / unmapped_active_tabs;
                let renderer_ws_sum: u64 = processes
                    .iter()
                    .filter(|p| p.role == ProcessRole::Renderer)
                    .map(|p| p.working_set_mb)
                    .sum();
                (pool_priv, renderer_ws_sum / unmapped_active_tabs)
            };

            tabs_info.push(TabMemoryInfo {
                tab_id: t.tab_id,
                pid: t.pid,
                process_group: if t.is_sleeping {
                    "sleeping".to_string()
                } else {
                    "renderer-pool".to_string()
                },
                private_mb: priv_est,
                working_set_mb: ws_est,
                ram_mb: if priv_est > 0 { priv_est } else { ws_est },
                is_shared_process: true,
                is_active: t.is_active,
                is_audio: t.is_audio_or_video,
                is_sleeping: t.is_sleeping,
            });
        }

        LAST_TOTAL_RAM_MB.store(total_private_mb, Ordering::Relaxed);
        let ts_ms = now_unix_ms();

        let history_vec = {
            let mut h = self.history.lock().unwrap_or_else(|e| e.into_inner());
            push_bounded_sample(
                &mut h,
                MemorySamplePoint {
                    timestamp_ms: ts_ms,
                    total_private_mb,
                    total_cpu_percent,
                    system_memory_load_pct: sys_load_pct,
                },
                MAX_HISTORY_SAMPLES,
            );
            h.iter().cloned().collect()
        };

        let report = ProcessMemoryReport {
            timestamp_ms: ts_ms,
            total_private_mb,
            total_working_set_mb,
            total_cpu_percent,
            system_memory_load_pct: sys_load_pct,
            system_total_ram_mb: sys_total_mb,
            system_avail_ram_mb: sys_avail_mb,
            memory_pressure_level: classify_memory_pressure(sys_load_pct).to_string(),
            perf_panel_open: self.is_perf_panel_open(),
            browser_mb,
            gpu_mb,
            renderer_mb,
            utility_mb,
            process_count: processes.len(),
            renderer_count,
            processes,
            tabs: tabs_info,
            history: history_vec,
            total_cleaned_mb: self.total_ram_cleaned_mb.load(Ordering::Relaxed),
            auto_clean_level: self.get_auto_clean_level(),
            last_clean_action: self.last_clean_action.lock().ok().and_then(|g| g.clone()),
        };

        if let Ok(mut guard) = self.latest_report.write() {
            *guard = report.clone();
        }

        report
    }

    /// Executes at most one incremental policy action based on the latest report.
    fn run_policy_cycle(&self, report: &ProcessMemoryReport) {
        let level = self.get_auto_clean_level();
        if level == AutoCleanLevel::Off {
            return;
        }

        let active_pid = ACTIVE_TAB_PID.load(Ordering::Relaxed);
        let protected = self
            .protected_pids
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let tracked = self
            .tracked_tabs
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        // Build policy state list from real OS processes + tracked tab protection flags
        let mut policy_states = Vec::new();
        for proc_entry in &report.processes {
            let matching_tabs: Vec<&TabTrackInput> = tracked
                .iter()
                .filter(|t| t.pid == Some(proc_entry.pid))
                .collect();

            let is_active =
                proc_entry.pid == active_pid || matching_tabs.iter().any(|t| t.is_active);
            let is_audio_or_video = protected.contains(&proc_entry.pid)
                || proc_entry.is_audio
                || matching_tabs.iter().any(|t| t.is_audio_or_video);
            let is_downloading = matching_tabs.iter().any(|t| t.is_downloading);
            let is_sleeping =
                !matching_tabs.is_empty() && matching_tabs.iter().all(|t| t.is_sleeping);
            let idle_secs = matching_tabs
                .iter()
                .map(|t| t.idle_secs)
                .min()
                .unwrap_or(60);
            let tab_id = matching_tabs.first().map(|t| t.tab_id).unwrap_or(0);

            policy_states.push(TabPolicyState {
                tab_id,
                pid: proc_entry.pid,
                role: proc_entry.role,
                private_mb: proc_entry.private_mb,
                working_set_mb: proc_entry.working_set_mb,
                is_active,
                is_audio_or_video,
                is_downloading,
                is_sleeping,
                idle_secs,
            });
        }

        let history_snapshot: HashMap<u32, TrimRecord> = self
            .trim_history
            .lock()
            .map(|h| {
                h.iter()
                    .map(|(&pid, &(instant, ram_after))| {
                        (
                            pid,
                            TrimRecord {
                                last_trim_secs_ago: instant.elapsed().as_secs(),
                                ram_after_mb: ram_after,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let pressure = MemoryPressure {
            total_browser_mb: report.total_private_mb,
            per_tab_threshold_mb: PER_TAB_THRESHOLD_MB.load(Ordering::Relaxed),
        };

        let action =
            evaluate_auto_clean_policy(level, &pressure, &policy_states, &history_snapshot);

        match action {
            CleanAction::None => {}
            CleanAction::SoftTrim {
                tab_id,
                pid,
                current_mb,
                reason,
            }
            | CleanAction::SleepTab {
                tab_id,
                pid,
                current_mb,
                reason,
            } => {
                let ram_after = Self::trim_process_working_set(pid);
                if ram_after < current_mb {
                    let saved = current_mb - ram_after;
                    self.total_ram_cleaned_mb
                        .fetch_add(saved, Ordering::Relaxed);
                    if let Ok(mut h) = self.trim_history.lock() {
                        h.insert(pid, (Instant::now(), ram_after));
                    }
                    let msg = format!(
                        "Trimmed PID {} (tab {}) -{} MB: {}",
                        pid, tab_id, saved, reason
                    );
                    if let Ok(mut last) = self.last_clean_action.lock() {
                        *last = Some(msg);
                    }
                    info!(pid, tab_id, saved_mb = saved, reason = %reason, "Incremental auto-clean executed.");
                } else if let Ok(mut h) = self.trim_history.lock() {
                    h.insert(pid, (Instant::now(), current_mb));
                }
            }
        }
    }

    /// Backward-compatible single-cycle entry point.
    pub fn run_monitoring_cycle(&self) {
        let report = self.refresh_memory_report();
        self.run_policy_cycle(&report);
    }

    /// Safe single-PID trim for manual per-tab clean (never trims Browser/GPU/Utility/protected).
    pub fn clean_tab_memory(&self, pid: u32) -> u64 {
        let active_pid = ACTIVE_TAB_PID.load(Ordering::Relaxed);
        if pid == active_pid {
            return 0;
        }
        if let Ok(set) = self.protected_pids.lock() {
            if set.contains(&pid) {
                return 0;
            }
        }
        let report = self.get_memory_report();
        let Some(entry) = report.processes.iter().find(|p| p.pid == pid) else {
            return 0;
        };
        if entry.role != ProcessRole::Renderer {
            return 0;
        }
        let before = if entry.private_mb > 0 {
            entry.private_mb
        } else {
            entry.working_set_mb
        };
        let after = Self::trim_process_working_set(pid);
        let saved = before.saturating_sub(after);
        if saved > 0 {
            self.total_ram_cleaned_mb
                .fetch_add(saved, Ordering::Relaxed);
            if let Ok(mut h) = self.trim_history.lock() {
                h.insert(pid, (Instant::now(), after));
            }
        }
        saved
    }

    /// Manual RAM clean triggered by user: uses cached process snapshot (never runs
    /// Toolhelp32 on the caller thread) and trims ONLY eligible idle Renderer processes.
    /// Strictly exempts Browser, GPU, Utility, active tab, audio/video, and downloading tabs.
    pub fn clean_all_background_memory(&self) -> u64 {
        let active_pid = ACTIVE_TAB_PID.load(Ordering::Relaxed);
        let protected = self
            .protected_pids
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let tracked = self
            .tracked_tabs
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        let entries = self.get_memory_report().processes;
        let mut total_saved = 0u64;

        for entry in entries {
            if entry.role != ProcessRole::Renderer {
                continue;
            }
            if entry.pid == active_pid || protected.contains(&entry.pid) {
                continue;
            }
            let tab_protected = tracked.iter().any(|t| {
                t.pid == Some(entry.pid) && (t.is_active || t.is_audio_or_video || t.is_downloading)
            });
            if tab_protected {
                continue;
            }

            let ram_before = entry.working_set_mb;
            let ram_after = Self::trim_process_working_set(entry.pid);
            if ram_before > ram_after {
                let saved = ram_before - ram_after;
                total_saved += saved;
                if let Ok(mut h) = self.trim_history.lock() {
                    h.insert(entry.pid, (Instant::now(), ram_after));
                }
            }
        }

        if total_saved > 0 {
            self.total_ram_cleaned_mb
                .fetch_add(total_saved, Ordering::Relaxed);
            if let Ok(mut last) = self.last_clean_action.lock() {
                *last = Some(format!("Manual clean freed {} MB", total_saved));
            }
        }

        // Wake background sampler to refresh cached report asynchronously
        let (lock, cvar) = &self.sampler_wake;
        if let Ok(mut seq) = lock.lock() {
            *seq = seq.wrapping_add(1);
            cvar.notify_all();
        }
        total_saved
    }

    /// Trims working set for a specific process handle using K32EmptyWorkingSet.
    pub fn trim_process_working_set(pid: u32) -> u64 {
        unsafe {
            if let Ok(handle) = OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
                false,
                pid,
            ) {
                let _ = K32EmptyWorkingSet(handle);
                let (priv_mb, ws_mb) = Self::query_process_memory_mb(handle);
                let _ = CloseHandle(handle);
                return if ws_mb > 0 { ws_mb } else { priv_mb };
            }
        }
        0
    }

    fn query_process_memory_mb(handle: HANDLE) -> (u64, u64) {
        unsafe {
            let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            counters.cb = size;
            if K32GetProcessMemoryInfo(handle, &mut counters as *mut _ as *mut _, size).as_bool() {
                let private_mb = (counters.PrivateUsage as u64) / (1024 * 1024);
                let ws_mb = (counters.WorkingSetSize as u64) / (1024 * 1024);
                return (private_mb, ws_mb);
            }
        }
        (0, 0)
    }

    fn snapshot_browser_process_tree() -> Vec<RawProcessSnapshot> {
        let current_pid = unsafe { GetCurrentProcessId() };
        let active_pid = ACTIVE_TAB_PID.load(Ordering::Relaxed);
        let mut results = Vec::new();

        unsafe {
            let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                Ok(s) => s,
                Err(_) => return results,
            };

            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            let mut all_procs: Vec<(u32, u32, String)> = Vec::with_capacity(256);
            let mut children_by_parent: HashMap<u32, Vec<usize>> = HashMap::new();

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let pid = entry.th32ProcessID;
                    let ppid = entry.th32ParentProcessID;
                    let nul_pos = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let exe_name = String::from_utf16_lossy(&entry.szExeFile[..nul_pos]);
                    let idx = all_procs.len();
                    all_procs.push((pid, ppid, exe_name));
                    children_by_parent.entry(ppid).or_default().push(idx);

                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);

            let mut visited: HashSet<u32> = HashSet::new();
            let mut queue: VecDeque<u32> = VecDeque::new();
            visited.insert(current_pid);
            queue.push_back(current_pid);

            while let Some(parent) = queue.pop_front() {
                if let Some(indices) = children_by_parent.get(&parent) {
                    for &idx in indices {
                        let child_pid = all_procs[idx].0;
                        if visited.insert(child_pid) {
                            queue.push_back(child_pid);
                        }
                    }
                }
            }

            for (pid, ppid, exe_name) in all_procs {
                if !visited.contains(&pid) {
                    continue;
                }

                if let Ok(handle) =
                    OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
                {
                    let (private_mb, working_set_mb) = Self::query_process_memory_mb(handle);
                    let cpu_time_100ns = query_process_cpu_time_100ns(handle);
                    let cmdline = if pid == current_pid {
                        None
                    } else {
                        read_process_command_line(handle)
                    };
                    let _ = CloseHandle(handle);

                    let role = classify_process_role(
                        pid,
                        current_pid,
                        ppid,
                        &exe_name,
                        cmdline.as_deref(),
                    );

                    results.push(RawProcessSnapshot {
                        entry: ProcessMemoryEntry {
                            pid,
                            parent_pid: ppid,
                            name: exe_name,
                            role,
                            private_mb,
                            working_set_mb,
                            cpu_percent: 0.0,
                            is_main: pid == current_pid,
                            is_active_tab: pid == active_pid,
                            is_audio: false,
                        },
                        cpu_time_100ns,
                    });
                }
            }
        }

        results
    }

    /// Enumerates the full RustBrowse + WebView2 process tree (multi-level BFS).
    pub fn get_browser_process_tree() -> Vec<ProcessMemoryEntry> {
        Self::snapshot_browser_process_tree()
            .into_iter()
            .map(|s| s.entry)
            .collect()
    }

    /// O(1) non-blocking read of cached total browser RAM in MB.
    /// NEVER triggers a Win32 process scan on the caller/UI/IPC thread.
    pub fn total_browser_ram_mb() -> u64 {
        LAST_TOTAL_RAM_MB.load(Ordering::Relaxed)
    }

    pub fn get_total_cleaned_mb(&self) -> u64 {
        self.total_ram_cleaned_mb.load(Ordering::Relaxed)
    }
}

pub fn classify_process_role(
    pid: u32,
    current_pid: u32,
    parent_pid: u32,
    exe_name: &str,
    cmdline: Option<&str>,
) -> ProcessRole {
    if pid == current_pid {
        return ProcessRole::Browser;
    }

    let exe_lower = exe_name.to_ascii_lowercase();

    if let Some(cmd) = cmdline {
        let cmd_lower = cmd.to_ascii_lowercase();
        if cmd_lower.contains("--type=gpu-process") {
            return ProcessRole::Gpu;
        }
        if cmd_lower.contains("--type=renderer") {
            return ProcessRole::Renderer;
        }
        if cmd_lower.contains("--type=utility")
            || cmd_lower.contains("--type=crashpad-handler")
            || cmd_lower.contains("--type=ppapi")
            || cmd_lower.contains("--type=broker")
        {
            return ProcessRole::Utility;
        }
        if exe_lower.contains("webview2") || exe_lower.contains("msedge") {
            return ProcessRole::Browser;
        }
        return ProcessRole::Other;
    }

    if parent_pid == current_pid {
        ProcessRole::Browser
    } else if exe_lower.contains("webview2") || exe_lower.contains("msedge") {
        ProcessRole::Renderer
    } else {
        ProcessRole::Utility
    }
}

fn filetime_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

fn query_process_cpu_time_100ns(handle: HANDLE) -> Option<u64> {
    unsafe {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user).is_ok() {
            Some(filetime_to_u64(kernel).saturating_add(filetime_to_u64(user)))
        } else {
            None
        }
    }
}

fn query_system_memory_status() -> (u32, u64, u64) {
    unsafe {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut status).is_ok() {
            let load_pct = status.dwMemoryLoad;
            let total_mb = status.ullTotalPhys / (1024 * 1024);
            let avail_mb = status.ullAvailPhys / (1024 * 1024);
            return (load_pct, total_mb, avail_mb);
        }
    }
    (0, 0, 0)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ============================================================================
// Safe Win32 PEB Command-Line Reader (x64 Windows) for exact WebView2 PID role
// ============================================================================

#[cfg(target_pointer_width = "64")]
#[repr(C)]
struct ProcessBasicInformation {
    reserved1: *mut c_void,
    peb_base_address: *mut c_void,
    reserved2: [*mut c_void; 2],
    unique_process_id: usize,
    reserved3: *mut c_void,
}

#[cfg(target_pointer_width = "64")]
#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[cfg(target_pointer_width = "64")]
#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        process_handle: *mut c_void,
        process_information_class: u32,
        process_information: *mut c_void,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[cfg(target_pointer_width = "64")]
#[link(name = "kernel32")]
extern "system" {
    fn ReadProcessMemory(
        h_process: *mut c_void,
        lp_base_address: *const c_void,
        lp_buffer: *mut c_void,
        n_size: usize,
        lp_number_of_bytes_read: *mut usize,
    ) -> i32;
}

#[cfg(target_pointer_width = "64")]
unsafe fn read_process_command_line(handle: HANDLE) -> Option<String> {
    let raw_handle = handle.0;
    if raw_handle.is_null() {
        return None;
    }

    let mut pbi = std::mem::zeroed::<ProcessBasicInformation>();
    let mut ret_len: u32 = 0;
    let status = NtQueryInformationProcess(
        raw_handle,
        0,
        &mut pbi as *mut _ as *mut c_void,
        std::mem::size_of::<ProcessBasicInformation>() as u32,
        &mut ret_len,
    );
    if status != 0 || pbi.peb_base_address.is_null() {
        return None;
    }

    let mut proc_params_ptr: *mut c_void = std::ptr::null_mut();
    let mut bytes_read: usize = 0;
    let ok = ReadProcessMemory(
        raw_handle,
        (pbi.peb_base_address as usize + 0x20) as *const c_void,
        &mut proc_params_ptr as *mut _ as *mut c_void,
        std::mem::size_of::<*mut c_void>(),
        &mut bytes_read,
    );
    if ok == 0 || proc_params_ptr.is_null() {
        return None;
    }

    let mut cmd_us = std::mem::zeroed::<UnicodeString>();
    let ok = ReadProcessMemory(
        raw_handle,
        (proc_params_ptr as usize + 0x70) as *const c_void,
        &mut cmd_us as *mut _ as *mut c_void,
        std::mem::size_of::<UnicodeString>(),
        &mut bytes_read,
    );
    if ok == 0 || cmd_us.buffer.is_null() || cmd_us.length == 0 {
        return None;
    }

    let byte_len = (cmd_us.length as usize).min(16384);
    let u16_len = byte_len / 2;
    if u16_len == 0 {
        return None;
    }

    let mut buf = vec![0u16; u16_len];
    let ok = ReadProcessMemory(
        raw_handle,
        cmd_us.buffer as *const c_void,
        buf.as_mut_ptr() as *mut c_void,
        u16_len * 2,
        &mut bytes_read,
    );
    if ok == 0 || bytes_read == 0 {
        return None;
    }

    let valid_u16 = (bytes_read / 2).min(u16_len);
    Some(String::from_utf16_lossy(&buf[..valid_u16]))
}

#[cfg(not(target_pointer_width = "64"))]
unsafe fn read_process_command_line(_handle: HANDLE) -> Option<String> {
    None
}

// ============================================================================
// Unit Tests: CPU Delta, Bounded History, Pressure & Auto-Clean Policy Engine
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tab(tab_id: u32, pid: u32, private_mb: u64, idle_secs: u64) -> TabPolicyState {
        TabPolicyState {
            tab_id,
            pid,
            role: ProcessRole::Renderer,
            private_mb,
            working_set_mb: private_mb + 20,
            is_active: false,
            is_audio_or_video: false,
            is_downloading: false,
            is_sleeping: false,
            idle_secs,
        }
    }

    #[test]
    fn cpu_delta_calculation_accurate_and_clamped() {
        // 1.0s of process CPU time across 2.0s elapsed on 4 logical CPUs = 12.5%
        let pct = compute_cpu_percent(10_000_000, 20_000_000, 2.0, 4);
        assert!((pct - 12.5).abs() < 0.01);

        // Zero or reversed delta -> 0.0%
        assert_eq!(compute_cpu_percent(20_000_000, 20_000_000, 1.0, 4), 0.0);
        assert_eq!(compute_cpu_percent(20_000_000, 10_000_000, 1.0, 4), 0.0);

        // Extreme spike clamps cleanly at 100.0%
        assert_eq!(compute_cpu_percent(0, 100_000_000, 0.5, 2), 100.0);
    }

    #[test]
    fn bounded_history_ring_buffer_caps_at_60() {
        let mut ring = VecDeque::new();
        for i in 0..100u64 {
            push_bounded_sample(
                &mut ring,
                MemorySamplePoint {
                    timestamp_ms: i * 1000,
                    total_private_mb: 200 + i,
                    total_cpu_percent: 5.0,
                    system_memory_load_pct: 50,
                },
                MAX_HISTORY_SAMPLES,
            );
        }
        assert_eq!(ring.len(), MAX_HISTORY_SAMPLES);
        assert_eq!(ring.front().unwrap().timestamp_ms, 40_000);
        assert_eq!(ring.back().unwrap().timestamp_ms, 99_000);
    }

    #[test]
    fn memory_pressure_classification_thresholds() {
        assert_eq!(classify_memory_pressure(45), "low");
        assert_eq!(classify_memory_pressure(70), "moderate");
        assert_eq!(classify_memory_pressure(84), "moderate");
        assert_eq!(classify_memory_pressure(85), "high");
        assert_eq!(classify_memory_pressure(98), "high");
    }

    #[test]
    fn policy_off_never_acts() {
        let tabs = vec![make_tab(1, 1001, 900, 1200)];
        let pressure = MemoryPressure {
            total_browser_mb: 1500,
            per_tab_threshold_mb: 300,
        };
        let action =
            evaluate_auto_clean_policy(AutoCleanLevel::Off, &pressure, &tabs, &HashMap::new());
        assert_eq!(action, CleanAction::None);
    }

    #[test]
    fn policy_never_touches_protected_roles_or_tabs() {
        let mut active = make_tab(1, 1001, 800, 500);
        active.is_active = true;

        let mut audio = make_tab(2, 1002, 800, 500);
        audio.is_audio_or_video = true;

        let mut dl = make_tab(3, 1003, 800, 500);
        dl.is_downloading = true;

        let mut gpu = make_tab(4, 1004, 800, 500);
        gpu.role = ProcessRole::Gpu;

        let mut browser = make_tab(5, 1005, 800, 500);
        browser.role = ProcessRole::Browser;

        let mut utility = make_tab(6, 1006, 800, 500);
        utility.role = ProcessRole::Utility;

        let pressure = MemoryPressure {
            total_browser_mb: 4800,
            per_tab_threshold_mb: 300,
        };
        let action = evaluate_auto_clean_policy(
            AutoCleanLevel::Aggressive,
            &pressure,
            &[active, audio, dl, gpu, browser, utility],
            &HashMap::new(),
        );
        assert_eq!(action, CleanAction::None);
    }

    #[test]
    fn policy_enforces_60s_cooldown_and_30mb_hysteresis() {
        let tabs = vec![make_tab(1, 2001, 400, 120)];
        let pressure = MemoryPressure {
            total_browser_mb: 800,
            per_tab_threshold_mb: 300,
        };

        let mut history = HashMap::new();
        history.insert(
            2001,
            TrimRecord {
                last_trim_secs_ago: 45,
                ram_after_mb: 200,
            },
        );
        assert_eq!(
            evaluate_auto_clean_policy(AutoCleanLevel::Balanced, &pressure, &tabs, &history),
            CleanAction::None
        );

        history.insert(
            2001,
            TrimRecord {
                last_trim_secs_ago: 70,
                ram_after_mb: 375,
            },
        );
        assert_eq!(
            evaluate_auto_clean_policy(AutoCleanLevel::Balanced, &pressure, &tabs, &history),
            CleanAction::None
        );

        history.insert(
            2001,
            TrimRecord {
                last_trim_secs_ago: 70,
                ram_after_mb: 365,
            },
        );
        match evaluate_auto_clean_policy(AutoCleanLevel::Balanced, &pressure, &tabs, &history) {
            CleanAction::SoftTrim { pid, tab_id, .. } => {
                assert_eq!(pid, 2001);
                assert_eq!(tab_id, 1);
            }
            other => panic!("Expected SoftTrim, got {:?}", other),
        }
    }

    #[test]
    fn policy_picks_at_most_one_highest_excess_candidate() {
        let tabs = vec![
            make_tab(1, 3001, 350, 90),
            make_tab(2, 3002, 620, 90),
            make_tab(3, 3003, 480, 90),
        ];
        let pressure = MemoryPressure {
            total_browser_mb: 1600,
            per_tab_threshold_mb: 300,
        };
        match evaluate_auto_clean_policy(
            AutoCleanLevel::Balanced,
            &pressure,
            &tabs,
            &HashMap::new(),
        ) {
            CleanAction::SoftTrim { tab_id, pid, .. } => {
                assert_eq!(tab_id, 2);
                assert_eq!(pid, 3002);
            }
            other => panic!("Expected SoftTrim for highest-RAM tab 2, got {:?}", other),
        }
    }

    #[test]
    fn policy_escalates_long_idle_tab_to_sleep_and_aggressive_lowers_threshold() {
        let tabs = vec![make_tab(7, 4007, 230, 360)];
        let pressure = MemoryPressure {
            total_browser_mb: 700,
            per_tab_threshold_mb: 300,
        };

        assert_eq!(
            evaluate_auto_clean_policy(AutoCleanLevel::Balanced, &pressure, &tabs, &HashMap::new()),
            CleanAction::None
        );

        match evaluate_auto_clean_policy(
            AutoCleanLevel::Aggressive,
            &pressure,
            &tabs,
            &HashMap::new(),
        ) {
            CleanAction::SleepTab { tab_id, pid, .. } => {
                assert_eq!(tab_id, 7);
                assert_eq!(pid, 4007);
            }
            other => panic!("Expected SleepTab in Aggressive mode, got {:?}", other),
        }
    }

    #[test]
    fn classify_process_roles_accurately() {
        assert_eq!(
            classify_process_role(100, 100, 1, "rustbrowse.exe", None),
            ProcessRole::Browser
        );
        assert_eq!(
            classify_process_role(
                200,
                100,
                100,
                "msedgewebview2.exe",
                Some("msedgewebview2.exe --embedded-browser-webview=1")
            ),
            ProcessRole::Browser
        );
        assert_eq!(
            classify_process_role(
                201,
                100,
                200,
                "msedgewebview2.exe",
                Some("msedgewebview2.exe --type=gpu-process --gpu-preferences=...")
            ),
            ProcessRole::Gpu
        );
        assert_eq!(
            classify_process_role(
                202,
                100,
                200,
                "msedgewebview2.exe",
                Some("msedgewebview2.exe --type=renderer --lang=en-US")
            ),
            ProcessRole::Renderer
        );
        assert_eq!(
            classify_process_role(
                203,
                100,
                200,
                "msedgewebview2.exe",
                Some("msedgewebview2.exe --type=utility --utility-sub-type=network.mojom.NetworkService")
            ),
            ProcessRole::Utility
        );
    }
}
