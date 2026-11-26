//! Universal Video Download Manager for RustBrowse (Windows 11, Tauri v2)
//! - Stores per-tab detected videos reported by `video_sniffer.js` (honest DRM flagging).
//! - Direct MP4/WebM async streaming downloads with Referer/User-Agent, Range resume,
//!   collision-safe filenames, real progress/speed reporting, pause/cancel, and Explorer reveal.
//! - Native Rust HLS (`.m3u8`) master/media playlist parser and concurrent segment joiner.
//! - Official `yt-dlp.exe` auto-helper (`%LOCALAPPDATA%\RustBrowse\bin\yt-dlp.exe`) for
//!   non-DRM complex sites (YouTube, Facebook, TikTok, etc.).
//! - Concurrency capped at 3 simultaneous downloads to protect network & disk I/O.

use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, RANGE, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, Webview};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Semaphore;
use tracing::{info, warn};
use url::Url;

const MAX_CONCURRENT_DOWNLOADS: usize = 3;
const HLS_SEGMENT_CONCURRENCY: usize = 4;
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const YT_DLP_RELEASE_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedVideo {
    pub id: String,
    #[serde(default, alias = "tab_id")]
    pub tab_id: u32,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub quality: String,
    #[serde(default)]
    pub format: String,
    #[serde(default, alias = "is_drm")]
    pub is_drm: bool,
    #[serde(default)]
    pub referer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadItem {
    pub id: String,
    pub title: String,
    pub url: String,
    pub path: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub status: String,
    pub speed_bps: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtDlpStatus {
    pub installed: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VideosDetectedPayload {
    tab_id: u32,
    count: usize,
    videos: Vec<DetectedVideo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IslandEventPayload {
    kind: String,
    title: String,
    detail: String,
}

struct DownloadControl {
    paused: AtomicBool,
    cancelled: AtomicBool,
}

impl DownloadControl {
    fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        }
    }
}

struct DownloadEntry {
    item: DownloadItem,
    referer: Option<String>,
    format: Option<String>,
    use_yt_dlp: bool,
    control: Arc<DownloadControl>,
}

struct Inner {
    detected_by_tab: Mutex<HashMap<u32, Vec<DetectedVideo>>>,
    downloads: Mutex<HashMap<String, DownloadEntry>>,
    download_order: Mutex<Vec<String>>,
    next_dl_id: AtomicU64,
    concurrency: Arc<Semaphore>,
    sniffer_enabled: AtomicBool,
    client: reqwest::Client,
}

#[derive(Clone)]
pub struct VideoDownloadManager {
    inner: Arc<Inner>,
}

impl Default for VideoDownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoDownloadManager {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(BROWSER_UA)
            .connect_timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            inner: Arc::new(Inner {
                detected_by_tab: Mutex::new(HashMap::new()),
                downloads: Mutex::new(HashMap::new()),
                download_order: Mutex::new(Vec::new()),
                next_dl_id: AtomicU64::new(1),
                concurrency: Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS)),
                sniffer_enabled: AtomicBool::new(true),
                client,
            }),
        }
    }

    /// Returns true if any download is currently queued or actively downloading.
    pub fn has_active_downloads(&self) -> bool {
        self.active_download_count() > 0
    }

    /// Returns count of queued or active downloads.
    pub fn active_download_count(&self) -> usize {
        let map = self.inner.downloads.lock().unwrap();
        map.values()
            .filter(|e| e.item.status == "downloading" || e.item.status == "queued")
            .count()
    }

    /// Clears detected videos for a closed tab.
    pub fn clear_tab(&self, tab_id: u32) {
        self.inner.detected_by_tab.lock().unwrap().remove(&tab_id);
    }

    /// Default download directory: `%USERPROFILE%\Downloads\RustBrowse`.
    pub fn default_download_dir() -> PathBuf {
        let base = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Downloads").join("RustBrowse")
    }

    /// Official yt-dlp helper binary path: `%LOCALAPPDATA%\RustBrowse\bin\yt-dlp.exe`.
    pub fn yt_dlp_path() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("RustBrowse").join("bin").join("yt-dlp.exe")
    }

    fn update_item(&self, app: &AppHandle, id: &str, f: impl FnOnce(&mut DownloadItem)) {
        let snapshot = {
            let mut map = self.inner.downloads.lock().unwrap();
            if let Some(entry) = map.get_mut(id) {
                f(&mut entry.item);
                Some(entry.item.clone())
            } else {
                None
            }
        };
        if let Some(item) = snapshot {
            let _ = app.emit("download-progress", item);
        }
    }

    fn emit_island(app: &AppHandle, kind: &str, title: &str, detail: &str) {
        let _ = app.emit(
            "island-event",
            IslandEventPayload {
                kind: kind.to_string(),
                title: title.to_string(),
                detail: detail.to_string(),
            },
        );
    }

    fn spawn_download_task(&self, app: AppHandle, id: String) {
        let mgr = self.clone();
        tauri::async_runtime::spawn(async move {
            let permit = match mgr.inner.concurrency.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };

            let (url, title, path, referer, format, use_yt_dlp, control) = {
                let mut map = mgr.inner.downloads.lock().unwrap();
                let Some(entry) = map.get_mut(&id) else {
                    return;
                };
                if entry.control.cancelled.load(Ordering::SeqCst) {
                    entry.item.status = "cancelled".to_string();
                    return;
                }
                if entry.control.paused.load(Ordering::SeqCst) {
                    entry.item.status = "paused".to_string();
                    return;
                }
                entry.item.status = "downloading".to_string();
                entry.item.error = None;
                let snap = entry.item.clone();
                let _ = app.emit("download-progress", snap);
                (
                    entry.item.url.clone(),
                    entry.item.title.clone(),
                    PathBuf::from(&entry.item.path),
                    entry.referer.clone(),
                    entry.format.clone(),
                    entry.use_yt_dlp,
                    entry.control.clone(),
                )
            };

            let result = if use_yt_dlp || is_complex_yt_dlp_url(&url) {
                mgr.run_yt_dlp_download(&app, &id, &url, &path, control.clone())
                    .await
            } else if is_hls_url(&url, format.as_deref()) {
                mgr.run_hls_download(&app, &id, &url, &path, referer.as_deref(), control.clone())
                    .await
            } else {
                mgr.run_direct_download(&app, &id, &url, &path, referer.as_deref(), control.clone())
                    .await
            };

            drop(permit);

            match result {
                Ok(DownloadOutcome::Completed) => {
                    mgr.update_item(&app, &id, |item| {
                        item.status = "completed".to_string();
                        item.speed_bps = 0;
                        if let Some(total) = item.total_bytes {
                            if item.downloaded_bytes < total {
                                item.downloaded_bytes = total;
                            }
                        }
                    });
                    Self::emit_island(&app, "download-complete", "Tải video hoàn tất", &title);
                }
                Ok(DownloadOutcome::Paused) => {
                    mgr.update_item(&app, &id, |item| {
                        item.status = "paused".to_string();
                        item.speed_bps = 0;
                    });
                }
                Ok(DownloadOutcome::Cancelled) => {
                    let _ = tokio::fs::remove_file(&path).await;
                    mgr.update_item(&app, &id, |item| {
                        item.status = "cancelled".to_string();
                        item.speed_bps = 0;
                    });
                }
                Err(err) => {
                    warn!(download_id = %id, error = %err, "Video download failed");
                    mgr.update_item(&app, &id, |item| {
                        item.status = "failed".to_string();
                        item.speed_bps = 0;
                        item.error = Some(err.clone());
                    });
                    Self::emit_island(&app, "download-error", "Lỗi tải video", &err);
                }
            }
        });
    }

    async fn run_direct_download(
        &self,
        app: &AppHandle,
        id: &str,
        url: &str,
        dest_path: &Path,
        referer: Option<&str>,
        control: Arc<DownloadControl>,
    ) -> Result<DownloadOutcome, String> {
        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create download directory: {e}"))?;
        }

        let existing_len = tokio::fs::metadata(dest_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let mut headers = build_media_headers(referer);
        if existing_len > 0 {
            if let Ok(val) = HeaderValue::from_str(&format!("bytes={existing_len}-")) {
                headers.insert(RANGE, val);
            }
        }

        let resp = self
            .inner
            .client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| format!("Network error: {e}"))?;

        if !resp.status().is_success() && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(format!("HTTP {}", resp.status()));
        }

        let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT && existing_len > 0;
        let content_len = resp.content_length();
        let total_bytes = content_len.map(|len| if resumed { len + existing_len } else { len });
        let mut downloaded = if resumed { existing_len } else { 0 };

        self.update_item(app, id, |item| {
            item.downloaded_bytes = downloaded;
            item.total_bytes = total_bytes;
        });

        let mut file = if resumed {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(dest_path)
                .await
                .map_err(|e| format!("File open error: {e}"))?
        } else {
            tokio::fs::File::create(dest_path)
                .await
                .map_err(|e| format!("File create error: {e}"))?
        };

        let mut stream = resp.bytes_stream();
        let mut last_tick = Instant::now();
        let mut bytes_since_tick: u64 = 0;

        while let Some(chunk_res) = stream.next().await {
            if control.cancelled.load(Ordering::SeqCst) {
                drop(file);
                return Ok(DownloadOutcome::Cancelled);
            }
            if control.paused.load(Ordering::SeqCst) {
                let _ = file.flush().await;
                return Ok(DownloadOutcome::Paused);
            }

            let chunk = chunk_res.map_err(|e| format!("Stream read error: {e}"))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Disk write error: {e}"))?;

            let n = chunk.len() as u64;
            downloaded += n;
            bytes_since_tick += n;

            let elapsed = last_tick.elapsed();
            if elapsed >= Duration::from_millis(250) {
                let speed_bps = (bytes_since_tick as f64 / elapsed.as_secs_f64()) as u64;
                last_tick = Instant::now();
                bytes_since_tick = 0;
                self.update_item(app, id, |item| {
                    item.downloaded_bytes = downloaded;
                    item.speed_bps = speed_bps;
                });
            }
        }

        file.flush()
            .await
            .map_err(|e| format!("Disk flush error: {e}"))?;

        self.update_item(app, id, |item| {
            item.downloaded_bytes = downloaded;
            if item.total_bytes.is_none() {
                item.total_bytes = Some(downloaded);
            }
        });

        Ok(DownloadOutcome::Completed)
    }

    async fn run_hls_download(
        &self,
        app: &AppHandle,
        id: &str,
        playlist_url: &str,
        dest_path: &Path,
        referer: Option<&str>,
        control: Arc<DownloadControl>,
    ) -> Result<DownloadOutcome, String> {
        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create download directory: {e}"))?;
        }

        let headers = build_media_headers(referer);
        let first_text = self
            .inner
            .client
            .get(playlist_url)
            .headers(headers.clone())
            .send()
            .await
            .map_err(|e| format!("Failed to fetch HLS playlist: {e}"))?
            .error_for_status()
            .map_err(|e| format!("HLS playlist HTTP error: {e}"))?
            .text()
            .await
            .map_err(|e| format!("Failed to read HLS playlist: {e}"))?;

        // Resolve master playlist -> highest bandwidth media playlist if needed
        let (media_url, media_text) =
            if let Some(variant_url) = parse_hls_master_best_variant(&first_text, playlist_url) {
                let text = self
                    .inner
                    .client
                    .get(&variant_url)
                    .headers(headers.clone())
                    .send()
                    .await
                    .map_err(|e| format!("Failed to fetch HLS variant playlist: {e}"))?
                    .error_for_status()
                    .map_err(|e| format!("HLS variant HTTP error: {e}"))?
                    .text()
                    .await
                    .map_err(|e| format!("Failed to read HLS variant: {e}"))?;
                (variant_url, text)
            } else {
                (playlist_url.to_string(), first_text)
            };

        if media_text.contains("#EXT-X-KEY:") && !media_text.contains("METHOD=NONE") {
            return Err(
                "HLS stream is encrypted (DRM/key-protected) and is not supported.".to_string(),
            );
        }

        let segments = parse_hls_segments(&media_text, &media_url);
        if segments.is_empty() {
            return Err("No downloadable media segments found in HLS playlist.".to_string());
        }

        // Prepare temp directory for concurrent segment downloads
        let temp_dir = dest_path.with_extension("hls_parts");
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| format!("Failed to create HLS temp dir: {e}"))?;

        let total_segments = segments.len();
        let seg_sem = Arc::new(Semaphore::new(HLS_SEGMENT_CONCURRENCY));
        let downloaded_bytes = Arc::new(AtomicU64::new(0));
        let completed_segments = Arc::new(AtomicU64::new(0));
        let start_time = Instant::now();

        let mut handles = Vec::with_capacity(total_segments);
        for (idx, seg_url) in segments.into_iter().enumerate() {
            let sem = seg_sem.clone();
            let client = self.inner.client.clone();
            let hdrs = headers.clone();
            let ctrl = control.clone();
            let part_path = temp_dir.join(format!("seg_{idx:06}.ts"));
            let dl_bytes = downloaded_bytes.clone();
            let comp_segs = completed_segments.clone();
            let mgr = self.clone();
            let app_c = app.clone();
            let id_c = id.to_string();

            handles.push(tokio::spawn(async move {
                let _permit = sem
                    .acquire_owned()
                    .await
                    .map_err(|_| "Semaphore closed".to_string())?;
                if ctrl.cancelled.load(Ordering::SeqCst) || ctrl.paused.load(Ordering::SeqCst) {
                    return Ok::<(), String>(());
                }

                // Skip already-downloaded segment on resume
                if let Ok(meta) = tokio::fs::metadata(&part_path).await {
                    if meta.len() > 0 {
                        dl_bytes.fetch_add(meta.len(), Ordering::Relaxed);
                        comp_segs.fetch_add(1, Ordering::Relaxed);
                        return Ok(());
                    }
                }

                let bytes = client
                    .get(&seg_url)
                    .headers(hdrs)
                    .send()
                    .await
                    .map_err(|e| format!("Segment {idx} network error: {e}"))?
                    .error_for_status()
                    .map_err(|e| format!("Segment {idx} HTTP error: {e}"))?
                    .bytes()
                    .await
                    .map_err(|e| format!("Segment {idx} read error: {e}"))?;

                tokio::fs::write(&part_path, &bytes)
                    .await
                    .map_err(|e| format!("Segment {idx} write error: {e}"))?;

                let total_dl =
                    dl_bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed) + bytes.len() as u64;
                let done_count = comp_segs.fetch_add(1, Ordering::Relaxed) + 1;
                let elapsed = start_time.elapsed().as_secs_f64().max(0.1);
                let speed = (total_dl as f64 / elapsed) as u64;
                let est_total = total_dl
                    .checked_div(done_count)
                    .map(|avg| avg * total_segments as u64);

                mgr.update_item(&app_c, &id_c, |item| {
                    item.downloaded_bytes = total_dl;
                    item.total_bytes = est_total;
                    item.speed_bps = speed;
                });

                Ok(())
            }));
        }

        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                    return Err(e);
                }
                Err(e) => {
                    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                    return Err(format!("HLS worker join error: {e}"));
                }
            }
        }

        if control.cancelled.load(Ordering::SeqCst) {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return Ok(DownloadOutcome::Cancelled);
        }
        if control.paused.load(Ordering::SeqCst) {
            return Ok(DownloadOutcome::Paused);
        }

        // Join segments in exact order into dest_path
        let mut out_file = tokio::fs::File::create(dest_path)
            .await
            .map_err(|e| format!("Failed to create output HLS file: {e}"))?;
        let mut total_written: u64 = 0;

        for idx in 0..total_segments {
            let part_path = temp_dir.join(format!("seg_{idx:06}.ts"));
            let data = tokio::fs::read(&part_path)
                .await
                .map_err(|e| format!("Missing segment {idx} during join: {e}"))?;
            out_file
                .write_all(&data)
                .await
                .map_err(|e| format!("Write error joining segment {idx}: {e}"))?;
            total_written += data.len() as u64;
        }

        out_file
            .flush()
            .await
            .map_err(|e| format!("Flush error: {e}"))?;
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;

        self.update_item(app, id, |item| {
            item.downloaded_bytes = total_written;
            item.total_bytes = Some(total_written);
        });

        Ok(DownloadOutcome::Completed)
    }

    async fn run_yt_dlp_download(
        &self,
        app: &AppHandle,
        id: &str,
        url: &str,
        dest_path: &Path,
        control: Arc<DownloadControl>,
    ) -> Result<DownloadOutcome, String> {
        let bin = Self::yt_dlp_path();
        if !bin.exists() {
            return Err("yt-dlp chưa được cài đặt. Nhấn 'Cài đặt yt-dlp (1-click)' để tải từ GitHub chính thức.".to_string());
        }

        let download_dir = dest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(Self::default_download_dir);
        tokio::fs::create_dir_all(&download_dir)
            .await
            .map_err(|e| format!("Failed to create download directory: {e}"))?;

        let output_template = download_dir
            .join("%(title).120B.%(ext)s")
            .to_string_lossy()
            .to_string();

        let mut cmd = tokio::process::Command::new(&bin);
        cmd.args([
            "--newline",
            "--no-playlist",
            "--progress",
            "--no-warnings",
            "-o",
            &output_template,
            "--",
            url,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to launch yt-dlp: {e}"))?;

        let stdout = child.stdout.take();
        let mut reader = stdout.map(|s| BufReader::new(s).lines());

        let mut final_dest: Option<String> = None;

        if let Some(ref mut lines) = reader {
            while let Ok(Some(line)) = lines.next_line().await {
                if control.cancelled.load(Ordering::SeqCst) {
                    let _ = child.kill().await;
                    return Ok(DownloadOutcome::Cancelled);
                }
                if control.paused.load(Ordering::SeqCst) {
                    let _ = child.kill().await;
                    return Ok(DownloadOutcome::Paused);
                }

                if let Some(dest) = line.strip_prefix("[download] Destination: ") {
                    let cleaned = dest.trim().to_string();
                    final_dest = Some(cleaned.clone());
                    self.update_item(app, id, |item| {
                        item.path = cleaned;
                    });
                } else if line.contains("[download]") && line.contains('%') {
                    if let Some((pct, total_est)) = parse_yt_dlp_progress(&line) {
                        self.update_item(app, id, |item| {
                            if let Some(t) = total_est {
                                item.total_bytes = Some(t);
                                item.downloaded_bytes = ((t as f64) * (pct / 100.0)) as u64;
                            }
                        });
                    }
                }
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| format!("yt-dlp wait error: {e}"))?;

        if control.cancelled.load(Ordering::SeqCst) {
            return Ok(DownloadOutcome::Cancelled);
        }
        if control.paused.load(Ordering::SeqCst) {
            return Ok(DownloadOutcome::Paused);
        }

        if status.success() {
            if let Some(p) = final_dest {
                if let Ok(meta) = tokio::fs::metadata(&p).await {
                    let len = meta.len();
                    self.update_item(app, id, |item| {
                        item.path = p;
                        item.downloaded_bytes = len;
                        item.total_bytes = Some(len);
                    });
                }
            }
            Ok(DownloadOutcome::Completed)
        } else {
            Err(format!("yt-dlp exited with status {}", status))
        }
    }
}

enum DownloadOutcome {
    Completed,
    Paused,
    Cancelled,
}

fn build_media_headers(referer: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(BROWSER_UA));
    if let Some(r) = referer {
        if !r.trim().is_empty() {
            if let Ok(val) = HeaderValue::from_str(r.trim()) {
                headers.insert(REFERER, val);
            }
        }
    }
    headers
}

pub fn is_complex_yt_dlp_url(raw_url: &str) -> bool {
    let Ok(parsed) = Url::parse(raw_url) else {
        return false;
    };
    let lower_path = parsed.path().to_lowercase();
    if lower_path.ends_with(".mp4")
        || lower_path.ends_with(".webm")
        || lower_path.ends_with(".m3u8")
        || lower_path.ends_with(".mkv")
    {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let h = host.to_lowercase();
    const HOSTS: &[&str] = &[
        "youtube.com",
        "youtu.be",
        "facebook.com",
        "fb.watch",
        "tiktok.com",
        "vimeo.com",
        "twitch.tv",
        "twitter.com",
        "x.com",
        "instagram.com",
        "bilibili.com",
    ];
    HOSTS
        .iter()
        .any(|d| h == *d || h.ends_with(&format!(".{d}")))
}

fn is_hls_url(url: &str, format: Option<&str>) -> bool {
    if matches!(format, Some("hls")) {
        return true;
    }
    url.to_lowercase().contains(".m3u8")
}

pub fn sanitize_filename(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(100));
    for ch in raw.chars() {
        let forbidden =
            matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control();
        if forbidden {
            out.push('_');
        } else {
            out.push(ch);
        }
        if out.len() >= 100 {
            break;
        }
    }
    let trimmed = out.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "rustbrowse_video".to_string()
    } else {
        trimmed
    }
}

