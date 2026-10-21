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
    resolve_snapshot_artifact_with_depth_inner(reference, reader)
}
fn resolve_snapshot_artifact_with_depth_inner(
    reference: &str,
    reader: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<(Snapshot, usize), String> {
    let (bytes, depth) =
        oasis7_formal_protocol::snapshot_delta::resolve_snapshot_artifact_bytes_with_depth(
            reference, &reader,
        )?;
    let snapshot = crate::codec::snapshot_from_cbor(&bytes).map_err(|e| e.to_string())?;
    if crate::codec::snapshot_cbor(&snapshot).map_err(|e| e.to_string())? != bytes {
        return Err("snapshot artifact typed canonical encoding mismatch".into());
    }
    Ok((snapshot, depth))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_artifact_preserves_snapshot_and_reader_errors() {
        let snapshot =
            crate::codec::snapshot_from_json(include_bytes!("../fixtures/snapshot.json")).unwrap();
        let bytes = crate::codec::snapshot_cbor(&snapshot).unwrap();
        let reference = blake3::hash(&bytes).to_hex().to_string();
        assert_eq!(
            resolve_snapshot_artifact_with_depth(&reference, &|_| Ok(bytes.clone())).unwrap(),
            (snapshot, 0)
        );
        let error = "fixture reader failure".to_string();
        assert_eq!(
            resolve_snapshot_artifact(&reference, &|_| Err(error.clone())),
            Err(error)
        );
    }
}
