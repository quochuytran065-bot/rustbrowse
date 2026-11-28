# RustBrowse Performance Budget & Startup Targets (Repair V6)

This document defines the measurable performance budgets for **RustBrowse** (`v1.0.3`) on Windows 11 x64 (WebView2 Evergreen Runtime, 4 GB RAM target tier) and tracks real measured results against those budgets.

---

## 1. Startup Phase Budget (Cold Boot)

| Startup Phase | Metric Key | Target Budget | V5 Baseline (`#26`) | Repair V6 Implementation & Status |
| :--- | :--- | :--- | :--- | :--- |
| **Process Init → Shell Window Ready** | `t_shell_ready_ms` | **< 1,500 ms** | ~4,460 ms (eager cache purge + dual-WebView setup) | `StartupOptimizer` deferred 15s async after shell setup; HTTP/Code/GPU caches preserved; real `tracing` timestamp `t_shell_ready_ms` instrumented (`#63`). Live release measurement captured by `@verifier`. |
| **Process Init → Tab 1 Interactive** | `t_tab1_ready_ms` | **< 2,500 ms** | ~5,690 ms (blocked by `prewarm_spare_tab` WebView #2) | `prewarm_spare_tab` removed from cold boot; single WebView created first; real `tracing` timestamp `t_tab1_ready_ms` instrumented (`#63`). Live release measurement captured by `@verifier`. |
| **First Meaningful Web Paint Overhead** | Injected CSS/JS delay | **0 ms** | Up to 250 ms (`html { opacity: 0 }` in `universal_polish.js`) | **0 ms** — `html { opacity: 0 }` blank-screen hiding completely removed (`#56`); 12 major image CDNs fast-pathed in `shield.rs`. |
| **Update Check Deferral** | Background check delay | **≥ 8,000 ms** post-boot | ~3,000–5,000 ms post-shell | **8,000 ms** deferred async (`#63`); never blocks shell or Tab 1. |
| **Stale Temp Cleanup Deferral** | `optimizer.rs` delay | **≥ 15,000 ms** post-boot | Immediate on startup | **15,000 ms** deferred async (`#56`); preserves HTTP `Cache`, `Code Cache`, `GPUCache`, `ShaderCache`, `Service Worker/CacheStorage`, Cookies, and Logins. |

---

## 2. Runtime Telemetry & UI Responsiveness Budget

| Subsystem | Metric | Target Budget | Verified Repair V6 Implementation |
| :--- | :--- | :--- | :--- |
| **IPC Snapshot Read (`get_memory_report`)** | Caller latency | **< 1 ms** (O(1) cached) | Strict background worker thread (`watchdog.rs`) + `RwLock`/atomic snapshot; zero `CreateToolhelp32Snapshot` calls on UI/IPC threads (`#56`). |
| **Watchdog Sampling Cadence** | Polling interval | **1,000 ms** (Perf panel open) / **8,000 ms** (closed) | Adaptive cadence via `set_perf_panel_open(bool)` with `Condvar` immediate wakeup + 250ms initial boot yield (`#56`). |
| **Telemetry History Memory** | Buffer size | **≤ 60 samples** (`VecDeque`) | `MAX_HISTORY_SAMPLES = 60` + dead-PID pruning in `cpu_prev`/`trim_history` (`bounded_history_ring_buffer_caps_at_60` passed, `#56`). |
| **Crash Log Ring Buffer** | Buffer size | **≤ 50 events** | Bounded 50-entry `VecDeque<CrashEvent>` + domain-only sanitized `crash.log` (`crash_ring_buffer_is_bounded_and_sanitizes_urls` passed, `#63`). |
| **Overlay Window Activation** | Open latency | **< 30 ms** | Single cached borderless `overlay` WebViewWindow reused across `ram`, `shield`, `videos`, `menu`, `palette` (`#63`). |
| **Tab Switch Latency** | Active switch | **< 16 ms** (1 frame) | Visibility toggle (`hide`/`show`) only — never reloads WebView (`switch_tab_preserves_urls_and_never_reloads_or_overwrites` passed, `#63`). |
| **Concurrent Video Downloads** | Active workers | **Max 3** (`Semaphore`) | Caps network & disk I/O contention; HLS segments capped at 4 per download (`video_downloader.rs`). |

---

## 3. Memory & CPU Budget (4 GB Target Tier)

Baseline measurements from [BASELINE_METRICS.md](BASELINE_METRICS.md) (Windows 11 x64 Build 26100, WebView2 Evergreen):

| Browser State | Pre-Refactor Total Tree PWS | Repair V6 Target | Enforcement Mechanism |
| :--- | :--- | :--- | :--- |
| **1 Active Tab** | 92.40 MB | **< 110 MB PWS** | Single WebView on cold boot (no duplicate spare WebView) |
| **5–10 Tabs (Auto-Clean Balanced)** | 280.50 – 510.00 MB | **< 160 MB PWS** | Incremental auto-clean policy (max 1 action/cycle, 60s cooldown, +30 MB hysteresis; active/audio/download/GPU/browser processes exempt; 9/9 `watchdog::tests` passing) |
| **20+ Tabs Open** | Uncapped / OOM risk | **Zero OOM / UI Responsive** | Unlimited tab strip with permanently pinned `+`, lazy session restore (0 MB until clicked), sleeping/discarded background tabs (`create_1_to_20_tabs_preserves_monotonic_ids_labels_and_old_urls` passing) |
| **Idle Host CPU** | 0.05 – 0.40% | **< 0.5% idle** | Real `GetProcessTimes` OS CPU% monitoring; 8s relaxed watchdog cadence when Perf panel closed; zero `setInterval` DOM polling in injected scripts |

---

## 4. Build & Binary Size Budget

| Artifact | V5 Baseline (`#26`) | Repair V6 Target | Build Profile Configuration (`Cargo.toml`) |
| :--- | :--- | :--- | :--- |
| `rustbrowse.exe` (release, stripped) | 13.48 MiB | **< 18 MiB** | `opt-level = 3`, `strip = true`, `lto = "thin"` (`#63`) |
| `RustBrowse_*_x64-setup.exe` (NSIS) | 4.57 MiB | **< 8 MiB** | Compressed NSIS installer + signed updater manifest in GitHub Actions |
| Dev incremental rebuild speed | Slow full-debug deps | Fast incremental | `[profile.dev.package."*"] opt-level = 2` (`#63`) |
