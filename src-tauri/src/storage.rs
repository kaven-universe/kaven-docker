use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tauri::{AppHandle, Manager};

pub const APP_ID: &str = "Kaven-Docker";
pub const BUNDLE_IDENTIFIER: &str = "xyz.kaven.app.docker";

static TEMP_INSTANCE: OnceLock<Mutex<Option<TempInstance>>> = OnceLock::new();

struct TempInstance {
    root: PathBuf,
    _lock: File,
}

pub fn root(app: &AppHandle) -> Result<PathBuf, tauri::Error> {
    Ok(data_root(&app.path().local_data_dir()?))
}

fn data_root(local_data: &Path) -> PathBuf {
    local_data.join("Kaven").join("Data").join(APP_ID)
}

pub fn webview_root(app: &AppHandle) -> Result<PathBuf, tauri::Error> {
    Ok(root(app)?.join("webview"))
}

pub fn initialize_temp_instance(app: &AppHandle) -> Result<(), String> {
    let temp_root = root(app).map_err(|error| error.to_string())?.join("tmp");
    fs::create_dir_all(&temp_root).map_err(|error| error.to_string())?;
    cleanup_stale_instances(&temp_root).map_err(|error| error.to_string())?;

    let instance = create_temp_instance(&temp_root).map_err(|error| error.to_string())?;
    let mut active = TEMP_INSTANCE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "temporary directory lock is poisoned".to_string())?;
    if active.is_some() {
        return Err("temporary directory was initialized more than once".into());
    }
    *active = Some(instance);
    Ok(())
}

pub fn temporary_root() -> Result<PathBuf, String> {
    TEMP_INSTANCE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "temporary directory lock is poisoned".to_string())?
        .as_ref()
        .map(|instance| instance.root.clone())
        .ok_or_else(|| "temporary directory is not initialized".to_string())
}

pub fn cleanup_temp_instance() {
    let Some(active) = TEMP_INSTANCE.get() else {
        return;
    };
    let instance = active.lock().ok().and_then(|mut value| value.take());
    if let Some(instance) = instance {
        let path = instance.root.clone();
        drop(instance);
        let _ = remove_tree_without_reparse_points(&path);
    }
}

fn create_temp_instance(temp_root: &Path) -> io::Result<TempInstance> {
    let instance_id = uuid::Uuid::new_v4();
    let root = temp_root.join(format!("instance-{}-{instance_id}", std::process::id()));
    fs::create_dir(&root)?;
    let lock_path = root.join("owner.lock");
    let lock = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(lock_path)?;
    FileExt::try_lock_exclusive(&lock)?;
    Ok(TempInstance { root, _lock: lock })
}

fn cleanup_stale_instances(temp_root: &Path) -> io::Result<()> {
    let entries = fs::read_dir(temp_root)?.collect::<Result<Vec<_>, _>>()?;
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(".cleanup-") {
            if safe_directory(&path)? {
                remove_tree_without_reparse_points(&path)?;
            }
            continue;
        }
        if !name.to_string_lossy().starts_with("instance-") || !safe_directory(&path)? {
            continue;
        }

        let lock_path = path.join("owner.lock");
        let lock = match OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(lock_path)
        {
            Ok(lock) => lock,
            Err(_) => continue,
        };
        if FileExt::try_lock_exclusive(&lock).is_err() {
            continue;
        }

        let cleanup_path = temp_root.join(format!(".cleanup-{}", uuid::Uuid::new_v4()));
        FileExt::unlock(&lock)?;
        drop(lock);
        if fs::rename(&path, &cleanup_path).is_err() {
            continue;
        }
        remove_tree_without_reparse_points(&cleanup_path)?;
    }
    Ok(())
}

fn remove_tree_without_reparse_points(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if !safe_directory(path)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "refusing to remove a temporary directory through a reparse point",
        ));
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        let metadata = fs::symlink_metadata(&child)?;
        if is_reparse_point(&metadata) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "refusing to remove a temporary entry through a reparse point",
            ));
        }
        if metadata.is_dir() {
            remove_tree_without_reparse_points(&child)?;
        } else {
            fs::remove_file(child)?;
        }
    }
    fs::remove_dir(path)
}

fn safe_directory(path: &Path) -> io::Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    Ok(metadata.is_dir() && !is_reparse_point(&metadata))
}

#[cfg(target_os = "windows")]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(target_os = "windows"))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
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
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACEFILE_WRITE_THROUGH,
    };

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let destination_exists = destination.exists();
    let source = wide(source);
    let destination = wide(destination);
    let replaced = unsafe {
        if destination_exists {
            ReplaceFileW(
                destination.as_ptr(),
                source.as_ptr(),
                ptr::null(),
                REPLACEFILE_WRITE_THROUGH,
                ptr::null(),
                ptr::null(),
            )
        } else {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
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

    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("kaven-docker-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn creates_and_removes_an_instance_owned_temporary_directory() {
        let temp_root = test_directory("temp-instance");
        fs::create_dir_all(&temp_root).expect("temporary root should be created");
        let instance = create_temp_instance(&temp_root).expect("instance should be created");
        assert!(instance.root.join("owner.lock").exists());
        let root = instance.root.clone();
        drop(instance);
        remove_tree_without_reparse_points(&root).expect("instance should be removed");
        assert!(!root.exists());
        fs::remove_dir(temp_root).expect("temporary root should be removable");
    }

    #[test]
    fn cleans_an_unlocked_stale_temporary_instance() {
        let temp_root = test_directory("stale-temp-instance");
        fs::create_dir_all(&temp_root).expect("temporary root should be created");
        let stale = create_temp_instance(&temp_root).expect("stale instance should be created");
        let stale_root = stale.root.clone();
        drop(stale);

        cleanup_stale_instances(&temp_root).expect("stale cleanup should succeed");

        assert!(!stale_root.exists());
        fs::remove_dir(temp_root).expect("temporary root should be removable");
    }

    #[test]
    fn atomically_creates_and_replaces_a_file() {
        let directory = test_directory("storage-test");
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
