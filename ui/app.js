// ============================================================================
// RustBrowse Topbar & Dynamic Island Controller (ui/app.js)
// Zero-dependency Tauri v2 IPC + GPU Spring Motion + Floating Overlay Trigger
// ============================================================================

(function () {
  'use strict';

  // ---- Tauri v2 IPC & Event Helpers ----
  async function invoke(cmd, args) {
    try {
      if (window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function') {
        return await window.__TAURI__.core.invoke(cmd, args || {});
      }
      if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === 'function') {
        return await window.__TAURI_INTERNALS__.invoke(cmd, args || {});
      }
    } catch (err) {
      console.error('[RustBrowse IPC]', cmd, err);
      return null;
    }
    return null;
  }

  function listenEvent(eventName, handler) {
    try {
      if (window.__TAURI__ && window.__TAURI__.event && typeof window.__TAURI__.event.listen === 'function') {
        window.__TAURI__.event.listen(eventName, (ev) => handler(ev.payload));
      }
    } catch (err) {
      console.warn('[RustBrowse Event]', eventName, err);
    }
  }

  // ---- State ----
  let tabs = [];
  let activeTabId = null;
  let openOverlayPanel = null;
  let superBoostEnabled = localStorage.getItem('rustbrowse-superboost') !== 'false';
  let islandTimer = null;
  let lastBlockedAds = 0;
  let detectedVideoCount = 0;
  let currentUrl = 'rustbrowse://home';

  // ---- DOM References ----
  const tabsScrollWrap = document.getElementById('tabs-scroll-wrap');
  const tabsContainer = document.getElementById('tabs-container');
  const tabStripSpacer = document.getElementById('tab-strip-spacer');
  const btnNewTab = document.getElementById('btn-new-tab');
  const btnBack = document.getElementById('btn-back');
  const btnForward = document.getElementById('btn-forward');
  const btnReload = document.getElementById('btn-reload');
  const btnHome = document.getElementById('btn-home');

  const dynamicIsland = document.getElementById('dynamic-island');
  const islandSecDot = document.getElementById('island-sec-dot');
  const islandDomainBtn = document.getElementById('island-domain-btn');
  const islandDomainText = document.getElementById('island-domain-text');
  const addressInput = document.getElementById('address-input');
  const islandEventPill = document.getElementById('island-event-pill');
  const islandEventLabel = document.getElementById('island-event-label');
  const islandAudioBar = document.getElementById('island-audio-bar');

  const btnCopyCleanUrl = document.getElementById('btn-copy-clean-url');
  const btnReaderMode = document.getElementById('btn-reader-mode');
  const btnGo = document.getElementById('btn-go');

  const btnVideos = document.getElementById('btn-videos');
  const videoCountEl = document.getElementById('video-count');
  const btnShield = document.getElementById('btn-shield');
  const shieldCountEl = document.getElementById('shield-count');
  const btnRamBadge = document.getElementById('btn-ram-badge');
  const ramTextEl = document.getElementById('ram-text');
  const btnSuperBoost = document.getElementById('btn-superboost');
  const btnTheme = document.getElementById('btn-theme');
  const btnDownloads = document.getElementById('btn-downloads');
  const btnSettings = document.getElementById('btn-settings');

  // ---- Theme System (Bright Glass default / Midnight Dark) ----
  function applyTheme(theme) {
    const resolved = theme === 'dark' ? 'dark' : 'light';
    document.documentElement.setAttribute('data-theme', resolved);
    localStorage.setItem('rustbrowse-theme', resolved);
  }

  applyTheme(localStorage.getItem('rustbrowse-theme') || 'light');

  if (btnTheme) {
    btnTheme.addEventListener('click', () => {
      const next = document.documentElement.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
      applyTheme(next);
      showIslandEvent({
        kind: 'theme',
        title: next === 'light' ? 'Bright Glass' : 'Midnight Dark',
        duration: 1800,
      });
    });
  }

  // ---- Super Boost UI Toggle ----
  function syncSuperBoostBtn() {
    if (!btnSuperBoost) return;
    btnSuperBoost.classList.toggle('active', superBoostEnabled);
    btnSuperBoost.setAttribute('aria-pressed', String(superBoostEnabled));
  }
  syncSuperBoostBtn();

  if (btnSuperBoost) {
    btnSuperBoost.addEventListener('click', async () => {
      superBoostEnabled = !superBoostEnabled;
      localStorage.setItem('rustbrowse-superboost', String(superBoostEnabled));
      syncSuperBoostBtn();
      await invoke('toggle_super_boost', { enabled: superBoostEnabled });
      showIslandEvent({
        kind: 'superboost',
        title: superBoostEnabled ? 'Super Boost: BẬT' : 'Super Boost: TẮT',
        duration: 2000,
      });
    });
  }

  // ---- Dynamic Island Helpers ----
  function formatCompactDomain(url) {
    if (!url || url === 'rustbrowse://home') return 'rustbrowse://home';
    if (url.startsWith('rustbrowse://')) return url;
    try {
      const parsed = new URL(url);
      return parsed.hostname.replace(/^www\./, '') || url;
    } catch {
      return url;
    }
  }

  function updateAddressDisplay(url) {
    currentUrl = url || 'rustbrowse://home';
    const compact = formatCompactDomain(currentUrl);
    if (islandDomainText) islandDomainText.textContent = compact;
    if (addressInput && document.activeElement !== addressInput) {
      addressInput.value = currentUrl === 'rustbrowse://home' ? '' : currentUrl;
    }
    if (islandSecDot) {
      islandSecDot.classList.remove('sec-internal', 'sec-https', 'sec-http');
      if (currentUrl.startsWith('https://')) {
        islandSecDot.classList.add('sec-https');
        islandSecDot.title = 'Kết nối HTTPS bảo mật';
      } else if (currentUrl.startsWith('http://')) {
        islandSecDot.classList.add('sec-http');
        islandSecDot.title = 'Kết nối HTTP không mã hóa';
      } else {
        islandSecDot.classList.add('sec-internal');
        islandSecDot.title = 'Trang nội bộ RustBrowse';
      }
    }
  }

  function showIslandEvent(evt) {
    if (!evt || !islandEventPill || !islandEventLabel) return;
    const kind = evt.kind || 'info';
    const text = evt.detail ? `${evt.title}: ${evt.detail}` : (evt.title || '');
    if (!text) return;

    islandEventPill.dataset.kind = kind;
    islandEventLabel.textContent = text;
    islandEventPill.hidden = false;

    if (dynamicIsland) {
      dynamicIsland.classList.remove('pulse');
      void dynamicIsland.offsetWidth;
      dynamicIsland.classList.add('pulse');
      setTimeout(() => dynamicIsland.classList.remove('pulse'), 240);
    }

    clearTimeout(islandTimer);
    islandTimer = setTimeout(() => {
      islandEventPill.hidden = true;
    }, evt.duration || 3600);
  }

  window.triggerIslandEvent = showIslandEvent;

  if (islandEventPill) {
    islandEventPill.addEventListener('click', () => {
      const kind = islandEventPill.dataset.kind;
      if (kind === 'video-detected' && btnVideos) {
        toggleOverlay('videos', btnVideos, 360, 420);
      } else if (kind === 'ad-blocked' && btnShield) {
        toggleOverlay('shield', btnShield, 320, 340);
      } else if (kind === 'ram-cleaned' && btnRamBadge) {
        toggleOverlay('ram', btnRamBadge, 360, 420);
      } else if (kind === 'update-available') {
        invoke('install_update');
      }
      islandEventPill.hidden = true;
    });
  }

  if (islandDomainBtn && addressInput) {
    islandDomainBtn.addEventListener('click', () => {
      addressInput.value = currentUrl === 'rustbrowse://home' ? '' : currentUrl;
      addressInput.focus();
      addressInput.select();
    });
  }

  if (addressInput) {
    addressInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        navigateCurrent();
        addressInput.blur();
      } else if (e.key === 'Escape') {
        addressInput.value = currentUrl === 'rustbrowse://home' ? '' : currentUrl;
        addressInput.blur();
      }
    });
    addressInput.addEventListener('focus', () => {
      addressInput.value = currentUrl === 'rustbrowse://home' ? '' : currentUrl;
      addressInput.select();
    });
  }

  function navigateCurrent() {
    if (!addressInput) return;
    const raw = addressInput.value.trim();
    if (!raw) return;
    invoke('navigate', { url: raw });
  }

  // ---- Floating Overlay Window Coordinator ----
  async function toggleOverlay(panel, anchorEl, width, height) {
    if (openOverlayPanel === panel) {
      openOverlayPanel = null;
      await invoke('close_overlay');
      return;
    }
    openOverlayPanel = panel;
    const rect = anchorEl ? anchorEl.getBoundingClientRect() : { right: window.innerWidth - 12, bottom: 74 };
    const x = Math.max(8, Math.round(rect.right - width));
    const y = 76;
    await invoke('open_overlay', { panel, x, y, width, height });
  }

  window.onOverlayClosed = function () {
    openOverlayPanel = null;
  };

  // ---- Unlimited Tab Strip Rendering ----
  function renderTabs() {
    if (!tabsContainer) return;
    tabsContainer.innerHTML = '';

    let audioTab = null;

    tabs.forEach((tab) => {
      if (tab.isPlayingAudio && !audioTab) audioTab = tab;

      const tabEl = document.createElement('div');
      tabEl.className = `tab${tab.id === activeTabId ? ' active' : ''}${tab.isSleeping ? ' sleeping' : ''}`;
      tabEl.dataset.id = String(tab.id);
      tabEl.setAttribute('role', 'tab');
      tabEl.setAttribute('aria-selected', String(tab.id === activeTabId));
      tabEl.title = `${tab.title || tab.url || 'Tab mới'}${tab.ramMb ? ` (${tab.ramMb} MB)` : ''}`;

      // Icon
      const iconEl = document.createElement('div');
      iconEl.className = 'tab-icon';
      iconEl.innerHTML = tab.isSleeping
        ? `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path></svg>`
        : `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"><circle cx="12" cy="12" r="9"></circle><path d="M2 12h20"></path></svg>`;

      // Title
      const titleEl = document.createElement('div');
      titleEl.className = 'tab-title';
      titleEl.textContent = tab.title || (tab.url === 'rustbrowse://home' ? 'Trang chủ' : (formatCompactDomain(tab.url) || 'Tab mới'));

      // Audio Indicator / Mute Button
      const audioEl = document.createElement('div');
      audioEl.className = `tab-audio-icon${tab.isPlayingAudio ? ' playing' : ''}${tab.isMuted ? ' muted' : ''}`;
      audioEl.title = tab.isMuted ? 'Bỏ tắt tiếng tab' : 'Tắt tiếng tab';
      audioEl.innerHTML = tab.isMuted
        ? `<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><line x1="23" y1="9" x2="17" y2="15"></line><line x1="17" y1="9" x2="23" y2="15"></line></svg>`
        : `<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>`;
      audioEl.addEventListener('click', (e) => {
        e.stopPropagation();
        invoke('toggle_tab_mute', { tabId: tab.id });
      });

      // Close Button
      const closeEl = document.createElement('div');
      closeEl.className = 'tab-close';
      closeEl.innerHTML = '&times;';
      closeEl.title = 'Đóng tab (Ctrl+W)';
      closeEl.addEventListener('click', (e) => {
        e.stopPropagation();
        invoke('close_tab', { tabId: tab.id });
      });

      tabEl.appendChild(iconEl);
      tabEl.appendChild(titleEl);
      tabEl.appendChild(audioEl);
      tabEl.appendChild(closeEl);

      // Left-click switch
      tabEl.addEventListener('click', () => {
        if (tab.id !== activeTabId) {
          invoke('switch_tab', { tabId: tab.id });
        }
      });

      // Middle-click close
      tabEl.addEventListener('auxclick', (e) => {
        if (e.button === 1) {
          e.preventDefault();
          invoke('close_tab', { tabId: tab.id });
        }
      });

      tabsContainer.appendChild(tabEl);
    });

    // Update Dynamic Island Audio Mini-Bar
    if (islandAudioBar) {
      if (audioTab && !audioTab.isMuted) {
        islandAudioBar.hidden = false;
        islandAudioBar.dataset.tabId = String(audioTab.id);
      } else {
        islandAudioBar.hidden = true;
      }
    }

    // Keep active tab scrolled into view smoothly
    const activeEl = tabsContainer.querySelector('.tab.active');
    if (activeEl) {
      activeEl.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
    }
  }

  // Mouse wheel horizontal scrolling on tab strip
  if (tabsScrollWrap) {
    tabsScrollWrap.addEventListener('wheel', (e) => {
      if (Math.abs(e.deltaY) > 0) {
        e.preventDefault();
        tabsScrollWrap.scrollLeft += e.deltaY;
      }
    }, { passive: false });
  }

  // Double-click empty tab strip area to open new tab
  if (tabStripSpacer) {
    tabStripSpacer.addEventListener('dblclick', () => {
      invoke('create_tab', { url: 'rustbrowse://home' });
    });
  }

  if (islandAudioBar) {
    islandAudioBar.addEventListener('click', () => {
      const tid = Number(islandAudioBar.dataset.tabId);
      if (tid) invoke('toggle_tab_mute', { tabId: tid });
    });
  }

  // ---- Toolbar Button Bindings (100% Working) ----
  if (btnNewTab) btnNewTab.addEventListener('click', () => invoke('create_tab', { url: 'rustbrowse://home' }));
  if (btnBack) btnBack.addEventListener('click', () => invoke('go_back'));
  if (btnForward) btnForward.addEventListener('click', () => invoke('go_forward'));
  if (btnReload) btnReload.addEventListener('click', () => invoke('reload'));
  if (btnHome) btnHome.addEventListener('click', () => invoke('navigate', { url: 'rustbrowse://home' }));
  if (btnGo) btnGo.addEventListener('click', navigateCurrent);

  if (btnCopyCleanUrl) {
    btnCopyCleanUrl.addEventListener('click', async () => {
      await invoke('copy_clean_url');
      showIslandEvent({ kind: 'info', title: 'Đã sao chép link sạch', duration: 1800 });
    });
  }

  if (btnReaderMode) {
    btnReaderMode.addEventListener('click', () => invoke('toggle_reader_mode'));
  }

  if (btnVideos) {
    btnVideos.addEventListener('click', () => toggleOverlay('videos', btnVideos, 360, 420));
  }

  if (btnShield) {
    btnShield.addEventListener('click', () => toggleOverlay('shield', btnShield, 320, 340));
  }

  if (btnRamBadge) {
    btnRamBadge.addEventListener('click', () => toggleOverlay('ram', btnRamBadge, 360, 420));
  }

  if (btnDownloads) {
    btnDownloads.addEventListener('click', () => invoke('create_tab', { url: 'rustbrowse://downloads' }));
  }

  if (btnSettings) {
    btnSettings.addEventListener('click', () => toggleOverlay('menu', btnSettings, 290, 380));
  }

  // ---- Video Count Badge Helper ----
  function updateVideoBadge(count) {
    detectedVideoCount = count || 0;
    if (videoCountEl) videoCountEl.textContent = String(detectedVideoCount);
    if (btnVideos) btnVideos.classList.toggle('has-videos', detectedVideoCount > 0);
  }

  async function refreshActiveTabVideos() {
    if (!activeTabId) return;
    const list = await invoke('get_detected_videos', { tabId: activeTabId });
    if (Array.isArray(list)) {
      updateVideoBadge(list.length);
    }
  }

  // ---- Global Backend Callbacks (called via Rust eval or IPC) ----
  window.updateTabs = function (tabData, currentId, ramMb) {
    tabs = Array.isArray(tabData) ? tabData : [];
    const tabChanged = activeTabId !== currentId;
    activeTabId = currentId;
    renderTabs();

    const active = tabs.find((t) => t.id === activeTabId);
    if (active) {
      updateAddressDisplay(active.url);
    }
    if (typeof ramMb === 'number') {
      window.updateMetrics(ramMb);
    }
    if (tabChanged) {
      refreshActiveTabVideos();
    }
  };

  window.updateUrl = function (url) {
    updateAddressDisplay(url);
  };

  window.updateMetrics = function (ramMb, blockedAds) {
    if (ramTextEl && typeof ramMb === 'number') {
      ramTextEl.textContent = `${ramMb} MB`;
    }
    if (shieldCountEl && typeof blockedAds === 'number') {
      if (blockedAds > lastBlockedAds && lastBlockedAds > 0) {
        showIslandEvent({
          kind: 'ad-blocked',
          title: `Đã chặn +${blockedAds - lastBlockedAds} quảng cáo`,
          duration: 2200,
        });
      }
      lastBlockedAds = blockedAds;
      shieldCountEl.textContent = String(blockedAds);
    }
    if (btnRamBadge && typeof ramMb === 'number') {
      btnRamBadge.classList.remove('ram-green', 'ram-yellow', 'ram-red');
      if (ramMb < 350) {
        btnRamBadge.classList.add('ram-green');
      } else if (ramMb < 650) {
        btnRamBadge.classList.add('ram-yellow');
      } else {
        btnRamBadge.classList.add('ram-red');
      }
    }
  };

  // ---- Tauri Live Events ----
  listenEvent('island-event', (payload) => {
    if (payload) showIslandEvent(payload);
  });

  listenEvent('videos-detected', (payload) => {
    if (!payload) return;
    if (!activeTabId || payload.tabId === activeTabId) {
      updateVideoBadge(payload.count || (payload.videos ? payload.videos.length : 0));
    }
    if (payload.count > 0) {
      showIslandEvent({
        kind: 'video-detected',
        title: `Phát hiện ${payload.count} video`,
        duration: 3200,
      });
    }
  });

  listenEvent('download-progress', (item) => {
    if (!item) return;
    if (item.status === 'completed') {
      showIslandEvent({ kind: 'download-complete', title: 'Tải xong', detail: item.title, duration: 3500 });
    } else if (item.status === 'failed') {
      showIslandEvent({ kind: 'download-error', title: 'Lỗi tải', detail: item.error || item.title, duration: 4000 });
    }
  });

  // ---- Periodic Real Telemetry Refresh (4s) ----
  async function pollTelemetry() {
    const [mem, shield] = await Promise.all([
      invoke('get_memory_report'),
      invoke('get_shield_stats'),
    ]);
    const ramVal = mem && typeof mem.totalWorkingSetMb === 'number'
      ? Math.round(mem.totalWorkingSetMb)
      : (mem && typeof mem.totalPrivateMb === 'number' ? Math.round(mem.totalPrivateMb) : undefined);
    const adsVal = shield && typeof shield.blockedAds === 'number' ? shield.blockedAds : undefined;
    if (ramVal !== undefined || adsVal !== undefined) {
      window.updateMetrics(ramVal !== undefined ? ramVal : undefined, adsVal);
    }
  }
  setInterval(pollTelemetry, 4000);
  setTimeout(pollTelemetry, 900);

  // ---- Full Keyboard Shortcuts ----
  window.addEventListener('keydown', (e) => {
    const key = e.key.toLowerCase();

    if (e.ctrlKey && !e.shiftKey && key === 't') {
      e.preventDefault();
      invoke('create_tab', { url: 'rustbrowse://home' });
    } else if (e.ctrlKey && e.shiftKey && key === 't') {
      e.preventDefault();
      invoke('reopen_closed_tab');
    } else if (e.ctrlKey && !e.shiftKey && key === 'w') {
      e.preventDefault();
      if (activeTabId) invoke('close_tab', { tabId: activeTabId });
    } else if (e.ctrlKey && !e.shiftKey && e.key === 'Tab') {
      e.preventDefault();
      if (tabs.length > 1) {
        const idx = tabs.findIndex((t) => t.id === activeTabId);
        const next = tabs[(idx + 1) % tabs.length];
        if (next) invoke('switch_tab', { tabId: next.id });
      }
    } else if (e.ctrlKey && e.shiftKey && e.key === 'Tab') {
      e.preventDefault();
      if (tabs.length > 1) {
        const idx = tabs.findIndex((t) => t.id === activeTabId);
        const prev = tabs[(idx - 1 + tabs.length) % tabs.length];
        if (prev) invoke('switch_tab', { tabId: prev.id });
      }
    } else if (e.ctrlKey && !e.shiftKey && key === 'l') {
      e.preventDefault();
      if (addressInput) {
        addressInput.value = currentUrl === 'rustbrowse://home' ? '' : currentUrl;
        addressInput.focus();
        addressInput.select();
      }
    } else if (e.ctrlKey && !e.shiftKey && key === 'j') {
      e.preventDefault();
      invoke('create_tab', { url: 'rustbrowse://downloads' });
    } else if ((e.ctrlKey && key === 'r') || e.key === 'F5') {
      e.preventDefault();
      invoke('reload');
    } else if (e.altKey && e.key === 'ArrowLeft') {
      e.preventDefault();
      invoke('go_back');
    } else if (e.altKey && e.key === 'ArrowRight') {
      e.preventDefault();
      invoke('go_forward');
    } else if (e.ctrlKey && e.shiftKey && key === 'g') {
      e.preventDefault();
      invoke('toggle_gaming_mode');
    } else if (e.altKey && key === 'r') {
      e.preventDefault();
      invoke('toggle_reader_mode');
    } else if (e.ctrlKey && e.shiftKey && key === 'c') {
      e.preventDefault();
      invoke('copy_clean_url');
    } else if (e.ctrlKey && e.shiftKey && key === 'z') {
      e.preventDefault();
      invoke('start_element_zapper');
    } else if (e.key === 'Escape' && openOverlayPanel) {
      openOverlayPanel = null;
      invoke('close_overlay');
    }
  });

  // Initial bootstrap placeholder until Rust syncs real tabs
  window.updateTabs([{ id: 1, title: 'Trang chủ', url: 'rustbrowse://home', isPlayingAudio: false, isMuted: false }], 1);
})();
