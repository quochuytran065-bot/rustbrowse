//! Filter list definitions, embedded rules, and background auto-updating rules loader.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;

pub const EMBEDDED_VN_GAMBLING_RULES: &str = include_str!("../assets/vn_anti_gambling_ads.txt");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterListMetadata {
    pub id: String,
    pub name: String,
    pub url: String,
    pub filename: String,
    pub enabled: bool,
    pub rule_count: usize,
}

pub struct FilterLists;

impl FilterLists {
    /// Remote filter list targets to keep AdBlock constantly updated.
    pub fn remote_filter_lists() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            (
                "abpvn",
                "https://raw.githubusercontent.com/abpvn/abpvn/master/filter/abpvn.txt",
                "abpvn.txt",
            ),
            (
                "easylist",
                "https://easylist.to/easylist/easylist.txt",
                "easylist.txt",
            ),
            (
                "adguard_base",
                "https://filters.adtidy.org/extension/ublock/filters/2.txt",
                "adguard_base.txt",
            ),
            (
                "adguard_annoyances",
                "https://filters.adtidy.org/extension/ublock/filters/14.txt",
                "adguard_annoyances.txt",
            ),
        ]
    }

    /// Returns default essential rules covering common ad networks, telemetry, and tracking beacons.
    pub fn default_rules() -> Vec<String> {
        let mut rules = vec![
            // YouTube Telemetry & Logging Hard-blocks
            "||youtube.com/api/stats/*".to_string(),
            "||youtube.com/api/stats/playback*".to_string(),
            "||youtube.com/api/stats/delayplay*".to_string(),
            "||youtube.com/api/stats/watchtime*".to_string(),
            "||youtube.com/api/stats/qoe*".to_string(),
            "||youtube.com/api/stats/atr*".to_string(),
            "||youtube.com/ptracking*".to_string(),
            "||youtube.com/log_event*".to_string(),
            "||youtube.com/youtubei/v1/log_event*".to_string(),
            "||youtube.com/generate_204*".to_string(),
            "||s.youtube.com/api/stats/ads^".to_string(),
            "||video-ad-stats.googlevideo.com^".to_string(),
            // Facebook Telemetry & Logging Hard-blocks
            "||facebook.com/ajax/bz*".to_string(),
            "||facebook.com/ajax/bz".to_string(),
            "||facebook.com/logging/*".to_string(),
            "||facebook.com/logging".to_string(),
            "||pixel.facebook.com*".to_string(),
            "||facebook.com/tr*".to_string(),
            "||connect.facebook.net/en_US/fbevents.js*".to_string(),
            "||connect.facebook.net/*".to_string(),
            "||graph.facebook.com/logging*".to_string(),
            // TikTok Telemetry & Logging
            "||analytics.tiktok.com^".to_string(),
            "||ads.tiktok.com^".to_string(),
            "||mon.tiktokv.com^".to_string(),
            "||log.tiktokv.com^".to_string(),
            // Google Ads & Tracking
            "||doubleclick.net^".to_string(),
            "||googlesyndication.com^".to_string(),
            "||googleadservices.com^".to_string(),
            "||pagead2.googlesyndication.com^".to_string(),
            "||adservice.google.com^".to_string(),
            "||google-analytics.com^".to_string(),
            "||analytics.google.com^".to_string(),
            // Common Ad Exchanges & Outbrain/Taboola
            "||adnxs.com^".to_string(),
            "||criteo.com^".to_string(),
            "||criteo.net^".to_string(),
            "||taboola.com^".to_string(),
            "||outbrain.com^".to_string(),
            "||rubiconproject.com^".to_string(),
            "||pubmatic.com^".to_string(),
            "||openx.net^".to_string(),
            "||casalemedia.com^".to_string(),
            "||smartadserver.com^".to_string(),
            // Analytics & Trackers
            "||scorecardresearch.com^".to_string(),
            "||quantserve.com^".to_string(),
            "||hotjar.com^".to_string(),
            "||clarity.ms^".to_string(),
            "||segment.io^".to_string(),
            "||mixpanel.com^".to_string(),
            "||spclient.wg.spotify.com/ads/^".to_string(),
        ];

        // Append embedded Vietnamese gambling, popunder, and affiliate defense rules
        for line in EMBEDDED_VN_GAMBLING_RULES.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('!') && !trimmed.starts_with('#') {
                rules.push(trimmed.to_string());
            }
        }

        rules
    }

    /// Loads all comprehensive rules from embedded and local cached filter files.
    /// Also initiates background download if files are missing or older than 7 days.
    pub fn load_comprehensive_rules(cache_dir: Option<&Path>) -> (Vec<String>, usize, usize) {
        let mut rules = Self::default_rules();

        if let Some(dir) = cache_dir {
            let filters_dir = dir.join("filters");
            fs::create_dir_all(&filters_dir).ok();

            // Load any existing local filter lists
            for (_, _, filename) in Self::remote_filter_lists() {
                let file_path = filters_dir.join(filename);
                if file_path.exists() {
                    if let Ok(file_rules) = Self::load_from_file(&file_path) {
                        rules.extend(file_rules);
                    }
                }
            }

            // Spawn background task to download or update filter lists via native curl
            let target_dir = filters_dir.clone();
            thread::Builder::new()
                .name("FilterListUpdater".to_string())
                .spawn(move || {
                    Self::sync_remote_filters(&target_dir);
                })
                .ok();
        }

        // Count network vs cosmetic rules
        let mut network_count = 0;
        let mut cosmetic_count = 0;

        for r in &rules {
            if r.contains("##") || r.contains("#?#") || r.contains("#@#") {
                cosmetic_count += 1;
            } else {
                network_count += 1;
            }
        }

        (rules, network_count, cosmetic_count)
    }

    /// Background synchronizer using native Windows curl.exe to fetch filter lists.
    pub fn sync_remote_filters(filters_dir: &Path) {
        for (id, url, filename) in Self::remote_filter_lists() {
            let dest_path = filters_dir.join(filename);
            let should_download = if dest_path.exists() {
                // If older than 3 days, refresh
                if let Ok(meta) = fs::metadata(&dest_path) {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            elapsed.as_secs() > 3 * 86400
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    true
                }
            } else {
                true
            };

            if should_download {
                tracing::info!(id, url, "Updating remote filter list in background...");
                let temp_path = filters_dir.join(format!("{}.tmp", filename));
                let status = Command::new("curl.exe")
                    .args([
                        "-s",
                        "-L",
                        "--max-time",
                        "30",
                        url,
                        "-o",
                        temp_path.to_string_lossy().as_ref(),
                    ])
                    .status();

                if let Ok(s) = status {
                    if s.success() && temp_path.exists() {
                        let _ = fs::rename(&temp_path, &dest_path);
                        tracing::info!(id, "Filter list updated successfully.");
                    }
                }
            }
        }
    }

    /// Loads filter rules from a file, skipping comments and blank lines.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<String>> {
        let content = fs::read_to_string(path)?;
        let rules: Vec<String> = content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('!') && !l.starts_with('#'))
            .collect();
        Ok(rules)
    }
}
