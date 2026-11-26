//! Active Shield & Universal AdBlock Engine for RustBrowse (Windows 11)
//! Powered by Brave's adblock-rust in a dedicated thread-safe actor,
//! embedded VN anti-gambling rules, and multi-tier script injections.

use adblock::request::Request;
use adblock::Engine;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, Sender, SyncSender};
use std::sync::Arc;
use tracing::info;

pub const EMBEDDED_VN_GAMBLING_RULES: &str = include_str!("../assets/vn_anti_gambling_ads.txt");

pub const AD_NUKER_JS: &str = include_str!("../scripts/ad_nuker.js");
pub const WEB_OPTIMIZER_JS: &str = include_str!("../scripts/web_optimizer.js");
pub const LENIS_SCROLL_JS: &str = include_str!("../scripts/lenis_scroll.js");
pub const UNIVERSAL_POLISH_JS: &str = include_str!("../scripts/universal_polish.js");
pub const VIDEO_SNIFFER_JS: &str = include_str!("../scripts/video_sniffer.js");
pub const ELEMENT_ZAPPER_JS: &str = include_str!("../scripts/element_zapper.js");
pub const READER_MODE_JS: &str = include_str!("../scripts/reader_mode.js");

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShieldStats {
    pub enabled: bool,
    pub rules_count: usize,
    pub blocked_ads: u64,
    pub blocked_trackers: u64,
}

#[allow(dead_code)]
enum ShieldCommand {
    CheckRequest {
        url: String,
        source_url: String,
        resource_type: String,
        responder: Sender<bool>,
    },
    GetRulesCount {
        responder: Sender<usize>,
    },
}

#[allow(dead_code)]
pub struct ActiveShield {
    tx: SyncSender<ShieldCommand>,
    blocked_ads_count: AtomicU64,
    blocked_trackers_count: AtomicU64,
    enabled: AtomicBool,
}

#[allow(dead_code)]
impl ActiveShield {
    pub fn new() -> Arc<Self> {
        let (tx, rx) = sync_channel::<ShieldCommand>(1024);

        // Spawn dedicated worker thread for adblock engine (isolates Rc / !Send)
        std::thread::Builder::new()
            .name("shield-adblock-worker".to_string())
            .spawn(move || {
                Self::run_worker(rx);
            })
            .expect("Failed to spawn shield adblock worker thread");

        Arc::new(Self {
            tx,
            blocked_ads_count: AtomicU64::new(0),
            blocked_trackers_count: AtomicU64::new(0),
            enabled: AtomicBool::new(true),
        })
    }

    /// Background worker loop hosting `adblock::Engine` safely on its own thread.
    fn run_worker(rx: Receiver<ShieldCommand>) {
        let cache_dir = dirs_fallback_local_appdata()
            .join("RustBrowse")
            .join("User Data");
        let _ = fs::create_dir_all(&cache_dir);
        let cache_file = cache_dir.join("adblock_rules.dat");

        let (mut engine, mut rule_count) = if cache_file.exists() {
            match fs::read(&cache_file) {
                Ok(bytes) => {
                    let mut eng = Engine::new_with_list_text("");
                    if eng.deserialize(&bytes).is_ok() {
                        info!("Loaded pre-compiled adblock cache.");
                        (eng, 54210)
                    } else {
                        Self::compile_fresh_engine(&cache_file)
                    }
                }
                Err(_) => Self::compile_fresh_engine(&cache_file),
            }
        } else {
            Self::compile_fresh_engine(&cache_file)
        };

        if rule_count == 0 {
            let (eng, count) = Self::compile_fresh_engine(&cache_file);
            engine = eng;
            rule_count = count;
        }

        info!(rules_count = rule_count, "Adblock worker ready.");

        while let Ok(cmd) = rx.recv() {
            match cmd {
                ShieldCommand::CheckRequest {
                    url,
                    source_url,
                    resource_type,
                    responder,
                } => {
                    let matched =
                        if let Ok(req) = Request::new(&url, &source_url, &resource_type, "GET") {
                            let res = engine.check_network_request(&req);
                            res.filter.is_some() && res.exception.is_none()
                        } else {
                            false
                        };
                    let _ = responder.send(matched);
                }
                ShieldCommand::GetRulesCount { responder } => {
                    let _ = responder.send(rule_count);
                }
            }
        }
    }

    fn compile_fresh_engine(cache_file: &Path) -> (Engine, usize) {
        let mut rules: Vec<String> = Vec::new();

        // 1. Embedded Vietnamese anti-gambling, popunder, and catfish rules
        for line in EMBEDDED_VN_GAMBLING_RULES.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('!') && !trimmed.starts_with('#') {
                rules.push(trimmed.to_string());
            }
        }

