//! Common primitives, data structures, contracts, and configuration for RustBrowse.

pub mod clock;
pub mod config;
pub mod engine;
pub mod errors;
pub mod messages;
pub mod optimizer;
pub mod types;

pub use clock::{Clock, MockClock, SystemClock};
pub use config::{BrowserConfig, ChromiumFlagsConfig, MemoryConfig, MonitorConfig};
pub use engine::Engine;
pub use errors::{Result, RustBrowseError};
pub use messages::{EngineEvent, TabPolicyAction, UiCommand};
pub use optimizer::StartupOptimizer;
pub use types::{
    DpiScale, MemoryPressureTier, Rect, SystemMemorySnapshot, TabId, TabLifecycleState, TabMetadata,
};
