use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn root(app: &AppHandle) -> Result<PathBuf, tauri::Error> {
    Ok(app
        .path()
        .local_data_dir()?
        .join("Kaven")
        .join("kaven-docker"))
}
