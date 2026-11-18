use crate::{app_error, storage};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
use tauri::AppHandle;

const KEYRING_SERVICE: &str = "kaven-docker";
const DEFAULT_UPLOAD_CHUNK_SIZE_MB: u64 = 32;
const MIN_UPLOAD_CHUNK_SIZE_MB: u64 = 1;
const MAX_UPLOAD_CHUNK_SIZE_MB: u64 = 256;
const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;
const MAX_CONFIGURATION_BUNDLE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PROXY_PROFILES: usize = 1_000;
const MAX_REGISTRIES: usize = 1_000;
const MAX_REPOSITORIES_PER_REGISTRY: usize = 10_000;
const CONFIG_SCHEMA_VERSION: u32 = 1;
const MIGRATION_JOURNAL_VERSION: u32 = 1;
static SETTINGS_WRITE_LOCK: Mutex<()> = Mutex::new(());

fn acquire_settings_write_lock() -> Result<MutexGuard<'static, ()>, String> {
    SETTINGS_WRITE_LOCK
        .lock()
        .map_err(|_| app_error("settingsLock", "settings write lock is poisoned"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub auth_type: String,
    pub allow_http: bool,
    pub skip_tls_verify: bool,
    pub ca_cert_path: Option<String>,
    pub timeout_secs: u64,
    #[serde(default = "default_upload_chunk_size_mb")]
    pub upload_chunk_size_mb: u64,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub repositories: Vec<String>,
    #[serde(default = "default_proxy_mode")]
    pub proxy_mode: String,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub proxy_username: String,
    #[serde(default)]
    pub proxy_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proxy_has_password: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_password: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRegistryInput {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub allow_http: bool,
    pub skip_tls_verify: bool,
    pub ca_cert_path: Option<String>,
    pub timeout_secs: u64,
    #[serde(default = "default_upload_chunk_size_mb")]
    pub upload_chunk_size_mb: u64,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub repositories: Vec<String>,
    #[serde(default = "default_proxy_mode")]
    pub proxy_mode: String,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub proxy_username: String,
    #[serde(default)]
    pub proxy_id: Option<String>,
    #[serde(default)]
    pub proxy_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSettings {
    #[serde(default)]
    pub default_proxy_id: Option<String>,
    #[serde(default)]
    pub proxy_enabled: bool,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub proxy_username: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proxy_has_password: bool,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default = "default_automatic_update_check")]
    pub automatic_update_check: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveAppSettingsInput {
    #[serde(default)]
    pub default_proxy_id: Option<String>,
    pub locale: String,
    #[serde(default = "default_automatic_update_check")]
    pub automatic_update_check: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyProfile {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_password: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthoritativeConfig {
    schema_version: u32,
    app_settings: AppSettings,
    proxy_profiles: Vec<ProxyProfile>,
    registries: Vec<RegistryConfig>,
}

impl Default for AuthoritativeConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            app_settings: AppSettings::default(),
            proxy_profiles: Vec::new(),
            registries: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigurationMigrationJournal {
    schema_version: u32,
    phase: String,
    configuration: AuthoritativeConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveProxyProfileInput {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigurationBundle {
    schema_version: u32,
    #[serde(alias = "app")]
    app_id: String,
    #[serde(default = "default_bundle_kind")]
    bundle_kind: String,
    app_settings: AppSettings,
    proxy_profiles: Vec<ProxyProfile>,
    registries: Vec<RegistryConfig>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationTransferSummary {
    pub registries: usize,
    pub proxy_profiles: usize,
}

pub struct ResolvedProxy {
    pub url: String,
    pub username: String,
    pub password: Option<String>,
}

fn default_provider() -> String {
    "generic".into()
}

fn default_locale() -> String {
    "zh-CN".into()
}

fn default_automatic_update_check() -> bool {
    true
}

fn default_bundle_kind() -> String {
    "configuration".into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_proxy_id: None,
            proxy_enabled: false,
            proxy_url: String::new(),
            proxy_username: String::new(),
            proxy_has_password: false,
            locale: default_locale(),
            automatic_update_check: default_automatic_update_check(),
        }
    }
}

fn default_proxy_mode() -> String {
    "global".into()
}

fn default_upload_chunk_size_mb() -> u64 {
    DEFAULT_UPLOAD_CHUNK_SIZE_MB
}

fn validate_locale(locale: &str) -> Result<(), String> {
    if matches!(locale, "zh-CN" | "en") {
        Ok(())
    } else {
        Err(app_error("invalidLocale", locale))
    }
}

fn serialize_json<T: Serialize>(value: &T, error_code: &str) -> Result<Vec<u8>, String> {
    let mut content =
        serde_json::to_vec_pretty(value).map_err(|error| app_error(error_code, error))?;
    content.push(b'\n');
    Ok(content)
}

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
    error_code: &str,
) -> Result<(), String> {
    let content = serialize_json(value, error_code)?;
    storage::write_atomic(path, &content).map_err(|error| app_error(error_code, error))
}

fn read_json<T: DeserializeOwned>(
    path: &std::path::Path,
    max_bytes: u64,
    read_error: &str,
    format_error: &str,
) -> Result<T, String> {
    let metadata = fs::metadata(path).map_err(|error| app_error(read_error, error))?;
    if metadata.len() > max_bytes {
        return Err(app_error(format_error, "file exceeds the size limit"));
    }
    let content = fs::read_to_string(path).map_err(|error| app_error(read_error, error))?;
    serde_json::from_str(&content).map_err(|error| app_error(format_error, error))
}

fn configuration_root(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = storage::root(app).map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| app_error("configDir", error))?;
    Ok(directory)
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(configuration_root(app)?.join("config.json"))
}

fn load_configuration(app: &AppHandle) -> Result<AuthoritativeConfig, String> {
    let mut configuration = read_json(
        &config_path(app)?,
        MAX_SETTINGS_BYTES,
        "configRead",
        "configFormat",
    )?;
    validate_authoritative_configuration(&mut configuration)?;
    Ok(configuration)
}

fn write_configuration(
    app: &AppHandle,
    configuration: &AuthoritativeConfig,
    error_code: &str,
) -> Result<(), String> {
    write_authoritative_configuration(&config_path(app)?, configuration, error_code)
}

fn write_authoritative_configuration(
    path: &Path,
    configuration: &AuthoritativeConfig,
    error_code: &str,
) -> Result<(), String> {
    let content = serialize_json(configuration, error_code)?;
    if content.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(app_error(
            error_code,
            "configuration exceeds the size limit",
        ));
    }
    storage::write_atomic(path, &content).map_err(|error| app_error(error_code, error))
}

fn migration_journal_path(root: &Path) -> PathBuf {
    root.join("recovery").join("config-v1-migration.json")
}

fn legacy_backup_directory(root: &Path) -> PathBuf {
    root.join("recovery").join("config-v0-backup")
}

fn legacy_configuration_paths(root: &Path) -> [PathBuf; 3] {
    [
        root.join("settings.json"),
        root.join("proxies.json"),
        root.join("registries.json"),
    ]
}

fn read_legacy_configuration(root: &Path) -> Result<AuthoritativeConfig, String> {
    let [settings, proxies, registries] = legacy_configuration_paths(root);
    let mut configuration = AuthoritativeConfig {
        schema_version: CONFIG_SCHEMA_VERSION,
        app_settings: if settings.exists() {
            read_json(
                &settings,
                MAX_SETTINGS_BYTES,
                "appSettingsRead",
                "appSettingsFormat",
            )?
        } else {
            AppSettings::default()
        },
        proxy_profiles: if proxies.exists() {
            read_json(&proxies, MAX_SETTINGS_BYTES, "proxiesRead", "proxiesFormat")?
        } else {
            Vec::new()
        },
        registries: if registries.exists() {
            read_json(
                &registries,
                MAX_SETTINGS_BYTES,
                "settingsRead",
                "settingsFormat",
            )?
        } else {
            Vec::new()
        },
    };
    validate_authoritative_configuration(&mut configuration)?;
    Ok(configuration)
}

fn archive_legacy_configuration(root: &Path) -> Result<(), String> {
    let backup = legacy_backup_directory(root);
    for source in legacy_configuration_paths(root) {
        if !source.exists() {
            continue;
        }
        fs::create_dir_all(&backup).map_err(|error| app_error("configMigration", error))?;
        let destination = backup.join(
            source
                .file_name()
                .ok_or_else(|| app_error("configMigration", "invalid legacy path"))?,
        );
        if destination.exists() {
            let source_content =
                fs::read(&source).map_err(|error| app_error("configMigration", error))?;
            let destination_content =
                fs::read(&destination).map_err(|error| app_error("configMigration", error))?;
            if source_content != destination_content {
                return Err(app_error(
                    "configMigration",
                    "legacy backup conflicts with the source file",
                ));
            }
            fs::remove_file(&source).map_err(|error| app_error("configMigration", error))?;
        } else {
            fs::rename(&source, destination)
                .map_err(|error| app_error("configMigration", error))?;
        }
    }
    Ok(())
}

fn complete_configuration_migration(
    root: &Path,
    mut journal: ConfigurationMigrationJournal,
    error_code: &str,
) -> Result<(), String> {
    if journal.schema_version != MIGRATION_JOURNAL_VERSION
        || !matches!(journal.phase.as_str(), "prepared" | "applied" | "completed")
    {
        return Err(app_error(error_code, "unsupported migration journal"));
    }
    validate_authoritative_configuration(&mut journal.configuration)?;

    let config = root.join("config.json");
    let journal_path = migration_journal_path(root);
    write_authoritative_configuration(&config, &journal.configuration, error_code)?;
    journal.phase = "applied".into();
    write_json(&journal_path, &journal, error_code)?;

    let mut verified: AuthoritativeConfig =
        read_json(&config, MAX_SETTINGS_BYTES, error_code, error_code)?;
    validate_authoritative_configuration(&mut verified)?;
    archive_legacy_configuration(root)?;

    journal.phase = "completed".into();
    write_json(&journal_path, &journal, error_code)?;
    fs::remove_file(&journal_path).map_err(|error| app_error(error_code, error))?;
    Ok(())
}

fn initialize_configuration_root(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|error| app_error("configDir", error))?;
    let config = root.join("config.json");
    let journal_path = migration_journal_path(root);

    if journal_path.exists() {
        let journal: ConfigurationMigrationJournal = read_json(
            &journal_path,
            MAX_CONFIGURATION_BUNDLE_BYTES,
            "configMigrationRecovery",
            "configMigrationRecovery",
        )?;
        return complete_configuration_migration(root, journal, "configMigrationRecovery");
    }

    if config.exists() {
        let mut configuration: AuthoritativeConfig =
            read_json(&config, MAX_SETTINGS_BYTES, "configRead", "configFormat")?;
        validate_authoritative_configuration(&mut configuration)?;
        archive_legacy_configuration(root)?;
        return Ok(());
    }

    let has_legacy_configuration = legacy_configuration_paths(root)
        .iter()
        .any(|path| path.exists());
    let configuration = if has_legacy_configuration {
        read_legacy_configuration(root)?
    } else {
        AuthoritativeConfig::default()
    };
    let journal = ConfigurationMigrationJournal {
        schema_version: MIGRATION_JOURNAL_VERSION,
        phase: "prepared".into(),
        configuration: configuration.clone(),
    };
    write_json(&journal_path, &journal, "configMigration")?;
    complete_configuration_migration(root, journal, "configMigration")
}

pub fn initialize_configuration(app: &AppHandle) -> Result<(), String> {
    let _write_guard = acquire_settings_write_lock()?;
    initialize_configuration_root(&configuration_root(app)?)
}

fn password_entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, id).map_err(|error| app_error("keyring", error))
}

