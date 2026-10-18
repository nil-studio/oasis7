use super::super::*;
use super::pos;

const PROFILED_AGENT: &str = "identity-application-agent";
const UNPROFILED_AGENT: &str = "identity-application-unprofiled";

fn local_guardians() -> Vec<String> {
    vec![
        "governance.local.finality.signer.1".to_string(),
        "governance.local.finality.signer.2".to_string(),
    ]
}

fn registered_world(agent_id: &str, with_profile: bool) -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register identity application agent");
    if with_profile {
        world
            .set_governance_identity_profile(agent_id, 100, 0, GovernanceIdentityStatus::Active)
            .expect("set identity profile");
    }
    world
}

fn apply_args(agent_id: &str, slash_stake: u64) -> (&str, &str, &str, u64) {
    (
        agent_id,
        "evidence.identity.application",
        "application test",
        slash_stake,
    )
}

#[test]
fn identity_penalty_application_failure_is_atomic_and_retries_same_id() {
    let mut world = registered_world(PROFILED_AGENT, true);
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let profile_before = world
        .governance_identity_profile(PROFILED_AGENT)
        .cloned()
        .expect("profile before application");
    let root_before = world.current_state_root_hash().expect("initial state root");
    let expected_id = snapshot_before.next_governance_identity_penalty_id;

    world.fail_next_append_after_publication_prepare_for_test();
    let (target, evidence, reason, slash) = apply_args(PROFILED_AGENT, 30);
    let error = world
        .apply_identity_penalty(
            target,
            evidence,
            reason,
            slash,
            10,
            "guardian-1",
            local_guardians(),
        )
        .expect_err("injected application publication failure must abort");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { ref reason }
            if reason.contains("publication preparation")
    ));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(
        world.snapshot().next_governance_identity_penalty_id,
        expected_id
    );
    assert_eq!(
        world.governance_identity_profile(PROFILED_AGENT),
        Some(&profile_before)
    );
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.current_state_root_hash().expect("failed root"),
        root_before
    );

    let replay_base = world.snapshot();
    let penalty_id = world
        .apply_identity_penalty(
            target,
            evidence,
            reason,
            slash,
            10,
            "guardian-1",
            local_guardians(),
        )
        .expect("retry identity penalty application");
    assert_eq!(penalty_id, expected_id);
    let record = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .expect("applied identity penalty");
    assert_eq!(record.target_agent_id, PROFILED_AGENT);
    assert_eq!(record.evidence_hash, evidence);
    assert_eq!(record.reason, reason);
    assert_eq!(record.slash_stake, slash);
    assert_eq!(record.status, GovernanceIdentityPenaltyStatus::Applied);
    assert_eq!(record.detection_source, "world.threat_heatmap.v1");
    let incident_id =
        util::sha256_hex(format!("identity-incident-v1|{PROFILED_AGENT}|{evidence}").as_bytes());
    assert_eq!(record.detection_incident_id, incident_id);
    assert_eq!(
        record.evidence_chain_hash,
        util::sha256_hex(
            format!(
                "identity-chain-v1|{penalty_id}|{PROFILED_AGENT}|{evidence}|{reason}|{incident_id}"
            )
            .as_bytes(),
        )
    );
    let profile = world
        .governance_identity_profile(PROFILED_AGENT)
        .expect("profile after application");
    assert_eq!(profile.stake_locked, 70);
    assert_eq!(profile.status, GovernanceIdentityStatus::Frozen);
    assert_eq!(profile.slash_count, 1);
    let resolved_root = world.current_state_root_hash().expect("applied root");
    assert_ne!(resolved_root, root_before);
    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay identity penalty application");
    assert_eq!(
        restored.governance_identity_penalties(),
        world.governance_identity_penalties()
    );
    assert_eq!(
        restored.governance_identity_profile(PROFILED_AGENT),
        world.governance_identity_profile(PROFILED_AGENT)
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
fn identity_penalty_application_inserts_missing_profile_and_replays_root() {
    let mut world = registered_world(UNPROFILED_AGENT, false);
    assert!(
        world
            .governance_identity_profile(UNPROFILED_AGENT)
            .is_none()
    );
    let snapshot_before = world.snapshot();
    let root_before = world.current_state_root_hash().expect("initial root");
    let application_tick = snapshot_before.state.time;

    let (target, evidence, reason, slash) = apply_args(UNPROFILED_AGENT, 0);
    let penalty_id = world
        .apply_identity_penalty(
            target,
            evidence,
            reason,
            slash,
            10,
            "guardian-1",
            local_guardians(),
        )
        .expect("apply penalty with default profile");
    assert_eq!(
        penalty_id,
        snapshot_before.next_governance_identity_penalty_id
    );
    let profile = world
        .governance_identity_profile(UNPROFILED_AGENT)
        .expect("default profile inserted");
    assert_eq!(profile.agent_id, UNPROFILED_AGENT);
    assert_eq!(profile.stake_locked, 0);
    assert_eq!(profile.status, GovernanceIdentityStatus::Frozen);
    assert_eq!(profile.slash_count, 1);
    assert_eq!(profile.updated_at, application_tick);
    let applied_root = world
        .current_state_root_hash()
        .expect("inserted profile root");
    assert_ne!(applied_root, root_before);
    let latest_consensus = world
        .tick_consensus_records()
        .last()
        .expect("application consensus");
    assert_eq!(latest_consensus.block.header.state_root, applied_root);

    let restored = World::from_snapshot(snapshot_before, world.journal().clone())
        .expect("replay default profile insertion");
    assert_eq!(
        restored.governance_identity_penalties(),
        world.governance_identity_penalties()
    );
    assert_eq!(
        restored.governance_identity_profile(UNPROFILED_AGENT),
        world.governance_identity_profile(UNPROFILED_AGENT)
    );
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(
        restored
            .current_state_root_hash()
            .expect("restored inserted root"),
        applied_root
    );
}

#[test]
fn identity_penalty_application_rejects_invalid_inputs_without_mutation() {
    let mut world = registered_world(PROFILED_AGENT, true);
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let invalid_signer = world
        .apply_identity_penalty(
            PROFILED_AGENT,
            "evidence.identity.invalid",
            "invalid signer",
            10,
            10,
            "guardian-1",
            vec!["governance.unknown.signer".to_string()],
        )
        .expect_err("unknown guardian must be rejected");
    assert!(matches!(
        invalid_signer,
        WorldError::GovernancePolicyInvalid { .. }
    ));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let insufficient_stake = world
        .apply_identity_penalty(
            PROFILED_AGENT,
            "evidence.identity.insufficient",
            "insufficient stake",
            101,
            10,
            "guardian-1",
            local_guardians(),
        )
        .expect_err("slash above locked stake must be rejected");
    assert!(matches!(
        insufficient_stake,
        WorldError::GovernancePolicyInvalid { .. }
    ));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
}
