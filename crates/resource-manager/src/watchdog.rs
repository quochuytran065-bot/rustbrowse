//! Anti-Freeze, Per-Tab RAM Watchdog & Smart Memory Manager.
//! Specialized for Windows 11 x64 and 4 GB RAM systems.
//! Handles:
//! 1. UI Process Priority Promotion (`ABOVE_NORMAL_PRIORITY_CLASS`)
//! 2. Per-Tab Renderer RAM Budget Monitoring (default: 300 MB, options: 250 / 300 / 400 / 500 MB)
//! 3. Zero-Freeze Soft Trim: trims at most 1 offending background PID per 10s tick
//! 4. 90s Cooldown + 40MB Hysteresis per individual PID (eliminates repetitive stutter loops)
//! 5. Absolute Exclusion List: GPU process, Main UI, Active foreground tab, Audio/Video tabs
//! 6. Mini Task Manager Process Tree Inspection

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{info, warn};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, SetPriorityClass, SetProcessInformation,
    ABOVE_NORMAL_PRIORITY_CLASS, PROCESS_INFORMATION_CLASS,
    PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    PROCESS_SET_QUOTA, PROCESS_VM_READ,
};

#[repr(C)]
#[allow(non_snake_case)]
struct ProcessPowerThrottlingState {
    Version: u32,
    ControlMask: u32,
    StateMask: u32,
}

const PROCESS_POWER_THROTTLING_CURRENT_VERSION: u32 = 1;
const PROCESS_POWER_THROTTLING_EXECUTION_SPEED: u32 = 0x1;
const PROCESS_POWER_THROTTLING: PROCESS_INFORMATION_CLASS = PROCESS_INFORMATION_CLASS(4);

// Configurable Per-Tab RAM Threshold (default 300 MB, options: 250, 300, 400, 500 MB)
static PER_TAB_THRESHOLD_MB: AtomicU64 = AtomicU64::new(300);
static GAMING_MODE: AtomicBool = AtomicBool::new(false);

// Active Tab PID (Protected from working set trim during active browsing)
static ACTIVE_TAB_PID: AtomicU32 = AtomicU32::new(0);

// GPU Process PID (Exempt from trimming)
static GPU_PROCESS_PID: AtomicU32 = AtomicU32::new(0);

// Explicit Protected PIDs (Media, Audio/Video playback tabs, etc.)
static PROTECTED_PIDS: Mutex<Option<HashSet<u32>>> = Mutex::new(None);

// Per-PID Cooldown (min 90s) and Hysteresis (+40MB above post-trim level)
static PID_TRIM_HISTORY: Mutex<Option<HashMap<u32, (Instant, u64)>>> = Mutex::new(None);

// Global Auto Clean Threshold (0 = disabled, defaults to 0 to prevent bulk wiping)
static AUTO_CLEAN_THRESHOLD: AtomicU64 = AtomicU64::new(0);

pub struct Watchdog;

impl Watchdog {
    /// Elevates current UI process priority to ABOVE_NORMAL_PRIORITY_CLASS to eliminate input lag.
    pub fn elevate_ui_priority() {
        unsafe {
            let proc = GetCurrentProcess();
            if let Err(e) = SetPriorityClass(proc, ABOVE_NORMAL_PRIORITY_CLASS) {
                warn!(error = ?e, "Failed to set ABOVE_NORMAL_PRIORITY_CLASS for UI process");
            } else {
                info!("Main UI process promoted to ABOVE_NORMAL_PRIORITY_CLASS (Anti-Freeze active).");
            }
        }
    }

    /// Sets per-tab memory threshold in MB (250, 300, 400, 500).
    pub fn set_per_tab_threshold(mb: u64) {
        PER_TAB_THRESHOLD_MB.store(mb, Ordering::Relaxed);
        info!(threshold_mb = mb, "Per-tab RAM threshold updated.");
    }

