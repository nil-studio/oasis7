use super::*;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const FILE: &str = "replicated-endpoint.json";
pub(super) const MAX_FILE: usize = 64 * 1024 * 1024;
static TEMP: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    TempCreated,
    Written,
    Synced,
    Renamed,
    DirectorySynced,
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) enum Fault {
    #[default]
    None,
    #[cfg(test)]
    Error(Phase),
    #[cfg(test)]
    Exit(Phase),
}
impl Fault {
    pub(super) fn check(self, _phase: Phase) -> Result<(), ProtocolError> {
        match self {
            Self::None => Ok(()),
            #[cfg(test)]
            Self::Error(p) if p == _phase => {
                Err(io(std::io::Error::other("injected uncertain write")))
            }
            #[cfg(test)]
            Self::Exit(p) if p == _phase => std::process::exit(73),
            #[cfg(test)]
            _ => Ok(()),
        }
    }
}
pub(super) fn no_symlink(path: &Path) -> Result<(), ProtocolError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Err(invalid("symlink in endpoint storage")),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(e)),
    }
}
pub(super) fn lock(path: &Path) -> Result<File, ProtocolError> {
    if !cfg!(unix) {
        return Err(ProtocolError::UnsupportedPlatform);
    }
    no_symlink(path)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => {
            sync_dir(path)?;
            sync_dir(
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if !fs::metadata(path).map_err(io)?.is_dir() {
                return Err(invalid("endpoint is not directory"));
            }
        }
        Err(e) => return Err(io(e)),
    }
    let lock = path.join(".replicated-endpoint.lock");
    no_symlink(&lock)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(lock).map_err(io)?;
    if !file.metadata().map_err(io)?.is_file() {
        return Err(invalid("lock is not regular file"));
    }
    file.try_lock().map_err(|_error| {
        #[cfg(test)]
        eprintln!(
            "replicated endpoint lock failed pid={} path={} error={_error:?}",
            std::process::id(),
            path.display()
        );
        ProtocolError::Locked
    })?;
    Ok(file)
}
pub(super) fn read_legacy(path: &Path) -> Result<Option<Vec<u8>>, ProtocolError> {
    let p = path.join(FILE);
    no_symlink(&p)?;
    // Reject FIFOs before open. This operator-controlled path contract excludes
    // hostile same-permission races replacing the inode between checks/open.
    match fs::symlink_metadata(&p) {
        Ok(m) if !m.is_file() => return Err(invalid("endpoint is not regular file")),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    }
    let file = match File::open(p) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    };
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() || metadata.len() > MAX_FILE as u64 {
        return Err(invalid("endpoint regular file/size limit"));
    }
    let mut bytes = Vec::new();
    file.take((MAX_FILE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > MAX_FILE {
        return Err(invalid("endpoint size limit"));
    }
    Ok(Some(bytes))
}
pub(super) fn temp(path: &Path) -> Result<(PathBuf, File), ProtocolError> {
    loop {
        let p = path.join(format!(
            ".replicated-pending-{}-{}",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&p) {
            Ok(file) => return Ok((p, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io(e)),
        }
    }
}
#[cfg(unix)]
pub(super) fn sync_dir(path: &Path) -> Result<(), ProtocolError> {
    File::open(path).and_then(|f| f.sync_all()).map_err(io)
}
#[cfg(not(unix))]
pub(super) fn sync_dir(_: &Path) -> Result<(), ProtocolError> {
    Err(ProtocolError::UnsupportedPlatform)
}

#[cfg(test)]
pub(super) fn read(path: &Path) -> Result<Option<Vec<u8>>, ProtocolError> {
    super::journal::read(path, read_legacy(path)?)
}
#[cfg(test)]
pub(super) fn persist(path: &Path, bytes: &[u8], fault: Fault) -> Result<(), ProtocolError> {
    super::journal::persist(path, bytes, fault)
}

pub(super) fn private_dir(path: &Path) -> Result<(), ProtocolError> {
    no_symlink(path)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => Ok(()),
        Err(e) => Err(io(e)),
    }
}
