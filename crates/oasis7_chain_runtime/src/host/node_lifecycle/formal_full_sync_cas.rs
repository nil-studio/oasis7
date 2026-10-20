//! Root-aware retention for the full-sync CAS.
//!
//! Blob writes are individually atomic, but a full-sync cut spans many blobs.
//! The durable pin shards provide the publication boundary for history and the
//! currently loaded cut; unpinned objects are only collectible after those
//! roots have been published.

use oasis7_runtime::LocalCasStore;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const FULL_SYNC_PIN_SCOPE: &str = "formal-full-sync";
const HISTORY_PIN_SHARD: &str = "verified-history";
const LOADED_WORLD_PIN_SHARD: &str = "loaded-world";
pub(super) const MAX_RETAINED_TREE_NODES: usize = 65_536;
const MAX_PIN_SHARD_ITEMS: usize = MAX_RETAINED_TREE_NODES * 2;
const MAX_PIN_SHARD_BYTES: usize = MAX_PIN_SHARD_ITEMS * 67 + 128;

// One old loaded cut and one candidate cut may coexist during transfer. Proof
// pages and manifests keep their existing, independent full-history bounds.
pub(super) const MAX_FULL_SYNC_CAS_BYTES: u64 =
    super::providers::MAX_TOTAL_SYNC_BYTES * 2 + super::history_chain::MAX_HISTORY_CAS_BYTES;

/// Inspect the exact persisted full-sync CAS without creating directories,
/// repairing pins, or pruning interrupted-transfer objects. Runtime format
/// compatibility must reject malformed high objects instead of silently
/// treating a lower readable proof head as the state that would be restored.
pub(super) fn verify_read_only_cas(store: &LocalCasStore) -> Result<serde_json::Value, String> {
    super::super::local_io::reject_symlinks(store.root())?;
    super::super::local_io::reject_symlinks(store.blobs_dir())?;
    verify_read_only_directory_entries(store.blobs_dir(), true)?;
    super::super::local_io::reject_symlinks(&store.root().join("pins.json"))?;
    let pin_scopes = store.root().join("pin_scopes");
    super::super::local_io::reject_symlinks(&pin_scopes)?;
    if pin_scopes.exists() {
        let metadata = fs::symlink_metadata(&pin_scopes).map_err(|error| error.to_string())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(
                "HistoryUnavailable: full-sync CAS pin-scope root is not a real directory".into(),
            );
        }
        for scope in fs::read_dir(&pin_scopes).map_err(|error| error.to_string())? {
            let scope = scope.map_err(|error| error.to_string())?;
            if scope
                .file_type()
                .map_err(|error| error.to_string())?
                .is_symlink()
                || !scope
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_dir()
            {
                return Err("HistoryUnavailable: full-sync CAS pin scope contains a non-directory or symlink".into());
            }
            verify_read_only_directory_entries(&scope.path(), false)?;
        }
    }
    let hashes = store
        .list_blob_hashes()
        .map_err(|error| format!("list full-sync CAS objects: {error:?}"))?;
    if hashes.len() > MAX_PIN_SHARD_ITEMS {
        return Err("HistoryUnavailable: full-sync CAS object count exceeds its bound".into());
    }
    let pins = store
        .list_effective_pins()
        .map_err(|error| format!("read full-sync CAS roots: {error:?}"))?
        .into_iter()
        .collect::<BTreeSet<_>>();
    if pins.len() > MAX_PIN_SHARD_ITEMS || pins.iter().any(|pin| !super::valid_hex(pin, 32)) {
        return Err(
            "HistoryUnavailable: full-sync CAS roots are malformed or exceed their bound".into(),
        );
    }
    let blobs = hashes.into_iter().collect::<BTreeSet<_>>();
    if !pins.is_subset(&blobs) {
        return Err(
            "HistoryUnavailable: a durable full-sync CAS root is missing its object".into(),
        );
    }

    let mut total_bytes = 0u64;
    let mut digest_input = b"oasis7.formal.full_sync.cas_probe.v1\0".to_vec();
    for hash in &blobs {
        let path = store.blobs_dir().join(format!("{hash}.blob"));
        super::super::local_io::reject_symlinks(&path)?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file()
            || metadata.len() > super::MAX_FULL_SYNC_RECORD_BYTES as u64 + 64 * 1024
        {
            return Err(format!(
                "HistoryUnavailable: full-sync CAS object {hash} is not a bounded regular file"
            ));
        }
        let (bytes, complete) = store
            .get_range(hash, 0, super::MAX_FULL_SYNC_RECORD_BYTES.saturating_add(1))
            .map_err(|error| format!("read full-sync CAS object {hash}: {error:?}"))?;
        if !complete || bytes.len() > super::MAX_FULL_SYNC_RECORD_BYTES {
            return Err(format!(
                "HistoryUnavailable: full-sync CAS object {hash} exceeds its decoded bound"
            ));
        }
        if blake3::hash(&bytes).to_hex().as_str() != hash {
            return Err(format!(
                "HistoryUnavailable: full-sync CAS object {hash} fails its content digest"
            ));
        }
        total_bytes = total_bytes
            .checked_add(bytes.len() as u64)
            .ok_or("full-sync CAS byte total overflow")?;
        if total_bytes > MAX_FULL_SYNC_CAS_BYTES {
            return Err("HistoryUnavailable: full-sync CAS exceeds its retained-byte bound".into());
        }
        digest_input.extend_from_slice(hash.as_bytes());
        digest_input.push(b':');
        digest_input.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        digest_input.push(b'\n');
    }

    Ok(serde_json::json!({
        "object_count": blobs.len(),
        "pin_count": pins.len(),
        "total_bytes": total_bytes,
        "roots_digest": oasis7_runtime::blake3_hex(&digest_input)
    }))
}

