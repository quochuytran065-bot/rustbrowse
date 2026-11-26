# RustBrowse Architectural Audit & Baseline Report

## 1. Hiện trạng Codebase hiện tại (Trước Refactor)

Dự án hiện bao gồm 11 crates trong `crates/` và 1 ứng dụng trong `apps/rustbrowse`:
1. `common`: Types (`TabId`, `TabLifecycleState`, `Rect`), Messages, Config, Errors, Clock, Engine trait.
2. `engine-webview2`: COM adapter, Controller, Environment, Events, Navigation, Suspension (`set_memory_usage_target_level`).
3. `shell`: Win32 Native window, TabStrip GDI, BrowserToolbar, AddressBar, Shortcuts, ContextMenu.
4. `tab-manager`: TabStateStore, TabManager, TabPolicy (`evaluate_tab_transition`).
5. `resource-manager`: MemoryMonitor (quét ToolHelp32 cây tiến trình), CpuMonitor (GetSystemTimes/GetProcessTimes), Pressure, Budget, Scheduler.
6. `storage`: SQLite nhúng WAL mode (`rusqlite` bundled) và luồng DbWorker riêng biệt.
7. `session-manager`: Quản lý snapshot tab phiên duyệt và CrashRecoveryDetector.
8. `download-manager`: DownloadTask, sanitize_filename, DownloadManager.
9. `profile-manager`: ProfileManager, BrowserProfile, InPrivate.
10. `privacy-manager`: RuleEngine dùng `adblock`, PermissionManager, TrackerFilter.
11. `telemetry`: BrowserMetrics, SystemDiagnostics, CrashReport.

---

## 2. Các điểm cần Refactor & Cải tiến theo Master Directive

### 2.1. Quản lý Bộ nhớ & Tab Lifecycle
- **Hiện trạng**: Vòng đời tab đang có cấu hình `normal_max_live_tabs = 3`, `warning_max_live_tabs = 2`.
- **Yêu cầu mới**: 
  - Chỉ cho phép **duy nhất 1 Active WebView** và **tối đa 1 Warm/Sleeping WebView**.
  - Toàn bộ các tab thứ 3 đến N (khi người dùng mở 5–10 tab) bắt buộc phải chuyển sang **`Discarded`** (Controller bị đóng và giải phóng hoàn toàn khỏi RAM, chỉ lưu giữ URL, Title, Navigation History, Favicon và Metadata).
  - Không lưu trữ toàn bộ DOM, snapshot màn hình hoặc JS state trong RAM chỉ để phục hồi tab.
  - Loại trừ không Discard: Tab đang phát audio/video (`is_audible`), tab đang download (`is_downloading`), tab ghim (`is_pinned`), hoặc user-protected.
- **Cơ chế Sleep**: 
  - Bỏ việc xen kẽ `MemoryUsageTargetLevel::Low` trên tab đang ngủ. 
  - Sử dụng cơ chế `TrySuspend` / `Resume` của `ICoreWebView2_3` khi tab ẩn vào trạng thái Sleeping.

### 2.2. Tách Crate `adblock-engine` & Request Interceptor
- **Hiện trạng**: `privacy-manager` đang chứa cả logic lọc rule và permission.
- **Yêu cầu mới**:
  - Tạo crate riêng biệt `crates/adblock-engine` với 8 modules theo thiết kế:
    - `engine.rs`: Lõi `adblock::Engine` (Brave adblock-rust 0.13.6).
    - `network_filter.rs`: Khớp URL, domain, resource type, third-party.
    - `cosmetic_filter.rs`: Trích xuất CSS selector ẩn quảng cáo.
    - `filter_lists.rs`: Quản lý nạp danh sách EasyList, EasyPrivacy, uBlock, Custom rules.
    - `site_rules.rs`: Các quy tắc chuyên biệt theo domain (YouTube, Spotify, Facebook, TikTok).
    - `exceptions.rs`: Whitelist / allowlist per site.
    - `rule_cache.rs`: Bộ nhớ đệm nhị phân nạp tức thì không cần biên dịch lại text.
    - `statistics.rs`: Đếm số ads/trackers đã chặn thực tế và danh sách blocked requests.
  - Trong `engine-webview2`:
    - Tạo `request_interceptor.rs`: Bắt `WebResourceRequested` của WebView2, kiểm tra request với `adblock-engine`, trả về `403 Forbidden` rỗng nếu bị chặn.
    - `response_handler.rs`: Xử lý response headers.
    - `page_injection.rs`: Inject CSS cosmetic script để ẩn vùng chứa quảng cáo.

### 2.3. Redesign UI Native theo phong cách Fluent 2 / Windows 11
- **Hiện trạng**: GDI toolbar cơ bản (nút văn bản chữ nhật `<, >, R, H`, Edit box).
- **Yêu cầu mới**:
  - Thiết kế hiện đại lấy cảm hứng từ Windows 11 / Fluent 2:
    - Font Segoe UI / Segoe UI Variable.
    - Bo góc nhẹ (radius 6–8px).
    - Light theme mặc định, Dark theme tùy chọn.
    - Nút điều hướng bo tròn, thanh địa chỉ hiện đại với Security indicator (khiên xanh/khóa), AdBlock shield icon, Download icon, Settings icon.
    - Tab strip phẳng, tab active có nền trắng nổi bật, nút đóng tab 'x', nút thêm tab '+'.
    - Popup AdBlock Shield: Mở khi click icon Shield trên toolbar, hiển thị số ads/trackers đã chặn trên site hiện tại, switch On/Off, nút Whitelist, danh sách URL bị chặn.
    - 8 màn hình/trang nội bộ: Home (giao diện chào buổi sáng, search bar trung tâm, 4 tile YouTube, Spotify, Facebook, TikTok, card Performance), Browser, Bookmarks, History, Downloads (tiến độ, tốc độ, thời gian), Privacy (thống kê tiết kiệm băng thông/thời gian/ads), Performance (số liệu RAM/CPU thực tế), Settings.

---

## 3. Lộ trình triển khai (Execution Order)

1. **Phase 1**: Hoàn thành Audit Report (Tài liệu này).
2. **Phase 2**: Đo RAM/CPU baseline thực tế.
3. **Phase 3**: Refactor `crates/engine-webview2` (TrySuspend/Resume) và `crates/tab-manager` (1 Active + max 1 Sleeping + Discarded rest).
4. **Phase 4**: Xây dựng `crates/adblock-engine` (8 modules với `adblock 0.13.6`).
5. **Phase 5**: Tích hợp site-specific filtering (YouTube, Spotify, Facebook, TikTok) & WebResourceRequested interceptor.
6. **Phase 6**: Redesign UI native Fluent 2 (Modern Tab Strip, Modern Omnibox, Shield Popup, 8 Internal Pages).
7. **Phase 7**: Kiểm thử toàn diện các site mục tiêu và đo lường CPU/RAM overhead (AdBlock ON vs OFF).
8. **Phase 8**: Đóng gói Windows Release x64 và cập nhật tài liệu kỹ thuật.
