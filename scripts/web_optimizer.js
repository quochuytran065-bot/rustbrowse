/**
 * RustBrowse / RustLite — Extreme RAM, Network & Anti-Freeze Web Optimizer
 * Injected automatically via add_script_to_execute_on_document_created
 * Target: Windows 11 x64 (4 GB RAM systems)
 * Specialized for: YouTube, Facebook, TikTok, Heavy SPAs
 */
(function () {
  'use strict';

  // Prevent multiple injections
  if (window.__rustbrowse_optimizer_injected) return;
  window.__rustbrowse_optimizer_injected = true;

  // --------------------------------------------------------------------------
  // 1. Ép H.264: Reject VP9, VP8, AV01 to force HW-accelerated H.264 (AVC1)
  // --------------------------------------------------------------------------
  const rejectedCodecs = ['av01', 'av1', 'vp9', 'vp09', 'vp8'];

  if (window.MediaSource && typeof MediaSource.isTypeSupported === 'function') {
    const originalIsTypeSupported = MediaSource.isTypeSupported.bind(MediaSource);
    MediaSource.isTypeSupported = function (type) {
      if (typeof type === 'string') {
        const lower = type.toLowerCase();
        for (let i = 0; i < rejectedCodecs.length; i++) {
          if (lower.indexOf(rejectedCodecs[i]) !== -1) {
            return false;
          }
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
          if (lower.indexOf(rejectedCodecs[i]) !== -1) {
            return '';
          }
        }
      }
      return originalCanPlayType.call(this, type);
    };
  }

  // --------------------------------------------------------------------------
  // 2. Kill Heavy GPU Effects, Animations & Ambient Lighting (Zero Delay CSS)
  // --------------------------------------------------------------------------
  function injectPerformanceStyles() {
    const styleEl = document.createElement('style');
    styleEl.id = '__rustbrowse_optimizer_css';
    styleEl.textContent = `
      /* Zero Delay: safe 0.001ms keeps JS transition/animation events firing while visual delay is 0ms */
      *, *::before, *::after {
        animation-duration: 0.001ms !important;
        animation-delay: 0s !important;
        animation-iteration-count: 1 !important;
        transition-duration: 0.001ms !important;
        transition-delay: 0s !important;
        scroll-behavior: auto !important;
        backdrop-filter: none !important;
        -webkit-backdrop-filter: none !important;
        box-shadow: none !important;
        text-shadow: none !important;
      }
      /* YouTube Ambient Mode (Cinematics) & hover preview hogs */
      #cinematics, #frosted-glass, ytd-playlist-panel-renderer #cinematics {
        display: none !important;
      }
      .html5-video-player .ytp-glow {
        display: none !important;
      }
      /* TikTok ambient glow & blur */
      [class*="DivBlurBackground"], [class*="DivAmbient"] {
        display: none !important;
      }
    `;
    const target = document.head || document.documentElement;
    if (target) {
      target.appendChild(styleEl);
    } else {
      document.addEventListener('DOMContentLoaded', () => {
        (document.head || document.documentElement).appendChild(styleEl);
      });
    }

    // Clean up prefetch / prerender links to save background bandwidth
    const prefetchLinks = document.querySelectorAll('link[rel="prefetch"], link[rel="prerender"]');
    for (let i = 0; i < prefetchLinks.length; i++) {
      prefetchLinks[i].remove();
    }
  }

  injectPerformanceStyles();

  // --------------------------------------------------------------------------
  // 3. Pause Offscreen Videos via IntersectionObserver
  // --------------------------------------------------------------------------
  let videoObserver = null;
  if ('IntersectionObserver' in window) {
    videoObserver = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        const video = entry.target;
        if (!entry.isIntersecting) {
          // ONLY pause if video is explicitly muted (e.g. autoplay news feed ads).
          // NEVER pause unmuted music, video or stream!
          if (!video.paused && !video.ended && video.muted) {
            video.__rb_auto_paused = true;
            try {
              video.pause();
            } catch (_) {}
          }
        } else {
          if (video.__rb_auto_paused) {
            video.__rb_auto_paused = false;
            try {
              video.play().catch(() => {});
            } catch (_) {}
          }
        }
      });
    }, { threshold: 0.05 });
  }

  function registerVideos(root) {
    if (!videoObserver) return;
    const scope = root || document;
    if (scope.querySelectorAll) {
      const videos = scope.querySelectorAll('video');
      for (let i = 0; i < videos.length; i++) {
        videoObserver.observe(videos[i]);
      }
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => registerVideos(document));
  } else {
    registerVideos(document);
  }

  // Observe dynamically inserted video elements
  const mutationObserver = new MutationObserver((mutations) => {
    for (let i = 0; i < mutations.length; i++) {
      const added = mutations[i].addedNodes;
      for (let j = 0; j < added.length; j++) {
        const node = added[j];
        if (node.nodeType === 1) {
          if (node.tagName === 'VIDEO' && videoObserver) {
            videoObserver.observe(node);
          } else if (node.querySelectorAll) {
            registerVideos(node);
          }
        }
      }
    }
  });

  if (document.documentElement) {
    mutationObserver.observe(document.documentElement, { childList: true, subtree: true });
  }

  // --------------------------------------------------------------------------
  // 4. DOM Pruning: Facebook, YouTube, TikTok Feed Elements > 3000px offscreen
  // --------------------------------------------------------------------------
  function pruneOffscreenDOM() {
    const scrollY = window.scrollY || window.pageYOffset || 0;
    const PRUNE_THRESHOLD_PX = 3000;
    const RESTORE_THRESHOLD_PX = 1200;

    // 4.1 Facebook Feed Articles
    const fbArticles = document.querySelectorAll('div[role="article"]');
    for (let i = 0; i < fbArticles.length; i++) {
      const art = fbArticles[i];
      const rect = art.getBoundingClientRect();
      const topAbs = rect.top + scrollY;
      const bottomAbs = topAbs + rect.height;

      if (scrollY - bottomAbs > PRUNE_THRESHOLD_PX) {
        if (!art.dataset.rbPruned) {
          const h = art.offsetHeight;
          art.dataset.rbPruned = 'true';
          art.style.minHeight = h + 'px';
          // Clean heavy media inside to free VRAM & memory
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
    }

    // 4.2 YouTube Comments & Long Thread Renderers
    const ytComments = document.querySelectorAll('ytd-comment-thread-renderer');
    for (let i = 0; i < ytComments.length; i++) {
      const comm = ytComments[i];
      const rect = comm.getBoundingClientRect();
      const topAbs = rect.top + scrollY;
      const bottomAbs = topAbs + rect.height;

      if (scrollY - bottomAbs > PRUNE_THRESHOLD_PX) {
        if (!comm.dataset.rbPruned) {
          const h = comm.offsetHeight;
          comm.dataset.rbPruned = 'true';
          comm.style.minHeight = h + 'px';
          const body = comm.querySelector('#body') || comm.querySelector('#comment');
          if (body) {
            body.style.display = 'none';
          }
        }
      } else if (comm.dataset.rbPruned && Math.abs(scrollY - topAbs) < RESTORE_THRESHOLD_PX) {
        comm.dataset.rbPruned = '';
        const body = comm.querySelector('#body') || comm.querySelector('#comment');
        if (body) {
          body.style.display = '';
        }
      }
    }

    // 4.3 TikTok Feed Items & Video Containers
    const tikTokItems = document.querySelectorAll(
      'div[data-e2e="feed-item"], div[class*="DivItemContainer"], div[class*="video-card"]'
    );
    for (let i = 0; i < tikTokItems.length; i++) {
      const item = tikTokItems[i];
      const rect = item.getBoundingClientRect();
      const topAbs = rect.top + scrollY;
      const bottomAbs = topAbs + rect.height;

      if (scrollY - bottomAbs > PRUNE_THRESHOLD_PX) {
        if (!item.dataset.rbPruned) {
          const h = item.offsetHeight;
          item.dataset.rbPruned = 'true';
          item.style.minHeight = h + 'px';
          const videos = item.querySelectorAll('video');
          for (let v = 0; v < videos.length; v++) {
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
  }

  // Run DOM pruning every 15 seconds
  setInterval(pruneOffscreenDOM, 15000);

  // Listen for immediate manual trigger from Host Watchdog IPC
  window.addEventListener('message', (event) => {
    if (event.data && (event.data === 'PRUNE_DOM_NOW' || event.data.type === 'PRUNE_DOM_NOW')) {
      pruneOffscreenDOM();
    }
  });

  // Also support chrome.webview host message if available
  if (window.chrome && window.chrome.webview) {
    window.chrome.webview.addEventListener('message', (event) => {
      if (event.data === 'PRUNE_DOM_NOW' || (event.data && event.data.type === 'PRUNE_DOM_NOW')) {
        pruneOffscreenDOM();
      }
    });
  }

  // --------------------------------------------------------------------------
  // Real-time Media Audio State Broadcast (Dual-layer Audio Preservation)
  // --------------------------------------------------------------------------
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
      if (window.chrome && window.chrome.webview) {
        window.chrome.webview.postMessage(JSON.stringify({
          type: 'AUDIO_STATE',
          isAudible: isAudible
        }));
      }
    } catch (_) {}
  }

  ['play', 'playing', 'pause', 'ended', 'volumechange', 'ratechange'].forEach(evt => {
    window.addEventListener(evt, checkAndBroadcastAudioState, true);
  });
  setInterval(checkAndBroadcastAudioState, 2000);
})();
