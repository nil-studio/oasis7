use super::*;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    sync::atomic::AtomicU64,
};
static STAGE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn metadata(metadata: &fs::Metadata, directory: bool) -> Result<()> {
    if if directory {
        !metadata.is_dir()
    } else {
        !metadata.is_file()
    } {
        return Err(PageStoreError::Corrupt("physical path type"));
    }
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no caller preconditions.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
            return Err(PageStoreError::Corrupt("physical path owner/write mode"));
        }
    }
    Ok(())
}
fn directory(path: &Path) -> Result<()> {
    metadata(&fs::symlink_metadata(path)?, true)
}
fn create_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path)?;
    sync(path)?;
    if let Some(parent) = path.parent() {
        sync(parent)?;
    }
    Ok(())
}
fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    options
}
fn open_regular(path: &Path) -> Result<File> {
    let before = fs::symlink_metadata(path)?;
    metadata(&before, false)?;
    let file = options().read(true).open(path)?;
    let after = file.metadata()?;
    metadata(&after, false)?;
    #[cfg(unix)]
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(PageStoreError::Corrupt("physical path changed"));
    }
    Ok(file)
}
fn sync(path: &Path) -> Result<()> {
    let before = fs::symlink_metadata(path)?;
    // System ancestors need not be owned by the store. Their directory fsync
    // is still required when creating a fresh child, with O_NOFOLLOW.
    let file = options().read(true).open(path)?;
    let after = file.metadata()?;
    if before.is_dir() != after.is_dir() || before.file_type().is_symlink() {
        return Err(PageStoreError::Corrupt("physical sync path"));
    }
    #[cfg(unix)]
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(PageStoreError::Corrupt("physical sync identity"));
    }
    file.sync_all()?;
    Ok(())
}
pub(super) fn create_store(root: &Path) -> Result<()> {
    create_directory(root)?;
    for name in ["frames", "recipes", "staging"] {
        create_directory(&root.join(name))?;
    }
    for (name, bytes) in [("lease", &b""[..]), ("format", FORMAT)] {
        let mut file = options()
            .write(true)
            .create_new(true)
            .open(root.join(name))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    sync(root)
}
pub(super) fn validate_store(root: &Path) -> Result<()> {
    directory(root)?;
    for name in ["frames", "recipes", "staging"] {
        directory(&root.join(name))?;
    }
    let mut file = open_regular(&root.join("format"))?;
    let mut buffer = [0u8; 64];
    if file.metadata()?.len() != FORMAT.len() as u64 {
        return Err(PageStoreError::Corrupt("physical format capability"));
    }
    file.read_exact(&mut buffer[..FORMAT.len()])?;
    if &buffer[..FORMAT.len()] != FORMAT {
        return Err(PageStoreError::Corrupt("physical format capability"));
    }
    Ok(())
}
pub(super) fn open_store(root: &Path) -> Result<File> {
    validate_store(root)?;
    let file = open_regular(&root.join("lease"))?;
    if file.metadata()?.len() != 0 {
        return Err(PageStoreError::Corrupt("physical fixed lease"));
    }
    file.try_lock_shared().map_err(|error| match error {
        fs::TryLockError::WouldBlock => {
            PageStoreError::ResourceLimited("physical retirement lease busy")
        }
        fs::TryLockError::Error(error) => PageStoreError::Io(error),
    })?;
    validate_lease(root, &file)?;
    Ok(file)
}
pub(super) fn validate_lease(root: &Path, file: &File) -> Result<()> {
    let current = fs::symlink_metadata(root.join("lease"))?;
    metadata(&current, false)?;
    if current.len() != 0 {
        return Err(PageStoreError::Corrupt("physical lease payload"));
    }
    #[cfg(unix)]
    {
        let opened = file.metadata()?;
        if current.dev() != opened.dev() || current.ino() != opened.ino() || current.nlink() != 1 {
            return Err(PageStoreError::Corrupt("physical lease identity"));
        }
    }
    Ok(())
}
pub(super) fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(value) => {
            metadata(&value, false)?;
            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
pub(super) fn read(
    path: &Path,
    bound: usize,
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>> {
    check_cancelled(cancelled)?;
    let mut file = open_regular(path)?;
    let length = file.metadata()?.len();
    if length > bound as u64 {
        return Err(PageStoreError::Corrupt("physical file bound"));
    }
    // Admit the final overrun probe too; a concurrently growing file cannot
    // force even one unadmitted byte of input.
    io.read(length as usize + 1)?;
    let mut bytes = vec![0; length as usize];
    file.read_exact(&mut bytes)?;
    let mut extra = [0u8; 1];
    if file.read(&mut extra)? != 0 {
        return Err(PageStoreError::Corrupt("physical file grew"));
    }
    check_cancelled(cancelled)?;
    Ok(bytes)
}
/// Immutable link publication. No rename ever overwrites a selected member.
/// Crashed private files remain unreferenced; Open does not guess their fate.
pub(super) fn publish(
    root: &Path,
    directory: &str,
    name: &str,
    bytes: &[u8],
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
    exact_existing: bool,
) -> Result<()> {
    check_cancelled(cancelled)?;
    let target = root.join(directory).join(name);
    if exists(&target)? {
        if exact_existing && read(&target, frame_codec::FRAME_BYTES, io, cancelled)? != bytes {
            return Err(PageStoreError::Corrupt("immutable physical frame differs"));
        }
        sync(&target)?;
        sync(&root.join(directory))?;
        return Ok(());
    }
    io.write(bytes.len())?;
    let mut attempts = 0;
    let (stage, mut file) = loop {
        if attempts == 64 {
            return Err(PageStoreError::ResourceLimited(
                "private staging name collisions",
            ));
        }
        attempts += 1;
        let sequence = STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = root
            .join("staging")
            .join(format!("{}-{sequence}", std::process::id()));
        match options().write(true).create_new(true).open(&path) {
            Ok(file) => break (path, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    };
    // Cleanup only our exact private pathname. On any ambiguous failure retain
    // source material for the future bounded maintenance/recovery procedure.
    file.write_all(bytes)?;
    file.sync_all()?;
    publication_point(root, directory, 1);
    check_cancelled(cancelled)?;
    match fs::hard_link(&stage, &target) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            if exact_existing && read(&target, frame_codec::FRAME_BYTES, io, cancelled)? != bytes {
                return Err(PageStoreError::Corrupt("concurrent physical frame differs"));
            }
        }
        Err(e) => return Err(e.into()),
    }
    publication_point(root, directory, 2);
    sync(&target)?;
    sync(&root.join(directory))?;
    publication_point(root, directory, 3);
    fs::remove_file(&stage)?;
    sync(&root.join("staging"))?;
    Ok(())
}

fn publication_point(_root: &Path, _directory: &str, _point: u8) {
    #[cfg(test)]
    if std::env::var("OASIS7_IWS_PAGE_CRASH_POINT").as_deref()
        == Ok(format!("{_directory}-{_point}").as_str())
    {
        // Only the exact child test sets this variable. The parent kills and
        // reaps it at a named real filesystem boundary, without Drop cleanup.
        fs::write(_root.parent().unwrap().join("paused"), b"paused").unwrap();
        loop {
            std::thread::park_timeout(std::time::Duration::from_millis(5));
        }
    }
}
