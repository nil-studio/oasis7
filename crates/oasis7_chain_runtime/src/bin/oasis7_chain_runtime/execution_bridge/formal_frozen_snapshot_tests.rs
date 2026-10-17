use super::*;

#[test]
fn original_snapshot_token_rejects_equal_clones_and_changed_rebuilds() {
    let (world, anchor) = wasm_fixture::world_and_anchor();
    let original = continuation(&world, anchor.head());
    let policy = ReleaseSecurityPolicy::default();
    let (_, mut source) = freeze_material(
        anchor.authority(),
        anchor.head(),
        &original,
        &world,
        &policy,
    )
    .unwrap();
    let target = world.snapshot();
    let mut base = target.clone();
    base.cognition = serde_json::json!({"original-base": true});
    let base_bytes = to_cbor(&base).unwrap();
    let base_ref = blake3_hex(&base_bytes);
    let artifact = oasis7::runtime::SnapshotArtifactV1::new(
        base_ref.clone(),
        1,
        oasis7::runtime::SnapshotDeltaV1::between(&base, &target).unwrap(),
    )
    .unwrap();
    let mut objects: BTreeMap<_, _> = source
        .objects
        .into_iter()
        .map(|object| (object.content_hash.clone(), object))
        .collect();
    insert(&mut objects, base_bytes, vec![]);
    let root = insert(&mut objects, artifact.encode().unwrap(), vec![base_ref]);
    source.objects = objects.into_values().collect();
    source.roots.insert(ArtifactRole::Snapshot, root);
    let (body, material) = freeze_material_inner(
        anchor.authority(),
        anchor.head(),
        &original,
        &world,
        &policy,
        None,
        None,
        None,
        Some(&source),
        None,
    )
    .unwrap();
    let input = SignedFormalFrozenV1 {
        signature_hex: fixtures::sign(frozen_signing_bytes(&body).unwrap(), 4),
        body,
    };
    let receiver = |role| FrozenReceiver {
        authority: anchor.authority().clone(),
        parent: anchor.head().clone(),
        role,
        retained: None,
    };
    let mut coordinator = FormalCoordinator::new(
        receiver(EndpointRole::Primary),
        receiver(EndpointRole::Replica),
        anchor.authority().clone(),
        anchor.authority().clone(),
        None,
        Some(anchor.head().clone()),
    )
    .unwrap();
    let frozen = coordinator.freeze(input, material).unwrap();
    let token = VerifiedFrozenSnapshot::new(&frozen, anchor.authority(), anchor.head()).unwrap();
    token
        .check_binding(&frozen, anchor.authority(), anchor.head())
        .unwrap();
    assert_eq!(
        token.bytes_for(frozen.material()).unwrap(),
        to_cbor(world.snapshot()).unwrap()
    );
    assert!(
        token
            .check_binding(&frozen.clone(), anchor.authority(), anchor.head())
            .is_err()
    );
    assert!(
        token
            .check_binding(&frozen, &anchor.authority().clone(), anchor.head())
            .is_err()
    );
    assert!(
        token
            .check_binding(&frozen, anchor.authority(), &anchor.head().clone())
            .is_err()
    );
    assert!(token.bytes_for(&frozen.material().clone()).is_err());
    token
        .verify_exact_rebuild(&frozen.input().body, frozen.material())
        .unwrap();
    let ordinary = freeze_material_inner(
        anchor.authority(),
        anchor.head(),
        &original,
        &world,
        &policy,
        None,
        None,
        None,
        Some(frozen.material()),
        None,
    )
    .unwrap();
    let reused = freeze_material_inner(
        anchor.authority(),
        anchor.head(),
        &original,
        &world,
        &policy,
        None,
        None,
        None,
        Some(frozen.material()),
        Some(&token),
    )
    .unwrap();
    token.verify_exact_rebuild(&reused.0, &reused.1).unwrap();
    assert_eq!(ordinary, reused);
    let mut body = frozen.input().body.clone();
    body.request_digest = fixtures::h(93);
    assert!(
        token
            .verify_exact_rebuild(&body, frozen.material())
            .is_err()
    );
    let mut changed = frozen.material().clone();
    let journal = changed.roots[&ArtifactRole::Journal].clone();
    changed
        .objects
        .iter_mut()
        .find(|object| object.content_hash == journal)
        .unwrap()
        .bytes
        .push(0);
    assert!(
        token
            .verify_exact_rebuild(&frozen.input().body, &changed)
            .is_err()
    );
    let mut missing = frozen.material().clone();
    let snapshot = missing.roots[&ArtifactRole::Snapshot].clone();
    missing
        .objects
        .retain(|object| object.content_hash != snapshot);
    assert!(verify_frozen_material_retaining_snapshot(&frozen.input().body, &missing).is_err());
    assert!(token.bytes_for(&missing).is_err());

    // The original token cannot authorize rebuilding a different typed World.
    let mut changed_world = world.snapshot().clone();
    changed_world.cognition = serde_json::json!({"changed-original-snapshot": true});
    let changed_world = World::from_snapshot(changed_world, world.journal().clone()).unwrap();
    assert!(
        freeze_material_inner(
            anchor.authority(),
            anchor.head(),
            &original,
            &changed_world,
            &policy,
            None,
            None,
            None,
            None,
            Some(&token),
        )
        .is_err()
    );
}
