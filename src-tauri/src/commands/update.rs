use std::future::Future;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

pub use super::update_cache::DownloadedUpdate;
use super::update_cache::{self, CacheRecord};
pub use app_core::update::UpdateInfo;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

use app_platform::identity::{GITHUB_RELEASE_DOWNLOAD_PREFIX, RELEASE_URL_PREFIX, UPDATE_ENDPOINT};
const OFFICIAL_UPDATE_ENDPOINTS: [&str; 1] = [UPDATE_ENDPOINT];
const UPDATE_DOWNLOAD_PROGRESS_EVENT: &str = "update-download-progress";
const DOWNLOAD_CANCELED_ERROR: &str = "Download canceled by user.";
const DOWNLOAD_STALL_TIMEOUT: Duration = Duration::from_secs(15);
const IS_WINDOWS_7_TARGET: bool = cfg!(target_vendor = "win7");

#[derive(Clone, Debug, Serialize)]
pub struct UpdateDownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub attempt_id: String,
    pub version: String,
}

#[derive(Default)]
struct UpdateDownloadProgressGate {
    last_visible_percentage: Option<u128>,
}

impl UpdateDownloadProgressGate {
    fn should_emit(&mut self, downloaded: u64, total: Option<u64>) -> bool {
        let visible_percentage = match total {
            Some(total) if total > 0 => {
                let total = total as u128;
                ((downloaded as u128).saturating_mul(100).saturating_add(total / 2)) / total
            }
            _ => 0,
        };
        if self.last_visible_percentage == Some(visible_percentage) {
            return false;
        }
        self.last_visible_percentage = Some(visible_percentage);
        true
    }
}

enum PendingUpdate {
    Downloading(Arc<DownloadCancellation>),
    Installing,
    Ready(Box<CachedUpdate>),
}

struct CachedUpdate {
    record: CacheRecord,
    ready: ReadyUpdate,
}

enum ReadyUpdate {
    Installer { update: Box<Update>, bytes: Vec<u8> },
}

#[derive(Debug)]
struct DownloadCancellation {
    canceled: tokio::sync::watch::Sender<bool>,
}

impl Default for DownloadCancellation {
    fn default() -> Self {
        let (canceled, _) = tokio::sync::watch::channel(false);
        Self { canceled }
    }
}

impl DownloadCancellation {
    fn cancel(&self) {
        self.canceled.send_replace(true);
    }

    fn is_canceled(&self) -> bool {
        *self.canceled.borrow()
    }

    async fn canceled(&self) {
        let mut canceled = self.canceled.subscribe();
        if *canceled.borrow() {
            return;
        }
        while canceled.changed().await.is_ok() {
            if *canceled.borrow_and_update() {
                return;
            }
        }
    }
}

#[derive(Default)]
pub struct PendingUpdateState {
    pending: Mutex<Option<PendingUpdate>>,
}

