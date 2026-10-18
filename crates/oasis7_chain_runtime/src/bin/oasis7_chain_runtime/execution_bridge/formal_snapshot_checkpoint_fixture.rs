//! Authenticated execution fixture whose exact prestate uses an older delta chain.
use super::*;

pub(super) fn frozen_at_snapshot_depth(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    continuation: &NodeLocalExecutionContinuation,
    world: &World,
    depth: usize,
) -> (FormalFrozenBodyV1, ClosedRecord) {
    let policy = ReleaseSecurityPolicy::default();
    let (_, mut source) = freeze_material(authority, parent, continuation, world, &policy).unwrap();
    let target = world.snapshot();
    let mut before = target.clone();
    before
        .module_tick_schedule
        .insert("checkpoint-fixture".into(), 0);
    let mut objects = BTreeMap::new();
    let mut root = insert(&mut objects, to_cbor(&before).unwrap(), vec![]);
    for level in 1..=depth {
        let mut after = target.clone();
        if level < depth {
            after
                .module_tick_schedule
                .insert("checkpoint-fixture".into(), level as u64);
        }
        let artifact = oasis7_runtime::SnapshotArtifactV1::new(
            root.clone(),
            level,
            oasis7_runtime::SnapshotDeltaV1::between(&before, &after).unwrap(),
        )
        .unwrap();
        root = insert(&mut objects, artifact.encode().unwrap(), vec![root]);
        before = after;
    }
    source.roots.insert(ArtifactRole::Snapshot, root);
    source.objects = objects.into_values().collect();
    freeze_material_inner(
        authority,
        parent,
        continuation,
        world,
        &policy,
        None,
        None,
        None,
        Some(&source),
        None,
    )
    .unwrap()
}
