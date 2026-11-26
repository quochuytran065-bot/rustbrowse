/**
 * RustBrowse — Video & Media AdBlocker (Module 3 / Phụ lục A3)
 * Specialized for: YouTube, Facebook, Spotify Web (SpotX Web Architecture)
 * Injected automatically via add_script_to_execute_on_document_created
 */
(function () {
  'use strict';

  if (window.__rustbrowse_media_adblock_injected) return;
  window.__rustbrowse_media_adblock_injected = true;

  const host = window.location.hostname.toLowerCase();

  // --------------------------------------------------------------------------
  // 1. YouTube Video Ad Protection (JSON Pruning + Instant Skip Fallback)
  // --------------------------------------------------------------------------
  if (host.indexOf('youtube.com') !== -1) {
    // 1.1 Hook JSON.parse to prune ad placements before player initialization
    const originalJsonParse = JSON.parse;
    JSON.parse = function (text, reviver) {
      const data = originalJsonParse.call(this, text, reviver);
      if (data && typeof data === 'object') {
        try {
          if (data.adPlacements) delete data.adPlacements;
          if (data.playerAds) delete data.playerAds;
          if (data.adSlots) delete data.adSlots;
          if (data.adBreakParams) delete data.adBreakParams;
        } catch (_) {}
      }
      return data;
    };

    // 1.2 Hook Response.prototype.json
    if (window.Response && Response.prototype.json) {
      const originalResponseJson = Response.prototype.json;
      Response.prototype.json = function () {
        return originalResponseJson.apply(this, arguments).then((data) => {
          if (data && typeof data === 'object') {
            try {
              if (data.adPlacements) delete data.adPlacements;
              if (data.playerAds) delete data.playerAds;
              if (data.adSlots) delete data.adSlots;
              if (data.adBreakParams) delete data.adBreakParams;
            } catch (_) {}
          }
          return data;
        });
      };
    }

    // 1.3 Fast Video Ad Skip Fallback & Banner Killer
    setInterval(() => {
      // Skip active video ad
      const adContainer = document.querySelector('.ad-showing, .ad-interrupting');
      if (adContainer) {
        const video = document.querySelector('video');
        if (video && isFinite(video.duration) && video.duration > 0) {
          video.currentTime = video.duration;
        }
        const skipBtn = document.querySelector('.ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern');
        if (skipBtn) {
          skipBtn.click();
        }
      }

      // Hide sponsored cards & merch
      const adElements = document.querySelectorAll(
        'ytd-merch-shelf-renderer, ytd-ad-slot-renderer, ytd-promoted-sparkles-web-renderer, ytd-in-feed-ad-layout-renderer, #masthead-ad'
      );
      for (let i = 0; i < adElements.length; i++) {
        adElements[i].remove();
      }
    }, 100);
  }

  // --------------------------------------------------------------------------
  // 2. Facebook Sponsored Post Filter
  // --------------------------------------------------------------------------
  if (host.indexOf('facebook.com') !== -1) {
    function filterFacebookAds() {
      const articles = document.querySelectorAll('div[role="article"]');
      for (let i = 0; i < articles.length; i++) {
        const art = articles[i];
        if (art.dataset.rbFbChecked) continue;
        art.dataset.rbFbChecked = 'true';

        const text = art.innerText || '';
        if (
          text.indexOf('Được tài trợ') !== -1 ||
          text.indexOf('Sponsored') !== -1 ||
          text.indexOf('Được quảng cáo') !== -1
        ) {
          art.style.display = 'none';
          art.remove();
        }
      }
    }
    setInterval(filterFacebookAds, 1000);
  }

  // --------------------------------------------------------------------------
  // 3. Spotify Web AdBlocker (SpotX Web Architecture)
  // --------------------------------------------------------------------------
  if (host.indexOf('spotify.com') !== -1) {
    // 3.1 Hook Fetch to neutralize ad-logic and tracking
    if (window.fetch) {
      const originalFetch = window.fetch;
      window.fetch = function (input, init) {
        const url = typeof input === 'string' ? input : (input && input.url ? input.url : '');
        if (
          url.indexOf('/ads/') !== -1 ||
          url.indexOf('/ad-logic/') !== -1 ||
          url.indexOf('/melody/v1/check_scope') !== -1 ||
          url.indexOf('gabo-receiver-service') !== -1
        ) {
          return Promise.resolve(new Response(JSON.stringify({}), {
            status: 200,
            headers: { 'Content-Type': 'application/json' }
          }));
        }
        return originalFetch.apply(this, arguments);
      };
    }

    // 3.2 Inject SpotX style hiding rules
    const style = document.createElement('style');
    style.textContent = `
      [aria-label*="Upgrade to Premium"],
      [data-testid="upgrade-button"],
      .main-leaderboard-container,
      .Root__top-bar [data-testid="ad-banner"],
      [data-testid="track-info-ad"] {
        display: none !important;
      }
    `;
    document.documentElement.appendChild(style);

    // 3.3 Audio Ad Auto-Skip / Mute Monitor
    setInterval(() => {
      const adTitle = document.querySelector('[data-testid="track-info-ad"]');
      if (adTitle) {
        const nextBtn = document.querySelector('[data-testid="control-button-skip-forward"]');
        if (nextBtn) {
          nextBtn.click();
        }
      }
    }, 500);
  }
})();