pub fn infer_extension(url: &str, format: Option<&str>) -> &'static str {
    if let Some(f) = format {
        match f.to_lowercase().as_str() {
            "hls" => return "ts",
            "webm" => return "webm",
            "mkv" => return "mkv",
            "mov" => return "mov",
            "mp4" => return "mp4",
            _ => {}
        }
    }
    let lower = url.to_lowercase();
    if lower.contains(".m3u8") {
        "ts"
    } else if lower.contains(".webm") {
        "webm"
    } else if lower.contains(".mkv") {
        "mkv"
    } else if lower.contains(".mov") {
        "mov"
    } else {
        "mp4"
    }
}

pub fn resolve_unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let clean_stem = sanitize_filename(stem);
    let first = dir.join(format!("{clean_stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    for i in 1..10_000 {
        let candidate = dir.join(format!("{clean_stem} ({i}).{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!(
        "{clean_stem}_{}.{ext}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ))
}

/// Parses an HLS master playlist and returns the resolved URL of the highest-bandwidth variant.
pub fn parse_hls_master_best_variant(playlist: &str, base_url: &str) -> Option<String> {
    if !playlist.contains("#EXT-X-STREAM-INF") {
        return None;
    }
    let base = Url::parse(base_url).ok()?;
    let mut best_bw: u64 = 0;
    let mut best_url: Option<String> = None;
    let mut pending_bw: Option<u64> = None;

    for raw_line in playlist.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let bw = rest
                .split(',')
                .find_map(|part| {
                    let kv = part.trim();
                    kv.strip_prefix("BANDWIDTH=")
                        .and_then(|v| v.parse::<u64>().ok())
                })
                .unwrap_or(1);
            pending_bw = Some(bw);
        } else if !line.starts_with('#') {
            if let Some(bw) = pending_bw.take() {
                if let Ok(resolved) = base.join(line) {
                    if bw >= best_bw || best_url.is_none() {
                        best_bw = bw;
                        best_url = Some(resolved.to_string());
                    }
                }
            }
        }
    }
    best_url
}

/// Parses an HLS media playlist and returns resolved segment URLs in order.
pub fn parse_hls_segments(playlist: &str, base_url: &str) -> Vec<String> {
    let Ok(base) = Url::parse(base_url) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for raw_line in playlist.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Ok(resolved) = base.join(line) {
            out.push(resolved.to_string());
        }
    }
    out
}

