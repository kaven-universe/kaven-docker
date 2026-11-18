use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

pub fn root(app: &AppHandle) -> Result<PathBuf, tauri::Error> {
    Ok(app
        .path()
        .local_data_dir()?
        .join("Kaven")
        .join("Kaven-Docker"))
}

pub fn migrate_legacy_root(app: &AppHandle) -> Result<(), tauri::Error> {
    let base = app.path().local_data_dir()?.join("Kaven");
    let legacy = base.join("kaven-docker");
    let current = base.join("Kaven-Docker");
    if legacy.exists() && !current.exists() {
        fs::rename(&legacy, &current)?;
    }
    let legacy_cache = current.join("blobs");
    let cache = current.join("cache").join("blobs");
    if legacy_cache.exists() && !cache.exists() {
        fs::create_dir_all(current.join("cache"))?;
        fs::rename(legacy_cache, cache)?;
    }
    Ok(())
}

struct TemporaryFile {
    path: PathBuf,
    keep: bool,
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid destination name"))?;
    let temp_path = parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let mut temporary = TemporaryFile {
        path: temp_path.clone(),
        keep: false,
    };
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)?;
    file.write_all(content)?;
    file.sync_all()?;
    drop(file);

    replace_file(&temp_path, path)?;
    temporary.keep = true;
    sync_directory(parent)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(target_os = "windows")]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    if !destination.exists() {
        return fs::rename(source, destination);
    }

    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination has no parent directory",
        )
    })?;
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid destination name"))?;
    let backup = parent.join(format!(".{file_name}.{}.bak", uuid::Uuid::new_v4()));
    fs::rename(destination, &backup)?;
    match fs::rename(source, destination) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(backup, destination);
            Err(error)
        }
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomically_creates_and_replaces_a_file() {
        let directory = std::env::temp_dir().join(format!(
            "kaven-docker-storage-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).expect("test directory should be created");
        let path = directory.join("config.json");

        write_atomic(&path, b"first\n").expect("initial write should succeed");
        write_atomic(&path, b"second\n").expect("replacement should succeed");

        assert_eq!(
            fs::read(&path).expect("test file should be readable"),
            b"second\n"
        );
        let entries = fs::read_dir(&directory)
            .expect("test directory should be readable")
            .count();
        assert_eq!(entries, 1);
        fs::remove_dir_all(directory).expect("test directory should be removable");
    }
}
