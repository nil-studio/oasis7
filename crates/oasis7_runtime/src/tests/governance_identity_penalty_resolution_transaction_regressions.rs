use super::super::*;
use super::pos;

fn local_guardians() -> Vec<String> {
    vec![
        "governance.local.finality.signer.1".to_string(),
        "governance.local.finality.signer.2".to_string(),
    ]
}

fn appealed_penalty_world() -> (World, u64) {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "identity-resolution-agent".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register identity resolution agent");
    world
        .set_governance_identity_profile(
            "identity-resolution-agent",
            100,
            0,
            GovernanceIdentityStatus::Active,
        )
        .expect("set identity profile");
    let penalty_id = world
        .apply_identity_penalty(
            "identity-resolution-agent",
            "evidence.identity.resolution",
            "identity resolution test",
            20,
            10,
            "guardian-1",
            local_guardians(),
        )
        .expect("apply identity penalty");
    world
        .appeal_identity_penalty(penalty_id, "identity-resolution-agent", "review request")
        .expect("appeal identity penalty");
    world.step().expect("advance resolution time");
    (world, penalty_id)
}

fn expected_hash(stage: &str, actor: &str, reason: &str) -> String {
    util::sha256_hex(format!("identity-stage-v1|{stage}|{actor}|{reason}").as_bytes())
}

fn expected_chain(base: &str, stage_hash: &str) -> String {
    util::sha256_hex(format!("identity-chain-v1|{base}|resolve|{stage_hash}").as_bytes())
}

