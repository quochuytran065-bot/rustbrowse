//! Central AdblockEngine facade orchestrating network filtering, cosmetic filtering, and statistics.

use crate::cosmetic_filter::CosmeticFilter;
use crate::exceptions::DomainExceptions;
use crate::filter_lists::FilterLists;
use crate::network_filter::{FilterDecision, NetworkFilter};
use crate::rule_cache::RuleCache;
use crate::statistics::AdblockStats;
use adblock::Engine;
use std::path::Path;
use std::sync::Arc;

pub struct AdblockEngine {
    network_filter: NetworkFilter,
    exceptions: Arc<DomainExceptions>,
    stats: Arc<AdblockStats>,
}

impl AdblockEngine {
    /// Creates a new engine with specified rule lines.
    pub fn new(rules: &[String]) -> Self {
        let text = rules.join("\n");
        let engine = Engine::new_with_list_text(text);
        let exceptions = Arc::new(DomainExceptions::new());
        let stats = Arc::new(AdblockStats::new());
        let network_filter = NetworkFilter::new(engine, exceptions.clone(), stats.clone());

        Self {
            network_filter,
            exceptions,
            stats,
        }
    }

    /// Initializes engine using default built-in high-impact filter rules.
    pub fn default_engine() -> Self {
        let rules = FilterLists::default_rules();
        Self::new(&rules)
    }

    /// Fast startup: loads serialized binary cache if exists, otherwise compiles rules and saves cache.
    pub fn load_or_create<P: AsRef<Path>>(cache_path: P) -> Self {
        let exceptions = Arc::new(DomainExceptions::new());
        let stats = Arc::new(AdblockStats::new());
        let parent_dir = cache_path.as_ref().parent();

        let (rules, network_count, cosmetic_count) = FilterLists::load_comprehensive_rules(parent_dir);

        let engine = if cache_path.as_ref().exists() {
            match RuleCache::load_from_file(&cache_path) {
                Ok(eng) => eng,
                Err(e) => {
                    tracing::warn!("Failed to load adblock cache, compiling rules: {:?}", e);
                    let eng = Engine::new_with_list_text(rules.join("\n"));
                    let _ = RuleCache::save_to_file(&eng, &cache_path);
                    eng
                }
            }
        } else {
            let eng = Engine::new_with_list_text(rules.join("\n"));
            let _ = RuleCache::save_to_file(&eng, &cache_path);
            eng
        };

        tracing::info!("[SHIELD] Loaded {} network rules & {} cosmetic rules", network_count, cosmetic_count);
        println!("[SHIELD] Loaded {} network rules & {} cosmetic rules", network_count, cosmetic_count);

        let network_filter = NetworkFilter::new(engine, exceptions.clone(), stats.clone());

        Self {
            network_filter,
            exceptions,
            stats,
        }
    }

    pub fn check_request(
        &self,
        url: &str,
        source_url: &str,
        resource_type: &str,
        method: &str,
        current_epoch_ms: u64,
    ) -> FilterDecision {
        self.network_filter
            .evaluate_request(url, source_url, resource_type, method, current_epoch_ms)
    }

    pub fn get_cosmetic_script(&self, host: &str) -> String {
        CosmeticFilter::build_injection_script(host)
    }

    pub fn exceptions(&self) -> &Arc<DomainExceptions> {
        &self.exceptions
    }

    pub fn stats(&self) -> &Arc<AdblockStats> {
        &self.stats
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.network_filter.set_enabled(enabled);
    }

    pub fn is_enabled(&self) -> bool {
        self.network_filter.is_enabled()
    }
}