fn parse_yt_dlp_progress(line: &str) -> Option<(f64, Option<u64>)> {
    // Example: "[download]  42.3% of ~12.45MiB at  2.10MiB/s ETA 00:03"
    let pct_idx = line.find('%')?;
    let before = line[..pct_idx].trim();
    let pct_str = before.split_whitespace().last()?;
    let pct: f64 = pct_str.parse().ok()?;

    let total_bytes = line.find(" of ").and_then(|idx| {
        let rest = line[idx + 4..].trim().trim_start_matches('~');
        let token = rest.split_whitespace().next()?;
        parse_human_size(token)
    });

    Some((pct.clamp(0.0, 100.0), total_bytes))
}

fn parse_human_size(s: &str) -> Option<u64> {
    let (num_str, mult) = if let Some(v) = s.strip_suffix("GiB") {
        (v, 1024.0 * 1024.0 * 1024.0)
    } else if let Some(v) = s.strip_suffix("MiB") {
        (v, 1024.0 * 1024.0)
    } else if let Some(v) = s.strip_suffix("KiB") {
        (v, 1024.0)
    } else if let Some(v) = s.strip_suffix("GB") {
        (v, 1e9)
    } else if let Some(v) = s.strip_suffix("MB") {
        (v, 1e6)
    } else if let Some(v) = s.strip_suffix("KB") {
        (v, 1e3)
    } else {
        let v = s.strip_suffix('B')?;
        (v, 1.0)
    };
    let n: f64 = num_str.trim().parse().ok()?;
    Some((n * mult) as u64)
}

