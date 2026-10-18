use super::super::*;

fn proposal_manifest() -> Manifest {
    Manifest {
        version: 2,
        content: serde_json::json!({ "name": "proposal-transaction" }),
    }
}

fn assert_publication_unchanged(
    world: &World,
    snapshot: &Snapshot,
    journal: &Journal,
    consensus: &[TickConsensusRecord],
) {
    assert_eq!(&world.snapshot(), snapshot);
    assert_eq!(world.snapshot().last_event_id, snapshot.last_event_id);
    assert_eq!(world.snapshot().next_proposal_id, snapshot.next_proposal_id);
    assert_eq!(world.journal(), journal);
    assert_eq!(world.tick_consensus_records(), consensus);
}

#[test]
fn proposal_creation_publication_failure_is_atomic_and_retry_reuses_id() {
    let mut world = World::new();
    let manifest = proposal_manifest();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .propose_manifest_update(manifest.clone(), "alice")
        .expect_err("injected proposal publication failure must abort");
    assert_publication_unchanged(
        &world,
        &snapshot_before,
        &journal_before,
        consensus_before.as_slice(),
    );
    assert!(world.proposals().is_empty());

    let replay_base = world.snapshot();
    let proposal_id = world
        .propose_manifest_update(manifest, "alice")
        .expect("retry proposal publication");
    assert_eq!(proposal_id, snapshot_before.next_proposal_id);
    assert_eq!(world.snapshot().last_event_id, 1);
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Governance(GovernanceEvent::Proposed {
            proposal_id: event_proposal_id,
            ..
        })) if *event_proposal_id == proposal_id
    ));
    let state_root = world.current_state_root_hash().expect("proposal root");
    let consensus = world
        .tick_consensus_records()
        .last()
        .expect("proposal consensus record");
    assert_eq!(consensus.block.header.state_root, state_root);
    assert_eq!(
        consensus.block.execution_digest.state_projection_hash,
        state_root
    );

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay proposal publication");
    assert_eq!(restored.proposals(), world.proposals());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
}

#[test]
fn proposal_shadow_publication_failure_is_atomic_and_retry_replays() {
    let mut world = World::new();
    let proposal_id = world
        .propose_manifest_update(proposal_manifest(), "alice")
        .expect("proposal");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let proposal_before = world.proposals().get(&proposal_id).cloned();

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .shadow_proposal(proposal_id)
        .expect_err("injected shadow publication failure must abort");
    assert_publication_unchanged(
        &world,
        &snapshot_before,
        &journal_before,
        consensus_before.as_slice(),
    );
    assert_eq!(
        world.proposals().get(&proposal_id).cloned(),
        proposal_before
    );

    let replay_base = world.snapshot();
    let manifest_hash = world.shadow_proposal(proposal_id).expect("retry shadow");
    let proposal = world
        .proposals()
        .get(&proposal_id)
        .expect("shadowed proposal");
    assert_eq!(
        proposal.status,
        ProposalStatus::Shadowed {
            manifest_hash: manifest_hash.clone()
        }
    );
    let Some(WorldEvent {
        body:
            WorldEventBody::Governance(GovernanceEvent::ShadowReport {
                proposal_id: event_proposal_id,
                manifest_hash: event_manifest_hash,
            }),
        ..
    }) = world.journal().events.last()
    else {
        panic!("expected shadow report event");
    };
    assert_eq!(*event_proposal_id, proposal_id);
    assert_eq!(event_manifest_hash, &manifest_hash);
    let state_root = world.current_state_root_hash().expect("shadow root");
    let consensus = world
        .tick_consensus_records()
        .last()
        .expect("shadow consensus record");
    assert_eq!(consensus.block.header.state_root, state_root);
    assert_eq!(
        consensus.block.execution_digest.state_projection_hash,
        state_root
    );

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay shadow publication");
    assert_eq!(restored.proposals(), world.proposals());
    assert_eq!(restored.journal(), world.journal());
}

