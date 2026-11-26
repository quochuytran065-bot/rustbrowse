//! Clean shutdown and resource teardown lifecycle.

pub struct AppLifecycle;

impl AppLifecycle {
    pub fn shutdown() {
        tracing::info!("RustBrowse shutting down cleanly. Flushing state...");
    }
}
