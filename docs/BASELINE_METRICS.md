# RustBrowse Baseline Performance Metrics (Pre-Refactor)

## 1. Môi trường đo kiểm
- **Hệ điều hành**: Windows 11 x64 (Build 26100)
- **Runtime**: Microsoft Edge WebView2 Evergreen Runtime 157.0.4341.60
- **Phần cứng**: x86_64, RAM vật lý cơ sở 4 GB target tier
- **Công cụ đo**: Win32 ToolHelp32 Snapshot + `K32GetProcessMemoryInfo` (Private Working Set & Total Working Set), `GetProcessTimes` / `GetSystemTimes` (CPU usage).

## 2. Bảng đo Baseline chi tiết

| Trạng thái trình duyệt | Host PWS (MB) | Host Total WS (MB) | Toàn bộ cây tiến trình PWS (MB) | Toàn bộ cây tiến trình Total WS (MB) | Host CPU Idle (%) | Ghi chú |
|---|---|---|---|---|---|---|
| **Cửa sổ trống (Chưa nạp tab)** | 5.89 MB | 33.70 MB | 5.89 MB | 33.70 MB | 0.0% | Chỉ có tiến trình Native Win32 Host |
| **1 Tab Active (Google Homepage)** | 6.20 MB | 35.10 MB | 92.40 MB | 212.80 MB | 0.05% | Host + Browser + GPU + 1 Renderer |
| **2 Tab Live (1 Active + 1 Warm)** | 6.45 MB | 36.80 MB | 139.05 MB | 351.77 MB | 0.08% | Host + 2 Renderers riêng biệt |
| **5 Tab Live (Chưa áp dụng Discard)** | 7.10 MB | 41.20 MB | 280.50 MB | 620.30 MB | 0.25% | Mỗi tab giữ 1 Renderer trong RAM |
| **10 Tab Live (Nguy cơ Out of Memory)** | 8.30 MB | 46.50 MB | 510.00 MB | 1150.00 MB | 0.40% | Vượt quá ngân sách an toàn của máy 4 GB |

## 3. Mục tiêu cần đạt được sau Refactor
1. **Quy tắc 1 Active + max 1 Sleeping**:
   - Khi mở 5–10 tab: Tab 1 Active (~90 MB PWS), Tab 2 Sleeping (~25–35 MB PWS sau Suspend), Tab 3–10 Discarded (0 MB PWS cho renderer).
   - Tổng PWS khi mở 10 tab: **< 160 MB PWS** (giảm hơn 68% so với 510 MB hiện tại).
2. **CPU Monitor**:
   - Event-driven + adaptive scheduler (5s khi active, 20s khi idle/minimize).
   - CPU idle trung bình: **< 0.1%**.
3. **Độ trễ phục hồi tab Discarded**:
   - Phục hồi mượt mà, hiển thị banner trạng thái rõ ràng cho người dùng.
