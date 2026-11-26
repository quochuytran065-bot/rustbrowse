//! Permission requested handling and origin-based persistence.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionKind {
    Geolocation,
    Notifications,
    Camera,
    Microphone,
    ClipboardRead,
    Unknown,
}

impl PermissionKind {
    pub fn from_raw_type(raw: &str) -> Self {
        match raw.to_lowercase().as_str() {
            "geolocation" | "location" => Self::Geolocation,
            "notifications" => Self::Notifications,
            "camera" | "video_capture" => Self::Camera,
            "microphone" | "audio_capture" => Self::Microphone,
            "clipboard_read" => Self::ClipboardRead,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionDecision {
    Allow,
    Deny,
    Prompt,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PermissionManager {
    // Key: (origin, permission_kind) -> Decision
    permissions: HashMap<(String, PermissionKind), PermissionDecision>,
}

impl PermissionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates current permission decision for an origin.
    /// Default policy is `Prompt` if never explicitly allowed or denied.
    pub fn query(&self, origin: &str, kind: PermissionKind) -> PermissionDecision {
        let key = (Self::normalize_origin(origin), kind);
        self.permissions
            .get(&key)
            .copied()
            .unwrap_or(PermissionDecision::Prompt)
    }

    pub fn grant(&mut self, origin: &str, kind: PermissionKind) {
        let key = (Self::normalize_origin(origin), kind);
        self.permissions.insert(key, PermissionDecision::Allow);
    }

    pub fn deny(&mut self, origin: &str, kind: PermissionKind) {
        let key = (Self::normalize_origin(origin), kind);
        self.permissions.insert(key, PermissionDecision::Deny);
    }

    pub fn revoke(&mut self, origin: &str, kind: PermissionKind) {
        let key = (Self::normalize_origin(origin), kind);
        self.permissions.remove(&key);
    }

    pub fn revoke_all_for_origin(&mut self, origin: &str) {
        let normalized = Self::normalize_origin(origin);
        self.permissions.retain(|(o, _), _| o != &normalized);
    }

    fn normalize_origin(origin: &str) -> String {
        if let Ok(url) = url::Url::parse(origin) {
            url.origin().ascii_serialization()
        } else {
            origin.trim().to_lowercase()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_query_default_prompt() {
        let pm = PermissionManager::new();
        assert_eq!(
            pm.query("https://example.com", PermissionKind::Camera),
            PermissionDecision::Prompt
        );
    }

    #[test]
    fn test_permission_grant_and_revoke() {
        let mut pm = PermissionManager::new();
        let origin = "https://maps.google.com/search";

        pm.grant(origin, PermissionKind::Geolocation);
        assert_eq!(
            pm.query("https://maps.google.com", PermissionKind::Geolocation),
            PermissionDecision::Allow
        );

        pm.revoke("https://maps.google.com", PermissionKind::Geolocation);
        assert_eq!(
            pm.query("https://maps.google.com", PermissionKind::Geolocation),
            PermissionDecision::Prompt
        );
    }

    #[test]
    fn test_permission_deny() {
        let mut pm = PermissionManager::new();
        let origin = "https://spam.com";

        pm.deny(origin, PermissionKind::Notifications);
        assert_eq!(
            pm.query(origin, PermissionKind::Notifications),
            PermissionDecision::Deny
        );
    }
}
