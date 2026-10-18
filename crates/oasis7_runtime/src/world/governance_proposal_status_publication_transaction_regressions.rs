use super::super::*;
use super::World;

fn proposal(id: ProposalId, status: ProposalStatus) -> Proposal {
    Proposal {
        id,
        author: "fixture-author".into(),
        base_manifest_hash: "fixture-base".into(),
        manifest: Manifest {
            version: id,
            content: serde_json::json!({"proposal": id}),
        },
        patch: None,
        queued_at_tick: None,
        not_before_tick: None,
        activate_epoch: None,
        timelock_ticks: 0,
        status,
    }
}

fn approved_event(id: ProposalId, decision: ProposalDecision) -> GovernanceEvent {
    GovernanceEvent::Approved {
        proposal_id: id,
        approver: "fixture-approver".into(),
        decision,
    }
}

fn queued_event(id: ProposalId, hash: &str, queued: u64, not_before: u64) -> GovernanceEvent {
    GovernanceEvent::Queued {
        proposal_id: id,
        manifest_hash: hash.into(),
        queued_at_tick: queued,
        not_before_tick: not_before,
        activate_epoch: 7,
        timelock_ticks: not_before.saturating_sub(queued),
    }
}

fn applied_event(id: ProposalId, hash: Option<&str>) -> GovernanceEvent {
    GovernanceEvent::Applied {
        proposal_id: id,
        manifest_hash: hash.map(str::to_string),
        consensus_height: Some(17),
        threshold: Some(2),
        signer_node_ids: vec!["validator-a".into(), "validator-b".into()],
    }
}

fn assert_world_unchanged(world: &World, expected: &World, expected_root: &str) {
    assert_eq!(world.snapshot(), expected.snapshot());
    assert_eq!(world.proposals, expected.proposals);
    assert_eq!(world.manifest, expected.manifest);
    assert_eq!(world.journal(), expected.journal());
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (expected.next_event_id, expected.next_event_id_era)
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected.tick_consensus_records()
    );
    assert_eq!(world.state.time, expected.state.time);
    assert_eq!(
        world.current_state_root_hash().expect("current state root"),
        expected_root
    );
}

fn assert_injected(error: &WorldError) {
    assert!(
        matches!(
            error,
            WorldError::ResourceBalanceInvalid { reason }
                if reason == "injected append_event failure after publication preparation"
        ),
        "unexpected error: {error:?}"
    );
}

#[test]
fn every_raw_proposal_status_event_post_prepare_failure_preserves_world() {
    for case in ["approved", "queued", "applied"] {
        let id = 100;
        let mut world = World::new();
        let status = if case == "approved" {
            ProposalStatus::Shadowed {
                manifest_hash: "shadow-hash".into(),
            }
        } else {
            ProposalStatus::Approved {
                manifest_hash: "approved-hash".into(),
                approver: "seed-approver".into(),
            }
        };
        world.proposals.insert(id, proposal(id, status));
        let event = match case {
            "approved" => approved_event(id, ProposalDecision::Approve),
            "queued" => queued_event(id, "approved-hash", 2, 5),
            "applied" => applied_event(id, None),
            _ => unreachable!(),
        };
        let before = world.clone();
        let root_before = world.current_state_root_hash().expect("state root before");
        world.fail_next_append_after_publication_prepare_for_test();

        let error = world
            .append_event(WorldEventBody::Governance(event), None)
            .expect_err("post-prepare fault must reject raw proposal status event");

        assert_injected(&error);
        assert_world_unchanged(&world, &before, &root_before);
    }
}

#[test]
fn missing_proposal_errors_are_exact_and_nonmutating() {
    for event in [
        approved_event(404, ProposalDecision::Approve),
        queued_event(404, "missing", 1, 2),
        applied_event(404, None),
    ] {
        let mut world = World::new();
        let before = world.clone();
        let root_before = world.current_state_root_hash().expect("state root before");
        let error = world
            .append_event(WorldEventBody::Governance(event), None)
            .expect_err("missing proposal must fail");
        assert!(matches!(
            error,
            WorldError::ProposalNotFound { proposal_id: 404 }
        ));
        assert_world_unchanged(&world, &before, &root_before);
    }
}

#[test]
fn approved_accept_requires_shadow_but_reject_clears_any_timeline() {
    let id = 101;
    let mut invalid = World::new();
    invalid
        .proposals
        .insert(id, proposal(id, ProposalStatus::Proposed));
    let before = invalid.clone();
    let root_before = invalid
        .current_state_root_hash()
        .expect("state root before");
    let error = invalid
        .append_event(
            WorldEventBody::Governance(approved_event(id, ProposalDecision::Approve)),
            None,
        )
        .expect_err("approve requires shadowed proposal");
    assert!(matches!(
        error,
        WorldError::ProposalInvalidState {
            proposal_id: 101,
            expected,
            found
        } if expected == "shadowed" && found == "proposed"
    ));
    assert_world_unchanged(&invalid, &before, &root_before);

    let mut rejecting = World::new();
    let mut seeded = proposal(
        id,
        ProposalStatus::Applied {
            manifest_hash: "old".into(),
        },
    );
    seeded.queued_at_tick = Some(3);
    seeded.not_before_tick = Some(8);
    seeded.activate_epoch = Some(9);
    seeded.timelock_ticks = 5;
    rejecting.proposals.insert(id, seeded);
    rejecting
        .append_event(
            WorldEventBody::Governance(approved_event(
                id,
                ProposalDecision::Reject {
                    reason: "late rejection".into(),
                },
            )),
            None,
        )
        .expect("reject remains valid from any proposal status");
    let rejected = &rejecting.proposals[&id];
    assert_eq!(
        rejected.status,
        ProposalStatus::Rejected {
            reason: "late rejection".into()
        }
    );
    assert_eq!(rejected.queued_at_tick, None);
    assert_eq!(rejected.not_before_tick, None);
    assert_eq!(rejected.activate_epoch, None);
    assert_eq!(rejected.timelock_ticks, 0);
}

