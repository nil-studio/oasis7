//! Real closure-walker regression: signed roots contain a shared index object
//! reachable first through a valid edge and later through an invalid depth edge.
use super::*;
#[derive(serde::Serialize, serde::Deserialize)]
enum GraphIndex {
    Leaf {
        request: String,
        entry: String,
    },
    Fork {
        bit: u16,
        left: String,
        right: String,
    },
}
fn put_graph_object(directory: &std::path::Path, value: &GraphIndex) -> String {
    let bytes = serde_cbor::to_vec(value).unwrap();
    let id = blake3::hash(&bytes).to_hex().to_string();
    let path = directory.join(&id);
    fs::write(&path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    id
}
#[test]
fn healthy_copy_walker_rejects_reused_index_branch_under_invalid_second_edge() {
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    coordinator.submit(fixture.initial.clone()).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    let objects = fixture.pdir.join("formal-objects-v1");
    let original_leaf = primary.store.body.index.as_ref().unwrap();
    let bytes = fs::read(objects.join(original_leaf)).unwrap();
    let GraphIndex::Leaf { request, .. } = serde_cbor::from_slice(&bytes).unwrap() else {
        panic!("one qualified request produces an index leaf");
    };
    let request_bytes = hex::decode(&request).unwrap();
    // The real authenticated history lookup takes the right (valid) branch.
    // Walking all index edges then visits this same object via the left branch.
    let root_bit = request_bytes
        .iter()
        .enumerate()
        .find_map(|(i, byte)| {
            (*byte != 0).then_some((i * 8 + byte.leading_zeros() as usize) as u16)
        })
        .unwrap();
    assert!(root_bit < 250);
    let shared = put_graph_object(
        &objects,
        &GraphIndex::Fork {
            bit: root_bit + 2,
            left: original_leaf.clone(),
            right: original_leaf.clone(),
        },
    );
    let deeper = put_graph_object(
        &objects,
        &GraphIndex::Fork {
            bit: root_bit + 3,
            left: shared.clone(),
            right: original_leaf.clone(),
        },
    );
    let root = put_graph_object(
        &objects,
        &GraphIndex::Fork {
            bit: root_bit,
            left: deeper,
            right: shared,
        },
    );
    let source = FileFormalArchiveSourceV1::new(fixture.pdir.clone()).unwrap();
    let mut body = primary.store.body.clone();
    // This test only provisions a fixture signature for the parser's signed
    // input. It does not manufacture an authority/floor/activation token.
    let signed_checkpoint = |body: &HotBody| {
        Store::encode_provider_checkpoint(
            body,
            &signed(Store::checkpoint_signing_bytes(body).unwrap(), 5),
            &public(5),
        )
        .unwrap()
    };
    super::super::store::durable_set::copy_authenticated_closure(
        &fixture.pdir,
        &signed_checkpoint(&body),
        &public(5),
        &source,
        1,
        64 * 1024 * 1024,
        false,
    )
    .unwrap();
    body.index = Some(root);
    let error = super::super::store::durable_set::copy_authenticated_closure(
        &fixture.pdir,
        &signed_checkpoint(&body),
        &public(5),
        &source,
        1,
        64 * 1024 * 1024,
        false,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("healthy copy index depth"),
        "{error}"
    );
    // The old (tag,id) marker skipped `shared` on that second edge and returned
    // success. The contextual marker reaches the actual walker depth rejection.
    let error = super::super::store::durable_set::copy_authenticated_closure(
        &fixture.pdir,
        &signed_checkpoint(&body),
        &public(5),
        &source,
        1,
        64 * 1024 * 1024,
        true,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("healthy copy index depth"),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn healthy_copy_walker_rejects_preexisting_writable_staging() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    coordinator.submit(fixture.initial.clone()).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    let source = FileFormalArchiveSourceV1::new(fixture.pdir.clone()).unwrap();
    let native = Store::read_native_checkpoint(&fixture.pdir).unwrap();
    let staging = fixture.pdir.join(".formal-durable-set-staging");
    fs::create_dir(&staging).unwrap();
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o777)).unwrap();
    let result = super::super::store::durable_set::copy_authenticated_closure(
        &fixture.pdir,
        &native,
        &public(5),
        &source,
        1,
        64 * 1024 * 1024,
        false,
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("staging private owner/mode")
    );
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).unwrap();
    super::super::store::durable_set::copy_authenticated_closure(
        &fixture.pdir,
        &native,
        &public(5),
        &source,
        1,
        64 * 1024 * 1024,
        false,
    )
    .unwrap();
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GraphHistory {
    head: FormalHeadRefV1,
    request: String,
    entry: String,
    ancestors: Vec<GraphHistoryLink>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct GraphHistoryLink {
    head: FormalHeadRefV1,
    node: String,
}
#[test]
fn healthy_copy_walker_reuses_entry_intrinsics_but_rejects_wrong_second_history_edge() {
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    coordinator.submit(fixture.initial.clone()).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    let source = FileFormalArchiveSourceV1::new(fixture.pdir.clone()).unwrap();
    let mut body = primary.store.body.clone();
    let signed_checkpoint = |body: &HotBody| {
        Store::encode_provider_checkpoint(
            body,
            &signed(Store::checkpoint_signing_bytes(body).unwrap(), 5),
            &public(5),
        )
        .unwrap()
    };
    let walk = |body: &HotBody, copy| {
        super::super::store::durable_set::copy_authenticated_closure(
            &fixture.pdir,
            &signed_checkpoint(body),
            &public(5),
            &source,
            1,
            64 * 1024 * 1024,
            copy,
        )
    };
    walk(&body, false).unwrap();
    // One genuine Qualified entry is reached from both signed history and index.
    assert_eq!(
        super::super::store::durable_set::entry_validation_counts(),
        (1, 1)
    );
    let objects = fixture.pdir.join("formal-objects-v1");
    let original_id = body.head_node.as_ref().unwrap().clone();
    let original: GraphHistory =
        serde_cbor::from_slice(&fs::read(objects.join(&original_id)).unwrap()).unwrap();
    let wrong_head = FormalHeadRefV1 {
        position: original.head.position + 1,
        record_id: h(211),
    };
    let root = GraphHistory {
        head: wrong_head.clone(),
        request: original.request,
        entry: original.entry,
        ancestors: vec![GraphHistoryLink {
            head: original.head,
            node: original_id,
        }],
    };
    let bytes = serde_cbor::to_vec(&root).unwrap();
    let id = blake3::hash(&bytes).to_hex().to_string();
    let path = objects.join(&id);
    fs::write(&path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    body.head = wrong_head.clone();
    body.head_node = Some(id);
    let seal = body.seal.as_mut().unwrap();
    seal.highest_complete.position = wrong_head.position;
    seal.highest_complete.decision_hash = wrong_head.record_id;
    // Fixture signature supplies malicious authenticated roots to the real
    // parser, never a production authority/floor/activation token. DFS first
    // validates the real ancestor entry, then encounters the same bytes under
    // this root's wrong head. The compact cache must reject that second context.
    let error = walk(&body, false).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("qualified material/history binding"),
        "{error}"
    );
    assert_eq!(
        super::super::store::durable_set::entry_validation_counts(),
        (1, 1)
    );
    // Content memo hits during a copy also need the second history edge's
    // original head binding, even though the Entry bytes are unchanged.
    let error = walk(&body, true).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("qualified material/history binding"),
        "{error}"
    );
    assert_eq!(
        super::super::store::durable_set::entry_validation_counts(),
        (1, 1)
    );
}

