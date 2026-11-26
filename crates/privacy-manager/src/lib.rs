//! Privacy, adblock, and tracker filtering manager.

pub mod permissions;
pub mod rule_engine;
pub mod tracker_filter;
pub mod url_normalizer;

pub use permissions::{PermissionDecision, PermissionKind, PermissionManager};
pub use rule_engine::{AdblockRuleEngine, FilterDecision};
pub use tracker_filter::TrackerFilter;
pub use url_normalizer::normalize_omnibox_input;
