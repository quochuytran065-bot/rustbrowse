//! Profile management with InPrivate support and isolated user data directory paths.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserProfile {
    pub id: String,
    pub name: String,
    pub is_in_private: bool,
    pub data_dir: PathBuf,
}

impl BrowserProfile {
    pub fn default_profile<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            id: "default".to_string(),
            name: "Default Profile".to_string(),
            is_in_private: false,
            data_dir: base_dir.as_ref().join("Default"),
        }
    }

    pub fn in_private_profile<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            id: "in_private".to_string(),
            name: "InPrivate".to_string(),
            is_in_private: true,
            data_dir: base_dir.as_ref().join("InPrivate"),
        }
    }
}

pub struct ProfileManager {
    base_dir: PathBuf,
    profiles: HashMap<String, BrowserProfile>,
    active_profile_id: String,
}

impl ProfileManager {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        let base = base_dir.as_ref().to_path_buf();
        let default_prof = BrowserProfile::default_profile(&base);
        let mut profiles = HashMap::new();
        profiles.insert(default_prof.id.clone(), default_prof.clone());

        Self {
            base_dir: base,
            profiles,
            active_profile_id: default_prof.id,
        }
    }

    pub fn active_profile(&self) -> &BrowserProfile {
        self.profiles
            .get(&self.active_profile_id)
            .expect("Active profile must exist")
    }

    pub fn get_or_create_in_private(&mut self) -> &BrowserProfile {
        if !self.profiles.contains_key("in_private") {
            let in_private = BrowserProfile::in_private_profile(&self.base_dir);
            self.profiles.insert("in_private".to_string(), in_private);
        }
        self.profiles.get("in_private").unwrap()
    }

    pub fn is_in_private_active(&self) -> bool {
        self.active_profile().is_in_private
    }

    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_manager_lifecycle() {
        let temp_dir = std::env::temp_dir().join("rustbrowse_prof_test");
        let mut pm = ProfileManager::new(&temp_dir);

        assert_eq!(pm.profile_count(), 1);
        assert!(!pm.is_in_private_active());
        assert_eq!(pm.active_profile().id, "default");

        let in_priv = pm.get_or_create_in_private();
        assert!(in_priv.is_in_private);
        assert_eq!(pm.profile_count(), 2);
    }
}
