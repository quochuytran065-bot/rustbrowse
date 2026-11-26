# RustBrowse

Desktop browser for Windows 10/11 x64 written in Rust, powered by Microsoft Edge WebView2 Evergreen Runtime, engineered specifically for resource-constrained systems (4 GB RAM target tier).

## Priorities
1. **Stability & Security**: strict process isolation, no administrative privileges, memory safety.
2. **Low RAM Footprint**: multi-state tab lifecycle (`Active -> Warm -> Sleeping -> Discarded`), aggressive memory throttling.
3. **Responsive UI**: native Win32 window and shell, sub-50ms tab switching, non-blocking UI thread.
4. **Zero Idle CPU**: event-driven background monitors, no polling loops.

## Workspace Layout
- `apps/rustbrowse`: Application entry point, bootstrap, lifecycle.
- `crates/common`: Core types, messages, contracts, trait `Engine`, trait `Clock`.
- `crates/engine-webview2`: WebView2 COM engine, environment, controller, navigation, suspension.
- `crates/shell`: Win32 native windowing, HiDPI scaling, shortcuts.
- `crates/tab-manager`: Pure tab lifecycle evaluation and store.
- `crates/resource-manager`: Memory & CPU monitoring, pressure evaluation.
- `crates/download-manager`: Download tracking and filename sanitization.
- `crates/privacy-manager`: Omnibox normalization, tracker filtering, permissions.
- `crates/profile-manager`: User data folders and InPrivate profiles.
- `crates/storage`: SQLite persistent storage.
- `crates/telemetry`: Diagnostics and crash reports.

## Building and Testing
```bash
cargo check --workspace
cargo test --workspace
cargo run -p rustbrowse
```
