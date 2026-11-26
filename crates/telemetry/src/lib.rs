//! Telemetry, health checks, and diagnostics.

pub mod crash_report;
pub mod diagnostics;
pub mod metrics;

pub use crash_report::CrashReport;
pub use diagnostics::SystemDiagnostics;
pub use metrics::{BrowserMetrics, MetricsSnapshot};
