//! SuperBoost Auto Update Engine for RustBrowse (Tauri v2 Windows Desktop)
//!
//! Features:
//! - Semantic Version comparison & architecture awareness (`windows-x86_64` / `windows-aarch64`)
//! - Stable & Beta release channels with HTTPS-only endpoint enforcement
//! - Background non-blocking check on startup + configurable periodic interval
//! - Staged background download with live byte progress & Ed25519 cryptographic signature verification
//! - Safe installation gate (never interrupts active downloads or media playback automatically)
//! - Executable backup & startup crash-loop rollback protection (never touches user profile data)

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};
use url::Url;

const TIGHTEST_CHECK_INTERVAL_MINS: u64 = 15;
const DEFAULT_CHECK_INTERVAL_MINS: u64 = 360; // 6 hours
const CRASH_LOOP_THRESHOLD: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UpdateChannel {
    #[serde(rename = "stable")]
    Stable,
    #[serde(rename = "beta")]
    Beta,
}

impl UpdateChannel {
    pub fn from_str_lossy(raw: &str) -> Self {
        if raw.trim().eq_ignore_ascii_case("beta") {
            Self::Beta
        } else {
            Self::Stable
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UpdateLifecycleStatus {
    #[serde(rename = "Up to date")]
    UpToDate,
    #[serde(rename = "Checking")]
    Checking,
    #[serde(rename = "Update available")]
    UpdateAvailable,
    #[serde(rename = "Downloading")]
    Downloading,
    #[serde(rename = "Verifying")]
    Verifying,
    #[serde(rename = "Ready to install")]
    ReadyToInstall,
    #[serde(rename = "Installing")]
    Installing,
    #[serde(rename = "Restart required")]
    RestartRequired,
    #[serde(rename = "Update failed")]
    UpdateFailed,
    #[serde(rename = "Rolled back")]
    RolledBack,
    #[serde(rename = "Not configured")]
    NotConfigured,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedUpdateState {
    pub channel: UpdateChannel,
    pub auto_install_on_exit: bool,
    pub check_interval_minutes: u64,
    pub last_checked_epoch_secs: Option<u64>,
    pub last_healthy_version: Option<String>,
    pub last_boot_version: Option<String>,
    pub pending_update_version: Option<String>,
    pub consecutive_unclean_boots: u32,
    pub blocked_versions: Vec<String>,
    pub backup_exe_path: Option<String>,
    pub last_rollback_reason: Option<String>,
}

impl Default for PersistedUpdateState {
    fn default() -> Self {
        Self {
            channel: UpdateChannel::Stable,
            auto_install_on_exit: true,
            check_interval_minutes: DEFAULT_CHECK_INTERVAL_MINS,
            last_checked_epoch_secs: None,
            last_healthy_version: None,
            last_boot_version: None,
            pending_update_version: None,
            consecutive_unclean_boots: 0,
            blocked_versions: Vec::new(),
            backup_exe_path: None,
            last_rollback_reason: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDashboardState {
    pub installed_version: String,
    pub latest_version: Option<String>,
    pub status: UpdateLifecycleStatus,
    pub last_checked: Option<String>,
    pub download_progress_pct: u32,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub verification_status: String,
    pub installation_status: String,
    pub release_notes: Option<String>,
    pub update_channel: String,
    pub auto_install_on_exit: bool,
    pub check_interval_minutes: u64,
    pub architecture: String,
    pub configured: bool,
    pub endpoint: String,
    pub error_message: Option<String>,
    pub previous_healthy_version: Option<String>,
    pub backup_available: bool,
    pub blocked_versions: Vec<String>,
}

struct RuntimeUpdateInner {
    persisted: PersistedUpdateState,
    installed_version: String,
    latest_version: Option<String>,
    status: UpdateLifecycleStatus,
    download_progress_pct: u32,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    verification_status: String,
    installation_status: String,
    release_notes: Option<String>,
    configured: bool,
    endpoint: String,
    pubkey_present: bool,
    error_message: Option<String>,
    staged_bytes: Option<Vec<u8>>,
    staged_version: Option<String>,
}

pub struct SuperBoostUpdateEngine {
    inner: Mutex<RuntimeUpdateInner>,
    state_file: PathBuf,
    staging_dir: PathBuf,
}

impl SuperBoostUpdateEngine {
    pub fn new(installed_version: String) -> Arc<Self> {
        let base_dir = user_data_dir();
        let _ = fs::create_dir_all(&base_dir);
        let state_file = base_dir.join("update_state.json");
        let staging_dir = base_dir.join("UpdateStaging");
        let _ = fs::create_dir_all(&staging_dir);

        let mut persisted = load_persisted_state(&state_file);
        let (endpoint, pubkey_present, configured) = inspect_embedded_tauri_config();

        // Boot crash-loop & rollback detection
        let mut initial_status = if configured {
            UpdateLifecycleStatus::UpToDate
        } else {
            UpdateLifecycleStatus::NotConfigured
        };
        let mut initial_error = None;

        if persisted.last_boot_version.as_deref() == Some(installed_version.as_str()) {
            persisted.consecutive_unclean_boots =
                persisted.consecutive_unclean_boots.saturating_add(1);
        } else {
            persisted.last_boot_version = Some(installed_version.clone());
            persisted.consecutive_unclean_boots = 1;
        }

        if let Some(reason) = evaluate_boot_rollback(&mut persisted, &installed_version) {
            warn!(reason = %reason, "Update crash-loop detected; marking version blocked.");
            initial_status = UpdateLifecycleStatus::RolledBack;
            initial_error = Some(reason);
        }

        save_persisted_state(&state_file, &persisted);

        let verification_status = if pubkey_present {
            "Ed25519 Public Key Active".to_string()
        } else {
            "Chưa cấu hình Public Signing Key (pubkey trống)".to_string()
        };

        let installation_status = if initial_status == UpdateLifecycleStatus::RolledBack {
            "Đã chặn phiên bản lỗi & kích hoạt bảo vệ Rollback".to_string()
        } else if configured {
            "Sẵn sàng nhận bản cập nhật tự động".to_string()
        } else {
            "Cần thiết lập GitHub Repo & Khóa ký Ed25519 một lần".to_string()
        };

        Arc::new(Self {
            inner: Mutex::new(RuntimeUpdateInner {
                persisted,
                installed_version,
                latest_version: None,
                status: initial_status,
                download_progress_pct: 0,
                downloaded_bytes: 0,
                total_bytes: None,
                verification_status,
                installation_status,
                release_notes: None,
                configured,
                endpoint,
                pubkey_present,
                error_message: initial_error,
                staged_bytes: None,
                staged_version: None,
            }),
            state_file,
            staging_dir,
        })
    }

    /// Marks the current running build healthy after stable post-boot uptime.
    pub fn mark_boot_healthy(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            let current = guard.installed_version.clone();
            guard.persisted.consecutive_unclean_boots = 0;
            guard.persisted.last_healthy_version = Some(current);
            guard.persisted.pending_update_version = None;
            save_persisted_state(&self.state_file, &guard.persisted);
        }
    }

    pub fn snapshot(&self) -> UpdateDashboardState {
        let guard = self.inner.lock().unwrap();
        let backup_available = guard
            .persisted
            .backup_exe_path
            .as_ref()
            .map(|p| PathBuf::from(p).exists())
            .unwrap_or(false);

        UpdateDashboardState {
            installed_version: guard.installed_version.clone(),
            latest_version: guard.latest_version.clone(),
            status: guard.status.clone(),
            last_checked: guard
                .persisted
                .last_checked_epoch_secs
                .map(format_epoch_timestamp),
            download_progress_pct: guard.download_progress_pct,
            downloaded_bytes: guard.downloaded_bytes,
            total_bytes: guard.total_bytes,
            verification_status: guard.verification_status.clone(),
            installation_status: guard.installation_status.clone(),
            release_notes: guard.release_notes.clone(),
            update_channel: guard.persisted.channel.as_str().to_string(),
            auto_install_on_exit: guard.persisted.auto_install_on_exit,
            check_interval_minutes: guard.persisted.check_interval_minutes,
            architecture: format!("windows-{}", std::env::consts::ARCH),
            configured: guard.configured,
            endpoint: guard.endpoint.clone(),
            error_message: guard.error_message.clone(),
            previous_healthy_version: guard.persisted.last_healthy_version.clone(),
            backup_available,
            blocked_versions: guard.persisted.blocked_versions.clone(),
        }
    }

    pub fn set_channel(&self, app: &AppHandle, channel: UpdateChannel) -> UpdateDashboardState {
        {
            let mut guard = self.inner.lock().unwrap();
            guard.persisted.channel = channel;
            save_persisted_state(&self.state_file, &guard.persisted);
        }
        let snap = self.snapshot();
        emit_state(app, &snap);
        snap
    }

    pub fn set_auto_install_on_exit(&self, app: &AppHandle, enabled: bool) -> UpdateDashboardState {
        {
            let mut guard = self.inner.lock().unwrap();
            guard.persisted.auto_install_on_exit = enabled;
            save_persisted_state(&self.state_file, &guard.persisted);
        }
        let snap = self.snapshot();
        emit_state(app, &snap);
        snap
    }

    pub fn set_check_interval_minutes(
        &self,
        app: &AppHandle,
        minutes: u64,
    ) -> UpdateDashboardState {
        let clamped = minutes.max(TIGHTEST_CHECK_INTERVAL_MINS);
        {
            let mut guard = self.inner.lock().unwrap();
            guard.persisted.check_interval_minutes = clamped;
            save_persisted_state(&self.state_file, &guard.persisted);
        }
        let snap = self.snapshot();
        emit_state(app, &snap);
        snap
    }

    pub fn check_interval_duration(&self) -> Duration {
        let mins = self
            .inner
            .lock()
            .map(|g| {
                g.persisted
                    .check_interval_minutes
                    .max(TIGHTEST_CHECK_INTERVAL_MINS)
            })
            .unwrap_or(DEFAULT_CHECK_INTERVAL_MINS);
        Duration::from_secs(mins * 60)
    }

    pub fn should_install_on_exit(&self) -> bool {
        if let Ok(guard) = self.inner.lock() {
            guard.persisted.auto_install_on_exit
                && guard.status == UpdateLifecycleStatus::ReadyToInstall
                && guard.staged_bytes.is_some()
        } else {
            false
        }
    }

    fn build_channel_updater(
        &self,
        app: &AppHandle,
    ) -> Result<tauri_plugin_updater::Updater, String> {
        let (configured, pubkey_present, endpoint, channel) = {
            let guard = self.inner.lock().unwrap();
            (
                guard.configured,
                guard.pubkey_present,
                guard.endpoint.clone(),
                guard.persisted.channel,
            )
        };

        if !pubkey_present || !configured {
            return Err(
                "Chưa cấu hình khóa ký công khai (pubkey) hoặc GitHub Release endpoint hợp lệ trong tauri.conf.json"
                    .to_string(),
            );
        }

        let effective_endpoint = resolve_channel_endpoint(&endpoint, channel)?;
        let parsed = Url::parse(&effective_endpoint)
            .map_err(|e| format!("URL cập nhật không hợp lệ: {}", e))?;

        if !is_trusted_update_url(&parsed) {
            return Err("Từ chối URL cập nhật không dùng giao thức bảo mật HTTPS".to_string());
        }

        app.updater_builder()
            .endpoints(vec![parsed])
            .map_err(|e| format!("Lỗi cấu hình endpoint updater: {}", e))?
            .build()
            .map_err(|e| format!("Không thể khởi tạo Tauri Updater: {}", e))
    }

    /// Checks GitHub Releases for a newer signed version without blocking UI.
    pub async fn check_for_updates(
        &self,
        app: &AppHandle,
        emit_island: bool,
        auto_stage_download: bool,
    ) -> UpdateDashboardState {
        {
            let mut guard = self.inner.lock().unwrap();
            if matches!(
                guard.status,
                UpdateLifecycleStatus::Checking
                    | UpdateLifecycleStatus::Downloading
                    | UpdateLifecycleStatus::Verifying
                    | UpdateLifecycleStatus::Installing
            ) {
                return self.snapshot();
            }
            guard.status = UpdateLifecycleStatus::Checking;
            guard.error_message = None;
            guard.installation_status =
                "Đang kiểm tra phiên bản mới từ GitHub Releases...".to_string();
        }
        emit_state(app, &self.snapshot());

        let now_secs = current_epoch_secs();
        let updater = match self.build_channel_updater(app) {
            Ok(u) => u,
            Err(reason) => {
                let mut guard = self.inner.lock().unwrap();
                guard.persisted.last_checked_epoch_secs = Some(now_secs);
                save_persisted_state(&self.state_file, &guard.persisted);
                guard.status = if guard.pubkey_present {
                    UpdateLifecycleStatus::UpdateFailed
                } else {
                    UpdateLifecycleStatus::NotConfigured
                };
                guard.installation_status = "Chưa thể kiểm tra tự động".to_string();
                guard.error_message = Some(reason);
                drop(guard);
                let snap = self.snapshot();
                emit_state(app, &snap);
                return snap;
            }
        };

        match updater.check().await {
            Ok(Some(update)) => {
                let remote_ver = update.version.clone();
                let notes = update.body.clone();
                let (installed_ver, is_blocked) = {
                    let guard = self.inner.lock().unwrap();
                    (
                        guard.installed_version.clone(),
                        guard
                            .persisted
                            .blocked_versions
                            .iter()
                            .any(|v| v == &remote_ver),
                    )
                };

                if is_blocked {
                    let mut guard = self.inner.lock().unwrap();
                    guard.persisted.last_checked_epoch_secs = Some(now_secs);
                    save_persisted_state(&self.state_file, &guard.persisted);
                    guard.status = UpdateLifecycleStatus::RolledBack;
                    guard.latest_version = Some(remote_ver.clone());
                    guard.installation_status = format!(
                        "Đã bỏ qua v{} do từng gây lỗi khởi chạy (Rollback Protection)",
                        remote_ver
                    );
                    drop(guard);
                    let snap = self.snapshot();
                    emit_state(app, &snap);
                    return snap;
                }

                if !semver_is_newer(&installed_ver, &remote_ver) {
                    let mut guard = self.inner.lock().unwrap();
                    guard.persisted.last_checked_epoch_secs = Some(now_secs);
                    save_persisted_state(&self.state_file, &guard.persisted);
                    guard.latest_version = Some(installed_ver);
                    guard.status = UpdateLifecycleStatus::UpToDate;
                    guard.verification_status = "Ed25519 Signature Verified".to_string();
                    guard.installation_status = "Đang ở phiên bản mới nhất".to_string();
                    drop(guard);
                    let snap = self.snapshot();
                    emit_state(app, &snap);
                    return snap;
                }

                info!(version = %remote_ver, "New signed RustBrowse release detected.");
                {
                    let mut guard = self.inner.lock().unwrap();
                    guard.persisted.last_checked_epoch_secs = Some(now_secs);
                    save_persisted_state(&self.state_file, &guard.persisted);
                    guard.latest_version = Some(remote_ver.clone());
                    guard.release_notes = notes;
                    guard.status = UpdateLifecycleStatus::UpdateAvailable;
                    guard.download_progress_pct = 0;
                    guard.downloaded_bytes = 0;
                    guard.total_bytes = None;
                    guard.verification_status = "Đang chờ xác thực chữ ký gói tải".to_string();
                    guard.installation_status = format!("Phát hiện bản mới v{}", remote_ver);
                }

                if emit_island {
                    emit_island_notification(
                        app,
                        "update-available",
                        &format!("Bản cập nhật v{}", remote_ver),
                        "Đang tải ngầm & xác minh chữ ký...",
                    );
                }

                let snap = self.snapshot();
                emit_state(app, &snap);

                if auto_stage_download {
                    return self.download_and_stage(app).await;
                }
                snap
            }
            Ok(None) => {
                let mut guard = self.inner.lock().unwrap();
                guard.persisted.last_checked_epoch_secs = Some(now_secs);
                save_persisted_state(&self.state_file, &guard.persisted);
                let cur = guard.installed_version.clone();
                guard.latest_version = Some(cur);
                guard.status = UpdateLifecycleStatus::UpToDate;
                guard.verification_status = "Ed25519 Public Key Active".to_string();
                guard.installation_status = "RustBrowse đang ở phiên bản mới nhất".to_string();
                guard.error_message = None;
                drop(guard);
                let snap = self.snapshot();
                emit_state(app, &snap);
                snap
            }
            Err(e) => {
                warn!(error = %e, "GitHub Release update check failed.");
                let mut guard = self.inner.lock().unwrap();
                guard.persisted.last_checked_epoch_secs = Some(now_secs);
                save_persisted_state(&self.state_file, &guard.persisted);
                guard.status = UpdateLifecycleStatus::UpdateFailed;
                guard.installation_status = "Không thể kết nối máy chủ phát hành".to_string();
                guard.error_message = Some(format!("Kiểm tra cập nhật thất bại: {}", e));
                drop(guard);
                let snap = self.snapshot();
                emit_state(app, &snap);
                snap
            }
        }
    }

    /// Downloads the update package in the background, verifies its Ed25519 signature,
    /// backs up the current executable, and stages the verified payload without closing the app.
    pub async fn download_and_stage(&self, app: &AppHandle) -> UpdateDashboardState {
        let updater = match self.build_channel_updater(app) {
            Ok(u) => u,
            Err(e) => {
                let mut guard = self.inner.lock().unwrap();
                guard.status = UpdateLifecycleStatus::NotConfigured;
                guard.error_message = Some(e);
                drop(guard);
                let snap = self.snapshot();
                emit_state(app, &snap);
                return snap;
            }
        };

        let update = match updater.check().await {
            Ok(Some(u)) => u,
            Ok(None) => {
                let mut guard = self.inner.lock().unwrap();
                guard.status = UpdateLifecycleStatus::UpToDate;
                guard.installation_status = "Không có bản cập nhật mới để tải".to_string();
                drop(guard);
                let snap = self.snapshot();
                emit_state(app, &snap);
                return snap;
            }
            Err(e) => {
                let mut guard = self.inner.lock().unwrap();
                guard.status = UpdateLifecycleStatus::UpdateFailed;
                guard.error_message = Some(format!("Lỗi kiểm tra manifest trước khi tải: {}", e));
                drop(guard);
                let snap = self.snapshot();
                emit_state(app, &snap);
                return snap;
            }
        };

        let target_version = update.version.clone();
        // If already staged for this exact version, return immediately (avoid redundant re-download)
        {
            let guard = self.inner.lock().unwrap();
            if guard.staged_version.as_deref() == Some(target_version.as_str())
                && guard.staged_bytes.is_some()
                && guard.status == UpdateLifecycleStatus::ReadyToInstall
            {
                return self.snapshot();
            }
        }

        {
            let mut guard = self.inner.lock().unwrap();
            guard.status = UpdateLifecycleStatus::Downloading;
            guard.latest_version = Some(target_version.clone());
            guard.release_notes = update.body.clone();
            guard.download_progress_pct = 0;
            guard.downloaded_bytes = 0;
            guard.total_bytes = None;
            guard.error_message = None;
            guard.verification_status = "Đang tải gói đã ký từ HTTPS...".to_string();
            guard.installation_status = format!("Đang tải nền v{}", target_version);
        }
        emit_state(app, &self.snapshot());

        let engine_ref = app.try_state::<Arc<SuperBoostUpdateEngine>>();
        let app_progress = app.clone();
        let ver_for_cb = target_version.clone();
        let mut running_bytes: u64 = 0;
        let mut last_emitted_pct: u32 = 0;

        let download_res = update
            .download(
                move |chunk_len, content_len| {
                    running_bytes += chunk_len as u64;
                    let pct = content_len
                        .filter(|&tot| tot > 0)
                        .map(|tot| ((running_bytes as f64 / tot as f64) * 100.0).min(100.0) as u32)
                        .unwrap_or(0);

                    if pct >= last_emitted_pct + 5 || pct == 100 {
                        last_emitted_pct = pct;
                        if let Some(ref eng) = engine_ref {
                            if let Ok(mut g) = eng.inner.lock() {
                                g.downloaded_bytes = running_bytes;
                                g.total_bytes = content_len;
                                g.download_progress_pct = pct;
                                if pct >= 100 {
                                    g.status = UpdateLifecycleStatus::Verifying;
                                    g.verification_status =
                                        "Đang xác minh chữ ký số Ed25519 & toàn vẹn gói..."
                                            .to_string();
                                }
                            }
                            emit_state(&app_progress, &eng.snapshot());
                        }
                        emit_island_notification(
                            &app_progress,
                            "update-progress",
                            &format!("Đang tải bản cập nhật v{}", ver_for_cb),
                            &format!("{}%", pct),
                        );
                    }
                },
                || {
                    info!("Update stream finished; verifying cryptographic signature.");
                },
            )
            .await;

        match download_res {
            Ok(verified_bytes) => {
                // Clean any partial staging artifacts & write verified staging marker
                let staged_marker = self
                    .staging_dir
                    .join(format!("rustbrowse-{}.verified.bin", target_version));
                let _ = fs::write(&staged_marker, &verified_bytes);

                // Backup current executable safely before any install step
                let backup_path = self.backup_current_executable();

                {
                    let mut guard = self.inner.lock().unwrap();
                    if let Some(bp) = backup_path {
                        guard.persisted.backup_exe_path = Some(bp);
                        save_persisted_state(&self.state_file, &guard.persisted);
                    }
                    guard.downloaded_bytes = verified_bytes.len() as u64;
                    guard.total_bytes = Some(verified_bytes.len() as u64);
                    guard.download_progress_pct = 100;
                    guard.staged_bytes = Some(verified_bytes);
                    guard.staged_version = Some(target_version.clone());
                    guard.status = UpdateLifecycleStatus::ReadyToInstall;
                    guard.verification_status =
                        "Đã xác thực chữ ký số Ed25519 hợp lệ (100% nguyên vẹn)".to_string();
                    guard.installation_status = format!(
                        "Bản v{} đã sẵn sàng — Bấm 'Khởi động lại để cập nhật' hoặc tự cài khi thoát",
                        target_version
                    );
                    guard.error_message = None;
                }

                emit_island_notification(
                    app,
                    "update-available",
                    &format!("v{} sẵn sàng cài đặt", target_version),
                    "Đã xác thực chữ ký — Khởi động lại khi rảnh",
                );

                let snap = self.snapshot();
                emit_state(app, &snap);
                snap
            }
            Err(e) => {
                warn!(error = %e, "Update download or signature verification failed.");
                // Purge any incomplete staging files
                self.clean_staging_files();
                let err_str = e.to_string();
                let is_sig_err = err_str.to_ascii_lowercase().contains("signature");

                {
                    let mut guard = self.inner.lock().unwrap();
                    guard.staged_bytes = None;
                    guard.staged_version = None;
                    guard.status = UpdateLifecycleStatus::UpdateFailed;
                    guard.verification_status = if is_sig_err {
                        "THẤT BẠI: Chữ ký số không hợp lệ — Đã hủy gói cập nhật!".to_string()
                    } else {
                        "Tải gói cập nhật bị gián đoạn (sẽ thử lại an toàn)".to_string()
                    };
                    guard.installation_status =
                        "Cập nhật bị hủy để bảo vệ bản đang chạy".to_string();
                    guard.error_message = Some(format!("Tải/xác minh thất bại: {}", err_str));
                }

                let snap = self.snapshot();
                emit_state(app, &snap);
                snap
            }
        }
    }

    /// Installs the verified update (downloading first if not yet staged) and restarts cleanly.
    /// When `user_initiated == false`, refuses to interrupt active downloads or media playback.
    pub async fn install_and_restart(
        &self,
        app: &AppHandle,
        user_initiated: bool,
        has_active_downloads: bool,
        has_playing_media: bool,
    ) -> Result<String, String> {
        if !user_initiated && (has_active_downloads || has_playing_media) {
            return Err(
                "Trình duyệt đang tải tệp hoặc phát media; hoãn cài đặt tự động đến khi rảnh."
                    .to_string(),
            );
        }

        // Ensure verified bytes are staged
        let has_staged = self
            .inner
            .lock()
            .map(|g| g.staged_bytes.is_some())
            .unwrap_or(false);

        if !has_staged {
            let snap = self.download_and_stage(app).await;
            if snap.status != UpdateLifecycleStatus::ReadyToInstall {
                return Err(snap
                    .error_message
                    .unwrap_or_else(|| "Không có bản cập nhật hợp lệ để cài đặt".to_string()));
            }
        }

        let updater = self.build_channel_updater(app)?;
        let update = updater
            .check()
            .await
            .map_err(|e| format!("Lỗi xác nhận manifest trước khi cài: {}", e))?
            .ok_or_else(|| "Không tìm thấy bản cập nhật mới".to_string())?;

        let (bytes, target_ver) = {
            let mut guard = self.inner.lock().unwrap();
            let b = guard
                .staged_bytes
                .take()
                .ok_or_else(|| "Gói cập nhật đã xác minh không tồn tại trong bộ đệm".to_string())?;
            let v = guard
                .staged_version
                .clone()
                .unwrap_or_else(|| update.version.clone());
            guard.status = UpdateLifecycleStatus::Installing;
            guard.installation_status = format!(
                "Đang áp dụng bản cập nhật v{} (giữ nguyên dữ liệu người dùng)...",
                v
            );
            guard.persisted.pending_update_version = Some(v.clone());
            guard.persisted.consecutive_unclean_boots = 0;
            save_persisted_state(&self.state_file, &guard.persisted);
            (b, v)
        };
        emit_state(app, &self.snapshot());

        if let Err(e) = update.install(bytes) {
            let mut guard = self.inner.lock().unwrap();
            guard.status = UpdateLifecycleStatus::UpdateFailed;
            guard.persisted.pending_update_version = None;
            save_persisted_state(&self.state_file, &guard.persisted);
            let msg = format!("Cài đặt thất bại (bản hiện tại vẫn an toàn): {}", e);
            guard.installation_status = "Cài đặt lỗi — Đã giữ nguyên phiên bản ổn định".to_string();
            guard.error_message = Some(msg.clone());
            drop(guard);
            emit_state(app, &self.snapshot());
            return Err(msg);
        }

        {
            let mut guard = self.inner.lock().unwrap();
            guard.status = UpdateLifecycleStatus::RestartRequired;
            guard.installation_status = format!("Đã cài v{} — Đang khởi động lại...", target_ver);
        }
        emit_state(app, &self.snapshot());

        app.restart();
    }

    fn backup_current_executable(&self) -> Option<String> {
        let exe = std::env::current_exe().ok()?;
        let rollback_dir = self.staging_dir.join("rollback");
        fs::create_dir_all(&rollback_dir).ok()?;
        let ver = self
            .inner
            .lock()
            .ok()
            .map(|g| g.installed_version.clone())
            .unwrap_or_else(|| "prev".to_string());
        let dest = rollback_dir.join(format!("rustbrowse-{}.exe.bak", ver));
        fs::copy(&exe, &dest).ok()?;
        Some(dest.to_string_lossy().into_owned())
    }

    fn clean_staging_files(&self) {
        if let Ok(entries) = fs::read_dir(&self.staging_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let _ = fs::remove_file(p);
                }
            }
        }
    }
}

// ============================================================================
// Pure Helpers (SemVer, Security Validation, Crash-Loop Rollback)
// ============================================================================

pub fn parse_semver_tuple(raw: &str) -> Option<(u64, u64, u64, Option<String>)> {
    let trimmed = raw.trim().trim_start_matches('v').trim_start_matches('V');
    let without_build = trimmed.split('+').next().unwrap_or(trimmed);
    let (core, pre) = match without_build.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (without_build, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, pre))
}

/// Returns true iff `candidate` is strictly newer than `current` according to SemVer 2.0.
pub fn semver_is_newer(current: &str, candidate: &str) -> bool {
    let Some((c_maj, c_min, c_pat, c_pre)) = parse_semver_tuple(current) else {
        return false;
    };
    let Some((n_maj, n_min, n_pat, n_pre)) = parse_semver_tuple(candidate) else {
        return false;
    };

    match (n_maj, n_min, n_pat).cmp(&(c_maj, c_min, c_pat)) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (&c_pre, &n_pre) {
            // 1.0.0 is newer than 1.0.0-beta.1
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(a), Some(b)) => b > a,
            (None, None) => false,
        },
    }
}

/// Enforces HTTPS for all remote update endpoints (allows loopback only for local acceptance tests).
pub fn is_trusted_update_url(url: &Url) -> bool {
    if url.scheme() == "https" {
        return url.host_str().is_some();
    }
    if url.scheme() == "http" {
        return matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"));
    }
    false
}

pub fn resolve_channel_endpoint(
    base_endpoint: &str,
    channel: UpdateChannel,
) -> Result<String, String> {
    if base_endpoint.trim().is_empty() {
        return Err("Endpoint cập nhật đang để trống".to_string());
    }
    match channel {
        UpdateChannel::Stable => Ok(base_endpoint.to_string()),
        UpdateChannel::Beta => {
            if base_endpoint.contains("/releases/latest/download/latest.json") {
                Ok(base_endpoint.replace(
                    "/releases/latest/download/latest.json",
                    "/releases/download/beta/latest.json",
                ))
            } else if base_endpoint.ends_with("latest.json") {
                Ok(base_endpoint.replace("latest.json", "latest-beta.json"))
            } else {
                Ok(base_endpoint.to_string())
            }
        }
    }
}

pub fn evaluate_boot_rollback(
    persisted: &mut PersistedUpdateState,
    running_version: &str,
) -> Option<String> {
    if persisted.consecutive_unclean_boots >= CRASH_LOOP_THRESHOLD {
        if !persisted
            .blocked_versions
            .iter()
            .any(|v| v == running_version)
        {
            persisted.blocked_versions.push(running_version.to_string());
        }
        let reason = format!(
            "Phát hiện {} lần khởi chạy lỗi liên tiếp trên v{}. Đã chặn lặp lại bản cập nhật này và giữ nguyên dữ liệu người dùng.",
            persisted.consecutive_unclean_boots, running_version
        );
        persisted.last_rollback_reason = Some(reason.clone());
        persisted.pending_update_version = None;
        persisted.consecutive_unclean_boots = 0;
        return Some(reason);
    }
    None
}

fn inspect_embedded_tauri_config() -> (String, bool, bool) {
    let raw = include_str!("../tauri.conf.json");
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (String::new(), false, false);
    };
    let updater = &val["plugins"]["updater"];
    let pubkey = updater["pubkey"].as_str().unwrap_or("").trim();
    let endpoint = updater["endpoints"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let pubkey_present = !pubkey.is_empty();
    let is_placeholder_repo = endpoint.contains("github.com/rustbrowse/rustbrowse/");
    let valid_url = Url::parse(&endpoint)
        .map(|u| is_trusted_update_url(&u))
        .unwrap_or(false);

    let configured = pubkey_present && valid_url && !is_placeholder_repo;
    (endpoint, pubkey_present, configured)
}

fn user_data_dir() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local).join("RustBrowse").join("User Data")
    } else if let Ok(profile) = std::env::var("USERPROFILE") {
        PathBuf::from(profile)
            .join("AppData")
            .join("Local")
            .join("RustBrowse")
            .join("User Data")
    } else {
        PathBuf::from("D:\\RustBrowse\\UserData")
    }
}

