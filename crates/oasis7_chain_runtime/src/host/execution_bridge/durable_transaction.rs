use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);

/// Synchronize a verified existing blob and its directory entry. The caller
/// must create its directory through ensure_dir_all_durable before publication.
pub(super) fn sync_existing_file_durable(path: &Path) -> Result<(), String> {
    sync_tree(path)?;
    sync_directory(parent_dir(path)?)
}

pub(super) fn write_file_durable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_file_durable_with_mode(path, bytes, false)
}

/// Atomically publish private protocol state with mode 0600 applied by the
/// kernel during temporary-file creation, before any bytes are written.
pub(super) fn write_private_file_durable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_file_durable_with_mode(path, bytes, true)
}

fn write_file_durable_with_mode(path: &Path, bytes: &[u8], private: bool) -> Result<(), String> {
    let parent = parent_dir(path)?;
    ensure_dir_all_durable(parent)?;
    let temp = unique_sibling(path, "tmp");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        if private {
            options.mode(0o600).open(temp.as_path())
        } else {
            options.open(temp.as_path())
        }
    }
    .map_err(|err| format!("create durable temp {} failed: {err}", temp.display()))?;
    #[cfg(not(unix))]
    let _ = private;
    #[cfg(not(unix))]
    let mut file = options
        .open(temp.as_path())
        .map_err(|err| format!("create durable temp {} failed: {err}", temp.display()))?;
    if let Err(err) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(temp.as_path());
        return Err(format!(
            "write durable temp {} failed: {err}",
            temp.display()
        ));
    }
    replace_file(temp.as_path(), path)?;
    sync_directory(parent)
}

pub(super) fn ensure_dir_all_durable(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }

    let mut missing = Vec::new();
    let mut current = path.to_path_buf();
    loop {
        match fs::symlink_metadata(current.as_path()) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(format!(
                        "durable directory path is not a directory: {}",
                        current.display()
                    ));
                }
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current.clone());
            }
            Err(err) => {
                return Err(format!(
                    "inspect durable directory {} failed: {err}",
                    current.display()
                ));
            }
        }

        current = match current.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
    }

    let mut created = Vec::with_capacity(missing.len());
    for directory in missing.iter().rev() {
        match fs::create_dir(directory.as_path()) {
            Ok(()) => created.push(directory.clone()),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(directory.as_path()).map_err(|inspect_err| {
                    format!(
                        "inspect concurrently created durable directory {} failed: {inspect_err}",
                        directory.display()
                    )
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(format!(
                        "concurrently created durable path is not a directory: {}",
                        directory.display()
                    ));
                }
                created.push(directory.clone());
            }
            Err(err) => {
                return Err(format!(
                    "create durable directory {} failed: {err}",
                    directory.display()
                ));
            }
        }
    }

    for directory in created.iter().rev() {
        sync_directory(directory.as_path())?;
        sync_directory(directory_parent_for_sync(directory.as_path()))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, path: &Path) -> Result<(), String> {
    fs::rename(temp, path).map_err(|err| {
        format!(
            "replace durable file {} -> {} failed: {err}",
            temp.display(),
            path.display()
        )
    })
}

#[cfg(windows)]
fn replace_file(temp: &Path, path: &Path) -> Result<(), String> {
    if !path.exists() {
        return fs::rename(temp, path).map_err(|err| {
            format!(
                "publish durable file {} -> {} failed: {err}",
                temp.display(),
                path.display()
            )
        });
    }
    replace_existing_file_windows(temp, path)
}

#[cfg(windows)]
fn replace_existing_file_windows(temp: &Path, path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
    }

    let replaced: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let replaced_ok = unsafe {
        ReplaceFileW(
            replaced.as_ptr(),
            replacement.as_ptr(),
            ptr::null(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if replaced_ok == 0 {
        return Err(format!(
            "replace durable file {} -> {} failed: {}",
            temp.display(),
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub(super) fn remove_file_durable(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => sync_directory(parent_dir(path)?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("remove {} failed: {err}", path.display())),
    }
}

pub(super) fn rename_durable(from: &Path, to: &Path) -> Result<(), String> {
    let from_parent = parent_dir(from)?;
    let to_parent = parent_dir(to)?;
    fs::rename(from, to).map_err(|err| {
        format!(
            "rename {} -> {} failed: {err}",
            from.display(),
            to.display()
        )
    })?;
    sync_directory(from_parent)?;
    if to_parent != from_parent {
        sync_directory(to_parent)?;
    }
    Ok(())
}

pub(super) fn remove_dir_all_durable(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => sync_directory(parent_dir(path)?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("remove directory {} failed: {err}", path.display())),
    }
}

pub(super) fn sync_tree(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| format!("read metadata {} failed: {err}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refuse to sync symlink in durable transaction: {}",
            path.display()
        ));
    }
    if metadata.is_file() {
        return File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|err| format!("sync file {} failed: {err}", path.display()));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "unsupported durable transaction entry: {}",
            path.display()
        ));
    }
    for entry in fs::read_dir(path)
        .map_err(|err| format!("read directory {} failed: {err}", path.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "read directory entry under {} failed: {err}",
                path.display()
            )
        })?;
        sync_tree(entry.path().as_path())?;
    }
    sync_directory(path)
}