// ============================================================================
// Tauri v2 IPC Commands
// ============================================================================

#[tauri::command]
pub fn report_detected_videos(
    app: AppHandle,
    webview: Webview,
    mgr: State<'_, VideoDownloadManager>,
    tab_id: Option<u32>,
    mut videos: Vec<DetectedVideo>,
) -> Result<usize, String> {
    let resolved_tab_id = tab_id.unwrap_or_else(|| {
        webview
            .label()
            .strip_prefix("tab-")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    });

    for v in &mut videos {
        v.tab_id = resolved_tab_id;
    }

    let downloadable_count = videos.iter().filter(|v| !v.is_drm).count();
    let prev_count = {
        let mut map = mgr.inner.detected_by_tab.lock().unwrap();
        let prev = map.get(&resolved_tab_id).map(|v| v.len()).unwrap_or(0);
        map.insert(resolved_tab_id, videos.clone());
        prev
    };

    let count = videos.len();
    let _ = app.emit(
        "videos-detected",
        VideosDetectedPayload {
            tab_id: resolved_tab_id,
            count,
            videos,
        },
    );

    if downloadable_count > 0 && prev_count == 0 {
        VideoDownloadManager::emit_island(
            &app,
            "video-detected",
            "Phát hiện video",
            &format!("{downloadable_count} video sẵn sàng tải"),
        );
    }

    Ok(count)
}