fn verify_read_only_directory_entries(path: &Path, blob_directory: bool) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "HistoryUnavailable: full-sync CAS path is not a real directory: {}",
            path.display()
        ));
    }
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_symlink() {
            return Err(format!(
                "HistoryUnavailable: symlink in full-sync CAS directory: {}",
                entry.path().display()
            ));
        }
        let name = entry.file_name();
        let name = name.to_str().ok_or("non-UTF8 full-sync CAS entry name")?;
        if blob_directory {
            if !kind.is_file() || !name.ends_with(".blob") {
                return Err(format!(
                    "HistoryUnavailable: unexpected entry in full-sync CAS object directory: {name}"
                ));
            }
            let hash = name
                .strip_suffix(".blob")
                .ok_or("invalid CAS blob suffix")?;
            if !super::valid_hex(hash, 32) {
                return Err(format!(
                    "HistoryUnavailable: invalid full-sync CAS object filename: {name}"
                ));
            }
        } else if !kind.is_file() {
            return Err(format!(
                "HistoryUnavailable: non-file entry in full-sync CAS pin scope: {name}"
            ));
        }
    }
    Ok(())
}

pub(super) struct CasBudget {
    bytes: u64,
    max_bytes: u64,
    pub(super) objects: BTreeSet<String>,
    pub(super) visited_paths: usize,
}

#[derive(Debug)]
pub(super) enum PinPublicationError {
    NotPublished(String),
    OutcomeUnknown(String),
}

impl PinPublicationError {
    pub(super) fn outcome_unknown(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_))
    }
}

impl std::fmt::Display for PinPublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPublished(message) => formatter.write_str(message),
            Self::OutcomeUnknown(message) => write!(formatter, "PublicationUnknown: {message}"),
        }
    }
}

#[derive(Serialize)]
struct PinShardFile<'a> {
    pins: &'a BTreeSet<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinShardFileOwned {
    pins: BTreeSet<String>,
}

impl CasBudget {
    pub(super) fn new(max_bytes: u64) -> Self {
        Self {
            bytes: 0,
            max_bytes,
            objects: BTreeSet::new(),
            visited_paths: 0,
        }
    }

    pub(super) fn add(&mut self, digest: &str, size: u64) -> Result<(), String> {
        if self.objects.insert(digest.to_owned()) {
            self.bytes = self
                .bytes
                .checked_add(size)
                .ok_or("full-sync CAS byte overflow")?;
            if self.objects.len() > MAX_RETAINED_TREE_NODES || self.bytes > self.max_bytes {
                return Err(
                    "full-sync retained/record CAS exceeds configured protocol bound".into(),
                );
            }
        }
        Ok(())
    }
}

pub(super) fn replace_history_pins(
    store: &LocalCasStore,
    hashes: &BTreeSet<String>,
) -> Result<(), PinPublicationError> {
    replace_pin_scope_shard_durable(store, HISTORY_PIN_SHARD, hashes).map_err(|error| match error {
        PinPublicationError::NotPublished(message) => PinPublicationError::NotPublished(format!(
            "publish full-sync history CAS pins: {message}"
        )),
        PinPublicationError::OutcomeUnknown(message) => PinPublicationError::OutcomeUnknown(
            format!("publish full-sync history CAS pins: {message}"),
        ),
    })
}