    /// Returns current per-tab memory threshold in MB.
    pub fn get_per_tab_threshold() -> u64 {
        PER_TAB_THRESHOLD_MB.load(Ordering::Relaxed)
    }

    /// Sets legacy auto clean threshold (maintained for backward compatibility).
    pub fn set_auto_clean_threshold(mb: u64) {
        AUTO_CLEAN_THRESHOLD.store(mb, Ordering::Relaxed);
    }

    /// Gets legacy auto clean threshold.
    pub fn get_auto_clean_threshold() -> u64 {
        AUTO_CLEAN_THRESHOLD.load(Ordering::Relaxed)
    }

    /// Sets or clears Gaming / Ultra-Low-Power Mode.
    pub fn set_gaming_mode(enable: bool) {
        GAMING_MODE.store(enable, Ordering::Relaxed);
        let root_pid = unsafe { GetCurrentProcessId() };
        if enable {
            Self::execute_tier3_clean(root_pid);
            info!("Gaming / Ultra-Low-Power Mode ENABLED.");
        } else {
            let children = Self::find_descendants(root_pid);
            for &child_pid in &children {
                Self::set_efficiency_mode(child_pid, false);
            }
            info!("Gaming Mode DISABLED. Restored normal priority.");
        }
    }

    /// Returns whether Gaming Mode is currently active.
    pub fn is_gaming_mode() -> bool {
        GAMING_MODE.load(Ordering::Relaxed)
    }

    /// Updates the PID of the foreground active tab renderer.
    pub fn set_active_tab_pid(pid: u32) {
        ACTIVE_TAB_PID.store(pid, Ordering::Relaxed);
    }

    /// Identifies and stores the GPU process PID for absolute trim exemption.
    pub fn set_gpu_pid(pid: u32) {
        GPU_PROCESS_PID.store(pid, Ordering::Relaxed);
    }

    /// Adds a PID to the protected list (e.g. tab playing background audio or video).
    pub fn protect_pid(pid: u32) {
        if pid == 0 {
            return;
        }
        if let Ok(mut lock) = PROTECTED_PIDS.lock() {
            let set = lock.get_or_insert_with(HashSet::new);
            set.insert(pid);
        }
    }

    /// Removes a PID from the protected list when media finishes playing.
    pub fn unprotect_pid(pid: u32) {
        if pid == 0 {
            return;
        }
        if let Ok(mut lock) = PROTECTED_PIDS.lock() {
            if let Some(set) = lock.as_mut() {
                set.remove(&pid);
            }
        }
    }

    /// Checks if a PID is explicitly protected against any RAM trimming.
    pub fn is_pid_protected(pid: u32) -> bool {
        if pid == 0 {
            return true;
        }
        let active = ACTIVE_TAB_PID.load(Ordering::Relaxed);
        if active != 0 && pid == active {
            return true;
        }
        let gpu = GPU_PROCESS_PID.load(Ordering::Relaxed);
        if gpu != 0 && pid == gpu {
            return true;
        }
        let root = unsafe { GetCurrentProcessId() };
        if pid == root {
            return true;
        }

        if let Ok(lock) = PROTECTED_PIDS.lock() {
            if let Some(set) = lock.as_ref() {
                if set.contains(&pid) {
                    return true;
                }
            }
        }
        false
    }

    /// Applies or removes Windows 11 Efficiency Mode (EcoQoS) on a target process.
    pub fn set_efficiency_mode(pid: u32, enable: bool) -> bool {
        unsafe {
            let handle = match OpenProcess(
                PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            ) {
                Ok(h) if !h.is_invalid() => h,
                _ => return false,
            };

            let state = ProcessPowerThrottlingState {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                StateMask: if enable {
                    PROCESS_POWER_THROTTLING_EXECUTION_SPEED
                } else {
                    0
                },
            };

            let res = SetProcessInformation(
                handle,
                PROCESS_POWER_THROTTLING,
                &state as *const _ as *const c_void,
                std::mem::size_of::<ProcessPowerThrottlingState>() as u32,
            );

            let _ = CloseHandle(handle);
            res.is_ok()
        }
    }

