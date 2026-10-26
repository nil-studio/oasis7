use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::v2::{
    BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftProposalHeaderV2, BftProposalV2, BftScopeV2,
    BftValidRoundEvidenceV2, BftValidatorV2, BftVotePhaseV2, BftVoteV2, BftVoteValueV2,
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

#[test]
fn alternate_valid_round_proof_for_same_signed_header_is_not_equivocation() {
    let scope = BftScopeV2 {
        world_id: "proposal-evidence-world".into(),
        chain_id: "proposal-evidence-chain".into(),
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
    let initial = signed_proposal_with_round_evidence(&schedule, &keys, 1, 0, 1, &[0, 1, 2]);
    let same_header_other_proof =
        signed_proposal_with_round_evidence(&schedule, &keys, 1, 0, 1, &[0, 1, 3]);
    assert_eq!(initial.header, same_header_other_proof.header);
    assert_ne!(
        initial.valid_round_evidence,
        same_header_other_proof.valid_round_evidence
    );

    let mut machine = BftConsensusMachineV2::new(schedule.clone(), "v1", 100, TIMEOUTS).unwrap();
    machine.receive_proposal(initial, true, 101).unwrap();
    machine
        .receive_proposal(same_header_other_proof.clone(), true, 102)
        .expect("alternate valid proof for the same signed header is a duplicate");
    assert_eq!(machine.proposal_equivocation_evidence().count(), 0);

    let conflicting = signed_proposal_with_round_evidence(&schedule, &keys, 1, 0, 2, &[0, 1, 2]);
    assert_ne!(
        machine.proposal_for_round(1).unwrap().header,
        conflicting.header
    );
    assert!(
        machine
            .receive_proposal(conflicting.clone(), true, 103)
            .is_err()
    );
    let evidence = machine.proposal_equivocation_evidence().next().unwrap();
    assert_eq!(evidence.0, &same_header_other_proof.header);
    assert_eq!(evidence.1, &conflicting.header);
}

#[test]
fn conflicting_signed_header_is_retained_when_its_block_fails_schedule_validation() {
    let (scope, keys, set, schedule) = machine_fixture("invalid-equivocation-block");
    let proposer_id = schedule.expected_proposer_for_round(0).unwrap();
    let mut machine =
        BftConsensusMachineV2::new(schedule.clone(), proposer_id, 100, TIMEOUTS).unwrap();
    let accepted = plain_proposal(
        &scope,
        &set,
        &schedule,
        &keys,
        0,
        fixture_block(&scope, &set, &schedule, 1),
    );
    machine
        .receive_proposal(accepted.clone(), true, 101)
        .unwrap();

    let mut invalid_block = fixture_block(&scope, &set, &schedule, 2);
    invalid_block.parent_commit_ref = "ff".repeat(32);
    let conflicting = plain_proposal(&scope, &set, &schedule, &keys, 0, invalid_block);
    assert_ne!(accepted.header, conflicting.header);
    assert!(
        machine
            .receive_proposal(conflicting.clone(), true, 102)
            .is_err()
    );

    assert_eq!(machine.proposal_for_round(0), Some(&accepted));
    assert_eq!(
        machine.proposal_equivocation_evidence().next(),
        Some((&accepted.header, &conflicting.header))
    );
}

#[test]
fn accepted_future_proposal_is_promoted_when_its_round_becomes_current() {
    let (scope, keys, set, schedule) = machine_fixture("future-proposal-promotion");
    let local_id = schedule.expected_proposer_for_round(0).unwrap();
    let mut machine =
        BftConsensusMachineV2::new(schedule.clone(), local_id, 100, TIMEOUTS).unwrap();
    let proposal = plain_proposal(
        &scope,
        &set,
        &schedule,
        &keys,
        1,
        fixture_block(&scope, &set, &schedule, 1),
    );
    let proposal_id = proposal.header.block_id.clone();

    machine
        .receive_proposal(proposal.clone(), true, 101)
        .expect("a valid future-round proposal can be cached before advancement");
    assert_eq!(machine.proposal_for_round(1), Some(&proposal));

    let mut advancement_actions = Vec::new();
    for (validator_id, key) in keys.iter().take(3) {
        advancement_actions.extend(
            machine
                .receive_vote(
                    fixture_vote(
                        &schedule,
                        validator_id,
                        key,
                        1,
                        BftVotePhaseV2::Prevote,
                        BftVoteValueV2::Nil,
                    ),
                    102,
                )
                .expect("a future-round quorum advances the machine"),
        );
    }

    assert_eq!(machine.round(), 1);
    assert_eq!(
        machine.proposal_for_round(1),
        Some(&proposal),
        "promotion reinstalls the accepted proposal body for the current round"
    );
    assert!(
        advancement_actions.iter().any(|action| matches!(
            action,
            BftConsensusActionV2::SignVote(intent)
                if intent.phase == BftVotePhaseV2::Prevote
                    && intent.value
                        == (BftVoteValueV2::Block {
                            block_id: proposal_id.clone(),
                        })
        )),
        "promotion must process the cached proposal and request its local prevote"
    );
}

fn signed_proposal_with_round_evidence(
    schedule: &BftHeightProposerScheduleV2,
    keys: &[(String, SigningKey)],
    proposal_round: u64,
    valid_round: u64,
    marker: u8,
    evidence_signers: &[usize],
) -> BftProposalV2 {
    let block = BftBlockV2 {
        protocol_version: BFT_PROTOCOL_VERSION_V2,
        scope: schedule.scope().clone(),
        height: 1,
        parent_commit_ref: schedule.parent_commit_ref().to_string(),
        authority_epoch: schedule.authority_epoch(),
        active_validator_set_hash: schedule.active_set().set_hash().to_string(),
        schedule_policy_hash: schedule.policy_hash().to_string(),
        manifest_hash: schedule.parent_manifest_hash().to_string(),
        post_manifest_hash: "44".repeat(32),
        actions_cbor: vec![0x80, marker],
        action_root: format!("{marker:02x}").repeat(32),
        event_root: "66".repeat(32),
        execution_block_hash: "77".repeat(32),
        execution_state_root: "88".repeat(32),
        state_root: "99".repeat(32),
        transition: None,
        transition_ref: None,
    };
    let block_id = block.block_id().unwrap();
    let proposer_key = |round| {
        let proposer = schedule.expected_proposer_for_round(round).unwrap();
        let key = keys
            .iter()
            .find(|(validator_id, _)| validator_id == &proposer)
            .unwrap()
            .1
            .clone();
        (proposer, key)
    };
    let (valid_proposer, valid_key) = proposer_key(valid_round);
    let valid_round_header = BftProposalHeaderV2::sign(
        schedule.scope().clone(),
        1,
        valid_round,
        schedule.active_set().set_hash().to_string(),
        schedule.policy_hash().to_string(),
        valid_proposer,
        block_id.clone(),
        None,
        &valid_key,
    )
    .unwrap();
    let prevotes = evidence_signers
        .iter()
        .map(|index| {
            let (validator_id, key) = &keys[*index];
            BftVoteV2::sign(
                schedule.scope().clone(),
                1,
                valid_round,
                BftVotePhaseV2::Prevote,
                schedule.active_set().set_hash().to_string(),
                schedule.policy_hash().to_string(),
                BftVoteValueV2::Block {
                    block_id: block_id.clone(),
                },
                validator_id.clone(),
                key,
            )
            .unwrap()
        })
        .collect();
    let (proposer, proposer_signing_key) = proposer_key(proposal_round);
    let header = BftProposalHeaderV2::sign(
        schedule.scope().clone(),
        1,
        proposal_round,
        schedule.active_set().set_hash().to_string(),
        schedule.policy_hash().to_string(),
        proposer,
        block_id,
        Some(valid_round),
        &proposer_signing_key,
    )
    .unwrap();
    BftProposalV2 {
        header,
        block,
        valid_round_evidence: Some(BftValidRoundEvidenceV2 {
            proposal: valid_round_header,
            prevotes,
        }),
    }
}

#[test]
fn a_locked_value_only_yields_to_a_valid_round_at_or_after_the_lock() {
    let (scope, keys, set, schedule) = machine_fixture("lock-boundary");
    let local_id = schedule.expected_proposer_for_round(0).unwrap();
    let local_key = fixture_key(&keys, &local_id);
    let mut machine =
        BftConsensusMachineV2::new(schedule.clone(), local_id.clone(), 100, TIMEOUTS).unwrap();

    // Time out the empty proposal step, then collect a quorum of distinct
    // prevote and precommit values to arm Algorithm 1's phase timers.
    machine.on_timeout(200, &local_id).unwrap();
    for (index, value) in [
        BftVoteValueV2::Nil,
        BftVoteValueV2::Block {
            block_id: "aa".repeat(32),
        },
        BftVoteValueV2::Block {
            block_id: "bb".repeat(32),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let (validator_id, key) = &keys[index];
        machine
            .receive_vote(
                fixture_vote(
                    &schedule,
                    validator_id,
                    key,
                    0,
                    BftVotePhaseV2::Prevote,
                    value,
                ),
                201 + index as i64,
            )
            .unwrap();
    }
    let prevote_deadline = machine.step_deadline_ms().unwrap();
    machine.on_timeout(prevote_deadline, &local_id).unwrap();
    for (index, value) in [
        BftVoteValueV2::Nil,
        BftVoteValueV2::Block {
            block_id: "cc".repeat(32),
        },
        BftVoteValueV2::Block {
            block_id: "dd".repeat(32),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let (validator_id, key) = &keys[index];
        machine
            .receive_vote(
                fixture_vote(
                    &schedule,
                    validator_id,
                    key,
                    0,
                    BftVotePhaseV2::Precommit,
                    value,
                ),
                310 + index as i64,
            )
            .unwrap();
    }
    let precommit_deadline = machine.step_deadline_ms().unwrap();
    machine.on_timeout(precommit_deadline, &local_id).unwrap();
    assert_eq!(machine.round(), 1);

    let round_one_now = machine.step_deadline_ms().unwrap() - TIMEOUTS.propose_ms as i64;
    let block_a = fixture_block(&scope, &set, &schedule, 10);
    let proposal_a = plain_proposal(&scope, &set, &schedule, &keys, 1, block_a);
    let block_a_id = proposal_a.header.block_id.clone();
    let actions = machine
        .receive_proposal(proposal_a, true, round_one_now + 1)
        .unwrap();
    let local_prevote = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV2::SignVote(intent) if intent.phase == BftVotePhaseV2::Prevote => {
                Some(intent.sign(&set, local_key).unwrap())
            }
            _ => None,
        })
        .expect("round-one proposal should request a local prevote");
    machine
        .record_local_vote(local_prevote, round_one_now + 2)
        .unwrap();
    for (validator_id, key) in &keys {
        if validator_id == &local_id {
            continue;
        }
        machine
            .receive_vote(
                fixture_vote(
                    &schedule,
                    validator_id,
                    key,
                    1,
                    BftVotePhaseV2::Prevote,
                    BftVoteValueV2::Block {
                        block_id: block_a_id.clone(),
                    },
                ),
                round_one_now + 3,
            )
            .unwrap();
    }
    assert_eq!(machine.locked(), Some((block_a_id.as_str(), 1)));

    // Equality is safe: a validRound equal to the lock round permits a
    // different value when that round itself has a valid quorum proof.
    let mut equal_round_machine = machine.clone();
    for (index, (validator_id, key)) in keys.iter().enumerate() {
        if validator_id == &local_id {
            continue;
        }
        equal_round_machine
            .receive_vote(
                fixture_vote(
                    &schedule,
                    validator_id,
                    key,
                    2,
                    BftVotePhaseV2::Prevote,
                    BftVoteValueV2::Nil,
                ),
                500 + index as i64,
            )
            .unwrap();
        if equal_round_machine.round() == 2 {
            break;
        }
    }
    assert_eq!(equal_round_machine.round(), 2);
    let equal_round_proposal =
        signed_proposal_with_round_evidence(&schedule, &keys, 2, 1, 12, &[0, 1, 2]);
    let equal_round_block_id = equal_round_proposal.header.block_id.clone();
    let actions = equal_round_machine
        .receive_proposal(equal_round_proposal, true, 510)
        .unwrap();
    assert!(actions.iter().any(|action| matches!(
        action,
        BftConsensusActionV2::SignVote(intent)
            if intent.phase == BftVotePhaseV2::Prevote
                && intent.value == (BftVoteValueV2::Block { block_id: equal_round_block_id.clone() })
    )));

    // A quorum of nil precommits arms the phase timeout without committing A.
    for (validator_id, key) in &keys {
        if validator_id == &local_id {
            continue;
        }
        machine
            .receive_vote(
                fixture_vote(
                    &schedule,
                    validator_id,
                    key,
                    1,
                    BftVotePhaseV2::Precommit,
                    BftVoteValueV2::Nil,
                ),
                round_one_now + 4,
            )
            .unwrap();
    }
    let deadline = machine.step_deadline_ms().unwrap();
    machine.on_timeout(deadline, &local_id).unwrap();
    assert_eq!(machine.round(), 2);

    let stale_valid_round =
        signed_proposal_with_round_evidence(&schedule, &keys, 2, 0, 11, &[0, 1, 2]);
    let actions = machine
        .receive_proposal(stale_valid_round, true, deadline + 1)
        .unwrap();
    let local_vote = actions.into_iter().find_map(|action| match action {
        BftConsensusActionV2::SignVote(intent) if intent.phase == BftVotePhaseV2::Prevote => {
            Some(intent)
        }
        _ => None,
    });
    assert_eq!(
        local_vote.map(|intent| intent.value),
        Some(BftVoteValueV2::Nil),
        "a valid round below the local lock must not unlock a different value"
    );
}

#[test]
fn nil_prevotes_advance_to_nil_precommit_without_emitting_a_commit() {
    let (_, keys, set, schedule) = machine_fixture("nil-votes");
    let local_id = schedule.expected_proposer_for_round(0).unwrap();
    let local_key = fixture_key(&keys, &local_id);
    let mut machine =
        BftConsensusMachineV2::new(schedule.clone(), local_id.clone(), 100, TIMEOUTS).unwrap();
    let prevote_intent = machine
        .on_timeout(200, &local_id)
        .unwrap()
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV2::SignVote(intent) if intent.phase == BftVotePhaseV2::Prevote => {
                Some(intent)
            }
            _ => None,
        })
        .expect("proposal timeout requests nil prevote");
    let local_prevote = prevote_intent.sign(&set, local_key).unwrap();
    let actions = machine.record_local_vote(local_prevote, 201).unwrap();
    assert!(actions.is_empty());
    let mut precommit_intent = None;
    for (validator_id, key) in &keys {
        if validator_id == &local_id {
            continue;
        }
        for action in machine
            .receive_vote(
                fixture_vote(
                    &schedule,
                    validator_id,
                    key,
                    0,
                    BftVotePhaseV2::Prevote,
                    BftVoteValueV2::Nil,
                ),
                202,
            )
            .unwrap()
        {
            if let BftConsensusActionV2::SignVote(intent) = action
                && intent.phase == BftVotePhaseV2::Precommit
            {
                precommit_intent = Some(intent);
            }
        }
    }
    let precommit_intent = precommit_intent.expect("nil prevote quorum requests nil precommit");
    assert_eq!(precommit_intent.value, BftVoteValueV2::Nil);
    let local_precommit = precommit_intent.sign(&set, local_key).unwrap();
    let actions = machine.record_local_vote(local_precommit, 203).unwrap();
    assert!(actions.is_empty());
    for (validator_id, key) in &keys {
        if validator_id == &local_id {
            continue;
        }
        let actions = machine
            .receive_vote(
                fixture_vote(
                    &schedule,
                    validator_id,
                    key,
                    0,
                    BftVotePhaseV2::Precommit,
                    BftVoteValueV2::Nil,
                ),
                204,
            )
            .unwrap();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, BftConsensusActionV2::Commit(_)))
        );
    }
    assert_eq!(machine.step(), BftStepV2::Precommit);
}