#[tauri::command]
pub fn get_detected_videos(
    mgr: State<'_, VideoDownloadManager>,
    tab_id: Option<u32>,
) -> Result<Vec<DetectedVideo>, String> {
    let map = mgr.inner.detected_by_tab.lock().unwrap();
    if let Some(id) = tab_id {
        Ok(map.get(&id).cloned().unwrap_or_default())
    } else {
        let mut all = Vec::new();
        for list in map.values() {
            all.extend(list.iter().cloned());
        }
        Ok(all)
    }
}

#[tauri::command]
pub fn clear_tab_videos(mgr: State<'_, VideoDownloadManager>, tab_id: u32) -> Result<(), String> {
    mgr.clear_tab(tab_id);
    Ok(())
}

#[tauri::command]
pub fn start_video_download(
    app: AppHandle,
    mgr: State<'_, VideoDownloadManager>,
    url: String,
    title: Option<String>,
    referer: Option<String>,
    format: Option<String>,
    use_yt_dlp: Option<bool>,
) -> Result<DownloadItem, String> {
    let trimmed_url = url.trim().to_string();
    if trimmed_url.is_empty() {
        return Err("URL video trống.".to_string());
    }
    if trimmed_url.starts_with("blob:") && !use_yt_dlp.unwrap_or(false) {
        return Err(
            "URL dạng blob/MSE không thể tải trực tiếp; hãy thử tải qua yt-dlp hoặc nguồn HLS."
                .to_string(),
        );
    }

    let dl_id = format!("dl-{}", mgr.inner.next_dl_id.fetch_add(1, Ordering::SeqCst));
    let clean_title = title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("RustBrowse Video")
        .to_string();

    let dir = VideoDownloadManager::default_download_dir();
    let _ = std::fs::create_dir_all(&dir);
    let ext = infer_extension(&trimmed_url, format.as_deref());
    let dest = resolve_unique_path(&dir, &clean_title, ext);

    let item = DownloadItem {
        id: dl_id.clone(),
        title: clean_title,
        url: trimmed_url,
        path: dest.to_string_lossy().to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        status: "queued".to_string(),
        speed_bps: 0,
        error: None,
    };

    let entry = DownloadEntry {
        item: item.clone(),
        referer,
        format,
        use_yt_dlp: use_yt_dlp.unwrap_or(false),
        control: Arc::new(DownloadControl::new()),
    };

    {
        let mut map = mgr.inner.downloads.lock().unwrap();
        map.insert(dl_id.clone(), entry);
        mgr.inner.download_order.lock().unwrap().push(dl_id.clone());
    }

    let _ = app.emit("download-progress", item.clone());
    mgr.spawn_download_task(app, dl_id);

    Ok(item)
}

