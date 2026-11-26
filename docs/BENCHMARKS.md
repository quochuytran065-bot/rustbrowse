# Benchmarks & Verified Measurements

## Phase 0 Spike Results (Measured on Windows 11 x64, WebView2 Runtime 157.0.4341.60)
- **Configuration**: 1 Native Win32 Window + 2 WebView2 Controllers sharing 1 Environment.
- **Process Memory Breakdown**:

| PID | Process Name | Working Set (MB) | Private Working Set / Commit (MB) | Role |
|---|---|---|---|---|
| 14552 | `spike_check.exe` | 33.70 | 5.89 | Native Win32 Shell Host |
| 17112 | `msedgewebview2.exe` | 100.79 | 34.90 | Browser Process / Network |
| 16964 | `msedgewebview2.exe` | 8.88 | 2.41 | Crashpad / Monitor |
| 13176 | `msedgewebview2.exe` | 28.70 | 10.09 | Utility Process |
| 7048 | `msedgewebview2.exe` | 71.61 | 41.35 | GPU Process |
| 1060 | `msedgewebview2.exe` | 17.37 | 6.95 | Storage / Service Worker |
| 6208 | `msedgewebview2.exe` | 46.41 | 18.85 | Renderer Process (Tab 1 Active) |
| 404 | `msedgewebview2.exe` | 44.32 | 18.59 | Renderer Process (Tab 2 Background) |

### Aggregate Totals
- **Total Process Tree Working Set**: **351.77 MB**
- **Total Process Tree Private Working Set**: **139.05 MB**
- **Native Shell Overhead**: **5.89 MB Private WS** (well under the 80 MB target ceiling)
- **Tab Switch Latency**: **42.2 ms** (p95 target < 150 ms)

## Performance Acceptance Targets
- Active -> Warm Tab Switch: p95 < 16 ms
- Sleeping -> Active Resume: p95 < 150 ms (Achieved 42.2 ms in prototype)
- Startup to First Frame: < 1.5 s
- Shell Native Idle RAM: < 80 MB (Achieved 33.70 MB Working Set)
- Idle CPU: < 0.5%
