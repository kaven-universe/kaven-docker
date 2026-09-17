mod registry;
mod settings;

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use registry::{
    ConnectionStatus, ImageDetails, RegistryClient, RepositoryPage, SyncPlatform, SyncReporter,
    SyncResult, TagList,
};
use serde::Deserialize;
use settings::{
    AppSettings, ProxyProfile, RegistryConfig, SaveAppSettingsInput, SaveProxyProfileInput,
    SaveRegistryInput,
};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

static SYNC_CANCELLATIONS: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();

struct SyncCancellation {
    operation_id: String,
    token: CancellationToken,
}

impl SyncCancellation {
    fn new(operation_id: &str) -> Self {
        let token = if let Ok(mut cancellations) = SYNC_CANCELLATIONS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            cancellations
                .entry(operation_id.to_string())
                .or_insert_with(CancellationToken::new)
                .clone()
        } else {
            CancellationToken::new()
        };
        Self {
            operation_id: operation_id.to_string(),
            token,
        }
    }
}

impl Drop for SyncCancellation {
    fn drop(&mut self) {
        if let Ok(mut cancellations) = SYNC_CANCELLATIONS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            cancellations.remove(&self.operation_id);
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncBatchItem {
    source_repository: String,
    target_repository: String,
    tags: Vec<String>,
    #[serde(default)]
    platform: Option<SyncPlatform>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchSyncProgress {
    operation_id: String,
    current_repository: String,
    current: usize,
    total: usize,
    succeeded: usize,
    failed: usize,
    remaining: usize,
    status: String,
    error: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn emit_batch_sync_progress(
    app: &AppHandle,
    operation_id: &str,
    current_repository: &str,
    current: usize,
    total: usize,
    succeeded: usize,
    failed: usize,
    status: &str,
    error: Option<String>,
) {
    let _ = app.emit(
        "sync-batch-progress",
        BatchSyncProgress {
            operation_id: operation_id.to_string(),
            current_repository: current_repository.to_string(),
            current,
            total,
            succeeded,
            failed,
            remaining: total.saturating_sub(succeeded + failed),
            status: status.to_string(),
            error,
        },
    );
}

#[tauri::command]
fn cancel_sync(operation_id: String) -> bool {
    let token = SYNC_CANCELLATIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|cancellations| cancellations.get(&operation_id).cloned());
    if let Some(token) = token {
        token.cancel();
        true
    } else {
        false
    }
}

pub(crate) fn app_error(code: &str, detail: impl std::fmt::Display) -> String {
    let detail = detail.to_string();
    if detail.is_empty() {
        format!("KAVEN_DOCKER|{code}")
    } else {
        format!("KAVEN_DOCKER|{code}|{detail}")
    }
}

#[tauri::command]
fn list_registries(app: AppHandle) -> Result<Vec<RegistryConfig>, String> {
    settings::load_registries(&app)
}

#[tauri::command]
fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    settings::load_app_settings(&app)
}

#[tauri::command]
fn save_app_settings(
    app: AppHandle,
    settings: SaveAppSettingsInput,
) -> Result<AppSettings, String> {
    settings::save_app_settings(&app, settings)
}

#[tauri::command]
fn list_proxy_profiles(app: AppHandle) -> Result<Vec<ProxyProfile>, String> {
    settings::load_proxy_profiles(&app)
}

#[tauri::command]
fn save_proxy_profile(
    app: AppHandle,
    proxy: SaveProxyProfileInput,
) -> Result<ProxyProfile, String> {
    settings::save_proxy_profile(&app, proxy)
}

#[tauri::command]
fn delete_proxy_profile(app: AppHandle, id: String) -> Result<(), String> {
    settings::delete_proxy_profile(&app, &id)
}

#[tauri::command]
fn save_registry(app: AppHandle, registry: SaveRegistryInput) -> Result<RegistryConfig, String> {
    settings::save_registry(&app, registry)
}

#[tauri::command]
fn delete_registry(app: AppHandle, id: String) -> Result<(), String> {
    settings::delete_registry(&app, &id)
}

#[tauri::command]
fn reorder_registries(
    app: AppHandle,
    registry_ids: Vec<String>,
) -> Result<Vec<RegistryConfig>, String> {
    settings::reorder_registries(&app, &registry_ids)
}

#[tauri::command]
fn add_repositories(
    app: AppHandle,
    registry_id: String,
    repositories: Vec<String>,
) -> Result<Vec<String>, String> {
    settings::add_repositories(&app, &registry_id, &repositories)
}

#[tauri::command]
fn remove_repository(
    app: AppHandle,
    registry_id: String,
    repository: String,
) -> Result<(), String> {
    settings::remove_repository(&app, &registry_id, &repository)
}

#[tauri::command]
async fn test_connection(app: AppHandle, registry_id: String) -> Result<ConnectionStatus, String> {
    RegistryClient::from_id(&app, &registry_id)?
        .test_connection()
        .await
}

#[tauri::command]
async fn list_repositories(app: AppHandle, registry_id: String) -> Result<RepositoryPage, String> {
    RegistryClient::from_id(&app, &registry_id)?
        .list_repositories()
        .await
}

#[tauri::command]
async fn discover_repositories(
    app: AppHandle,
    registry_id: String,
    namespace: String,
) -> Result<RepositoryPage, String> {
    RegistryClient::from_id(&app, &registry_id)?
        .discover_repositories(&namespace)
        .await
}

#[tauri::command]
async fn list_tags(
    app: AppHandle,
    registry_id: String,
    repository: String,
) -> Result<TagList, String> {
    RegistryClient::from_id(&app, &registry_id)?
        .list_tags(&repository)
        .await
}

#[tauri::command]
async fn get_image_details(
    app: AppHandle,
    registry_id: String,
    repository: String,
    reference: String,
) -> Result<ImageDetails, String> {
    RegistryClient::from_id(&app, &registry_id)?
        .image_details(&repository, &reference)
        .await
}

#[tauri::command]
async fn delete_manifest(
    app: AppHandle,
    registry_id: String,
    repository: String,
    reference: String,
) -> Result<String, String> {
    RegistryClient::from_id(&app, &registry_id)?
        .delete_manifest(&repository, &reference)
        .await
}

#[tauri::command]
async fn sync_images(
    app: AppHandle,
    operation_id: String,
    source_registry_id: String,
    target_registry_id: String,
    source_repository: String,
    target_repository: String,
    tags: Vec<String>,
    platform: Option<SyncPlatform>,
) -> Result<SyncResult, String> {
    let cancellation = SyncCancellation::new(&operation_id);
    let reporter = SyncReporter::new(&app, &operation_id)?;
    let result = async {
        if tags.is_empty() {
            return Err(app_error("syncTagsRequired", ""));
        }
        if source_repository.trim().is_empty() || target_repository.trim().is_empty() {
            return Err(app_error("syncRepositoryRequired", ""));
        }

        let platform = platform.map(SyncPlatform::validate).transpose()?;
        reporter.report(
            "prepare",
            "active",
            0,
            2,
            0,
            None,
            format!("Initializing source Registry client {source_registry_id}"),
            None,
            true,
        );
        let source_config = settings::find_registry(&app, &source_registry_id)?;
        reporter.report(
            "prepare",
            "active",
            0,
            2,
            0,
            None,
            format!(
                "Source Registry: {} ({}) using {}",
                source_config.name,
                source_config.url,
                proxy_summary(&app, &source_config)?
            ),
            None,
            true,
        );
        let source = RegistryClient::from_id(&app, &source_registry_id)?;
        reporter.report(
            "prepare",
            "active",
            1,
            2,
            0,
            None,
            format!("Source client ready for {source_repository}"),
            None,
            true,
        );

        let target_config = settings::find_registry(&app, &target_registry_id)?;
        if target_config.provider != "generic" {
            return Err(app_error("syncPublicTargetUnsupported", ""));
        }
        reporter.report(
            "prepare",
            "active",
            1,
            2,
            0,
            None,
            format!(
                "Target Registry: {} ({}) using {}",
                target_config.name,
                target_config.url,
                proxy_summary(&app, &target_config)?
            ),
            None,
            true,
        );
        let target = RegistryClient::from_id(&app, &target_registry_id)?;
        reporter.report(
            "prepare",
            "complete",
            2,
            2,
            0,
            None,
            format!("Target client ready for {target_repository}"),
            None,
            true,
        );

        tokio::select! {
            biased;
            _ = cancellation.token.cancelled() => Err(app_error("syncCancelled", "")),
            result = source.sync_to(
                    &target,
                    &source_repository,
                    &target_repository,
                    &tags,
                    platform.as_ref(),
                    &reporter,
                ) => result,
        }
    }
    .await;

    match result {
        Ok(result) => {
            reporter.report(
                "complete",
                "complete",
                1,
                1,
                result.transferred_bytes,
                Some(result.transferred_bytes),
                format!(
                    "Sync complete: {} tags, {} blobs copied, {} blobs skipped, {} cache hits, {} bytes transferred",
                    result.synced_tags,
                    result.copied_blobs,
                    result.skipped_blobs,
                    result.cached_blobs,
                    result.transferred_bytes
                ),
                None,
                true,
            );
            Ok(result)
        }
        Err(error) => {
            reporter.fail(&error);
            Err(error)
        }
    }
}

#[tauri::command]
async fn sync_images_batch(
    app: AppHandle,
    operation_id: String,
    source_registry_id: String,
    target_registry_id: String,
    mut items: Vec<SyncBatchItem>,
) -> Result<SyncResult, String> {
    let cancellation = SyncCancellation::new(&operation_id);
    let reporter = SyncReporter::new(&app, &operation_id)?;
    let result = async {
        if items.is_empty() || items.iter().any(|item| item.tags.is_empty()) {
            return Err(app_error("syncBatchSelectionRequired", ""));
        }
        if items.iter().any(|item| {
            item.source_repository.trim().is_empty() || item.target_repository.trim().is_empty()
        }) {
            return Err(app_error("syncRepositoryRequired", ""));
        }
        for item in &mut items {
            item.platform = item
                .platform
                .take()
                .map(SyncPlatform::validate)
                .transpose()?;
        }

        reporter.report(
            "prepare",
            "active",
            0,
            2,
            0,
            None,
            format!("Initializing batch source Registry client {source_registry_id}"),
            None,
            true,
        );
        let source_config = settings::find_registry(&app, &source_registry_id)?;
        reporter.report(
            "prepare",
            "active",
            0,
            2,
            0,
            None,
            format!(
                "Batch source Registry: {} ({}) using {}",
                source_config.name,
                source_config.url,
                proxy_summary(&app, &source_config)?
            ),
            None,
            true,
        );
        let source = RegistryClient::from_id(&app, &source_registry_id)?;

        let target_config = settings::find_registry(&app, &target_registry_id)?;
        if target_config.provider != "generic" {
            return Err(app_error("syncPublicTargetUnsupported", ""));
        }
        reporter.report(
            "prepare",
            "active",
            1,
            2,
            0,
            None,
            format!(
                "Batch target Registry: {} ({}) using {}",
                target_config.name,
                target_config.url,
                proxy_summary(&app, &target_config)?
            ),
            None,
            true,
        );
        let target = RegistryClient::from_id(&app, &target_registry_id)?;
        reporter.report(
            "prepare",
            "complete",
            2,
            2,
            0,
            None,
            format!("Batch clients ready for {} repositories", items.len()),
            None,
            true,
        );

        let mut total = SyncResult {
            synced_tags: 0,
            copied_blobs: 0,
            skipped_blobs: 0,
            cached_blobs: 0,
            transferred_bytes: 0,
            succeeded_repositories: 0,
            failed_repositories: Vec::new(),
        };
        for (index, item) in items.iter().enumerate() {
            emit_batch_sync_progress(
                &app,
                &operation_id,
                &item.source_repository,
                index,
                items.len(),
                total.succeeded_repositories,
                total.failed_repositories.len(),
                "running",
                None,
            );
            reporter.report(
                "manifests",
                "active",
                index,
                items.len(),
                0,
                None,
                format!(
                    "Batch repository {}/{}: {} -> {} ({} tags)",
                    index + 1,
                    items.len(),
                    item.source_repository,
                    item.target_repository,
                    item.tags.len()
                ),
                None,
                true,
            );
            let result = tokio::select! {
                biased;
                _ = cancellation.token.cancelled() => Err(app_error("syncCancelled", "")),
                result = source.sync_to(
                    &target,
                    &item.source_repository,
                    &item.target_repository,
                    &item.tags,
                    item.platform.as_ref(),
                    &reporter,
                ) => result,
            };
            if cancellation.token.is_cancelled() {
                return Err(app_error("syncCancelled", ""));
            }
            match result {
                Ok(result) => {
                    total.synced_tags += result.synced_tags;
                    total.copied_blobs += result.copied_blobs;
                    total.skipped_blobs += result.skipped_blobs;
                    total.cached_blobs += result.cached_blobs;
                    total.transferred_bytes += result.transferred_bytes;
                    total.succeeded_repositories += 1;
                    emit_batch_sync_progress(
                        &app,
                        &operation_id,
                        &item.source_repository,
                        index + 1,
                        items.len(),
                        total.succeeded_repositories,
                        total.failed_repositories.len(),
                        "success",
                        None,
                    );
                }
                Err(error) => {
                    total
                        .failed_repositories
                        .push(item.source_repository.clone());
                    reporter.report(
                        "manifests",
                        "active",
                        index + 1,
                        items.len(),
                        0,
                        None,
                        format!("Batch repository failed: {}", item.source_repository),
                        Some(error.clone()),
                        true,
                    );
                    emit_batch_sync_progress(
                        &app,
                        &operation_id,
                        &item.source_repository,
                        index + 1,
                        items.len(),
                        total.succeeded_repositories,
                        total.failed_repositories.len(),
                        "failed",
                        Some(error),
                    );
                }
            }
        }
        Ok(total)
    }
    .await;

    match result {
        Ok(result) => {
            emit_batch_sync_progress(
                &app,
                &operation_id,
                "",
                items.len(),
                items.len(),
                result.succeeded_repositories,
                result.failed_repositories.len(),
                "complete",
                None,
            );
            reporter.report(
                "complete",
                "complete",
                items.len(),
                items.len(),
                result.transferred_bytes,
                Some(result.transferred_bytes),
                format!(
                    "Batch sync complete: {} succeeded, {} failed, {} tags, {} blobs copied, {} skipped, {} cache hits, {} bytes transferred",
                    result.succeeded_repositories, result.failed_repositories.len(), result.synced_tags,
                    result.copied_blobs, result.skipped_blobs, result.cached_blobs,
                    result.transferred_bytes
                ),
                None,
                true,
            );
            Ok(result)
        }
        Err(error) => {
            reporter.fail(&error);
            Err(error)
        }
    }
}

fn proxy_summary(app: &AppHandle, registry: &RegistryConfig) -> Result<String, String> {
    let Some(proxy) = settings::resolve_proxy(app, registry)? else {
        return Ok("direct connection".into());
    };
    let parsed =
        url::Url::parse(&proxy.url).map_err(|error| app_error("invalidProxyUrl", error))?;
    let endpoint = match parsed.port_or_known_default() {
        Some(port) => format!(
            "{}://{}:{port}",
            parsed.scheme(),
            parsed.host_str().unwrap_or("?")
        ),
        None => format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or("?")),
    };
    Ok(format!("proxy {endpoint}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_registries,
            get_app_settings,
            save_app_settings,
            list_proxy_profiles,
            save_proxy_profile,
            delete_proxy_profile,
            save_registry,
            delete_registry,
            reorder_registries,
            add_repositories,
            remove_repository,
            test_connection,
            list_repositories,
            discover_repositories,
            list_tags,
            get_image_details,
            delete_manifest,
            sync_images,
            cancel_sync,
            sync_images_batch
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kaven Docker");
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;

    #[test]
    fn cancels_and_unregisters_an_active_sync() {
        let operation_id = format!("test-{}", uuid::Uuid::new_v4());
        let cancellation = SyncCancellation::new(&operation_id);

        assert!(!cancellation.token.is_cancelled());
        assert!(cancel_sync(operation_id.clone()));
        assert!(cancellation.token.is_cancelled());

        drop(cancellation);
        assert!(!cancel_sync(operation_id));
    }
}
