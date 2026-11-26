//! System and process tree memory monitor using Win32 API.

use common::types::SystemMemorySnapshot;
use std::collections::{HashMap, HashSet, VecDeque};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, SetProcessWorkingSetSize,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessMemoryBytes {
    pub working_set_bytes: u64,
    pub private_working_set_bytes: u64,
}

pub struct MemoryMonitor;

impl MemoryMonitor {
    /// Queries total and available physical RAM and system memory load.
    pub fn sample_system_memory() -> Option<SystemMemorySnapshot> {
        unsafe {
            let mut mem_status = MEMORYSTATUSEX {
                dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };

            if GlobalMemoryStatusEx(&mut mem_status).is_ok() {
                let tree_pws_mb = Self::sample_tree_private_working_set_mb();

                Some(SystemMemorySnapshot {
                    total_phys_mb: mem_status.ullTotalPhys / (1024 * 1024),
                    avail_phys_mb: mem_status.ullAvailPhys / (1024 * 1024),
                    memory_load_percent: mem_status.dwMemoryLoad,
                    tree_private_working_set_mb: tree_pws_mb,
                })
            } else {
                None
            }
        }
    }

    /// Measures Private Working Set (MB) of the entire browser process tree (Host + WebView2 renderers).
    pub fn sample_tree_private_working_set_mb() -> u64 {
        let mem = Self::sample_tree_memory();
        mem.private_working_set_bytes / (1024 * 1024)
    }

    /// Measures aggregate Working Set and Private Working Set for host and all descendant processes.
    pub fn sample_tree_memory() -> ProcessMemoryBytes {
        let root_pid = unsafe { GetCurrentProcessId() };
        let descendant_pids = Self::find_descendant_pids(root_pid);

        let mut total_ws = 0u64;
        let mut total_pws = 0u64;

        // Add root process itself
        let mut all_pids = descendant_pids;
        all_pids.insert(root_pid);

        for pid in all_pids {
            let mem = Self::sample_single_process_memory(pid);
            total_ws = total_ws.saturating_add(mem.working_set_bytes);
            total_pws = total_pws.saturating_add(mem.private_working_set_bytes);
        }

        ProcessMemoryBytes {
            working_set_bytes: total_ws,
            private_working_set_bytes: total_pws,
        }
    }

    /// Queries memory counters for a specific PID.
    pub fn sample_single_process_memory(pid: u32) -> ProcessMemoryBytes {
        unsafe {
            let handle_res = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                false,
                pid,
            );

            let handle = match handle_res {
                Ok(h) if !h.is_invalid() => h,
                _ => return ProcessMemoryBytes::default(),
            };

            let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;

            let success = K32GetProcessMemoryInfo(handle, &mut counters as *mut _ as *mut _, size);

            let _ = CloseHandle(handle);

            if success.as_bool() {
                ProcessMemoryBytes {
                    working_set_bytes: counters.WorkingSetSize as u64,
                    private_working_set_bytes: counters.PrivateUsage as u64,
                }
            } else {
                ProcessMemoryBytes::default()
            }
        }
    }

    /// Trims the Working Set of the host RustBrowse process (single call on minimize).
    /// Does NOT call with child WebView2 processes to avoid thrashing renderer paging.
    pub fn trim_host_working_set() -> bool {
        unsafe {
            // Passing usize::MAX to minimum and maximum sizes tells Windows to empty the working set.
            SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX).is_ok()
        }
    }

    /// Builds a parent-child map of all running processes to find all descendants of root_pid.
    fn find_descendant_pids(root_pid: u32) -> HashSet<u32> {
        let mut descendants = HashSet::new();

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

            // Map: Parent PID -> Vec<Child PID>
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

            // Breadth-first search for all descendants of root_pid
            let mut queue = VecDeque::new();
            queue.push_back(root_pid);

            while let Some(current_pid) = queue.pop_front() {
                if let Some(children) = children_map.get(&current_pid) {
                    for &child_pid in children {
                        if descendants.insert(child_pid) {
                            queue.push_back(child_pid);
                        }
                    }
                }
            }
        }

        descendants
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_system_memory() {
        let snap = MemoryMonitor::sample_system_memory();
        assert!(snap.is_some());
        let snap = snap.unwrap();
        assert!(snap.total_phys_mb > 0);
        assert!(snap.avail_phys_mb > 0);
        assert!(snap.memory_load_percent <= 100);
    }

    #[test]
    fn test_sample_current_process_memory() {
        let root_pid = unsafe { GetCurrentProcessId() };
        let mem = MemoryMonitor::sample_single_process_memory(root_pid);
        assert!(mem.working_set_bytes > 0);
        assert!(mem.private_working_set_bytes > 0);
    }

    #[test]
    fn test_tree_memory_sampling() {
        let mem = MemoryMonitor::sample_tree_memory();
        assert!(mem.working_set_bytes > 0);
        assert!(mem.private_working_set_bytes > 0);
    }
}
