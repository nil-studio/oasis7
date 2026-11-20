use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

const MAX_JSON: u64 = 4 * 1024 * 1024;

pub(crate) struct WriterLock(File);
impl WriterLock {
    pub(crate) fn try_lock(file: File) -> Result<Self, std::fs::TryLockError> {
        file.try_lock()?;
        Ok(Self(file))
    }
}
impl Drop for WriterLock {
    fn drop(&mut self) {
        // A forked child can retain the description before exec. End this
        // writer's critical section explicitly; process lifetime locks differ.
        let _ = self.0.unlock();
    }
}

pub(crate) fn reject_symlinks(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err("parent-directory path components are unsupported".into());
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!("symlink refused: {}", current.display()));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
pub(crate) fn regular(path: &Path) -> Result<PathBuf, String> {
    reject_symlinks(path)?;
    if !fs::metadata(path).map_err(|e| e.to_string())?.is_file() {
        return Err(format!("not a regular file: {}", path.display()));
    }
    fs::canonicalize(path).map_err(|e| e.to_string())
}
pub(crate) fn private_dir(path: &Path) -> Result<(), String> {
    reject_symlinks(path)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => {
            sync_dir(path)?;
            sync_dir(path.parent().ok_or("directory parent missing")?)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    super::super::controlled_live_config::require_private_owned(path, true)
}
pub(crate) fn private_file(path: &Path) -> Result<File, String> {
    reject_symlinks(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path).map_err(|e| e.to_string())?;
    super::super::controlled_live_config::require_private_owned(path, false)?;
    Ok(file)
}
// The path preflight is not atomic with open. Recheck the opened descriptor
// after refusing replacement leaf symlinks and nonregular files.
fn open_checked_regular(path: &Path) -> Result<File, String> {
    open_checked_regular_with(path, || {})
}

fn open_checked_regular_with(path: &Path, after_preflight: impl FnOnce()) -> Result<File, String> {
    let path = regular(path)?;
    after_preflight();
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options.open(&path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!("not a regular file: {}", path.display()));
    }
    Ok(file)
}

pub(crate) fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    read_bounded_json(path, MAX_JSON)
}
pub(super) fn read_bounded_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T, String> {
    let limit_plus_probe = limit.checked_add(1).ok_or("invalid node metadata size limit")?;
    let file = open_checked_regular(path)?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err("node metadata size limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit_plus_probe)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("node metadata size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub(crate) fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    write_bounded_json(path, value, MAX_JSON)
}
pub(super) fn write_bounded_json<T: Serialize>(path: &Path, value: &T, limit: u64) -> Result<(), String> {
    reject_symlinks(path)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("node metadata size limit".into());
    }
    let parent = path.parent().ok_or("node metadata parent missing")?;
    let temporary = parent.join(format!(".node-write-{}", random_id()?));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        sync_dir(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
pub(crate) fn sha256(path: &Path) -> Result<String, String> {
    let mut file = open_checked_regular(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
pub(crate) fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(|e| e.to_string())?,
    )))
}
pub(crate) fn random_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(hex::encode(bytes))
}
pub(crate) fn sync_dir(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}

#[cfg(all(test, unix))]
mod regular_open_tests {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::{ffi::OsStrExt, fs::symlink};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_symlink_and_fifo_swaps_after_path_preflight() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!(
            "oasis7-node-read-open-{}-{nonce}", std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let original = root.join("operation.json");
        let replacement = root.join("replacement.json");
        fs::write(&original, b"{}").unwrap();
        fs::write(&replacement, b"{\"untrusted\":true}").unwrap();

        let err = open_checked_regular_with(&original, || {
            fs::remove_file(&original).unwrap();
            symlink(&replacement, &original).unwrap();
        })
        .err()
        .expect("a symlink replacement after preflight must be rejected");
        assert!(
            err.contains("symbolic") || err.contains("symlink")
                || err.contains("Too many levels"),
            "unexpected error: {err}"
        );

        fs::remove_file(&original).unwrap();
        fs::write(&original, b"{}").unwrap();
        let err = open_checked_regular_with(&original, || {
            fs::remove_file(&original).unwrap();
            let path = CString::new(original.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        })
        .err()
        .expect("a FIFO substituted without a writer must not block");
        assert!(err.contains("not a regular file"), "unexpected error: {err}");

        fs::remove_file(&original).unwrap();
        fs::write(&original, b"{}").unwrap();
        let mut file = open_checked_regular(&original).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "{}");
    }
}
