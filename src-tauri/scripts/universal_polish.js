// ============================================================================
// RustBrowse Universal Super Boost Web Polish (scripts/universal_polish.js)
// Safe, toggleable CSS/JS visual & performance polish for all websites:
// - Respects prefers-reduced-motion & per-domain disable flag (localStorage)
// - Smooth GPU page reveal (opacity 0 -> 1 in 150ms, zero artificial delay)
// - Crisp typography (-webkit-font-smoothing: antialiased)
// - Low-specificity @layer rustbrowse-polish + :where(...) rounded corners (12px)
//   for content images >= 100x80 (excluding logos/avatars/icons/sprites/SVG/
//   canvas/maps/fullscreen videos), cards, and inputs
// - Thin modern rounded scrollbar (6px)
// - Lazy-load offscreen images (loading="lazy", decoding="async")
// - Throttled MutationObserver (>= 250ms + requestIdleCallback), addedNodes only
// ============================================================================

(() => {
  'use strict';

  if (window.__RUSTBROWSE_POLISH_INJECTED__) return;
  window.__RUSTBROWSE_POLISH_INJECTED__ = true;

  // Skip internal browser pages (they already have native Bright Glass / Dark UI)
  try {
    const proto = window.location.protocol;
    if (proto === 'rustbrowse:' || proto === 'about:' || proto === 'chrome:' || proto === 'edge:') {
      return;
    }
  } catch (_) {
    return;
  }

  const STORAGE_KEY_DOMAIN_DISABLED = 'rb_polish_disabled';
  const STYLE_ID = '__rustbrowse_universal_polish_css';
  const REVEAL_CLASS = 'rb-polish-reveal-ready';
  const MUTATION_THROTTLE_MS = 300;

  const scheduleIdle =
    typeof window.requestIdleCallback === 'function'
      ? (cb, timeout) => window.requestIdleCallback(cb, { timeout: timeout || 1200 })
      : (cb) => window.setTimeout(cb, 120);

  function prefersReducedMotion() {
    try {
      return !!(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches);
    } catch (_) {
      return false;
    }
  }

  function isDomainDisabled() {
    try {
      return localStorage.getItem(STORAGE_KEY_DOMAIN_DISABLED) === '1';
    } catch (_) {
      return false;
    }
  }

  let polishEnabled = !isDomainDisabled();
  if (typeof window.__RUSTBROWSE_POLISH_ENABLED__ === 'boolean') {
    polishEnabled = window.__RUSTBROWSE_POLISH_ENABLED__ && !isDomainDisabled();
  } else {
    window.__RUSTBROWSE_POLISH_ENABLED__ = polishEnabled;
  }

  // Excluded keywords for images that must NEVER get rounded corners
  const EXCLUDED_IMG_RE =
    /\b(logo|brand|avatar|profile|userpic|icon|ico|sprite|emoji|smiley|badge|favicon|flag|captcha|qr|barcode|map|tile|marker|pin|pixel|spacer|tracking)\b/i;

  function isExcludedContainer(el) {
    let cur = el.parentElement;
    let depth = 0;
    while (cur && depth < 6) {
      const tag = cur.tagName;
      if (tag === 'SVG' || tag === 'HEADER' || tag === 'NAV' || tag === 'BUTTON') {
        return true;
      }
      const cls = typeof cur.className === 'string' ? cur.className : '';
      if (
        cls &&
        (cls.indexOf('gm-style') !== -1 ||
          cls.indexOf('leaflet-') !== -1 ||
          cls.indexOf('mapboxgl-') !== -1 ||
          cls.indexOf('avatar') !== -1 ||
          cls.indexOf('logo') !== -1)
      ) {
        return true;
      }
      cur = cur.parentElement;
      depth++;
    }
    return false;
  }

  function processImage(img) {
    if (!img || img.tagName !== 'IMG' || img.dataset.rbPolishChecked === '1') return;

    // 1. Lazy-load & async decode offscreen images
    if (!img.hasAttribute('decoding')) {
      img.setAttribute('decoding', 'async');
    }
    if (!img.hasAttribute('loading')) {
      try {
        const rect = img.getBoundingClientRect();
        if (rect.top > (window.innerHeight || 800) * 1.1) {
          img.setAttribute('loading', 'lazy');
        }
      } catch (_) {}
    }

    // 2. Evaluate eligibility for safe 12px rounded corners (>= 100x80 content image)
    const src = img.currentSrc || img.getAttribute('src') || '';
    if (src.indexOf('.svg') !== -1 || src.indexOf('data:image/svg') === 0) {
      img.dataset.rbPolishChecked = '1';
      return;
    }

    const meta = `${img.className || ''} ${img.id || ''} ${img.alt || ''} ${src}`;
    if (EXCLUDED_IMG_RE.test(meta) || isExcludedContainer(img)) {
      img.dataset.rbPolishChecked = '1';
      return;
    }

    const markIfSized = () => {
      if (!img.isConnected) return;
      const w = img.clientWidth || img.naturalWidth || 0;
      const h = img.clientHeight || img.naturalHeight || 0;
      if (w >= 100 && h >= 80) {
        img.dataset.rbPolishImg = '1';
      }
      img.dataset.rbPolishChecked = '1';
    };

    if (img.complete && (img.naturalWidth > 0 || img.clientWidth > 0)) {
      markIfSized();
    } else {
      img.addEventListener('load', markIfSized, { once: true, passive: true });
    }
  }

  function scanImagesInNode(node) {
    if (!polishEnabled || !node || node.nodeType !== 1) return;
    if (node.tagName === 'IMG') {
      processImage(node);
      return;
    }
    if (typeof node.getElementsByTagName === 'function') {
      const imgs = node.getElementsByTagName('img');
      for (let i = 0; i < imgs.length; i++) {
        processImage(imgs[i]);
      }
    }
  }

  function buildPolishCss() {
    const reduced = prefersReducedMotion();
    const revealCss = reduced
      ? ''
      : `
      html:not(.${REVEAL_CLASS}) {
        opacity: 0;
      }
      html.${REVEAL_CLASS} {
        opacity: 1;
        transition: opacity 150ms cubic-bezier(0.22, 1, 0.36, 1);
      }
    `;

    return `
      @layer rustbrowse-polish {
        ${revealCss}

        /* Crisp typography */
        :where(html, body) {
          -webkit-font-smoothing: antialiased;
          text-rendering: optimizeLegibility;
        }

        /* Safe 12px rounded corners for qualifying content images (>= 100x80) */
        :where(img[data-rb-polish-img="1"]) {
          border-radius: 12px;
        }

        /* Never round fullscreen videos, SVG, canvas, or map elements */
        :where(video:fullscreen, video:-webkit-full-screen, svg, canvas, .gm-style img, .leaflet-container img, .mapboxgl-canvas) {
          border-radius: 0;
        }

        /* Subtle rounded cards & form inputs (low specificity so site CSS wins) */
        :where(article, [class*="card" i]:not([class*="discard" i])) {
          border-radius: 12px;
        }
        :where(
          input:not([type="checkbox"]):not([type="radio"]):not([type="range"]):not([type="hidden"]):not([type="color"]),
          textarea,
          select
        ) {
          border-radius: 10px;
        }

        /* Thin modern rounded scrollbar (6px) */
        ::-webkit-scrollbar {
          width: 6px;
          height: 6px;
        }
        ::-webkit-scrollbar-track {
          background: transparent;
        }
        ::-webkit-scrollbar-thumb {
          background: rgba(128, 134, 148, 0.38);
          border-radius: 9999px;
        }
        ::-webkit-scrollbar-thumb:hover {
          background: rgba(128, 134, 148, 0.62);
        }
      }
    `;
  }

  function triggerPageReveal() {
    const root = document.documentElement;
    if (!root || root.classList.contains(REVEAL_CLASS)) return;
    requestAnimationFrame(() => {
      root.classList.add(REVEAL_CLASS);
    });
  }

  function applyPolishStyles() {
    if (!polishEnabled) return;
    let styleEl = document.getElementById(STYLE_ID);
    if (!styleEl) {
      styleEl = document.createElement('style');
      styleEl.id = STYLE_ID;
      styleEl.textContent = buildPolishCss();
      const target = document.head || document.documentElement;
      if (target) {
        target.appendChild(styleEl);
      }
    }

    // Immediate GPU reveal with 0 artificial delay + 250ms hard failsafe
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', triggerPageReveal, { once: true });
      window.setTimeout(triggerPageReveal, 250);
    } else {
      triggerPageReveal();
    }
  }

  function removePolishStyles() {
    const styleEl = document.getElementById(STYLE_ID);
    if (styleEl) styleEl.remove();
    if (document.documentElement) {
      document.documentElement.classList.add(REVEAL_CLASS);
    }
  }

  // Public toggles for RustBrowse UI / IPC
  window.__RUSTBROWSE_TOGGLE_POLISH = function (enable) {
    polishEnabled = Boolean(enable) && !isDomainDisabled();
    window.__RUSTBROWSE_POLISH_ENABLED__ = polishEnabled;
    if (polishEnabled) {
      applyPolishStyles();
      scheduleIdle(() => scanImagesInNode(document.body || document.documentElement));
    } else {
      removePolishStyles();
    }
  };

  window.__RUSTBROWSE_SET_DOMAIN_POLISH = function (disabled) {
    try {
      if (disabled) localStorage.setItem(STORAGE_KEY_DOMAIN_DISABLED, '1');
      else localStorage.removeItem(STORAGE_KEY_DOMAIN_DISABLED);
    } catch (_) {}
    window.__RUSTBROWSE_TOGGLE_POLISH(!disabled);
  };

  // Throttled MutationObserver (>= 300ms + requestIdleCallback), addedNodes only
  const pendingNodes = new Set();
  let flushTimer = null;

  function flushPendingNodes() {
    flushTimer = null;
    if (!polishEnabled || pendingNodes.size === 0) {
      pendingNodes.clear();
      return;
    }
    const batch = Array.from(pendingNodes);
    pendingNodes.clear();
    scheduleIdle(() => {
      for (let i = 0; i < batch.length; i++) {
        const n = batch[i];
        if (n && n.isConnected) {
          scanImagesInNode(n);
        }
      }
    }, 1000);
  }

  function startObserver() {
    if (!document.documentElement) return;
    const observer = new MutationObserver((mutations) => {
      if (!polishEnabled) return;
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
    observer.observe(document.documentElement, { childList: true, subtree: true });
  }

  // Boot
  if (polishEnabled) {
    applyPolishStyles();
  }
  if (document.readyState === 'loading') {
    document.addEventListener(
      'DOMContentLoaded',
      () => {
        if (polishEnabled) {
          scheduleIdle(() => scanImagesInNode(document.body || document.documentElement));
        }
        startObserver();
      },
      { once: true }
    );
  } else {
    if (polishEnabled) {
      scheduleIdle(() => scanImagesInNode(document.body || document.documentElement));
    }
    startObserver();
  }
})();
