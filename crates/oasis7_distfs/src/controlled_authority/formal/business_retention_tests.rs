use super::*;
use crate::controlled_authority::replicated_protocol::ArtifactObject;
use std::collections::BTreeMap;
struct Nodes(BTreeMap<String, Vec<u8>>);
impl BusinessIndexNodeReader for Nodes {
    fn read_node(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(id).cloned())
    }
}
fn root(index_root: BusinessIndexRootV1) -> RetainedRoot {
    let value =
        serde_cbor::value::to_value(("oasis7.formal.retained-history.root.v1", 1u32, &index_root))
            .unwrap();
    RetainedRoot {
        schema_version: 1,
        index_root,
        root_digest: blake3::hash(&serde_cbor::to_vec(&value).unwrap())
            .to_hex()
            .to_string(),
    }
}
fn material(root: Option<RetainedRoot>) -> ClosedRecord {
    #[derive(Serialize)]
    struct Snapshot {
        state: State,
    }
    #[derive(Serialize)]
    struct State {
        retained_history_root: Option<RetainedRoot>,
    }
    let bytes = serde_cbor::to_vec(&Snapshot {
        state: State {
            retained_history_root: root,
        },
    })
    .unwrap();
    let hash = blake3::hash(&bytes).to_hex().to_string();
    ClosedRecord {
        payload_digest: hash.clone(),
        before_state_root: hash.clone(),
        after_state_root: hash.clone(),
        roots: BTreeMap::from([(ArtifactRole::Snapshot, hash.clone())]),
        objects: vec![ArtifactObject {
            content_hash: hash,
            bytes,
            references: vec![],
        }],
    }
}
#[test]
fn retention_delta_uses_closed_before_after_and_rejects_missing_changed_corrupt_roots() {
    use super::super::business_index::*;
    let empty = BusinessIndexRootV1::empty().unwrap();
    let reader = Nodes(BTreeMap::new());
    let before = material(Some(root(empty.clone())));
    assert!(verify_retained_transition(true, &before, None, None, &reader).is_ok());
    let delta = BusinessIndexDeltaV1::build(
        empty,
        vec![BusinessIndexLeafV1 {
            key_digest: metadata_key_digest(),
            kind: BusinessIndexLeafKindV1::CommitMetadata,
            canonical_value: b"archive metadata".to_vec(),
        }],
        &reader,
    )
    .unwrap();
    let after = material(Some(root(delta.new.clone())));
    assert!(
        verify_retained_transition(false, &after, Some(&before), Some(&delta), &reader).is_ok()
    );
    assert!(verify_retained_transition(false, &after, Some(&before), None, &reader).is_err());
    assert!(
        verify_retained_transition(false, &material(None), Some(&before), None, &reader).is_err()
    );
    assert!(verify_retained_transition(true, &after, None, Some(&delta), &reader).is_err());
    assert!(verify_retained_transition(false, &after, Some(&after), None, &reader).is_err());
    let populated = Nodes(
        delta
            .node_writes
            .iter()
            .map(|write| (write.digest.clone(), write.bytes.clone()))
            .collect(),
    );
    assert!(verify_retained_transition(false, &after, Some(&after), None, &populated).is_ok());
    let mut invalid_root = root(delta.new.clone());
    invalid_root.root_digest = "00".repeat(32);
    assert!(
        verify_retained_transition(
            false,
            &material(Some(invalid_root)),
            Some(&before),
            Some(&delta),
            &reader
        )
        .is_err()
    );
    let mut altered = delta;
    altered.base_root = "00".repeat(32);
    assert!(
        verify_retained_transition(false, &after, Some(&before), Some(&altered), &reader).is_err()
    );
}

#[test]
fn retained_original_bytes_path_preserves_delta_and_prefix_validation() {
    use super::super::business_index::*;
    let reader = Nodes(BTreeMap::new());
    let empty = BusinessIndexRootV1::empty().unwrap();
    let before = material(Some(root(empty.clone())));
    let delta = BusinessIndexDeltaV1::build(
        empty,
        vec![BusinessIndexLeafV1 {
            key_digest: metadata_key_digest(),
            kind: BusinessIndexLeafKindV1::CommitMetadata,
            canonical_value: b"archive".to_vec(),
        }],
        &reader,
    )
    .unwrap();
    let after = material(Some(root(delta.new.clone())));
    let before_bytes = before.snapshot_bytes().unwrap();
    let after_bytes = after.snapshot_bytes().unwrap();
    assert!(
        verify_retained_transition(false, &after, Some(&before), Some(&delta), &reader).is_ok()
    );
    assert!(
        verify_retained_transition_from_bytes(
            false,
            &after_bytes,
            Some(&before_bytes),
            Some(&delta),
            &reader
        )
        .is_ok()
    );
    assert!(
        verify_retained_transition_from_bytes(
            false,
            &after_bytes,
            Some(&before_bytes),
            None,
            &reader
        )
        .is_err()
    );
    assert!(
        verify_retained_transition_from_bytes(
            false,
            &before_bytes,
            Some(&after_bytes),
            Some(&delta),
            &reader
        )
        .is_err()
    );
    let mut wrong_delta = delta.clone();
    wrong_delta.base_root = "00".repeat(32);
    assert!(
        verify_retained_transition_from_bytes(
            false,
            &after_bytes,
            Some(&before_bytes),
            Some(&wrong_delta),
            &reader
        )
        .is_err()
    );
    // A malformed hot Tick certificate must still be rejected by the original
    // TickSnapshotMaterial decoder, even when retained roots/delta are valid.
    let mut changed: serde_cbor::Value = serde_cbor::from_slice(&after_bytes).unwrap();
    let serde_cbor::Value::Map(ref mut map) = changed else {
        panic!("snapshot map")
    };
    map.insert(
        serde_cbor::Value::Text("tick_consensus_records".into()),
        serde_cbor::Value::Array(vec![serde_cbor::Value::Null]),
    );
    map.insert(
        serde_cbor::Value::Text("tick_consensus_total_record_count".into()),
        serde_cbor::Value::Integer(1),
    );
    let malformed = serde_cbor::to_vec(&changed).unwrap();
    assert!(
        verify_retained_transition_from_bytes(
            false,
            &malformed,
            Some(&before_bytes),
            Some(&delta),
            &reader
        )
        .is_err()
    );
}
