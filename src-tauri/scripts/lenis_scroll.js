// ============================================================================
// RustBrowse Smooth Scroll Engine (scripts/lenis_scroll.js)
// - Default: Native GPU compositor smooth scrolling (ZERO passive:false wheel
//   listeners, zero main-thread scroll blocking or jank!)
// - Optional: Toggleable light smooth wheel mode via
//   window.__RUSTBROWSE_TOGGLE_SMOOTH_SCROLL(true), auto-disabled on
//   problematic/canvas/map/editor domains and prefers-reduced-motion.
// ============================================================================

(() => {
  'use strict';

  if (window.__RUSTBROWSE_LENIS_INJECTED__) return;
  window.__RUSTBROWSE_LENIS_INJECTED__ = true;

  const PROBLEMATIC_DOMAINS = [
    'maps.google.',
    'figma.com',
    'docs.google.com',
    'sheets.google.com',
    'slides.google.com',
    'youtube.com',
    'excalidraw.com',
    'miro.com',
    'vscode.dev',
    'github.dev',
    'codepen.io',
    'photopea.com',
    'canva.com',
    'discord.com',
    'notion.so',
    'openstreetmap.org',
    'earth.google.com',
  ];

  function isProblematicDomain() {
    try {
      const host = (window.location.hostname || '').toLowerCase();
      if (!host) return true;
      for (let i = 0; i < PROBLEMATIC_DOMAINS.length; i++) {
        if (host.indexOf(PROBLEMATIC_DOMAINS[i]) !== -1) return true;
      }
      if (window.location.protocol === 'rustbrowse:' || window.location.protocol === 'about:') {
        return true;
      }
    } catch (_) {
      return true;
    }
    return false;
  }

  function prefersReducedMotion() {
    try {
      return !!(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches);
    } catch (_) {
      return false;
    }
  }

  // Native compositor smooth scrolling is the default (lightModeEnabled = false).
  // No global { passive: false } wheel listener is attached unless explicitly enabled
  // on a safe domain.
  let lightModeEnabled = false;
  let wheelListenerAttached = false;

  const DAMPING = 0.16;
  const EPSILON = 0.5;
  let targetY = 0;
  let currentY = 0;
  let isRunning = false;
  let rafId = null;
  let lastTime = 0;

  function onRaf(now) {
    if (!isRunning || !lightModeEnabled) {
      isRunning = false;
      rafId = null;
      return;
    }

    const dt = Math.min((now - lastTime) / 1000, 0.05);
    lastTime = now;

    const factor = 1 - Math.exp(-22 * DAMPING * dt);
    currentY += (targetY - currentY) * factor;
    window.scrollTo(window.scrollX || 0, currentY);

    if (Math.abs(targetY - currentY) <= EPSILON) {
      currentY = targetY;
      window.scrollTo(window.scrollX || 0, currentY);
      isRunning = false;
      rafId = null;
      return;
    }

    rafId = requestAnimationFrame(onRaf);
  }

  function hasScrollableAncestor(el) {
    let cur = el;
    let depth = 0;
    while (cur && cur !== document.body && cur !== document.documentElement && depth < 8) {
      if (cur.scrollHeight > cur.clientHeight + 2) {
        const oy = window.getComputedStyle(cur).overflowY;
        if (oy === 'auto' || oy === 'scroll') return true;
      }
      cur = cur.parentElement;
      depth++;
    }
    return false;
  }

  function handleLightWheel(e) {
    if (!lightModeEnabled || e.ctrlKey || e.metaKey || e.shiftKey || e.defaultPrevented) return;
    // Pass through trackpad / high-frequency smooth wheel events directly to native compositor
    if (Math.abs(e.deltaY) < 40 && e.deltaMode === 0) return;
    if (e.target && hasScrollableAncestor(e.target)) return;

    let deltaY = e.deltaY;
    if (e.deltaMode === 1) deltaY *= 32;
    else if (e.deltaMode === 2) deltaY *= window.innerHeight;

    const doc = document.documentElement;
    const maxScroll = Math.max((doc ? doc.scrollHeight : 0) - window.innerHeight, 0);
    if (maxScroll <= 0) return;

    if (!isRunning) {
      currentY = window.scrollY || 0;
      targetY = currentY;
    }

    const nextTarget = Math.max(0, Math.min(maxScroll, targetY + deltaY));
    if (nextTarget === targetY) return;

    e.preventDefault();
    targetY = nextTarget;
    if (!isRunning) {
      isRunning = true;
      lastTime = performance.now();
      rafId = requestAnimationFrame(onRaf);
    }
  }

  function attachLightMode() {
    if (wheelListenerAttached) return;
    if (isProblematicDomain() || prefersReducedMotion()) return;
    window.addEventListener('wheel', handleLightWheel, { passive: false });
    wheelListenerAttached = true;
  }

  function detachLightMode() {
    if (wheelListenerAttached) {
      window.removeEventListener('wheel', handleLightWheel, { passive: false });
      wheelListenerAttached = false;
    }
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    isRunning = false;
  }

  window.__RUSTBROWSE_SMOOTH_SCROLL_ENABLED__ = false;

  window.__RUSTBROWSE_TOGGLE_SMOOTH_SCROLL = function (enable) {
    const wantLight = Boolean(enable);
    window.__RUSTBROWSE_SMOOTH_SCROLL_ENABLED__ = wantLight;
    try {
      localStorage.setItem('rb_smooth_scroll_mode', wantLight ? 'light' : 'native');
    } catch (_) {}

    if (wantLight && !isProblematicDomain() && !prefersReducedMotion()) {
      lightModeEnabled = true;
      attachLightMode();
    } else {
      lightModeEnabled = false;
      detachLightMode();
    }
  };

  // Restore saved preference ONLY if user explicitly opted into light mode
  try {
    if (localStorage.getItem('rb_smooth_scroll_mode') === 'light') {
      window.__RUSTBROWSE_TOGGLE_SMOOTH_SCROLL(true);
    }
  } catch (_) {}

  // Passive scroll sync only (never blocks compositor)
  window.addEventListener(
    'scroll',
    () => {
      if (!isRunning) {
        currentY = window.scrollY || 0;
        targetY = currentY;
      }
    },
    { passive: true }
  );
})();