fn proxy_password_key(id: &str) -> String {
    format!("{id}:proxy")
}

fn global_proxy_password_key() -> &'static str {
    "__global_proxy__"
}

fn profile_password_key(id: &str) -> String {
    format!("proxy-profile:{id}")
}

pub fn get_password(id: &str) -> Result<Option<String>, String> {
    match password_entry(id)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(app_error("credentialsRead", error)),
    }
}

fn delete_password(id: &str, error_code: &str) -> Result<(), String> {
    match password_entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(app_error(error_code, error)),
    }
}

fn validate_proxy_url(value: &str) -> Result<String, String> {
    let parsed = url::Url::parse(value).map_err(|_| app_error("invalidProxyUrl", ""))?;
    if !matches!(
        parsed.scheme(),
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
    ) || parsed.host_str().is_none()
    {
        return Err(app_error("invalidProxyUrl", ""));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(app_error("proxyCredentialsInUrl", ""));
    }
    Ok(value.trim_end_matches('/').to_string())
}

fn validate_proxy_auth(url: &str, username: &str) -> Result<(), String> {
    let scheme = url::Url::parse(url)
        .map_err(|_| app_error("invalidProxyUrl", ""))?
        .scheme()
        .to_string();
    if !username.trim().is_empty() && matches!(scheme.as_str(), "socks4" | "socks4a") {
        return Err(app_error("proxyAuthUnsupported", ""));
    }
    Ok(())
}

