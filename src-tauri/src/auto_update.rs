use crate::{app_error, runtime_state};
use semver::Version;
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tauri::{ipc::Channel, AppHandle, Url};
use tauri_plugin_updater::{Update, UpdaterExt};
use updater_reqwest::redirect::Policy;

const UPDATE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CANONICAL_MANIFEST_URL: &str = "https://public.data.kaven.xyz/json/kaven-docker/latest.json";
static UPDATE_CHECK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

struct UpdateCheckGuard;

impl UpdateCheckGuard {
    fn acquire() -> Option<Self> {
        UPDATE_CHECK_IN_PROGRESS
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}

impl Drop for UpdateCheckGuard {
    fn drop(&mut self) {
        UPDATE_CHECK_IN_PROGRESS.store(false, Ordering::Release);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current_version: String,
    latest_version: String,
    update_available: bool,
    download_url: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
pub enum UpdateEvent {
    Checking,
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
}

fn parse_version(value: &str) -> Result<Version, semver::Error> {
    Version::parse(value.trim_start_matches('v'))
}

fn normalized_version(value: &str) -> Result<Version, String> {
    parse_version(value).map_err(|error| app_error("updateInstall", error))
}

fn update_info(
    current_version: &str,
    latest_version: &str,
    download_url: String,
) -> Result<UpdateInfo, String> {
    let current_version =
        parse_version(current_version).map_err(|error| app_error("updateCheck", error))?;
    let latest_version =
        parse_version(latest_version).map_err(|error| app_error("updateCheck", error))?;
    Ok(UpdateInfo {
        update_available: latest_version > current_version,
        current_version: current_version.to_string(),
        latest_version: latest_version.to_string(),
        download_url,
    })
}

fn platform_manifest<'a>(manifest: &'a Value, selected_url: &Url) -> Option<&'a Value> {
    match manifest.get("platforms") {
        Some(Value::Object(platforms)) => platforms.values().find(|platform| {
            platform
                .get("url")
                .and_then(Value::as_str)
                .and_then(|value| Url::parse(value).ok())
                .is_some_and(|url| url == *selected_url)
        }),
        _ => Some(manifest),
    }
}

fn download_urls(manifest: &Value, selected_url: &Url) -> Result<Vec<Url>, String> {
    let Some(urls) = platform_manifest(manifest, selected_url).and_then(|value| value.get("urls"))
    else {
        return Ok(vec![selected_url.clone()]);
    };
    let urls = urls
        .as_array()
        .ok_or_else(|| "the update manifest 'urls' field must be an array".to_owned())?;

    let mut parsed = Vec::with_capacity(urls.len() + 1);
    for value in urls {
        let value = value
            .as_str()
            .ok_or_else(|| "every update address must be a string".to_owned())?;
        let url = Url::parse(value)
            .map_err(|error| format!("an update download address is invalid: {error}"))?;
        if !parsed.contains(&url) {
            parsed.push(url);
        }
    }
    if !parsed.contains(selected_url) {
        parsed.push(selected_url.clone());
    }
    Ok(parsed)
}

fn download_failure_summary(error: &str) -> &'static str {
    let error = error.to_ascii_lowercase();
    if error.contains("signature") || error.contains("minisign") {
        "artifact signature verification failed"
    } else if error.contains("status") || error.contains("http") {
        "the update server returned an unsuccessful response"
    } else {
        "the artifact download failed"
    }
}

fn redirect_allowed(previous: &[updater_reqwest::Url], next: &updater_reqwest::Url) -> bool {
    let checking_canonical_manifest = previous
        .first()
        .is_some_and(|url| url.as_str() == CANONICAL_MANIFEST_URL);
    !checking_canonical_manifest
        || (next.scheme() == "https" && next.host_str() == Some("public.data.kaven.xyz"))
}

fn configure_update_client(
    client: updater_reqwest::ClientBuilder,
) -> updater_reqwest::ClientBuilder {
    client.redirect(Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.stop()
        } else if redirect_allowed(attempt.previous(), attempt.url()) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    }))
}

#[tauri::command(async)]
pub async fn check_for_updates(
    app: AppHandle,
    automatic: bool,
) -> Result<Option<UpdateInfo>, String> {
    if automatic && !runtime_state::automatic_update_check_due(&app)? {
        return Ok(None);
    }
    let Some(_guard) = UpdateCheckGuard::acquire() else {
        if automatic {
            return Ok(None);
        }
        return Err(app_error(
            "updateCheck",
            "another update check is already running",
        ));
    };

    let result = async {
        let update = app
            .updater_builder()
            .timeout(UPDATE_REQUEST_TIMEOUT)
            .configure_client(configure_update_client)
            .version_comparator(|_, _| true)
            .build()
            .map_err(|error| app_error("updateCheck", error))?
            .check()
            .await
            .map_err(|error| app_error("updateCheck", error))?
            .ok_or_else(|| app_error("updateCheck", "the signed manifest has no release"))?;
        let urls = download_urls(&update.raw_json, &update.download_url)
            .map_err(|error| app_error("updateCheck", error))?;
        update_info(
            &update.current_version,
            &update.version,
            urls[0].to_string(),
        )
        .map(Some)
    }
    .await;

    if automatic {
        runtime_state::record_update_check(&app, result.is_ok())?;
    }
    result
}

