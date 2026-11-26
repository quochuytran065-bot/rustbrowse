//! Tab management and lifecycle policy enforcement.

pub mod manager;
pub mod tab_policy;
pub mod tab_state;

pub use manager::TabManager;
pub use tab_policy::evaluate_tab_transition;
pub use tab_state::TabStateStore;