fn has_embedded_credentials(url: &url::Url) -> bool {
    !url.username().is_empty() || url.password().is_some()
}

pub fn load_app_settings(app: &AppHandle) -> Result<AppSettings, String> {
    let mut settings = load_configuration(app)?.app_settings;
    settings.proxy_has_password = get_password(global_proxy_password_key())?.is_some();
    Ok(settings)
}

pub fn save_app_settings(
    app: &AppHandle,
    input: SaveAppSettingsInput,
) -> Result<AppSettings, String> {
    let _write_guard = acquire_settings_write_lock()?;
    validate_locale(&input.locale)?;
    if let Some(id) = input.default_proxy_id.as_deref() {
        find_proxy_profile(app, id)?;
    }
    let mut configuration = load_configuration(app)?;
    configuration.app_settings.default_proxy_id = input.default_proxy_id;
    configuration.app_settings.locale = input.locale;
    configuration.app_settings.automatic_update_check = input.automatic_update_check;
    configuration.app_settings.proxy_enabled = false;
    configuration.app_settings.proxy_has_password = false;
    write_configuration(app, &configuration, "appSettingsSave")?;
    load_app_settings(app)
}

pub fn load_proxy_profiles(app: &AppHandle) -> Result<Vec<ProxyProfile>, String> {
    let mut profiles = load_configuration(app)?.proxy_profiles;
    for profile in &mut profiles {
        profile.has_password = get_password(&profile_password_key(&profile.id))?.is_some();
    }
    profiles.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(profiles)
}

fn write_proxy_profiles(app: &AppHandle, profiles: &[ProxyProfile]) -> Result<(), String> {
    let mut configuration = load_configuration(app)?;
    let mut stored = profiles.to_vec();
    for profile in &mut stored {
        profile.has_password = false;
    }
    configuration.proxy_profiles = stored;
    write_configuration(app, &configuration, "proxiesSave")
}

pub fn save_proxy_profile(
    app: &AppHandle,
    input: SaveProxyProfileInput,
) -> Result<ProxyProfile, String> {
    let _write_guard = acquire_settings_write_lock()?;
    if input.name.trim().is_empty() {
        return Err(app_error("invalidProxyName", ""));
    }
    let id = if input.id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        input.id.clone()
    };
    let url = validate_proxy_url(input.url.trim())?;
    let username = input.username.trim().to_string();
    validate_proxy_auth(&url, &username)?;
    let password_key = profile_password_key(&id);
    if username.is_empty() {
        delete_password(&password_key, "proxyCredentialsClear")?;
    } else if let Some(password) = input.password.filter(|value| !value.is_empty()) {
        password_entry(&password_key)?
            .set_password(&password)
            .map_err(|error| app_error("proxyCredentialsSave", error))?;
    }
    let profile = ProxyProfile {
        id: id.clone(),
        name: input.name.trim().to_string(),
        url,
        username,
        has_password: get_password(&password_key)?.is_some(),
    };
    let mut profiles = load_proxy_profiles(app)?;
    if let Some(existing) = profiles.iter_mut().find(|item| item.id == id) {
        *existing = profile.clone();
    } else {
        profiles.push(profile.clone());
    }
    write_proxy_profiles(app, &profiles)?;
    Ok(profile)
}