#[tauri::command]
pub fn list_downloads(mgr: State<'_, VideoDownloadManager>) -> Result<Vec<DownloadItem>, String> {
    let map = mgr.inner.downloads.lock().unwrap();
    let order = mgr.inner.download_order.lock().unwrap();
    let mut items = Vec::with_capacity(order.len());
    for id in order.iter().rev() {
        if let Some(entry) = map.get(id) {
            items.push(entry.item.clone());
        }
    }
    Ok(items)
}

#[tauri::command]
pub fn pause_download(mgr: State<'_, VideoDownloadManager>, id: String) -> Result<(), String> {
    let map = mgr.inner.downloads.lock().unwrap();
    let entry = map
        .get(&id)
        .ok_or_else(|| format!("Download '{id}' not found"))?;
    entry.control.paused.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn resume_download(
    app: AppHandle,
    mgr: State<'_, VideoDownloadManager>,
    id: String,
) -> Result<(), String> {
    {
        let mut map = mgr.inner.downloads.lock().unwrap();
        let entry = map
            .get_mut(&id)
            .ok_or_else(|| format!("Download '{id}' not found"))?;
        if entry.item.status != "paused" && entry.item.status != "failed" {
            return Ok(());
        }
        entry.control.paused.store(false, Ordering::SeqCst);
        entry.control.cancelled.store(false, Ordering::SeqCst);
        entry.item.status = "queued".to_string();
        entry.item.error = None;
        let _ = app.emit("download-progress", entry.item.clone());
    }
    mgr.spawn_download_task(app, id);
    Ok(())
}

#[tauri::command]
pub fn cancel_download(mgr: State<'_, VideoDownloadManager>, id: String) -> Result<(), String> {
    let mut map = mgr.inner.downloads.lock().unwrap();
    let entry = map
        .get_mut(&id)
        .ok_or_else(|| format!("Download '{id}' not found"))?;
    entry.control.cancelled.store(true, Ordering::SeqCst);
    if entry.item.status == "paused" || entry.item.status == "queued" {
        entry.item.status = "cancelled".to_string();
        let _ = std::fs::remove_file(&entry.item.path);
    }
    Ok(())
}

#[tauri::command]
pub fn open_download_folder(
    mgr: State<'_, VideoDownloadManager>,
    id: Option<String>,
) -> Result<(), String> {
    let target_file = id.and_then(|dl_id| {
        mgr.inner
            .downloads
            .lock()
            .unwrap()
            .get(&dl_id)
            .map(|e| PathBuf::from(&e.item.path))
    });

    let dir = VideoDownloadManager::default_download_dir();
    let _ = std::fs::create_dir_all(&dir);

    #[cfg(target_os = "windows")]
    {
        if let Some(file_path) = target_file {
            if file_path.exists() {
                std::process::Command::new("explorer.exe")
                    .arg(format!("/select,{}", file_path.display()))
                    .spawn()
                    .map_err(|e| format!("Failed to open Explorer: {e}"))?;
                return Ok(());
            }
        }
        std::process::Command::new("explorer.exe")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to open Explorer: {e}"))?;
    }

    #[cfg(not(target_os = "windows"))]
    let _ = target_file;

    Ok(())
}

#[tauri::command]
pub fn get_yt_dlp_status() -> Result<YtDlpStatus, String> {
    let path = VideoDownloadManager::yt_dlp_path();
    Ok(YtDlpStatus {
        installed: path.exists(),
        path: path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn install_yt_dlp(app: AppHandle) -> Result<YtDlpStatus, String> {
    let target = VideoDownloadManager::yt_dlp_path();
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create bin directory: {e}"))?;
    }

    info!(dest = %target.display(), "Downloading official yt-dlp.exe from GitHub releases");
    let client = reqwest::Client::builder()
        .user_agent(BROWSER_UA)
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .get(YT_DLP_RELEASE_URL)
        .send()
        .await
        .map_err(|e| format!("Failed to download yt-dlp.exe: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GitHub release HTTP error: {e}"))?;

    let part_path = target.with_extension("exe.part");
    let mut file = tokio::fs::File::create(&part_path)
        .await
        .map_err(|e| format!("Failed to create temp yt-dlp file: {e}"))?;

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let data = chunk.map_err(|e| format!("Download stream error: {e}"))?;
        file.write_all(&data)
            .await
            .map_err(|e| format!("Write error: {e}"))?;
    }
    file.flush()
        .await
        .map_err(|e| format!("Flush error: {e}"))?;
    drop(file);

    if target.exists() {
        let _ = tokio::fs::remove_file(&target).await;
    }
    tokio::fs::rename(&part_path, &target)
        .await
        .map_err(|e| format!("Failed to finalize yt-dlp.exe: {e}"))?;

    VideoDownloadManager::emit_island(
        &app,
        "download-complete",
        "yt-dlp đã sẵn sàng",
        "Hỗ trợ tải YouTube, Facebook, TikTok",
    );

    Ok(YtDlpStatus {
        installed: true,
        path: target.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn set_sniffer_enabled(
    app: AppHandle,
    mgr: State<'_, VideoDownloadManager>,
    enabled: bool,
) -> Result<bool, String> {
    mgr.inner.sniffer_enabled.store(enabled, Ordering::SeqCst);
    let js = format!(
        "if (window.__RUSTBROWSE_SET_SNIFFER_ENABLED__) window.__RUSTBROWSE_SET_SNIFFER_ENABLED__({});",
        if enabled { "true" } else { "false" }
    );
    for (label, wv) in app.webviews() {
        if label.starts_with("tab-") {
            let _ = wv.eval(&js);
        }
    }
    Ok(enabled)
}

#[tauri::command]
pub fn get_sniffer_enabled(mgr: State<'_, VideoDownloadManager>) -> Result<bool, String> {
    Ok(mgr.inner.sniffer_enabled.load(Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(
            sanitize_filename("My:Video/Name?*<>|.mp4"),
            "My_Video_Name_____.mp4"
        );
        assert_eq!(sanitize_filename("   ...   "), "rustbrowse_video");
    }

    #[test]
    fn test_complex_yt_dlp_host_detection() {
        assert!(is_complex_yt_dlp_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(is_complex_yt_dlp_url("https://youtu.be/dQw4w9WgXcQ"));
        assert!(is_complex_yt_dlp_url(
            "https://www.tiktok.com/@user/video/12345"
        ));
        assert!(is_complex_yt_dlp_url(
            "https://www.facebook.com/watch/?v=123"
        ));
        assert!(!is_complex_yt_dlp_url("https://cdn.example.com/clip.mp4"));
        assert!(!is_complex_yt_dlp_url(
            "https://cdn.youtube.com/direct.m3u8"
        ));
    }

    #[test]
    fn test_parse_hls_master_and_segments() {
        let master = "#EXTM3U\n\
            #EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\n\
            low/index.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=3500000,RESOLUTION=1920x1080\n\
            high/index.m3u8\n";
        let best = parse_hls_master_best_variant(master, "https://cdn.example.com/vod/master.m3u8");
        assert_eq!(
            best.as_deref(),
            Some("https://cdn.example.com/vod/high/index.m3u8")
        );

        let media = "#EXTM3U\n#EXTINF:6.0,\nseg0.ts\n#EXTINF:6.0,\nseg1.ts\n#EXT-X-ENDLIST\n";
        let segs = parse_hls_segments(media, "https://cdn.example.com/vod/high/index.m3u8");
        assert_eq!(
            segs,
            vec![
                "https://cdn.example.com/vod/high/seg0.ts".to_string(),
                "https://cdn.example.com/vod/high/seg1.ts".to_string(),
            ]
        );
    }

    #[test]
    fn test_detected_video_serde_aliases() {
        let json = r#"{"id":"v1","tabId":2,"title":"Demo","url":"https://a.com/v.mp4","quality":"1080p","format":"mp4","isDrm":false,"referer":"https://a.com"}"#;
        let v: DetectedVideo = serde_json::from_str(json).unwrap();
        assert_eq!(v.tab_id, 2);
        assert!(!v.is_drm);
    }

    #[test]
    fn test_parse_yt_dlp_progress() {
        let line = "[download]  50.0% of ~10.00MiB at  2.50MiB/s ETA 00:02";
        let (pct, total) = parse_yt_dlp_progress(line).unwrap();
        assert!((pct - 50.0).abs() < 0.01);
        assert_eq!(total, Some(10 * 1024 * 1024));
    }
}