    /// Calls K32EmptyWorkingSet to trim unreferenced physical RAM back to the OS.
    pub fn trim_working_set(pid: u32) -> bool {
        unsafe {
            let handle = match OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            ) {
                Ok(h) if !h.is_invalid() => h,
                _ => return false,
            };

            let res = K32EmptyWorkingSet(handle);
            let _ = CloseHandle(handle);
            res.as_bool()
        }
    }

    /// Checks individual PID Cooldown (min 90s) and Hysteresis (+40MB above post-trim level).
    pub fn can_trim_pid(pid: u32, current_ram_mb: u64) -> bool {
        let history = match PID_TRIM_HISTORY.lock() {
            Ok(g) => g,
            Err(_) => return true,
        };

        if let Some(map) = history.as_ref() {
            if let Some(&(last_time, last_mb)) = map.get(&pid) {
                let elapsed = Instant::now().duration_since(last_time);
                // 1. Cooldown constraint: at least 90s must have elapsed
                if elapsed < Duration::from_secs(90) {
                    return false;
                }
                // 2. Hysteresis constraint: RAM must have grown at least +40 MB
                if current_ram_mb < last_mb + 40 {
                    return false;
                }
            }
        }

        true
    }

    /// Records trim timestamp and post-trim memory for a specific PID.
    pub fn record_pid_trim(pid: u32, post_trim_mb: u64) {
        if let Ok(mut history) = PID_TRIM_HISTORY.lock() {
            let map = history.get_or_insert_with(HashMap::new);
            map.insert(pid, (Instant::now(), post_trim_mb));
        }
    }

    /// Smart Per-Tab Soft Trim:
    /// Scans child renderer processes and trims AT MOST ONE offending background PID per cycle.
    /// Strictly protects: GPU process, Main UI process, Active tab, Audio/Video playback tabs.
    pub fn scan_and_soft_trim_per_tab(root_pid: u32) -> Option<(u32, u64)> {
        let children = Self::find_descendants(root_pid);
        let threshold = Self::get_per_tab_threshold();

        // 1. Collect candidate PIDs exceeding individual threshold
        let mut candidates = Vec::new();
        for &child_pid in &children {
            if Self::is_pid_protected(child_pid) {
                continue;
            }

            let ram_bytes = Self::sample_pid_working_set(child_pid);
            let ram_mb = ram_bytes / (1024 * 1024);

            if ram_mb > threshold && Self::can_trim_pid(child_pid, ram_mb) {
                candidates.push((child_pid, ram_mb));
            }
        }

        // 2. Sort candidates by RAM descending (target the single heaviest offender)
        candidates.sort_by(|a, b| b.1.cmp(&a.1));

        // 3. Process AT MOST 1 PID per cycle to prevent disk/CPU I/O stutter
        if let Some(&(target_pid, pre_mb)) = candidates.first() {
            info!(
                target_pid,
                pre_mb,
                threshold_mb = threshold,
                "Smart Per-Tab Trim: Softly trimming single background tab PID..."
            );

            Self::set_efficiency_mode(target_pid, true);
            Self::trim_working_set(target_pid);

            let post_bytes = Self::sample_pid_working_set(target_pid);
            let post_mb = post_bytes / (1024 * 1024);
            Self::record_pid_trim(target_pid, post_mb);

            let freed_mb = pre_mb.saturating_sub(post_mb);
            info!(target_pid, post_mb, freed_mb, "Smart Per-Tab Trim completed.");
            return Some((target_pid, freed_mb));
        }

        None
    }

    /// Legacy Tier 1 clean: routes to safe per-tab trim.
    pub fn execute_tier1_clean(root_pid: u32) -> u64 {
        if let Some((_, freed)) = Self::scan_and_soft_trim_per_tab(root_pid) {
            freed
        } else {
            0
        }
    }

    /// Tier 3 Clean (Minimize / Gaming Mode / Manual Clean):
    /// Trims whole process tree and applies EcoQoS to bring RAM down to rock-bottom (~60-120 MB).
    pub fn execute_tier3_clean(root_pid: u32) -> u64 {
        let (pre_ram, _) = Self::sample_tree_ram_mb();
        let children = Self::find_descendants(root_pid);

        for &child_pid in &children {
            // Still protect active media playing tabs even on minimize!
            if Self::is_pid_protected(child_pid) {
                continue;
            }
            Self::set_efficiency_mode(child_pid, true);
            Self::trim_working_set(child_pid);
        }

        let (post_ram, _) = Self::sample_tree_ram_mb();
        info!(
            pre_ram_mb = pre_ram,
            post_ram_mb = post_ram,
            "Watchdog Tier 3: Process tree trimmed to minimum."
        );
        pre_ram.saturating_sub(post_ram)
    }

    /// Legacy compat wrapper.
    pub fn safe_background_trim(root_pid: u32) {
        Self::scan_and_soft_trim_per_tab(root_pid);
    }

    /// Finds all child/descendant PIDs of the specified root process.
    pub fn find_descendants(root_pid: u32) -> Vec<u32> {
        let mut descendants = Vec::new();
        let mut visited = HashSet::new();

        unsafe {
            let snapshot_res = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            let snapshot: HANDLE = match snapshot_res {
                Ok(h) if !h.is_invalid() => h,
                _ => return descendants,
            };

            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    children_map
                        .entry(entry.th32ParentProcessID)
                        .or_default()
                        .push(entry.th32ProcessID);

                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);

            let mut queue = VecDeque::new();
            queue.push_back(root_pid);

            while let Some(current_pid) = queue.pop_front() {
                if let Some(children) = children_map.get(&current_pid) {
                    for &child_pid in children {
                        if visited.insert(child_pid) {
                            descendants.push(child_pid);
                            queue.push_back(child_pid);
                        }
                    }
                }
            }
        }

        descendants
    }

    /// Measures working set bytes for a single PID.
    pub fn sample_pid_working_set(pid: u32) -> u64 {
        unsafe {
            let handle = match OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
            {
                Ok(h) if !h.is_invalid() => h,
                _ => return 0,
            };

            let mut pmc = PROCESS_MEMORY_COUNTERS_EX {
                cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                ..Default::default()
            };

            let success = K32GetProcessMemoryInfo(
                handle,
                &mut pmc as *mut _ as *mut _,
                pmc.cb,
            );

            let _ = CloseHandle(handle);

            if success.as_bool() {
                pmc.WorkingSetSize as u64
            } else {
                0
            }
        }
    }

    /// Measures total RAM across host + WebView2 descendants.
    pub fn sample_tree_ram_mb() -> (u64, Vec<u32>) {
        let root_pid = unsafe { GetCurrentProcessId() };
        let children = Self::find_descendants(root_pid);

        let mut total_bytes = Self::sample_pid_working_set(root_pid);
        for &child_pid in &children {
            total_bytes = total_bytes.saturating_add(Self::sample_pid_working_set(child_pid));
        }

        (total_bytes / (1024 * 1024), children)
    }

    /// Returns detailed list of processes for Mini Task Manager: (PID, RAM_MB, is_child)
    pub fn get_process_tree_stats(root_pid: u32) -> Vec<(u32, u64, bool)> {
        let mut stats = Vec::new();
        let root_ram = Self::sample_pid_working_set(root_pid) / (1024 * 1024);
        stats.push((root_pid, root_ram, false));

        let children = Self::find_descendants(root_pid);
        for child_pid in children {
            let ram_mb = Self::sample_pid_working_set(child_pid) / (1024 * 1024);
            stats.push((child_pid, ram_mb, true));
        }

        stats
    }
}