pub fn find_proxy_profile(app: &AppHandle, id: &str) -> Result<ProxyProfile, String> {
    load_proxy_profiles(app)?
        .into_iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| app_error("proxyMissing", ""))
}

pub fn delete_proxy_profile(app: &AppHandle, id: &str) -> Result<(), String> {
    let _write_guard = acquire_settings_write_lock()?;
    let settings = load_app_settings(app)?;
    let used_by_registry = load_registries(app)?.iter().any(|registry| {
        registry.proxy_mode == "profile" && registry.proxy_id.as_deref() == Some(id)
    });
    if settings.default_proxy_id.as_deref() == Some(id) || used_by_registry {
        return Err(app_error("proxyInUse", ""));
    }
    let mut profiles = load_proxy_profiles(app)?;
    if !profiles.iter().any(|profile| profile.id == id) {
        return Err(app_error("proxyMissing", ""));
    }
    profiles.retain(|profile| profile.id != id);
    write_proxy_profiles(app, &profiles)?;
    delete_password(&profile_password_key(id), "proxyCredentialsDelete")
}

pub fn load_registries(app: &AppHandle) -> Result<Vec<RegistryConfig>, String> {
    let mut registries = load_configuration(app)?.registries;
    for registry in &mut registries {
        registry.has_password = get_password(&registry.id)?.is_some();
        registry.proxy_has_password = get_password(&proxy_password_key(&registry.id))?.is_some();
    }
    Ok(registries)
}

fn write_registries(app: &AppHandle, registries: &[RegistryConfig]) -> Result<(), String> {
    let mut configuration = load_configuration(app)?;
    let mut stored = registries.to_vec();
    for registry in &mut stored {
        registry.has_password = false;
        registry.proxy_has_password = false;
    }
    configuration.registries = stored;
    write_configuration(app, &configuration, "settingsSave")
}

pub fn export_configuration(
    app: &AppHandle,
    path: &str,
) -> Result<ConfigurationTransferSummary, String> {
    let _write_guard = acquire_settings_write_lock()?;
    let AuthoritativeConfig {
        mut app_settings,
        mut proxy_profiles,
        mut registries,
        ..
    } = load_configuration(app)?;
    app_settings.proxy_has_password = false;
    for profile in &mut proxy_profiles {
        profile.has_password = false;
    }
    for registry in &mut registries {
        registry.has_password = false;
        registry.proxy_has_password = false;
    }
    let summary = ConfigurationTransferSummary {
        registries: registries.len(),
        proxy_profiles: proxy_profiles.len(),
    };
    let bundle = ConfigurationBundle {
        schema_version: 1,
        app_id: "kaven-docker".into(),
        bundle_kind: default_bundle_kind(),
        app_settings,
        proxy_profiles,
        registries,
    };
    write_json(&PathBuf::from(path), &bundle, "configExportWrite")?;
    Ok(summary)
}

pub fn import_configuration(
    app: &AppHandle,
    path: &str,
) -> Result<ConfigurationTransferSummary, String> {
    let _write_guard = acquire_settings_write_lock()?;
    let mut bundle: ConfigurationBundle = read_json(
        &PathBuf::from(path),
        MAX_CONFIGURATION_BUNDLE_BYTES,
        "configImportRead",
        "configImportFormat",
    )?;
    validate_configuration_bundle(&mut bundle)?;
    let configuration = AuthoritativeConfig {
        schema_version: CONFIG_SCHEMA_VERSION,
        app_settings: bundle.app_settings,
        proxy_profiles: bundle.proxy_profiles,
        registries: bundle.registries,
    };
    write_configuration(app, &configuration, "configImportSave")?;

    Ok(ConfigurationTransferSummary {
        registries: configuration.registries.len(),
        proxy_profiles: configuration.proxy_profiles.len(),
    })
}

