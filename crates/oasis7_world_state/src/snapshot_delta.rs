//! Typed snapshot access over the complete byte-preserving protocol delta.
use super::Snapshot;
pub use oasis7_formal_protocol::snapshot_delta::{
    MAX_SNAPSHOT_CHAIN_BYTES, MAX_SNAPSHOT_DELTA_BYTES, MAX_SNAPSHOT_DELTA_DEPTH,
    SnapshotArtifactV1, SnapshotDeltaV1,
};
pub fn resolve_snapshot_artifact(
    reference: &str,
    reader: &impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<Snapshot, String> {
    resolve_snapshot_artifact_with_depth(reference, reader).map(|(snapshot, _)| snapshot)
}
pub fn resolve_snapshot_artifact_with_depth(
    reference: &str,
    reader: &impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<(Snapshot, usize), String> {
    let (bytes, depth) =
        oasis7_formal_protocol::snapshot_delta::resolve_snapshot_artifact_bytes_with_depth(
            reference, reader,
        )?;
    let snapshot = serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
    if serde_cbor::to_vec(&snapshot).map_err(|e| e.to_string())? != bytes {
        return Err("snapshot artifact typed canonical encoding mismatch".into());
    }
    Ok((snapshot, depth))
}
