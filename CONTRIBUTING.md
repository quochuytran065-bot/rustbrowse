# Contributing to RustBrowse

Thank you for contributing to **RustBrowse** (Windows 11, Rust + Tauri v2 + Wry + WebView2).
Every pull request and patch must follow the engineering guardrails below so the browser stays fast, crash-isolated, and freeze-free.

---

## 1. Anti-Bug Engineering Guardrails (Mandatory)

### 1.1 Zero Blocking on the UI / IPC Thread
- **Never** perform Win32 process enumeration (`CreateToolhelp32Snapshot`, `K32GetProcessMemoryInfo`), filesystem scans, SQLite writes, or blocking network I/O on the Tauri main thread or directly inside synchronous `#[tauri::command]` handlers.
- Background telemetry (RAM, CPU%, system memory pressure) MUST run on a dedicated background worker thread and expose **O(1) cached snapshots** (e.g., via `RwLock`) to IPC callers.
- Any heavy or non-critical boot task (stale temp cleanup, remote update checks, adblock warming) MUST be deferred asynchronously after the shell window and Tab 1 are interactive.

### 1.2 Never Hold Locks Across `.await` or WebView Calls
- Always drop `std::sync::Mutex` / `RwLock` guards in a tight lexical scope **before** calling `.await`, emitting Tauri events, or invoking `webview.eval(...)` / `webview.set_bounds(...)` / `window.add_child(...)`.
- Holding a mutex across a WebView2 COM call or async suspension point is a primary cause of UI deadlocks and panel freezes.

### 1.3 Explicit Timeouts on All I/O & External Calls
- Every HTTP request (`reqwest::Client`), subprocess (`yt-dlp.exe`), and IPC wait MUST define explicit connect/read timeouts or cancellation tokens (`AtomicBool` / `tokio::time::timeout`).
- Never leave unbounded retry loops or silent hangs on unreachable endpoints (e.g., unconfigured updater manifests).

### 1.4 Zero `.unwrap()` / `.expect()` on User or Runtime Paths
- Never call `.unwrap()` or `.expect()` on user input, URLs, file paths, IPC payloads, mutex locks in hot paths, or optional WebView lookups.
- Return `Result<T, String>` from `#[tauri::command]` handlers, propagate errors cleanly, and handle poisoned locks or missing webviews gracefully.
- Test code (`#[cfg(test)]`) may use `.unwrap()` for concise assertions.

### 1.5 Bounded Buffers & Collections Only
- All in-memory histories, queues, logs, and caches MUST have an explicit upper bound (e.g., `VecDeque` capped at 60 telemetry samples, crash ring buffer capped at 50 entries, closed-tab stack capped at 50 tabs, concurrent downloads capped at 3 via `Semaphore`).
- Unbounded `Vec` / `HashMap` growth in long-running browser sessions is treated as a memory leak bug.

### 1.6 Strict 1:1 Tab Identity & Lifecycle Isolation
- Every tab MUST have a unique monotonic `TabId` strictly paired 1:1 with its child WebView label (`tab-<id>`).
- Creating a new tab MUST NEVER overwrite or navigate an existing tab's WebView, URL, or title.
- Switching tabs MUST only hide the previous WebView and show the target WebView — never reload on switch.
- Do not call `prewarm_spare_tab` during cold startup (spawning multiple WebViews simultaneously degrades cold-boot latency).

### 1.7 Frontend & Injected Script Hygiene
- Never use `opacity: 0` or artificial delays on initial page render in injected scripts (`universal_polish.js`, `web_optimizer.js`).
- Never hijack global wheel events with `{ passive: false }` by default (`lenis_scroll.js`).
- Never run heavy `setInterval` polling or `querySelectorAll('*')` inside `MutationObserver` callbacks; always debounce and schedule via `requestIdleCallback`.
- Respect `prefers-reduced-motion` and keep animations GPU-composited (`transform` and `opacity` only).

---

## 2. Local Development & Verification Workflow

### Toolchain (Windows 11)
```cmd
set RUSTUP_HOME=D:\.rustup
set CARGO_HOME=D:\.cargo
set PATH=D:\.cargo\bin;C:\ProgramData\mingw64\mingw64\bin;%PATH%
```

### Mandatory Pre-Commit Checks
Run all checks below before committing or opening a PR:

```powershell
# 1. Rust formatting, lints (zero warnings), and unit tests
Push-Location src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
Pop-Location

# 2. JavaScript syntax checks
node --check ui/app.js
node --check src-tauri/scripts/lenis_scroll.js
node --check src-tauri/scripts/web_optimizer.js
node --check src-tauri/scripts/universal_polish.js
node --check src-tauri/scripts/video_sniffer.js

# 3. Release build check
Push-Location src-tauri
cargo build --release
Pop-Location
```

---

## 3. Commit & Release Conventions

- Use **Conventional Commits**: `feat(...)`, `fix(...)`, `perf(...)`, `docs(...)`, `chore(...)`, `refactor(...)`, `test(...)`.
- **Never commit** build artifacts (`*.exe`, `*.dll`, `target/`, `dist/`), local signing keys (`*.key`), or user profile data.
- Bump versions atomically across `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`:
  ```powershell
  node scripts/bump_version.mjs <X.Y.Z>
  node scripts/bump_version.mjs --check --expect <X.Y.Z>
  ```
- See [docs/RELEASE.md](docs/RELEASE.md) for signed GitHub Actions release steps and [docs/PERF_BUDGET.md](docs/PERF_BUDGET.md) for performance targets.
