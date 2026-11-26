//! User domain exceptions and whitelist management.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::RwLock;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExceptionStore {
    whitelisted_domains: HashSet<String>,
}

#[derive(Debug, Default)]
pub struct DomainExceptions {
    store: RwLock<ExceptionStore>,
}

impl DomainExceptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_domains(domains: &[&str]) -> Self {
        let mut set = HashSet::new();
        for d in domains {
            set.insert(d.trim().to_lowercase());
        }
        Self {
            store: RwLock::new(ExceptionStore {
                whitelisted_domains: set,
            }),
        }
    }

    pub fn add(&self, domain: &str) {
        let clean = domain.trim().to_lowercase();
        if !clean.is_empty() {
            if let Ok(mut lock) = self.store.write() {
                lock.whitelisted_domains.insert(clean);
            }
        }
    }

    pub fn remove(&self, domain: &str) -> bool {
        let clean = domain.trim().to_lowercase();
        if let Ok(mut lock) = self.store.write() {
            lock.whitelisted_domains.remove(&clean)
        } else {
            false
        }
    }

    pub fn is_exempt(&self, domain: &str) -> bool {
        let clean = domain.trim().to_lowercase();
        if let Ok(lock) = self.store.read() {
            if lock.whitelisted_domains.contains(&clean) {
                return true;
            }
            // Also check parent domain (e.g., sub.example.com exempt if example.com is exempt)
            for item in &lock.whitelisted_domains {
                if clean.ends_with(&format!(".{}", item)) {
                    return true;
                }
            }
        }
        false
    }

    pub fn all_whitelisted(&self) -> Vec<String> {
        self.store
            .read()
            .map(|l| l.whitelisted_domains.iter().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_exceptions() {
        let exceptions = DomainExceptions::new();
        exceptions.add("example.com");

        assert!(exceptions.is_exempt("example.com"));
        assert!(exceptions.is_exempt("sub.example.com"));
        assert!(!exceptions.is_exempt("other.com"));

        assert!(exceptions.remove("example.com"));
        assert!(!exceptions.is_exempt("example.com"));
    }
}