/// Atomically makes the fully verified cut the loaded-world root. The current
/// shard and history shard are included in the capacity preflight so an
/// over-capacity candidate cannot unpin the prior recoverable cut.
pub(super) fn replace_loaded_world_pins(
    store: &LocalCasStore,
    candidate: &BTreeSet<String>,
) -> Result<(), PinPublicationError> {
    replace_loaded_world_pins_with_limit(store, candidate, MAX_FULL_SYNC_CAS_BYTES)
}

fn replace_loaded_world_pins_with_limit(
    store: &LocalCasStore,
    candidate: &BTreeSet<String>,
    max_bytes: u64,
) -> Result<(), PinPublicationError> {
    if candidate.is_empty() {
        return Err(PinPublicationError::NotPublished(
            "HistoryUnavailable: verified full-sync cut has no CAS objects".into(),
        ));
    }
    let mut protected = store
        .list_effective_pins()
        .map_err(|error| {
            PinPublicationError::NotPublished(format!("read full-sync CAS roots: {error:?}"))
        })?
        .into_iter()
        .collect::<BTreeSet<_>>();
    protected.extend(candidate.iter().cloned());
    let protected_bytes =
        stored_blob_bytes(store, &protected).map_err(PinPublicationError::NotPublished)?;
    if protected_bytes > max_bytes {
        return Err(PinPublicationError::NotPublished(format!(
            "StorageCapacityExceeded: protected full-sync CAS requires {protected_bytes} bytes, limit is {max_bytes}; prior loaded cut remains pinned"
        )));
    }
    replace_pin_scope_shard_durable(store, LOADED_WORLD_PIN_SHARD, candidate).map_err(|error| {
        match error {
            PinPublicationError::NotPublished(message) => PinPublicationError::NotPublished(
                format!("publish verified full-sync loaded-world CAS pins: {message}"),
            ),
            PinPublicationError::OutcomeUnknown(message) => PinPublicationError::OutcomeUnknown(
                format!("publish verified full-sync loaded-world CAS pins: {message}"),
            ),
        }
    })
}

fn replace_pin_scope_shard_durable(
    store: &LocalCasStore,
    shard: &str,
    hashes: &BTreeSet<String>,
) -> Result<(), PinPublicationError> {
    sync_verified_blobs(store, hashes).map_err(PinPublicationError::NotPublished)?;
    let scope_dir = ensure_pin_scope_dir(store)?;
    publish_pin_shard(&scope_dir.join(format!("{shard}.json")), hashes, |path| {
        super::super::local_io::sync_dir(path)
    })
}

/// Re-publishes a previous loaded-world shard through the durable writer
/// before startup GC. Earlier builds used the generic non-fsync CAS helper.
pub(super) fn stabilize_loaded_world_pins(store: &LocalCasStore) -> Result<(), String> {
    let scope_dir = ensure_pin_scope_dir(store).map_err(|error| error.to_string())?;
    let path = scope_dir.join(format!("{LOADED_WORLD_PIN_SHARD}.json"));
    let Some(hashes) = read_pin_shard(&path)? else {
        return Ok(());
    };
    sync_verified_blobs(store, &hashes)?;
    publish_pin_shard(&path, &hashes, |parent| {
        super::super::local_io::sync_dir(parent)
    })
    .map_err(|error| error.to_string())
}

