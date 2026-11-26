//! Hardware and operating environment diagnostics.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDiagnostics {
    pub app_version: String,
    pub os_platform: String,
    pub cpu_cores: usize,
    pub total_ram_mb: u64,
}

impl SystemDiagnostics {
    pub fn gather(total_ram_mb: u64) -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os_platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            cpu_cores: std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1),
            total_ram_mb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics_gathering() {
        let diag = SystemDiagnostics::gather(4096);
        assert!(!diag.app_version.is_empty());
        assert!(diag.cpu_cores >= 1);
        assert_eq!(diag.total_ram_mb, 4096);
    }
}