#[test]
fn future_round_advances_only_after_unique_power_exceeds_one_third() {
    let (_, keys, set, schedule) = machine_fixture("future-round");
    let local_id = "v4";
    let mut machine =
        BftConsensusMachineV2::new(schedule.clone(), local_id, 100, TIMEOUTS).unwrap();
    assert_eq!(set.future_round_voting_power(), 2);
    let vote = |index: usize| {
        let (validator_id, key) = &keys[index];
        fixture_vote(
            &schedule,
            validator_id,
            key,
            1,
            BftVotePhaseV2::Prevote,
            BftVoteValueV2::Nil,
        )
    };

    let first = vote(0);
    machine.receive_vote(first.clone(), 101).unwrap();
    machine.receive_vote(first, 102).unwrap();
    assert_eq!(machine.round(), 0, "repeated signer power is counted once");

    let actions = machine.receive_vote(vote(1), 103).unwrap();
    assert_eq!(machine.round(), 1);
    assert!(actions.iter().any(|action| matches!(
        action,
        BftConsensusActionV2::RoundAdvanced {
            height: 1,
            round: 1
        }
    )));
}

#[test]
fn quorum_of_any_prevote_value_arms_timeout_when_entering_prevote() {
    let (scope, keys, set, schedule) = machine_fixture("any-vote-timeout");
    let local_id = "observer";
    let mut machine =
        BftConsensusMachineV2::new(schedule.clone(), local_id, 100, TIMEOUTS).unwrap();
    let values = [
        BftVoteValueV2::Nil,
        BftVoteValueV2::Block {
            block_id: "aa".repeat(32),
        },
        BftVoteValueV2::Block {
            block_id: "bb".repeat(32),
        },
    ];
    for (index, value) in values.into_iter().enumerate() {
        let (validator_id, key) = &keys[index];
        machine
            .receive_vote(
                fixture_vote(
                    &schedule,
                    validator_id,
                    key,
                    0,
                    BftVotePhaseV2::Prevote,
                    value,
                ),
                110 + index as i64,
            )
            .unwrap();
    }
    assert_eq!(machine.step(), BftStepV2::Propose);
    assert_eq!(machine.step_deadline_ms(), Some(200));

    let proposal = plain_proposal(
        &scope,
        &set,
        &schedule,
        &keys,
        0,
        fixture_block(&scope, &set, &schedule, 21),
    );
    machine.receive_proposal(proposal, true, 150).unwrap();
    assert_eq!(machine.step(), BftStepV2::Prevote);
    assert_eq!(machine.step_deadline_ms(), Some(250));
    let (validator_id, key) = &keys[3];
    machine
        .receive_vote(
            fixture_vote(
                &schedule,
                validator_id,
                key,
                0,
                BftVotePhaseV2::Prevote,
                BftVoteValueV2::Block {
                    block_id: "cc".repeat(32),
                },
            ),
            170,
        )
        .unwrap();
    assert_eq!(
        machine.step_deadline_ms(),
        Some(250),
        "the quorum timeout arms once"
    );
}

