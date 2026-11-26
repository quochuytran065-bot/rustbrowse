//! Adblock Plus & Brave-compatible network filter engine.

use adblock::request::Request;
use adblock::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterDecision {
    Allow,
    Block,
    Whitelisted,
}

pub struct AdblockRuleEngine {
    engine: Engine,
    domain_exceptions: HashSet<String>,
    enabled: bool,
}

impl AdblockRuleEngine {
    /// Creates a new rule engine with initial rule lines.
    pub fn new(rules: &[String]) -> Self {
        let rules_text = rules.join("\n");
        let engine = Engine::new_with_list_text(rules_text);

        Self {
            engine,
            domain_exceptions: HashSet::new(),
            enabled: true,
        }
    }

    /// Creates an empty engine.
    pub fn empty() -> Self {
        let engine = Engine::new_with_list_text("");

        Self {
            engine,
            domain_exceptions: HashSet::new(),
            enabled: true,
        }
    }

    /// Loads pre-serialized filter engine from disk cache for instant startup.
    pub fn load_from_cache<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let mut engine = Engine::new_with_list_text("");
        engine.deserialize(&bytes).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
        })?;

        Ok(Self {
            engine,
            domain_exceptions: HashSet::new(),
            enabled: true,
        })
    }

    /// Serializes compiled filter engine to disk cache.
    pub fn save_to_cache<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let bytes = self.engine.serialize();
        std::fs::write(path, bytes)
    }

    /// Enables or disables global adblocking.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Adds a domain to the user exception whitelist (adblocking bypassed for this site).
    pub fn add_domain_exception(&mut self, domain: &str) {
        self.domain_exceptions.insert(domain.to_lowercase());
    }

    /// Removes a domain from the user exception whitelist.
    pub fn remove_domain_exception(&mut self, domain: &str) {
        self.domain_exceptions.remove(&domain.to_lowercase());
    }

    pub fn is_domain_exempt(&self, domain: &str) -> bool {
        self.domain_exceptions.contains(&domain.to_lowercase())
    }

    /// Checks a network request against filter rules and exception lists.
    pub fn check_request(
        &self,
        url: &str,
        source_url: &str,
        resource_type: &str,
    ) -> FilterDecision {
        if !self.enabled {
            return FilterDecision::Allow;
        }

        // Check if top-level page domain is whitelisted by user
        if let Ok(parsed_src) = url::Url::parse(source_url) {
            if let Some(host) = parsed_src.host_str() {
                if self.is_domain_exempt(host) {
                    return FilterDecision::Whitelisted;
                }
            }
        }

        let request = match Request::new(url, source_url, resource_type, "GET") {
            Ok(req) => req,
            Err(_) => return FilterDecision::Allow,
        };

        let result = self.engine.check_network_request(&request);

        if result.filter.is_some() {
            if result.important || result.exception.is_none() {
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
    fn test_adblock_rule_blocking() {
        let rules = vec![
            "||doubleclick.net^".to_string(),
            "||adservice.google.com^".to_string(),
            "@@||doubleclick.net/allowed^".to_string(),
        ];

        let engine = AdblockRuleEngine::new(&rules);

        // Blocked ad request
        let res = engine.check_request(
            "https://doubleclick.net/ad.js",
            "https://example.com/page",
            "script",
        );
        assert_eq!(res, FilterDecision::Block);

        // Allowed request
        let res = engine.check_request(
            "https://doubleclick.net/allowed",
            "https://example.com/page",
            "script",
        );
        assert_eq!(res, FilterDecision::Allow);

        // Normal page request
        let res = engine.check_request(
            "https://example.com/main.js",
            "https://example.com/page",
            "script",
        );
        assert_eq!(res, FilterDecision::Allow);
    }

    #[test]
    fn test_domain_exception_whitelist() {
        let rules = vec!["||badads.com^".to_string()];
        let mut engine = AdblockRuleEngine::new(&rules);

        // Before whitelist: blocked
        let res = engine.check_request(
            "https://badads.com/banner.png",
            "https://news.com/article",
            "image",
        );
        assert_eq!(res, FilterDecision::Block);

        // Whitelist the source site
        engine.add_domain_exception("news.com");

        // After whitelist: whitelisted
        let res = engine.check_request(
            "https://badads.com/banner.png",
            "https://news.com/article",
            "image",
        );
        assert_eq!(res, FilterDecision::Whitelisted);
    }

    #[test]
    fn test_toggle_disabled() {
        let rules = vec!["||badads.com^".to_string()];
        let mut engine = AdblockRuleEngine::new(&rules);

        engine.set_enabled(false);
        let res = engine.check_request(
            "https://badads.com/banner.png",
            "https://news.com/article",
            "image",
        );
        assert_eq!(res, FilterDecision::Allow);
    }
}
