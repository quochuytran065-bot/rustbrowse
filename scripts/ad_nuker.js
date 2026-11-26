// ============================================================================
// RustBrowse Universal Ad Nuker (scripts/ad_nuker.js)
// Runs at document_start in ALL frames & iframes.
// Zero-Tolerance Defense: Self-hosted gambling banners, catfish, click-jacking,
// popunders, sponsored feeds, video ads, and anti-adblock walls.
// ============================================================================

(() => {
    if (window.__RUSTBROWSE_AD_NUKER_ACTIVE__) return;
    window.__RUSTBROWSE_AD_NUKER_ACTIVE__ = true;

    const hostname = window.location.hostname.toLowerCase();

    // ------------------------------------------------------------------------
    // 1. Blacklist tokens & patterns for gambling, popunder, affiliate
    // ------------------------------------------------------------------------
    const AD_TOKENS = [
        "tx88", "lu88", "huvip", "hbet", "man88", "vu88", "k88", "sin88",
        "da88", "9bet", "go88", "sunwin", "hitclub", "kubet", "jun88", "hi88",
        "f8bet", "new88", "789bet", "shbet", "mb66", "qq88", "ok9", "rikvip",
        "banca", "lode", "xoso", "taixiu", "nohu", "nhacai", "casino",
        "c=cpd", "short-link", "clickadu", "popunder", "popads", "popcash",
        "adsterra", "propellerads", "exoclick", "hilltopads", "monetag",
        "trafficjunky", "realsrv", "tsyndicate", "shope.ee", "accesstrade",
        "isclix", "invol.co", "topbanner", "catfish"
    ];

    const SPONSORED_LABELS = [
        "được tài trợ", "sponsored", "tin tài trợ", "quảng cáo", "sản phẩm tài trợ"
    ];

    function isBlacklistedUrl(url) {
        if (!url || typeof url !== "string") return false;
        const lower = url.toLowerCase();
        return AD_TOKENS.some(token => lower.includes(token));
    }

    function isExternalLink(href) {
        if (!href || typeof href !== "string") return false;
        try {
            const parsed = new URL(href, window.location.href);
            return parsed.hostname && !parsed.hostname.endsWith(hostname) && !hostname.endsWith(parsed.hostname);
        } catch (_) {
            return false;
        }
    }

    function notifyHostBlocked(action, url) {
        try {
            if (window.chrome && window.chrome.webview) {
                window.chrome.webview.postMessage({
                    action: action,
                    url: url || window.location.href,
                    timestamp: Date.now()
                });
            }
        } catch (_) {}
    }

    // ------------------------------------------------------------------------
    // 2. Global window.open & Capture-Phase Click Hijack Guard
    // ------------------------------------------------------------------------
    const originalOpen = window.open;
    window.open = function(url, target, features) {
        const targetUrl = url ? String(url) : "";
        if (isBlacklistedUrl(targetUrl) || (targetUrl && isExternalLink(targetUrl))) {
            notifyHostBlocked("BlockedPopup", targetUrl);
            return null;
        }
        return originalOpen.call(window, url, target, features);
    };

    window.addEventListener("click", (e) => {
        let el = e.target;
        while (el && el !== document.documentElement) {
            if (el.tagName === "A" && el.href) {
                if (isBlacklistedUrl(el.href)) {
                    e.preventDefault();
                    e.stopImmediatePropagation();
                    notifyHostBlocked("BlockedPopup", el.href);
                    el.remove();
                    return false;
                }
            }
            el = el.parentElement;
        }
    }, true);

    // ------------------------------------------------------------------------
    // 3. YouTube & Spotify Web Ad Elimination Hooks
    // ------------------------------------------------------------------------
    if (hostname.includes("youtube.com")) {
        const origParse = JSON.parse;
        JSON.parse = function(text, reviver) {
            const data = origParse.call(JSON, text, reviver);
            if (data && typeof data === "object") {
                if (data.adPlacements) delete data.adPlacements;
                if (data.playerAds) delete data.playerAds;
                if (data.adSlots) delete data.adSlots;
            }
            return data;
        };

        if (window.Response && Response.prototype.json) {
            const origJson = Response.prototype.json;
            Response.prototype.json = async function() {
                const data = await origJson.call(this);
                if (data && typeof data === "object") {
                    if (data.adPlacements) delete data.adPlacements;
                    if (data.playerAds) delete data.playerAds;
                    if (data.adSlots) delete data.adSlots;
                }
                return data;
            };
        }
    }

    if (hostname.includes("spotify.com")) {
        const origFetch = window.fetch;
        if (origFetch) {
            window.fetch = function(input, init) {
                const url = typeof input === "string" ? input : (input && input.url ? input.url : "");
                if (url.includes("/ads/") || url.includes("/ad-logic/") || url.includes("adeventtracker")) {
                    notifyHostBlocked("BlockedRequest", url);
                    return Promise.resolve(new Response("{}", { status: 200, headers: { "Content-Type": "application/json" } }));
                }
                return origFetch.apply(this, arguments);
            };
        }
    }

    // ------------------------------------------------------------------------
    // 4. DOM Cleaning Engine (Self-Hosted Banners, Catfish, Transparent Overlays)
    // ------------------------------------------------------------------------
    function cleanDom() {
        const viewW = window.innerWidth || document.documentElement.clientWidth || 800;
        const viewH = window.innerHeight || document.documentElement.clientHeight || 600;

        // A. Destroy transparent click-jacking overlays
        const allDivs = document.querySelectorAll("div, a");
        for (let i = 0; i < allDivs.length; i++) {
            const el = allDivs[i];
            const z = parseInt(window.getComputedStyle(el).zIndex || "0", 10);
            if (z >= 999) {
                const rect = el.getBoundingClientRect();
                const area = rect.width * rect.height;
                const screenArea = viewW * viewH;
                if (area >= screenArea * 0.40) {
                    const style = window.getComputedStyle(el);
                    const isTransparent = style.opacity === "0" ||
                        style.backgroundColor === "transparent" ||
                        style.backgroundColor === "rgba(0, 0, 0, 0)";
                    if (isTransparent && !el.innerText.trim()) {
                        el.remove();
                    }
                }
            }
        }

        // B. Destroy Self-hosted banners / gambling ads based on size & aspect ratio
        const links = document.querySelectorAll("a[href]");
        for (let i = 0; i < links.length; i++) {
            const a = links[i];
            const href = a.href;
            if (isBlacklistedUrl(href)) {
                a.remove();
                continue;
            }

            if (isExternalLink(href)) {
                const media = a.querySelector("img, picture, video");
                if (media) {
                    const r = media.getBoundingClientRect();
                    const w = r.width || media.naturalWidth || 0;
                    const h = r.height || media.naturalHeight || 0;
                    const src = (media.currentSrc || media.src || "").toLowerCase();

                    const isBannerAspect = w >= 180 && h > 0 && (w / h >= 2.5);
                    const isStandardAdSize =
                        (w === 728 && h === 90) || (w === 970 && h === 90) ||
                        (w === 320 && h === 50) || (w === 300 && h === 250) ||
                        (w === 160 && h === 600);
                    const isGifAd = src.endsWith(".gif") || src.includes(".webp");

                    if (isBannerAspect || isStandardAdSize || (isGifAd && isExternalLink(src))) {
                        // Remove link and parent wrapper if parent only holds this ad
                        const parent = a.parentElement;
                        a.remove();
                        if (parent && parent.children.length === 0 && !parent.innerText.trim()) {
                            parent.remove();
                        }
                    }
                }
            }
        }

        // C. Destroy Sticky / Fixed Catfish ads
        const fixedElements = document.querySelectorAll("div, aside, section");
        for (let i = 0; i < fixedElements.length; i++) {
            const el = fixedElements[i];
            if (el.tagName === "HEADER" || el.tagName === "NAV") continue;
            const pos = window.getComputedStyle(el).position;
            if (pos === "fixed" || pos === "sticky") {
                const rect = el.getBoundingClientRect();
                // Bottom catfish or side floating banners
                if (rect.height > 0 && rect.height <= 200 && rect.width >= viewW * 0.7) {
                    const hasExt = el.querySelector("a[href]");
                    const hasGif = el.querySelector("img[src*='.gif'], img[src*='short-link'], a[href*='c=cpd']");
                    const hasClose = el.querySelector("[class*='close'], [id*='close'], span:contains('X')");
                    if (hasGif || (hasExt && isExternalLink(hasExt.href)) || hasClose) {
                        el.remove();
                    }
                }
            }
        }

        // D. Destroy Sponsored / Native Articles on social feeds & news
        const articles = document.querySelectorAll("div[role='article'], div[data-testid='feed_story'], div[class*='feed-item']");
        for (let i = 0; i < articles.length; i++) {
            const art = articles[i];
            const text = (art.innerText || "").toLowerCase();
            if (SPONSORED_LABELS.some(label => text.includes(label))) {
                art.remove();
            }
        }

        // E. Anti-Adblock Wall Neutralization
        const html = document.documentElement;
        const body = document.body;
        if (html && window.getComputedStyle(html).overflow === "hidden") {
            html.style.setProperty("overflow", "auto", "important");
        }
        if (body && window.getComputedStyle(body).overflow === "hidden") {
            body.style.setProperty("overflow", "auto", "important");
        }
        const overlays = document.querySelectorAll("div[class*='adblock'], div[id*='adblock'], div[class*='blocker']");
        for (let i = 0; i < overlays.length; i++) {
            overlays[i].remove();
        }

        // F. YouTube Video Ad Fast-Forward Fallback
        if (hostname.includes("youtube.com")) {
            const adShowing = document.querySelector(".ad-showing, .ad-interrupting");
            if (adShowing) {
                const video = document.querySelector("video");
                if (video && !isNaN(video.duration)) {
                    video.currentTime = video.duration;
                }
                const skipBtn = document.querySelector(".ytp-ad-skip-button, .ytp-ad-skip-button-modern");
                if (skipBtn) skipBtn.click();
            }
            const merch = document.querySelectorAll("ytd-merch-shelf-renderer, ytd-ad-slot-renderer, #masthead-ad");
            merch.forEach(m => m.remove());
        }
    }

    // ------------------------------------------------------------------------
    // 5. Debounced Observer (100ms via requestAnimationFrame)
    // ------------------------------------------------------------------------
    let scheduleId = null;
    function scheduleClean() {
        if (scheduleId) return;
        scheduleId = requestAnimationFrame(() => {
            scheduleId = null;
            cleanDom();
        });
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", cleanDom);
    } else {
        cleanDom();
    }

    const observer = new MutationObserver((mutations) => {
        let hasNewNodes = false;
        for (let i = 0; i < mutations.length; i++) {
            if (mutations[i].addedNodes.length > 0) {
                hasNewNodes = true;
                break;
            }
        }
        if (hasNewNodes) {
            scheduleClean();
        }
    });

    observer.observe(document.documentElement || document, {
        childList: true,
        subtree: true
    });

    setInterval(cleanDom, 1000);
})();