fn machine_fixture(
    label: &str,
) -> (
    BftScopeV2,
    Vec<(String, SigningKey)>,
    BftValidatorSetV2,
    BftHeightProposerScheduleV2,
) {
    let scope = BftScopeV2 {
        world_id: format!("{label}-world"),
        chain_id: format!("{label}-chain"),
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
    (scope, keys, set, schedule)
}

fn fixture_key<'a>(keys: &'a [(String, SigningKey)], validator_id: &str) -> &'a SigningKey {
    &keys
        .iter()
        .find(|(id, _)| id == validator_id)
        .expect("validator key exists")
        .1
}

fn fixture_block(
    scope: &BftScopeV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
    marker: u8,
) -> BftBlockV2 {
    BftBlockV2 {
        protocol_version: BFT_PROTOCOL_VERSION_V2,
        scope: scope.clone(),
        height: schedule.height(),
        parent_commit_ref: schedule.parent_commit_ref().to_string(),
        authority_epoch: schedule.authority_epoch(),
        active_validator_set_hash: set.set_hash().to_string(),
        schedule_policy_hash: schedule.policy_hash().to_string(),
        manifest_hash: schedule.parent_manifest_hash().to_string(),
        post_manifest_hash: "44".repeat(32),
        actions_cbor: vec![0x80, marker],
        action_root: format!("{marker:02x}").repeat(32),
        event_root: "66".repeat(32),
        execution_block_hash: "77".repeat(32),
        execution_state_root: "88".repeat(32),
        state_root: "99".repeat(32),
        transition: None,
        transition_ref: None,
    }
}

fn plain_proposal(
    scope: &BftScopeV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
    keys: &[(String, SigningKey)],
    round: u64,
    block: BftBlockV2,
) -> BftProposalV2 {
    let proposer_id = schedule.expected_proposer_for_round(round).unwrap();
    let key = fixture_key(keys, &proposer_id);
    let header = BftProposalHeaderV2::sign(
        scope.clone(),
        block.height,
        round,
        set.set_hash().to_string(),
        schedule.policy_hash().to_string(),
        proposer_id,
        block.block_id().unwrap(),
        None,
        key,
    )
    .unwrap();
    BftProposalV2 {
        header,
        block,
        valid_round_evidence: None,
    }
}

fn fixture_vote(
    schedule: &BftHeightProposerScheduleV2,
    validator_id: &str,
    key: &SigningKey,
    round: u64,
    phase: BftVotePhaseV2,
    value: BftVoteValueV2,
) -> BftVoteV2 {
    BftVoteV2::sign(
        schedule.scope().clone(),
        schedule.height(),
        round,
        phase,
        schedule.active_set().set_hash().to_string(),
        schedule.policy_hash().to_string(),
        value,
        validator_id.to_string(),
        key,
    )
    .unwrap()
}
