use super::{io, source::Component};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::os::unix::{
    ffi::OsStrExt,
    fs::{MetadataExt, OpenOptionsExt},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_ENTRIES: usize = 16_000;
const MAX_BYTES: u64 = 1 << 40;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub source: PathBuf,
    pub target: PathBuf,
    pub directory: bool,
    pub bytes: u64,
    pub sha256: String,
    pub source_mode: u32,
}
fn safe_source(path: &Path) -> Result<fs::Metadata, String> {
    io::reject_symlinks(path)?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if (!meta.is_file() && !meta.is_dir()) || meta.uid() != unsafe { libc::geteuid() } {
        return Err("legacy tree must contain owned regular files/directories only".into());
    }
    Ok(meta)
}
pub(super) fn hash(path: &Path) -> Result<(String, u64), String> {
    let before = safe_source(path)?;
    if !before.is_file() {
        return Err("file required".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| e.to_string())?;
    let opened = file.metadata().map_err(|e| e.to_string())?;
    if before.dev() != opened.dev() || before.ino() != opened.ino() {
        return Err("source changed while opening".into());
    }
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count = count.checked_add(n as u64).ok_or("byte overflow")?;
        if count > MAX_BYTES {
            return Err("legacy source size limit".into());
        }
        hash.update(&buffer[..n]);
    }
    if count != before.len() {
        return Err("source changed during hash".into());
    }
    Ok((hex::encode(hash.finalize()), count))
}
pub(super) fn file_entry(source: &Path, target: PathBuf) -> Result<Entry, String> {
    let meta = safe_source(source)?;
    let (sha256, bytes) = hash(source)?;
    Ok(Entry {
        source: source.into(),
        target,
        directory: false,
        bytes,
        sha256,
        source_mode: meta.mode() & 0o777,
    })
}
pub(super) fn inventory(
    components: &[Component],
    excluded: Option<&Path>,
) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut total = 0u64;
    for component in components {
        let meta = safe_source(&component.source)?;
        let device = meta.dev();
        let mut pending = vec![(component.source.clone(), component.target.clone(), 0usize)];
        while let Some((source, target, depth)) = pending.pop() {
            if excluded == Some(source.as_path()) {
                continue;
            }
            if depth > 64 || entries.len() >= MAX_ENTRIES {
                return Err("legacy migration depth/entry limit".into());
            }
            let meta = safe_source(&source)?;
            if meta.dev() != device {
                return Err(
                    "nested mounts require explicit component ownership; migration refused".into(),
                );
            }
            if meta.is_dir() {
                entries.push(Entry {
                    source: source.clone(),
                    target: target.clone(),
                    directory: true,
                    bytes: 0,
                    sha256: String::new(),
                    source_mode: meta.mode() & 0o777,
                });
                let mut children = fs::read_dir(&source)
                    .map_err(|e| e.to_string())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                children.sort_by_key(|e| e.file_name());
                for child in children.into_iter().rev() {
                    pending.push((child.path(), target.join(child.file_name()), depth + 1));
                }
            } else {
                let entry = file_entry(&source, target)?;
                total = total
                    .checked_add(entry.bytes)
                    .ok_or("aggregate bytes overflow")?;
                if total > MAX_BYTES {
                    return Err("legacy migration total size limit".into());
                }
                entries.push(entry);
            }
        }
    }
    entries.sort_by(|a, b| a.target.cmp(&b.target).then(a.source.cmp(&b.source)));
    Ok(entries)
}
pub(super) fn verify_entries(entries: &[Entry], destination: Option<&Path>) -> Result<(), String> {
    for entry in entries {
        let path =
            destination.map_or_else(|| entry.source.clone(), |root| root.join(&entry.target));
        let meta = safe_source(&path)?;
        if meta.is_dir() != entry.directory {
            return Err("migration entry kind differs".into());
        }
        if !entry.directory {
            let (actual, size) = hash(&path)?;
            if actual != entry.sha256 || size != entry.bytes {
                return Err(format!("migration bytes differ: {}", path.display()));
            }
        }
        if destination.is_some() && meta.mode() & 0o077 != 0 {
            return Err("migration target must remain private".into());
        }
    }
    Ok(())
}
pub(super) fn verify_exact(
    entries: &[Entry],
    components: &[Component],
    root: &Path,
) -> Result<(), String> {
    verify_entries(entries, Some(root))?;
    let mapped = components
        .iter()
        .map(|c| Component {
            name: c.name.clone(),
            source: root.join(&c.target),
            target: c.target.clone(),
        })
        .collect::<Vec<_>>();
    let actual = inventory(&mapped, None)?;
    let shape = |items: &[Entry]| {
        items
            .iter()
            .map(|e| (e.target.clone(), (e.directory, e.bytes, e.sha256.clone())))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    if shape(&actual) != shape(entries) {
        return Err("partial/published component contains missing or unexpected material".into());
    }
    Ok(())
}
#[allow(
    clippy::unnecessary_cast,
    reason = "statvfs integer widths differ across supported Unix ABIs"
)]
pub(super) fn require_capacity(entries: &[Entry], stage: &Path) -> Result<(), String> {
    let parent = stage.parent().ok_or("stage parent missing")?;
    let path = std::ffi::CString::new(parent.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let stat = unsafe { stat.assume_init() };
    let available_blocks = stat.f_bavail as u64;
    let block_size = stat.f_frsize as u64;
    let available = available_blocks.saturating_mul(block_size);
    let mut required = 64u64 * 1024 * 1024;
    for entry in entries {
        if stage.join(&entry.target).exists() {
            if !entry.directory {
                verify_entries(std::slice::from_ref(entry), Some(stage))?;
            }
            continue;
        }
        required = required.saturating_add(entry.bytes).saturating_add(4096);
    }
    if required > available {
        return Err(format!(
            "CAPACITY_INSUFFICIENT: remaining migration needs {required} bytes, available {available}"
        ));
    }
    Ok(())
}
pub(super) fn copy(entry: &Entry, stage: &Path) -> Result<(), String> {
    let target = stage.join(&entry.target);
    io::reject_symlinks(&target)?;
    if entry.directory {
        io::private_dir(&target)
            .map_err(|error| format!("create migrated directory {}: {error}", target.display()))?;
        return Ok(());
    }
    let parent = target.parent().ok_or("copy parent missing")?;
    io::private_dir(parent).map_err(|error| {
        format!(
            "create migration target parent {}: {error}",
            parent.display()
        )
    })?;
    let temporary = parent.join(format!(".legacy-copy-{}", io::digest(&entry.target)?));
    if target.exists() {
        verify_entries(std::slice::from_ref(entry), Some(stage))?;
        if temporary.exists() {
            safe_source(&temporary)?;
            fs::remove_file(&temporary).map_err(|e| e.to_string())?;
            io::sync_dir(parent)?;
        }
        return Ok(());
    }
    if temporary.exists() {
        safe_source(&temporary)?;
        fs::remove_file(&temporary).map_err(|e| e.to_string())?;
        io::sync_dir(parent)?;
    }
    let mut input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&entry.source)
        .map_err(|e| format!("open migration source {}: {e}", entry.source.display()))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode((entry.source_mode & 0o700) | 0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|e| {
            format!(
                "create migration temporary {} for {}: {e}",
                temporary.display(),
                target.display()
            )
        })?;
    let mut digest = Sha256::new();
    let mut copied = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = input
            .read(&mut buffer)
            .map_err(|e| format!("read migration source {}: {e}", entry.source.display()))?;
        if n == 0 {
            break;
        }
        copied = copied.checked_add(n as u64).ok_or("copy overflow")?;
        if copied > entry.bytes {
            return Err("source grew during copy".into());
        }
        digest.update(&buffer[..n]);
        output
            .write_all(&buffer[..n])
            .map_err(|e| format!("write migration temporary {}: {e}", temporary.display()))?;
    }
    if copied != entry.bytes || hex::encode(digest.finalize()) != entry.sha256 {
        return Err("source changed during copy".into());
    }
    output
        .sync_all()
        .map_err(|e| format!("sync migration temporary {}: {e}", temporary.display()))?;
    drop(output);
    // This target belongs to a fixed operation's private stage; an existing
    // result is checked, never overwritten. Publication is no-replace below.
    fs::hard_link(&temporary, &target).map_err(|e| {
        format!(
            "publish migration file {} from {}: {e}",
            target.display(),
            temporary.display()
        )
    })?;
    fs::remove_file(&temporary)
        .map_err(|e| format!("remove migration temporary {}: {e}", temporary.display()))?;
    io::sync_dir(parent).map_err(|error| {
        format!(
            "sync migration target directory {}: {error}",
            parent.display()
        )
    })
}
pub(super) fn publish(stage: &Path, destination: &Path) -> Result<(), String> {
    let a = std::ffi::CString::new(stage.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let b =
        std::ffi::CString::new(destination.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let result = unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_EXCL) };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            a.as_ptr(),
            libc::AT_FDCWD,
            b.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err("atomic no-replace migration publication unsupported".into());
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if result != 0 {
        return Err(format!(
            "atomically publish migration stage {} at {}: {}",
            stage.display(),
            destination.display(),
            std::io::Error::last_os_error()
        ));
    }
    io::sync_dir(destination.parent().ok_or("destination parent")?)
}
pub(super) fn sync_directories(entries: &[Entry], stage: &Path) -> Result<(), String> {
    for entry in entries.iter().rev().filter(|e| e.directory) {
        let directory = stage.join(&entry.target);
        File::open(&directory)
            .and_then(|f| f.sync_all())
            .map_err(|e| format!("sync migrated directory {}: {e}", directory.display()))?;
    }
    io::sync_dir(stage)
}