#[cfg(unix)]
#[test]
fn healthy_copy_walker_preserves_raw_native_codecs_and_independently_rechecks_all_logical_objects()
{
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    coordinator.submit(fixture.initial.clone()).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    let native = Store::read_native_checkpoint(&fixture.pdir).unwrap();
    let source = FileFormalArchiveSourceV1::new(fixture.pdir.clone()).unwrap();
    let target = fixture.pdir.join("independent-copy-target");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
    let copied = super::super::store::durable_set::copy_authenticated_closure(
        &target,
        &native,
        &public(5),
        &source,
        1,
        64 * 1024 * 1024,
        true,
    )
    .unwrap();
    let target_source = FileFormalArchiveSourceV1::new(target.clone()).unwrap();
    let verified = super::super::store::durable_set::copy_authenticated_closure(
        &target,
        &native,
        &public(5),
        &target_source,
        1,
        64 * 1024 * 1024,
        false,
    )
    .unwrap();
    assert_eq!(copied, verified);
    let objects = target.join("formal-objects-v1");
    let mut physical = std::collections::BTreeSet::new();
    let mut logical = 0;
    let mut first = None;
    for entry in fs::read_dir(&objects).unwrap() {
        let entry = entry.unwrap();
        let id = entry.file_name().to_string_lossy().into_owned();
        if id.len() != 64 {
            continue;
        }
        let metadata = entry.metadata().unwrap();
        physical.insert((metadata.dev(), metadata.ino()));
        logical += 1;
        let bytes = target_source.read_object(&id).unwrap().unwrap();
        assert_eq!(blake3::hash(&bytes).to_hex().as_str(), id);
        assert_eq!(bytes, source.read_object(&id).unwrap().unwrap());
        let original_root = fixture.pdir.join("formal-objects-v1");
        let canonical = original_root.join(&id);
        let original = fs::metadata(if canonical.exists() {
            canonical
        } else {
            original_root.join(".representation-v1").join(&id)
        })
        .unwrap();
        assert_ne!(
            (metadata.dev(), metadata.ino()),
            (original.dev(), original.ino())
        );
        first.get_or_insert(entry.path());
    }
    assert_eq!(logical, copied.logical_objects);
    assert_eq!(
        physical.len(),
        logical as usize,
        "profile-1 native objects retain raw codecs"
    );
    // Repeating publication still validates existing immutable logical bytes.
    super::super::store::durable_set::copy_authenticated_closure(
        &target,
        &native,
        &public(5),
        &source,
        1,
        64 * 1024 * 1024,
        true,
    )
    .unwrap();
    fs::write(first.unwrap(), b"corrupted independent native object").unwrap();
    assert!(
        super::super::store::durable_set::copy_authenticated_closure(
            &target,
            &native,
            &public(5),
            &target_source,
            1,
            64 * 1024 * 1024,
            false,
        )
        .is_err()
    );
    assert!(
        super::super::store::durable_set::copy_authenticated_closure(
            &target,
            &native,
            &public(5),
            &source,
            1,
            64 * 1024 * 1024,
            true,
        )
        .is_err()
    );
}
