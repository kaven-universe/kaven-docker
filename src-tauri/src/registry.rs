use crate::{
    app_error,
    settings::{find_registry, get_password, resolve_proxy, RegistryConfig},
};
use bytes::Bytes;
use reqwest::{header, Method, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MANIFEST_ACCEPT: &str = "application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    registry_version: String,
    message: String,
}

#[derive(Serialize)]
pub struct RepositoryPage {
    repositories: Vec<String>,
}

#[derive(Serialize)]
pub struct TagList {
    name: String,
    tags: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    media_type: String,
    digest: String,
    size: u64,
}

#[derive(Serialize)]
pub struct PlatformInfo {
    architecture: String,
    os: String,
    variant: Option<String>,
    digest: String,
    size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageDetails {
    repository: String,
    reference: String,
    digest: String,
    media_type: String,
    size: u64,
    created: Option<String>,
    architecture: Option<String>,
    os: Option<String>,
    labels: BTreeMap<String, String>,
    layers: Vec<LayerInfo>,
    platforms: Vec<PlatformInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub(crate) synced_tags: usize,
    pub(crate) copied_blobs: usize,
    pub(crate) skipped_blobs: usize,
    pub(crate) cached_blobs: usize,
    pub(crate) transferred_bytes: u64,
    pub(crate) succeeded_repositories: usize,
    pub(crate) failed_repositories: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlatform {
    os: String,
    architecture: String,
    variant: Option<String>,
}

impl SyncPlatform {
    pub fn validate(mut self) -> Result<Self, String> {
        self.os = self.os.trim().to_ascii_lowercase();
        self.architecture = self.architecture.trim().to_ascii_lowercase();
        self.variant = self
            .variant
            .take()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        let valid = |value: &str| {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
        };
        if !valid(&self.os)
            || !valid(&self.architecture)
            || self.variant.as_deref().is_some_and(|value| !valid(value))
        {
            return Err(app_error("invalidSyncPlatform", self.display()));
        }
        Ok(self)
    }

    fn display(&self) -> String {
        match self.variant.as_deref() {
            Some(variant) => format!("{}/{}/{}", self.os, self.architecture, variant),
            None => format!("{}/{}", self.os, self.architecture),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    operation_id: String,
    timestamp: u64,
    stage: String,
    status: String,
    current: usize,
    total: usize,
    bytes_current: u64,
    bytes_total: Option<u64>,
    message: String,
    detail: Option<String>,
    log_path: String,
    append_log: bool,
}

#[derive(Clone)]
pub struct SyncReporter {
    app: AppHandle,
    operation_id: String,
    log_path: PathBuf,
    active: Arc<Mutex<(String, String)>>,
}

struct TempBlob {
    path: PathBuf,
}

impl Drop for TempBlob {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl SyncReporter {
    pub fn new(app: &AppHandle, operation_id: &str) -> Result<Self, String> {
        if operation_id.is_empty()
            || !operation_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err(app_error("invalidSyncOperation", ""));
        }
        let directory = app
            .path()
            .app_log_dir()
            .map_err(|error| app_error("syncLogCreate", error))?
            .join("sync");
        fs::create_dir_all(&directory).map_err(|error| app_error("syncLogCreate", error))?;
        let log_path = directory.join(format!("sync-{operation_id}.jsonl"));
        fs::write(&log_path, "").map_err(|error| app_error("syncLogCreate", error))?;
        Ok(Self {
            app: app.clone(),
            operation_id: operation_id.to_string(),
            log_path,
            active: Arc::new(Mutex::new(("prepare".into(), "Starting sync".into()))),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn report(
        &self,
        stage: &str,
        status: &str,
        current: usize,
        total: usize,
        bytes_current: u64,
        bytes_total: Option<u64>,
        message: impl Into<String>,
        detail: Option<String>,
        append_log: bool,
    ) {
        let message = message.into();
        if status == "active" {
            if let Ok(mut active) = self.active.lock() {
                *active = (stage.to_string(), message.clone());
            }
        }
        let event = SyncProgress {
            operation_id: self.operation_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            stage: stage.to_string(),
            status: status.to_string(),
            current,
            total,
            bytes_current,
            bytes_total,
            message,
            detail,
            log_path: self.log_path.to_string_lossy().into_owned(),
            append_log,
        };
        if append_log {
            if let Ok(line) = serde_json::to_string(&event) {
                if let Ok(mut file) = OpenOptions::new().append(true).open(&self.log_path) {
                    let _ = writeln!(file, "{line}");
                }
            }
        }
        let _ = self.app.emit("sync-progress", event);
    }

    pub fn fail(&self, error: &str) {
        let (stage, message) = self
            .active
            .lock()
            .map(|active| active.clone())
            .unwrap_or_else(|_| ("prepare".into(), "Sync failed".into()));
        self.report(
            &stage,
            "error",
            0,
            0,
            0,
            None,
            message,
            Some(error.to_string()),
            true,
        );
    }

    fn temp_blob(&self, digest: &str) -> Result<TempBlob, String> {
        let (algorithm, encoded) = parse_blob_digest(digest)?;
        let directory = self
            .app
            .path()
            .app_cache_dir()
            .map_err(|error| app_error("syncCacheCreate", error))?
            .join("blobs")
            .join(algorithm);
        fs::create_dir_all(&directory).map_err(|error| app_error("syncCacheCreate", error))?;
        Ok(TempBlob {
            path: directory.join(format!(".{encoded}.{}.part", self.operation_id)),
        })
    }

    fn cached_blob(&self, digest: &str) -> Result<PathBuf, String> {
        let (algorithm, encoded) = parse_blob_digest(digest)?;
        let directory = self
            .app
            .path()
            .app_cache_dir()
            .map_err(|error| app_error("syncCacheCreate", error))?
            .join("blobs")
            .join(algorithm);
        fs::create_dir_all(&directory).map_err(|error| app_error("syncCacheCreate", error))?;
        Ok(directory.join(format!("{encoded}.blob")))
    }
}

#[derive(Clone)]
struct RawManifest {
    digest: String,
    media_type: String,
    body: Bytes,
    payload: Value,
}

pub struct RegistryClient {
    config: RegistryConfig,
    password: Option<String>,
    client: reqwest::Client,
}

impl RegistryClient {
    pub fn from_id(app: &AppHandle, id: &str) -> Result<Self, String> {
        let config = find_registry(app, id)?;
        let password = get_password(id)?;
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(config.timeout_secs))
            .read_timeout(Duration::from_secs(config.timeout_secs))
            .danger_accept_invalid_certs(config.skip_tls_verify)
            .user_agent("Kaven-Docker/0.1.0");
        if let Some(proxy) = resolve_proxy(app, &config)? {
            let mut reqwest_proxy = reqwest::Proxy::all(&proxy.url)
                .map_err(|error| app_error("invalidProxyUrl", error))?;
            if !proxy.username.is_empty() {
                let scheme = url::Url::parse(&proxy.url)
                    .map_err(|error| app_error("invalidProxyUrl", error))?
                    .scheme()
                    .to_string();
                if matches!(scheme.as_str(), "socks4" | "socks4a") {
                    return Err(app_error("proxyAuthUnsupported", ""));
                }
                reqwest_proxy = reqwest_proxy
                    .basic_auth(&proxy.username, proxy.password.as_deref().unwrap_or(""));
            }
            builder = builder.proxy(reqwest_proxy);
        } else {
            builder = builder.no_proxy();
        }
        if let Some(path) = &config.ca_cert_path {
            let pem = fs::read(path).map_err(|error| app_error("caRead", error))?;
            let certificate = reqwest::Certificate::from_pem(&pem)
                .map_err(|error| app_error("caFormat", error))?;
            builder = builder.add_root_certificate(certificate);
        }
        let client = builder
            .build()
            .map_err(|error| app_error("client", error))?;
        Ok(Self {
            config,
            password,
            client,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.url.trim_end_matches('/'), path)
    }

    fn request_builder(
        &self,
        method: Method,
        url: &str,
        accept: Option<&str>,
        content_type: Option<&str>,
        content_range: Option<&str>,
        body: Option<Bytes>,
    ) -> reqwest::RequestBuilder {
        let mut request = self.client.request(method, url);
        if let Some(value) = accept {
            request = request.header(header::ACCEPT, value);
        }
        if let Some(value) = content_type {
            request = request.header(header::CONTENT_TYPE, value);
        }
        if let Some(value) = content_range {
            request = request.header(header::CONTENT_RANGE, value);
        }
        if let Some(value) = body {
            request = request.body(value);
        }
        if self.config.auth_type == "basic" {
            request = request.basic_auth(&self.config.username, self.password.as_deref());
        }
        request
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        accept: Option<&str>,
    ) -> Result<Response, String> {
        let url = self.url(path);
        self.send_url(method, &url, accept, None, None, None).await
    }

    async fn send_url(
        &self,
        method: Method,
        url: &str,
        accept: Option<&str>,
        content_type: Option<&str>,
        content_range: Option<&str>,
        body: Option<Bytes>,
    ) -> Result<Response, String> {
        let response = self
            .request_builder(
                method.clone(),
                url,
                accept,
                content_type,
                content_range,
                body.clone(),
            )
            .send()
            .await
            .map_err(network_error)?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }
        let challenge = response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        if !challenge.to_ascii_lowercase().starts_with("bearer ") {
            return Ok(response);
        }
        let token = self.bearer_token(&challenge).await?;
        self.request_builder(method, url, accept, content_type, content_range, body)
            .bearer_auth(token)
            .send()
            .await
            .map_err(network_error)
    }

    async fn bearer_token(&self, challenge: &str) -> Result<String, String> {
        let values = parse_bearer_challenge(challenge);
        let realm = values
            .get("realm")
            .ok_or_else(|| app_error("bearerChallenge", ""))?;
        let mut request = self.client.get(realm);
        let mut query: Vec<(&str, &str)> = Vec::new();
        if let Some(service) = values.get("service") {
            query.push(("service", service));
        }
        if let Some(scope) = values.get("scope") {
            query.push(("scope", scope));
        }
        if !self.config.username.is_empty() {
            query.push(("account", &self.config.username));
        }
        request = request.query(&query);
        if self.config.auth_type == "basic" {
            request = request.basic_auth(&self.config.username, self.password.as_deref());
        }
        let response = request.send().await.map_err(network_error)?;
        let response = ensure_success(response, "获取认证 Token").await?;
        let payload: Value = response
            .json()
            .await
            .map_err(|error| app_error("tokenFormat", error))?;
        payload
            .get("token")
            .or_else(|| payload.get("access_token"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| app_error("tokenMissing", ""))
    }

    pub async fn test_connection(&self) -> Result<ConnectionStatus, String> {
        let response =
            ensure_success(self.send(Method::GET, "/v2/", None).await?, "连接 Registry").await?;
        let version = response
            .headers()
            .get("docker-distribution-api-version")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("registry/2.0")
            .to_string();
        Ok(ConnectionStatus {
            registry_version: version,
            message: "Connected".into(),
        })
    }

    pub async fn list_repositories(&self) -> Result<RepositoryPage, String> {
        let mut repositories = self.config.repositories.clone();
        if self.config.provider != "generic" {
            repositories.sort();
            repositories.dedup();
            return Ok(RepositoryPage { repositories });
        }
        let mut last: Option<String> = None;
        for _ in 0..100 {
            let path = match &last {
                Some(value) => format!("/v2/_catalog?n=100&last={}", urlencoding::encode(value)),
                None => "/v2/_catalog?n=100".to_string(),
            };
            let response =
                ensure_success(self.send(Method::GET, &path, None).await?, "读取仓库目录").await?;
            let payload: Value = response
                .json()
                .await
                .map_err(|error| app_error("catalogFormat", error))?;
            let page: Vec<String> = payload
                .get("repositories")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if page.is_empty() {
                break;
            }
            let next_last = page.last().cloned();
            let page_len = page.len();
            for repository in page {
                if !repositories.contains(&repository) {
                    repositories.push(repository);
                }
            }
            if page_len < 100 || next_last == last {
                break;
            }
            last = next_last;
        }
        repositories.sort();
        Ok(RepositoryPage { repositories })
    }

    pub async fn discover_repositories(&self, namespace: &str) -> Result<RepositoryPage, String> {
        let namespace = namespace.trim().trim_matches('/');
        if namespace.is_empty()
            || namespace.contains('/')
            || namespace.contains(':')
            || namespace.contains('@')
            || namespace.chars().any(char::is_whitespace)
        {
            return Err(app_error("invalidNamespace", ""));
        }

        let inferred_provider = if self.config.url.contains("registry-1.docker.io")
            || self.config.url.contains("docker.io")
        {
            "dockerHub"
        } else if self.config.url.contains("ghcr.io") {
            "ghcr"
        } else if self.config.url.contains("quay.io") {
            "quay"
        } else {
            self.config.provider.as_str()
        };
        let mut repositories = match inferred_provider {
            "dockerHub" => self.discover_docker_hub(namespace).await?,
            "ghcr" => self.discover_github_packages(namespace).await?,
            "quay" => self.discover_quay(namespace).await?,
            // A generic Registry already exposes its own catalog in the main view. The add
            // dialog is used to seed public repository names (for example on a pull-through
            // cache), so namespace discovery defaults to Docker Hub.
            "generic" => self.discover_docker_hub(namespace).await?,
            _ => return Err(app_error("repositoryDiscoveryUnsupported", "")),
        };
        repositories.sort();
        repositories.dedup();
        Ok(RepositoryPage { repositories })
    }

    async fn discover_docker_hub(&self, namespace: &str) -> Result<Vec<String>, String> {
        let mut url = Some(format!(
            "https://hub.docker.com/v2/namespaces/{}/repositories?page_size=100",
            urlencoding::encode(namespace)
        ));
        let mut repositories = Vec::new();
        for _ in 0..100 {
            let Some(page_url) = url.take() else { break };
            let mut request = self.client.get(&page_url);
            if self.config.auth_type == "basic" {
                if let Some(password) = self.password.as_deref() {
                    request = request.bearer_auth(password);
                }
            }
            let response =
                ensure_success(request.send().await.map_err(network_error)?, "发现仓库").await?;
            let payload: Value = response
                .json()
                .await
                .map_err(|error| app_error("repositoryDiscoveryFormat", error))?;
            repositories.extend(
                payload
                    .get("results")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|item| item.get("name").and_then(Value::as_str))
                    .map(|name| format!("{namespace}/{name}")),
            );
            url = payload
                .get("next")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        Ok(repositories)
    }

    async fn discover_github_packages(&self, namespace: &str) -> Result<Vec<String>, String> {
        let password = self
            .password
            .as_deref()
            .filter(|password| !password.is_empty())
            .ok_or_else(|| app_error("repositoryDiscoveryAuthRequired", ""))?;
        let owner_url = format!(
            "https://api.github.com/users/{}",
            urlencoding::encode(namespace)
        );
        let owner = ensure_success(
            self.client
                .get(owner_url)
                .bearer_auth(password)
                .header(header::ACCEPT, "application/vnd.github+json")
                .send()
                .await
                .map_err(network_error)?,
            "读取 GitHub 用户",
        )
        .await?
        .json::<Value>()
        .await
        .map_err(|error| app_error("repositoryDiscoveryFormat", error))?;
        let owner_kind = if owner.get("type").and_then(Value::as_str) == Some("Organization") {
            "orgs"
        } else {
            "users"
        };

        let mut repositories = Vec::new();
        for page in 1..=100 {
            let url = format!(
                "https://api.github.com/{owner_kind}/{}/packages?package_type=container&per_page=100&page={page}",
                urlencoding::encode(namespace)
            );
            let response = ensure_success(
                self.client
                    .get(url)
                    .bearer_auth(password)
                    .header(header::ACCEPT, "application/vnd.github+json")
                    .send()
                    .await
                    .map_err(network_error)?,
                "发现 GitHub Container Registry 仓库",
            )
            .await?;
            let payload: Vec<Value> = response
                .json()
                .await
                .map_err(|error| app_error("repositoryDiscoveryFormat", error))?;
            let page_len = payload.len();
            repositories.extend(
                payload
                    .iter()
                    .filter_map(|item| item.get("name").and_then(Value::as_str))
                    .map(|name| format!("{namespace}/{name}")),
            );
            if page_len < 100 {
                break;
            }
        }
        Ok(repositories)
    }

    async fn discover_quay(&self, namespace: &str) -> Result<Vec<String>, String> {
        let mut next_page: Option<String> = None;
        let mut repositories = Vec::new();
        for _ in 0..100 {
            let mut request = self
                .client
                .get("https://quay.io/api/v1/repository")
                .query(&[("public", "true"), ("namespace", namespace)]);
            if let Some(page) = next_page.as_deref() {
                request = request.query(&[("next_page", page)]);
            }
            if self.config.auth_type == "basic" {
                if let Some(password) = self.password.as_deref() {
                    request = request.bearer_auth(password);
                }
            }
            let response =
                ensure_success(request.send().await.map_err(network_error)?, "发现仓库").await?;
            let payload: Value = response
                .json()
                .await
                .map_err(|error| app_error("repositoryDiscoveryFormat", error))?;
            repositories.extend(
                payload
                    .get("repositories")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|item| item.get("name").and_then(Value::as_str))
                    .map(|name| format!("{namespace}/{name}")),
            );
            next_page = payload
                .get("next_page")
                .and_then(Value::as_str)
                .filter(|page| !page.is_empty())
                .map(str::to_string);
            if next_page.is_none() {
                break;
            }
        }
        Ok(repositories)
    }

    pub async fn list_tags(&self, repository: &str) -> Result<TagList, String> {
        let path = format!("/v2/{}/tags/list", encode_repository(repository));
        let response =
            ensure_success(self.send(Method::GET, &path, None).await?, "读取镜像标签").await?;
        let payload: Value = response
            .json()
            .await
            .map_err(|error| app_error("tagsFormat", error))?;
        let mut tags: Vec<String> = payload
            .get("tags")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        tags.sort_by(|a, b| b.cmp(a));
        Ok(TagList {
            name: payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(repository)
                .to_string(),
            tags,
        })
    }

    async fn manifest(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<(Value, String, String), String> {
        let raw = self.raw_manifest(repository, reference).await?;
        Ok((raw.payload, raw.digest, raw.media_type))
    }

    async fn raw_manifest(&self, repository: &str, reference: &str) -> Result<RawManifest, String> {
        let path = format!(
            "/v2/{}/manifests/{}",
            encode_repository(repository),
            urlencoding::encode(reference)
        );
        let response = ensure_success(
            self.send(Method::GET, &path, Some(MANIFEST_ACCEPT)).await?,
            "读取 Manifest",
        )
        .await?;
        let digest_header = response
            .headers()
            .get("docker-content-digest")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .to_string();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| app_error("manifestRead", error))?;
        let digest = digest_header
            .unwrap_or_else(|| format!("sha256:{}", hex::encode(Sha256::digest(&bytes))));
        let payload: Value =
            serde_json::from_slice(&bytes).map_err(|error| app_error("manifestFormat", error))?;
        let media_type = payload
            .get("mediaType")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(&content_type)
            .to_string();
        Ok(RawManifest {
            digest,
            media_type,
            body: bytes,
            payload,
        })
    }

    async fn image_config(&self, repository: &str, digest: &str) -> Result<Value, String> {
        let path = format!(
            "/v2/{}/blobs/{}",
            encode_repository(repository),
            urlencoding::encode(digest)
        );
        let response =
            ensure_success(self.send(Method::GET, &path, None).await?, "读取镜像配置").await?;
        response
            .json()
            .await
            .map_err(|error| app_error("imageConfigFormat", error))
    }

    pub async fn image_details(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<ImageDetails, String> {
        let (manifest, digest, media_type) = self.manifest(repository, reference).await?;
        let mut details = ImageDetails {
            repository: repository.to_string(),
            reference: reference.to_string(),
            digest,
            media_type,
            size: 0,
            created: None,
            architecture: None,
            os: None,
            labels: BTreeMap::new(),
            layers: Vec::new(),
            platforms: Vec::new(),
        };
        let mut seen_blobs = HashSet::new();
        if let Some(manifests) = manifest.get("manifests").and_then(Value::as_array) {
            for item in manifests {
                let child_digest = item.get("digest").and_then(Value::as_str).unwrap_or("");
                if child_digest.is_empty() {
                    continue;
                }
                let child = self.raw_manifest(repository, child_digest).await?;
                let platform_size = manifest_content_size(&child.payload);
                merge_manifest_blobs(&mut details, &child.payload, &mut seen_blobs);
                if let Some(config_digest) = child
                    .payload
                    .get("config")
                    .and_then(|config| config.get("digest"))
                    .and_then(Value::as_str)
                {
                    let config = self.image_config(repository, config_digest).await?;
                    merge_config_metadata(&mut details, &config, false);
                }
                let platform = item.get("platform").unwrap_or(&Value::Null);
                details.platforms.push(PlatformInfo {
                    architecture: platform
                        .get("architecture")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .into(),
                    os: platform
                        .get("os")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .into(),
                    variant: platform
                        .get("variant")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    digest: child_digest.into(),
                    size: platform_size,
                });
            }
            return Ok(details);
        }
        merge_manifest_blobs(&mut details, &manifest, &mut seen_blobs);
        if let Some(config_digest) = manifest
            .get("config")
            .and_then(|config| config.get("digest"))
            .and_then(Value::as_str)
        {
            let config = self.image_config(repository, config_digest).await?;
            merge_config_metadata(&mut details, &config, true);
        }
        Ok(details)
    }

    pub async fn sync_to(
        &self,
        target: &RegistryClient,
        source_repository: &str,
        target_repository: &str,
        tags: &[String],
        platform: Option<&SyncPlatform>,
        reporter: &SyncReporter,
    ) -> Result<SyncResult, String> {
        let (manifests, tag_digests) = self
            .collect_manifest_graph(source_repository, tags, platform, reporter)
            .await?;
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        for digest in tag_digests.values() {
            visit_manifest(digest, &manifests, &mut visited, &mut order);
        }

        let mut result = SyncResult {
            synced_tags: 0,
            copied_blobs: 0,
            skipped_blobs: 0,
            cached_blobs: 0,
            transferred_bytes: 0,
            succeeded_repositories: 1,
            failed_repositories: Vec::new(),
        };
        let mut blob_digests = Vec::new();
        let mut processed_blobs = HashSet::new();
        for digest in &order {
            let manifest = manifests
                .get(digest)
                .ok_or_else(|| app_error("manifestMissing", digest))?;
            for blob_digest in manifest_blob_digests(&manifest.payload) {
                if processed_blobs.insert(blob_digest.clone()) {
                    blob_digests.push(blob_digest);
                }
            }
        }

        reporter.report(
            "blobs",
            "active",
            0,
            blob_digests.len(),
            0,
            None,
            format!("Preparing to process {} unique blobs", blob_digests.len()),
            None,
            true,
        );
        for (index, blob_digest) in blob_digests.iter().enumerate() {
            let (copied, cache_hit) = self
                .copy_blob_to(
                    target,
                    source_repository,
                    target_repository,
                    blob_digest,
                    &mut result.transferred_bytes,
                    reporter,
                    index,
                    blob_digests.len(),
                )
                .await?;
            if copied {
                result.copied_blobs += 1;
            } else {
                result.skipped_blobs += 1;
            }
            if cache_hit {
                result.cached_blobs += 1;
            }
            reporter.report(
                "blobs",
                "active",
                index + 1,
                blob_digests.len(),
                0,
                None,
                format!("Processed blob {blob_digest}"),
                None,
                true,
            );
        }
        reporter.report(
            "blobs",
            "complete",
            blob_digests.len(),
            blob_digests.len(),
            0,
            None,
            format!(
                "Blob transfer complete: {} copied, {} skipped, {} cache hits",
                result.copied_blobs, result.skipped_blobs, result.cached_blobs
            ),
            None,
            true,
        );

        reporter.report(
            "manifestsWrite",
            "active",
            0,
            order.len(),
            0,
            None,
            format!("Preparing to write {} manifests", order.len()),
            None,
            true,
        );
        for (index, digest) in order.iter().enumerate() {
            let manifest = manifests
                .get(digest)
                .ok_or_else(|| app_error("manifestMissing", digest))?;
            reporter.report(
                "manifestsWrite",
                "active",
                index,
                order.len(),
                0,
                None,
                format!("PUT target manifest {digest}"),
                None,
                true,
            );
            target
                .put_manifest(target_repository, &manifest.digest, manifest)
                .await?;
            reporter.report(
                "manifestsWrite",
                "active",
                index + 1,
                order.len(),
                0,
                None,
                format!("Wrote target manifest {digest}"),
                None,
                true,
            );
        }
        reporter.report(
            "manifestsWrite",
            "complete",
            order.len(),
            order.len(),
            0,
            None,
            "All manifests written",
            None,
            true,
        );

        reporter.report(
            "tags",
            "active",
            0,
            tags.len(),
            0,
            None,
            format!("Preparing to publish {} tags", tags.len()),
            None,
            true,
        );
        for (index, tag) in tags.iter().enumerate() {
            let digest = tag_digests
                .get(tag)
                .ok_or_else(|| app_error("manifestMissing", tag))?;
            let manifest = manifests
                .get(digest)
                .ok_or_else(|| app_error("manifestMissing", digest))?;
            reporter.report(
                "tags",
                "active",
                index,
                tags.len(),
                0,
                None,
                format!("Publishing target tag {tag} -> {digest}"),
                None,
                true,
            );
            target
                .put_manifest(target_repository, tag, manifest)
                .await?;
            result.synced_tags += 1;
            reporter.report(
                "tags",
                "active",
                index + 1,
                tags.len(),
                0,
                None,
                format!("Published target tag {tag}"),
                None,
                true,
            );
        }
        reporter.report(
            "tags",
            "complete",
            tags.len(),
            tags.len(),
            0,
            None,
            "All selected tags published",
            None,
            true,
        );
        Ok(result)
    }

    async fn collect_manifest_graph(
        &self,
        repository: &str,
        tags: &[String],
        platform: Option<&SyncPlatform>,
        reporter: &SyncReporter,
    ) -> Result<(HashMap<String, RawManifest>, BTreeMap<String, String>), String> {
        if let Some(platform) = platform {
            return self
                .collect_platform_manifest_graph(repository, tags, platform, reporter)
                .await;
        }
        let mut manifests = HashMap::new();
        let mut tag_digests = BTreeMap::new();
        let mut queue = VecDeque::new();
        let mut completed = 0;
        let mut total = tags.len();

        for tag in tags {
            reporter.report(
                "manifests",
                "active",
                completed,
                total,
                0,
                None,
                format!("GET source manifest {repository}:{tag}"),
                None,
                true,
            );
            let manifest = self.raw_manifest(repository, tag).await?;
            reporter.report(
                "manifests",
                "active",
                completed + 1,
                total,
                0,
                None,
                format!("Resolved source tag {tag} -> {}", manifest.digest),
                None,
                true,
            );
            tag_digests.insert(tag.clone(), manifest.digest.clone());
            for child in child_manifest_digests(&manifest.payload) {
                queue.push_back(child);
            }
            manifests.insert(manifest.digest.clone(), manifest);
            completed += 1;
            total = completed + queue.len() + tags.len().saturating_sub(completed);
        }

        while let Some(reference) = queue.pop_front() {
            if manifests.contains_key(&reference) {
                continue;
            }
            reporter.report(
                "manifests",
                "active",
                completed,
                total,
                0,
                None,
                format!("GET source child manifest {repository}@{reference}"),
                None,
                true,
            );
            let manifest = self.raw_manifest(repository, &reference).await?;
            reporter.report(
                "manifests",
                "active",
                completed + 1,
                total,
                0,
                None,
                format!("Resolved child manifest {reference} -> {}", manifest.digest),
                None,
                true,
            );
            for child in child_manifest_digests(&manifest.payload) {
                if !manifests.contains_key(&child) {
                    queue.push_back(child);
                }
            }
            manifests.insert(manifest.digest.clone(), manifest);
            completed += 1;
            total = completed + queue.len();
        }
        reporter.report(
            "manifests",
            "complete",
            completed,
            completed,
            0,
            None,
            format!("Resolved {completed} source manifests"),
            None,
            true,
        );
        Ok((manifests, tag_digests))
    }

    async fn collect_platform_manifest_graph(
        &self,
        repository: &str,
        tags: &[String],
        platform: &SyncPlatform,
        reporter: &SyncReporter,
    ) -> Result<(HashMap<String, RawManifest>, BTreeMap<String, String>), String> {
        let mut manifests = HashMap::new();
        let mut tag_digests = BTreeMap::new();
        let mut completed = 0;
        let mut total = tags.len();

        for tag in tags {
            reporter.report(
                "manifests",
                "active",
                completed,
                total,
                0,
                None,
                format!("GET source manifest {repository}:{tag}"),
                None,
                true,
            );
            let mut manifest = self.raw_manifest(repository, tag).await?;
            completed += 1;
            let mut selected_from_index = false;

            while manifest
                .payload
                .get("manifests")
                .and_then(Value::as_array)
                .is_some()
            {
                selected_from_index = true;
                let digest =
                    platform_manifest_digest(&manifest.payload, platform).ok_or_else(|| {
                        app_error(
                            "syncPlatformNotFound",
                            format!("{repository}:{tag} ({})", platform.display()),
                        )
                    })?;
                total += 1;
                reporter.report(
                    "manifests",
                    "active",
                    completed,
                    total,
                    0,
                    None,
                    format!(
                        "GET source platform manifest {repository}@{digest} ({})",
                        platform.display()
                    ),
                    None,
                    true,
                );
                manifest = self.raw_manifest(repository, &digest).await?;
                completed += 1;
            }

            if !selected_from_index {
                self.ensure_manifest_platform(repository, &manifest, platform)
                    .await?;
            }
            reporter.report(
                "manifests",
                "active",
                completed,
                total,
                0,
                None,
                format!(
                    "Selected source tag {tag} -> {} ({})",
                    manifest.digest,
                    platform.display()
                ),
                None,
                true,
            );
            tag_digests.insert(tag.clone(), manifest.digest.clone());
            manifests.insert(manifest.digest.clone(), manifest);
        }

        reporter.report(
            "manifests",
            "complete",
            completed,
            completed,
            0,
            None,
            format!(
                "Resolved {} source manifests for {}",
                manifests.len(),
                platform.display()
            ),
            None,
            true,
        );
        Ok((manifests, tag_digests))
    }

    async fn ensure_manifest_platform(
        &self,
        repository: &str,
        manifest: &RawManifest,
        platform: &SyncPlatform,
    ) -> Result<(), String> {
        let config_digest = manifest
            .payload
            .get("config")
            .and_then(|config| config.get("digest"))
            .and_then(Value::as_str)
            .ok_or_else(|| app_error("syncPlatformNotFound", platform.display()))?;
        let config = self.image_config(repository, config_digest).await?;
        if platform_matches(
            config.get("os").and_then(Value::as_str),
            config.get("architecture").and_then(Value::as_str),
            config.get("variant").and_then(Value::as_str),
            platform,
        ) {
            Ok(())
        } else {
            Err(app_error(
                "syncPlatformNotFound",
                format!("{repository}@{} ({})", manifest.digest, platform.display()),
            ))
        }
    }

    async fn blob_exists(&self, repository: &str, digest: &str) -> Result<bool, String> {
        let path = format!(
            "/v2/{}/blobs/{}",
            encode_repository(repository),
            urlencoding::encode(digest)
        );
        let response = self.send(Method::HEAD, &path, None).await?;
        if response.status().is_success() {
            Ok(true)
        } else if response.status() == StatusCode::NOT_FOUND {
            Ok(false)
        } else {
            ensure_success(response, "check blob").await?;
            Ok(false)
        }
    }

    async fn copy_blob_to(
        &self,
        target: &RegistryClient,
        source_repository: &str,
        target_repository: &str,
        digest: &str,
        transferred_bytes: &mut u64,
        reporter: &SyncReporter,
        blob_index: usize,
        blob_total: usize,
    ) -> Result<(bool, bool), String> {
        reporter.report(
            "blobs",
            "active",
            blob_index,
            blob_total,
            0,
            None,
            format!("HEAD target blob {target_repository}@{digest}"),
            None,
            true,
        );
        if target.blob_exists(target_repository, digest).await? {
            reporter.report(
                "blobs",
                "active",
                blob_index + 1,
                blob_total,
                0,
                Some(0),
                format!("Skipped existing target blob {digest}"),
                None,
                true,
            );
            return Ok((false, false));
        }

        let cached_blob = reporter.cached_blob(digest)?;
        let mut cache_hit = cached_blob.exists();
        if cache_hit && !file_matches_digest(&cached_blob, digest).await? {
            tokio::fs::remove_file(&cached_blob)
                .await
                .map_err(|error| app_error("syncCacheWrite", error))?;
            cache_hit = false;
            reporter.report(
                "blobs",
                "active",
                blob_index,
                blob_total,
                0,
                None,
                format!("Discarded invalid cached blob {digest}"),
                None,
                true,
            );
        }

        let downloaded = if cache_hit {
            let size = tokio::fs::metadata(&cached_blob)
                .await
                .map_err(|error| app_error("syncCacheRead", error))?
                .len();
            reporter.report(
                "blobs",
                "active",
                blob_index,
                blob_total,
                0,
                Some(size),
                format!("Cache hit for blob {digest} ({size} bytes)"),
                None,
                true,
            );
            size
        } else {
            let source_path = format!(
                "/v2/{}/blobs/{}",
                encode_repository(source_repository),
                urlencoding::encode(digest)
            );
            reporter.report(
                "blobs",
                "active",
                blob_index,
                blob_total,
                0,
                None,
                format!("GET source blob {source_repository}@{digest}"),
                None,
                true,
            );
            let mut source_response = ensure_success(
                self.send(Method::GET, &source_path, None).await?,
                "read blob",
            )
            .await?;
            let blob_size = source_response.content_length();
            let transfer_total = blob_size.map(|size| size.saturating_mul(2));
            let temp_blob = reporter.temp_blob(digest)?;
            let mut temp_file = tokio::fs::File::create(&temp_blob.path)
                .await
                .map_err(|error| app_error("syncTempWrite", error))?;
            let mut downloaded = 0_u64;

            reporter.report(
                "blobs",
                "active",
                blob_index,
                blob_total,
                0,
                transfer_total,
                format!("Downloading source blob {source_repository}@{digest}"),
                None,
                true,
            );
            loop {
                let Some(chunk) = source_response.chunk().await.map_err(|error| {
                    app_error(
                        "blobRead",
                        format!(
                            "source={}; repository={source_repository}; digest={digest}; received={downloaded}; expected={}; {}",
                            self.config.url,
                            blob_size
                                .map(|size| size.to_string())
                                .unwrap_or_else(|| "unknown".into()),
                            error_chain(&error)
                        ),
                    )
                })? else {
                    break;
                };
                temp_file
                    .write_all(&chunk)
                    .await
                    .map_err(|error| app_error("syncTempWrite", error))?;
                downloaded += chunk.len() as u64;
                reporter.report(
                    "blobs",
                    "active",
                    blob_index,
                    blob_total,
                    downloaded,
                    transfer_total,
                    format!("Downloading source blob {digest}"),
                    None,
                    false,
                );
            }
            temp_file
                .flush()
                .await
                .map_err(|error| app_error("syncTempWrite", error))?;
            drop(temp_file);
            if let Some(expected) = blob_size {
                if downloaded != expected {
                    return Err(app_error(
                        "blobRead",
                        format!(
                            "source={}; repository={source_repository}; digest={digest}; received={downloaded}; expected={expected}; response ended before the complete blob was received",
                            self.config.url
                        ),
                    ));
                }
            }
            if !file_matches_digest(&temp_blob.path, digest).await? {
                return Err(app_error("syncCacheDigest", digest));
            }
            if cached_blob.exists() {
                tokio::fs::remove_file(&cached_blob)
                    .await
                    .map_err(|error| app_error("syncCacheWrite", error))?;
            }
            tokio::fs::rename(&temp_blob.path, &cached_blob)
                .await
                .map_err(|error| app_error("syncCacheWrite", error))?;
            reporter.report(
                "blobs",
                "active",
                blob_index,
                blob_total,
                downloaded,
                Some(downloaded.saturating_mul(2)),
                format!("Cached source blob {digest} ({downloaded} bytes)"),
                None,
                true,
            );
            downloaded
        };
        let upload_progress_base = if cache_hit { 0 } else { downloaded };
        let transfer_total = if cache_hit {
            Some(downloaded)
        } else {
            Some(downloaded.saturating_mul(2))
        };

        let start_path = format!(
            "/v2/{}/blobs/uploads/",
            encode_repository(target_repository)
        );
        reporter.report(
            "blobs",
            "active",
            blob_index,
            blob_total,
            upload_progress_base,
            transfer_total,
            format!("POST target blob upload {target_repository}@{digest}"),
            None,
            true,
        );
        let start_response = ensure_success(
            target.send(Method::POST, &start_path, None).await?,
            "start blob upload",
        )
        .await?;
        let mut upload_url = target.upload_location(&start_response)?;
        let mut temp_file = tokio::fs::File::open(&cached_blob)
            .await
            .map_err(|error| app_error("syncTempRead", error))?;
        let mut offset = 0_u64;
        let mut uploaded = 0_u64;
        let upload_chunk_size =
            target.config.upload_chunk_size_mb.clamp(1, 256) as usize * 1024 * 1024;
        let mut buffer = vec![0_u8; upload_chunk_size];
        reporter.report(
            "blobs",
            "active",
            blob_index,
            blob_total,
            upload_progress_base,
            transfer_total,
            format!("Uploading target blob {target_repository}@{digest}"),
            None,
            true,
        );

        loop {
            let read = temp_file
                .read(&mut buffer)
                .await
                .map_err(|error| app_error("syncTempRead", error))?;
            if read == 0 {
                break;
            }
            let bytes = Bytes::copy_from_slice(&buffer[..read]);
            reporter.report(
                "blobs",
                "active",
                blob_index,
                blob_total,
                upload_progress_base + uploaded,
                transfer_total,
                format!(
                    "PATCH target blob {target_repository}@{digest} bytes {}-{}",
                    offset,
                    offset + bytes.len() as u64 - 1
                ),
                None,
                false,
            );
            upload_url = target
                .upload_chunk(&upload_url, offset, bytes.clone())
                .await?;
            offset += bytes.len() as u64;
            uploaded += bytes.len() as u64;
            *transferred_bytes += bytes.len() as u64;
            reporter.report(
                "blobs",
                "active",
                blob_index,
                blob_total,
                upload_progress_base + uploaded,
                transfer_total,
                format!("Uploading target blob {digest}"),
                None,
                false,
            );
        }

        let mut final_url =
            url::Url::parse(&upload_url).map_err(|error| app_error("uploadLocation", error))?;
        final_url.query_pairs_mut().append_pair("digest", digest);
        reporter.report(
            "blobs",
            "active",
            blob_index,
            blob_total,
            upload_progress_base + uploaded,
            transfer_total,
            format!("PUT finalize target blob {target_repository}@{digest}"),
            None,
            true,
        );
        ensure_success(
            target
                .send_url(
                    Method::PUT,
                    final_url.as_str(),
                    None,
                    Some("application/octet-stream"),
                    None,
                    Some(Bytes::new()),
                )
                .await?,
            "finish blob upload",
        )
        .await?;
        reporter.report(
            "blobs",
            "active",
            blob_index + 1,
            blob_total,
            upload_progress_base + uploaded,
            transfer_total,
            format!("Copied blob {digest} ({uploaded} bytes)"),
            None,
            true,
        );
        Ok((true, cache_hit))
    }

    async fn upload_chunk(
        &self,
        upload_url: &str,
        offset: u64,
        bytes: Bytes,
    ) -> Result<String, String> {
        let end = offset + bytes.len() as u64 - 1;
        let range = format!("{offset}-{end}");
        let response = ensure_success(
            self.send_url(
                Method::PATCH,
                upload_url,
                None,
                Some("application/octet-stream"),
                Some(&range),
                Some(bytes),
            )
            .await?,
            "upload blob chunk",
        )
        .await?;
        self.upload_location(&response)
    }

    fn upload_location(&self, response: &Response) -> Result<String, String> {
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| app_error("uploadLocation", "missing Location header"))?;
        if url::Url::parse(location).is_ok() {
            return Ok(location.to_string());
        }
        let base = url::Url::parse(&format!("{}/", self.config.url.trim_end_matches('/')))
            .map_err(|error| app_error("uploadLocation", error))?;
        base.join(location)
            .map(|value| value.to_string())
            .map_err(|error| app_error("uploadLocation", error))
    }

    async fn put_manifest(
        &self,
        repository: &str,
        reference: &str,
        manifest: &RawManifest,
    ) -> Result<(), String> {
        let url = self.url(&format!(
            "/v2/{}/manifests/{}",
            encode_repository(repository),
            urlencoding::encode(reference)
        ));
        ensure_success(
            self.send_url(
                Method::PUT,
                &url,
                None,
                Some(&manifest.media_type),
                None,
                Some(manifest.body.clone()),
            )
            .await?,
            "write manifest",
        )
        .await?;
        Ok(())
    }

    pub async fn delete_manifest(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<String, String> {
        let (_, digest, _) = self.manifest(repository, reference).await?;
        let path = format!(
            "/v2/{}/manifests/{}",
            encode_repository(repository),
            urlencoding::encode(&digest)
        );
        ensure_success(
            self.send(Method::DELETE, &path, Some(MANIFEST_ACCEPT))
                .await?,
            "删除 Manifest",
        )
        .await?;
        Ok(digest)
    }
}

fn manifest_content_size(manifest: &Value) -> u64 {
    let config_size = manifest
        .get("config")
        .and_then(|config| config.get("size"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let layer_size = manifest
        .get("layers")
        .and_then(Value::as_array)
        .map(|layers| {
            layers
                .iter()
                .filter_map(|layer| layer.get("size").and_then(Value::as_u64))
                .sum()
        })
        .unwrap_or(0);
    config_size + layer_size
}

fn merge_manifest_blobs(
    details: &mut ImageDetails,
    manifest: &Value,
    seen_blobs: &mut HashSet<String>,
) {
    if let Some(config) = manifest.get("config") {
        let digest = config.get("digest").and_then(Value::as_str).unwrap_or("");
        if !digest.is_empty() && seen_blobs.insert(digest.to_string()) {
            details.size += config.get("size").and_then(Value::as_u64).unwrap_or(0);
        }
    }
    if let Some(layers) = manifest.get("layers").and_then(Value::as_array) {
        for layer in layers {
            let digest = layer.get("digest").and_then(Value::as_str).unwrap_or("");
            if digest.is_empty() || !seen_blobs.insert(digest.to_string()) {
                continue;
            }
            let size = layer.get("size").and_then(Value::as_u64).unwrap_or(0);
            details.size += size;
            details.layers.push(LayerInfo {
                media_type: layer
                    .get("mediaType")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                digest: digest.into(),
                size,
            });
        }
    }
}

fn merge_config_metadata(details: &mut ImageDetails, config: &Value, set_platform: bool) {
    if let Some(created) = config.get("created").and_then(Value::as_str) {
        if details
            .created
            .as_deref()
            .map(|current| created > current)
            .unwrap_or(true)
        {
            details.created = Some(created.to_string());
        }
    }
    if set_platform {
        details.architecture = config
            .get("architecture")
            .and_then(Value::as_str)
            .map(str::to_string);
        details.os = config.get("os").and_then(Value::as_str).map(str::to_string);
    }
    if let Some(labels) = config.pointer("/config/Labels").and_then(Value::as_object) {
        for (key, value) in labels {
            details.labels.insert(
                key.clone(),
                value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string()),
            );
        }
    }
}

fn platform_matches(
    os: Option<&str>,
    architecture: Option<&str>,
    variant: Option<&str>,
    selected: &SyncPlatform,
) -> bool {
    os.is_some_and(|value| value.eq_ignore_ascii_case(&selected.os))
        && architecture.is_some_and(|value| value.eq_ignore_ascii_case(&selected.architecture))
        && selected.variant.as_deref().is_none_or(|expected| {
            variant.is_some_and(|value| value.eq_ignore_ascii_case(expected))
        })
}

fn platform_manifest_digest(payload: &Value, selected: &SyncPlatform) -> Option<String> {
    payload
        .get("manifests")
        .and_then(Value::as_array)?
        .iter()
        .find(|item| {
            let platform = item.get("platform").unwrap_or(&Value::Null);
            platform_matches(
                platform.get("os").and_then(Value::as_str),
                platform.get("architecture").and_then(Value::as_str),
                platform.get("variant").and_then(Value::as_str),
                selected,
            )
        })
        .and_then(|item| item.get("digest").and_then(Value::as_str))
        .map(str::to_string)
}

fn child_manifest_digests(payload: &Value) -> Vec<String> {
    payload
        .get("manifests")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("digest").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn manifest_blob_digests(payload: &Value) -> Vec<String> {
    let mut digests = Vec::new();
    if let Some(digest) = payload
        .get("config")
        .and_then(|config| config.get("digest"))
        .and_then(Value::as_str)
    {
        digests.push(digest.to_string());
    }
    for field in ["layers", "blobs"] {
        if let Some(items) = payload.get(field).and_then(Value::as_array) {
            digests.extend(
                items
                    .iter()
                    .filter_map(|item| item.get("digest").and_then(Value::as_str))
                    .map(str::to_string),
            );
        }
    }
    digests
}

fn visit_manifest(
    digest: &str,
    manifests: &HashMap<String, RawManifest>,
    visited: &mut HashSet<String>,
    order: &mut Vec<String>,
) {
    if !visited.insert(digest.to_string()) {
        return;
    }
    if let Some(manifest) = manifests.get(digest) {
        for child in child_manifest_digests(&manifest.payload) {
            visit_manifest(&child, manifests, visited, order);
        }
        order.push(digest.to_string());
    }
}

fn parse_blob_digest(digest: &str) -> Result<(&str, &str), String> {
    let (algorithm, encoded) = digest
        .split_once(':')
        .ok_or_else(|| app_error("syncCacheDigest", digest))?;
    let valid_length = match algorithm {
        "sha256" => encoded.len() == 64,
        "sha512" => encoded.len() == 128,
        _ => false,
    };
    if !valid_length
        || !encoded
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(app_error("syncCacheDigest", digest));
    }
    Ok((algorithm, encoded))
}

async fn hash_file<D: Digest + Default>(path: &std::path::Path) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| app_error("syncCacheRead", error))?;
    let mut hasher = D::new();
    let mut buffer = vec![0_u8; 8 * 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| app_error("syncCacheRead", error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn file_matches_digest(path: &std::path::Path, digest: &str) -> Result<bool, String> {
    let (algorithm, expected) = parse_blob_digest(digest)?;
    let actual = match algorithm {
        "sha256" => hash_file::<Sha256>(path).await?,
        "sha512" => hash_file::<Sha512>(path).await?,
        _ => return Ok(false),
    };
    Ok(actual.eq_ignore_ascii_case(expected))
}

fn encode_repository(repository: &str) -> String {
    repository
        .split('/')
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn parse_bearer_challenge(challenge: &str) -> BTreeMap<String, String> {
    challenge
        .trim_start_matches(|character: char| character != ' ')
        .trim()
        .split(',')
        .filter_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            Some((
                key.trim().to_ascii_lowercase(),
                value.trim().trim_matches('"').to_string(),
            ))
        })
        .collect()
}

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        app_error("timeout", error_chain(&error))
    } else if error.is_connect() {
        app_error("connect", error_chain(&error))
    } else {
        app_error("request", error_chain(&error))
    }
}

fn error_chain(error: &(dyn Error + 'static)) -> String {
    let mut messages = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        let message = cause.to_string();
        if !messages.contains(&message) {
            messages.push(message);
        }
        source = cause.source();
    }
    messages.join("; caused by: ")
}

async fn ensure_success(response: Response, action: &str) -> Result<Response, String> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/errors/0/message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| body.chars().take(240).collect());
    let code = match status {
        StatusCode::UNAUTHORIZED => "auth",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "notFound",
        StatusCode::METHOD_NOT_ALLOWED => "deleteDisabled",
        _ => "registryError",
    };
    let detail = if detail.is_empty() {
        format!("{action}: HTTP {status}")
    } else {
        format!("{action}: HTTP {status}; {detail}")
    };
    Err(app_error(code, detail))
}

#[cfg(test)]
mod tests {
    use super::{
        child_manifest_digests, encode_repository, manifest_blob_digests, manifest_content_size,
        merge_config_metadata, merge_manifest_blobs, parse_bearer_challenge, parse_blob_digest,
        platform_manifest_digest, ImageDetails, SyncPlatform,
    };
    use serde_json::json;
    use std::collections::{BTreeMap, HashSet};

    #[test]
    fn encodes_each_repository_path_segment() {
        assert_eq!(encode_repository("team/api image"), "team/api%20image");
    }

    #[test]
    fn validates_cache_digest_paths() {
        let sha256 = format!("sha256:{}", "a".repeat(64));
        let sha512 = format!("sha512:{}", "b".repeat(128));
        assert_eq!(parse_blob_digest(&sha256).unwrap().0, "sha256");
        assert_eq!(parse_blob_digest(&sha512).unwrap().0, "sha512");
        assert!(parse_blob_digest("sha256:../escape").is_err());
        assert!(parse_blob_digest("md5:d41d8cd98f00b204e9800998ecf8427e").is_err());
    }

    #[test]
    fn parses_standard_bearer_challenge() {
        let values = parse_bearer_challenge(
            "Bearer realm=\"https://auth.example/token\",service=\"registry.example\",scope=\"repository:team/api:pull\"",
        );
        assert_eq!(values.get("realm").unwrap(), "https://auth.example/token");
        assert_eq!(values.get("service").unwrap(), "registry.example");
        assert_eq!(values.get("scope").unwrap(), "repository:team/api:pull");
    }

    #[test]
    fn extracts_manifest_dependencies() {
        let index = json!({
            "manifests": [
                { "digest": "sha256:amd64" },
                { "digest": "sha256:arm64" }
            ]
        });
        assert_eq!(
            child_manifest_digests(&index),
            vec!["sha256:amd64", "sha256:arm64"]
        );

        let manifest = json!({
            "config": { "digest": "sha256:config" },
            "layers": [
                { "digest": "sha256:layer1" },
                { "digest": "sha256:layer2" }
            ]
        });
        assert_eq!(
            manifest_blob_digests(&manifest),
            vec!["sha256:config", "sha256:layer1", "sha256:layer2"]
        );
    }

    #[test]
    fn selects_only_the_requested_platform_manifest() {
        let index = json!({
            "manifests": [
                {
                    "digest": "sha256:amd64",
                    "platform": { "os": "linux", "architecture": "amd64" }
                },
                {
                    "digest": "sha256:arm64",
                    "platform": { "os": "linux", "architecture": "arm64", "variant": "v8" }
                },
                {
                    "digest": "sha256:attestation",
                    "platform": { "os": "unknown", "architecture": "unknown" }
                }
            ]
        });
        let amd64 = SyncPlatform {
            os: "linux".into(),
            architecture: "amd64".into(),
            variant: None,
        };
        let arm64 = SyncPlatform {
            os: "linux".into(),
            architecture: "arm64".into(),
            variant: Some("v8".into()),
        };

        assert_eq!(
            platform_manifest_digest(&index, &amd64).as_deref(),
            Some("sha256:amd64")
        );
        assert_eq!(
            platform_manifest_digest(&index, &arm64).as_deref(),
            Some("sha256:arm64")
        );
    }

    #[test]
    fn aggregates_multi_platform_image_metadata() {
        let manifest = json!({
            "config": { "digest": "sha256:config", "size": 100 },
            "layers": [
                { "digest": "sha256:shared", "size": 200, "mediaType": "layer" },
                { "digest": "sha256:unique", "size": 300, "mediaType": "layer" }
            ]
        });
        let mut details = ImageDetails {
            repository: "team/image".into(),
            reference: "latest".into(),
            digest: "sha256:index".into(),
            media_type: "index".into(),
            size: 0,
            created: None,
            architecture: None,
            os: None,
            labels: BTreeMap::new(),
            layers: Vec::new(),
            platforms: Vec::new(),
        };
        let mut seen = HashSet::new();

        merge_manifest_blobs(&mut details, &manifest, &mut seen);
        merge_manifest_blobs(&mut details, &manifest, &mut seen);
        merge_config_metadata(
            &mut details,
            &json!({ "created": "2026-01-01T00:00:00Z" }),
            false,
        );
        merge_config_metadata(
            &mut details,
            &json!({ "created": "2026-02-01T00:00:00Z" }),
            false,
        );

        assert_eq!(manifest_content_size(&manifest), 600);
        assert_eq!(details.size, 600);
        assert_eq!(details.layers.len(), 2);
        assert_eq!(details.created.as_deref(), Some("2026-02-01T00:00:00Z"));
    }
}
