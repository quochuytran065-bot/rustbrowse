//! Session persistence and recovery manager.

pub mod recovery;
pub mod service;
pub mod session;

pub use recovery::{CrashRecoveryDetector, RecoveryStatus};
pub use service::SessionService;
pub use session::{SessionSnapshot, SessionTab};
