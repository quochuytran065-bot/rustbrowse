//! Active Shield Module: Anti-Gambling, Anti-Scam, Anti-Redirect, and Malvertising Blocker.
//! Layer A: Rust & WebView2 URL & Domain Inspection Engine.
//! Sub-microsecond matching using token tables and prefix/suffix matching.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use url::Url;

/// Active Shield engine tracking blocked counts and matching malicious patterns.
#[derive(Debug)]
pub struct ActiveShield {
    blocked_count: AtomicU64,
    gambling_domains: HashSet<&'static str>,
    malvertising_domains: HashSet<&'static str>,
    gambling_tokens: Vec<&'static str>,
    allowlist: HashSet<&'static str>,
}

impl Default for ActiveShield {
    fn default() -> Self {
        Self::new()
    }
}

impl ActiveShield {
    pub fn new() -> Self {
        let mut gambling_domains = HashSet::new();
        // High-profile Vietnamese & Asian gambling, casino, tài xỉu, cá độ domains
        let g_domains = [
            "short-link.app", "tx88.com", "tx88.vip", "lu88.com", "lu88.fun", "lu88.vip",
            "huvip.club", "huvip.top", "hbet.com", "hbet.net", "man88.com", "vu88.com",
            "k88.com", "sin88.com", "da88.com", "9bet.com", "go88.com", "go88.vn",
            "sunwin.vn", "sunwin.club", "hitclub.com", "kubet.com", "kubet88.net",
            "jun88.com", "hi88.com", "f8bet.com", "new88.com", "789bet.com", "shbet.com",
            "mb66.com", "qq88.com", "ok9.com", "rikvip.club", "taixiu88.com", "nohu88.top",
            "nhacai88.com", "soikeo88.com", "banca88.com", "lode88.net", "xoso88.com",
            "bet88.com", "88bet.com", "win88.com", "vip88.com", "club88.com",
            "playkclub.info", "kclub.com", "kclub.info",
        ];
        for d in g_domains {
            gambling_domains.insert(d);
        }

        let mut malvertising_domains = HashSet::new();
        // Malvertising networks, popunder, click-jacking domains & Commercial Ad Platforms
        let m_domains = [
            "exoclick.com", "juicyads.com", "trafficjunky.com", "popads.net",
            "popcash.net", "propellerads.com", "adsterra.com", "mgid.com",
            "taboola.com", "outbrain.com", "clickadu.com", "hilltopads.com",
            "monetag.com", "doubleclick.net", "googlesyndication.com",
            "realsrv.com", "tsyndicate.com", "magsrv.com", "stripchat.com",
            "bongacams.com", "chaturbate.com", "adservice.google.com",
            "googleadservices.com", "criteo.com", "admicro.vn", "eclick.vn",
            "ants.vn", "accesstrade.vn", "isclix.com", "invol.co",
            "amazon-adsystem.com", "adeventtracker.spotify.com",
            "shope.ee", "link1s.com", "link4m.com", "megaurl.in",
        ];
        for d in m_domains {
            malvertising_domains.insert(d);
        }

        // Token list for heuristic scanning in path / query / subdomain
        let gambling_tokens = vec![
            "c=cpd", "topbanner", "catfish", "tx88", "lu88", "huvip", "hbet",
            "man88", "vu88", "k88", "sin88", "da88", "9bet", "go88", "sunwin",
            "hitclub", "kubet", "jun88", "hi88", "f8bet", "new88", "789bet",
            "shbet", "mb66", "qq88", "ok9", "rikvip", "taixiu", "nohu", "nhacai",
            "soikeo", "banca", "lode", "xoso", "casino", "bet88", "88bet",
            "win88", "vip88", "club88", "kclub", "popunder", "popup", "clickadu",
            "popads", "popcash", "adsterra", "propellerads", "exoclick",
            "hilltopads", "monetag", "trafficjunky", "short-link", "realsrv",
            "tsyndicate", "shope.ee", "accesstrade", "isclix",
        ];

        let mut allowlist = HashSet::new();
        // Legitimate domains containing substrings like 'bet', 'win', '88'
        let allowed = [
            "google.com", "youtube.com", "facebook.com", "tiktok.com", "github.com",
            "microsoft.com", "wikipedia.org", "cloudflare.com", "amazon.com",
            "apple.com", "twitter.com", "x.com", "alphabet.com", "better.com",
            "bettles.com", "winzip.com", "windows.com", "netflix.com", "spotify.com",
        ];
        for a in allowed {
            allowlist.insert(a);
        }

        Self {
            blocked_count: AtomicU64::new(0),
            gambling_domains,
            malvertising_domains,
            gambling_tokens,
            allowlist,
        }
    }