#[tauri::command(async)]
pub async fn install_update(
    app: AppHandle,
    expected_version: String,
    on_event: Channel<UpdateEvent>,
) -> Result<(), String> {
    let _ = on_event.send(UpdateEvent::Checking);
    let update = app
        .updater_builder()
        .timeout(UPDATE_REQUEST_TIMEOUT)
        .configure_client(configure_update_client)
        .build()
        .map_err(|error| app_error("updateInstall", error))?
        .check()
        .await
        .map_err(|error| app_error("updateInstall", error))?
        .ok_or_else(|| app_error("updateInstall", "the signed manifest has no newer release"))?;

    if normalized_version(&update.version)? != normalized_version(&expected_version)? {
        return Err(app_error(
            "updateInstall",
            format!(
                "the signed manifest announces version {}, but version {} was expected",
                update.version, expected_version
            ),
        ));
    }

    let urls = download_urls(&update.raw_json, &update.download_url)
        .map_err(|error| app_error("updateInstall", error))?;
    let mut failures = Vec::new();
    for url in urls {
        let mut candidate: Update = update.clone();
        candidate.download_url = url.clone();
        let progress_channel = on_event.clone();
        let mut started = false;
        match candidate
            .download(
                move |chunk_length, content_length| {
                    if !started {
                        let _ = progress_channel.send(UpdateEvent::Started { content_length });
                        started = true;
                    }
                    let _ = progress_channel.send(UpdateEvent::Progress { chunk_length });
                },
                || {},
            )
            .await
        {
            Ok(bytes) => {
                let installation = crate::begin_update_installation()?;
                let _ = on_event.send(UpdateEvent::Finished);
                if let Err(error) = candidate.install(&bytes) {
                    return Err(app_error("updateInstall", error));
                }
                std::mem::forget(installation);

                #[cfg(not(windows))]
                app.restart();
                return Ok(());
            }
            Err(error) => failures.push(format!(
                "attempt {} via {}: {}",
                failures.len() + 1,
                url.host_str().unwrap_or("unknown update host"),
                download_failure_summary(&error.to_string())
            )),
        }
    }

    Err(app_error(
        "updateInstall",
        format!(
            "every update download address failed: {}",
            failures.join("; ")
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        download_failure_summary, download_urls, normalized_version, redirect_allowed, update_info,
        CANONICAL_MANIFEST_URL,
    };
    use serde_json::json;
    use tauri::Url;

    #[test]
    fn accepts_versions_with_or_without_a_v_prefix() {
        assert_eq!(
            normalized_version("v1.2.3").unwrap(),
            normalized_version("1.2.3").unwrap()
        );
    }

    #[test]
    fn compares_manifest_and_installed_versions() {
        let available =
            update_info("1.0.0", "1.1.0", "https://example.test/app.exe".to_owned()).unwrap();
        assert!(available.update_available);
        assert_eq!(available.latest_version, "1.1.0");

        let current = update_info("1.1.0", "v1.1.0", available.download_url).unwrap();
        assert!(!current.update_available);
    }

    #[test]
    fn reads_ordered_download_urls_from_a_static_manifest() {
        let selected = Url::parse("https://one.test/setup.exe").unwrap();
        let manifest = json!({
            "platforms": {
                "windows-x86_64-nsis": {
                    "url": "https://ONE.test/setup.exe",
                    "urls": [
                        "https://one.test/setup.exe",
                        "https://two.test/setup.exe",
                        "https://one.test/setup.exe"
                    ],
                    "signature": "signed"
                }
            }
        });
        let urls = download_urls(&manifest, &selected).unwrap();

        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].host_str(), Some("one.test"));
        assert_eq!(urls[1].host_str(), Some("two.test"));
    }

    #[test]
    fn keeps_the_standard_url_as_a_fallback_without_restricting_artifact_hosts() {
        let selected = Url::parse("https://one.test/setup.exe").unwrap();
        let manifest = json!({
            "url": selected,
            "urls": [
                "https://two.test/setup.exe",
                "http://distribution.test/setup.exe"
            ],
            "signature": "signed"
        });
        let urls = download_urls(&manifest, &selected).unwrap();
        assert_eq!(urls[1].host_str(), Some("distribution.test"));
        assert_eq!(urls[2], selected);
    }

    #[test]
    fn summarizes_download_failures_without_repeating_sensitive_urls() {
        assert_eq!(
            download_failure_summary(
                "request to https://example.test/setup.exe?token=secret returned status 503"
            ),
            "the update server returned an unsuccessful response"
        );
        assert_eq!(
            download_failure_summary("minisign verification failed"),
            "artifact signature verification failed"
        );
    }

    #[test]
    fn keeps_manifest_redirects_on_the_canonical_host_without_restricting_artifacts() {
        let manifest = updater_reqwest::Url::parse(CANONICAL_MANIFEST_URL).unwrap();
        let release = updater_reqwest::Url::parse(
            "https://public.data.kaven.xyz/json/kaven-docker/1.0.0.json",
        )
        .unwrap();
        let foreign = updater_reqwest::Url::parse("https://example.test/latest.json").unwrap();
        assert!(redirect_allowed(std::slice::from_ref(&manifest), &release));
        assert!(!redirect_allowed(std::slice::from_ref(&manifest), &foreign));

        let artifact = updater_reqwest::Url::parse("http://downloads.test/setup.exe").unwrap();
        assert!(redirect_allowed(std::slice::from_ref(&artifact), &foreign));
    }
}
