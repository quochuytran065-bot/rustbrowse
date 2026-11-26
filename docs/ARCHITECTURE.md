# RustBrowse Architecture Specification

## 1. Threading Model
- **UI Thread (STA)**:
  - Owns Win32 message loop (`GetMessageW` / `DispatchMessageW`).
  - Owns all WebView2 `Controller` instances and COM objects.
  - Receives `UiCommand` from background workers.
  - Emits `EngineEvent` to background workers.
- **Tokio Runtime (1–2 Workers)**:
  - Handles disk I/O, adblock filter updates, scheduled timers.
- **Database Thread**:
  - Dedicated SQLite thread running in WAL mode with batch debouncing (2 seconds).
  - Never blocks UI thread.

## 2. Multi-Process Architecture
```
RustBrowse.exe (Native Shell Host - STA)
   │
   ├── msedgewebview2.exe (Browser Process / Network Service)
   ├── msedgewebview2.exe (GPU Process - Hardware Acceleration)
   ├── msedgewebview2.exe (Renderer Process - Tab 1 Active)
   └── msedgewebview2.exe (Renderer Process - Tab 2 Warm/Sleeping)
```
- **Single Shared Environment**: All tabs share one `ICoreWebView2Environment` and one User Data Folder (`%LOCALAPPDATA%\RustBrowse\User Data`).
- **No Extra UI Renderer**: Native Win32 window handles frame, tabs, and navigation bar without spinning up an extra Chromium renderer process.

## 3. Communication Contract
- Background channels use `tokio::sync::mpsc`.
- `Engine` trait abstracts all WebView2 operations so business logic remains 100% testable without live COM runtimes.
