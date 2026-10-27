//! Private publication under the prefix ledger's stable OS locks. Complete
//! pre-rename stages retain the original bytes; no stage is blindly discarded.
use super::super::super::storage as common_storage;
use serde::{Serialize, de::DeserializeOwned};
use std::fs::{self, File};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub(super) fn lock(directory: &Path, initialize: bool) -> Result<File, String> {
    let existed = directory.exists();
    if !existed && !initialize {
        return Err("formal prefix ledger is absent; verification cannot initialize it".into());
    }
    super::super::super::super::super::controlled_live::private_directory(directory)?;
    let carrier = directory.join("writer.lock");
    if existed && fs::symlink_metadata(&carrier).is_err() {
        let empty = fs::read_dir(directory)
            .map_err(|e| e.to_string())?
            .next()
            .is_none();
        if !initialize || !empty {
            return Err(
                "formal prefix stable writer lock is missing; refusing to recreate it".into(),
            );
        }
    }
    let identity_started = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .try_fold(false, |found, entry| {
            entry.map(|entry| found || entry.file_name() != "writer.lock")
        })
        .map_err(|e| e.to_string())?;
    if identity_started {
        common_storage::lock_existing_carrier(&carrier)
    } else {
        common_storage::lock_carrier(&carrier)
    }
}
pub(super) fn sync(directory: &Path) -> Result<(), String> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())
}
pub(super) fn single_link(path: &Path) -> Result<(), String> {
    crate::controlled_live_config::require_private_owned(path, false)?;
    if fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .nlink()
        != 1
    {
        return Err("formal prefix file has an unexpected alias".into());
    }
    Ok(())
}
pub(super) fn same_pair(record: &Path, intent: &Path) -> Result<(), String> {
    crate::controlled_live_config::require_private_owned(record, false)?;
    crate::controlled_live_config::require_private_owned(intent, false)?;
    let a = fs::symlink_metadata(record).map_err(|e| e.to_string())?;
    let b = fs::symlink_metadata(intent).map_err(|e| e.to_string())?;
    if a.dev() != b.dev() || a.ino() != b.ino() || a.nlink() != 2 || b.nlink() != 2 {
        return Err(
            "formal prefix record/intent are not the exact original two-link publication".into(),
        );
    }
    Ok(())
}
pub(super) fn stage_target(name: &str) -> Option<&str> {
    let (target, token) = name
        .strip_prefix('.')?
        .strip_suffix(".tmp")?
        .split_once(".checkpoint-install-")?;
    if token.len() > 80
        || token.split('-').count() != 3
        || !token
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(target)
}
pub(super) fn stages(path: &Path) -> Result<Vec<PathBuf>, String> {
    let directory = path
        .parent()
        .ok_or("formal prefix publication parent missing")?;
    let target = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("formal prefix target name invalid")?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_name().to_str().and_then(stage_target) == Some(target) {
            if paths.len() >= 64 {
                return Err("formal prefix staging count capacity exceeded".into());
            }
            single_link(&entry.path())?;
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}
pub(super) fn staged_value<T: DeserializeOwned + Serialize + PartialEq>(
    path: &Path,
    maximum: usize,
) -> Result<Option<T>, String> {
    let mut value = None;
    for stage in stages(path)? {
        let next: T = common_storage::read_optional_canonical(&stage, maximum)?
            .ok_or("formal prefix original stage disappeared")?;
        if value.as_ref().is_some_and(|original| original != &next) {
            return Err("formal prefix stages contain conflicting original requests".into());
        }
        value = Some(next);
    }
    Ok(value)
}
fn equal_bytes(path: &Path, expected: &[u8]) -> Result<(), String> {
    let bytes = common_storage::read_optional_bytes(path, expected.len())?
        .ok_or("formal prefix original publication disappeared")?;
    if bytes != expected {
        return Err("formal prefix original publication bytes changed".into());
    }
    crate::controlled_live_config::require_private_owned(path, false)
}
pub(super) fn clean_equivalent_stages(path: &Path, expected: &[u8]) -> Result<(), String> {
    let paths = stages(path)?;
    // Check the whole set before deleting anything. An incomplete or different
    // stage is original evidence, never disposable garbage.
    for stage in &paths {
        equal_bytes(stage, expected)?;
    }
    for stage in paths {
        fs::remove_file(stage).map_err(|e| e.to_string())?;
    }
    sync(path.parent().ok_or("formal prefix parent missing")?)
}
/// Called only with the ledger lock and validated exact original input.
pub(super) fn publish(path: &Path, expected: &[u8]) -> Result<(), String> {
    let paths = stages(path)?;
    for stage in &paths {
        equal_bytes(stage, expected)?;
    }
    if common_storage::read_optional_bytes(path, expected.len())?.is_some() {
        equal_bytes(path, expected)?;
        single_link(path)?;
    } else if let Some(stage) = paths.first() {
        promote(stage, path, expected)?;
    } else {
        super::super::super::super::super::durable_transaction::write_private_file_durable(
            path, expected,
        )?;
    }
    clean_equivalent_stages(path, expected)
}
/// Atomic name transfer under the shared format lock. Never replace a target.
pub(super) fn promote(source: &Path, destination: &Path, expected: &[u8]) -> Result<(), String> {
    single_link(source)?;
    equal_bytes(source, expected)?;
    match fs::symlink_metadata(destination) {
        Ok(_) => return Err("formal prefix promotion target already exists".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    fs::rename(source, destination).map_err(|e| e.to_string())?;
    sync(destination.parent().ok_or("formal prefix parent missing")?)?;
    single_link(destination)
}
pub(super) fn link_record(record: &Path, intent: &Path, expected: &[u8]) -> Result<(), String> {
    equal_bytes(intent, expected)?;
    match fs::hard_link(intent, record) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(format!("publish formal prefix original intent: {error}")),
    }
    same_pair(record, intent)?;
    equal_bytes(record, expected)?;
    sync(
        record
            .parent()
            .ok_or("formal prefix record parent missing")?,
    )
}
pub(super) fn finish(record: &Path, intent: &Path, expected: &[u8]) -> Result<(), String> {
    link_record(record, intent, expected)?;
    fs::remove_file(intent).map_err(|e| e.to_string())?;
    sync(record.parent().ok_or("formal prefix parent missing")?)?;
    single_link(record)
}