#[test]
fn identity_penalty_accept_resolution_failure_is_atomic_and_retry_replays() {
    let (mut world, penalty_id) = appealed_penalty_world();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let penalty_before = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .cloned()
        .expect("appealed penalty");
    let profile_before = world
        .governance_identity_profile("identity-resolution-agent")
        .cloned()
        .expect("frozen identity profile");
    let root_before = world.current_state_root_hash().expect("initial state root");

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .resolve_identity_penalty_appeal(penalty_id, "committee-1", true, "appeal accepted")
        .expect_err("injected accepted resolution failure must abort");
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.governance_identity_penalties().get(&penalty_id),
        Some(&penalty_before)
    );
    assert_eq!(
        world.governance_identity_profile("identity-resolution-agent"),
        Some(&profile_before)
    );
    assert_eq!(
        world.current_state_root_hash().expect("failed root"),
        root_before
    );

    let replay_base = world.snapshot();
    world
        .resolve_identity_penalty_appeal(penalty_id, "committee-1", true, "appeal accepted")
        .expect("retry accepted resolution");
    let resolved = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .expect("accepted resolution");
    let resolution_hash = expected_hash("resolve_accept", "committee-1", "appeal accepted");
    assert_eq!(
        resolved.status,
        GovernanceIdentityPenaltyStatus::AppealAccepted
    );
    assert_eq!(resolved.resolved_by.as_deref(), Some("committee-1"));
    assert_eq!(
        resolved.resolution_reason.as_deref(),
        Some("appeal accepted")
    );
    assert_eq!(
        resolved.resolution_evidence_hash.as_deref(),
        Some(resolution_hash.as_str())
    );
    assert_eq!(
        resolved.evidence_chain_hash,
        expected_chain(
            penalty_before.evidence_chain_hash.as_str(),
            resolution_hash.as_str()
        )
    );
    let profile = world
        .governance_identity_profile("identity-resolution-agent")
        .expect("restored identity profile");
    assert_eq!(profile.status, GovernanceIdentityStatus::Active);
    assert_eq!(profile.stake_locked, 100);
    let resolved_root = world.current_state_root_hash().expect("resolved root");
    assert_ne!(resolved_root, root_before);
    let Some(WorldEvent {
        body:
            WorldEventBody::Governance(GovernanceEvent::IdentityPenaltyResolved {
                resolver,
                accepted,
                reason,
                ..
            }),
        ..
    }) = world.journal().events.last()
    else {
        panic!("expected identity penalty resolution event");
    };
    assert_eq!(resolver, "committee-1");
    assert!(*accepted);
    assert_eq!(reason, "appeal accepted");

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay accepted resolution");
    assert_eq!(
        restored.governance_identity_penalties(),
        world.governance_identity_penalties()
    );
    assert_eq!(
        restored.governance_identity_profile("identity-resolution-agent"),
        world.governance_identity_profile("identity-resolution-agent")
    );
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
fn identity_penalty_reject_resolution_failure_is_atomic_and_retry_replays() {
    let (mut world, penalty_id) = appealed_penalty_world();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let penalty_before = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .cloned()
        .expect("appealed penalty");
    let profile_before = world
        .governance_identity_profile("identity-resolution-agent")
        .cloned()
        .expect("frozen identity profile");
    let root_before = world.current_state_root_hash().expect("initial state root");

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .resolve_identity_penalty_appeal(penalty_id, "committee-1", false, "appeal rejected")
        .expect_err("injected rejected resolution failure must abort");
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.governance_identity_penalties().get(&penalty_id),
        Some(&penalty_before)
    );
    assert_eq!(
        world.governance_identity_profile("identity-resolution-agent"),
        Some(&profile_before)
    );

    let replay_base = world.snapshot();
    world
        .resolve_identity_penalty_appeal(penalty_id, "committee-1", false, "appeal rejected")
        .expect("retry rejected resolution");
    let resolved = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .expect("rejected resolution");
    let resolution_hash = expected_hash("resolve_reject", "committee-1", "appeal rejected");
    assert_eq!(
        resolved.status,
        GovernanceIdentityPenaltyStatus::AppealRejected
    );
    assert_eq!(
        resolved.resolution_evidence_hash.as_deref(),
        Some(resolution_hash.as_str())
    );
    assert_eq!(
        resolved.evidence_chain_hash,
        expected_chain(
            penalty_before.evidence_chain_hash.as_str(),
            resolution_hash.as_str()
        )
    );
    assert_eq!(resolved.resolved_at_tick, Some(2));
    let mut expected_profile = profile_before.clone();
    expected_profile.updated_at = 2;
    assert_eq!(
        world.governance_identity_profile("identity-resolution-agent"),
        Some(&expected_profile)
    );
    let resolved_root = world.current_state_root_hash().expect("resolved root");
    assert_ne!(resolved_root, root_before);
    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay rejected resolution");
    assert_eq!(
        restored.governance_identity_penalties(),
        world.governance_identity_penalties()
    );
    assert_eq!(
        restored.governance_identity_profile("identity-resolution-agent"),
        world.governance_identity_profile("identity-resolution-agent")
    );
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(
        restored.current_state_root_hash().expect("restored root"),
        resolved_root
    );
}

#[test]
fn identity_penalty_resolution_missing_profile_rejects_without_mutation() {
    let (world, penalty_id) = appealed_penalty_world();
    let mut snapshot = world.snapshot();
    snapshot
        .state
        .governance_identity_profiles
        .remove("identity-resolution-agent");
    // The retained consensus record commits the pre-mutation canonical root;
    // remove the unarchived test records and their count together so this
    // deliberately missing-profile snapshot can reach the reducer.
    assert_eq!(snapshot.tick_consensus_archived_record_count, 0);
    snapshot.tick_consensus_records.clear();
    snapshot.tick_consensus_total_record_count = 0;
    let mut world = World::from_snapshot(snapshot, world.journal().clone())
        .expect("restore malformed identity penalty state");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let penalty_before = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .cloned()
        .expect("appealed penalty");
    let err = world
        .resolve_identity_penalty_appeal(penalty_id, "committee-1", true, "appeal accepted")
        .expect_err("missing profile must reject before publication");
    assert!(matches!(err, WorldError::AgentNotFound { .. }));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(
        world.governance_identity_penalties().get(&penalty_id),
        Some(&penalty_before)
    );
}
