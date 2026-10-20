//! Private staging and kernel-enforced exclusive publication. No overwrite fallback.
#[cfg(unix)]
use std::os::unix::{
    ffi::OsStrExt,
    fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    io::AsRawFd,
};
use std::{
    fs::{self, File, OpenOptions},
    path::{Component, Path, PathBuf},
};

pub(super) fn normalized(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .skip(1)
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("preview paths must be absolute and normalized".into());
    }
    let mut cursor = PathBuf::new();
    for part in path.components() {
        cursor.push(part);
        match fs::symlink_metadata(&cursor) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err("preview path symlink rejected".into());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("preview path metadata unavailable".into()),
        }
    }
    Ok(())
}

pub(super) fn private_dir(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|e| format!("private preview directory: {e}"))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("preview initializer requires Linux or macOS".into())
    }
}

pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    #[cfg(unix)]
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path);
    #[cfg(not(unix))]
    let mut file = OpenOptions::new().write(true).create_new(true).open(path);
    let file = file
        .as_mut()
        .map_err(|_| "preview output already exists or is unavailable")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "preview output persistence failed".into())
}

pub(super) fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    #[cfg(unix)]
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path);
    #[cfg(not(unix))]
    let file = File::open(path);
    let file = file.map_err(|_| "preview artifact unavailable")?;
    if !file
        .metadata()
        .map_err(|_| "preview artifact metadata unavailable")?
        .is_file()
    {
        return Err("preview artifact must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "preview artifact read failed")?;
    if bytes.len() > max {
        return Err("preview artifact exceeds byte bound".into());
    }
    Ok(bytes)
}

#[cfg(unix)]
pub(super) fn verify_private(path: &Path, directory: bool, owner: u32) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|_| "private preview metadata unavailable")?;
    let mode = if directory { 0o700 } else { 0o600 };
    if m.file_type().is_symlink()
        || m.uid() != owner
        || m.mode() & 0o777 != mode
        || (directory && !m.is_dir())
        || (!directory && !m.is_file())
    {
        return Err("private preview type, mode or owner mismatch".into());
    }
    Ok(())
}
#[cfg(not(unix))]
pub(super) fn verify_private(_: &Path, _: bool, _: u32) -> Result<(), String> {
    Err("preview initializer requires Linux or macOS".into())
}

pub(super) struct Staging {
    pub path: PathBuf,
    parent: File,
    destination: PathBuf,
    published: bool,
}
impl Staging {
    pub fn new(destination: &Path) -> Result<Self, String> {
        if !cfg!(any(target_os = "linux", target_os = "macos")) {
            return Err("exclusive preview publication requires Linux or macOS".into());
        }
        normalized(destination)?;
        let parent_path = destination.parent().ok_or("preview root parent missing")?;
        #[cfg(unix)]
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(parent_path);
        #[cfg(not(unix))]
        let parent = File::open(parent_path);
        let parent = parent.map_err(|_| "preview parent directory unavailable")?;
        #[cfg(unix)]
        if parent
            .metadata()
            .map_err(|_| "preview parent metadata unavailable")?
            .mode()
            & 0o022
            != 0
        {
            return Err("preview parent must not be group or world writable".into());
        }
        if destination
            .try_exists()
            .map_err(|_| "preview root existence check failed")?
        {
            return Err("preview root already exists".into());
        }
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| "preview entropy unavailable")?;
        let path = parent_path.join(format!(".preview-init-{}", hex::encode(random)));
        private_dir(&path)?;
        parent
            .sync_all()
            .map_err(|_| "preview parent sync failed")?;
        Ok(Self {
            path,
            parent,
            destination: destination.to_owned(),
            published: false,
        })
    }
    pub fn publish(mut self) -> Result<(), String> {
        sync_tree(&self.path)?;
        #[cfg(unix)]
        {
            let source = std::ffi::CString::new(self.path.file_name().unwrap().as_bytes())
                .map_err(|_| "invalid staging name")?;
            let target = std::ffi::CString::new(self.destination.file_name().unwrap().as_bytes())
                .map_err(|_| "invalid destination name")?;
            // Both names are confined to the held parent FD. The kernel rejects
            // any pre-existing destination, including an empty directory/symlink.
            #[cfg(target_os = "linux")]
            let status = unsafe {
                libc::renameat2(
                    self.parent.as_raw_fd(),
                    source.as_ptr(),
                    self.parent.as_raw_fd(),
                    target.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            };
            #[cfg(target_os = "macos")]
            let status = unsafe {
                libc::renameatx_np(
                    self.parent.as_raw_fd(),
                    source.as_ptr(),
                    self.parent.as_raw_fd(),
                    target.as_ptr(),
                    libc::RENAME_EXCL,
                )
            };
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            let status = -1;
            if status != 0 {
                return Err(
                    "exclusive preview publication failed; existing target preserved".into(),
                );
            }
            self.published = true;
            self.parent
                .sync_all()
                .map_err(|_| "published preview parent sync failed")?;
            Ok(())
        }
        #[cfg(not(unix))]
        Err("exclusive preview publication unavailable".into())
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
fn sync_tree(path: &Path) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(|_| "preview sync directory unavailable")? {
        let entry = entry.map_err(|_| "preview sync entry unavailable")?;
        let kind = entry
            .file_type()
            .map_err(|_| "preview sync entry type unavailable")?;
        if kind.is_dir() {
            sync_tree(&entry.path())?;
        } else if kind.is_file() {
            File::open(entry.path())
                .and_then(|f| f.sync_all())
                .map_err(|_| "preview artifact sync failed")?;
        } else {
            return Err("preview tree must contain only regular files and directories".into());
        }
    }
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| "preview directory sync failed".into())
}
