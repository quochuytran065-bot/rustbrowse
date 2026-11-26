/**
 * RustBrowse — Extreme RAM, Network & Anti-Freeze Web Optimizer
 * Injected via add_script_to_execute_on_document_created
 * Target: Windows 11 x64
 * Zero scroll jank: debounced requestIdleCallback MutationObserver (>= 250ms),
 * inspects only addedNodes (never querySelectorAll('*')), preserves native
 * compositor scrolling and CSS animations/polish.
 */
(function () {
  'use strict';

  if (window.__rustbrowse_optimizer_injected) return;
  window.__rustbrowse_optimizer_injected = true;

  const scheduleIdle =
    typeof window.requestIdleCallback === 'function'
      ? (cb, timeout) => window.requestIdleCallback(cb, { timeout: timeout || 1500 })
      : (cb) => window.setTimeout(cb, 120);

  // --------------------------------------------------------------------------
  // 1. Prefer HW-accelerated H.264 (AVC1) over CPU-heavy AV1/VP9 on low-end GPUs
  // --------------------------------------------------------------------------
  const rejectedCodecs = ['av01', 'av1', 'vp9', 'vp09', 'vp8'];

  if (window.MediaSource && typeof MediaSource.isTypeSupported === 'function') {
    const originalIsTypeSupported = MediaSource.isTypeSupported.bind(MediaSource);
    MediaSource.isTypeSupported = function (type) {
      if (typeof type === 'string') {
        const lower = type.toLowerCase();
        for (let i = 0; i < rejectedCodecs.length; i++) {
          if (lower.indexOf(rejectedCodecs[i]) !== -1) return false;
        }
      }
      return originalIsTypeSupported(type);
    };
  }

  if (window.HTMLMediaElement && window.HTMLMediaElement.prototype) {
    const originalCanPlayType = HTMLMediaElement.prototype.canPlayType;
    HTMLMediaElement.prototype.canPlayType = function (type) {
      if (typeof type === 'string') {
        const lower = type.toLowerCase();
        for (let i = 0; i < rejectedCodecs.length; i++) {
          if (lower.indexOf(rejectedCodecs[i]) !== -1) return '';
        }
      }
      return originalCanPlayType.call(this, type);
    };
  }

  // --------------------------------------------------------------------------
  // 2. Targeted GPU-hog cleanup (strictly scoped — never overrides global
  //    scroll-behavior, animations, box-shadows, or universal_polish.js)
  // --------------------------------------------------------------------------
  function injectPerformanceStyles() {
    if (document.getElementById('__rustbrowse_optimizer_css')) return;
    const styleEl = document.createElement('style');
    styleEl.id = '__rustbrowse_optimizer_css';
    styleEl.textContent = `
      /* YouTube Ambient Mode (Cinematics) & hover glow hogs */
      #cinematics, #frosted-glass, ytd-playlist-panel-renderer #cinematics,
      .html5-video-player .ytp-glow {
        display: none !important;
      }
      /* TikTok ambient blur background */
      [class*="DivBlurBackground"], [class*="DivAmbient"] {
        display: none !important;
      }
    `;
    const target = document.head || document.documentElement;
    if (target) {
      target.appendChild(styleEl);
    } else {
      document.addEventListener(
        'DOMContentLoaded',
        () => {
          (document.head || document.documentElement).appendChild(styleEl);
        },
        { once: true }
      );
    }

    scheduleIdle(() => {
      try {
        const prefetchLinks = document.querySelectorAll('link[rel="prefetch"], link[rel="prerender"]');
        for (let i = 0; i < prefetchLinks.length; i++) {
          prefetchLinks[i].remove();
        }
      } catch (_) {}
    }, 2000);
  }

  injectPerformanceStyles();

  // --------------------------------------------------------------------------
  // 3. Pause Offscreen Muted Videos via IntersectionObserver
  // --------------------------------------------------------------------------
  let videoObserver = null;
  if ('IntersectionObserver' in window) {
    videoObserver = new IntersectionObserver(
      (entries) => {
        for (let i = 0; i < entries.length; i++) {
          const entry = entries[i];
          const video = entry.target;
          if (!entry.isIntersecting) {
            // ONLY pause muted autoplay feed videos; never pause unmuted media
            if (!video.paused && !video.ended && video.muted) {
              video.__rb_auto_paused = true;
              try {
                video.pause();
              } catch (_) {}
            }
          } else if (video.__rb_auto_paused) {
            video.__rb_auto_paused = false;
            try {
              video.play().catch(() => {});
            } catch (_) {}
          }
        }
      },
      { threshold: 0.05 }
    );
  }

  const observedVideos = new WeakSet();

  function observeVideoElement(video) {
    if (!videoObserver || !video || observedVideos.has(video)) return;
    observedVideos.add(video);
    videoObserver.observe(video);
  }

  function registerVideosInNode(node) {
    if (!videoObserver || !node || node.nodeType !== 1) return;
    if (node.tagName === 'VIDEO') {
      observeVideoElement(node);
      return;
    }
    if (typeof node.getElementsByTagName === 'function') {
      const videos = node.getElementsByTagName('video');
      for (let i = 0; i < videos.length; i++) {
        observeVideoElement(videos[i]);
      }
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener(
      'DOMContentLoaded',
      () => scheduleIdle(() => registerVideosInNode(document.body || document.documentElement)),
      { once: true }
    );
  } else {
    scheduleIdle(() => registerVideosInNode(document.body || document.documentElement));
  }

  // Throttled MutationObserver (>= 300ms + requestIdleCallback), inspecting ONLY addedNodes
  const pendingNodes = new Set();
  let flushTimer = null;
  const MUTATION_THROTTLE_MS = 300;

  function flushPendingNodes() {
    flushTimer = null;
    if (pendingNodes.size === 0) return;
    const batch = Array.from(pendingNodes);
    pendingNodes.clear();

    scheduleIdle(() => {
      for (let i = 0; i < batch.length; i++) {
        const n = batch[i];
        if (n && n.isConnected) {
          registerVideosInNode(n);
        }
      }
    }, 1000);
  }

  const mutationObserver = new MutationObserver((mutations) => {
    for (let i = 0; i < mutations.length; i++) {
      const added = mutations[i].addedNodes;
      for (let j = 0; j < added.length; j++) {
        const node = added[j];
        if (node.nodeType === 1) {
          pendingNodes.add(node);
        }
      }
    }
    if (pendingNodes.size > 0 && flushTimer === null) {
      flushTimer = window.setTimeout(flushPendingNodes, MUTATION_THROTTLE_MS);
    }
  });

  if (document.documentElement) {
    mutationObserver.observe(document.documentElement, { childList: true, subtree: true });
  }

  // --------------------------------------------------------------------------
  // 4. Idle DOM Pruning: Heavy Feed Elements > 3500px offscreen
  // --------------------------------------------------------------------------
  function pruneOffscreenDOM() {
    if (document.hidden) return;
    const scrollY = window.scrollY || window.pageYOffset || 0;
    const PRUNE_THRESHOLD_PX = 3500;
    const RESTORE_THRESHOLD_PX = 1200;

    // 4.1 Facebook Feed Articles
    const fbArticles = document.querySelectorAll('div[role="article"]');
    for (let i = 0; i < fbArticles.length; i++) {
      const art = fbArticles[i];
      if (art.dataset.rbPruned) continue;
      const rect = art.getBoundingClientRect();
      const bottomAbs = rect.top + scrollY + rect.height;
      if (scrollY - bottomAbs > PRUNE_THRESHOLD_PX) {
        art.dataset.rbPruned = 'true';
        art.style.minHeight = art.offsetHeight + 'px';
        const media = art.querySelectorAll('video, img, iframe, canvas');
        for (let m = 0; m < media.length; m++) {
          const el = media[m];
          if (el.tagName === 'VIDEO') {
            if (el.muted) {
              try {
                el.pause();
                el.src = '';
                el.load();
                el.remove();
              } catch (_) {}
            }
          } else {
            el.remove();
          }
        }
      }
    }

    // 4.2 YouTube Comments
    const ytComments = document.querySelectorAll('ytd-comment-thread-renderer');
    for (let i = 0; i < ytComments.length; i++) {
      const comm = ytComments[i];
      const rect = comm.getBoundingClientRect();
      const topAbs = rect.top + scrollY;
      const bottomAbs = topAbs + rect.height;
      if (scrollY - bottomAbs > PRUNE_THRESHOLD_PX) {
        if (!comm.dataset.rbPruned) {
          comm.dataset.rbPruned = 'true';
          comm.style.minHeight = comm.offsetHeight + 'px';
          const body = comm.querySelector('#body') || comm.querySelector('#comment');
          if (body) body.style.display = 'none';
        }
      } else if (comm.dataset.rbPruned && Math.abs(scrollY - topAbs) < RESTORE_THRESHOLD_PX) {
        comm.dataset.rbPruned = '';
        const body = comm.querySelector('#body') || comm.querySelector('#comment');
        if (body) body.style.display = '';
      }
    }

    // 4.3 TikTok Feed Items
    const tikTokItems = document.querySelectorAll(
      'div[data-e2e="feed-item"], div[class*="DivItemContainer"], div[class*="video-card"]'
    );
    for (let i = 0; i < tikTokItems.length; i++) {
      const item = tikTokItems[i];
      if (item.dataset.rbPruned) continue;
      const rect = item.getBoundingClientRect();
      const bottomAbs = rect.top + scrollY + rect.height;
      if (scrollY - bottomAbs > PRUNE_THRESHOLD_PX) {
        item.dataset.rbPruned = 'true';
        item.style.minHeight = item.offsetHeight + 'px';
        const videos = item.getElementsByTagName('video');
        for (let v = videos.length - 1; v >= 0; v--) {
          if (!videos[v].muted) continue;
          try {
            videos[v].pause();
            videos[v].src = '';
            videos[v].load();
          } catch (_) {}
          videos[v].remove();
        }
      }
    }
  }

  window.setInterval(() => {
    scheduleIdle(pruneOffscreenDOM, 2000);
  }, 15000);

  window.addEventListener('message', (event) => {
    if (event.data && (event.data === 'PRUNE_DOM_NOW' || event.data.type === 'PRUNE_DOM_NOW')) {
      scheduleIdle(pruneOffscreenDOM, 500);
    }
  });

  if (window.chrome && window.chrome.webview) {
    window.chrome.webview.addEventListener('message', (event) => {
      if (event.data === 'PRUNE_DOM_NOW' || (event.data && event.data.type === 'PRUNE_DOM_NOW')) {
        scheduleIdle(pruneOffscreenDOM, 500);
      }
    });
  }

  // --------------------------------------------------------------------------
  // 5. Real-time Media Audio State Broadcast (Audio Protection)
  // --------------------------------------------------------------------------
  let lastBroadcastAudible = null;

  function checkAndBroadcastAudioState() {
    try {
      const mediaElements = document.querySelectorAll('video, audio');
      let isAudible = false;
      for (let i = 0; i < mediaElements.length; i++) {
        const m = mediaElements[i];
        if (!m.paused && !m.muted && m.volume > 0 && !m.ended) {
          isAudible = true;
          break;
        }
      }
      if (!isAudible && navigator.mediaSession && navigator.mediaSession.playbackState === 'playing') {
        isAudible = true;
      }
      if (isAudible !== lastBroadcastAudible) {
        lastBroadcastAudible = isAudible;
        if (window.chrome && window.chrome.webview) {
          window.chrome.webview.postMessage(
            JSON.stringify({
              type: 'AUDIO_STATE',
              isAudible: isAudible,
            })
          );
        }
      }
    } catch (_) {}
  }

  ['play', 'playing', 'pause', 'ended', 'volumechange'].forEach((evt) => {
    window.addEventListener(evt, checkAndBroadcastAudioState, { capture: true, passive: true });
  });
  window.setInterval(() => scheduleIdle(checkAndBroadcastAudioState, 1500), 3000);
})();
