/**
 * RustBrowse — Super Ad Cleaner & Anti-Gambling Sweeper (Layer B)
 * Injected automatically via add_script_to_execute_on_document_created
 * Specialized for: Vietnamese & Asian Gambling, Catfish Banners, Fake 'X' Closers, Click-Jacking
 */
(function () {
  'use strict';

  if (window.__rustbrowse_sweeper_injected) return;
  window.__rustbrowse_sweeper_injected = true;

  // --------------------------------------------------------------------------
  // 1. Blacklist tokens for elements and attributes
  // --------------------------------------------------------------------------
  const BAD_TOKENS = [
    'c=cpd', 'topbanner', 'catfish', 'tx88', 'lu88', 'huvip', 'hbet',
    'man88', 'vu88', 'k88', 'sin88', 'da88', '9bet', 'go88', 'sunwin',
    'hitclub', 'kubet', 'jun88', 'hi88', 'f8bet', 'new88', '789bet',
    'shbet', 'mb66', 'qq88', 'ok9', 'rikvip', 'taixiu', 'nohu', 'nhacai',
    'soikeo', 'banca', 'lode', 'xoso', 'casino', 'bet88', '88bet',
    'win88', 'vip88', 'club88', 'short-link', 'popunder', 'quangcao',
    'banner-ads', 'ads-floating', 'ads_catfish'
  ];

  // Whitelisted origins and paths to prevent breaking real UI
  const ALLOWED_HOSTS = ['google', 'youtube', 'facebook', 'tiktok', 'github', 'wikipedia'];

  function isAllowedHost() {
    const host = window.location.hostname.toLowerCase();
    for (let i = 0; i < ALLOWED_HOSTS.length; i++) {
      if (host.indexOf(ALLOWED_HOSTS[i]) !== -1) return true;
    }
    return false;
  }

  // --------------------------------------------------------------------------
  // 2. Chống Tự Nhảy Trang (Capture-phase Click Interceptor & window.open Override)
  // --------------------------------------------------------------------------
  const originalOpen = window.open;
  window.open = function (url, target, features) {
    if (typeof url === 'string') {
      const lower = url.toLowerCase();
      for (let i = 0; i < BAD_TOKENS.length; i++) {
        if (lower.indexOf(BAD_TOKENS[i]) !== -1) {
          console.warn('[RustBrowse Shield] Blocked window.open to gambling/scam:', url);
          return null;
        }
      }
    }
    // Block popups with empty or about:blank url that are often used as redirect bridges
    if (!url || url === 'about:blank') {
      console.warn('[RustBrowse Shield] Blocked suspicious blank popup');
      return null;
    }
    return originalOpen.call(window, url, target, features);
  };

  // Intercept click on document in capture phase
  document.addEventListener('click', function (e) {
    let target = e.target;
    while (target && target !== document.body && target !== document.documentElement) {
      if (target.tagName === 'A' && target.href) {
        const lower = target.href.toLowerCase();
        for (let i = 0; i < BAD_TOKENS.length; i++) {
          if (lower.indexOf(BAD_TOKENS[i]) !== -1) {
            e.preventDefault();
            e.stopImmediatePropagation();
            console.warn('[RustBrowse Shield] Neutralized click on gambling redirect:', target.href);
            // Remove the malicious link from DOM
            target.remove();
            return false;
          }
        }
      }
      target = target.parentElement;
    }
  }, true);

  // --------------------------------------------------------------------------
  // 3. Quét & Xóa Catfish Banners, Fake 'X' Closers, and Click-Jacking Overlays
  // --------------------------------------------------------------------------
  function sweepDOM() {
    if (isAllowedHost()) return;

    // 3.1 Remove transparent or screen-covering click-jacking overlays (z-index > 999)
    const allDivs = document.querySelectorAll('div, a');
    for (let i = 0; i < allDivs.length; i++) {
      const el = allDivs[i];
      const style = window.getComputedStyle(el);
      const zIndex = parseInt(style.zIndex, 10);

      if (zIndex > 999 && (style.position === 'fixed' || style.position === 'absolute')) {
        const rect = el.getBoundingClientRect();
        // Covers full screen or large area with transparent or near-zero opacity
        if (rect.width > window.innerWidth * 0.8 && rect.height > window.innerHeight * 0.8) {
          if (parseFloat(style.opacity) < 0.1 || style.backgroundColor === 'transparent' || style.backgroundColor === 'rgba(0, 0, 0, 0)') {
            el.remove();
            continue;
          }
        }
      }

      // 3.2 Fixed Catfish Banners at top or bottom (Aspect ratio >= 3.2)
      if (style.position === 'fixed' || style.position === 'sticky') {
        const rect = el.getBoundingClientRect();
        if (rect.width > 280 && rect.height > 30) {
          const ratio = rect.width / rect.height;
          // Check if at bottom or top edge
          const isBottom = rect.bottom >= window.innerHeight - 30;
          const isTop = rect.top <= 30;

          if ((isBottom || isTop) && ratio >= 3.0) {
            // Check if contains external link, gif/webp banner, or fake close X
            const innerHtml = el.innerHTML.toLowerCase();
            let isBadBanner = false;
            for (let t = 0; t < BAD_TOKENS.length; t++) {
              if (innerHtml.indexOf(BAD_TOKENS[t]) !== -1) {
                isBadBanner = true;
                break;
              }
            }

            // Check for fake X close button inside
            if (!isBadBanner) {
              const fakeX = el.querySelector('[class*="close"], [id*="close"], button, span');
              if (fakeX && (fakeX.textContent.trim().toLowerCase() === 'x' || fakeX.textContent.trim() === '×')) {
                // If contains link or image, it's a floating ad catfish
                if (el.querySelector('a, img, iframe')) {
                  isBadBanner = true;
                }
              }
            }

            if (isBadBanner) {
              el.remove();
              continue;
            }
          }
        }
      }
    }

    // 3.3 Heuristic token sweeping on <a>, <img>, <iframe>, <div data-*>
    const candidates = document.querySelectorAll('a, img, iframe, div[class*="banner"], div[id*="banner"], div[class*="ads"], div[id*="ads"]');
    for (let i = 0; i < candidates.length; i++) {
      const el = candidates[i];
      const href = (el.getAttribute('href') || '').toLowerCase();
      const src = (el.getAttribute('src') || '').toLowerCase();
      const alt = (el.getAttribute('alt') || '').toLowerCase();
      const title = (el.getAttribute('title') || '').toLowerCase();
      const id = (el.getAttribute('id') || '').toLowerCase();
      const className = (el.getAttribute('class') || '').toLowerCase();

      const combined = href + ' ' + src + ' ' + alt + ' ' + title + ' ' + id + ' ' + className;

      for (let t = 0; t < BAD_TOKENS.length; t++) {
        const token = BAD_TOKENS[t];
        if (combined.indexOf(token) !== -1) {
          // Check if parent container was only wrapping this ad
          const parent = el.parentElement;
          el.remove();
          if (parent && parent !== document.body && parent.children.length === 0 && parent.textContent.trim() === '') {
            parent.remove();
          }
          break;
        }
      }
    }
  }

  // Run on DOM ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', sweepDOM);
  } else {
    sweepDOM();
  }

  // Debounced MutationObserver with requestAnimationFrame (150ms)
  let sweepScheduled = false;
  const observer = new MutationObserver(function () {
    if (!sweepScheduled) {
      sweepScheduled = true;
      setTimeout(function () {
        requestAnimationFrame(function () {
          sweepDOM();
          sweepScheduled = false;
        });
      }, 150);
    }
  });

  if (document.documentElement) {
    observer.observe(document.documentElement, { childList: true, subtree: true });
  }
})();