fn validate_configuration_bundle(bundle: &mut ConfigurationBundle) -> Result<(), String> {
    if bundle.schema_version != 1 {
        return Err(app_error("configImportVersion", bundle.schema_version));
    }
    if bundle.app_id != "kaven-docker" || bundle.bundle_kind != "configuration" {
        return Err(app_error("configImportApp", &bundle.app_id));
    }
    validate_locale(&bundle.app_settings.locale)
        .map_err(|_| app_error("configImportFormat", "invalid locale"))?;
    if bundle.proxy_profiles.len() > MAX_PROXY_PROFILES
        || bundle.registries.len() > MAX_REGISTRIES
        || bundle
            .registries
            .iter()
            .any(|registry| registry.repositories.len() > MAX_REPOSITORIES_PER_REGISTRY)
    {
        return Err(app_error(
            "configImportFormat",
            "configuration limit exceeded",
        ));
    }

    let mut proxy_ids = HashSet::new();
    for profile in &mut bundle.proxy_profiles {
        if profile.id.trim().is_empty()
            || !proxy_ids.insert(profile.id.clone())
            || profile.name.trim().is_empty()
        {
            return Err(app_error("configImportFormat", "invalid proxy profile"));
        }
        profile.name = profile.name.trim().to_string();
        profile.url = validate_proxy_url(profile.url.trim())?;
        profile.username = profile.username.trim().to_string();
        validate_proxy_auth(&profile.url, &profile.username)?;
        profile.has_password = false;
    }

    if let Some(default_proxy_id) = bundle.app_settings.default_proxy_id.as_deref() {
        if !proxy_ids.contains(default_proxy_id) {
            return Err(app_error(
                "configImportFormat",
                "missing default proxy profile",
            ));
        }
    }
    bundle.app_settings.proxy_has_password = false;

    let mut registry_ids = HashSet::new();
    for registry in &mut bundle.registries {
        let parsed = url::Url::parse(&registry.url)
            .map_err(|_| app_error("configImportFormat", "invalid Registry URL"))?;
        if registry.id.trim().is_empty()
            || !registry_ids.insert(registry.id.clone())
            || registry.name.trim().is_empty()
            || parsed.host_str().is_none()
            || has_embedded_credentials(&parsed)
            || !matches!(parsed.scheme(), "http" | "https")
            || (parsed.scheme() == "http" && !registry.allow_http)
            || !matches!(registry.auth_type.as_str(), "anonymous" | "basic")
            || !matches!(
                registry.proxy_mode.as_str(),
                "global" | "direct" | "profile" | "custom"
            )
        {
            return Err(app_error(
                "configImportFormat",
                "invalid Registry configuration",
            ));
        }
        if registry.proxy_mode == "profile"
            && !registry
                .proxy_id
                .as_deref()
                .is_some_and(|id| proxy_ids.contains(id))
        {
            return Err(app_error(
                "configImportFormat",
                "missing Registry proxy profile",
            ));
        }
        if registry.proxy_mode == "custom" {
            registry.proxy_url = validate_proxy_url(registry.proxy_url.trim())?;
            registry.proxy_username = registry.proxy_username.trim().to_string();
            validate_proxy_auth(&registry.proxy_url, &registry.proxy_username)?;
        } else {
            registry.proxy_url.clear();
            registry.proxy_username.clear();
            if registry.proxy_mode != "profile" {
                registry.proxy_id = None;
            }
        }
        registry.name = registry.name.trim().to_string();
        registry.url = registry.url.trim_end_matches('/').to_string();
        registry.username = registry.username.trim().to_string();
        registry.timeout_secs = registry.timeout_secs.clamp(3, 120);
        registry.upload_chunk_size_mb = registry
            .upload_chunk_size_mb
            .clamp(MIN_UPLOAD_CHUNK_SIZE_MB, MAX_UPLOAD_CHUNK_SIZE_MB);
        registry.has_password = false;
        registry.proxy_has_password = false;
    }
    Ok(())
}

fn validate_authoritative_configuration(
    configuration: &mut AuthoritativeConfig,
) -> Result<(), String> {
    if configuration.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(app_error("configVersion", configuration.schema_version));
    }
    let mut bundle = ConfigurationBundle {
        schema_version: configuration.schema_version,
        app_id: "kaven-docker".into(),
        bundle_kind: default_bundle_kind(),
        app_settings: configuration.app_settings.clone(),
        proxy_profiles: configuration.proxy_profiles.clone(),
        registries: configuration.registries.clone(),
    };
    validate_configuration_bundle(&mut bundle)?;
    configuration.app_settings = bundle.app_settings;
    configuration.proxy_profiles = bundle.proxy_profiles;
    configuration.registries = bundle.registries;
    Ok(())
}

pub fn save_registry(app: &AppHandle, input: SaveRegistryInput) -> Result<RegistryConfig, String> {
    let _write_guard = acquire_settings_write_lock()?;
    let parsed = url::Url::parse(&input.url).map_err(|_| app_error("invalidUrl", ""))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(app_error("scheme", ""));
    }
    if parsed.scheme() == "http" && !input.allow_http {
        return Err(app_error("httpDisabled", ""));
    }
    if input.name.trim().is_empty()
        || parsed.host_str().is_none()
        || has_embedded_credentials(&parsed)
    {
        return Err(app_error("invalidRegistry", ""));
    }
    if !matches!(
        input.proxy_mode.as_str(),
        "global" | "direct" | "profile" | "custom"
    ) {
        return Err(app_error("invalidProxyMode", ""));
    }
    let proxy_id = if input.proxy_mode == "profile" {
        let id = input
            .proxy_id
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| app_error("proxyMissing", ""))?;
        find_proxy_profile(app, id)?;
        Some(id.to_string())
    } else {
        None
    };
    let (proxy_url, proxy_username) = if input.proxy_mode == "custom" {
        let proxy_url = validate_proxy_url(input.proxy_url.trim())?;
        let proxy_username = input.proxy_username.trim().to_string();
        validate_proxy_auth(&proxy_url, &proxy_username)?;
        (proxy_url, proxy_username)
    } else {
        (String::new(), String::new())
    };

    let id = if input.id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        input.id.clone()
    };
    if input.auth_type == "anonymous" {
        match password_entry(&id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(app_error("credentialsClear", error)),
        }
    } else if let Some(password) = input.password.filter(|value| !value.is_empty()) {
        password_entry(&id)?
            .set_password(&password)
            .map_err(|error| app_error("credentialsSave", error))?;
    }
    let proxy_key = proxy_password_key(&id);
    if input.proxy_mode != "custom" || proxy_username.is_empty() {
        delete_password(&proxy_key, "proxyCredentialsClear")?;
    } else if let Some(password) = input.proxy_password.filter(|value| !value.is_empty()) {
        password_entry(&proxy_key)?
            .set_password(&password)
            .map_err(|error| app_error("proxyCredentialsSave", error))?;
    }
    let has_password = get_password(&id)?.is_some();
    let proxy_has_password = get_password(&proxy_key)?.is_some();
    let registry = RegistryConfig {
        id: id.clone(),
        name: input.name.trim().to_string(),
        url: input.url.trim_end_matches('/').to_string(),
        username: input.username.trim().to_string(),
        auth_type: input.auth_type,
        allow_http: input.allow_http,
        skip_tls_verify: input.skip_tls_verify,
        ca_cert_path: input.ca_cert_path.filter(|value| !value.trim().is_empty()),
        timeout_secs: input.timeout_secs.clamp(3, 120),
        upload_chunk_size_mb: input
            .upload_chunk_size_mb
            .clamp(MIN_UPLOAD_CHUNK_SIZE_MB, MAX_UPLOAD_CHUNK_SIZE_MB),
        provider: input.provider,
        repositories: input.repositories,
        proxy_mode: input.proxy_mode,
        proxy_url,
        proxy_username,
        proxy_id,
        proxy_has_password,
        has_password,
    };
    let mut registries = load_registries(app)?;
    if let Some(existing) = registries.iter_mut().find(|item| item.id == id) {
        *existing = registry.clone();
    } else {
        registries.push(registry.clone());
    }
    write_registries(app, &registries)?;
    Ok(registry)
}

