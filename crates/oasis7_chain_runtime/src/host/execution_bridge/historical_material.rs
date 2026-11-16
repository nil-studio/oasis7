//! Retention cache for explicitly reconstructed DevLocal historical cuts.
//! A pin never supplies authority: the original record/proof and CAS do.
use super::{ExecutionBridgeRecord, WorldHeadProofV1, checkpoint};
use oasis7_runtime::LocalCasStore;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, io::Read, path::Path};

pub(crate) const SCOPE: &str = "preview_history_rehydration_v1";
/// Independent local cache bound, not a formal protocol or protected-state cap.
pub(crate) const MAX_CUTS: usize = 64;
pub(crate) const MAX_BLOB: usize = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pins {
    pub pins: BTreeSet<String>,
}

/// Validate the source CAS carrier independently of pin-suffix availability.
pub(crate) fn validate_blob_namespace(store: &LocalCasStore) -> Result<(), String> {
    for directory in [store.root().to_path_buf(), store.blobs_dir().to_path_buf()] {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            _ => return Err("historical blob namespace carrier invalid".into()),
        }
    }
    Ok(())
}

/// Refuse namespace redirection before reading a pin or publishing any CAS.
/// Missing suffix directories are allowed only as absence; readers never create.
pub(crate) fn pin_namespace_present(store: &LocalCasStore) -> Result<bool, String> {
    validate_blob_namespace(store)?;
    for directory in [
        store.root().to_path_buf(),
        store.root().join("pin_scopes"),
        store.root().join("pin_scopes").join(SCOPE),
    ] {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err("historical pin namespace carrier invalid".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err("historical pin namespace unavailable".into()),
        }
    }
    Ok(true)
}

pub(crate) fn shard(store: &LocalCasStore, height: u64) -> std::path::PathBuf {
    store
        .root()
        .join("pin_scopes")
        .join(SCOPE)
        .join(format!("{height:020}.json"))
}

pub(crate) fn read_regular(path: &Path, max: usize) -> Result<Option<Vec<u8>>, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("historical material file unavailable".into()),
    };
    if !file
        .metadata()
        .map_err(|_| "historical material file metadata unavailable")?
        .is_file()
    {
        return Err("historical material file carrier invalid".into());
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "historical material file read failed")?;
    if bytes.len() > max {
        return Err("historical material file bound exhausted".into());
    }
    Ok(Some(bytes))
}

pub(crate) fn load_record(
    records: &Path,
    height: u64,
) -> Result<Option<ExecutionBridgeRecord>, String> {
    read_regular(
        &checkpoint::execution_bridge_record_path(records, height),
        1024 * 1024,
    )?
    .map(|bytes| {
        serde_json::from_slice(&bytes)
            .map_err(|_| "historical material original record invalid".into())
    })
    .transpose()
}

pub(crate) fn cas(store: &LocalCasStore, hash: &str) -> Result<Option<Vec<u8>>, String> {
    validate_blob_namespace(store)?;
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("historical material hash invalid".into());
    }
    let path = store.blobs_dir().join(format!("{hash}.blob"));
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("historical material CAS unavailable".into()),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "historical material CAS metadata unavailable")?;
    if !metadata.is_file() || metadata.len() > MAX_BLOB as u64 {
        return Err("historical material CAS carrier or size invalid".into());
    }
    let mut bytes = Vec::new();
    oasis7_distfs::decode_local_blob_into(
        store.hash_algorithm(),
        hash,
        file.take(MAX_BLOB as u64 + 1),
        &mut bytes,
        MAX_BLOB,
    )
    .map_err(|_| "historical material CAS hash or size rejected")?;
    Ok(Some(bytes))
}