#[test]
fn queued_error_priority_is_status_then_hash_then_timeline() {
    let id = 102;
    let cases = [
        (
            ProposalStatus::Shadowed {
                manifest_hash: "approved-hash".into(),
            },
            queued_event(id, "wrong-hash", 5, 4),
            "status",
        ),
        (
            ProposalStatus::Approved {
                manifest_hash: "approved-hash".into(),
                approver: "seed".into(),
            },
            queued_event(id, "wrong-hash", 5, 4),
            "hash",
        ),
        (
            ProposalStatus::Approved {
                manifest_hash: "approved-hash".into(),
                approver: "seed".into(),
            },
            queued_event(id, "approved-hash", 5, 4),
            "timeline",
        ),
    ];
    for (status, event, expected) in cases {
        let mut world = World::new();
        world.proposals.insert(id, proposal(id, status));
        let before = world.clone();
        let root_before = world.current_state_root_hash().expect("state root before");
        let error = world
            .append_event(WorldEventBody::Governance(event), None)
            .expect_err("queued validation must fail");
        match expected {
            "status" => assert!(matches!(
                error,
                WorldError::ProposalInvalidState { expected, found, .. }
                    if expected == "approved" && found == "shadowed"
            )),
            "hash" => assert!(matches!(
                error,
                WorldError::GovernancePolicyInvalid { reason }
                    if reason.contains("queued manifest hash drift")
            )),
            "timeline" => assert!(matches!(
                error,
                WorldError::GovernancePolicyInvalid { reason }
                    if reason.contains("invalid queued timeline")
            )),
            _ => unreachable!(),
        }
        assert_world_unchanged(&world, &before, &root_before);
    }
}

#[test]
fn applied_requires_approved_and_preserves_some_or_none_hash_semantics() {
    let mut world = World::new();
    world
        .proposals
        .insert(103, proposal(103, ProposalStatus::Proposed));
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");
    let error = world
        .append_event(WorldEventBody::Governance(applied_event(103, None)), None)
        .expect_err("applied requires approved proposal");
    assert!(matches!(
        error,
        WorldError::ProposalInvalidState { expected, found, .. }
            if expected == "approved" && found == "proposed"
    ));
    assert_world_unchanged(&world, &before, &root_before);

    for (id, supplied, expected) in [
        (104, Some("event-selected-hash"), "event-selected-hash"),
        (105, None, "approved-fallback-hash"),
    ] {
        world.proposals.insert(
            id,
            proposal(
                id,
                ProposalStatus::Approved {
                    manifest_hash: "approved-fallback-hash".into(),
                    approver: "seed".into(),
                },
            ),
        );
        world
            .append_event(
                WorldEventBody::Governance(applied_event(id, supplied)),
                None,
            )
            .expect("valid applied event");
        assert_eq!(
            world.proposals[&id].status,
            ProposalStatus::Applied {
                manifest_hash: expected.into()
            }
        );
    }
}

#[test]
fn failed_raw_approval_retry_then_sequence_replays_proposal_and_root_with_cause() {
    let id = 106;
    let mut world = World::new();
    world.proposals.insert(
        id,
        proposal(
            id,
            ProposalStatus::Shadowed {
                manifest_hash: "sequence-hash".into(),
            },
        ),
    );
    let baseline = world.snapshot();
    let cause = Some(CausedBy::Action(900));
    world.fail_next_append_after_publication_prepare_for_test();
    world
        .append_event(
            WorldEventBody::Governance(approved_event(id, ProposalDecision::Approve)),
            cause.clone(),
        )
        .expect_err("first raw approval fails at publication boundary");
    assert!(world.journal().events.is_empty());

    for event in [
        approved_event(id, ProposalDecision::Approve),
        queued_event(id, "sequence-hash", 4, 9),
        applied_event(id, Some("final-event-hash")),
    ] {
        world
            .append_event(WorldEventBody::Governance(event), cause.clone())
            .expect("retry sequence event");
    }
    assert_eq!(
        world.proposals[&id].status,
        ProposalStatus::Applied {
            manifest_hash: "final-event-hash".into()
        }
    );
    assert_eq!(world.proposals[&id].queued_at_tick, Some(4));
    assert_eq!(world.proposals[&id].not_before_tick, Some(9));
    assert!(
        world
            .journal()
            .events
            .iter()
            .all(|event| event.caused_by == cause)
    );
    let replay = World::from_snapshot(baseline, world.journal().clone()).expect("replay sequence");
    assert_eq!(replay.proposals, world.proposals);
    assert_eq!(
        replay.current_state_root_hash().expect("replay root"),
        world.current_state_root_hash().expect("live root")
    );
}
