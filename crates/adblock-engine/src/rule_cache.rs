//! Binary serialization and deserialization cache for compiled filter rules.

use adblock::Engine;
use std::fs;
use std::io;
use std::path::Path;

pub struct RuleCache;

impl RuleCache {
    /// Saves compiled `adblock::Engine` bytes directly to the specified path.
    pub fn save_to_file<P: AsRef<Path>>(engine: &Engine, path: P) -> io::Result<()> {
        let bytes = engine.serialize();
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)
    }

    /// Loads serialized filter engine from disk cache.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> io::Result<Engine> {
        let bytes = fs::read(path)?;
        let mut engine = Engine::new_with_list_text("");
        engine.deserialize(&bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to deserialize adblock cache: {:?}", e),
            )
        })?;
        Ok(engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_cache_roundtrip() {
        let rules = "||doubleclick.net^\n||adservice.google.com^";
        let engine = Engine::new_with_list_text(rules);

        let temp_dir = std::env::temp_dir();
        let cache_path = temp_dir.join("rustbrowse_test_adblock.dat");

        RuleCache::save_to_file(&engine, &cache_path).expect("Failed to save cache");
        assert!(cache_path.exists());

        let restored = RuleCache::load_from_file(&cache_path).expect("Failed to load cache");
        let _ = fs::remove_file(cache_path);

        // Verify restored engine blocks
        let req = adblock::request::Request::new(
            "https://doubleclick.net/ad.js",
            "https://example.com",
            "script",
            "GET",
        )
        .unwrap();
        let res = restored.check_network_request(&req);
        assert!(res.filter.is_some());
    }
}