/// Infer refs only from the original hash-bound proof, never a cache manifest.
pub(crate) fn references(
    record: &ExecutionBridgeRecord,
    store: &LocalCasStore,
) -> Result<Option<BTreeSet<String>>, String> {
    let Some(reference) = record.world_head_proof_ref.as_deref() else {
        return Ok(None);
    };
    let Some(bytes) = cas(store, reference)? else {
        return Ok(None);
    };
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("historical head proof exceeds bound".into());
    }
    let proof: WorldHeadProofV1 =
        serde_cbor::from_slice(&bytes).map_err(|_| "historical head proof invalid")?;
    proof.validate_contract()?;
    if record.world_head_proof_hash.as_deref() != Some(proof.proof_hash()?.as_str())
        || proof.world_id != record.world_id
        || proof.height != record.height
        || proof.timestamp_ms != record.timestamp_ms
        || proof.execution.execution_state_root != record.execution_state_root
        || proof.execution.execution_block_hash != record.execution_block_hash
        || proof.execution.node_block_hash != record.node_block_hash.as_deref().unwrap_or("")
        || proof.execution.action_root != record.action_root.as_deref().unwrap_or("")
        || proof.snapshot_manifest_ref.content_hash != record.execution_state_root
    {
        return Err("historical head proof differs from original record".into());
    }
    if record
        .snapshot_ref
        .as_deref()
        .is_some_and(|value| value != proof.snapshot_manifest_ref.content_hash)
        || record
            .latest_state_ref
            .as_deref()
            .is_some_and(|value| value != proof.snapshot_manifest_ref.content_hash)
        || record
            .journal_ref
            .as_deref()
            .is_some_and(|value| value != proof.journal_segments_ref.content_hash)
    {
        return Err("historical material conflicts with existing record refs".into());
    }
    Ok(Some(
        [
            reference.to_owned(),
            proof.snapshot_manifest_ref.content_hash,
            proof.journal_segments_ref.content_hash,
        ]
        .into_iter()
        .collect(),
    ))
}

/// Read-only admission. Missing material stays unavailable; malformed material
/// is an error. This never creates a lock, pin, record or blob.
fn pinned_references(
    record: &ExecutionBridgeRecord,
    store: &LocalCasStore,
) -> Result<Option<BTreeSet<String>>, String> {
    if !pin_namespace_present(store)? {
        return Ok(None);
    }
    let Some(bytes) = read_regular(&shard(store, record.height), 16 * 1024)? else {
        return Ok(None);
    };
    let pins: Pins =
        serde_json::from_slice(&bytes).map_err(|_| "historical material pin invalid")?;
    let Some(expected) = references(record, store)? else {
        return Ok(None);
    };
    if pins.pins != expected || expected.len() != 3 {
        return Err("historical material exact pin mismatch".into());
    }
    Ok(Some(expected))
}

pub(crate) fn hydrate(
    record: &ExecutionBridgeRecord,
    store: &LocalCasStore,
) -> Result<Option<ExecutionBridgeRecord>, String> {
    let Some(expected) = pinned_references(record, store)? else {
        return Ok(None);
    };
    let proof_ref = record
        .world_head_proof_ref
        .as_deref()
        .ok_or("historical proof ref absent")?;
    let proof: WorldHeadProofV1 =
        serde_cbor::from_slice(&cas(store, proof_ref)?.ok_or("historical proof disappeared")?)
            .map_err(|_| "historical proof invalid")?;
    for hash in &expected {
        if cas(store, hash)?.is_none() {
            return Ok(None);
        }
    }
    if record
        .snapshot_ref
        .as_deref()
        .is_some_and(|value| value != proof.snapshot_manifest_ref.content_hash)
        || record
            .latest_state_ref
            .as_deref()
            .is_some_and(|value| value != proof.snapshot_manifest_ref.content_hash)
        || record
            .journal_ref
            .as_deref()
            .is_some_and(|value| value != proof.journal_segments_ref.content_hash)
    {
        return Err("historical material conflicts with existing record refs".into());
    }
    let mut hydrated = record.clone();
    hydrated.latest_state_ref = Some(proof.snapshot_manifest_ref.content_hash.clone());
    hydrated.snapshot_ref = Some(proof.snapshot_manifest_ref.content_hash);
    hydrated.journal_ref = Some(proof.journal_segments_ref.content_hash);
    Ok(Some(hydrated))
}

pub(crate) fn protected_heights(
    records: &Path,
    store: &LocalCasStore,
) -> Result<BTreeSet<u64>, String> {
    if !pin_namespace_present(store)? {
        return Ok(BTreeSet::new());
    }
    let directory = store.root().join("pin_scopes").join(SCOPE);
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(_) => return Err("historical material pin directory unavailable".into()),
    };
    let mut result = BTreeSet::new();
    for entry in entries {
        if result.len() >= MAX_CUTS {
            return Err("historical material cut cache exhausted".into());
        }
        let entry = entry.map_err(|_| "historical material pin entry unavailable")?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or("historical material pin name invalid")?;
        let stem = name
            .strip_suffix(".json")
            .ok_or("historical material pin name invalid")?;
        if stem.len() != 20 || !stem.bytes().all(|b| b.is_ascii_digit()) {
            return Err("historical material pin name invalid".into());
        }
        let height = stem
            .parse::<u64>()
            .map_err(|_| "historical material pin height invalid")?;
        let record =
            load_record(records, height)?.ok_or("historical protected record unavailable")?;
        if height == 0 || record.height != height || pinned_references(&record, store)?.is_none() {
            return Err("historical material protected closure unavailable".into());
        }
        result.insert(height);
    }
    Ok(result)
}