pub(super) fn unique_sibling(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("transaction");
    path.with_file_name(format!(
        ".{name}.checkpoint-install-{}.{}",
        unique_token(),
        suffix
    ))
}

pub(super) fn unique_token() -> String {
    let unique = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_else(|err| err.duration().as_nanos());
    format!("{}-{timestamp}-{unique}", std::process::id())
}

fn parent_dir(path: &Path) -> Result<&Path, String> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| format!("path {} has no parent directory", path.display()))
}

fn directory_parent_for_sync(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn sync_directory(path: &Path) -> Result<(), String> {
    open_directory_for_sync(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|err| format!("sync directory {} failed: {err}", path.display()))
}

#[cfg(unix)]
fn open_directory_for_sync(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

#[cfg(windows)]
fn open_directory_for_sync(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

#[cfg(not(any(unix, windows)))]
fn open_directory_for_sync(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_nested_directory_creation_is_idempotent() {
        let root =
            std::env::temp_dir().join(format!("oasis7-durable-nested-dir-{}", unique_token()));
        let nested = root.join("checkpoints").join("42");

        ensure_dir_all_durable(nested.as_path()).expect("create nested directories durably");
        ensure_dir_all_durable(nested.as_path()).expect("repeat durable directory creation");

        assert!(nested.is_dir());
        let manifest = nested.join("manifest.json");
        write_file_durable(manifest.as_path(), b"manifest").expect("publish nested manifest");
        assert_eq!(fs::read(manifest).expect("read manifest"), b"manifest");

        fs::remove_dir_all(root).expect("remove durable nested directory test root");
    }

    #[test]
    fn unique_tokens_include_time_nonce_and_distinguish_siblings() {
        let target = Path::new("checkpoint-install-transaction.json");
        let first_token = unique_token();
        let second_token = unique_token();
        let first_sibling = unique_sibling(target, "tmp");
        let second_sibling = unique_sibling(target, "tmp");

        for token in [&first_token, &second_token] {
            let components: Vec<_> = token.split('-').collect();
            assert_eq!(components.len(), 3, "token must contain pid, time, counter");
            assert!(components.iter().all(|component| {
                !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
            }));
        }
        assert_ne!(first_token, second_token);
        assert_ne!(first_sibling, second_sibling);
    }

    #[test]
    fn durable_file_repeatedly_replaces_marker_and_restores_previous_bytes() {
        let root =
            std::env::temp_dir().join(format!("oasis7-durable-file-replace-{}", unique_token()));
        let marker = root.join("checkpoint-install-transaction.json");
        write_file_durable(marker.as_path(), br#"{"phase":"Prepared"}"#)
            .expect("persist Prepared marker");
        write_file_durable(marker.as_path(), br#"{"phase":"Committed"}"#)
            .expect("replace with Committed marker");
        assert_eq!(
            fs::read(marker.as_path()).expect("read Committed marker"),
            br#"{"phase":"Committed"}"#
        );
        write_file_durable(marker.as_path(), b"previous publication")
            .expect("restore rollback publication");
        assert_eq!(
            fs::read(marker.as_path()).expect("read restored publication"),
            b"previous publication"
        );
        fs::remove_dir_all(root).expect("remove durable replacement test root");
    }

    #[cfg(unix)]
    #[test]
    fn private_durable_file_is_mode_0600_on_publication_and_replacement() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("oasis7-durable-private-file-{}", unique_token()));
        ensure_dir_all_durable(root.as_path()).expect("create durable private test root");
        let path = root.join("formal-private.cbor");
        write_private_file_durable(path.as_path(), b"first")
            .expect("publish private protocol file");
        assert_eq!(
            fs::metadata(path.as_path())
                .expect("read private file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        write_private_file_durable(path.as_path(), b"replacement")
            .expect("replace private protocol file");
        assert_eq!(
            fs::metadata(path.as_path())
                .expect("read replacement metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(fs::read(path.as_path()).unwrap(), b"replacement");
        fs::remove_dir_all(root).expect("remove durable private test root");
    }
}
