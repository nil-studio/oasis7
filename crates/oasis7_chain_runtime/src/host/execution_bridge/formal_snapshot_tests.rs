use super::*;
use oasis7_runtime::{World, blake3_hex};
fn material(root: String, objects: &BTreeMap<String, ArtifactObject>) -> ClosedRecord {
    ClosedRecord {
        payload_digest: "00".repeat(32),
        before_state_root: "00".repeat(32),
        after_state_root: "00".repeat(32),
        roots: BTreeMap::from([(ArtifactRole::Snapshot, root)]),
        objects: objects.values().cloned().collect(),
    }
}
#[test]
fn sparse_snapshot_history_uses_bounded_checkpoints_and_exact_expanded_state_roots() {
    let mut before = World::new().snapshot();
    before
        .module_artifact_bytes
        .insert("unchanged-module".into(), vec![7; 65536]);
    let mut objects = BTreeMap::new();
    let raw = to_cbor(&before).unwrap();
    let root = insert(&mut objects, raw, vec![]);
    let mut frozen = material(root, &objects);
    for step in 1..=40 {
        let mut after = before.clone();
        after.next_action_id += 1;
        after.module_tick_schedule.insert("instance-a".into(), step);
        let raw_after = to_cbor(&after).unwrap();
        let state_root = blake3_hex(&raw_after);
        let mut inherited = BTreeMap::new();
        let base_ref = inherit(&mut inherited, &frozen, &to_cbor(&before).unwrap()).unwrap();
        let current = material(base_ref, &inherited);
        let snapshot_ref =
            append(&mut inherited, &current, &before, &after, raw_after.clone()).unwrap();
        frozen = material(snapshot_ref.clone(), &inherited);
        assert_eq!(bytes(&frozen).unwrap(), raw_after);
        assert_eq!(blake3_hex(&bytes(&frozen).unwrap()), state_root);
        let (_, depth) =
            oasis7_runtime::resolve_snapshot_artifact_with_depth(&snapshot_ref, &|hash| {
                capture::object(&frozen, hash).map(<[u8]>::to_vec)
            })
            .unwrap();
        assert_eq!(
            depth,
            step as usize % (PRODUCER_MAX_SNAPSHOT_DELTA_DEPTH + 1)
        );
        if depth > 0 {
            assert_ne!(snapshot_ref, state_root);
        }
        assert!(inherited.len() <= PRODUCER_MAX_SNAPSHOT_DELTA_DEPTH + 2);
        if depth == PRODUCER_MAX_SNAPSHOT_DELTA_DEPTH {
            assert_eq!(
                append(&mut inherited, &frozen, &after, &after, raw_after).unwrap(),
                snapshot_ref,
                "an unchanged depth-8 Snapshot must keep its original root"
            );
        }
        before = after;
    }
}

#[test]
fn historical_depths_through_thirty_two_remain_readable_and_inheritable() {
    assert_eq!(MAX_SNAPSHOT_DELTA_DEPTH, 32);
    let mut before = World::new().snapshot();
    let mut objects = BTreeMap::new();
    let mut root = insert(&mut objects, to_cbor(&before).unwrap(), vec![]);
    for depth in 1..=32 {
        let mut after = before.clone();
        after.next_action_id += 1;
        let artifact = SnapshotArtifactV1::new(
            root.clone(),
            depth,
            SnapshotDeltaV1::between(&before, &after).unwrap(),
        )
        .unwrap();
        root = insert(&mut objects, artifact.encode().unwrap(), vec![root]);
        let old = material(root.clone(), &objects);
        assert_eq!(bytes(&old).unwrap(), to_cbor(&after).unwrap());
        let mut inherited = BTreeMap::new();
        assert_eq!(
            inherit(&mut inherited, &old, &to_cbor(&after).unwrap()).unwrap(),
            root
        );
        assert_eq!(inherited.len(), depth + 1);
        before = after;
    }
}

#[test]
fn checkpoint_transition_keeps_the_original_closed_capture_budget() {
    let mut before = World::new().snapshot();
    before
        .module_artifact_bytes
        .insert("budget-fixture".into(), vec![7; 5 * 1024 * 1024]);
    let mut objects = BTreeMap::new();
    let mut root = insert(&mut objects, to_cbor(&before).unwrap(), vec![]);
    for depth in 1..=PRODUCER_MAX_SNAPSHOT_DELTA_DEPTH {
        let mut after = before.clone();
        after.next_action_id += 1;
        let artifact = SnapshotArtifactV1::new(
            root.clone(),
            depth,
            SnapshotDeltaV1::between(&before, &after).unwrap(),
        )
        .unwrap();
        root = insert(&mut objects, artifact.encode().unwrap(), vec![root]);
        before = after;
    }
    let original = material(root.clone(), &objects);
    let mut after = before.clone();
    after
        .module_artifact_bytes
        .get_mut("budget-fixture")
        .unwrap()
        .fill(8);
    let raw = to_cbor(&after).unwrap();
    let checkpoint = append(&mut objects, &original, &before, &after, raw.clone()).unwrap();
    assert_eq!(checkpoint, blake3_hex(&raw));
    assert!(
        objects.contains_key(&root),
        "checkpoint must retain the original prestate chain"
    );
    let transition = material(checkpoint, &objects);
    assert_eq!(bytes(&transition).unwrap(), raw);
    let encoded = to_cbor(&transition).unwrap();
    assert!(encoded.len() > 8 * 1024 * 1024);
    assert!(
        capture::validate_package_budget(&encoded).is_err(),
        "earlier checkpoints must not relax the existing closed capture budget"
    );
}
#[test]
fn snapshot_closure_rejects_wrong_prestate_missing_base_and_declared_edge_mismatch() {
    let before = World::new().snapshot();
    let mut after = before.clone();
    after.next_action_id += 1;
    let mut objects = BTreeMap::new();
    let root = insert(&mut objects, to_cbor(&before).unwrap(), vec![]);
    let frozen = material(root.clone(), &objects);
    assert_eq!(
        append(
            &mut objects.clone(),
            &frozen,
            &before,
            &before,
            to_cbor(&before).unwrap()
        )
        .unwrap(),
        root
    );
    assert!(inherit(&mut BTreeMap::new(), &frozen, &to_cbor(&after).unwrap()).is_err());
    let artifact = SnapshotArtifactV1::new(
        root.clone(),
        1,
        SnapshotDeltaV1::between(&before, &after).unwrap(),
    )
    .unwrap();
    let encoded = artifact.encode().unwrap();
    let artifact_ref = insert(&mut objects, encoded.clone(), vec![root.clone()]);
    let complete = material(artifact_ref.clone(), &objects);
    assert_eq!(complete.snapshot_bytes().unwrap(), to_cbor(&after).unwrap());
    let mut bad = complete.clone();
    bad.objects.retain(|object| object.content_hash != root);
    assert!(bad.snapshot_bytes().is_err());
    let mut bad = complete;
    bad.objects
        .iter_mut()
        .find(|object| object.content_hash == artifact_ref)
        .unwrap()
        .references
        .clear();
    assert!(bad.snapshot_bytes().is_err());
}
