//! Network request classification and filter decision engine.

use crate::exceptions::DomainExceptions;
use crate::site_rules::SiteRules;
use crate::statistics::AdblockStats;
use adblock::request::Request;
use adblock::Engine;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterDecision {
    Allow,
    Block,
    Whitelisted,
}

pub struct NetworkFilter {
    engine: Engine,
    exceptions: Arc<DomainExceptions>,
    stats: Arc<AdblockStats>,
    enabled: AtomicBool,
}

impl NetworkFilter {
    pub fn new(
        engine: Engine,
        exceptions: Arc<DomainExceptions>,
        stats: Arc<AdblockStats>,
    ) -> Self {
        Self {
            engine,
            exceptions,
            stats,
            enabled: AtomicBool::new(true),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Evaluates an outbound HTTP request against rules, exceptions, and platform safeguards.
    pub fn evaluate_request(
        &self,
        url: &str,
        source_url: &str,
        resource_type: &str,
        method: &str,
        current_epoch_ms: u64,
    ) -> FilterDecision {
        if !self.is_enabled() {
            return FilterDecision::Allow;
        }

        // 1. Check user whitelist / exceptions for top-level source domain
        let source_host = Url::parse(source_url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_default();

        if !source_host.is_empty() && self.exceptions.is_exempt(&source_host) {
            return FilterDecision::Whitelisted;
        }

        // 2. Check platform-specific preservation safeguards (never break media or auth)
        if SiteRules::is_youtube_protected_request(url)
            || SiteRules::is_spotify_protected_request(url)
            || SiteRules::is_facebook_protected_request(url)
            || SiteRules::is_tiktok_protected_request(url)
        {
            return FilterDecision::Allow;
        }

        // 3. Check platform-specific ad/tracker definitions
        let is_platform_ad = SiteRules::is_youtube_ad_or_tracker(url)
            || SiteRules::is_spotify_ad_or_tracker(url)
            || SiteRules::is_facebook_ad_or_tracker(url)
            || SiteRules::is_tiktok_ad_or_tracker(url);

        if is_platform_ad {
            let target_host = Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(|s| s.to_string()))
                .unwrap_or_else(|| source_host.clone());

            let is_tracker = url.contains("analytics") || url.contains("stats") || url.contains("pixel") || url.contains("tr");
            let est_size = if resource_type == "image" || resource_type == "media" { 50_000 } else { 15_000 };
            self.stats.record_blocked(&target_host, is_tracker, est_size, current_epoch_ms);
            return FilterDecision::Block;
        }

        // 4. Query core adblock engine rules
        let req = match Request::new(url, source_url, resource_type, method) {
            Ok(r) => r,
            Err(_) => return FilterDecision::Allow,
        };

        let result = self.engine.check_network_request(&req);

        if result.filter.is_some() {
            if result.important || result.exception.is_none() {
                let target_host = Url::parse(url)
                    .ok()
                    .and_then(|u| u.host_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| source_host.clone());

                let is_tracker = resource_type == "ping" || url.contains("tracker") || url.contains("analytics");
                let est_size = if resource_type == "script" { 35_000 } else { 10_000 };
                self.stats.record_blocked(&target_host, is_tracker, est_size, current_epoch_ms);
                FilterDecision::Block
            } else {
                FilterDecision::Allow
            }
        } else {
            FilterDecision::Allow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_filter_decision() {
        let rules = vec!["||badadserver.com^".to_string()];
        let adblock_engine = Engine::new_with_list_text(rules.join("\n"));
        let exceptions = Arc::new(DomainExceptions::new());
        let stats = Arc::new(AdblockStats::new());

        let filter = NetworkFilter::new(adblock_engine, exceptions.clone(), stats.clone());

        let dec = filter.evaluate_request(
            "https://badadserver.com/banner.js",
            "https://news.com/article",
            "script",
            "GET",
            1000,
        );
        assert_eq!(dec, FilterDecision::Block);
        assert_eq!(stats.total_ads_blocked(), 1);

        // Whitelist news.com
        exceptions.add("news.com");
        let dec2 = filter.evaluate_request(
            "https://badadserver.com/banner.js",
            "https://news.com/article",
            "script",
            "GET",
            1001,
        );
        assert_eq!(dec2, FilterDecision::Whitelisted);
    }
}
