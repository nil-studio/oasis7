//! Stable kernel exclusion for the directory owner protocol. Never unlink this file.
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::registration_replay_lock::LOCK_RETRY_INTERVAL;

#[cfg(test)]
pub(super) static FAIL_NEXT_ANCHOR_SYNC: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Normalize ordinary directory syntax and resolve parent aliases before suffixing.
/// The target may not exist yet (world bootstrap); its parent is the stable namespace.
pub(super) fn normalized_target(path: &Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            use std::os::windows::ffi::OsStrExt;
            let units: Vec<u16> = name.encode_wide().collect();
            if units
                .last()
                .is_some_and(|c| *c == b'.' as u16 || *c == b' ' as u16)
                || units.contains(&(b':' as u16))
            {
                return Err("registration replay lock ambiguous Windows path refused".into());
            }
        }
    }
    let normalized: PathBuf = path.components().collect();
    let name = normalized
        .file_name()
        .ok_or_else(|| "registration replay lock target has no file name".to_string())?;
    let parent = normalized
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|err| format!("create registration replay lock parent failed: {err}"))?;
    let parent = fs::canonicalize(parent)
        .map_err(|err| format!("resolve registration replay lock parent failed: {err}"))?;
    let target = parent.join(name);
    match fs::symlink_metadata(&target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err("registration replay lock target symlink refused".into());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.is_file() && metadata.nlink() != 1 {
                    return Err("registration replay lock target hardlink refused".into());
                }
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err("registration replay lock target reparse point refused".into());
                }
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(format!(
                "inspect registration replay lock target failed: {err}"
            ));
        }
    }
    #[cfg(windows)]
    if target.exists() {
        // Resolve existing 8.3 short names and case aliases to the same anchor.
        return fs::canonicalize(&target)
            .map_err(|err| format!("resolve registration replay lock target failed: {err}"));
    }
    Ok(target)
}

pub(super) fn acquire(lock_dir: &Path, deadline: Instant, wait: bool) -> Result<File, String> {
    if !cfg!(any(target_os = "linux", target_os = "macos", windows)) {
        return Err("registration replay kernel lock unsupported on this platform".into());
    }
    let mut name = lock_dir.as_os_str().to_owned();
    name.push(".guard");
    let path = PathBuf::from(name);
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        // Deny delete sharing so an open anchor cannot be renamed or replaced.
        options
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    let file = options
        .open(&path)
        .map_err(|err| format!("open registration replay kernel lock failed: {err}"))?;
    verify_anchor(&file, &path)?;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if wait && Instant::now() < deadline => {
                std::thread::sleep(LOCK_RETRY_INTERVAL);
            }
            Err(TryLockError::WouldBlock) => {
                return Err("registration replay ledger lock is held by a live process".into());
            }
            Err(TryLockError::Error(err)) => {
                return Err(format!(
                    "acquire registration replay kernel lock failed: {err}"
                ));
            }
        }
    }
    verify_anchor(&file, &path)?;
    #[cfg(test)]
    if FAIL_NEXT_ANCHOR_SYNC.swap(false, std::sync::atomic::Ordering::SeqCst) {
        return Err("injected kernel lock sync failure".into());
    }
    file.sync_all()
        .and_then(|_| {
            super::atomic_file::sync_parent_directory(
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new(".")),
            )
        })
        .map_err(|err| format!("sync registration replay kernel lock failed: {err}"))?;
    Ok(file)
}

fn verify_anchor(file: &File, path: &Path) -> Result<(), String> {
    let opened = file
        .metadata()
        .map_err(|err| format!("inspect kernel lock failed: {err}"))?;
    let named = fs::symlink_metadata(path)
        .map_err(|err| format!("inspect kernel lock path failed: {err}"))?;
    if !opened.is_file() || !named.is_file() || named.file_type().is_symlink() {
        return Err("registration replay kernel lock must be a regular non-symlink file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.dev() != named.dev()
            || opened.ino() != named.ino()
            || opened.nlink() != 1
            || opened.uid() != unsafe { libc::geteuid() }
            || opened.mode() & 0o077 != 0
        {
            return Err(
                "registration replay kernel lock identity or private permissions invalid".into(),
            );
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if opened.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || named.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err("registration replay kernel lock reparse point refused".into());
        }
    }
    Ok(())
}