impl PendingUpdateState {
    fn begin_download(&self) -> Result<Arc<DownloadCancellation>, String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Update state is unavailable.".to_string())?;
        if pending.is_some() {
            return Err("An update is already downloading or ready to install.".to_string());
        }
        let cancellation = Arc::new(DownloadCancellation::default());
        *pending = Some(PendingUpdate::Downloading(Arc::clone(&cancellation)));
        Ok(cancellation)
    }

    fn finish_download(
        &self,
        cancellation: &Arc<DownloadCancellation>,
        update: CachedUpdate,
        root: &std::path::Path,
    ) -> Result<(), String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Update state is unavailable.".to_string())?;
        let is_current = matches!(
            pending.as_ref(),
            Some(PendingUpdate::Downloading(current))
                if Arc::ptr_eq(current, cancellation) && !cancellation.is_canceled()
        );
        if !is_current {
            return Err(DOWNLOAD_CANCELED_ERROR.to_string());
        }
        let ReadyUpdate::Installer { bytes, .. } = &update.ready;
        update_cache::commit(root, &update.record, bytes)?;
        *pending = Some(PendingUpdate::Ready(Box::new(update)));
        Ok(())
    }

    fn finish_failed_download(&self, cancellation: &Arc<DownloadCancellation>) -> Result<(), String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Update state is unavailable.".to_string())?;
        let is_current = matches!(
            pending.as_ref(),
            Some(PendingUpdate::Downloading(current)) if Arc::ptr_eq(current, cancellation)
        );
        if is_current {
            *pending = None;
        }
        Ok(())
    }

    pub fn cancel_download(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            if let Some(PendingUpdate::Downloading(cancellation)) = pending.as_ref() {
                cancellation.cancel();
                *pending = None;
            }
        }
    }

    fn take_ready(&self, cache_id: &str, expected_version: &str) -> Result<CachedUpdate, String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Update state is unavailable.".to_string())?;
        match pending.take() {
            Some(PendingUpdate::Ready(update))
                if update.record.info.cache_id == cache_id && update.record.info.version == expected_version =>
            {
                *pending = Some(PendingUpdate::Installing);
                Ok(*update)
            }
            other => {
                *pending = other;
                Err("No downloaded update is ready to install.".to_string())
            }
        }
    }

    fn restore_ready(&self, update: CachedUpdate) -> Result<(), String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Update state is unavailable.".to_string())?;
        *pending = Some(PendingUpdate::Ready(Box::new(update)));
        Ok(())
    }

    fn discard_ready(&self, cache_id: &str, root: &std::path::Path) -> Result<(), String> {
        let mut pending = self.pending.lock().map_err(|_| "Update state is unavailable.")?;
        match pending.as_ref() {
            Some(PendingUpdate::Ready(cached)) if cached.record.info.cache_id == cache_id => {}
            _ => return Err("Cached update changed or is busy.".into()),
        }
        // Logical eviction must succeed even if disk cleanup fails. A later download
        // retries cleanup before any network work or cache commit.
        *pending = None;
        update_cache::discard(root)
    }

    fn finish_install(&self) -> Result<(), String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Update state is unavailable.".to_string())?;
        *pending = None;
        Ok(())
    }
}

fn updater_endpoints() -> Vec<String> {
    OFFICIAL_UPDATE_ENDPOINTS
        .iter()
        .map(|endpoint| endpoint.to_string())
        .collect()
}

fn installer_asset_candidates(download_url: &str, latest_version: Option<&str>) -> Vec<String> {
    let filename = download_url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("");

    let tag = latest_version.map(tag_version).unwrap_or_else(|| {
        if let Some(pos) = download_url.find("/releases/download/") {
            let rest = &download_url[pos + "/releases/download/".len()..];
            if let Some(tag_end) = rest.find('/') {
                return rest[..tag_end].to_string();
            }
        }
        "".to_string()
    });

    let mut raw_candidates = vec![download_url.to_string()];
    if !tag.is_empty() && !filename.is_empty() {
        raw_candidates.push(format!("{GITHUB_RELEASE_DOWNLOAD_PREFIX}{tag}/{filename}"));
    }

    let mut unique = Vec::new();
    for url in raw_candidates {
        if !unique.contains(&url) {
            unique.push(url);
        }
    }
    unique
}