    /// Evaluates if a given URL or host is gambling, scam, or malvertising.
    pub fn is_blocked(&self, raw_url: &str) -> bool {
        let parsed = match Url::parse(raw_url) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let host = match parsed.host_str() {
            Some(h) => h.to_lowercase(),
            None => return false,
        };

        // 1. Check Allowlist first to prevent any false positives
        if self.allowlist.contains(host.as_str()) {
            return false;
        }
        for allowed in &self.allowlist {
            if host.ends_with(&format!(".{}", allowed)) {
                return false;
            }
        }

        // 2. Direct exact domain match
        if self.gambling_domains.contains(host.as_str()) || self.malvertising_domains.contains(host.as_str()) {
            self.blocked_count.fetch_add(1, Ordering::Relaxed);
            return true;
        }

        // Check if host ends with any known bad domain (e.g. sub.lu88.com)
        for gd in &self.gambling_domains {
            if host.ends_with(&format!(".{}", gd)) {
                self.blocked_count.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        }
        for md in &self.malvertising_domains {
            if host.ends_with(&format!(".{}", md)) {
                self.blocked_count.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        }

        // 3. Heuristic token inspection on host, path, and query
        let lower_url = raw_url.to_lowercase();
        for token in &self.gambling_tokens {
            if lower_url.contains(token) {
                // Ensure token is not an accidental match in normal query
                if host.contains(token)
                    || lower_url.contains(&format!("c={}", token))
                    || lower_url.contains(&format!("/{}", token))
                    || lower_url.contains(&format!("_{}", token))
                    || lower_url.contains(&format!("-{}", token))
                    || lower_url.contains("short-link")
                    || lower_url.contains("popunder")
                    || lower_url.contains("clickadu")
                {
                    self.blocked_count.fetch_add(1, Ordering::Relaxed);
                    return true;
                }
            }
        }

        false
    }

    /// Evaluates if a requested new window/popup is an untrusted cross-origin popunder.
    pub fn is_untrusted_cross_origin_popup(&self, source_host: &str, target_url: &str) -> bool {
        if self.is_blocked(target_url) {
            return true;
        }

        let parsed = match Url::parse(target_url) {
            Ok(u) => u,
            Err(_) => return true,
        };

        let target_host = match parsed.host_str() {
            Some(h) => h.to_lowercase(),
            None => return true,
        };

        // Same domain or subdomain is legitimate
        if !source_host.is_empty() && (target_host == source_host || target_host.ends_with(&format!(".{}", source_host))) {
            return false;
        }

        // Whitelisted authentic identity / platform providers
        let safe_providers = [
            "google.com", "accounts.google.com", "youtube.com", "facebook.com",
            "github.com", "microsoft.com", "apple.com", "twitter.com", "x.com",
            "wikipedia.org", "cloudflare.com", "netflix.com", "spotify.com",
        ];

        for sp in &safe_providers {
            if target_host == *sp || target_host.ends_with(&format!(".{}", sp)) {
                return false;
            }
        }

        true
    }

    /// Terminal & structured logging for blocked requests
    pub fn log_blocked_request(&self, url: &str) {
        self.record_block();
        tracing::warn!("[SHIELD] Blocked REQUEST: {}", url);
        println!("[SHIELD] Blocked REQUEST: {}", url);
    }

    /// Terminal & structured logging for killed popups
    pub fn log_killed_popup(&self, url: &str) {
        self.record_block();
        tracing::warn!("[SHIELD] KILLED POPUP: {}", url);
        println!("[SHIELD] KILLED POPUP: {}", url);
    }

    /// Terminal & structured logging for blocked redirects
    pub fn log_blocked_redirect(&self, url: &str) {
        self.record_block();
        tracing::warn!("[SHIELD] Blocked REDIRECT: {}", url);
        println!("[SHIELD] Blocked REDIRECT: {}", url);
    }

    /// Whitelist check for essential CDN icon fonts and video streaming media.
    pub fn is_essential_resource(raw_url: &str) -> bool {
        let lower = raw_url.to_lowercase();
        // Whitelist font icons so websites never break
        if lower.contains("fontawesome")
            || lower.contains("fa-")
            || lower.contains("material")
            || lower.contains("icomoon")
            || lower.contains("glyphicons")
            || lower.contains("remixicon")
            || lower.contains("lucide")
            || lower.contains("ionicons")
            || lower.contains("feather")
            || lower.contains("segoe")
        {
            return true;
        }

        // Whitelist video streaming chunks
        if lower.ends_with(".m3u8") || lower.ends_with(".ts") || lower.ends_with(".mp4") {
            return true;
        }

        false
    }

    pub fn blocked_count(&self) -> u64 {
        self.blocked_count.load(Ordering::Relaxed)
    }

    pub fn record_block(&self) {
        self.blocked_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Strips tracking parameters (utm_*, fbclid, gclid, si, ref, igshid, yclid, mc_cid) from a URL.
    pub fn clean_url(raw_url: &str) -> String {
        let mut parsed = match Url::parse(raw_url) {
            Ok(u) => u,
            Err(_) => return raw_url.to_string(),
        };

        let tracking_keys = [
            "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content",
            "fbclid", "gclid", "si", "ref", "igshid", "yclid", "mc_cid",
        ];

        let filtered_pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .filter(|(k, v)| {
                if tracking_keys.iter().any(|&tk| k.eq_ignore_ascii_case(tk)) {
                    return false;
                }
                if k.eq_ignore_ascii_case("c") && v.eq_ignore_ascii_case("cpd") {
                    return false;
                }
                if k.eq_ignore_ascii_case("m") && (v.contains("topbanner") || v.contains("catfish")) {
                    return false;
                }
                true
            })
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        if filtered_pairs.is_empty() {
            parsed.set_query(None);
        } else {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in filtered_pairs {
                serializer.append_pair(&k, &v);
            }
            parsed.set_query(Some(&serializer.finish()));
        }

        parsed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gambling_url_blocking() {
        let shield = ActiveShield::new();
        assert!(shield.is_blocked("https://short-link.app/play?c=cpd&m=topbanner4"));
        assert!(shield.is_blocked("https://lu88.com/games"));
        assert!(shield.is_blocked("https://tx88.vip/taixiu"));
        assert!(shield.is_blocked("https://hbet.com/signup"));
        assert!(shield.is_blocked("https://playkclub.info/index.html"));
        assert!(shield.is_blocked("https://example.com/click?utm_source=clickadu&utm_medium=popunder"));
    }

    #[test]
    fn test_allowlist_never_blocked() {
        let shield = ActiveShield::new();
        assert!(!shield.is_blocked("https://www.google.com/search?q=rust"));
        assert!(!shield.is_blocked("https://youtube.com/watch?v=123"));
        assert!(!shield.is_blocked("https://github.com/better/repo"));
        assert!(!shield.is_blocked("https://www.windows.com"));
    }

    #[test]
    fn test_clean_url_strips_tracking() {
        let raw = "https://example.com/item?id=123&utm_source=facebook&fbclid=abcdef&c=cpd";
        let clean = ActiveShield::clean_url(raw);
        assert_eq!(clean, "https://example.com/item?id=123");
    }

    #[test]
    fn test_cross_origin_popup_detection() {
        let shield = ActiveShield::new();
        // Same domain is permitted
        assert!(!shield.is_untrusted_cross_origin_popup("phimmoichill.net", "https://phimmoichill.net/watch/1"));
        // Cross-origin untrusted popunder is blocked
        assert!(shield.is_untrusted_cross_origin_popup("phimmoichill.net", "https://playkclub.info/join"));
        // Legitimate SSO login is permitted
        assert!(!shield.is_untrusted_cross_origin_popup("example.com", "https://accounts.google.com/o/oauth2/v2/auth"));
    }
}
