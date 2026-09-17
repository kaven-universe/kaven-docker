use crate::app_error;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "io.kaven.docker";
const DEFAULT_UPLOAD_CHUNK_SIZE_MB: u64 = 32;
const MIN_UPLOAD_CHUNK_SIZE_MB: u64 = 1;
const MAX_UPLOAD_CHUNK_SIZE_MB: u64 = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAppSettingsInput {
    #[serde(default)]
    pub default_proxy_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProfile {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_password: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProxyProfileInput {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
}

pub struct ResolvedProxy {
    pub url: String,
    pub username: String,
    pub password: Option<String>,
}

fn default_provider() -> String {
    "generic".into()
}

fn default_proxy_mode() -> String {
    "global".into()
}

fn default_upload_chunk_size_mb() -> u64 {
    DEFAULT_UPLOAD_CHUNK_SIZE_MB
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| app_error("configDir", error))?;
    Ok(directory.join("registries.json"))
}

fn app_settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?.with_file_name("settings.json"))
}

fn proxy_profiles_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?.with_file_name("proxies.json"))
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

pub fn load_app_settings(app: &AppHandle) -> Result<AppSettings, String> {
    let path = app_settings_path(app)?;
    let mut settings = if path.exists() {
        let content =
            fs::read_to_string(path).map_err(|error| app_error("appSettingsRead", error))?;
        serde_json::from_str(&content).map_err(|error| app_error("appSettingsFormat", error))?
    } else {
        AppSettings::default()
    };
    settings.proxy_has_password = get_password(global_proxy_password_key())?.is_some();
    Ok(settings)
}

pub fn save_app_settings(
    app: &AppHandle,
    input: SaveAppSettingsInput,
) -> Result<AppSettings, String> {
    if let Some(id) = input.default_proxy_id.as_deref() {
        find_proxy_profile(app, id)?;
    }
    let mut settings = load_app_settings(app)?;
    settings.default_proxy_id = input.default_proxy_id;
    settings.proxy_enabled = false;
    let mut stored = settings.clone();
    stored.proxy_has_password = false;
    let content = serde_json::to_string_pretty(&stored)
        .map_err(|error| app_error("appSettingsSave", error))?;
    fs::write(app_settings_path(app)?, content)
        .map_err(|error| app_error("appSettingsSave", error))?;
    Ok(settings)
}

pub fn load_proxy_profiles(app: &AppHandle) -> Result<Vec<ProxyProfile>, String> {
    let path = proxy_profiles_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|error| app_error("proxiesRead", error))?;
    let mut profiles: Vec<ProxyProfile> =
        serde_json::from_str(&content).map_err(|error| app_error("proxiesFormat", error))?;
    for profile in &mut profiles {
        profile.has_password = get_password(&profile_password_key(&profile.id))?.is_some();
    }
    profiles.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(profiles)
}

fn write_proxy_profiles(app: &AppHandle, profiles: &[ProxyProfile]) -> Result<(), String> {
    let mut stored = profiles.to_vec();
    for profile in &mut stored {
        profile.has_password = false;
    }
    let content =
        serde_json::to_string_pretty(&stored).map_err(|error| app_error("proxiesSave", error))?;
    fs::write(proxy_profiles_path(app)?, content).map_err(|error| app_error("proxiesSave", error))
}

pub fn save_proxy_profile(
    app: &AppHandle,
    input: SaveProxyProfileInput,
) -> Result<ProxyProfile, String> {
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
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|error| app_error("settingsRead", error))?;
    let mut registries: Vec<RegistryConfig> =
        serde_json::from_str(&content).map_err(|error| app_error("settingsFormat", error))?;
    for registry in &mut registries {
        registry.has_password = get_password(&registry.id)?.is_some();
        registry.proxy_has_password = get_password(&proxy_password_key(&registry.id))?.is_some();
    }
    Ok(registries)
}

fn write_registries(app: &AppHandle, registries: &[RegistryConfig]) -> Result<(), String> {
    let path = settings_path(app)?;
    let mut stored = registries.to_vec();
    for registry in &mut stored {
        registry.has_password = false;
        registry.proxy_has_password = false;
    }
    let content =
        serde_json::to_string_pretty(&stored).map_err(|error| app_error("settingsSave", error))?;
    fs::write(path, content).map_err(|error| app_error("settingsSave", error))
}

pub fn save_registry(app: &AppHandle, input: SaveRegistryInput) -> Result<RegistryConfig, String> {
    let parsed = url::Url::parse(&input.url).map_err(|_| app_error("invalidUrl", ""))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(app_error("scheme", ""));
    }
    if parsed.scheme() == "http" && !input.allow_http {
        return Err(app_error("httpDisabled", ""));
    }
    if input.name.trim().is_empty() || parsed.host_str().is_none() {
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
    if input.proxy_mode != "custom" {
        delete_password(&proxy_key, "proxyCredentialsClear")?;
    } else if proxy_username.is_empty() {
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
}
