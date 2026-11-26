# Third-Party Notices & Attributions

RustBrowse builds upon and draws architectural inspiration from the following open-source projects and specifications. All third-party trademarks and copyrights belong to their respective owners.

---

## Bundled / Linked Open-Source Dependencies

### 1. Tauri v2, Wry, and Tao
- **Project**: [https://tauri.app](https://tauri.app)
- **License**: Dual-licensed under Apache License 2.0 or MIT License
- **Usage**: Native desktop shell, multi-webview management, IPC bridge, NSIS bundler, and signed auto-updater (`tauri`, `tauri-build`, `tauri-plugin-single-instance`, `tauri-plugin-updater`, `tauri-plugin-process`).

### 2. adblock-rust (Brave Adblock Engine)
- **Project**: [https://github.com/brave/adblock-rust](https://github.com/brave/adblock-rust)
- **License**: Mozilla Public License 2.0 (MPL-2.0)
- **Usage**: Native Rust ad and tracker network/cosmetic filter engine powering RustBrowse ActiveShield (`adblock` crate).

### 3. Rust Ecosystem Crates
- **tokio** (MIT) — Async runtime for downloads, IPC, and background tasks.
- **reqwest** (Apache-2.0 / MIT) — Async HTTP streaming client (Windows native Schannel TLS).
- **futures-util** (Apache-2.0 / MIT) — Async stream combinators.
- **rusqlite** (MIT) — Embedded SQLite session storage.
- **serde / serde_json** (Apache-2.0 / MIT) — Serialization for IPC and configuration.
- **windows-rs** (Apache-2.0 / MIT) — Win32 process/memory telemetry APIs.
- **tracing / tracing-subscriber** (MIT) — Structured diagnostics and logging.
- **url / regex / aho-corasick** (Apache-2.0 / MIT) — URL parsing and fast pattern matching.

---

## On-Demand External Helper (Not Bundled)

### 4. yt-dlp
- **Project**: [https://github.com/yt-dlp/yt-dlp](https://github.com/yt-dlp/yt-dlp)
- **License**: The Unlicense (Public Domain)
- **Usage**: Optional external video downloader helper for non-DRM websites. RustBrowse does **not** bundle `yt-dlp.exe` in its installer; when requested by the user via the 1-click install action, RustBrowse downloads the official release binary directly from GitHub into `%LOCALAPPDATA%\RustBrowse\bin\yt-dlp.exe`. RustBrowse never bypasses DRM/EME protection.

---

## Design & Motion References (Reference-Only — No Code Vendored)

### 5. Lenis (Smooth Scroll)
- **Project**: [https://github.com/darkroomengineering/lenis](https://github.com/darkroomengineering/lenis)
- **License**: MIT License
- **Usage**: Smooth wheel-scroll physics and interpolation concepts adapted into lightweight zero-dependency browser script (`src-tauri/scripts/lenis_scroll.js`).

### 6. open-design
- **Project**: [https://github.com/nicholasgriffintn/open-design](https://github.com/nicholasgriffintn/open-design) (and open design-token specifications)
- **License**: Apache License 2.0
- **Usage**: **Reference-only** inspiration for semantic design token hierarchy, typography scale, and high-contrast accessible color roles in `DESIGN.md` and `ui/tokens.css`. No source files or packages are copied or vendored.

### 7. animate-ui
- **Project**: [https://animate-ui.com](https://animate-ui.com)
- **License**: Commons Clause / Proprietary UI kit terms (reference-only notice)
- **Usage**: **Reference-only** conceptual inspiration for spring-like easing curves (`cubic-bezier(0.22, 1, 0.36, 1)`), 140–220ms micro-interaction timings, and `prefers-reduced-motion` discipline implemented from scratch in pure zero-dependency CSS/WAAPI. **Zero** `animate-ui` source code, React components, Tailwind classes, or npm packages are copied, vendored, or distributed in RustBrowse.
