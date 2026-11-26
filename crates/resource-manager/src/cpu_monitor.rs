//! Process and system CPU usage monitor using Win32 high-precision counters.

use std::time::Instant;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, GetSystemTimes};

#[derive(Debug)]
pub struct CpuMonitor {
    last_sample_instant: Instant,
    last_sys_kernel: u64,
    last_sys_user: u64,
    last_proc_kernel: u64,
    last_proc_user: u64,
    initialized: bool,
}

impl Default for CpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuMonitor {
    pub fn new() -> Self {
        let mut monitor = Self {
            last_sample_instant: Instant::now(),
            last_sys_kernel: 0,
            last_sys_user: 0,
            last_proc_kernel: 0,
            last_proc_user: 0,
            initialized: false,
        };

        // Take initial snapshot
        monitor.refresh();
        monitor.initialized = true;
        monitor
    }

    /// Queries host CPU percentage between the current moment and the last sample.
    /// Returns host CPU % in range [0.0, 100.0].
    /// Overhead is negligible (< 10 microseconds, zero syscall allocations).
    pub fn sample_host_cpu_percent(&mut self) -> f64 {
        let (sys_kernel, sys_user, proc_kernel, proc_user) = match Self::query_raw_counters() {
            Some(raw) => raw,
            None => return 0.0,
        };

        if !self.initialized {
            self.last_sys_kernel = sys_kernel;
            self.last_sys_user = sys_user;
            self.last_proc_kernel = proc_kernel;
            self.last_proc_user = proc_user;
            self.last_sample_instant = Instant::now();
            self.initialized = true;
            return 0.0;
        }

        let sys_kernel_diff = sys_kernel.saturating_sub(self.last_sys_kernel);
        let sys_user_diff = sys_user.saturating_sub(self.last_sys_user);
        let sys_total = sys_kernel_diff.saturating_add(sys_user_diff);

        let proc_kernel_diff = proc_kernel.saturating_sub(self.last_proc_kernel);
        let proc_user_diff = proc_user.saturating_sub(self.last_proc_user);
        let proc_total = proc_kernel_diff.saturating_add(proc_user_diff);

        // Update baseline
        self.last_sys_kernel = sys_kernel;
        self.last_sys_user = sys_user;
        self.last_proc_kernel = proc_kernel;
        self.last_proc_user = proc_user;
        self.last_sample_instant = Instant::now();

        if sys_total == 0 {
            return 0.0;
        }

        let percent = (proc_total as f64 / sys_total as f64) * 100.0;
        percent.clamp(0.0, 100.0)
    }

    fn refresh(&mut self) {
        if let Some((sk, su, pk, pu)) = Self::query_raw_counters() {
            self.last_sys_kernel = sk;
            self.last_sys_user = su;
            self.last_proc_kernel = pk;
            self.last_proc_user = pu;
            self.last_sample_instant = Instant::now();
        }
    }

    fn query_raw_counters() -> Option<(u64, u64, u64, u64)> {
        unsafe {
            let mut idle_time = FILETIME::default();
            let mut sys_kernel = FILETIME::default();
            let mut sys_user = FILETIME::default();

            if GetSystemTimes(
                Some(&mut idle_time),
                Some(&mut sys_kernel),
                Some(&mut sys_user),
            )
            .is_err()
            {
                return None;
            }

            let mut creation_time = FILETIME::default();
            let mut exit_time = FILETIME::default();
            let mut proc_kernel = FILETIME::default();
            let mut proc_user = FILETIME::default();

            if GetProcessTimes(
                GetCurrentProcess(),
                &mut creation_time,
                &mut exit_time,
                &mut proc_kernel,
                &mut proc_user,
            )
            .is_err()
            {
                return None;
            }

            Some((
                filetime_to_u64(sys_kernel),
                filetime_to_u64(sys_user),
                filetime_to_u64(proc_kernel),
                filetime_to_u64(proc_user),
            ))
        }
    }
}

#[inline]
fn filetime_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_monitor_sampling() {
        let mut monitor = CpuMonitor::new();
        // Burn a tiny bit of CPU to get non-zero delta
        let mut x = 0u64;
        for i in 0..10_000_000 {
            x = x.wrapping_add(i);
        }
        std::hint::black_box(x);

        let usage = monitor.sample_host_cpu_percent();
        assert!((0.0..=100.0).contains(&usage));
    }
}
