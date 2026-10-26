use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::v2::{
    BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftScopeV2, BftValidatorV2, BftVotePhaseV2, BftVoteV2,
    BftVoteValueV2,
};

use super::*;
use crate::tendermint_v1::BftTimeoutsV1;
use crate::tendermint_v2::{
    BftHeightProposerScheduleV2, BftSchedulePolicyV2, BftValidatorSetV2,
    verify_commit_certificate_token_v2,
};

const TIMEOUTS: BftTimeoutsV1 = BftTimeoutsV1 {
    propose_ms: 100,
    prevote_ms: 100,
    precommit_ms: 100,
    round_delta_ms: 25,
};

#[test]
fn v2_machine_reaches_qc_and_applies_only_its_verified_schedule_successor() {
    let scope = BftScopeV2 {
        world_id: "machine-v2-world".into(),
        chain_id: "machine-v2-chain".into(),
        genesis_hash: "11".repeat(32),
    };
    let keys = (1..=4)
        .map(|seed| (format!("v{seed}"), SigningKey::from_bytes(&[seed; 32])))
        .collect::<Vec<_>>();
    let set = BftValidatorSetV2::new(
        scope.clone(),
        keys.iter()
            .map(|(validator_id, key)| BftValidatorV2 {
                validator_id: validator_id.clone(),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                voting_power: 1,
            })
            .collect(),
    )
    .unwrap();
    let schedule = BftHeightProposerScheduleV2::genesis(
        set.clone(),
        BftSchedulePolicyV2::new(8, "controlled-bft-v2").unwrap(),
        4,
        "22".repeat(32),
        "33".repeat(32),
    )
    .unwrap();
    let local_id = schedule.expected_proposer_for_round(0).unwrap();
    let local_key = &keys
        .iter()
        .find(|(validator_id, _)| validator_id == &local_id)
        .unwrap()
        .1;
    let mut machine =
        BftConsensusMachineV2::new(schedule.clone(), local_id.clone(), 100, TIMEOUTS).unwrap();
    let block = BftBlockV2 {
        protocol_version: BFT_PROTOCOL_VERSION_V2,
        scope: scope.clone(),
        height: 1,
        parent_commit_ref: schedule.parent_commit_ref().to_string(),
        authority_epoch: schedule.authority_epoch(),
        active_validator_set_hash: set.set_hash().to_string(),
        schedule_policy_hash: schedule.policy_hash().to_string(),
        manifest_hash: schedule.parent_manifest_hash().to_string(),
        post_manifest_hash: "44".repeat(32),
        actions_cbor: vec![0x80],
        action_root: "55".repeat(32),
        event_root: "66".repeat(32),
        execution_block_hash: "77".repeat(32),
        execution_state_root: "88".repeat(32),
        state_root: "99".repeat(32),
        transition: None,
        transition_ref: None,
    };

    let intent = machine.proposal_intent(block, &local_id).unwrap();
    let proposal = machine.sign_proposal_intent(&intent, local_key).unwrap();
    let proposal_id = proposal.header.block_id.clone();
    let actions = machine.receive_proposal(proposal, true, 101).unwrap();
    let prevote_intent = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV2::SignVote(intent) if intent.phase == BftVotePhaseV2::Prevote => {
                Some(intent)
            }
            _ => None,
        })
        .expect("valid proposal requests local prevote");
    let local_prevote = prevote_intent.sign(&set, local_key).unwrap();
    let actions = machine.record_local_vote(local_prevote, 102).unwrap();
    assert!(actions.is_empty());

    let block_value = BftVoteValueV2::Block {
        block_id: proposal_id,
    };
    let remote_votes = keys
        .iter()
        .filter(|(validator_id, _)| validator_id != &local_id)
        .map(|(validator_id, key)| {
            BftVoteV2::sign(
                scope.clone(),
                1,
                0,
                BftVotePhaseV2::Prevote,
                set.set_hash().to_string(),
                schedule.policy_hash().to_string(),
                block_value.clone(),
                validator_id.clone(),
                key,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let mut precommit_intent = None;
    for (index, vote) in remote_votes.into_iter().enumerate() {
        for action in machine.receive_vote(vote, 103 + index as i64).unwrap() {
            if let BftConsensusActionV2::SignVote(intent) = action
                && intent.phase == BftVotePhaseV2::Precommit
            {
                precommit_intent = Some(intent);
            }
        }
    }
    let precommit_intent = precommit_intent.expect("prevote quorum locks and requests precommit");
    let local_precommit = precommit_intent.sign(&set, local_key).unwrap();
    machine
        .record_local_vote(local_precommit, 110)
        .expect("persisted local precommit is accepted");

    let remote_precommits = keys
        .iter()
        .filter(|(validator_id, _)| validator_id != &local_id)
        .map(|(validator_id, key)| {
            BftVoteV2::sign(
                scope.clone(),
                1,
                0,
                BftVotePhaseV2::Precommit,
                set.set_hash().to_string(),
                schedule.policy_hash().to_string(),
                block_value.clone(),
                validator_id.clone(),
                key,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let mut certificate = None;
    for (index, vote) in remote_precommits.into_iter().enumerate() {
        for action in machine.receive_vote(vote, 111 + index as i64).unwrap() {
            if let BftConsensusActionV2::Commit(value) = action {
                certificate = Some(value);
            }
        }
    }
    let certificate = certificate.expect("precommit quorum emits full QC");
    let token = verify_commit_certificate_token_v2(&schedule, &certificate).unwrap();
    machine
        .mark_commit_applied(&token, 120, TIMEOUTS)
        .expect("verified token advances machine");
    assert_eq!(machine.height(), 2);
    assert_eq!(machine.schedule(), token.next_height_schedule());
    let expected_proposer = token
        .next_height_schedule()
        .expected_proposer_for_round(0)
        .unwrap();
    assert_eq!(
        machine.current_proposer_id(),
        Some(expected_proposer.as_str())
    );
}
