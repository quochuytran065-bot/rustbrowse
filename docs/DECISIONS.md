# Architectural Decision Records (ADR)

## ADR-001: Native Win32 Shell vs Web/Tauri Shell
- **Status**: Accepted
- **Context**: 4 GB RAM machines have zero tolerance for redundant renderers. A web-based shell (Tauri/Electron) consumes 40–70 MB private RAM just for the UI chrome.
- **Decision**: Retain Native Win32 shell. Measured shell private working set is only 5.89 MB (Total WS 33.70 MB).

## ADR-002: Dual WebView2 Crate Strategy (`windows-webview` + `webview2-com`)
- **Status**: Accepted
- **Context**: `windows-webview 0.100.0` provides safe RAII bindings for Environment and Controller with Profile and InPrivate options, but leaves `TrySuspend`/`Resume` as raw vtable offsets. `webview2-com 0.39.1` provides complete typed COM interfaces for all WebView2 versions.
- **Decision**: Combine `windows-webview` for high-level controller/environment management and `webview2-com` within `engine-webview2` for complete COM operations.

## ADR-003: Single Environment & Shared User Data Folder
- **Status**: Accepted
- **Context**: Creating separate environments per tab duplicates browser processes and GPU processes.
- **Decision**: All tabs share one `ICoreWebView2Environment` and one User Data Folder (`%LOCALAPPDATA%\RustBrowse\User Data`), sharing cookies and auth sessions across tabs seamlessly.

## ADR-004: Workspace & Toolchain Isolation on Drive D:
- **Status**: Accepted
- **Context**: Strict requirement from operator to operate on drive D: and leave drive C: clean.
- **Decision**: `RUSTUP_HOME = "D:\.rustup"`, `CARGO_HOME = "D:\.cargo"`, project root at `D:\RustBrowse`.

## ADR-005: Embedded SQLite Storage (`rusqlite` with WAL Mode & Dedicated Worker Thread)
- **Status**: Accepted
- **Context**: Storage (history, bookmarks, crash recovery session tabs) must be crash-resilient, zero-external-dependency, and never block the STA Win32 UI thread.
- **Decision**: Use `rusqlite` with feature `bundled` (C SQLite compiled in, no external DLL needed). Configure SQLite with `PRAGMA journal_mode=WAL`, `PRAGMA synchronous=NORMAL`, `PRAGMA busy_timeout=5000`. Offload all disk operations to a dedicated background database worker thread with a 2-second debounced write queue, communicating via `tokio::sync::mpsc` channels and oneshot reply channels.

## ADR-006: Adaptive Resource Scheduling & Process Tree Memory Accounting
- **Status**: Accepted
- **Context**: 4 GB RAM systems require aggressive tab suspension and discarding when memory is constrained. Periodic polling must consume < 0.1% CPU when idle. Measuring only the host process (RustBrowse.exe) is insufficient because WebView2 allocates renderers and utility processes out-of-process (`msedgewebview2.exe`).
- **Decision**: 
  1. Implement `MemoryMonitor` using `CreateToolhelp32Snapshot` to walk the process tree and aggregate `WorkingSetSize` and `PrivateUsage` across host + child renderers.
  2. Implement `CpuMonitor` using Win32 `GetProcessTimes` and `GetSystemTimes` for microsecond-level CPU measurement without process allocations.
  3. Implement `ResourceScheduler` throttling measurement frequency dynamically: 5 seconds when visible, 20 seconds when minimized or idle.
  4. Implement `MemoryBudget` enforcing live tab caps: 4 GB tier caps to 3 live tabs in Normal mode, 2 live tabs in Warning mode, and 1 live tab in Critical mode (with LRU non-protected background tab discard).
  5. Support single host working set trim on minimize (`trim_on_minimize`), leaving child WebView2 processes untrimmed to avoid disk thrashing.
