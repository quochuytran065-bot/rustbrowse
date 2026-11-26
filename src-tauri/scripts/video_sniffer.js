// RustBrowse Universal Video Sniffer
// Lightweight MutationObserver + requestIdleCallback detector with honest EME/DRM detection.
// Never polls heavily; never attempts DRM bypass.
(() => {
  'use strict';

  if (window.__RUSTBROWSE_VIDEO_SNIFFER__) return;
  window.__RUSTBROWSE_VIDEO_SNIFFER__ = true;

  const proto = window.location && window.location.protocol;
  if (!proto || proto === 'about:' || proto === 'rustbrowse:' || proto === 'tauri:') {
    return;
  }

  let snifferEnabled =
    typeof window.__RUSTBROWSE_SNIFFER_ENABLED__ === 'boolean'
      ? window.__RUSTBROWSE_SNIFFER_ENABLED__
      : true;

  const detectedMap = new Map(); // key -> DetectedVideo
  const networkStreams = new Map(); // url -> { format, ts }
  const drmVideos = new WeakSet();
  let pageHasEme = false;
  let lastReportedSignature = '';
  let scanScheduled = false;
  let mutationTimer = null;

  const YTDLP_HOSTS = [
    'youtube.com',
    'youtu.be',
    'facebook.com',
    'fb.watch',
    'tiktok.com',
    'vimeo.com',
    'twitch.tv',
    'twitter.com',
    'x.com',
    'instagram.com',
    'bilibili.com',
  ];

  function getTauriInvoke() {
    if (window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function') {
      return window.__TAURI__.core.invoke;
    }
    if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === 'function') {
      return window.__TAURI_INTERNALS__.invoke;
    }
    return null;
  }

  function isYtDlpHost(hostname) {
    const h = (hostname || '').toLowerCase();
    return YTDLP_HOSTS.some((d) => h === d || h.endsWith('.' + d));
  }

  function inferFormat(url) {
    if (!url) return 'unknown';
    const lower = url.toLowerCase();
    if (lower.startsWith('blob:')) return 'mse';
    if (lower.includes('.m3u8')) return 'hls';
    if (lower.includes('.mpd')) return 'dash';
    if (lower.includes('.webm')) return 'webm';
    if (lower.includes('.mp4') || lower.includes('mime=video%2fmp4') || lower.includes('mime=video/mp4')) return 'mp4';
    if (lower.includes('.mov')) return 'mov';
    if (lower.includes('.mkv')) return 'mkv';
    return 'video';
  }

  function isMediaUrl(url) {
    if (!url || typeof url !== 'string') return false;
    const lower = url.toLowerCase();
    if (lower.startsWith('data:') || lower.startsWith('javascript:')) return false;
    return (
      /\.(mp4|webm|m3u8|mpd|mov|mkv)(\?|#|$)/i.test(lower) ||
      lower.includes('application/vnd.apple.mpegurl') ||
      lower.includes('application/x-mpegurl')
    );
  }

  function hashId(str) {
    let h = 2166136261;
    for (let i = 0; i < str.length; i++) {
      h ^= str.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return 'vid-' + (h >>> 0).toString(16);
  }

  function cleanTitle(video) {
    const attrTitle =
      (video && (video.getAttribute('title') || video.getAttribute('aria-label'))) || '';
    const docTitle = (document.title || '').trim();
    const raw = (attrTitle.trim() || docTitle || window.location.hostname || 'Video').slice(0, 140);
    return raw;
  }

  function inferQuality(video, url) {
    if (video && video.videoHeight) {
      const h = video.videoHeight;
      if (h >= 2160) return '4K';
      if (h >= 1440) return '1440p';
      if (h >= 1080) return '1080p';
      if (h >= 720) return '720p';
      if (h >= 480) return '480p';
      if (h >= 360) return '360p';
      return h + 'p';
    }
    if (url) {
      const m = url.match(/(2160|1440|1080|720|480|360)p/i);
      if (m) return m[1] + 'p';
    }
    return 'Auto';
  }

  // --- Passive DRM / EME observation (honest flagging, zero bypass) ---
  try {
    if (navigator.requestMediaKeySystemAccess) {
      const origRequestEME = navigator.requestMediaKeySystemAccess.bind(navigator);
      navigator.requestMediaKeySystemAccess = function (...args) {
        pageHasEme = true;
        scheduleScan();
        return origRequestEME(...args);
      };
    }
  } catch (_) {}

  try {
    if (typeof HTMLMediaElement !== 'undefined' && HTMLMediaElement.prototype.setMediaKeys) {
      const origSetMediaKeys = HTMLMediaElement.prototype.setMediaKeys;
      HTMLMediaElement.prototype.setMediaKeys = function (mediaKeys) {
        if (mediaKeys) {
          drmVideos.add(this);
          pageHasEme = true;
          scheduleScan();
        }
        return origSetMediaKeys.call(this, mediaKeys);
      };
    }
  } catch (_) {}

  function isVideoDrm(video) {
    if (!video) return pageHasEme;
    try {
      if (video.mediaKeys) return true;
    } catch (_) {}
    if (drmVideos.has(video)) return true;
    return pageHasEme;
  }

  // --- Lightweight passive network sniffing for .m3u8 / .mpd / direct video ---
  function recordNetworkUrl(rawUrl) {
    if (!snifferEnabled || !rawUrl || typeof rawUrl !== 'string') return;
    let absUrl = rawUrl;
    try {
      absUrl = new URL(rawUrl, window.location.href).href;
    } catch (_) {
      return;
    }
    if (!isMediaUrl(absUrl)) return;
    if (!networkStreams.has(absUrl)) {
      if (networkStreams.size > 40) {
        const firstKey = networkStreams.keys().next().value;
        networkStreams.delete(firstKey);
      }
      networkStreams.set(absUrl, {
        format: inferFormat(absUrl),
        ts: Date.now(),
      });
      scheduleScan();
    }
  }

  try {
    const origFetch = window.fetch;
    if (typeof origFetch === 'function') {
      window.fetch = function (input, init) {
        try {
          const url = typeof input === 'string' ? input : input && input.url;
          if (url) recordNetworkUrl(url);
        } catch (_) {}
        return origFetch.apply(this, arguments);
      };
    }
  } catch (_) {}

  try {
    const origXhrOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function (_method, url) {
      try {
        if (typeof url === 'string') recordNetworkUrl(url);
      } catch (_) {}
      return origXhrOpen.apply(this, arguments);
    };
  } catch (_) {}

  // --- Floating "⬇ Tải Video" hover button (isolated Shadow DOM overlay) ---
  let hostEl = null;
  let shadowRoot = null;
  let floatBtn = null;
  let activeVideo = null;
  let hideTimer = null;

  function ensureOverlayButton() {
    if (floatBtn) return floatBtn;
    hostEl = document.createElement('div');
    hostEl.id = 'rustbrowse-video-dl-host';
    hostEl.style.cssText =
      'position:fixed;top:0;left:0;width:0;height:0;z-index:2147483646;pointer-events:none;';
    shadowRoot = hostEl.attachShadow({ mode: 'closed' });

    const style = document.createElement('style');
    style.textContent = `
      .rb-dl-btn {
        position: fixed;
        pointer-events: auto;
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 6px 12px;
        border-radius: 999px;
        border: 1px solid rgba(255, 255, 255, 0.28);
        background: rgba(15, 23, 42, 0.78);
        backdrop-filter: blur(12px);
        -webkit-backdrop-filter: blur(12px);
        color: #f8fafc;
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
        font-size: 12px;
        font-weight: 600;
        letter-spacing: 0.01em;
        cursor: pointer;
        box-shadow: 0 6px 18px rgba(0, 0, 0, 0.28);
        opacity: 0;
        transform: translateY(-4px) scale(0.96);
        transition: opacity 160ms cubic-bezier(0.22, 1, 0.36, 1),
                    transform 160ms cubic-bezier(0.22, 1, 0.36, 1),
                    background-color 140ms ease;
        user-select: none;
        white-space: nowrap;
      }
      .rb-dl-btn.visible {
        opacity: 1;
        transform: translateY(0) scale(1);
      }
      .rb-dl-btn:hover {
        background: rgba(16, 185, 129, 0.92);
        border-color: rgba(255, 255, 255, 0.45);
      }
      .rb-dl-btn:active {
        transform: scale(0.97);
      }
      @media (prefers-reduced-motion: reduce) {
        .rb-dl-btn { transition: none; }
      }
    `;

    floatBtn = document.createElement('button');
    floatBtn.type = 'button';
    floatBtn.className = 'rb-dl-btn';
    floatBtn.textContent = '⬇ Tải Video';

    floatBtn.addEventListener('mouseenter', () => {
      if (hideTimer) {
        clearTimeout(hideTimer);
        hideTimer = null;
      }
    });

    floatBtn.addEventListener('mouseleave', () => {
      scheduleHideButton();
    });

    floatBtn.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (!activeVideo) return;
      const info = resolveBestVideoEntry(activeVideo);
      if (!info || info.isDrm) return;

      const invoke = getTauriInvoke();
      if (!invoke) {
        floatBtn.textContent = 'Không có IPC';
        return;
      }

      const prevText = floatBtn.textContent;
      floatBtn.textContent = '⏳ Đang tải...';
      invoke('start_video_download', {
        url: info.url,
        title: info.title,
        referer: info.referer,
        format: info.format,
        useYtDlp: info.format === 'yt-dlp' || info.format === 'mse',
      })
        .then(() => {
          floatBtn.textContent = '✓ Đã thêm tải';
          setTimeout(() => {
            if (floatBtn) floatBtn.textContent = '⬇ Tải Video';
          }, 1800);
        })
        .catch(() => {
          floatBtn.textContent = '✕ Lỗi tải';
          setTimeout(() => {
            if (floatBtn) floatBtn.textContent = prevText;
          }, 2000);
        });
    });

    shadowRoot.appendChild(style);
    shadowRoot.appendChild(floatBtn);
    (document.documentElement || document.body).appendChild(hostEl);
    return floatBtn;
  }

  function positionButtonForVideo(video) {
    if (!snifferEnabled || !video || !video.isConnected || isVideoDrm(video)) {
      hideButtonImmediately();
      return;
    }
    const rect = video.getBoundingClientRect();
    if (rect.width < 160 || rect.height < 90 || rect.bottom < 0 || rect.top > window.innerHeight) {
      hideButtonImmediately();
      return;
    }
    const btn = ensureOverlayButton();
    const top = Math.max(8, Math.min(window.innerHeight - 40, rect.top + 12));
    const rightOffset = Math.max(8, window.innerWidth - rect.right + 12);
    btn.style.top = top + 'px';
    btn.style.right = rightOffset + 'px';
    btn.style.left = 'auto';
    btn.classList.add('visible');
  }

  function scheduleHideButton() {
    if (hideTimer) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      if (floatBtn) floatBtn.classList.remove('visible');
      activeVideo = null;
    }, 260);
  }

  function hideButtonImmediately() {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    if (floatBtn) floatBtn.classList.remove('visible');
    activeVideo = null;
  }

  function resolveBestVideoEntry(video) {
    const drm = isVideoDrm(video);
    const title = cleanTitle(video);
    const quality = inferQuality(video, video.currentSrc || video.src);
    const referer = window.location.href;

    let src = video.currentSrc || video.src || '';
    if (!src) {
      const sourceEl = video.querySelector('source[src]');
      if (sourceEl) src = sourceEl.src || sourceEl.getAttribute('src') || '';
    }

    // If blob/MSE, prefer a sniffed HLS/MP4 stream if available, else fall back to page URL for yt-dlp sites
    let format = inferFormat(src);
    if (!src || src.startsWith('blob:')) {
      // Check latest sniffed HLS/direct stream
      let bestStream = null;
      for (const [u, meta] of networkStreams.entries()) {
        if (meta.format === 'hls' || meta.format === 'mp4' || meta.format === 'webm') {
          bestStream = { url: u, format: meta.format };
        }
      }
      if (bestStream && !isYtDlpHost(window.location.hostname)) {
        src = bestStream.url;
        format = bestStream.format;
      } else if (isYtDlpHost(window.location.hostname)) {
        src = window.location.href;
        format = 'yt-dlp';
      } else if (bestStream) {
        src = bestStream.url;
        format = bestStream.format;
      } else {
        src = src || window.location.href;
        format = 'mse';
      }
    }

    return {
      id: hashId(src + '|' + format),
      tabId: 0, // Filled by Rust from webview label
      title,
      url: src,
      quality,
      format,
      isDrm: drm,
      referer,
    };
  }

  const boundVideos = new WeakSet();

  function attachVideoListeners(video) {
    if (boundVideos.has(video)) return;
    boundVideos.add(video);

    video.addEventListener(
      'mouseenter',
      () => {
        if (!snifferEnabled || isVideoDrm(video)) return;
        if (hideTimer) {
          clearTimeout(hideTimer);
          hideTimer = null;
        }
        activeVideo = video;
        positionButtonForVideo(video);
      },
      { passive: true }
    );

    video.addEventListener(
      'mouseleave',
      () => {
        scheduleHideButton();
      },
      { passive: true }
    );

    video.addEventListener(
      'loadedmetadata',
      () => {
        scheduleScan();
      },
      { passive: true }
    );
  }

  function scanDocument() {
    scanScheduled = false;
    if (!snifferEnabled) return;

    const current = new Map();

    // 1. Scan <video> elements
    const videos = document.querySelectorAll('video');
    for (let i = 0; i < videos.length; i++) {
      const v = videos[i];
      attachVideoListeners(v);
      const entry = resolveBestVideoEntry(v);
      if (entry && entry.url && !entry.url.startsWith('data:')) {
        current.set(entry.url, entry);
      }
    }

    // 2. Scan standalone <source> tags with direct media URLs
    const sources = document.querySelectorAll('source[src]');
    for (let i = 0; i < sources.length; i++) {
      const s = sources[i];
      const src = s.src || s.getAttribute('src');
      if (src && isMediaUrl(src) && !current.has(src)) {
        const parentVideo = s.closest('video');
        current.set(src, {
          id: hashId(src),
          tabId: 0,
          title: cleanTitle(parentVideo),
          url: src,
          quality: inferQuality(parentVideo, src),
          format: inferFormat(src),
          isDrm: isVideoDrm(parentVideo),
          referer: window.location.href,
        });
      }
    }

    // 3. Include passively sniffed HLS / DASH / MP4 streams
    for (const [url, meta] of networkStreams.entries()) {
      if (!current.has(url)) {
        current.set(url, {
          id: hashId(url),
          tabId: 0,
          title: (document.title || window.location.hostname || 'Stream').slice(0, 140),
          url,
          quality: inferQuality(null, url),
          format: meta.format,
          isDrm: pageHasEme,
          referer: window.location.href,
        });
      }
    }

    // 4. Sync with detectedMap and report via Tauri IPC if changed
    detectedMap.clear();
    const list = [];
    for (const entry of current.values()) {
      detectedMap.set(entry.url, entry);
      list.push(entry);
    }

    const sig = list.map((v) => `${v.id}:${v.isDrm}:${v.quality}`).join('|');
    if (sig !== lastReportedSignature) {
      lastReportedSignature = sig;
      const invoke = getTauriInvoke();
      if (invoke) {
        invoke('report_detected_videos', { videos: list }).catch(() => {});
      }
    }
  }

  function scheduleScan() {
    if (!snifferEnabled || scanScheduled) return;
    scanScheduled = true;
    const ric = window.requestIdleCallback;
    if (typeof ric === 'function') {
      ric(() => scanDocument(), { timeout: 1200 });
    } else {
      setTimeout(scanDocument, 250);
    }
  }

  function debouncedMutationHandler() {
    if (!snifferEnabled) return;
    if (mutationTimer) clearTimeout(mutationTimer);
    mutationTimer = setTimeout(() => {
      mutationTimer = null;
      scheduleScan();
    }, 400);
  }

  // Expose toggle helper callable from Rust (`set_sniffer_enabled`)
  window.__RUSTBROWSE_SET_SNIFFER_ENABLED__ = function (enabled) {
    snifferEnabled = Boolean(enabled);
    window.__RUSTBROWSE_SNIFFER_ENABLED__ = snifferEnabled;
    if (!snifferEnabled) {
      hideButtonImmediately();
    } else {
      scheduleScan();
    }
  };

  // Reposition floating button on scroll/resize only when visible
  window.addEventListener(
    'scroll',
    () => {
      if (activeVideo && floatBtn && floatBtn.classList.contains('visible')) {
        positionButtonForVideo(activeVideo);
      }
    },
    { passive: true }
  );

  // Observe DOM mutations cleanly (debounced + idle-scheduled)
  const startObserver = () => {
    scheduleScan();
    if (!document.documentElement) return;
    const observer = new MutationObserver((mutations) => {
      for (let i = 0; i < mutations.length; i++) {
        const m = mutations[i];
        if (m.addedNodes && m.addedNodes.length > 0) {
          debouncedMutationHandler();
          return;
        }
        if (m.type === 'attributes' && (m.attributeName === 'src' || m.attributeName === 'currentSrc')) {
          debouncedMutationHandler();
          return;
        }
      }
    });
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['src'],
    });
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', startObserver, { once: true });
  } else {
    startObserver();
  }
})();
