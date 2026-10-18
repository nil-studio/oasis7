//! Self-contained sparse Snapshot checkpoint closures; state roots stay raw-byte hashes.
use super::{controlled_capture as capture, formal_package::insert, to_cbor};
use oasis7_distfs::controlled_authority::replicated_protocol::{
    ArtifactObject, ArtifactRole, ClosedRecord,
};
use oasis7_runtime::{MAX_SNAPSHOT_DELTA_DEPTH, Snapshot, SnapshotArtifactV1, SnapshotDeltaV1};
use std::collections::{BTreeMap, BTreeSet};

// Producer policy only: old signed chains through depth 32 remain readable.
// A changed Snapshot after depth 8 starts a fresh exact checkpoint.
const PRODUCER_MAX_SNAPSHOT_DELTA_DEPTH: usize = 8;

pub(crate) fn bytes(material: &ClosedRecord) -> Result<Vec<u8>, String> {
    material.snapshot_bytes().map_err(|error| error.to_string())
}

/// Copy only the predecessor's Snapshot chain, not its input/results or private
/// authority closures. Every dependency remains inside the bounded ClosedRecord.
pub(super) fn inherit(
    objects: &mut BTreeMap<String, ArtifactObject>,
    parent: &ClosedRecord,
    expected_bytes: &[u8],
) -> Result<String, String> {
    inherit_from_bytes(objects, parent, expected_bytes, &bytes(parent)?)
}

pub(super) fn inherit_verified(
    objects: &mut BTreeMap<String, ArtifactObject>,
    parent: &ClosedRecord,
    expected_bytes: &[u8],
    verified: &super::formal_package::VerifiedFrozenSnapshot<'_>,
) -> Result<String, String> {
    inherit_from_bytes(objects, parent, expected_bytes, verified.bytes_for(parent)?)
}

fn inherit_from_bytes(
    objects: &mut BTreeMap<String, ArtifactObject>,
    parent: &ClosedRecord,
    expected_bytes: &[u8],
    original_bytes: &[u8],
) -> Result<String, String> {
    if original_bytes != expected_bytes {
        return Err("Snapshot inherited predecessor differs from exact current prestate".into());
    }
    let root = parent
        .roots
        .get(&ArtifactRole::Snapshot)
        .ok_or("parent Snapshot missing")?
        .clone();
    let mut stack = vec![root.clone()];
    let mut seen = BTreeSet::new();
    while let Some(hash) = stack.pop() {
        if !seen.insert(hash.clone()) {
            continue;
        }
        if seen.len() > MAX_SNAPSHOT_DELTA_DEPTH + 1 {
            return Err("inherited Snapshot closure depth bound".into());
        }
        let object = parent
            .objects
            .iter()
            .find(|object| object.content_hash == hash)
            .ok_or("parent Snapshot dependency missing")?;
        let references = if let Some(artifact) = SnapshotArtifactV1::decode(&object.bytes)? {
            vec![artifact.base_ref]
        } else {
            vec![]
        };
        if object.references != references {
            return Err(
                "Snapshot dependency edges differ from declared artifact references".into(),
            );
        }
        stack.extend(references);
        objects.insert(hash, object.clone());
    }
    Ok(root)
}

pub(super) fn append(
    objects: &mut BTreeMap<String, ArtifactObject>,
    frozen: &ClosedRecord,
    before: &Snapshot,
    after: &Snapshot,
    raw_after: Vec<u8>,
) -> Result<String, String> {
    let base_ref = frozen
        .roots
        .get(&ArtifactRole::Snapshot)
        .ok_or("Frozen Snapshot missing")?
        .clone();
    let (base_snapshot, depth) =
        oasis7_runtime::resolve_snapshot_artifact_with_depth(&base_ref, &|hash| {
            capture::object(frozen, hash).map(<[u8]>::to_vec)
        })?;
    if to_cbor(&base_snapshot)? != to_cbor(before)? {
        return Err("Snapshot delta producer base differs from signed Frozen prestate".into());
    }
    if to_cbor(before)? == raw_after {
        return Ok(base_ref);
    }
    if depth < PRODUCER_MAX_SNAPSHOT_DELTA_DEPTH {
        // A changed field schema is a checkpoint boundary, never inferred state.
        if let Ok(delta) = SnapshotDeltaV1::between(before, after) {
            let artifact = SnapshotArtifactV1::new(base_ref.clone(), depth + 1, delta)?;
            let encoded = artifact.encode()?;
            if encoded.len() < raw_after.len() {
                let digest = oasis7_runtime::blake3_hex(&encoded);
                let rebuilt = oasis7_runtime::resolve_snapshot_artifact(&digest, &|hash| {
                    if hash == digest {
                        Ok(encoded.clone())
                    } else {
                        capture::object(frozen, hash).map(<[u8]>::to_vec)
                    }
                })
                .and_then(|snapshot| to_cbor(&snapshot));
                // Validate the exact prospective chain's recovery budget. A full
                // checkpoint is safe when adding a delta would exceed it.
                if rebuilt.as_deref() == Ok(raw_after.as_slice()) {
                    return Ok(insert(objects, encoded, vec![base_ref]));
                }
            }
        }
    }
    // Full checkpoint is a normal, exact Snapshot when depth or size requires it.
    Ok(insert(objects, raw_after, vec![]))
}

#[cfg(test)]
#[path = "formal_snapshot_tests.rs"]
mod tests;