fn read_pin_shard(path: &Path) -> Result<Option<BTreeSet<String>>, String> {
    super::super::local_io::reject_symlinks(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read full-sync pin shard: {error}")),
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_PIN_SHARD_BYTES as u64 {
        return Err("HistoryUnavailable: full-sync pin shard is not a bounded regular file".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_PIN_SHARD_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read full-sync pin shard: {error}"))?;
    if bytes.len() > MAX_PIN_SHARD_BYTES {
        return Err(
            "HistoryUnavailable: full-sync pin shard exceeds its encoded byte bound".into(),
        );
    }
    let parsed: PinShardFileOwned = serde_json::from_slice(&bytes)
        .map_err(|error| format!("decode full-sync pin shard: {error}"))?;
    if parsed.pins.len() > MAX_PIN_SHARD_ITEMS
        || parsed.pins.iter().any(|hash| !super::valid_hex(hash, 32))
    {
        return Err("HistoryUnavailable: full-sync pin shard has invalid roots".into());
    }
    Ok(Some(parsed.pins))
}

/// Durably flush roots whose content was authenticated by the caller before
/// publication. This helper never decodes blobs: it first rejects symlinks,
/// non-regular files and files larger than the full-sync object limit, so a
/// stale or hostile CAS entry cannot trigger an unbounded decompression read.
fn sync_verified_blobs(store: &LocalCasStore, hashes: &BTreeSet<String>) -> Result<(), String> {
    if hashes.len() > MAX_PIN_SHARD_ITEMS {
        return Err("HistoryUnavailable: full-sync pin shard exceeds its object bound".into());
    }
    super::super::local_io::private_dir(store.root())?;
    let blobs_dir = store.blobs_dir();
    super::super::local_io::reject_symlinks(blobs_dir)?;
    fs::create_dir_all(blobs_dir).map_err(|error| error.to_string())?;
    super::super::local_io::reject_symlinks(blobs_dir)?;

    for hash in hashes {
        if !super::valid_hex(hash, 32) {
            return Err("HistoryUnavailable: invalid hash in full-sync CAS roots".into());
        }
        let path = blobs_dir.join(format!("{hash}.blob"));
        super::super::local_io::reject_symlinks(&path)?;
        let file = File::open(&path).map_err(|error| {
            format!("HistoryUnavailable: open full-sync CAS root {hash}: {error}")
        })?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "HistoryUnavailable: full-sync CAS root {hash} is not a regular blob"
            ));
        }
        if metadata.len() > super::MAX_FULL_SYNC_RECORD_BYTES as u64 {
            return Err(format!(
                "HistoryUnavailable: full-sync CAS root {hash} exceeds the per-object byte bound"
            ));
        }
        file.sync_all()
            .map_err(|error| format!("durably sync full-sync CAS root {hash}: {error}"))?;
    }

    super::super::local_io::sync_dir(blobs_dir)?;
    super::super::local_io::sync_dir(store.root())?;
    Ok(())
}

