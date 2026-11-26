//! RustBrowse high-performance AdBlock Engine powered by Brave's `adblock` crate.
//! Features network filtering, cosmetic element hiding, disk caching, site rules, and real-time statistics.

pub mod cosmetic_filter;
pub mod engine;
pub mod exceptions;
pub mod filter_lists;
pub mod network_filter;
pub mod rule_cache;
pub mod site_rules;
pub mod statistics;

pub use cosmetic_filter::CosmeticFilter;
pub use engine::AdblockEngine;
pub use exceptions::DomainExceptions;
pub use filter_lists::{FilterListMetadata, FilterLists};
pub use network_filter::{FilterDecision, NetworkFilter};
pub use rule_cache::RuleCache;
pub use site_rules::{Platform, SiteRules};
pub use statistics::{AdblockStats, DomainStats};
