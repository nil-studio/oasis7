use super::super::*;

fn local_guardians() -> Vec<String> {
    vec![
        "governance.local.finality.signer.1".to_string(),
        "governance.local.finality.signer.2".to_string(),
    ]
}

fn queued_proposal_world() -> (World, ProposalId) {
    let mut world = World::new();
    let proposal_id = world
        .propose_manifest_update(
            Manifest {
                version: 2,
                content: serde_json::json!({ "name": "emergency-veto-transaction" }),
            },
            "alice",
        )
        .expect("propose manifest update");
    world.shadow_proposal(proposal_id).expect("shadow proposal");
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("queue proposal");
    (world, proposal_id)
}

#[test]
fn emergency_veto_publication_failure_is_atomic_and_retry_replays() {
    let (mut world, proposal_id) = queued_proposal_world();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let proposal_before = world.proposals().get(&proposal_id).cloned();
    let root_before = world.current_state_root_hash().expect("initial state root");

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .emergency_veto_proposal(
            proposal_id,
            "guardian-1",
            "unsafe parameter drift",
            local_guardians(),
        )
        .expect_err("injected veto publication failure must abort");
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id
    );
    assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);
    assert_eq!(
        world.proposals().get(&proposal_id).cloned(),
        proposal_before
    );
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());

    let replay_base = world.snapshot();
    world
        .emergency_veto_proposal(
            proposal_id,
            "guardian-1",
            "unsafe parameter drift",
            local_guardians(),
        )
        .expect("retry emergency veto");
    let proposal = world
        .proposals()
        .get(&proposal_id)
        .expect("vetoed proposal");
    let ProposalStatus::Rejected { reason } = &proposal.status else {
        panic!("proposal should be rejected");
    };
    assert_eq!(reason, "emergency_veto: unsafe parameter drift");
    assert!(proposal.queued_at_tick.is_none());
    assert!(proposal.not_before_tick.is_none());
    assert!(proposal.activate_epoch.is_none());
    assert_eq!(proposal.timelock_ticks, 0);
    let Some(WorldEvent {
        body:
            WorldEventBody::Governance(GovernanceEvent::EmergencyVetoed {
                initiator,
                reason,
                signer_node_ids,
                ..
            }),
        ..
    }) = world.journal().events.last()
    else {
        panic!("expected emergency veto event");
    };
    assert_eq!(initiator, "guardian-1");
    assert_eq!(reason, "unsafe parameter drift");
    assert_eq!(signer_node_ids, &local_guardians());
    assert_eq!(
        world.current_state_root_hash().expect("veto root"),
        root_before
    );

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay emergency veto publication");
    assert_eq!(restored.proposals(), world.proposals());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(
        restored.current_state_root_hash().expect("restored root"),
        world.current_state_root_hash().expect("live root")
    );
}

#[test]
fn emergency_veto_rejects_invalid_signer_and_unqueued_status_without_mutation() {
    let (mut world, proposal_id) = queued_proposal_world();
    let mut unqueued_snapshot = world.snapshot();
    let unqueued = unqueued_snapshot
        .proposals
        .get_mut(&proposal_id)
        .expect("queued proposal in snapshot");
    unqueued.queued_at_tick = None;
    unqueued.not_before_tick = None;
    unqueued.activate_epoch = None;
    unqueued.timelock_ticks = 0;
    let mut approved_unqueued = World::from_snapshot(unqueued_snapshot, world.journal().clone())
        .expect("restore approved unqueued proposal");
    let unqueued_before = approved_unqueued.snapshot();
    let unqueued_error = approved_unqueued
        .emergency_veto_proposal(proposal_id, "guardian-1", "not queued", local_guardians())
        .expect_err("approved but unqueued proposal must be rejected");
    assert!(matches!(
        unqueued_error,
        WorldError::GovernancePolicyInvalid { .. }
    ));
    assert_eq!(approved_unqueued.snapshot(), unqueued_before);

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let invalid_signer = world
        .emergency_veto_proposal(
            proposal_id,
            "guardian-1",
            "bad signer",
            vec![
                "governance.local.finality.signer.1".to_string(),
                "governance.unknown.signer".to_string(),
            ],
        )
        .expect_err("untrusted guardian must be rejected");
    assert!(matches!(
        invalid_signer,
        WorldError::GovernancePolicyInvalid { .. }
    ));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);

    world
        .emergency_veto_proposal(proposal_id, "guardian-1", "veto", local_guardians())
        .expect("valid veto");
    let snapshot_after_veto = world.snapshot();
    assert!(matches!(
        world
            .emergency_veto_proposal(
                proposal_id,
                "guardian-1",
                "duplicate veto",
                local_guardians(),
            )
            .expect_err("rejected proposal must not be vetoable"),
        WorldError::ProposalInvalidState { .. }
    ));
    assert_eq!(world.snapshot(), snapshot_after_veto);
}
