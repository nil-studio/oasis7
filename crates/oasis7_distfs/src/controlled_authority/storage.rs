use crate::controlled_authority::writer_lock::WriterLock;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{LocalJournalError, invalid};

pub(super) const SNAPSHOT_FILE: &str = "local-authority-snapshot.json";
pub(super) const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PersistPhase {
    TempCreated,
    TempWritten,
    TempSynced,
    Renamed,
    DirectorySynced,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) enum FaultInjection {
    #[default]
    None,
    #[cfg(test)]
    ErrorAt(PersistPhase),
    #[cfg(test)]
    ExitAt(PersistPhase),
}

impl FaultInjection {
    fn check(self, _phase: PersistPhase) -> io::Result<()> {
        match self {
            Self::None => Ok(()),
            #[cfg(test)]
            Self::ErrorAt(target) if target == _phase => {
                Err(io::Error::other("injected uncertain write"))
            }
            #[cfg(test)]
            Self::ExitAt(target) if target == _phase => std::process::exit(73),
            #[cfg(test)]
            _ => Ok(()),
        }
    }
}

pub(super) fn io_error(error: io::Error) -> LocalJournalError {
    LocalJournalError::Io(error.to_string())
}

fn reject_symlink(path: &Path) -> Result<(), LocalJournalError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid("symlink in journal storage"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

pub(super) fn lock_directory(directory: &Path) -> Result<WriterLock, LocalJournalError> {
    if !cfg!(unix) {
        return Err(LocalJournalError::UnsupportedPlatform);
    }
    reject_symlink(directory)?;
    match fs::create_dir(directory) {
        Ok(()) => {
            sync_directory(directory).map_err(io_error)?;
            sync_directory(
                directory
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )
            .map_err(io_error)?;
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if !fs::metadata(directory).map_err(io_error)?.is_dir() {
                return Err(invalid("journal storage is not a directory"));
            }
        }
        Err(error) => return Err(io_error(error)),
    }
    let path = directory.join(".local-authority-writer.lock");
    reject_symlink(&path)?;
    // Retain this inode permanently. Removing a lock path would permit two locks
    // on different inodes after a concurrent reopen.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(io_error)?;
    WriterLock::acquire(file).map_err(|_| LocalJournalError::Locked)
}

pub(super) fn read_snapshot(directory: &Path) -> Result<Option<Vec<u8>>, LocalJournalError> {
    let path = directory.join(SNAPSHOT_FILE);
    reject_symlink(&path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    let mut bytes = Vec::new();
    file.take((MAX_SNAPSHOT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(invalid("local snapshot size limit exceeded"));
    }
    Ok(Some(bytes))
}

pub(super) fn sync_existing_snapshot(directory: &Path) -> Result<(), LocalJournalError> {
    reject_symlink(&directory.join(SNAPSHOT_FILE))?;
    File::open(directory.join(SNAPSHOT_FILE))
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    sync_directory(directory).map_err(io_error)
}

pub(super) fn persist_snapshot(
    directory: &Path,
    bytes: &[u8],
    fault: FaultInjection,
) -> io::Result<()> {
    let (temp, mut file) = create_temp(directory)?;
    let result = (|| {
        fault.check(PersistPhase::TempCreated)?;
        file.write_all(bytes)?;
        fault.check(PersistPhase::TempWritten)?;
        file.sync_all()?;
        fault.check(PersistPhase::TempSynced)?;
        fs::rename(&temp, directory.join(SNAPSHOT_FILE))?;
        fault.check(PersistPhase::Renamed)?;
        sync_directory(directory)?;
        fault.check(PersistPhase::DirectorySynced)
    })();
    if result.is_err() {
        // Canonical snapshot is never rolled back on an ambiguous result.
        let _ = fs::remove_file(temp);
    }
    result
}

fn create_temp(directory: &Path) -> io::Result<(PathBuf, File)> {
    loop {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            ".local-authority-pending-{}-{serial}",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "directory durability unsupported",
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn writer_lock_releases_before_inherited_fork_descriptor_closes() {
        use crate::controlled_authority::writer_lock::regression;
        if regression::isolate(
            "controlled_authority::storage::tests::writer_lock_releases_before_inherited_fork_descriptor_closes",
        ) {
            return;
        }
        let root =
            std::env::temp_dir().join(format!("o7-journal-fork-local-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let result = std::panic::catch_unwind(|| {
            regression::assert_release(
                || lock_directory(&root),
                |error| matches!(error, LocalJournalError::Locked),
            )
        });
        fs::remove_dir_all(&root).unwrap();
        result.unwrap();
    }
}