#[test]
fn proposal_validation_rejects_invalid_status_hash_and_patch_without_mutation() {
    let mut world = World::new();
    let proposal_id = world
        .propose_manifest_update(proposal_manifest(), "alice")
        .expect("proposal");
    world.shadow_proposal(proposal_id).expect("shadow proposal");

    let status_snapshot = world.snapshot();
    let status_journal = world.journal().clone();
    let status_consensus = world.tick_consensus_records().to_vec();
    assert!(matches!(
        world.shadow_proposal(proposal_id),
        Err(WorldError::ProposalInvalidState { .. })
    ));
    assert_publication_unchanged(
        &world,
        &status_snapshot,
        &status_journal,
        status_consensus.as_slice(),
    );

    let patch_snapshot = world.snapshot();
    let patch_journal = world.journal().clone();
    let patch_consensus = world.tick_consensus_records().to_vec();
    let patch = ManifestPatch {
        base_manifest_hash: "not-the-current-manifest".to_string(),
        ops: vec![ManifestPatchOp::Set {
            path: vec!["settings".to_string(), "mode".to_string()],
            value: serde_json::json!("invalid"),
        }],
        new_version: Some(3),
    };
    assert!(matches!(
        world.propose_manifest_patch(patch, "alice"),
        Err(WorldError::PatchBaseMismatch { .. })
    ));
    assert_publication_unchanged(
        &world,
        &patch_snapshot,
        &patch_journal,
        patch_consensus.as_slice(),
    );
}

#[test]
fn proposal_id_rollover_is_installed_and_replayed() {
    let seed = World::new();
    let mut snapshot = seed.snapshot();
    snapshot.next_proposal_id = u64::MAX;
    snapshot.proposal_id_era = 7;
    let mut world = World::from_snapshot(snapshot, seed.journal().clone()).expect("restore max id");
    let replay_base = world.snapshot();

    let proposal_id = world
        .propose_manifest_update(proposal_manifest(), "alice")
        .expect("max proposal id");
    assert_eq!(proposal_id, u64::MAX);
    assert_eq!(world.snapshot().next_proposal_id, 1);
    assert_eq!(world.snapshot().proposal_id_era, 8);
    let state_root = world.current_state_root_hash().expect("rollover root");
    let consensus = world
        .tick_consensus_records()
        .last()
        .expect("rollover consensus record");
    assert_eq!(consensus.block.header.state_root, state_root);
    assert_eq!(
        consensus.block.execution_digest.state_projection_hash,
        state_root
    );

    let restored = World::from_snapshot(replay_base.clone(), world.journal().clone())
        .expect("replay max proposal id");
    assert_eq!(restored.proposals(), world.proposals());
    assert_eq!(restored.snapshot().next_proposal_id, 1);
    assert_eq!(restored.snapshot().proposal_id_era, 8);
    assert_eq!(restored.journal(), world.journal());
    let delta = SnapshotDeltaV1::between(&replay_base, &world.snapshot()).unwrap();
    let mut delta_restored =
        World::from_snapshot(delta.apply(&replay_base).unwrap(), world.journal().clone()).unwrap();
    assert_eq!(delta_restored.snapshot(), world.snapshot());
    let live_next = world
        .propose_manifest_update(proposal_manifest(), "alice")
        .unwrap();
    let restored_next = delta_restored
        .propose_manifest_update(proposal_manifest(), "alice")
        .unwrap();
    assert_eq!(live_next, 1);
    assert_eq!(live_next, restored_next);
    assert_eq!(delta_restored.snapshot(), world.snapshot());
    assert_eq!(delta_restored.journal(), world.journal());
}

#[test]
fn malformed_module_change_rejects_shadow_without_mutation() {
    let mut world = World::new();
    let proposal_id = world
        .propose_manifest_update(
            Manifest {
                version: 2,
                content: serde_json::json!({ "module_changes": { "register": true } }),
            },
            "alice",
        )
        .expect("malformed proposal payload is journalable");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();

    assert!(matches!(
        world.shadow_proposal(proposal_id),
        Err(WorldError::ModuleChangeInvalid { .. })
    ));
    assert_publication_unchanged(
        &world,
        &snapshot_before,
        &journal_before,
        consensus_before.as_slice(),
    );
}