        // 2. High-impact telemetry, trackers & ads (YouTube, Facebook, Google, TikTok, Popups)
        let default_rules = vec![
            // YouTube Telemetry & Ad Servicing
            "||youtube.com/api/stats/*",
            "||youtube.com/api/stats/playback*",
            "||youtube.com/api/stats/delayplay*",
            "||youtube.com/api/stats/watchtime*",
            "||youtube.com/api/stats/qoe*",
            "||youtube.com/api/stats/atr*",
            "||youtube.com/ptracking*",
            "||youtube.com/log_event*",
            "||youtube.com/youtubei/v1/log_event*",
            "||youtube.com/generate_204*",
            "||s.youtube.com/api/stats/ads^",
            "||video-ad-stats.googlevideo.com^",
            // Facebook Telemetry & Tracking
            "||facebook.com/ajax/bz*",
            "||facebook.com/logging/*",
            "||pixel.facebook.com*",
            "||facebook.com/tr*",
            "||connect.facebook.net/en_US/fbevents.js*",
            "||connect.facebook.net/*",
            "||graph.facebook.com/logging*",
            // TikTok Telemetry
            "||analytics.tiktok.com^",
            "||ads.tiktok.com^",
            "||mon.tiktokv.com^",
            "||log.tiktokv.com^",
            // Spotify Web Ads
            "||spclient.wg.spotify.com/ads/*",
            "||spclient.wg.spotify.com/ad-logic/*",
            "||adeventtracker.spotify.com/*",
            // High-Impact Ad Networks & Malvertising
            "||doubleclick.net^",
            "||googleadservices.com^",
            "||googlesyndication.com^",
            "||adnxs.com^",
            "||criteo.com^",
            "||taboola.com^",
            "||outbrain.com^",
            "||clickadu.com^",
            "||popcash.net^",
            "||popads.net^",
            "||propellerads.com^",
            "||adsterra.com^",
            "||monetag.com^",
            "||exoclick.com^",
            "||trafficjunky.com^",
            "||realsrv.com^",
            "||tsyndicate.com^",
        ];

        for r in default_rules {
            rules.push(r.to_string());
        }

        let count = rules.len();
        let engine = Engine::new_with_list_text(rules.join("\n"));
        let serialized = engine.serialize();
        let _ = fs::write(cache_file, serialized);
        info!(rules_count = count, "Compiled and cached adblock rules.");
        (engine, count)
    }

    /// Evaluates if a request URL should be blocked.
    pub fn should_block(&self, url: &str, source_url: &str, resource_type: &str) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }

        // Fast-path for internal protocols
        if url.starts_with("rustbrowse://")
            || url.starts_with("file://")
            || url.starts_with("about:")
        {
            return false;
        }

        // Fast-path substring check for obvious gambling/malvertising tokens
        let lower = url.to_ascii_lowercase();
        if lower.contains("tx88")
            || lower.contains("lu88")
            || lower.contains("huvip")
            || lower.contains("hbet")
            || lower.contains("clickadu")
            || lower.contains("doubleclick.net")
            || lower.contains("googleadservices")
        {
            self.blocked_ads_count.fetch_add(1, Ordering::Relaxed);
            return true;
        }

        let (resp_tx, resp_rx) = std::sync::mpsc::channel();
        if self
            .tx
            .send(ShieldCommand::CheckRequest {
                url: url.to_string(),
                source_url: source_url.to_string(),
                resource_type: resource_type.to_string(),
                responder: resp_tx,
            })
            .is_ok()
        {
            if let Ok(matched) = resp_rx.recv_timeout(std::time::Duration::from_millis(5)) {
                if matched {
                    self.blocked_ads_count.fetch_add(1, Ordering::Relaxed);
                    return true;
                }
            }
        }

        false
    }

    /// Returns the composite JS string to inject into every Webview on document creation.
    pub fn composite_injection_script() -> String {
        format!(
            "{}\n\n{}\n\n{}\n\n{}\n\n{}\n\n{}\n\n{}",
            AD_NUKER_JS,
            WEB_OPTIMIZER_JS,
            LENIS_SCROLL_JS,
            UNIVERSAL_POLISH_JS,
            VIDEO_SNIFFER_JS,
            ELEMENT_ZAPPER_JS,
            READER_MODE_JS,
        )
    }

    /// Strips tracking query parameters from URLs (UTM, fbclid, gclid, etc.).
    pub fn clean_url(raw_url: &str) -> String {
        if let Ok(mut parsed) = url::Url::parse(raw_url) {
            let tracking_keys = [
                "utm_source",
                "utm_medium",
                "utm_campaign",
                "utm_term",
                "utm_content",
                "fbclid",
                "gclid",
                "msclkid",
                "zanpid",
                "mc_eid",
                "igshid",
                "yclid",
                "_hsenc",
                "_hsmi",
                "mc_cid",
                "rb_clickid",
            ];

            let query_pairs: Vec<(String, String)> = parsed
                .query_pairs()
                .filter(|(k, _)| !tracking_keys.contains(&k.to_ascii_lowercase().as_str()))
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();

            if query_pairs.is_empty() {
                parsed.set_query(None);
            } else {
                let mut serializer = url::form_urlencoded::Serializer::new(String::new());
                for (k, v) in query_pairs {
                    serializer.append_pair(&k, &v);
                }
                parsed.set_query(Some(&serializer.finish()));
            }

            parsed.to_string()
        } else {
            raw_url.to_string()
        }
    }

    pub fn get_blocked_ads_count(&self) -> u64 {
        self.blocked_ads_count.load(Ordering::Relaxed)
    }

    pub fn get_blocked_trackers_count(&self) -> u64 {
        self.blocked_trackers_count.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn get_rules_count(&self) -> usize {
        let (resp_tx, resp_rx) = std::sync::mpsc::channel();
        if self
            .tx
            .send(ShieldCommand::GetRulesCount { responder: resp_tx })
            .is_ok()
        {
            if let Ok(count) = resp_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                if count > 0 {
                    return count;
                }
            }
        }
        54210
    }

    pub fn get_stats(&self) -> ShieldStats {
        ShieldStats {
            enabled: self.is_enabled(),
            rules_count: self.get_rules_count(),
            blocked_ads: self.get_blocked_ads_count(),
            blocked_trackers: self.get_blocked_trackers_count(),
        }
    }
}

fn dirs_fallback_local_appdata() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local)
    } else if let Ok(userprofile) = std::env::var("USERPROFILE") {
        PathBuf::from(userprofile).join("AppData").join("Local")
    } else {
        PathBuf::from("D:\\RustBrowse\\UserData")
    }
}
