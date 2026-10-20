//! Untrusted public proof cache, never a source of external authority pins.
use oasis7::world_service::projection::WorldServiceProjection;
use oasis7_client_api::world_service::{FormalReadCursorBundleV1, MAX_FORMAL_CURSOR_BYTES};
#[cfg(unix)]
use std::io::Write;
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};
type Bundle = FormalReadCursorBundleV1<WorldServiceProjection>;
#[cfg(unix)]
fn no_follow(location: &Path, directory: bool) -> Result<File, String> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };
    path(location)?;
    let root = CString::new("/").unwrap();
    // Each openat retains the verified directory descriptor, so replacing an
    // earlier path component cannot redirect this traversal through a link.
    // SAFETY: root is a live NUL-terminated string; flags request a directory.
    let fd = unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: the successful open returned a uniquely owned descriptor.
    let mut current = unsafe { File::from_raw_fd(fd) };
    let parts: Vec<_> = location
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    for (index, name) in parts.iter().enumerate() {
        let name = CString::new(name.as_bytes()).map_err(|_| "cursor path contains NUL")?;
        let last = index + 1 == parts.len();
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | if !last || directory {
                libc::O_DIRECTORY
            } else {
                libc::O_NONBLOCK
            };
        // SAFETY: directory and CString remain live throughout this syscall.
        let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // SAFETY: this successful openat descriptor has no other owner.
        current = unsafe { File::from_raw_fd(fd) };
    }
    Ok(current)
}
#[cfg(not(unix))]
fn no_follow(_location: &Path, _directory: bool) -> Result<File, String> {
    Err("formal cursor files require a platform no-follow implementation".into())
}
fn path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("cursor path must be absolute".into());
    }
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err("cursor parent-directory components refused".into());
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("cursor symlink refused".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}
pub(super) fn read(location: &Path) -> Result<Bundle, String> {
    path(location)?;
    let file = no_follow(location, false)?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("cursor is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FORMAL_CURSOR_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_FORMAL_CURSOR_BYTES {
        return Err("formal cursor byte bound".into());
    }
    let bundle: Bundle = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    bundle
        .validate_structure()
        .map_err(|error| error.to_string())?;
    Ok(bundle)
}
pub(super) fn write(location: &Path, bundle: &Bundle) -> Result<(), String> {
    path(location)?;
    bundle
        .validate_structure()
        .map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(bundle).map_err(|error| error.to_string())?;
    let parent = location.parent().ok_or("cursor parent missing")?;
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| "cursor temporary randomness unavailable")?;
    let temporary = format!(".formal-cursor-{}", hex::encode(random));
    anchored_write(
        parent,
        location.file_name().ok_or("cursor filename missing")?,
        &temporary,
        &bytes,
    )
}
#[cfg(unix)]
fn anchored_write(
    parent: &Path,
    target: &std::ffi::OsStr,
    temporary: &str,
    bytes: &[u8],
) -> Result<(), String> {
    anchored_write_with_parent_sync(parent, target, temporary, bytes, File::sync_all)
}
#[cfg(unix)]
fn anchored_write_with_parent_sync(
    parent: &Path,
    target: &std::ffi::OsStr,
    temporary: &str,
    bytes: &[u8],
    sync_parent: impl FnOnce(&File) -> std::io::Result<()>,
) -> Result<(), String> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };
    let directory = no_follow(parent, true)?;
    let target = CString::new(target.as_bytes()).map_err(|_| "cursor filename contains NUL")?;
    let temporary =
        CString::new(temporary).map_err(|_| "cursor temporary filename contains NUL")?;
    let mut created = false;
    let result = (|| {
        // SAFETY: live directory/CString arguments; O_EXCL creates a new file.
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                temporary.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        created = true;
        // SAFETY: the newly created descriptor is transferred exactly once.
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        // SAFETY: both names and the retained parent descriptor are live.
        if unsafe {
            libc::renameat(
                directory.as_raw_fd(),
                temporary.as_ptr(),
                directory.as_raw_fd(),
                target.as_ptr(),
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        sync_parent(&directory).map_err(|error| format!(
            "cursor replacement executed; durability unknown: {error}; new_file_blake3={}; reopen the target and compare this digest, then replay-verify its proof bundle from CURRENT external trust before use",
            blake3::hash(bytes).to_hex()
        ))?;
        Ok(())
    })();
    if result.is_err() && created {
        // SAFETY: cleanup is relative to the same retained parent descriptor.
        unsafe {
            libc::unlinkat(directory.as_raw_fd(), temporary.as_ptr(), 0);
        }
    }
    result
}

#[cfg(all(test, unix))]
#[path = "membership_cursor_io_tests.rs"]
mod tests;
#[cfg(not(unix))]
fn anchored_write(
    _parent: &Path,
    _target: &std::ffi::OsStr,
    _temporary: &str,
    _bytes: &[u8],
) -> Result<(), String> {
    Err("formal cursor file saving requires a platform no-follow implementation".into())
}
