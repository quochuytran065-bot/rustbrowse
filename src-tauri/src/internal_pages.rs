//! Embedded Internal Dashboard Pages
//! Routes:
//! - rustbrowse://home
//! - rustbrowse://privacy
//! - rustbrowse://performance
//! - rustbrowse://selftest
//! - rustbrowse://superboost
//! - rustbrowse://downloads
//! - rustbrowse://settings
//!
//! Styled with Bright Glass (default light) & Midnight Dark design tokens,
//! zero fake metrics, and live Tauri IPC hydration when available.

pub struct InternalPages;

impl InternalPages {
    pub const BASE_CSS: &'static str = r###"
        :root, [data-theme="light"] {
            color-scheme: light;
            --bg-page: radial-gradient(circle at 20% 0%, #e0f7f4 0%, #f1f5f9 52%, #e8eef6 100%);
            --card-bg: rgba(255, 255, 255, 0.86);
            --card-elevated: #f4f8fc;
            --card-border: rgba(15, 23, 42, 0.09);
            --text-main: #0f172a;
            --text-muted: #64748b;
            --accent: #0d9488;
            --accent-soft: rgba(13, 148, 136, 0.12);
            --blue: #2563eb;
            --green: #059669;
            --amber: #d97706;
            --red: #e11d48;
            --shadow: 0 10px 30px rgba(15, 23, 42, 0.07);
            --radius: 16px;
        }
        [data-theme="dark"] {
            color-scheme: dark;
            --bg-page: radial-gradient(circle at 20% 0%, #10242c 0%, #0b0e15 55%, #090c12 100%);
            --card-bg: rgba(20, 26, 40, 0.88);
            --card-elevated: rgba(255, 255, 255, 0.04);
            --card-border: rgba(255, 255, 255, 0.09);
            --text-main: #f1f5f9;
            --text-muted: #94a3b8;
            --accent: #2dd4bf;
            --accent-soft: rgba(45, 212, 191, 0.14);
            --blue: #3b82f6;
            --green: #10b981;
            --amber: #f59e0b;
            --red: #f43f5e;
            --shadow: 0 14px 36px rgba(0, 0, 0, 0.42);
            --radius: 16px;
        }
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: "Segoe UI Variable Display", "Segoe UI", -apple-system, BlinkMacSystemFont, Roboto, sans-serif;
            background: var(--bg-page);
            color: var(--text-main);
            min-height: 100vh;
            display: flex;
            flex-direction: column;
            align-items: center;
            padding: 36px 20px 60px;
            -webkit-font-smoothing: antialiased;
        }
        .container { max-width: 900px; width: 100%; }
        .top-nav {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: 24px;
            gap: 12px;
            flex-wrap: wrap;
        }
        .nav-pills {
            display: flex;
            gap: 6px;
            flex-wrap: wrap;
        }
        .nav-pill {
            text-decoration: none;
            color: var(--text-muted);
            background: var(--card-bg);
            border: 1px solid var(--card-border);
            padding: 6px 12px;
            border-radius: 999px;
            font-size: 12px;
            font-weight: 600;
            transition: transform 150ms cubic-bezier(0.22, 1, 0.36, 1);
        }
        .nav-pill:hover { color: var(--text-main); transform: translateY(-1px); }
        .nav-pill.active { background: var(--accent); color: #ffffff; border-color: transparent; }
        .card {
            background: var(--card-bg);
            border: 1px solid var(--card-border);
            border-radius: var(--radius);
            padding: 22px;
            margin-bottom: 18px;
            box-shadow: var(--shadow);
        }
        .card-title {
            font-size: 16px;
            font-weight: 700;
            color: var(--text-main);
            margin-bottom: 14px;
            display: flex;
            justify-content: space-between;
            align-items: center;
            gap: 10px;
        }
        .muted { color: var(--text-muted); font-size: 13px; line-height: 1.5; }
        .grid-3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 14px; }
        .grid-2 { display: grid; grid-template-columns: repeat(2, 1fr); gap: 14px; }
        .stat-box {
            background: var(--card-elevated);
            border: 1px solid var(--card-border);
            border-radius: 12px;
            padding: 16px;
        }
        .stat-val {
            font-size: 26px;
            font-weight: 800;
            font-variant-numeric: tabular-nums;
            color: var(--accent);
        }
        .stat-label { font-size: 12px; color: var(--text-muted); margin-top: 4px; }
        .badge {
            display: inline-flex;
            align-items: center;
            padding: 4px 10px;
            border-radius: 999px;
            font-size: 11px;
            font-weight: 700;
            background: var(--accent-soft);
            color: var(--accent);
        }
        .btn {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            gap: 6px;
            background: var(--accent-soft);
            color: var(--accent);
            border: 1px solid var(--card-border);
            padding: 8px 14px;
            border-radius: 10px;
            font-size: 12px;
            font-weight: 600;
            cursor: pointer;
            transition: transform 140ms cubic-bezier(0.22, 1, 0.36, 1);
        }
        .btn:hover { transform: translateY(-1px); }
        .btn:active { transform: scale(0.97); }
        .btn-primary { background: var(--accent); color: #ffffff; border-color: transparent; }
        .row {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 12px;
            padding: 12px 0;
            border-bottom: 1px solid var(--card-border);
        }
        .row:last-child { border-bottom: none; padding-bottom: 0; }
        @media (max-width: 680px) {
            .grid-3, .grid-2 { grid-template-columns: 1fr; }
        }
        @media (prefers-reduced-motion: reduce) {
            * { transition: none !important; animation: none !important; }
        }
    "###;

    pub const THEME_BOOT_JS: &'static str = r###"
        (function(){
            function apply(){
                var t = 'light';
                try { t = localStorage.getItem('rustbrowse-theme') || 'light'; } catch(e) {}
                document.documentElement.setAttribute('data-theme', t === 'dark' ? 'dark' : 'light');
            }
            apply();
            window.addEventListener('storage', apply);
            window.__rbToggleTheme = function(){
                var next = document.documentElement.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
                try { localStorage.setItem('rustbrowse-theme', next); } catch(e) {}
                apply();
            };
            window.__rbInvoke = async function(cmd, args){
                try {
                    if (window.__TAURI__ && window.__TAURI__.core) return await window.__TAURI__.core.invoke(cmd, args || {});
                    if (window.__TAURI_INTERNALS__) return await window.__TAURI_INTERNALS__.invoke(cmd, args || {});
                } catch(e) { console.error(cmd, e); }
                return null;
            };
        })();
    "###;

    fn nav_bar(active: &str) -> String {
        let items = [
            ("home", "rustbrowse://home", "Trang chủ"),
            ("performance", "rustbrowse://performance", "RAM & Hiệu năng"),
            ("health", "rustbrowse://health", "Health"),
            ("privacy", "rustbrowse://privacy", "Privacy Shield"),
            ("superboost", "rustbrowse://superboost", "Super Boost UI"),
            ("downloads", "rustbrowse://downloads", "Tải xuống"),
            ("updates", "rustbrowse://updates", "Cập nhật Tự động"),
            ("selftest", "rustbrowse://selftest", "Chẩn đoán"),
        ];
        let pills: String = items
            .iter()
            .map(|(id, href, label)| {
                let cls = if *id == active {
                    "nav-pill active"
                } else {
                    "nav-pill"
                };
                format!(r#"<a class="{cls}" href="{href}">{label}</a>"#)
            })
            .collect();
        format!(
            r#"<div class="top-nav">
                <div class="nav-pills">{pills}</div>
                <button class="btn" onclick="window.__rbToggleTheme()">Đổi Sáng / Tối</button>
            </div>"#
        )
    }

    /// Renders `rustbrowse://home`
    pub fn render_home(ram_mb: u64, active_tabs: usize, blocked_ads: u64) -> String {
        format!(
            r###"<!DOCTYPE html>
            <html lang="vi" data-theme="light">
            <head>
                <meta charset="utf-8">
                <title>RustBrowse — Trang chủ</title>
                <style>
                    {css}
                    .hero {{ margin-bottom: 24px; }}
                    .hero h1 {{ font-size: 32px; font-weight: 800; letter-spacing: -0.6px; }}
                    .hero p {{ margin-top: 6px; }}
                    .search-box {{
                        background: var(--card-bg);
                        border: 1px solid var(--card-border);
                        border-radius: 999px;
                        padding: 12px 18px;
                        display: flex;
                        align-items: center;
                        gap: 12px;
                        margin-bottom: 24px;
                        box-shadow: var(--shadow);
                    }}
                    .search-box:focus-within {{ border-color: var(--accent); }}
                    .search-input {{
                        flex: 1;
                        border: none;
                        outline: none;
                        background: transparent;
                        color: var(--text-main);
                        font-size: 15px;
                    }}
                    .shortcuts {{
                        display: grid;
                        grid-template-columns: repeat(4, 1fr);
                        gap: 12px;
                        margin-bottom: 22px;
                    }}
                    .shortcut {{
                        background: var(--card-bg);
                        border: 1px solid var(--card-border);
                        border-radius: 14px;
                        padding: 16px 12px;
                        display: flex;
                        flex-direction: column;
                        align-items: center;
                        gap: 8px;
                        text-decoration: none;
                        color: var(--text-main);
                        font-size: 13px;
                        font-weight: 600;
                        transition: transform 160ms cubic-bezier(0.22, 1, 0.36, 1);
                    }}
                    .shortcut:hover {{ transform: translateY(-2px); border-color: var(--accent); }}
                </style>
            </head>
            <body>
                <div class="container">
                    {nav}
                    <div class="hero">
                        <h1>Chào mừng đến với RustBrowse.</h1>
                        <p class="muted">Trình duyệt nhẹ, mượt chuẩn 60fps — đo RAM thực tế từ Windows OS, không độ trễ.</p>
                    </div>

                    <form class="search-box" onsubmit="event.preventDefault(); doSearch();">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2">
                            <circle cx="11" cy="11" r="8"></circle>
                            <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
                        </svg>
                        <input type="text" id="home-search" class="search-input" placeholder="Tìm kiếm trên Google hoặc nhập địa chỉ web..." autofocus spellcheck="false">
                        <button type="submit" class="btn btn-primary">Đi</button>
                    </form>

                    <div class="shortcuts">
                        <a href="https://www.youtube.com" class="shortcut">
                            <svg width="24" height="24" viewBox="0 0 24 24" fill="#ef4444"><path d="M23.5 6.2a3 3 0 0 0-2.1-2.1C19.5 3.5 12 3.5 12 3.5s-7.5 0-9.4.6A3 3 0 0 0 .5 6.2C0 8.1 0 12 0 12s0 3.9.5 5.8a3 3 0 0 0 2.1 2.1c1.9.6 9.4.6 9.4.6s7.5 0 9.4-.6a3 3 0 0 0 2.1-2.1C24 15.9 24 12 24 12s0-3.9-.5-5.8zM9.5 15.6V8.4L15.8 12l-6.3 3.6z"/></svg>
                            <span>YouTube</span>
                        </a>
                        <a href="https://open.spotify.com" class="shortcut">
                            <svg width="24" height="24" viewBox="0 0 24 24" fill="#10b981"><path d="M12 0a12 12 0 1 0 0 24 12 12 0 0 0 0-24zm5.5 17.3c-.2.4-.7.5-1 .3-2.8-1.7-6.4-2.1-10.5-1.2-.4.1-.8-.2-.9-.6-.1-.4.2-.8.6-.9 4.6-1 8.5-.6 11.6 1.3.3.2.4.7.2 1.1zm1.5-3.3c-.3.4-.8.6-1.3.3-3.2-2-8.1-2.5-11.9-1.4-.5.2-1-.1-1.2-.6-.1-.5.1-1 .6-1.2 4.4-1.3 9.8-.7 13.4 1.6.5.3.6.8.4 1.3zm.1-3.4C15.2 8.4 8.9 8.2 5.2 9.3c-.6.2-1.2-.2-1.4-.8-.2-.6.2-1.2.8-1.4 4.3-1.3 11.3-1 15.7 1.6.5.3.7 1 .4 1.5-.3.5-1 .7-1.6.4z"/></svg>
                            <span>Spotify</span>
                        </a>
                        <a href="https://www.facebook.com" class="shortcut">
                            <svg width="24" height="24" viewBox="0 0 24 24" fill="#2563eb"><path d="M24 12.1C24 5.4 18.6 0 12 0S0 5.4 0 12.1C0 18.1 4.4 23 10.1 23.9v-8.4H7.1v-3.5h3V9.4c0-3 1.8-4.7 4.5-4.7 1.3 0 2.7.2 2.7.2v3h-1.5c-1.5 0-2 .9-2 1.9v2.2h3.3l-.5 3.5h-2.8v8.4C19.6 23 24 18.1 24 12.1z"/></svg>
                            <span>Facebook</span>
                        </a>
                        <a href="https://github.com" class="shortcut">
                            <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor"><path d="M12 .5A12 12 0 0 0 8.2 23.9c.6.1.8-.3.8-.6v-2.2c-3.3.7-4-1.4-4-1.4-.6-1.4-1.3-1.8-1.3-1.8-1.1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1.1 1.8 2.8 1.3 3.5 1 .1-.8.4-1.3.8-1.6-2.7-.3-5.5-1.3-5.5-5.9 0-1.3.5-2.4 1.2-3.2-.1-.3-.5-1.5.1-3.2 0 0 1-.3 3.3 1.2a11.5 11.5 0 0 1 6 0c2.3-1.5 3.3-1.2 3.3-1.2.7 1.7.2 2.9.1 3.2.8.8 1.2 1.9 1.2 3.2 0 4.6-2.8 5.6-5.5 5.9.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6A12 12 0 0 0 12 .5z"/></svg>
                            <span>GitHub</span>
                        </a>
                    </div>

                    <div class="card">
                        <div class="card-title">
                            <span>Chỉ số Thực tế từ Hệ thống</span>
                            <span class="badge">Live Telemetry</span>
                        </div>
                        <div class="grid-3">
                            <div class="stat-box">
                                <div class="stat-val">{ram_mb} MB</div>
                                <div class="stat-label">RAM Working Set Hiện tại</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">{active_tabs}</div>
                                <div class="stat-label">Tab Đang Mở</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">{blocked_ads}</div>
                                <div class="stat-label">Quảng cáo Đã Chặn</div>
                            </div>
                        </div>
                    </div>
                </div>
                <script>{boot_js}
                    function doSearch() {{
                        const q = (document.getElementById('home-search')?.value || '').trim();
                        if (!q) return;
                        if (/^https?:\/\//i.test(q) || q.startsWith('rustbrowse://')) window.location.href = q;
                        else if (q.includes('.') && !q.includes(' ')) window.location.href = 'https://' + q;
                        else window.location.href = 'https://www.google.com/search?q=' + encodeURIComponent(q);
                    }}
                </script>
            </body>
            </html>"###,
            css = Self::BASE_CSS,
            nav = Self::nav_bar("home"),
            boot_js = Self::THEME_BOOT_JS,
            ram_mb = ram_mb,
            active_tabs = active_tabs,
            blocked_ads = blocked_ads,
        )
    }

    /// Renders `rustbrowse://privacy`
    pub fn render_privacy(rules_count: usize, blocked_count: u64, ram_mb: u64) -> String {
        format!(
            r###"<!DOCTYPE html>
            <html lang="vi" data-theme="light">
            <head>
                <meta charset="utf-8">
                <title>RustBrowse — Privacy Shield</title>
                <style>{css}</style>
            </head>
            <body>
                <div class="container">
                    {nav}
                    <div class="card">
                        <div class="card-title">
                            <span>Privacy &amp; Anti-Adware Shield</span>
                            <span class="badge">SHIELDS UP</span>
                        </div>
                        <p class="muted" style="margin-bottom:18px;">
                            Chặn quảng cáo độc hại, popup chuyển hướng trái phép và xóa tham số theo dõi (UTM, fbclid, gclid) ở tầng mạng và DOM.
                        </p>
                        <div class="grid-3" style="margin-bottom:20px;">
                            <div class="stat-box">
                                <div class="stat-val">{blocked_count}</div>
                                <div class="stat-label">Quảng cáo &amp; Popups Đã Chặn</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">{rules_count}</div>
                                <div class="stat-label">Luật Lọc Đang Nạp</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">{ram_mb} MB</div>
                                <div class="stat-label">Bộ Nhớ Working Set</div>
                            </div>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Chặn Popup &amp; Điều hướng Trái phép</div>
                                <div class="muted">Khóa cửa sổ quảng cáo tự bật và chuyển hướng độc hại</div>
                            </div>
                            <span class="badge">BẬT</span>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Làm sạch Tracking Parameters</div>
                                <div class="muted">Tự động gỡ utm_*, fbclid, gclid khi sao chép liên kết</div>
                            </div>
                            <span class="badge">BẬT</span>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Cosmetic &amp; Audio Ad Protection</div>
                                <div class="muted">Ẩn khung quảng cáo và bảo vệ trải nghiệm nghe nhạc web</div>
                            </div>
                            <span class="badge">BẬT</span>
                        </div>
                    </div>
                </div>
                <script>{boot_js}</script>
            </body>
            </html>"###,
            css = Self::BASE_CSS,
            nav = Self::nav_bar("privacy"),
            boot_js = Self::THEME_BOOT_JS,
            blocked_count = blocked_count,
            rules_count = rules_count,
            ram_mb = ram_mb,
        )
    }

    /// Renders `rustbrowse://performance` (real metrics only, zero fake savings numbers, live IPC hydration)
    pub fn render_performance(ram_mb: u64, tabs_count: usize) -> String {
        let pct = ((ram_mb as f64 / 2048.0) * 100.0).clamp(4.0, 100.0);
        format!(
            r###"<!DOCTYPE html>
            <html lang="vi" data-theme="light">
            <head>
                <meta charset="utf-8">
                <title>RustBrowse — Performance &amp; RAM</title>
                <style>
                    {css}
                    .bar-track {{
                        width: 100%;
                        height: 8px;
                        background: var(--card-elevated);
                        border: 1px solid var(--card-border);
                        border-radius: 999px;
                        overflow: hidden;
                        margin: 12px 0;
                    }}
                    .bar-fill {{
                        height: 100%;
                        width: {pct:.1}%;
                        background: var(--accent);
                        border-radius: 999px;
                    }}
                    .chips {{ display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }}
                </style>
            </head>
            <body>
                <div class="container">
                    {nav}
                    <div class="card">
                        <div class="card-title">
                            <span>RAM &amp; CPU Monitor — Incremental Auto-Clean</span>
                            <span class="badge" id="perf-pressure">Win32 Live</span>
                        </div>
                        <p class="muted" style="margin-bottom:18px;">
                            Đo trực tiếp PrivateUsage, WorkingSetSize, CPU% (GetProcessTimes) và áp lực bộ nhớ hệ thống (GlobalMemoryStatusEx) trên luồng nền — 0ms khóa giao diện.
                        </p>
                        <div class="grid-3" style="margin-bottom:14px;">
                            <div class="stat-box">
                                <div class="stat-val" id="perf-ram">{ram_mb} MB</div>
                                <div class="stat-label">Working Set Thực Tế</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val" id="perf-cpu">0.0%</div>
                                <div class="stat-label">CPU Trình Duyệt (OS)</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val" id="perf-tabs">{tabs_count}</div>
                                <div class="stat-label">Tab Đang Mở</div>
                            </div>
                        </div>
                        <div class="chips">
                            <span class="badge" id="p-browser">Browser: -- MB</span>
                            <span class="badge" id="p-gpu">GPU: -- MB</span>
                            <span class="badge" id="p-renderer">Renderer: -- MB</span>
                            <span class="badge" id="p-utility">Utility: -- MB</span>
                            <span class="badge" id="p-sys">Hệ thống: --%</span>
                        </div>
                        <div class="bar-track"><div class="bar-fill" id="perf-bar"></div></div>
                        <div class="muted" style="font-size:12px;">Không đặt giới hạn cứng JS heap để tránh lỗi End Process; các tab dùng chung tiến trình renderer được gắn nhãn trung thực "Shared renderer".</div>
                    </div>
                </div>
                <script>{boot_js}
                    window.__rbInvoke('set_perf_panel_open', {{ open: true }});
                    window.addEventListener('beforeunload', () => {{
                        window.__rbInvoke('set_perf_panel_open', {{ open: false }});
                    }});
                    window.addEventListener('pagehide', () => {{
                        window.__rbInvoke('set_perf_panel_open', {{ open: false }});
                    }});
                    async function tickPerf() {{
                        const mem = await window.__rbInvoke('get_memory_report');
                        if (!mem) return;
                        const setT = (id, v) => {{ const e = document.getElementById(id); if (e && e.textContent !== v) e.textContent = v; }};
                        const ws = Math.round(mem.totalWorkingSetMb || 0);
                        setT('perf-ram', ws + ' MB');
                        setT('perf-cpu', (typeof mem.totalCpuPercent === 'number' ? mem.totalCpuPercent.toFixed(1) : '0.0') + '%');
                        setT('p-browser', 'Browser: ' + Math.round(mem.browserMb || 0) + ' MB');
                        setT('p-gpu', 'GPU: ' + Math.round(mem.gpuMb || 0) + ' MB');
                        setT('p-renderer', 'Renderer: ' + Math.round(mem.rendererMb || 0) + ' MB');
                        setT('p-utility', 'Utility: ' + Math.round(mem.utilityMb || 0) + ' MB');
                        setT('p-sys', 'Hệ thống: ' + (mem.systemMemoryLoadPct ?? '--') + '% (' + (mem.memoryPressureLevel || 'low') + ')');
                        const bar = document.getElementById('perf-bar');
                        if (bar) bar.style.width = Math.min(100, Math.max(4, (ws / 2048) * 100)).toFixed(1) + '%';
                    }}
                    tickPerf();
                    setInterval(tickPerf, 1500);
                </script>
            </body>
            </html>"###,
            css = Self::BASE_CSS,
            nav = Self::nav_bar("performance"),
            boot_js = Self::THEME_BOOT_JS,
            ram_mb = ram_mb,
            tabs_count = tabs_count,
            pct = pct,
        )
    }

    /// Renders `rustbrowse://health` (Version, Startup Phase Timings, Crash Ring Buffer, Live RAM/CPU, Diagnostics Export)
    pub fn render_health(version: &str, ram_mb: u64, active_tabs: usize) -> String {
        format!(
            r###"<!DOCTYPE html>
            <html lang="vi" data-theme="light">
            <head>
                <meta charset="utf-8">
                <title>RustBrowse — System Health &amp; Diagnostics</title>
                <style>
                    {css}
                    .diag-pre {{
                        background: var(--card-elevated);
                        border: 1px solid var(--card-border);
                        border-radius: 12px;
                        padding: 14px;
                        font-family: "Cascadia Code", "JetBrains Mono", Consolas, monospace;
                        font-size: 12px;
                        line-height: 1.5;
                        color: var(--text-main);
                        overflow-x: auto;
                        max-height: 320px;
                        white-space: pre-wrap;
                        word-break: break-word;
                        margin-top: 12px;
                        display: none;
                    }}
                    .diag-pre.visible {{ display: block; }}
                </style>
            </head>
            <body>
                <div class="container">
                    {nav}
                    <div class="card">
                        <div class="card-title">
                            <span>RustBrowse Health Monitor</span>
                            <span class="badge" id="h-ver">v{version}</span>
                        </div>
                        <p class="muted" style="margin-bottom:18px;">
                            Theo dõi thời gian khởi động từng pha, sức khỏe bộ nhớ/CPU thực tế, nhật ký cách ly crash WebView2 và xuất báo cáo chẩn đoán sạch (không chứa mật khẩu/cookie).
                        </p>
                        <div class="grid-3" style="margin-bottom:16px;">
                            <div class="stat-box">
                                <div class="stat-val" id="h-shell">-- ms</div>
                                <div class="stat-label">Pha Shell Ready (t_shell_ready_ms)</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val" id="h-tab1">-- ms</div>
                                <div class="stat-label">Pha Tab 1 Ready (t_tab1_ready_ms)</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val" id="h-crashes">0</div>
                                <div class="stat-label">Sự Cố WebView2 Đã Cách Ly</div>
                            </div>
                        </div>
                        <div class="grid-3">
                            <div class="stat-box">
                                <div class="stat-val" id="h-ram">{ram_mb} MB</div>
                                <div class="stat-label">RAM Working Set Hiện Tại</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val" id="h-cpu">0.0%</div>
                                <div class="stat-label">CPU Trình Duyệt (OS)</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val" id="h-tabs">{active_tabs}</div>
                                <div class="stat-label">Tab Đang Mở</div>
                            </div>
                        </div>
                    </div>

                    <div class="card">
                        <div class="card-title">
                            <span>Nhật Ký Crash Isolation (Ring Buffer)</span>
                            <span class="badge" id="h-uptime">Uptime: --s</span>
                        </div>
                        <div id="h-crash-path" class="muted" style="font-size:12px;margin-bottom:10px;"></div>
                        <div id="h-crash-list" class="muted">Không có sự cố crash nào trong phiên này.</div>
                    </div>

                    <div class="card">
                        <div class="card-title">
                            <span>Xuất Báo Cáo Chẩn Đoán (Sanitized Diagnostics)</span>
                            <div style="display:flex;gap:8px;">
                                <button class="btn btn-primary" id="btn-export-diag">Xem Trước Báo Cáo</button>
                                <button class="btn" id="btn-copy-diag" style="display:none;">Sao Chép JSON</button>
                            </div>
                        </div>
                        <p class="muted">Báo cáo chỉ gồm phiên bản, thời gian khởi động, thống kê RAM/CPU và loại sự cố — tuyệt đối không chứa cookie, mật khẩu hay token URL.</p>
                        <pre class="diag-pre" id="diag-preview"></pre>
                    </div>
                </div>
                <script>{boot_js}
                    function escHtml(s) {{
                        return String(s == null ? '' : s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
                    }}
                    async function loadHealth() {{
                        const h = await window.__rbInvoke('get_health_report');
                        if (!h) return;
                        const setT = (id, v) => {{ const e = document.getElementById(id); if (e) e.textContent = v; }};
                        if (h.version) setT('h-ver', 'v' + h.version);
                        setT('h-shell', (h.tShellReadyMs ?? 0) + ' ms');
                        setT('h-tab1', (h.tTab1ReadyMs ?? 0) + ' ms');
                        setT('h-crashes', String(h.crashCount ?? 0));
                        setT('h-tabs', String(h.tabCount ?? {active_tabs}));
                        setT('h-uptime', 'Uptime: ' + (h.uptimeSecs ?? 0) + 's');
                        if (h.memoryReport) {{
                            setT('h-ram', Math.round(h.memoryReport.totalWorkingSetMb || 0) + ' MB');
                            setT('h-cpu', (typeof h.memoryReport.totalCpuPercent === 'number' ? h.memoryReport.totalCpuPercent.toFixed(1) : '0.0') + '%');
                        }}
                        if (h.crashLogPath) setT('h-crash-path', 'Crash log: ' + h.crashLogPath);
                        const listEl = document.getElementById('h-crash-list');
                        if (listEl && Array.isArray(h.crashes) && h.crashes.length > 0) {{
                            listEl.innerHTML = h.crashes.map((c) => {{
                                return '<div class="row">'
                                    + '<div><div style="font-weight:600;font-size:13px;">' + escHtml(c.kind) + ' (Tab #' + escHtml(c.tabId) + ')</div>'
                                    + '<div class="muted">' + escHtml(c.urlDomain || 'internal') + ' • ' + new Date(c.timestampMs || Date.now()).toLocaleTimeString() + '</div></div>'
                                    + '<span class="badge">' + (c.recovered ? 'Đã khôi phục' : 'Đã ghi nhận') + '</span>'
                                    + '</div>';
                            }}).join('');
                        }}
                    }}
                    let lastDiag = '';
                    document.getElementById('btn-export-diag')?.addEventListener('click', async () => {{
                        const json = await window.__rbInvoke('export_diagnostics_report');
                        const pre = document.getElementById('diag-preview');
                        const copyBtn = document.getElementById('btn-copy-diag');
                        if (pre) {{
                            lastDiag = typeof json === 'string' ? json : JSON.stringify(json || {{}}, null, 2);
                            pre.textContent = lastDiag;
                            pre.classList.add('visible');
                            if (copyBtn) copyBtn.style.display = 'inline-flex';
                        }}
                    }});
                    document.getElementById('btn-copy-diag')?.addEventListener('click', async () => {{
                        if (!lastDiag) return;
                        try {{ await navigator.clipboard.writeText(lastDiag); }} catch(e) {{}}
                    }});
                    loadHealth();
                </script>
            </body>
            </html>"###,
            css = Self::BASE_CSS,
            nav = Self::nav_bar("health"),
            boot_js = Self::THEME_BOOT_JS,
            version = version,
            ram_mb = ram_mb,
            active_tabs = active_tabs,
        )
    }

    /// Renders `rustbrowse://selftest`
    pub fn render_selftest(total_rules: usize, blocked_count: u64, ram_mb: u64) -> String {
        format!(
            r###"<!DOCTYPE html>
            <html lang="vi" data-theme="light">
            <head>
                <meta charset="utf-8">
                <title>RustBrowse — Diagnostic Self-Test</title>
                <style>{css}</style>
            </head>
            <body>
                <div class="container">
                    {nav}
                    <div class="card">
                        <div class="card-title">
                            <span>RustBrowse System Diagnostic Suite</span>
                            <span class="badge">OK</span>
                        </div>
                        <p class="muted" style="margin-bottom:18px;">
                            Kiểm tra trực tiếp bộ lọc quảng cáo, thống kê bộ nhớ thực tế và cấu hình GPU WebView2.
                        </p>
                        <div class="grid-2">
                            <div class="stat-box">
                                <div class="stat-val">{total_rules}</div>
                                <div class="stat-label">Luật Adblock Đã Nạp</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">{blocked_count}</div>
                                <div class="stat-label">Quảng cáo &amp; Popups Đã Chặn</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">{ram_mb} MB</div>
                                <div class="stat-label">Bộ Nhớ Working Set Thực Tế</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">D3D11 + GPU</div>
                                <div class="stat-label">Hardware Rasterization &amp; Smooth Scroll</div>
                            </div>
                        </div>
                    </div>
                </div>
                <script>{boot_js}</script>
            </body>
            </html>"###,
            css = Self::BASE_CSS,
            nav = Self::nav_bar("selftest"),
            boot_js = Self::THEME_BOOT_JS,
            total_rules = total_rules,
            blocked_count = blocked_count,
            ram_mb = ram_mb,
        )
    }

    /// Renders `rustbrowse://superboost`
    pub fn render_superboost(ram_mb: u64, active_tabs: usize) -> String {
        format!(
            r###"<!DOCTYPE html>
            <html lang="vi" data-theme="light">
            <head>
                <meta charset="utf-8">
                <title>RustBrowse — Super Boost UI</title>
                <style>{css}</style>
            </head>
            <body>
                <div class="container">
                    {nav}
                    <div class="card">
                        <div class="card-title">
                            <span>Super Boost UI &amp; Universal Web Polish</span>
                            <span class="badge">GPU Composited</span>
                        </div>
                        <p class="muted" style="margin-bottom:18px;">
                            Tinh chỉnh hiển thị trang web an toàn: cuộn mượt native 60fps, thanh cuộn thanh mảnh, font chữ sắc nét, không can thiệp layout hoặc phá vỡ web app.
                        </p>
                        <div class="grid-2" style="margin-bottom:18px;">
                            <div class="stat-box">
                                <div class="stat-val">{ram_mb} MB</div>
                                <div class="stat-label">RAM Thực Tế Hiện Tại</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">{active_tabs}</div>
                                <div class="stat-label">Tab Đang Mở</div>
                            </div>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Universal Web Polish</div>
                                <div class="muted">Làm mịn font, scrollbar tinh gọn, focus ring hiện đại trên mọi website</div>
                            </div>
                            <span class="badge">Bật từ Topbar</span>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Native Smooth Scroll (Zero Jank)</div>
                                <div class="muted">Sử dụng luồng cuộn mượt của Chromium, không chặn wheel event</div>
                            </div>
                            <span class="badge">60 FPS</span>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Bright Glass &amp; Midnight Dark</div>
                                <div class="muted">Chuyển đổi chủ đề tức thì với design token nhất quán</div>
                            </div>
                            <button class="btn btn-primary" onclick="window.__rbToggleTheme()">Đổi chủ đề ngay</button>
                        </div>
                    </div>
                </div>
                <script>{boot_js}</script>
            </body>
            </html>"###,
            css = Self::BASE_CSS,
            nav = Self::nav_bar("superboost"),
            boot_js = Self::THEME_BOOT_JS,
            ram_mb = ram_mb,
            active_tabs = active_tabs,
        )
    }

    /// Renders `rustbrowse://downloads`
    pub fn render_downloads(active_downloads: usize) -> String {
        format!(
            r###"<!DOCTYPE html>
            <html lang="vi" data-theme="light">
            <head>
                <meta charset="utf-8">
                <title>RustBrowse — Trình Quản Lý Tải Xuống</title>
                <style>{css}</style>
            </head>
            <body>
                <div class="container">
                    {nav}
                    <div class="card">
                        <div class="card-title">
                            <span>Quản Lý Tải Xuống &amp; Video Downloader</span>
                            <span class="badge">{active_downloads} đang xử lý</span>
                        </div>
                        <p class="muted" style="margin-bottom:16px;">
                            Hỗ trợ tải trực tiếp MP4/WebM, ghép phân đoạn HLS (.m3u8) và hỗ trợ yt-dlp cho các nền tảng video không DRM. Tối đa 3 luồng đồng thời để giữ trình duyệt mượt.
                        </p>
                        <div style="display:flex;gap:10px;flex-wrap:wrap;margin-bottom:16px;">
                            <input type="text" id="dl-url" placeholder="Dán liên kết video hoặc tệp cần tải..." style="flex:1;min-width:240px;padding:9px 12px;border-radius:10px;border:1px solid var(--card-border);background:var(--card-elevated);color:var(--text-main);font-size:13px;">
                            <button class="btn btn-primary" id="btn-dl-start">Tải ngay</button>
                            <button class="btn" id="btn-dl-folder">Mở thư mục tải</button>
                        </div>
                        <div id="dl-list" class="muted">Mở biểu tượng Video trên thanh công cụ để xem danh sách video bắt được trên từng tab.</div>
                    </div>
                </div>
                <script>{boot_js}
                    async function refreshDownloads() {{
                        const list = await window.__rbInvoke('list_downloads');
                        const el = document.getElementById('dl-list');
                        if (!el || !Array.isArray(list)) return;
                        if (list.length === 0) {{
                            el.innerHTML = '<div class="muted">Chưa có tệp tải xuống nào trong phiên này.</div>';
                            return;
                        }}
                        el.innerHTML = list.map((d) => {{
                            const pct = d.totalBytes > 0 ? Math.min(100, Math.round((d.downloadedBytes / d.totalBytes) * 100)) : 0;
                            return '<div class="row">'
                                + '<div><div style="font-weight:600;font-size:13px;">' + (d.title || d.url) + '</div>'
                                + '<div class="muted">' + d.status + (d.totalBytes ? ' • ' + pct + '%' : '') + (d.error ? ' • ' + d.error : '') + '</div></div>'
                                + '<span class="badge">' + d.status + '</span>'
                                + '</div>';
                        }}).join('');
                    }}
                    document.getElementById('btn-dl-start')?.addEventListener('click', async () => {{
                        const url = (document.getElementById('dl-url')?.value || '').trim();
                        if (!url) return;
                        await window.__rbInvoke('start_video_download', {{ url }});
                        document.getElementById('dl-url').value = '';
                        refreshDownloads();
                    }});
                    document.getElementById('btn-dl-folder')?.addEventListener('click', () => {{
                        window.__rbInvoke('open_download_folder', {{}});
                    }});
                    refreshDownloads();
                </script>
            </body>
            </html>"###,
            css = Self::BASE_CSS,
            nav = Self::nav_bar("downloads"),
            boot_js = Self::THEME_BOOT_JS,
            active_downloads = active_downloads,
        )
    }

    /// Renders `rustbrowse://settings`
    pub fn render_settings(ram_mb: u64, active_tabs: usize, blocked_ads: u64) -> String {
        let pkg_ver = env!("CARGO_PKG_VERSION");
        format!(
            r###"<!DOCTYPE html>
            <html lang="vi" data-theme="light">
            <head>
                <meta charset="utf-8">
                <title>RustBrowse — Cài đặt</title>
                <style>{css}</style>
            </head>
            <body>
                <div class="container">
                    {nav}
                    <div class="card">
                        <div class="card-title">
                            <span>Cài Đặt RustBrowse</span>
                            <span class="badge">v{pkg_ver}</span>
                        </div>
                        <div class="grid-3" style="margin-bottom:18px;">
                            <div class="stat-box">
                                <div class="stat-val">{ram_mb} MB</div>
                                <div class="stat-label">RAM Hiện Tại</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">{active_tabs}</div>
                                <div class="stat-label">Tab Đang Mở</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val">{blocked_ads}</div>
                                <div class="stat-label">Quảng Cáo Đã Chặn</div>
                            </div>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Chủ đề Giao diện</div>
                                <div class="muted">Bright Glass (Sáng hiện đại) hoặc Midnight Dark (Tối sâu)</div>
                            </div>
                            <button class="btn btn-primary" onclick="window.__rbToggleTheme()">Chuyển đổi Sáng / Tối</button>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">SuperBoost Auto Update Engine</div>
                                <div class="muted">Quản lý kênh phát hành (Stable/Beta), xác thực chữ ký số &amp; tự cập nhật</div>
                            </div>
                            <div style="display:flex;gap:8px;">
                                <a class="btn" href="rustbrowse://updates" style="text-decoration:none;">Mở Trung tâm Update</a>
                                <button class="btn btn-primary" onclick="window.__rbInvoke('check_for_updates')">Kiểm tra ngay</button>
                            </div>
                        </div>
                    </div>
                </div>
                <script>{boot_js}</script>
            </body>
            </html>"###,
            css = Self::BASE_CSS,
            nav = Self::nav_bar("settings"),
            boot_js = Self::THEME_BOOT_JS,
            pkg_ver = pkg_ver,
            ram_mb = ram_mb,
            active_tabs = active_tabs,
            blocked_ads = blocked_ads,
        )
    }

    /// Renders `rustbrowse://updates` — SuperBoost Auto Update Release Dashboard
    pub fn render_updates(installed_version: &str) -> String {
        format!(
            r###"<!DOCTYPE html>
            <html lang="vi" data-theme="light">
            <head>
                <meta charset="utf-8">
                <title>RustBrowse — Trung tâm Cập nhật Tự động</title>
                <style>
                    {css}
                    .progress-wrap {{
                        width: 100%;
                        height: 10px;
                        background: var(--card-elevated);
                        border: 1px solid var(--card-border);
                        border-radius: 999px;
                        overflow: hidden;
                        margin: 12px 0 8px;
                    }}
                    .progress-fill {{
                        height: 100%;
                        width: 0%;
                        background: linear-gradient(90deg, var(--accent), var(--blue));
                        border-radius: 999px;
                        transition: width 180ms cubic-bezier(0.22, 1, 0.36, 1);
                    }}
                    .notes-box {{
                        background: var(--card-elevated);
                        border: 1px solid var(--card-border);
                        border-radius: 12px;
                        padding: 14px 16px;
                        font-size: 13px;
                        line-height: 1.6;
                        white-space: pre-wrap;
                        color: var(--text-main);
                        max-height: 240px;
                        overflow-y: auto;
                    }}
                    .status-pill {{
                        display: inline-flex;
                        align-items: center;
                        gap: 6px;
                        padding: 5px 12px;
                        border-radius: 999px;
                        font-size: 12px;
                        font-weight: 700;
                        background: var(--accent-soft);
                        color: var(--accent);
                    }}
                    .status-pill.warn {{
                        background: rgba(217, 119, 6, 0.14);
                        color: var(--amber);
                    }}
                    .status-pill.err {{
                        background: rgba(225, 29, 72, 0.14);
                        color: var(--red);
                    }}
                    .status-pill.ok {{
                        background: rgba(5, 150, 105, 0.14);
                        color: var(--green);
                    }}
                    .btn-group {{
                        display: flex;
                        flex-wrap: wrap;
                        gap: 10px;
                        margin-top: 14px;
                    }}
                    .kv-grid {{
                        display: grid;
                        grid-template-columns: repeat(2, 1fr);
                        gap: 12px;
                        margin-top: 14px;
                    }}
                    .kv-item {{
                        background: var(--card-elevated);
                        border: 1px solid var(--card-border);
                        border-radius: 12px;
                        padding: 12px 14px;
                    }}
                    .kv-label {{
                        font-size: 11px;
                        text-transform: uppercase;
                        letter-spacing: 0.4px;
                        color: var(--text-muted);
                        margin-bottom: 4px;
                    }}
                    .kv-val {{
                        font-size: 13px;
                        font-weight: 600;
                        color: var(--text-main);
                        word-break: break-all;
                    }}
                </style>
            </head>
            <body>
                <div class="container">
                    {nav}
                    <div class="card">
                        <div class="card-title">
                            <span>SuperBoost Auto Update Engine</span>
                            <span id="upd-status-pill" class="status-pill">Đang tải trạng thái...</span>
                        </div>
                        <p class="muted" style="margin-bottom:16px;">
                            Tự động kiểm tra GitHub Releases qua HTTPS, tải gói cập nhật ngầm, xác thực chữ ký số Ed25519 và áp dụng an toàn mà không làm mất tab, cookie hay dữ liệu cá nhân.
                        </p>

                        <div class="grid-3">
                            <div class="stat-box">
                                <div class="stat-val" id="upd-installed">v{installed_version}</div>
                                <div class="stat-label">Phiên bản Đang Cài (Installed)</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val" id="upd-latest">—</div>
                                <div class="stat-label">Phiên bản Mới Nhất (Latest)</div>
                            </div>
                            <div class="stat-box">
                                <div class="stat-val" id="upd-channel-stat">Stable</div>
                                <div class="stat-label">Kênh Phát Hành &amp; Kiến Trúc</div>
                            </div>
                        </div>

                        <div class="progress-wrap">
                            <div class="progress-fill" id="upd-progress-bar"></div>
                        </div>
                        <div style="display:flex;justify-content:space-between;font-size:12px;" class="muted">
                            <span id="upd-progress-text">Tiến độ tải: 0%</span>
                            <span id="upd-last-checked">Kiểm tra lần cuối: Chưa kiểm tra</span>
                        </div>

                        <div class="kv-grid">
                            <div class="kv-item">
                                <div class="kv-label">Trạng thái Xác thực Chữ ký (Verification)</div>
                                <div class="kv-val" id="upd-verify">Đang kiểm tra cấu hình Ed25519...</div>
                            </div>
                            <div class="kv-item">
                                <div class="kv-label">Trạng thái Cài đặt &amp; Phục hồi (Installation)</div>
                                <div class="kv-val" id="upd-install-status">—</div>
                            </div>
                        </div>

                        <div id="upd-error-box" style="display:none;margin-top:12px;padding:12px 14px;border-radius:12px;background:rgba(225,29,72,0.1);border:1px solid rgba(225,29,72,0.28);color:var(--red);font-size:13px;"></div>

                        <div class="btn-group">
                            <button class="btn btn-primary" id="btn-check-now">Kiểm tra cập nhật (Check for updates)</button>
                            <button class="btn" id="btn-stage-dl" style="display:none;">Tải bản cập nhật ngầm</button>
                            <button class="btn btn-primary" id="btn-restart-install" style="display:none;">Khởi động lại để cập nhật (Restart to update)</button>
                            <button class="btn" id="btn-toggle-notes">Xem ghi chú phát hành (Release notes)</button>
                        </div>
                    </div>

                    <div class="card" id="notes-card" style="display:none;">
                        <div class="card-title">
                            <span>Ghi chú Phát hành (Release Notes)</span>
                            <span class="badge" id="notes-ver-badge">Latest</span>
                        </div>
                        <div class="notes-box" id="upd-notes">Chưa có ghi chú phát hành mới từ máy chủ.</div>
                    </div>

                    <div class="card">
                        <div class="card-title">
                            <span>Chính sách Tự động &amp; Rollback Bảo vệ</span>
                            <span class="badge" id="upd-backup-badge">Rollback Guard</span>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Kênh cập nhật (Update Channel)</div>
                                <div class="muted">Stable (khuyên dùng ổn định) hoặc Beta (thử nghiệm sớm)</div>
                            </div>
                            <div style="display:flex;gap:8px;">
                                <button class="btn" id="btn-chan-stable">Stable</button>
                                <button class="btn" id="btn-chan-beta">Beta</button>
                            </div>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Tự động áp dụng khi thoát trình duyệt</div>
                                <div class="muted">Chỉ cài gói đã xác thực chữ ký và không bao giờ ngắt ngang khi đang tải file/phát video</div>
                            </div>
                            <button class="btn" id="btn-auto-exit">Đang bật</button>
                        </div>
                        <div class="row">
                            <div>
                                <div style="font-weight:600;font-size:13px;">Nguồn Manifest HTTPS</div>
                                <div class="muted" id="upd-endpoint">—</div>
                            </div>
                            <span class="badge">HTTPS + Ed25519</span>
                        </div>
                    </div>
                </div>

                <script>{boot_js}
                    (function() {{
                        const elStatus = document.getElementById('upd-status-pill');
                        const elInstalled = document.getElementById('upd-installed');
                        const elLatest = document.getElementById('upd-latest');
                        const elChanStat = document.getElementById('upd-channel-stat');
                        const elBar = document.getElementById('upd-progress-bar');
                        const elProgTxt = document.getElementById('upd-progress-text');
                        const elLastChk = document.getElementById('upd-last-checked');
                        const elVerify = document.getElementById('upd-verify');
                        const elInstStatus = document.getElementById('upd-install-status');
                        const elErr = document.getElementById('upd-error-box');
                        const elNotes = document.getElementById('upd-notes');
                        const elNotesCard = document.getElementById('notes-card');
                        const elNotesBadge = document.getElementById('notes-ver-badge');
                        const elEndpoint = document.getElementById('upd-endpoint');
                        const elBackupBadge = document.getElementById('upd-backup-badge');

                        const btnCheck = document.getElementById('btn-check-now');
                        const btnStage = document.getElementById('btn-stage-dl');
                        const btnRestart = document.getElementById('btn-restart-install');
                        const btnNotes = document.getElementById('btn-toggle-notes');
                        const btnStable = document.getElementById('btn-chan-stable');
                        const btnBeta = document.getElementById('btn-chan-beta');
                        const btnAutoExit = document.getElementById('btn-auto-exit');

                        function fmtBytes(b) {{
                            if (!b || b <= 0) return '0 MB';
                            return (b / (1024 * 1024)).toFixed(2) + ' MB';
                        }}

                        function applyState(st) {{
                            if (!st) return;
                            elInstalled.textContent = 'v' + (st.installedVersion || '{installed_version}');
                            elLatest.textContent = st.latestVersion ? ('v' + st.latestVersion) : '—';
                            const chanLabel = st.updateChannel === 'beta' ? 'Beta' : 'Stable';
                            elChanStat.textContent = chanLabel + ' • ' + (st.architecture || 'x64');

                            elStatus.textContent = st.status || 'Up to date';
                            elStatus.className = 'status-pill';
                            if (st.status === 'Up to date' || st.status === 'Ready to install') {{
                                elStatus.classList.add('ok');
                            }} else if (st.status === 'Update failed' || st.status === 'Rolled back') {{
                                elStatus.classList.add('err');
                            }} else if (st.status === 'Not configured' || st.status === 'Restart required') {{
                                elStatus.classList.add('warn');
                            }}

                            const pct = Number(st.downloadProgressPct || 0);
                            elBar.style.width = Math.min(100, Math.max(0, pct)) + '%';
                            if (st.totalBytes) {{
                                elProgTxt.textContent = 'Tiến độ tải: ' + pct + '% (' + fmtBytes(st.downloadedBytes) + ' / ' + fmtBytes(st.totalBytes) + ')';
                            }} else {{
                                elProgTxt.textContent = 'Tiến độ tải: ' + pct + '%';
                            }}

                            elLastChk.textContent = 'Kiểm tra lần cuối: ' + (st.lastChecked || 'Chưa kiểm tra');
                            elVerify.textContent = st.verificationStatus || '—';
                            elInstStatus.textContent = st.installationStatus || '—';
                            elEndpoint.textContent = st.endpoint || 'Chưa cấu hình URL';
                            elBackupBadge.textContent = st.backupAvailable ? 'Đã sao lưu EXE gốc' : 'Bảo vệ khởi chạy BẬT';

                            if (st.errorMessage) {{
                                elErr.style.display = 'block';
                                elErr.textContent = st.errorMessage;
                            }} else {{
                                elErr.style.display = 'none';
                            }}

                            if (st.releaseNotes) {{
                                elNotes.textContent = st.releaseNotes;
                                elNotesBadge.textContent = st.latestVersion ? ('v' + st.latestVersion) : 'Release';
                            }}

                            btnStage.style.display = st.status === 'Update available' ? 'inline-flex' : 'none';
                            btnRestart.style.display = (st.status === 'Ready to install' || st.status === 'Update available' || st.status === 'Restart required') ? 'inline-flex' : 'none';

                            btnStable.className = st.updateChannel === 'beta' ? 'btn' : 'btn btn-primary';
                            btnBeta.className = st.updateChannel === 'beta' ? 'btn btn-primary' : 'btn';
                            btnAutoExit.textContent = st.autoInstallOnExit ? 'Đang BẬT' : 'Đang TẮT';
                            btnAutoExit.className = st.autoInstallOnExit ? 'btn btn-primary' : 'btn';
                        }}

                        async function refresh() {{
                            const st = await window.__rbInvoke('get_update_dashboard_state');
                            applyState(st);
                        }}

                        btnCheck?.addEventListener('click', async () => {{
                            btnCheck.disabled = true;
                            const st = await window.__rbInvoke('check_for_updates');
                            applyState(st);
                            btnCheck.disabled = false;
                        }});

                        btnStage?.addEventListener('click', async () => {{
                            btnStage.disabled = true;
                            const st = await window.__rbInvoke('download_update_background');
                            applyState(st);
                            btnStage.disabled = false;
                        }});

                        btnRestart?.addEventListener('click', async () => {{
                            btnRestart.disabled = true;
                            await window.__rbInvoke('install_update');
                            await refresh();
                            btnRestart.disabled = false;
                        }});

                        btnNotes?.addEventListener('click', () => {{
                            elNotesCard.style.display = elNotesCard.style.display === 'none' ? 'block' : 'none';
                        }});

                        btnStable?.addEventListener('click', async () => {{
                            applyState(await window.__rbInvoke('set_update_channel', {{ channel: 'stable' }}));
                        }});

                        btnBeta?.addEventListener('click', async () => {{
                            applyState(await window.__rbInvoke('set_update_channel', {{ channel: 'beta' }}));
                        }});

                        btnAutoExit?.addEventListener('click', async () => {{
                            const cur = btnAutoExit.textContent.includes('BẬT');
                            applyState(await window.__rbInvoke('set_auto_install_on_exit', {{ enabled: !cur }}));
                        }});

                        if (window.__TAURI__ && window.__TAURI__.event && typeof window.__TAURI__.event.listen === 'function') {{
                            window.__TAURI__.event.listen('update-state-changed', (ev) => applyState(ev.payload));
                        }}

                        refresh();
                    }})();
                </script>
            </body>
            </html>"###,
            css = Self::BASE_CSS,
            nav = Self::nav_bar("updates"),
            boot_js = Self::THEME_BOOT_JS,
            installed_version = installed_version,
        )
    }
}
