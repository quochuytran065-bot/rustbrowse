/**
 * RustBrowse — Global New Tab & Popup/Popunder Guard (Module 1 / Phụ lục A1)
 * Injected automatically via add_script_to_execute_on_document_created for EVERY tab.
 * 
 * Features:
 * 1. Strict window.open override: blocks unprompted popups, popunders, and rogue redirects.
 * 2. Click-hijacking / transparent overlay neutralization (Capture Phase).
 * 3. Soft GC hook (window.__RUST_CHROME_SOFT_GC__) for Watchdog Tier 1.
 */
(function () {
  'use strict';

  if (window.__rustbrowse_popup_guard_injected) return;
  window.__rustbrowse_popup_guard_injected = true;

  const BAD_PATTERNS = [
    'popunder', 'popup', 'clickadu', 'popads', 'popcash', 'adsterra',
    'propellerads', 'exoclick', 'hilltopads', 'monetag', 'trafficjunky',
    'c=cpd', 'utm_medium=popunder', 'utm_source=clickadu', 'short-link',
    'realsrv', 'tsyndicate', 'magsrv', 'tx88', 'lu88', 'huvip', 'hbet',
    'man88', 'kubet', 'go88', 'sunwin', 'hitclub', 'taixiu', 'nohu'
  ];

  function isBlockedUrl(url) {
    if (!url || typeof url !== 'string') return false;
    const lower = url.toLowerCase();
    for (let i = 0; i < BAD_PATTERNS.length; i++) {
      if (lower.indexOf(BAD_PATTERNS[i]) !== -1) {
        return true;
      }
    }
    return false;
  }

  // 1. Hook window.open to block popunders and uninitiated popups
  const originalOpen = window.open;
  window.open = function (url, target, features) {
    const hasUserActivation = (navigator.userActivation && navigator.userActivation.isActive) ||
                              (window.event && window.event.isTrusted);

    if (isBlockedUrl(url)) {
      console.warn('[RustBrowse Popup Guard] Blocked popunder/ad URL:', url);
      notifyHostBlocked(url);
      return null;
    }

    if (!hasUserActivation) {
      console.warn('[RustBrowse Popup Guard] Blocked uninitiated window.open (no user gesture):', url);
      notifyHostBlocked(url || 'about:blank');
      return null;
    }

    // Allow genuine OAuth / user-clicked windows
    return originalOpen.call(window, url, target, features);
  };

  // 2. Capture-phase click listener to neutralize transparent click-jacking overlays
  document.addEventListener('click', function (e) {
    const el = e.target;
    if (!el || el === document.documentElement || el === document.body) return;

    try {
      const style = window.getComputedStyle(el);
      const zIndex = parseInt(style.zIndex, 10);
      const isFixed = style.position === 'fixed' || style.position === 'absolute';

      if (isFixed && zIndex >= 500) {
        const rect = el.getBoundingClientRect();
        const coversViewport = (rect.width >= window.innerWidth * 0.75) && (rect.height >= window.innerHeight * 0.75);
        const isTransparent = parseFloat(style.opacity) < 0.15 ||
                              style.backgroundColor === 'transparent' ||
                              style.backgroundColor === 'rgba(0, 0, 0, 0)';

        if (coversViewport && isTransparent) {
          // This is a malicious click-jacking overlay!
          e.preventDefault();
          e.stopPropagation();
          e.stopImmediatePropagation();
          console.warn('[RustBrowse Popup Guard] Neutralized transparent click-jacking overlay!', el);
          el.remove();
          notifyHostBlocked('Click-jacking Overlay');
          return false;
        }
      }

      // Check if target is a link leading to a bad pattern
      let current = el;
      while (current && current !== document.body && current !== document.documentElement) {
        if (current.tagName === 'A' && current.href) {
          if (isBlockedUrl(current.href)) {
            e.preventDefault();
            e.stopPropagation();
            e.stopImmediatePropagation();
            console.warn('[RustBrowse Popup Guard] Blocked click navigation to gambling/scam link:', current.href);
            notifyHostBlocked(current.href);
            current.remove();
            return false;
          }
        }
        current = current.parentElement;
      }
    } catch (_) {}
  }, true);

  // 3. Soft GC Hook invoked by Watchdog Tier 1
  window.__RUST_CHROME_SOFT_GC__ = function () {
    try {
      // Clean detached / dead iframes
      const iframes = document.querySelectorAll('iframe');
      for (let i = 0; i < iframes.length; i++) {
        const ifr = iframes[i];
        if (!ifr.offsetParent && ifr.offsetWidth === 0 && ifr.offsetHeight === 0) {
          ifr.src = 'about:blank';
          ifr.remove();
        }
      }
      // Clear console buffer
      if (console.clear && !window.chrome) {
        console.clear();
      }
    } catch (_) {}
  };

  function notifyHostBlocked(destination) {
    if (window.chrome && window.chrome.webview) {
      window.chrome.webview.postMessage(JSON.stringify({
        action: 'BlockedPopup',
        url: destination
      }));
    }
  }
})();