fn ensure_pin_scope_dir(store: &LocalCasStore) -> Result<PathBuf, PinPublicationError> {
    let root = store.root();
    super::super::local_io::private_dir(root).map_err(PinPublicationError::NotPublished)?;
    let scopes_dir = root.join("pin_scopes");
    ensure_private_directory(&scopes_dir).map_err(PinPublicationError::NotPublished)?;
    let scope_dir = scopes_dir.join(FULL_SYNC_PIN_SCOPE);
    ensure_private_directory(&scope_dir).map_err(PinPublicationError::NotPublished)?;

    // The parent entries for newly created directories must be durable before
    // a shard can be renamed into the leaf directory.
    for directory in [scope_dir.as_path(), scopes_dir.as_path(), root] {
        super::super::local_io::sync_dir(directory).map_err(PinPublicationError::NotPublished)?;
    }
    Ok(scope_dir)
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    super::super::local_io::reject_symlinks(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};
                // These fixed paths can predate the hardened writer, which
                // created scoped pin directories under the process umask.
                // Tighten only our owned directories before storing private
                // retention roots in them.
                // SAFETY: geteuid takes no arguments and reads process state.
                let current_uid = unsafe { libc::geteuid() };
                if metadata.uid() != current_uid {
                    return Err(format!(
                        "full-sync pin directory is not owned by this process: {}",
                        path.display()
                    ));
                }
                if metadata.permissions().mode() & 0o777 != 0o700 {
                    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                        .map_err(|error| error.to_string())?;
                    super::super::local_io::sync_dir(path)?;
                    if let Some(parent) = path.parent() {
                        super::super::local_io::sync_dir(parent)?;
                    }
                }
            }
        }
        Ok(_) => {
            return Err(format!(
                "full-sync pin path is not a directory: {}",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    super::super::local_io::private_dir(path)
}

fn publish_pin_shard(
    path: &Path,
    hashes: &BTreeSet<String>,
    sync_parent: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), PinPublicationError> {
    publish_pin_shard_with(path, hashes, |from, to| fs::rename(from, to), sync_parent)
}

fn publish_pin_shard_with(
    path: &Path,
    hashes: &BTreeSet<String>,
    rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    sync_parent: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), PinPublicationError> {
    if hashes.len() > MAX_PIN_SHARD_ITEMS {
        return Err(PinPublicationError::NotPublished(
            "HistoryUnavailable: full-sync pin shard exceeds its object bound".into(),
        ));
    }
    for hash in hashes {
        if !super::valid_hex(hash, 32) {
            return Err(PinPublicationError::NotPublished(
                "HistoryUnavailable: invalid hash in full-sync CAS roots".into(),
            ));
        }
    }
    super::super::local_io::reject_symlinks(path).map_err(PinPublicationError::NotPublished)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            return Err(PinPublicationError::NotPublished(
                "full-sync pin shard is not a regular file".into(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(PinPublicationError::NotPublished(error.to_string())),
    }

    let bytes = serde_json::to_vec(&PinShardFile { pins: hashes })
        .map_err(|error| PinPublicationError::NotPublished(error.to_string()))?;
    if bytes.len() > MAX_PIN_SHARD_BYTES {
        return Err(PinPublicationError::NotPublished(
            "HistoryUnavailable: full-sync pin shard exceeds its encoded byte bound".into(),
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        PinPublicationError::NotPublished("full-sync pin shard parent is missing".into())
    })?;
    let temporary = parent.join(format!(
        ".full-sync-pin-{}.tmp",
        super::super::local_io::random_id().map_err(PinPublicationError::NotPublished)?
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| PinPublicationError::NotPublished(error.to_string()))?;
    let write_result = file
        .write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string());
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(PinPublicationError::NotPublished(format!(
            "write durable full-sync pin shard: {error}"
        )));
    }
    if let Err(error) = rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(PinPublicationError::OutcomeUnknown(format!(
            "pin shard rename returned an error and publication cannot be ruled out: {error}"
        )));
    }
    sync_parent(parent).map_err(|error| {
        PinPublicationError::OutcomeUnknown(format!(
            "pin shard rename succeeded but parent directory sync failed: {error}"
        ))
    })
}

/// Removes every unpinned blob and fails closed if the protected roots alone
/// cannot fit. Callers run this at session open, before a new poll, or after a
/// failed candidate; the current loaded-world and verified-history shards are
/// never removed here. Passing the protected-set size as the prune target is
/// intentional: LocalCasStore's generic GC otherwise waits for pressure and
/// would leave old full-sync closures behind indefinitely when below the cap.
pub(super) fn prune_unpinned(store: &LocalCasStore) -> Result<(), String> {
    prune_unpinned_with_limit(store, MAX_FULL_SYNC_CAS_BYTES)
}

pub(super) fn cleanup_after_failed_candidate(
    store: &LocalCasStore,
    durable_root_uncertain: bool,
) -> Result<(), String> {
    if durable_root_uncertain {
        return Err(
            "garbage collection withheld because a durable root outcome is uncertain".into(),
        );
    }
    prune_unpinned(store)
}

fn prune_unpinned_with_limit(store: &LocalCasStore, max_bytes: u64) -> Result<(), String> {
    let protected = store
        .list_effective_pins()
        .map_err(|error| format!("read full-sync CAS roots: {error:?}"))?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let protected_bytes = stored_blob_bytes(store, &protected)?;
    if protected_bytes > max_bytes {
        return Err(format!(
            "StorageCapacityExceeded: protected full-sync CAS requires {protected_bytes} bytes, limit is {max_bytes}"
        ));
    }
    store
        .prune_unpinned(protected_bytes)
        .map_err(|error| format!("prune unpinned full-sync CAS blobs: {error:?}"))?;
    let total_bytes = stored_blob_bytes(
        store,
        &store
            .list_blob_hashes()
            .map_err(|error| format!("list full-sync CAS blobs: {error:?}"))?
            .into_iter()
            .collect(),
    )?;
    if total_bytes > max_bytes {
        return Err(format!(
            "StorageCapacityExceeded: full-sync CAS remains at {total_bytes} bytes after pruning, limit is {max_bytes}"
        ));
    }
    Ok(())
}

fn stored_blob_bytes(store: &LocalCasStore, hashes: &BTreeSet<String>) -> Result<u64, String> {
    let mut total = 0u64;
    for hash in hashes {
        if !super::valid_hex(hash, 32) {
            return Err("HistoryUnavailable: invalid hash in full-sync CAS roots".into());
        }
        let path = store.blobs_dir().join(format!("{hash}.blob"));
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!("HistoryUnavailable: full-sync CAS root {hash} is unavailable: {error}")
        })?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "HistoryUnavailable: full-sync CAS root {hash} is not a regular blob"
            ));
        }
        total = total
            .checked_add(metadata.len())
            .ok_or("StorageCapacityExceeded: full-sync CAS byte count overflow")?;
    }
    Ok(total)
}

#[cfg(test)]
#[path = "formal_full_sync_cas_tests.rs"]
mod tests;
