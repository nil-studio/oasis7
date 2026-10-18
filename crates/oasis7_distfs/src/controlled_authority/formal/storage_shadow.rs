//! Bounded, offline packing into a new, explicitly non-activatable directory.
//! Source objects and checkpoint are never rewritten or retired.
use super::{
    FormalError, business_index::BusinessIndexNodeWriteV1, business_pack, crypto::invalid,
};
use crate::storage_inventory::{StorageInventory, inventory_storage};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

const MAX_OBJECTS: usize = 65_536;
const MAX_LOGICAL_BYTES: u64 = 1024 * 1024 * 1024;
const BATCH_BYTES: usize = 8 * 1024 * 1024;
pub(super) const SHADOW_MARKER: &str = ".shadow-only";

#[derive(Debug, Serialize)]
pub struct FormalStorageShadowReport {
    pub verified_objects: usize,
    pub logical_bytes: u64,
    pub source_objects: StorageInventory,
    pub shadow_objects: StorageInventory,
}
fn io(error: impl std::fmt::Display) -> FormalError {
    FormalError::Io(error.to_string())
}
fn regular_directory(path: &Path) -> Result<(), FormalError> {
    if !fs::symlink_metadata(path).map_err(io)?.is_dir() {
        return Err(invalid("shadow source must be a regular directory"));
    }
    Ok(())
}
fn sync_directory(path: &Path) -> Result<(), FormalError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io)
}
fn names(directory: &Path) -> Result<Vec<String>, FormalError> {
    if super::physical::enabled(directory)? {
        super::physical::roots::verify(directory)?;
        return super::physical::logical_ids(directory).map(|ids| ids.into_iter().collect());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(directory).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("shadow non-UTF8 object name"))?;
        if name == ".business-pack-staging-v1" {
            regular_directory(&entry.path())?;
            if fs::read_dir(entry.path()).map_err(io)?.next().is_some() {
                return Err(invalid(
                    "shadow source has pending staging; recover source first",
                ));
            }
            continue;
        }
        if matches!(name.as_str(), ".representation-v1" | ".frames-v1") {
            regular_directory(&entry.path())?;
            continue;
        }
        if name == ".physical-write-v1" {
            super::physical::enabled(directory)?;
            continue;
        }
        if name == ".physical-guard-v1" {
            let _pin = super::physical::pin(directory, false)?;
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(io)?;
        if names.len() >= MAX_OBJECTS
            || name.len() != 64
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || !metadata.is_file()
            || metadata.len() > business_pack::MAX_PACK
        {
            return Err(invalid("shadow source object name/type/count bound"));
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}
fn write_raw(directory: &Path, id: &str, bytes: &[u8]) -> Result<(), FormalError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(directory.join(id)).map_err(io)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(io)
}
fn flush(directory: &Path, batch: &mut Vec<BusinessIndexNodeWriteV1>) -> Result<(), FormalError> {
    business_pack::persist(directory, batch)?;
    batch.clear();
    Ok(())
}
/// Pack every digest-named source object into a separate new directory, then
/// independently read back and compare exact logical bytes. Acquires the same
/// exclusive endpoint lease as Store; a live endpoint makes this fail before
/// creating output. Only the objects namespace is copied, not an endpoint,
/// checkpoint, authority, or complete recovery generation. No activation/GC.
///
/// Limits: 65,536 objects, 1 GiB total logical input, existing 64 MiB object
/// limit and 8 MiB packing batches. Readback holds two logical objects at once
/// (up to 128 MiB) plus the bounded name list. Errors retain source and an
/// possibly incomplete shadow. The marker is synced before any object write;
/// initialization failure can leave only an empty directory. Use new output on retry.
pub fn build_formal_storage_shadow(
    source_endpoint: &Path,
    destination: &Path,
) -> Result<FormalStorageShadowReport, FormalError> {
    build_with_limit(source_endpoint, destination, MAX_LOGICAL_BYTES)
}
#[path = "storage_shadow_resume.rs"]
mod resume;
#[path = "storage_shadow_stream.rs"]
mod stream;
pub use resume::{FormalStorageShadowProgress, advance_formal_storage_shadow};
pub use stream::{FormalStorageShadowOptions, build_formal_storage_shadow_with_options};
/// Streaming fresh-world codec shadow; source and output remain non-activating.
pub fn build_formal_storage_shadow_optimized(
    source_endpoint: &Path,
    destination: &Path,
) -> Result<FormalStorageShadowReport, FormalError> {
    build_formal_storage_shadow_with_options(
        source_endpoint,
        destination,
        &FormalStorageShadowOptions::default(),
    )
}
fn build_with_limit(
    source_endpoint: &Path,
    destination: &Path,
    max_logical_bytes: u64,
) -> Result<FormalStorageShadowReport, FormalError> {
    if !cfg!(unix) {
        return Err(invalid(
            "formal shadow requires Unix endpoint lease/directory fsync",
        ));
    }
    regular_directory(source_endpoint)?;
    let source = source_endpoint.join("formal-objects-v1");
    regular_directory(&source)?;
    let lock_path = source_endpoint.join(".formal-endpoint.lock");
    if !fs::symlink_metadata(&lock_path).map_err(io)?.is_file() {
        return Err(invalid("shadow endpoint lease must be regular"));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let lease = options.open(lock_path).map_err(io)?;
    lease
        .try_lock()
        .map_err(|_| FormalError::Conflict("formal endpoint locked".into()))?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let resolved = parent.canonicalize().map_err(io)?.join(
        destination
            .file_name()
            .ok_or_else(|| invalid("shadow destination name"))?,
    );
    if resolved.starts_with(source_endpoint.canonicalize().map_err(io)?) {
        return Err(invalid(
            "shadow destination must be outside the source endpoint",
        ));
    }
    let ids = names(&source)?;
    let source_objects = inventory_storage(&source);
    if !source_objects.complete {
        return Err(invalid("shadow source physical inventory incomplete"));
    }
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(destination).map_err(io)?;
    write_raw(destination, SHADOW_MARKER, b"Shadow objects only; never activate as an endpoint. Build may be incomplete; full readback is required.\n")?;
    sync_directory(destination)?;
    sync_directory(parent)?;
    let mut batch = Vec::new();
    let (mut batch_bytes, mut logical_bytes) = (0usize, 0u64);
    for id in &ids {
        let bytes = business_pack::read_object(&source.join(id), id)?
            .ok_or_else(|| invalid("shadow source object disappeared"))?;
        logical_bytes = logical_bytes
            .checked_add(bytes.len() as u64)
            .filter(|total| *total <= max_logical_bytes)
            .ok_or_else(|| invalid("shadow total logical byte budget"))?;
        if bytes.is_empty() || bytes.len() as u64 > business_pack::MAX_PACK_NODE {
            write_raw(destination, id, &bytes)?;
        } else {
            if batch_bytes + bytes.len() > BATCH_BYTES {
                flush(destination, &mut batch)?;
                batch_bytes = 0;
            }
            batch_bytes += bytes.len();
            batch.push(BusinessIndexNodeWriteV1 {
                digest: id.clone(),
                bytes,
            });
        }
    }
    if !batch.is_empty() {
        flush(destination, &mut batch)?;
    }
    drop(batch);
    sync_directory(destination)?;
    for id in &ids {
        let original = business_pack::read_object(&source.join(id), id)?
            .ok_or_else(|| invalid("shadow source object disappeared at readback"))?;
        let rebuilt = business_pack::read_object(&destination.join(id), id)?
            .ok_or_else(|| invalid("shadow target object missing at readback"))?;
        if original != rebuilt {
            return Err(invalid("shadow logical bytes changed"));
        }
    }
    let shadow_objects = inventory_storage(destination);
    if !shadow_objects.complete {
        return Err(invalid("shadow target physical inventory incomplete"));
    }
    // The exclusive lease remains alive through the last source readback.
    drop(lease);
    Ok(FormalStorageShadowReport {
        verified_objects: ids.len(),
        logical_bytes,
        source_objects,
        shadow_objects,
    })
}

#[cfg(all(test, unix))]
#[path = "storage_shadow_tests.rs"]
mod tests;