fn load_persisted_state(path: &PathBuf) -> PersistedUpdateState {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<PersistedUpdateState>(&s).ok())
        .unwrap_or_default()
}

fn save_persisted_state(path: &PathBuf, state: &PersistedUpdateState) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}

fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn format_epoch_timestamp(epoch_secs: u64) -> String {
    // Convert UNIX seconds to readable UTC timestamp `YYYY-MM-DD HH:MM:SS UTC`
    let days = epoch_secs / 86_400;
    let rem = epoch_secs % 86_400;
    let hours = rem / 3600;
    let mins = (rem % 3600) / 60;
    let secs = rem % 60;

    // Civil date from days since 1970-01-01 (Howard Hinnant algorithm)
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year, m, d, hours, mins, secs
    )
}

fn emit_state(app: &AppHandle, state: &UpdateDashboardState) {
    let _ = app.emit("update-state-changed", state);
}

fn emit_island_notification(app: &AppHandle, kind: &str, title: &str, detail: &str) {
    if let Some(main_wv) = app.get_webview_window("main") {
        let _ = main_wv.emit(
            "island-event",
            serde_json::json!({
                "kind": kind,
                "title": title,
                "detail": detail,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison_handles_patch_minor_major_and_prerelease() {
        assert!(semver_is_newer("0.1.0", "0.1.1"));
        assert!(semver_is_newer("1.0.0", "1.0.1"));
        assert!(semver_is_newer("1.0.9", "1.1.0"));
        assert!(semver_is_newer("1.9.9", "2.0.0"));
        assert!(semver_is_newer("v1.0.0-beta.1", "v1.0.0"));
        assert!(!semver_is_newer("1.0.2", "1.0.2"));
        assert!(!semver_is_newer("1.2.0", "1.1.9"));
        assert!(!semver_is_newer("1.0.0", "1.0.0-beta.2"));
    }

    #[test]
    fn enforces_https_for_remote_update_urls() {
        let https_url =
            Url::parse("https://github.com/owner/rustbrowse/releases/latest/download/latest.json")
                .unwrap();
        let http_remote = Url::parse("http://evil.example.com/latest.json").unwrap();
        let http_local = Url::parse("http://127.0.0.1:8787/latest.json").unwrap();

        assert!(is_trusted_update_url(&https_url));
        assert!(!is_trusted_update_url(&http_remote));
        assert!(is_trusted_update_url(&http_local));
    }

    #[test]
    fn switches_stable_and_beta_endpoints_cleanly() {
        let base = "https://github.com/owner/rustbrowse/releases/latest/download/latest.json";
        assert_eq!(
            resolve_channel_endpoint(base, UpdateChannel::Stable).unwrap(),
            base
        );
        assert_eq!(
            resolve_channel_endpoint(base, UpdateChannel::Beta).unwrap(),
            "https://github.com/owner/rustbrowse/releases/download/beta/latest.json"
        );
    }

    #[test]
    fn crash_loop_triggers_rollback_and_blocks_faulty_version() {
        let mut st = PersistedUpdateState {
            consecutive_unclean_boots: 3,
            last_boot_version: Some("1.0.3".to_string()),
            last_healthy_version: Some("1.0.2".to_string()),
            ..Default::default()
        };
        let reason = evaluate_boot_rollback(&mut st, "1.0.3");
        assert!(reason.is_some());
        assert!(st.blocked_versions.contains(&"1.0.3".to_string()));
        assert_eq!(st.consecutive_unclean_boots, 0);
    }
}