pub fn delete_registry(app: &AppHandle, id: &str) -> Result<(), String> {
    let _write_guard = acquire_settings_write_lock()?;
    let mut registries = load_registries(app)?;
    registries.retain(|item| item.id != id);
    write_registries(app, &registries)?;
    delete_password(id, "credentialsDelete")?;
    delete_password(&proxy_password_key(id), "proxyCredentialsDelete")
}

pub fn reorder_registries(
    app: &AppHandle,
    registry_ids: &[String],
) -> Result<Vec<RegistryConfig>, String> {
    let _write_guard = acquire_settings_write_lock()?;
    let mut remaining = load_registries(app)?;
    if registry_ids.len() != remaining.len() {
        return Err(app_error("invalidRegistryOrder", ""));
    }
    let mut ordered = Vec::with_capacity(remaining.len());
    for id in registry_ids {
        let Some(index) = remaining.iter().position(|registry| registry.id == *id) else {
            return Err(app_error("invalidRegistryOrder", ""));
        };
        ordered.push(remaining.remove(index));
    }
    if !remaining.is_empty() {
        return Err(app_error("invalidRegistryOrder", ""));
    }
    write_registries(app, &ordered)?;
    Ok(ordered)
}

pub fn find_registry(app: &AppHandle, id: &str) -> Result<RegistryConfig, String> {
    load_registries(app)?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| app_error("registryMissing", ""))
}

pub fn resolve_proxy(
    app: &AppHandle,
    registry: &RegistryConfig,
) -> Result<Option<ResolvedProxy>, String> {
    match registry.proxy_mode.as_str() {
        "direct" => Ok(None),
        "profile" => {
            let id = registry
                .proxy_id
                .as_deref()
                .ok_or_else(|| app_error("proxyMissing", ""))?;
            let profile = find_proxy_profile(app, id)?;
            Ok(Some(ResolvedProxy {
                url: profile.url,
                username: profile.username,
                password: get_password(&profile_password_key(id))?,
            }))
        }
        "custom" => Ok(Some(ResolvedProxy {
            url: registry.proxy_url.clone(),
            username: registry.proxy_username.clone(),
            password: get_password(&proxy_password_key(&registry.id))?,
        })),
        _ => {
            let settings = load_app_settings(app)?;
            if let Some(id) = settings.default_proxy_id.as_deref() {
                let profile = find_proxy_profile(app, id)?;
                return Ok(Some(ResolvedProxy {
                    url: profile.url,
                    username: profile.username,
                    password: get_password(&profile_password_key(id))?,
                }));
            }
            if !settings.proxy_enabled {
                return Ok(None);
            }
            Ok(Some(ResolvedProxy {
                url: settings.proxy_url,
                username: settings.proxy_username,
                password: get_password(global_proxy_password_key())?,
            }))
        }
    }
}

fn normalize_repository(repository: &str) -> Result<String, String> {
    let repository = repository.trim().trim_matches('/');
    if repository.is_empty() || repository.contains(':') || repository.contains('@') {
        return Err(app_error("invalidRepository", ""));
    }
    Ok(repository.to_string())
}

pub fn add_repositories(
    app: &AppHandle,
    id: &str,
    repositories: &[String],
) -> Result<Vec<String>, String> {
    let _write_guard = acquire_settings_write_lock()?;
    if repositories.is_empty() {
        return Err(app_error("invalidRepository", ""));
    }
    let mut normalized = repositories
        .iter()
        .map(|repository| normalize_repository(repository))
        .collect::<Result<Vec<_>, _>>()?;
    normalized.sort();
    normalized.dedup();

    let mut registries = load_registries(app)?;
    let registry = registries
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| app_error("registryMissing", ""))?;
    for repository in &normalized {
        if !registry.repositories.contains(repository) {
            registry.repositories.push(repository.clone());
        }
    }
    registry.repositories.sort();
    registry.repositories.dedup();
    write_registries(app, &registries)?;
    Ok(normalized)
}

