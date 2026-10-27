//! Actual World execution from a separately qualified wire 3 H, not synthetic roots.
use super::super::super::formal_header::{FormalBftHeaderCandidateV3, FormalBftHeaderExecutionV3};
use super::*;

const EXECUTION_CANARY: &str = "private-wire3-world-agent-4975";

#[test]
fn wire3_formal_read_actual_world_executes_nonempty_actions_and_chains_header_qcs() {
    on_execution_stack(actual_execution_case);
}

#[test]
fn wire3_formal_read_signed_execution_lie_and_private_input_tampering_are_rejected() {
    on_execution_stack(execution_tamper_case);
}

fn on_execution_stack(case: fn()) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(case)
        .unwrap()
        .join()
        .unwrap();
}

fn execution() -> FormalBftHeaderExecutionV3 {
    let h = qualified_handoff_v3();
    FormalBftHeaderExecutionV3::from_handoff(
        h.anchor,
        h.target,
        &h.material,
        h.reader,
        Vec::new(),
        false,
    )
    .expect("restore actual qualified wire 3 H and bind its World")
}

pub(super) fn private_actions() -> Vec<u8> {
    let payload = oasis7::consensus_action_payload::encode_consensus_action_payload(
        &oasis7::consensus_action_payload::ConsensusActionPayloadEnvelope::from_runtime_action(
            oasis7_runtime::Action::RegisterAgent {
                agent_id: EXECUTION_CANARY.into(),
                pos: oasis7::geometry::GeoPos::new(7, 8, 9),
            },
        ),
    )
    .unwrap();
    serde_cbor::to_vec(&vec![
        oasis7_node::NodeConsensusAction::from_payload(1, "wire3-world-private", payload).unwrap(),
    ])
    .unwrap()
}

pub(super) fn certify(
    schedule: &BftHeaderHeightScheduleV3,
    candidate: &FormalBftHeaderCandidateV3,
) -> FormalBftCertifiedHeaderV3 {
    let block_id = candidate.header.block_id().unwrap();
    let proposer = schedule.expected_proposer_for_round(0).unwrap();
    let proposal = BftProposalHeaderV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        schedule.active_set().set_hash().to_owned(),
        schedule.policy_hash().to_owned(),
        proposer.clone(),
        block_id.clone(),
        None,
        &key_for_id(&proposer),
    )
    .unwrap();
    let precommits = [0, 1, 2]
        .into_iter()
        .map(|index| {
            let id = validator_id(index);
            signed_vote(
                schedule,
                &candidate.commitment,
                &block_id,
                &id,
                0,
                BftVotePhaseV3::Precommit,
                &key_for_id(&id),
            )
        })
        .collect();
    FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence: None,
            precommits,
        },
        candidate.header.clone(),
        candidate.commitment.clone(),
    )
    .unwrap()
}

fn actual_execution_case() {
    let mut execution = execution();
    let bytes = private_actions();
    let decoded: Vec<oasis7_node::NodeConsensusAction> = serde_cbor::from_slice(&bytes).unwrap();
    assert!(
        decoded[0]
            .payload_cbor
            .windows(EXECUTION_CANARY.len())
            .any(|x| x == EXECUTION_CANARY.as_bytes()),
        "the actual private action payload contains the canary",
    );
    let h_tick = execution.parent_world().state().time;
    let before = serde_cbor::to_vec(&execution.parent_world().snapshot()).unwrap();
    let candidate = execution
        .build_candidate(&bytes)
        .expect("execute actual private action");
    assert_eq!(
        serde_cbor::to_vec(&execution.parent_world().snapshot()).unwrap(),
        before,
        "preview leaves the qualified parent byte-identical"
    );
    let entry = certify(execution.schedule(), &candidate);
    let public = entry.canonical_bytes().unwrap();
    // Vec<u8> is encoded as an integer sequence here. Search the encoded input
    // too; an ASCII-only canary check would miss a leaked raw action vector.
    let encoded_private_input = serde_cbor::to_vec(&bytes).unwrap();
    assert!(
        !public
            .windows(encoded_private_input.len())
            .any(|part| part == encoded_private_input.as_slice())
    );
    let public_json = serde_json::to_string(&entry).unwrap();
    assert!(!public_json.contains("\"actions_cbor\":"));
    assert!(!public_json.contains("\"payload_cbor\":"));
    assert!(
        !public
            .windows(EXECUTION_CANARY.len())
            .any(|x| x == EXECUTION_CANARY.as_bytes())
    );
    let first_id = entry.header.block_id().unwrap();
    let result = execution
        .replay_next(&entry, &bytes)
        .expect("verify QC and actual World transition");
    assert!(result.world.state().agents.contains_key(EXECUTION_CANARY));
    assert_eq!(result.world.state().time, h_tick + 1);
    assert_eq!(
        blake3_hex(&oasis7_world_state::codec::snapshot_cbor(&result.world.snapshot()).unwrap()),
        candidate.commitment.execution_state_root
    );
    assert_eq!(execution.schedule().parent_commit_ref(), first_id);
    for offset in 2..=3 {
        let candidate = execution.build_candidate(EMPTY_ACTIONS_CBOR).unwrap();
        let entry = certify(execution.schedule(), &candidate);
        let result = execution.replay_next(&entry, EMPTY_ACTIONS_CBOR).unwrap();
        assert_eq!(result.world.state().time, h_tick + offset);
        assert!(result.world.state().agents.contains_key(EXECUTION_CANARY));
        assert_eq!(
            execution.schedule().parent_commit_ref(),
            entry.header.block_id().unwrap()
        );
    }
}

fn execution_tamper_case() {
    let execution = execution();
    let bytes = private_actions();
    let candidate = execution.build_candidate(&bytes).unwrap();
    let entry = certify(execution.schedule(), &candidate);
    let before = serde_cbor::to_vec(&execution.parent_world().snapshot()).unwrap();
    let mut changed = bytes.clone();
    *changed.last_mut().unwrap() ^= 1;
    assert!(execution.verify_and_execute(&entry, &changed).is_err());
    // A real quorum authenticates this self-consistent public lie. Only actual
    // deterministic execution rejects it; signature rejection is not the oracle.
    let mut lie = candidate.clone();
    lie.commitment.state_root = hash("authenticated-wire3-execution-lie");
    lie.header.execution_commitment_hash = lie.commitment.digest().unwrap();
    let lie = certify(execution.schedule(), &lie);
    verify_formal_bft_header_certificate_v3(execution.schedule(), &lie)
        .expect("syntactically and cryptographically valid QC");
    let error = execution
        .verify_and_execute(&lie, &bytes)
        .err()
        .expect("actual World must disagree");
    assert!(error.contains("actual World execution"), "{error}");
    assert_eq!(
        serde_cbor::to_vec(&execution.parent_world().snapshot()).unwrap(),
        before
    );
    assert!(
        !execution
            .parent_world()
            .state()
            .agents
            .contains_key(EXECUTION_CANARY)
    );
}