fn tag_version(version: &str) -> String {
    let version = version.trim();
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

#[tauri::command]
pub async fn check_for_updates() -> Result<UpdateInfo, String> {
    let release = app_core::update::fetch_latest_release().await?;
    let current_version = env!("CARGO_PKG_VERSION");
    let mut info = app_core::update::build_update_info(release, current_version);
    info.manual_update_only = requires_manual_update(IS_WINDOWS_7_TARGET);
    Ok(info)
}

fn requires_manual_update(is_windows_7_target: bool) -> bool {
    is_windows_7_target
}

#[tauri::command]
pub async fn fetch_changelog() -> Result<app_core::changelog::ChangelogData, String> {
    app_core::changelog::fetch_changelog().await
}

#[tauri::command]
pub async fn get_system_proxy_url() -> Option<String> {
    tauri::async_runtime::spawn_blocking(app_core::update::system_proxy_url)
        .await
        .ok()
        .flatten()
}

#[tauri::command]
pub fn cancel_update_download(state: tauri::State<'_, PendingUpdateState>) {
    state.cancel_download();
}

fn package_kind(app: &AppHandle) -> Result<String, String> {
    Ok(app
        .updater_builder()
        .build()
        .map_err(|e| e.to_string())?
        .package_kind()
        .into())
}

fn validate_cache_target(record: &CacheRecord, package_kind: &str) -> Result<(), String> {
    if record.package_kind != package_kind || record.os != std::env::consts::OS || record.arch != std::env::consts::ARCH
    {
        return Err("Cached update does not match this installation.".into());
    }
    Ok(())
}

fn restore_cached(app: &AppHandle) -> Result<Option<CachedUpdate>, String> {
    let root = update_cache::root(app)?;
    if requires_manual_update(IS_WINDOWS_7_TARGET) {
        update_cache::discard(&root)?;
        return Ok(None);
    }
    let restored = (|| {
        let Some((record, bytes)) = update_cache::read(&root)? else {
            return Ok(None);
        };
        validate_cache_target(&record, &package_kind(app)?)?;
        let update = app
            .updater_builder()
            .build()
            .map_err(|e| e.to_string())?
            .restore_update(record.manifest.clone().ok_or("Missing cached release metadata")?)
            .map_err(|e| e.to_string())?
            .ok_or("Cached update is no longer newer")?;
        if update.version != record.info.version {
            return Err("Cached release version mismatch".into());
        }
        update.verify(&bytes).map_err(|e| e.to_string())?;
        Ok(Some(CachedUpdate {
            record,
            ready: ReadyUpdate::Installer {
                update: Box::new(update),
                bytes,
            },
        }))
    })();
    if restored.is_err() {
        update_cache::discard(&root)?;
    }
    restored
}

#[tauri::command(async)]
pub fn get_downloaded_update(
    app: AppHandle,
    state: tauri::State<'_, PendingUpdateState>,
) -> Result<Option<DownloadedUpdate>, String> {
    let mut pending = state.pending.lock().map_err(|_| "Update state is unavailable.")?;
    match pending.as_ref() {
        Some(PendingUpdate::Ready(cached)) => return Ok(Some(cached.record.info.clone())),
        Some(_) => return Ok(None),
        None => {}
    }
    update_cache::cleanup_partial(&update_cache::root(&app)?)?;
    match restore_cached(&app) {
        Ok(Some(cached)) => {
            let info = cached.record.info.clone();
            *pending = Some(PendingUpdate::Ready(Box::new(cached)));
            Ok(Some(info))
        }
        Ok(None) => Ok(None),
        Err(error) => {
            eprintln!("[UPDATE] discarded invalid cache: {error}");
            Ok(None)
        }
    }
}

#[tauri::command(async)]
pub fn discard_downloaded_update(
    app: AppHandle,
    state: tauri::State<'_, PendingUpdateState>,
    cache_id: String,
) -> Result<(), String> {
    state.discard_ready(&cache_id, &update_cache::root(&app)?)
}

#[tauri::command]
pub async fn download_update(
    app: AppHandle,
    state: tauri::State<'_, PendingUpdateState>,
    latest_version: String,
    attempt_id: String,
    release_notes: Option<String>,
) -> Result<DownloadedUpdate, String> {
    if requires_manual_update(IS_WINDOWS_7_TARGET) {
        return Err("Windows 7 builds must be updated with the dedicated Windows 7 offline installer.".to_string());
    }
    if attempt_id.is_empty() || attempt_id.len() > 128 {
        return Err("Invalid download attempt identifier.".into());
    }
    let cancellation = state.begin_download()?;
    let result = async {
        let cache_root = update_cache::root(&app)?;
        update_cache::discard(&cache_root)?;
        update_cache::ensure_writable(&cache_root)?;
        let (update, bytes) = download_update_inner(&app, Some(&latest_version), &attempt_id, &cancellation).await?;
        let manifest = Some(update.raw_json.clone());
        let notes = update.body.clone().unwrap_or_default();
        let package_length = bytes.len() as u64;
        let package_sha256 = update_cache::digest(&bytes);
        let ready = ReadyUpdate::Installer {
            update: Box::new(update),
            bytes,
        };
        let version_text = latest_version.trim().trim_start_matches('v');
        let info = DownloadedUpdate {
            cache_id: uuid::Uuid::new_v4().to_string(),
            version: version_text.to_string(),
            portable_mode: false,
            release_url: format!("{RELEASE_URL_PREFIX}{version_text}"),
            release_notes: release_notes.unwrap_or(notes),
            downloaded_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis() as u64,
        };
        let cached = CachedUpdate {
            record: CacheRecord {
                schema_version: 1,
                package_kind: package_kind(&app)?,
                package_length,
                package_sha256,
                info: info.clone(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                manifest,
                signature: None,
            },
            ready,
        };
        state.finish_download(&cancellation, cached, &update_cache::root(&app)?)?;
        Ok(info)
    }
    .await;
    if result.is_err() {
        state.finish_failed_download(&cancellation)?;
    }
    result
}

async fn download_update_inner(
    app: &AppHandle,
    latest_version: Option<&str>,
    attempt_id: &str,
    cancellation: &Arc<DownloadCancellation>,
) -> Result<(Update, Vec<u8>), String> {
    let endpoint_urls = updater_endpoints();
    println!("[UPDATE] checking from endpoints: {}", endpoint_urls.join(", "));
    let mut endpoints = Vec::with_capacity(endpoint_urls.len());
    for endpoint_url in endpoint_urls {
        endpoints.push(
            endpoint_url
                .parse()
                .map_err(|e| format!("Invalid update endpoint: {e}"))?,
        );
    }
    let mut builder = app
        .updater_builder()
        .endpoints(endpoints)
        .map_err(|e| format!("Failed to configure updater endpoint: {e}"))?;

    if let Some(proxy_url) = app_core::update::system_proxy_url() {
        let proxy = proxy_url
            .parse()
            .map_err(|e| format!("Invalid system proxy URL: {e}"))?;
        builder = builder.proxy(proxy);
    }

    let updater = builder.build().map_err(|e| format!("Failed to create updater: {e}"))?;
    let update = wait_for_download_step(
        updater.check(),
        cancellation,
        Duration::from_secs(30),
        "Update check timed out.".into(),
    )
    .await?
    .map_err(|e| format!("Failed to check updates: {e}"))?;
    let Some(update) = update else {
        return Err("No update available.".to_string());
    };

    if Some(update.version.as_str()) != latest_version.map(|v| v.trim().trim_start_matches('v')) {
        return Err("Update version changed; check for updates again.".into());
    }
    let candidates = installer_asset_candidates(update.download_url.as_str(), latest_version);
    println!("[UPDATE] candidates for installer download: {:?}", candidates);

    let mut failures = Vec::new();

    for candidate_url in candidates {
        if cancellation.is_canceled() {
            return Err(DOWNLOAD_CANCELED_ERROR.to_string());
        }
        println!("[UPDATE] downloading installer update from {candidate_url}");
        let parsed_url = match reqwest::Url::parse(&candidate_url) {
            Ok(url) => url,
            Err(e) => {
                failures.push(format!("{candidate_url}: Invalid URL ({e})"));
                continue;
            }
        };

        let downloaded = Arc::new(AtomicU64::new(0));
        let finished_downloaded = Arc::clone(&downloaded);
        let progress_app_chunk = app.clone();
        let progress_app_finish = app.clone();
        let mut progress_gate = UpdateDownloadProgressGate::default();

        let (progress_tx, progress_rx) = tokio::sync::mpsc::channel::<()>(16);

        let attempt_id = attempt_id.to_string();
        let version = update.version.clone();
        let finish_attempt_id = attempt_id.clone();
        let finish_version = version.clone();
        let download_result = {
            let mut candidate_update = update.clone();
            candidate_update.download_url = parsed_url.clone();

            let download_fut = async move {
                candidate_update
                    .download(
                        move |chunk_len, total| {
                            let downloaded = downloaded
                                .fetch_add(chunk_len as u64, Ordering::Relaxed)
                                .saturating_add(chunk_len as u64);
                            if progress_gate.should_emit(downloaded, total) {
                                let _ = progress_app_chunk.emit(
                                    UPDATE_DOWNLOAD_PROGRESS_EVENT,
                                    UpdateDownloadProgress {
                                        downloaded,
                                        total,
                                        attempt_id: attempt_id.clone(),
                                        version: version.clone(),
                                    },
                                );
                            }
                            let _ = progress_tx.try_send(());
                        },
                        move || {
                            let downloaded = finished_downloaded.load(Ordering::Relaxed);
                            let _ = progress_app_finish.emit(
                                UPDATE_DOWNLOAD_PROGRESS_EVENT,
                                UpdateDownloadProgress {
                                    downloaded,
                                    total: Some(downloaded),
                                    attempt_id: finish_attempt_id,
                                    version: finish_version,
                                },
                            );
                        },
                    )
                    .await
                    .map_err(|error| format!("Failed to download update: {error}"))
            };

            wait_for_progressing_download(download_fut, progress_rx, cancellation, DOWNLOAD_STALL_TIMEOUT).await
        };

        match download_result {
            Ok(bytes) => {
                let mut ready_update = update.clone();
                ready_update.download_url = parsed_url;
                return Ok((ready_update, bytes));
            }
            Err(error) => {
                if cancellation.is_canceled() || error.contains("canceled") {
                    return Err(DOWNLOAD_CANCELED_ERROR.to_string());
                }
                println!("[UPDATE] installer candidate failed ({candidate_url}): {error}");
                failures.push(format!("{candidate_url}: {error}"));
            }
        }
    }

    Err(format!(
        "Failed to download update after trying all available mirrors. {}",
        failures.join("; ")
    ))
}

async fn wait_for_progressing_download<T>(
    download: impl Future<Output = Result<T, String>>,
    mut progress: tokio::sync::mpsc::Receiver<()>,
    cancellation: &DownloadCancellation,
    stall_timeout: Duration,
) -> Result<T, String> {
    tokio::pin!(download);
    let stall = tokio::time::sleep(stall_timeout);
    tokio::pin!(stall);
    let mut progress_open = true;

    loop {
        tokio::select! {
            biased;
            _ = cancellation.canceled() => return Err(DOWNLOAD_CANCELED_ERROR.to_string()),
            result = &mut download => return result,
            update = progress.recv(), if progress_open => {
                if update.is_some() {
                    stall.as_mut().reset(tokio::time::Instant::now() + stall_timeout);
                } else {
                    progress_open = false;
                }
            }
            _ = &mut stall => return Err("Download stalled: no data received for 15 seconds".to_string()),
        }
    }
}

async fn wait_for_download_step<T>(
    step: impl Future<Output = T>,
    cancellation: &DownloadCancellation,
    stall_timeout: Duration,
    timeout_error: String,
) -> Result<T, String> {
    tokio::select! {
        biased;
        _ = cancellation.canceled() => Err(DOWNLOAD_CANCELED_ERROR.to_string()),
        result = tokio::time::timeout(stall_timeout, step) => result.map_err(|_| timeout_error),
    }
}

#[tauri::command(async)]
pub fn install_downloaded_update(
    app: AppHandle,
    state: tauri::State<'_, PendingUpdateState>,
    cache_id: String,
    expected_version: String,
) -> Result<(), String> {
    let cached = state.take_ready(&cache_id, &expected_version)?;
    // Read and reverify disk bytes immediately before installation; never trust memory alone.
    let refreshed = match restore_cached(&app) {
        Ok(Some(ready)) if ready.record.info.cache_id == cache_id && ready.record.info.version == expected_version => {
            ready
        }
        other => {
            state.finish_install()?;
            return Err(match other {
                Err(e) => e,
                _ => "Cached update is missing or changed.".into(),
            });
        }
    };
    let ready = &refreshed.ready;
    let install_result = match ready {
        ReadyUpdate::Installer { update, bytes } => update
            .install(bytes)
            .map_err(|error| format!("Failed to install update: {error}")),
    };
    if let Err(error) = install_result {
        state.restore_ready(cached)?;
        return Err(error);
    }
    state.finish_install()?;
    let _ = update_cache::discard(&update_cache::root(&app)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        installer_asset_candidates, requires_manual_update, tag_version, updater_endpoints,
        wait_for_progressing_download, DownloadCancellation, PendingUpdateState, UpdateDownloadProgressGate,
        DOWNLOAD_CANCELED_ERROR, GITHUB_RELEASE_DOWNLOAD_PREFIX, OFFICIAL_UPDATE_ENDPOINTS,
    };
    use std::{future::pending, sync::Arc, time::Duration};

    #[test]
    fn all_windows_7_builds_require_manual_updates() {
        assert!(requires_manual_update(true));
        assert!(!requires_manual_update(false));
    }

    #[test]
    fn normalizes_update_tag_versions() {
        assert_eq!(tag_version("0.5.39"), "v0.5.39");
        assert_eq!(tag_version("v0.5.39"), "v0.5.39");
    }

    #[test]
    fn builds_update_endpoints_from_github() {
        let endpoints = updater_endpoints();
        assert_eq!(
            endpoints,
            OFFICIAL_UPDATE_ENDPOINTS
                .iter()
                .map(|endpoint| endpoint.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn installer_candidates_dedupe_github_urls() {
        let url = format!("{GITHUB_RELEASE_DOWNLOAD_PREFIX}v0.5.64/MyApp_0.5.64_x64-setup.nsis.zip");
        assert_eq!(installer_asset_candidates(&url, Some("0.5.64")), vec![url]);
    }

    #[test]
    fn installer_candidates_append_github_fallback() {
        let candidates =
            installer_asset_candidates("https://mirror.test/MyApp_0.5.64_x64-setup.nsis.zip", Some("0.5.64"));
        assert_eq!(
            candidates,
            vec![
                "https://mirror.test/MyApp_0.5.64_x64-setup.nsis.zip".to_string(),
                format!("{GITHUB_RELEASE_DOWNLOAD_PREFIX}v0.5.64/MyApp_0.5.64_x64-setup.nsis.zip"),
            ]
        );
    }

    #[test]
    fn emits_progress_only_when_the_visible_percentage_changes() {
        let mut gate = UpdateDownloadProgressGate::default();

        assert!(gate.should_emit(0, Some(1_000)));
        assert!(!gate.should_emit(4, Some(1_000)));
        assert!(gate.should_emit(5, Some(1_000)));
        assert!(!gate.should_emit(14, Some(1_000)));
        assert!(gate.should_emit(15, Some(1_000)));
        assert!(gate.should_emit(1_000, Some(1_000)));
    }

    #[test]
    fn keeps_unknown_length_progress_visually_stable_until_completion() {
        let mut gate = UpdateDownloadProgressGate::default();

        assert!(gate.should_emit(0, None));
        assert!(!gate.should_emit(512, None));
        assert!(!gate.should_emit(1_024, Some(0)));
        assert!(gate.should_emit(1_024, Some(1_024)));
    }

    #[test]
    fn handles_large_progress_values_without_overflow() {
        let mut gate = UpdateDownloadProgressGate::default();

        assert!(gate.should_emit(u64::MAX / 2, Some(u64::MAX)));
        assert!(gate.should_emit(u64::MAX, Some(u64::MAX)));
    }

    #[test]
    fn limits_chunk_events_to_visible_percentage_updates() {
        let total = 19_527_892_u64;
        let mut downloaded = 0_u64;
        let mut emitted = 0;
        let mut gate = UpdateDownloadProgressGate::default();

        while downloaded < total {
            downloaded = downloaded.saturating_add(16_384).min(total);
            emitted += usize::from(gate.should_emit(downloaded, Some(total)));
        }

        assert_eq!(emitted, 101);
    }

    #[test]
    fn retry_uses_an_independent_cancellation_token() {
        let state = PendingUpdateState::default();
        let first = state.begin_download().unwrap();

        state.cancel_download();
        let second = state.begin_download().unwrap();

        assert!(first.is_canceled());
        assert!(!second.is_canceled());
    }

    #[test]
    fn stale_attempt_cannot_clear_an_active_retry() {
        let state = PendingUpdateState::default();
        let first = state.begin_download().unwrap();
        state.cancel_download();
        let second = state.begin_download().unwrap();

        state.finish_failed_download(&first).unwrap();

        assert!(state.begin_download().is_err());
        assert!(!second.is_canceled());
    }

    #[tokio::test]
    async fn cancel_wakes_installer_download_without_waiting_for_stall_timeout() {
        let cancellation = Arc::new(DownloadCancellation::default());
        let task_cancellation = Arc::clone(&cancellation);
        let (_progress_tx, progress_rx) = tokio::sync::mpsc::channel(1);
        let task = tokio::spawn(async move {
            wait_for_progressing_download(
                pending::<Result<(), String>>(),
                progress_rx,
                &task_cancellation,
                Duration::from_secs(30),
            )
            .await
        });

        tokio::task::yield_now().await;
        cancellation.cancel();

        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.unwrap_err(), DOWNLOAD_CANCELED_ERROR);
    }
}
