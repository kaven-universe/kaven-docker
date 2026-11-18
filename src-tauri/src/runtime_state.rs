use crate::{app_error, storage};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

const STATE_VERSION: u32 = 2;
const MAX_STATE_BYTES: u64 = 64 * 1024;
const UPDATE_CHECK_INTERVAL_SECONDS: i64 = 24 * 60 * 60;
const INITIAL_RETRY_SECONDS: i64 = 15 * 60;
const MAX_RETRY_SECONDS: i64 = 6 * 60 * 60;

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeState {
    version: u32,
    last_successful_update_check_at: Option<i64>,
    next_automatic_update_retry_at: Option<i64>,
    consecutive_update_check_failures: u32,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            version: STATE_VERSION,
            ..Self::default()
        }
    }

    fn automatic_update_check_due(&self, now: i64) -> bool {
        let interval_elapsed = self
            .last_successful_update_check_at
            .is_none_or(|last| now >= last.saturating_add(UPDATE_CHECK_INTERVAL_SECONDS));
        let retry_elapsed = self
            .next_automatic_update_retry_at
            .is_none_or(|retry| now >= retry);
        interval_elapsed && retry_elapsed
    }

    fn record_update_check(&mut self, now: i64, succeeded: bool) {
        if succeeded {
            self.last_successful_update_check_at = Some(now);
            self.next_automatic_update_retry_at = None;
            self.consecutive_update_check_failures = 0;
            return;
        }

        self.consecutive_update_check_failures =
            self.consecutive_update_check_failures.saturating_add(1);
        let exponent = self
            .consecutive_update_check_failures
            .saturating_sub(1)
            .min(5);
        let retry_seconds = (INITIAL_RETRY_SECONDS * (1_i64 << exponent)).min(MAX_RETRY_SECONDS);
        self.next_automatic_update_retry_at = Some(now.saturating_add(retry_seconds));
    }
}

fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    storage::root(app)
        .map(|directory| directory.join("state.json"))
        .map_err(|error| app_error("updateStatePath", error))
}

fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn diagnostic_path(path: &Path, now: i64) -> PathBuf {
    path.with_file_name(format!("state.invalid-{now}.json"))
}

fn preserve_invalid_state(path: &Path, now: i64) {
    if path.exists() {
        let _ = fs::copy(path, diagnostic_path(path, now));
    }
}

fn load_from(path: &Path, now: i64) -> RuntimeState {
    let content = match fs::read(path) {
        Ok(content) if content.len() as u64 <= MAX_STATE_BYTES => content,
        Ok(_) => {
            preserve_invalid_state(path, now);
            return RuntimeState::new();
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return RuntimeState::new(),
        Err(_) => {
            preserve_invalid_state(path, now);
            return RuntimeState::new();
        }
    };

    match serde_json::from_slice::<RuntimeState>(&content) {
        Ok(state) if state.version == STATE_VERSION => state,
        _ => {
            preserve_invalid_state(path, now);
            RuntimeState::new()
        }
    }
}

fn save_to(path: &Path, state: &RuntimeState) -> io::Result<()> {
    let mut content = serde_json::to_vec_pretty(state)?;
    content.push(b'\n');
    storage::write_atomic(path, &content)
}

pub fn automatic_update_check_due(app: &AppHandle) -> Result<bool, String> {
    let now = unix_timestamp_now();
    Ok(load_from(&state_path(app)?, now).automatic_update_check_due(now))
}

pub fn record_update_check(app: &AppHandle, succeeded: bool) -> Result<(), String> {
    let now = unix_timestamp_now();
    let path = state_path(app)?;
    let mut state = load_from(&path, now);
    state.record_update_check(now, succeeded);
    save_to(&path, &state).map_err(|error| app_error("updateStateSave", error))
}

#[cfg(test)]
mod tests {
    use super::{RuntimeState, STATE_VERSION};

    #[test]
    fn schedules_daily_checks_with_bounded_failure_backoff() {
        let now = 1_795_000_000;
        let mut state = RuntimeState::new();
        assert!(state.automatic_update_check_due(now));

        state.record_update_check(now, false);
        assert!(!state.automatic_update_check_due(now + 14 * 60));
        assert!(state.automatic_update_check_due(now + 15 * 60));

        for offset in 1..10 {
            state.record_update_check(now + offset * 60, false);
        }
        assert!(state.next_automatic_update_retry_at.unwrap() <= now + 9 * 60 + 6 * 60 * 60);

        state.record_update_check(now + 15 * 60, true);
        assert!(!state.automatic_update_check_due(now + 23 * 60 * 60));
        assert!(state.automatic_update_check_due(now + 25 * 60 * 60));
        assert_eq!(state.version, STATE_VERSION);
    }
}
