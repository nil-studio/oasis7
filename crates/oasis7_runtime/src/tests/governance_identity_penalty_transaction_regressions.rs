use super::super::*;
use super::pos;

fn local_guardians() -> Vec<String> {
    vec![
        "governance.local.finality.signer.1".to_string(),
        "governance.local.finality.signer.2".to_string(),
    ]
}

fn applied_penalty_world(appeal_window_ticks: u64) -> (World, u64) {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "identity-penalty-agent".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register identity penalty agent");
    world
        .set_governance_identity_profile(
            "identity-penalty-agent",
            100,
            0,
            GovernanceIdentityStatus::Active,
        )
        .expect("set identity profile");
    let penalty_id = world
        .apply_identity_penalty(
            "identity-penalty-agent",
            "evidence.identity.penalty",
            "identity penalty test",
            20,
            appeal_window_ticks,
            "guardian-1",
            local_guardians(),
        )
        .expect("apply identity penalty");
    (world, penalty_id)
}

#[test]
fn identity_penalty_appeal_publication_failure_is_atomic_and_retry_replays() {
    let (mut world, penalty_id) = applied_penalty_world(10);
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let penalty_before = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .cloned()
        .expect("applied penalty");
    let root_before = world.current_state_root_hash().expect("initial state root");

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .appeal_identity_penalty(penalty_id, "identity-penalty-agent", "counter evidence")
        .expect_err("injected appeal publication failure must abort");
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id
    );
    assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);
    assert_eq!(
        world.governance_identity_penalties().get(&penalty_id),
        Some(&penalty_before)
    );
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());

    let replay_base = world.snapshot();
    world
        .appeal_identity_penalty(penalty_id, "identity-penalty-agent", "counter evidence")
        .expect("retry identity penalty appeal");
    let appealed = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .expect("appealed penalty");
    let expected_appeal_hash =
        util::sha256_hex(b"identity-stage-v1|appeal|identity-penalty-agent|counter evidence");
    let expected_chain_hash = util::sha256_hex(
        format!(
            "identity-chain-v1|{}|appeal|{}",
            penalty_before.evidence_chain_hash, expected_appeal_hash
        )
        .as_bytes(),
    );
    assert_eq!(appealed.status, GovernanceIdentityPenaltyStatus::Appealed);
    assert_eq!(
        appealed.appellant.as_deref(),
        Some("identity-penalty-agent")
    );
    assert_eq!(appealed.appeal_reason.as_deref(), Some("counter evidence"));
    assert_eq!(
        appealed.appeal_evidence_hash.as_deref(),
        Some(expected_appeal_hash.as_str())
    );
    assert_eq!(appealed.evidence_chain_hash, expected_chain_hash);
    assert_eq!(
        world.current_state_root_hash().expect("appeal root"),
        root_before
    );

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay identity penalty appeal");
    assert_eq!(
        restored.governance_identity_penalties(),
        world.governance_identity_penalties()
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
    let Some(WorldEvent {
        body:
            WorldEventBody::Governance(GovernanceEvent::IdentityPenaltyAppealed {
                appellant,
                reason,
                ..
            }),
        ..
    }) = world.journal().events.last()
    else {
        panic!("expected identity penalty appeal event");
    };
    assert_eq!(appellant, "identity-penalty-agent");
    assert_eq!(reason, "counter evidence");
}

#[test]
fn identity_penalty_appeal_backfills_legacy_chain_fields_and_replays() {
    let (world, penalty_id) = applied_penalty_world(10);
    let mut legacy_snapshot = world.snapshot();
    let legacy = legacy_snapshot
        .governance_identity_penalties
        .get_mut(&penalty_id)
        .expect("legacy penalty");
    legacy.detection_source.clear();
    legacy.detection_incident_id.clear();
    legacy.evidence_chain_hash.clear();
    let mut world = World::from_snapshot(legacy_snapshot, world.journal().clone())
        .expect("restore legacy identity penalty");
    let replay_base = world.snapshot();
    world
        .appeal_identity_penalty(penalty_id, "identity-penalty-agent", "legacy review")
        .expect("appeal legacy identity penalty");
    let appealed = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .expect("appealed legacy penalty");
    let incident_id =
        util::sha256_hex(b"identity-incident-v1|identity-penalty-agent|evidence.identity.penalty");
    let base_chain = util::sha256_hex(
        format!(
            "identity-chain-v1|{}|identity-penalty-agent|evidence.identity.penalty|identity penalty test|{}",
            penalty_id, incident_id
        )
        .as_bytes(),
    );
    let appeal_hash =
        util::sha256_hex(b"identity-stage-v1|appeal|identity-penalty-agent|legacy review");
    let expected_chain =
        util::sha256_hex(format!("identity-chain-v1|{base_chain}|appeal|{appeal_hash}").as_bytes());
    assert_eq!(appealed.detection_source, "world.threat_heatmap.v1");
    assert_eq!(appealed.detection_incident_id, incident_id);
    assert_eq!(appealed.evidence_chain_hash, expected_chain);
    assert_eq!(
        appealed.appeal_evidence_hash.as_deref(),
        Some(appeal_hash.as_str())
    );
    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay legacy identity penalty appeal");
    assert_eq!(
        restored.governance_identity_penalties(),
        world.governance_identity_penalties()
    );
}

#[test]
fn identity_penalty_appeal_rejects_expired_window_and_resolved_status_without_mutation() {
    let (mut expired, expired_id) = applied_penalty_world(1);
    for _ in 0..2 {
        expired.step().expect("advance beyond appeal deadline");
    }
    let expired_snapshot = expired.snapshot();
    let expired_journal = expired.journal().clone();
    let expired_penalty = expired.governance_identity_penalties().clone();
    assert!(matches!(
        expired
            .appeal_identity_penalty(expired_id, "identity-penalty-agent", "late review")
            .unwrap_err(),
        WorldError::GovernancePolicyInvalid { .. }
    ));
    assert_eq!(expired.snapshot(), expired_snapshot);
    assert_eq!(expired.journal(), &expired_journal);
    assert_eq!(expired.governance_identity_penalties(), &expired_penalty);

    let (mut resolved, resolved_id) = applied_penalty_world(10);
    resolved
        .appeal_identity_penalty(resolved_id, "identity-penalty-agent", "first review")
        .expect("initial appeal");
    let resolved_snapshot = resolved.snapshot();
    let resolved_journal = resolved.journal().clone();
    let resolved_penalty = resolved.governance_identity_penalties().clone();
    assert!(matches!(
        resolved
            .appeal_identity_penalty(resolved_id, "identity-penalty-agent", "duplicate review")
            .unwrap_err(),
        WorldError::GovernancePolicyInvalid { .. }
    ));
    assert_eq!(resolved.snapshot(), resolved_snapshot);
    assert_eq!(resolved.journal(), &resolved_journal);
    assert_eq!(resolved.governance_identity_penalties(), &resolved_penalty);
}