pub fn remove_repository(app: &AppHandle, id: &str, repository: &str) -> Result<(), String> {
    let _write_guard = acquire_settings_write_lock()?;
    let repository = normalize_repository(repository)?;
    let mut registries = load_registries(app)?;
    let registry = registries
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| app_error("registryMissing", ""))?;
    registry.repositories.retain(|item| item != &repository);
    write_registries(app, &registries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_configuration_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("kaven-docker-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("test configuration root should be created");
        root
    }

    #[test]
    fn normalizes_repository_names() {
        assert_eq!(
            normalize_repository("/owner/image/").unwrap(),
            "owner/image"
        );
        assert!(normalize_repository("owner/image:latest").is_err());
        assert!(normalize_repository("owner/image@sha256:abc").is_err());
        assert!(normalize_repository(" / ").is_err());
    }

    #[test]
    fn validates_supported_locales() {
        assert!(validate_locale("zh-CN").is_ok());
        assert!(validate_locale("en").is_ok());
        assert!(validate_locale("fr").is_err());
        assert!(validate_locale("").is_err());
    }

    #[test]
    fn rejects_unknown_configuration_bundle_fields() {
        let bundle = serde_json::json!({
            "schemaVersion": 1,
            "appId": "kaven-docker",
            "bundleKind": "configuration",
            "appSettings": { "defaultProxyId": null, "locale": "en" },
            "proxyProfiles": [],
            "registries": [],
            "unexpected": true
        });

        assert!(serde_json::from_value::<ConfigurationBundle>(bundle).is_err());
    }

    #[test]
    fn accepts_the_legacy_configuration_bundle_identity() {
        let bundle = serde_json::json!({
            "schemaVersion": 1,
            "app": "kaven-docker",
            "appSettings": { "defaultProxyId": null },
            "proxyProfiles": [],
            "registries": []
        });

        let parsed = serde_json::from_value::<ConfigurationBundle>(bundle)
            .expect("legacy configuration should remain importable");
        assert_eq!(parsed.app_id, "kaven-docker");
        assert_eq!(parsed.bundle_kind, "configuration");
        assert_eq!(parsed.app_settings.locale, "zh-CN");
        assert!(parsed.app_settings.automatic_update_check);
    }

    #[test]
    fn validates_supported_proxy_urls_without_embedded_credentials() {
        assert!(validate_proxy_url("http://127.0.0.1:7890").is_ok());
        assert!(validate_proxy_url("https://proxy.example.com").is_ok());
        assert!(validate_proxy_url("socks4://127.0.0.1:1080").is_ok());
        assert!(validate_proxy_url("socks4a://proxy.example.com:1080").is_ok());
        assert!(validate_proxy_url("socks5://127.0.0.1:1080").is_ok());
        assert!(validate_proxy_url("socks5h://proxy.example.com:1080").is_ok());
        assert!(validate_proxy_url("ftp://proxy.example.com").is_err());
        assert!(validate_proxy_url("http://user:secret@proxy.example.com").is_err());
        assert!(validate_proxy_url("socks5://user:secret@proxy.example.com").is_err());
    }

    #[test]
    fn detects_registry_urls_with_embedded_credentials() {
        assert!(has_embedded_credentials(
            &url::Url::parse("https://user:secret@registry.example.com")
                .expect("fixture URL should parse")
        ));
        assert!(!has_embedded_credentials(
            &url::Url::parse("https://registry.example.com").expect("fixture URL should parse")
        ));
    }

    #[test]
    fn rejects_credentials_for_socks4_proxies() {
        assert!(validate_proxy_auth("socks4://127.0.0.1:1080", "user").is_err());
        assert!(validate_proxy_auth("socks4a://proxy.example.com:1080", "user").is_err());
        assert!(validate_proxy_auth("socks5://127.0.0.1:1080", "user").is_ok());
        assert!(validate_proxy_auth("https://proxy.example.com", "user").is_ok());
    }

    #[test]
    fn reqwest_accepts_every_supported_proxy_scheme() {
        for url in [
            "http://127.0.0.1:7890",
            "https://127.0.0.1:7890",
            "socks4://127.0.0.1:1080",
            "socks4a://127.0.0.1:1080",
            "socks5://127.0.0.1:1080",
            "socks5h://127.0.0.1:1080",
        ] {
            assert!(reqwest::Proxy::all(url).is_ok(), "unsupported proxy: {url}");
        }
    }

    #[test]
    fn legacy_registry_settings_follow_the_global_proxy() {
        let registry: RegistryConfig = serde_json::from_value(serde_json::json!({
            "id": "legacy",
            "name": "Legacy Registry",
            "url": "https://registry.example.com",
            "username": "",
            "authType": "anonymous",
            "allowHttp": false,
            "skipTlsVerify": false,
            "caCertPath": null,
            "timeoutSecs": 15
        }))
        .expect("legacy Registry settings should deserialize");

        assert_eq!(registry.proxy_mode, "global");
        assert_eq!(registry.upload_chunk_size_mb, DEFAULT_UPLOAD_CHUNK_SIZE_MB);
        assert!(registry.proxy_url.is_empty());
        assert!(!registry.proxy_has_password);
    }

    #[test]
    fn migrates_legacy_files_to_one_authoritative_configuration() {
        let root = temporary_configuration_root("configuration-migration");
        let settings = AppSettings {
            locale: "en".into(),
            ..AppSettings::default()
        };
        write_json(&root.join("settings.json"), &settings, "test")
            .expect("legacy settings should be written");
        write_json(
            &root.join("proxies.json"),
            &Vec::<ProxyProfile>::new(),
            "test",
        )
        .expect("legacy proxies should be written");
        write_json(
            &root.join("registries.json"),
            &Vec::<RegistryConfig>::new(),
            "test",
        )
        .expect("legacy registries should be written");

        initialize_configuration_root(&root).expect("legacy configuration should migrate");

        let configuration: AuthoritativeConfig = read_json(
            &root.join("config.json"),
            MAX_CONFIGURATION_BUNDLE_BYTES,
            "test",
            "test",
        )
        .expect("authoritative configuration should be readable");
        assert_eq!(configuration.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(configuration.app_settings.locale, "en");
        for legacy_name in ["settings.json", "proxies.json", "registries.json"] {
            assert!(!root.join(legacy_name).exists());
            assert!(legacy_backup_directory(&root).join(legacy_name).exists());
        }
        assert!(!migration_journal_path(&root).exists());
        fs::remove_dir_all(root).expect("test root should be removable");
    }

    #[test]
    fn recovers_an_interrupted_configuration_migration_on_restart() {
        let root = temporary_configuration_root("configuration-recovery");
        let configuration = AuthoritativeConfig {
            app_settings: AppSettings {
                locale: "en".into(),
                ..AppSettings::default()
            },
            ..AuthoritativeConfig::default()
        };
        let journal = ConfigurationMigrationJournal {
            schema_version: MIGRATION_JOURNAL_VERSION,
            phase: "prepared".into(),
            configuration,
        };
        write_json(&migration_journal_path(&root), &journal, "test")
            .expect("prepared migration journal should be durable");
        write_json(&root.join("settings.json"), &AppSettings::default(), "test")
            .expect("legacy settings should be written");

        initialize_configuration_root(&root).expect("restart should recover the migration");

        let recovered: AuthoritativeConfig = read_json(
            &root.join("config.json"),
            MAX_CONFIGURATION_BUNDLE_BYTES,
            "test",
            "test",
        )
        .expect("recovered configuration should be readable");
        assert_eq!(recovered.app_settings.locale, "en");
        assert!(!migration_journal_path(&root).exists());
        assert!(legacy_backup_directory(&root)
            .join("settings.json")
            .exists());
        fs::remove_dir_all(root).expect("test root should be removable");
    }

    #[test]
    fn finishes_an_applied_migration_before_loading_configuration() {
        let root = temporary_configuration_root("configuration-applied-recovery");
        let configuration = AuthoritativeConfig {
            app_settings: AppSettings {
                locale: "en".into(),
                ..AppSettings::default()
            },
            ..AuthoritativeConfig::default()
        };
        write_authoritative_configuration(&root.join("config.json"), &configuration, "test")
            .expect("authoritative configuration should be applied");
        let journal = ConfigurationMigrationJournal {
            schema_version: MIGRATION_JOURNAL_VERSION,
            phase: "applied".into(),
            configuration,
        };
        write_json(&migration_journal_path(&root), &journal, "test")
            .expect("applied migration journal should be durable");
        write_json(&root.join("settings.json"), &AppSettings::default(), "test")
            .expect("legacy settings should remain before archival");

        initialize_configuration_root(&root).expect("restart should finish the migration");

        assert!(!migration_journal_path(&root).exists());
        assert!(!root.join("settings.json").exists());
        assert!(legacy_backup_directory(&root)
            .join("settings.json")
            .exists());
        fs::remove_dir_all(root).expect("test root should be removable");
    }

    #[test]
    fn rejects_a_newer_authoritative_configuration_without_mutation() {
        let root = temporary_configuration_root("configuration-version");
        let unsupported = serde_json::json!({
            "schemaVersion": CONFIG_SCHEMA_VERSION + 1,
            "appSettings": AppSettings::default(),
            "proxyProfiles": [],
            "registries": []
        });
        let original = serialize_json(&unsupported, "test").expect("fixture should serialize");
        storage::write_atomic(&root.join("config.json"), &original)
            .expect("unsupported configuration should be written");

        assert!(initialize_configuration_root(&root).is_err());
        assert_eq!(
            fs::read(root.join("config.json")).expect("configuration should remain readable"),
            original
        );
        fs::remove_dir_all(root).expect("test root should be removable");
    }

    #[test]
    fn rejects_an_oversized_authoritative_configuration_before_writing() {
        let root = temporary_configuration_root("configuration-size");
        let configuration = AuthoritativeConfig {
            app_settings: AppSettings {
                proxy_url: "x".repeat(MAX_SETTINGS_BYTES as usize),
                ..AppSettings::default()
            },
            ..AuthoritativeConfig::default()
        };

        assert!(write_authoritative_configuration(
            &root.join("config.json"),
            &configuration,
            "test"
        )
        .is_err());
        assert!(!root.join("config.json").exists());
        fs::remove_dir_all(root).expect("test root should be removable");
    }
}
