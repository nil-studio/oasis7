use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::{
    BftBlockV1, BftProposalHeaderV1, BftProposalV1, BftScopeV1, BftValidRoundEvidenceV1,
    BftVotePhaseV1, BftVoteV1, BftVoteValueV1,
};

use super::super::BftValidatorV1;
use super::*;

const TEST_TIMEOUTS: BftTimeoutsV1 = BftTimeoutsV1 {
    propose_ms: 10,
    prevote_ms: 10,
    precommit_ms: 10,
    round_delta_ms: 5,
};

fn keys() -> Vec<SigningKey> {
    (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect()
}

fn scope() -> BftScopeV1 {
    BftScopeV1 {
        world_id: "bft-test-world".into(),
        chain_id: "bft-test-chain".into(),
        genesis_hash: "11".repeat(32),
    }
}

fn validator_set(keys: &[SigningKey]) -> BftValidatorSetV1 {
    BftValidatorSetV1::new(
        scope(),
        keys.iter()
            .enumerate()
            .map(|(index, key)| BftValidatorV1 {
                validator_id: format!("v{index}"),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                voting_power: 1,
            })
            .collect(),
    )
    .expect("valid test set")
}

fn proposer(set: &BftValidatorSetV1) -> (String, SigningKey) {
    let mut priority = BftProposerPriorityV1::new(set).expect("priority");
    let selected = priority.next_proposer(set).expect("proposer");
    let index = selected
        .validator_id
        .strip_prefix('v')
        .expect("test validator id")
        .parse::<usize>()
        .expect("test validator index");
    (selected.validator_id, keys()[index].clone())
}

fn proposer_at(set: &BftValidatorSetV1, round: u64) -> (String, SigningKey) {
    let mut priority = BftProposerPriorityV1::new(set).expect("priority");
    let mut selected = None;
    for _ in 0..=round {
        selected = Some(priority.next_proposer(set).expect("proposer"));
    }
    let selected = selected.expect("round has proposer");
    let index = selected
        .validator_id
        .strip_prefix('v')
        .expect("test validator id")
        .parse::<usize>()
        .expect("test validator index");
    (selected.validator_id, keys()[index].clone())
}

fn signed_proposal(
    set: &BftValidatorSetV1,
    round: u64,
    block: BftBlockV1,
    proposer_id: &str,
    key: &SigningKey,
    evidence: Option<BftValidRoundEvidenceV1>,
) -> BftProposalV1 {
    let block_id = block.block_id().expect("block id");
    let valid_round = evidence.as_ref().map(|proof| proof.proposal.round);
    BftProposalV1 {
        header: BftProposalHeaderV1::sign(
            block.scope.clone(),
            block.height,
            round,
            set.set_hash().to_string(),
            proposer_id.to_string(),
            block_id,
            valid_round,
            key,
        )
        .expect("proposal header"),
        block,
        valid_round_evidence: evidence,
    }
}

fn block(scope: BftScopeV1) -> BftBlockV1 {
    BftBlockV1 {
        scope,
        height: 1,
        parent_commit_ref: "22".repeat(32),
        authority_epoch: 0,
        manifest_hash: "33".repeat(32),
        actions_cbor: vec![0x80],
        action_root: "44".repeat(32),
        event_root: "55".repeat(32),
        execution_block_hash: "66".repeat(32),
        execution_state_root: "77".repeat(32),
        state_root: "88".repeat(32),
    }
}

fn vote(
    set: &BftValidatorSetV1,
    key: &SigningKey,
    validator_id: &str,
    round: u64,
    phase: BftVotePhaseV1,
    value: BftVoteValueV1,
) -> BftVoteV1 {
    BftVoteV1::sign(
        set.scope().clone(),
        1,
        round,
        phase,
        set.set_hash().to_string(),
        value,
        validator_id.to_string(),
        key,
    )
    .expect("signed vote")
}

fn decide_round_zero(
    machine: &mut BftConsensusMachineV1,
    set: &BftValidatorSetV1,
    keys: &[SigningKey],
    proposal: &BftProposalV1,
) -> BftCommitCertificateV1 {
    let local_id = machine
        .current_proposer_id()
        .expect("round proposer")
        .to_string();
    let local_index = local_id[1..].parse::<usize>().expect("local index");
    let actions = machine
        .receive_proposal(proposal.clone(), true, 1)
        .expect("valid proposal");
    let prevote_intent = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::SignVote(intent) if intent.phase == BftVotePhaseV1::Prevote => {
                Some(intent)
            }
            _ => None,
        })
        .expect("local prevote intent");
    machine
        .record_local_vote(
            prevote_intent
                .sign(set, &keys[local_index])
                .expect("signed prevote"),
            2,
        )
        .expect("record local prevote");
    let value = BftVoteValueV1::Block {
        block_id: proposal.header.block_id.clone(),
    };
    let mut precommit_intent = None;
    for (index, key) in keys.iter().enumerate() {
        if index == local_index {
            continue;
        }
        let actions = machine
            .receive_vote(
                vote(
                    set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    value.clone(),
                ),
                3,
            )
            .expect("record external prevote");
        precommit_intent = precommit_intent.or_else(|| {
            actions.into_iter().find_map(|action| match action {
                BftConsensusActionV1::SignVote(intent)
                    if intent.phase == BftVotePhaseV1::Precommit =>
                {
                    Some(intent)
                }
                _ => None,
            })
        });
        if precommit_intent.is_some() {
            break;
        }
    }
    let precommit_intent = precommit_intent.expect("quorum precommit intent");
    machine
        .record_local_vote(
            precommit_intent
                .sign(set, &keys[local_index])
                .expect("signed precommit"),
            4,
        )
        .expect("record local precommit");
    for (index, key) in keys.iter().enumerate() {
        if index == local_index {
            continue;
        }
        let actions = machine
            .receive_vote(
                vote(
                    set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Precommit,
                    value.clone(),
                ),
                5,
            )
            .expect("record external precommit");
        if let Some(certificate) = actions.into_iter().find_map(|action| match action {
            BftConsensusActionV1::Commit(certificate) => Some(certificate),
            _ => None,
        }) {
            return certificate;
        }
    }
    panic!("quorum certificate was not emitted")
}

#[test]
fn q_minus_one_votes_do_not_commit_and_q_votes_emit_one_certificate() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let expected_local_id = local_id.clone();
    let mut machine = BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");
    assert_eq!(
        machine.current_proposer_id(),
        Some(expected_local_id.as_str())
    );
    let proposal = machine
        .make_proposal(
            block(scope()),
            machine.current_proposer_id().expect("proposer"),
            &local_key,
        )
        .expect("proposal");

    let actions = machine
        .receive_proposal(proposal.clone(), false, 1)
        .expect_err("unvalidated candidate must not enter the machine");
    assert!(actions.contains("deterministic valid(v)"));
    assert!(machine.proposal_for_round(0).is_none());

    let certificate = decide_round_zero(&mut machine, &set, &keys, &proposal);
    verify_commit_certificate_v1(&set, machine.round_proposers(), &certificate)
        .expect("complete certificate verifies");
    assert_eq!(
        certificate.precommits.len(),
        set.quorum_voting_power() as usize
    );
}

#[test]
fn old_round_complete_certificate_remains_valid_after_round_advance() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let mut machine = BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");
    let proposal = machine
        .make_proposal(
            block(scope()),
            machine.current_proposer_id().expect("proposer"),
            &local_key,
        )
        .expect("proposal");
    let certificate = decide_round_zero(&mut machine, &set, &keys, &proposal);

    for (index, key) in keys.iter().enumerate() {
        if machine.round() == 1 {
            break;
        }
        machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    1,
                    BftVotePhaseV1::Prevote,
                    BftVoteValueV1::Nil,
                ),
                6,
            )
            .expect("future round vote");
    }
    assert_eq!(machine.round(), 1);
    verify_commit_certificate_v1(&set, machine.round_proposers(), &certificate)
        .expect("old-round QC remains verifiable");
    machine
        .mark_commit_applied(&certificate, 7, TEST_TIMEOUTS)
        .expect("old-round QC advances height");
    assert_eq!(machine.height(), 2);
}

#[test]
fn late_round_zero_commit_keeps_next_height_schedule_independent_of_local_round() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let proposal =
        BftConsensusMachineV1::new(set.clone(), local_id.clone(), 1, None, 0, TEST_TIMEOUTS)
            .expect("fresh machine")
            .make_proposal(block(scope()), &local_id, &local_key)
            .expect("round-zero proposal");
    let mut height_one_machine =
        BftConsensusMachineV1::new(set.clone(), local_id.clone(), 1, None, 0, TEST_TIMEOUTS)
            .expect("height-one machine");
    let certificate = decide_round_zero(&mut height_one_machine, &set, &keys, &proposal);
    let mut caught_up_machine =
        BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
            .expect("second height-one machine");
    caught_up_machine
        .advance_to_round(64, 1)
        .expect("advance to cache boundary");
    caught_up_machine
        .advance_to_round(65, 2)
        .expect("advance beyond cache boundary");

    height_one_machine
        .mark_commit_applied(&certificate, 3, TEST_TIMEOUTS)
        .expect("first node applies round-zero certificate");
    caught_up_machine
        .mark_commit_applied(&certificate, 3, TEST_TIMEOUTS)
        .expect("advanced node accepts the same late certificate");
    assert_eq!(height_one_machine.height(), 2);
    assert_eq!(caught_up_machine.height(), 2);
    assert_eq!(
        height_one_machine.current_proposer_id(),
        caught_up_machine.current_proposer_id(),
        "the next height schedule follows the committed QC round, not local timeout history"
    );
}

#[test]
fn duplicate_votes_do_not_increase_power_and_equivocation_cannot_double_count() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let mut machine = BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");
    let proposal = machine
        .make_proposal(
            block(scope()),
            machine.current_proposer_id().expect("proposer"),
            &local_key,
        )
        .expect("proposal");
    machine
        .receive_proposal(proposal.clone(), true, 1)
        .expect("proposal");
    let local_id = machine.current_proposer_id().expect("proposer").to_string();
    let local_index = local_id[1..].parse::<usize>().expect("local index");
    let block_value = BftVoteValueV1::Block {
        block_id: proposal.header.block_id.clone(),
    };
    let local_prevote = vote(
        &set,
        &keys[local_index],
        &local_id,
        0,
        BftVotePhaseV1::Prevote,
        block_value.clone(),
    );
    machine
        .record_local_vote(local_prevote, 2)
        .expect("local vote");
    let external = vote(
        &set,
        &keys[(local_index + 1) % 4],
        &format!("v{}", (local_index + 1) % 4),
        0,
        BftVotePhaseV1::Prevote,
        block_value.clone(),
    );
    machine
        .receive_vote(external.clone(), 3)
        .expect("external vote");
    machine
        .receive_vote(external.clone(), 4)
        .expect("duplicate vote is idempotent");
    assert_eq!(machine.equivocation_evidence().len(), 0);
    let conflicting = vote(
        &set,
        &keys[(local_index + 1) % 4],
        &format!("v{}", (local_index + 1) % 4),
        0,
        BftVotePhaseV1::Prevote,
        BftVoteValueV1::Nil,
    );
    machine
        .receive_vote(conflicting, 5)
        .expect("equivocation is evidence, not an engine failure");
    assert_eq!(machine.equivocation_evidence().len(), 1);
    let next_index = (local_index + 2) % keys.len();
    let actions = machine
        .receive_vote(
            vote(
                &set,
                &keys[next_index],
                &format!("v{next_index}"),
                0,
                BftVotePhaseV1::Prevote,
                block_value.clone(),
            ),
            6,
        )
        .expect("equivocation cannot erase the first block vote");
    assert!(actions.iter().any(|action| matches!(action,
        BftConsensusActionV1::SignVote(intent)
            if intent.phase == BftVotePhaseV1::Precommit
                && intent.value == block_value)));
    assert_eq!(
        machine.locked(),
        Some((proposal.header.block_id.as_str(), 0))
    );
    assert_eq!(machine.step(), BftStepV1::Precommit);
    assert_eq!(
        machine.step_deadline_ms(),
        None,
        "precommit timeout arms only after a precommit quorum is observed"
    );
}

#[test]
fn nil_first_equivocator_can_supply_one_block_vote_without_double_weight() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let mut machine = BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");
    let proposal = machine
        .make_proposal(
            block(scope()),
            machine.current_proposer_id().expect("proposer"),
            &local_key,
        )
        .expect("proposal");
    machine
        .receive_proposal(proposal.clone(), true, 1)
        .expect("proposal");
    let local_id = machine.current_proposer_id().expect("proposer").to_string();
    let local_index = local_id[1..].parse::<usize>().expect("local index");
    let first_index = (local_index + 1) % keys.len();
    let second_index = (local_index + 2) % keys.len();
    let block_value = BftVoteValueV1::Block {
        block_id: proposal.header.block_id.clone(),
    };
    machine
        .record_local_vote(
            vote(
                &set,
                &keys[local_index],
                &local_id,
                0,
                BftVotePhaseV1::Prevote,
                block_value.clone(),
            ),
            2,
        )
        .expect("local block vote");
    machine
        .receive_vote(
            vote(
                &set,
                &keys[first_index],
                &format!("v{first_index}"),
                0,
                BftVotePhaseV1::Prevote,
                BftVoteValueV1::Nil,
            ),
            3,
        )
        .expect("nil first vote");
    machine
        .receive_vote(
            vote(
                &set,
                &keys[first_index],
                &format!("v{first_index}"),
                0,
                BftVotePhaseV1::Prevote,
                block_value.clone(),
            ),
            4,
        )
        .expect("conflicting block vote is retained once for its value");
    assert_eq!(machine.equivocation_evidence().len(), 1);
    let actions = machine
        .receive_vote(
            vote(
                &set,
                &keys[second_index],
                &format!("v{second_index}"),
                0,
                BftVotePhaseV1::Prevote,
                block_value.clone(),
            ),
            5,
        )
        .expect("q block votes");
    assert!(actions.iter().any(|action| matches!(action,
        BftConsensusActionV1::SignVote(intent)
            if intent.phase == BftVotePhaseV1::Precommit
                && intent.value == block_value)));
    assert_eq!(
        machine.locked(),
        Some((proposal.header.block_id.as_str(), 0))
    );
}

#[test]
fn late_equivocation_does_not_retract_a_complete_commit_certificate() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let mut machine = BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");
    let proposal = machine
        .make_proposal(
            block(scope()),
            machine.current_proposer_id().expect("proposer"),
            &local_key,
        )
        .expect("proposal");
    let certificate = decide_round_zero(&mut machine, &set, &keys, &proposal);
    let equivocal_vote = certificate.precommits.first().expect("QC has votes");
    let index = equivocal_vote.validator_id[1..]
        .parse::<usize>()
        .expect("validator index");
    let actions = machine
        .receive_vote(
            vote(
                &set,
                &keys[index],
                &equivocal_vote.validator_id,
                0,
                BftVotePhaseV1::Precommit,
                BftVoteValueV1::Nil,
            ),
            6,
        )
        .expect("late equivocation is retained as evidence");
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, BftConsensusActionV1::Commit(_)))
    );
    verify_commit_certificate_v1(&set, machine.round_proposers(), &certificate)
        .expect("independently valid QC remains valid");
}

#[test]
fn buffered_quorums_are_replayed_when_their_proposal_arrives_late() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let mut machine = BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");
    let proposal = machine
        .make_proposal(
            block(scope()),
            machine.current_proposer_id().expect("proposer"),
            &local_key,
        )
        .expect("proposal");
    let block_value = BftVoteValueV1::Block {
        block_id: proposal.header.block_id.clone(),
    };
    let signer_count = set.quorum_voting_power() as usize;
    for (index, key) in keys.iter().enumerate().take(signer_count) {
        machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    block_value.clone(),
                ),
                1,
            )
            .expect("buffer prevote before proposal");
        machine
            .receive_vote(
                vote(
                    &set,
                    &keys[index],
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Precommit,
                    block_value.clone(),
                ),
                2,
            )
            .expect("buffer precommit before proposal");
    }
    assert_eq!(machine.step(), BftStepV1::Propose);
    assert_eq!(machine.locked(), None);

    let actions = machine
        .receive_proposal(proposal, true, 3)
        .expect("late proposal");
    assert!(actions.iter().any(|action| matches!(action,
        BftConsensusActionV1::SignVote(intent)
            if intent.phase == BftVotePhaseV1::Prevote)));
    assert!(actions.iter().any(|action| matches!(action,
        BftConsensusActionV1::SignVote(intent)
            if intent.phase == BftVotePhaseV1::Precommit
                && intent.value == block_value)));
    let certificate = actions.iter().find_map(|action| match action {
        BftConsensusActionV1::Commit(certificate) => Some(certificate),
        _ => None,
    });
    assert!(
        certificate.is_some(),
        "buffered q precommits must commit once proposal is known"
    );
}

#[test]
fn precommit_timeout_waits_for_precommit_quorum_until_phase_entry() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let mut machine =
        BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 100, TEST_TIMEOUTS)
            .expect("machine");
    let proposal = machine
        .make_proposal(
            block(scope()),
            machine.current_proposer_id().expect("proposer"),
            &local_key,
        )
        .expect("proposal");
    let block_value = BftVoteValueV1::Block {
        block_id: proposal.header.block_id.clone(),
    };
    for (index, key) in keys
        .iter()
        .enumerate()
        .take(set.quorum_voting_power() as usize)
    {
        machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Precommit,
                    BftVoteValueV1::Nil,
                ),
                101,
            )
            .expect("buffer precommits before proposal");
    }
    machine
        .receive_proposal(proposal, true, 105)
        .expect("enter prevote");
    assert_eq!(machine.step(), BftStepV1::Prevote);
    assert_eq!(machine.step_deadline_ms(), None);

    for (index, key) in keys
        .iter()
        .enumerate()
        .take(set.quorum_voting_power() as usize)
    {
        machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    block_value.clone(),
                ),
                106,
            )
            .expect("quorum locks proposal and enters precommit");
    }
    assert_eq!(machine.step(), BftStepV1::Precommit);
    assert_eq!(machine.step_deadline_ms(), Some(116));
}

#[test]
fn locked_valid_round_proof_and_proposer_schedule_survive_cache_window() {
    let keys = keys();
    let set = validator_set(&keys);
    let (round_zero_proposer, round_zero_key) = proposer_at(&set, 0);
    let (local_id, _) = proposer_at(&set, 65);
    let mut machine =
        BftConsensusMachineV1::new(set.clone(), local_id.clone(), 1, None, 0, TEST_TIMEOUTS)
            .expect("machine");
    let value_block = block(scope());
    let proposal = signed_proposal(
        &set,
        0,
        value_block.clone(),
        &round_zero_proposer,
        &round_zero_key,
        None,
    );
    machine
        .receive_proposal(proposal.clone(), true, 1)
        .expect("round-zero proposal");
    let value = BftVoteValueV1::Block {
        block_id: proposal.header.block_id.clone(),
    };
    for (index, key) in keys
        .iter()
        .enumerate()
        .take(set.quorum_voting_power() as usize)
    {
        machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    value.clone(),
                ),
                2 + index as i64,
            )
            .expect("round-zero prevote");
    }
    assert_eq!(
        machine.valid(),
        Some((proposal.header.block_id.as_str(), 0))
    );
    assert_eq!(
        machine.locked(),
        Some((proposal.header.block_id.as_str(), 0))
    );

    advance_with_future_votes(&mut machine, &set, &keys, 65);
    assert_eq!(machine.round(), 65);
    assert!(!machine.proposals.contains_key(&0));
    assert!(!machine.votes.contains_key(&(0, BftVotePhaseV1::Prevote)));
    assert_eq!(machine.round_proposers.get(&0), Some(&round_zero_proposer));

    let current_proposer = machine.current_proposer_id().expect("round proposer");
    assert_eq!(current_proposer, local_id);
    let current_index = current_proposer[1..]
        .parse::<usize>()
        .expect("current proposer index");
    let reproposal = machine
        .make_proposal(value_block, current_proposer, &keys[current_index])
        .expect("old locked value must remain reproposable");
    assert_eq!(reproposal.header.valid_round, Some(0));
    assert_eq!(reproposal.valid_round_evidence, machine.valid_round_proof);
}

#[test]
fn pruned_vote_cache_does_not_fail_open_on_wrong_old_valid_round_proposer() {
    let keys = keys();
    let set = validator_set(&keys);
    let (correct_old_proposer, _) = proposer_at(&set, 0);
    let (local_id, _) = proposer_at(&set, 65);
    let wrong_old_index = (0..keys.len())
        .find(|index| format!("v{index}") != correct_old_proposer)
        .expect("a non-proposer exists");
    let mut machine = BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");
    let value_block = block(scope());
    let block_id = value_block.block_id().expect("block id");
    let forged_old_header = BftProposalHeaderV1::sign(
        scope(),
        1,
        0,
        set.set_hash().to_string(),
        format!("v{wrong_old_index}"),
        block_id.clone(),
        None,
        &keys[wrong_old_index],
    )
    .expect("valid signature from wrong scheduled proposer");
    let evidence = BftValidRoundEvidenceV1 {
        proposal: forged_old_header,
        prevotes: (0..set.quorum_voting_power() as usize)
            .map(|index| {
                vote(
                    &set,
                    &keys[index],
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    BftVoteValueV1::Block {
                        block_id: block_id.clone(),
                    },
                )
            })
            .collect(),
    };

    advance_with_future_votes(&mut machine, &set, &keys, 65);
    let current_proposer = machine.current_proposer_id().expect("round proposer");
    assert_eq!(current_proposer, proposer_at(&set, 65).0);
    let current_index = current_proposer[1..]
        .parse::<usize>()
        .expect("current proposer index");
    let proposal = signed_proposal(
        &set,
        65,
        value_block,
        current_proposer,
        &keys[current_index],
        Some(evidence),
    );
    let result = machine.receive_proposal(proposal, true, 100);
    assert!(result.is_err_and(|error| error.contains("wrong deterministic proposer")));
    assert!(machine.proposal_for_round(65).is_none());
}

fn advance_with_future_votes(
    machine: &mut BftConsensusMachineV1,
    set: &BftValidatorSetV1,
    keys: &[SigningKey],
    target_round: u64,
) {
    let mut now_ms = 10;
    while machine.round() < target_round {
        let future_round = machine
            .round()
            .saturating_add(BFT_MAX_TRACKED_ROUNDS_V1)
            .min(target_round);
        for (index, key) in keys
            .iter()
            .enumerate()
            .take(set.future_round_voting_power() as usize)
        {
            machine
                .receive_vote(
                    vote(
                        set,
                        key,
                        &format!("v{index}"),
                        future_round,
                        BftVotePhaseV1::Precommit,
                        BftVoteValueV1::Nil,
                    ),
                    now_ms,
                )
                .expect("future-round vote advances the bounded window");
            now_ms += 1;
        }
    }
}

#[test]
fn round_schedule_and_local_vote_slots_stay_bounded_without_resetting_height() {
    let keys = keys();
    let set = validator_set(&keys);
    let local_id = "v0";
    let mut machine = BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");
    for round in 0..=BFT_MAX_ROUNDS_PER_HEIGHT_V1 {
        if round > 0 {
            machine
                .advance_to_round(round, round as i64)
                .expect("bounded next round");
        }
        for phase in [BftVotePhaseV1::Prevote, BftVotePhaseV1::Precommit] {
            machine
                .record_local_vote(
                    vote(&set, &keys[0], local_id, round, phase, BftVoteValueV1::Nil),
                    round as i64,
                )
                .expect("record local vote");
        }
    }
    assert_eq!(machine.round(), BFT_MAX_ROUNDS_PER_HEIGHT_V1);
    assert_eq!(
        machine.round_proposers.len(),
        BFT_MAX_ROUNDS_PER_HEIGHT_V1 as usize + 1
    );
    assert_eq!(
        machine.round_proposers.get(&0).map(String::as_str),
        Some(proposer(&set).0.as_str())
    );
    assert!(machine.local_votes.len() <= 2 * (BFT_MAX_TRACKED_ROUNDS_V1 as usize + 1));
    assert!(
        machine.local_votes.keys().all(|(round, _)| {
            *round >= BFT_MAX_ROUNDS_PER_HEIGHT_V1 - BFT_MAX_TRACKED_ROUNDS_V1
        })
    );
    assert!(
        machine
            .vote_intent(local_id, BftVotePhaseV1::Prevote, BftVoteValueV1::Nil, 0,)
            .is_err()
    );

    assert!(
        machine
            .advance_to_round(BFT_MAX_ROUNDS_PER_HEIGHT_V1 + 1, 5000)
            .is_err()
    );
    assert_eq!(machine.round(), BFT_MAX_ROUNDS_PER_HEIGHT_V1);
    assert_eq!(
        machine.round_proposers.len(),
        BFT_MAX_ROUNDS_PER_HEIGHT_V1 as usize + 1
    );
}

#[test]
fn timeout_growth_is_checked_and_nil_precommit_advances_round() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, _) = proposer(&set);
    let mut machine =
        BftConsensusMachineV1::new(set.clone(), local_id.clone(), 1, None, 100, TEST_TIMEOUTS)
            .expect("machine");
    assert!(
        machine
            .on_timeout(109, &local_id)
            .expect("early timeout")
            .is_empty()
    );
    let prevote = machine.on_timeout(110, &local_id).expect("propose timeout");
    assert!(prevote.iter().any(|action| matches!(action,
        BftConsensusActionV1::SignVote(intent)
            if intent.phase == BftVotePhaseV1::Prevote
                && intent.value == BftVoteValueV1::Nil)));
    assert_eq!(machine.step(), BftStepV1::Prevote);
    assert_eq!(machine.step_deadline_ms(), None);
    let unrelated = BftVoteValueV1::Block {
        block_id: "99".repeat(32),
    };
    for (index, key) in keys
        .iter()
        .enumerate()
        .take(set.quorum_voting_power() as usize)
    {
        machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    unrelated.clone(),
                ),
                111,
            )
            .expect("prevote quorum arms timeout");
    }
    assert_eq!(machine.step_deadline_ms(), Some(121));
    assert!(
        machine
            .on_timeout(120, &local_id)
            .expect("prevote timeout not due")
            .is_empty()
    );
    let precommit = machine
        .on_timeout(121, &local_id)
        .expect("prevote timeout emits nil precommit");
    assert!(precommit.iter().any(|action| matches!(action,
        BftConsensusActionV1::SignVote(intent)
            if intent.phase == BftVotePhaseV1::Precommit
                && intent.value == BftVoteValueV1::Nil)));
    assert_eq!(machine.step(), BftStepV1::Precommit);
    assert_eq!(machine.step_deadline_ms(), None);
    for (index, key) in keys
        .iter()
        .enumerate()
        .take(set.quorum_voting_power() as usize)
    {
        machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Precommit,
                    BftVoteValueV1::Nil,
                ),
                122,
            )
            .expect("precommit quorum arms timeout");
    }
    assert_eq!(machine.step_deadline_ms(), Some(132));
    assert!(
        machine
            .on_timeout(131, &local_id)
            .expect("precommit timeout not due")
            .is_empty()
    );
    let advanced = machine
        .on_timeout(132, &local_id)
        .expect("precommit timeout advances round");
    assert!(
        advanced
            .iter()
            .any(|action| matches!(action, BftConsensusActionV1::RoundAdvanced { round: 1, .. }))
    );
    assert_eq!(machine.round(), 1);

    let round_one = vote(
        &set,
        &keys[0],
        "v0",
        1,
        BftVotePhaseV1::Precommit,
        BftVoteValueV1::Nil,
    );
    let round_two = vote(
        &set,
        &keys[1],
        "v1",
        2,
        BftVotePhaseV1::Precommit,
        BftVoteValueV1::Nil,
    );
    machine
        .receive_vote(round_one, 131)
        .expect("current round vote one");
    machine
        .receive_vote(
            vote(
                &set,
                &keys[1],
                "v1",
                1,
                BftVotePhaseV1::Precommit,
                BftVoteValueV1::Nil,
            ),
            132,
        )
        .expect("current round vote two");
    assert_eq!(machine.round(), 1);
    machine
        .receive_vote(round_two, 133)
        .expect("round two vote");
    machine
        .receive_vote(
            vote(
                &set,
                &keys[2],
                "v2",
                2,
                BftVotePhaseV1::Precommit,
                BftVoteValueV1::Nil,
            ),
            134,
        )
        .expect("round two f+1 vote");
    assert_eq!(machine.round(), 2);
    assert!(machine.step_deadline_ms().is_some());
    assert_eq!(TEST_TIMEOUTS.for_step(BftStepV1::Propose, 2), Ok(20));
    assert!(
        BftTimeoutsV1 {
            propose_ms: u64::MAX,
            ..TEST_TIMEOUTS
        }
        .for_step(BftStepV1::Propose, 2)
        .is_err()
    );
}

#[test]
fn signing_intents_are_idempotent_and_reject_conflicting_local_slot() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, _) = proposer(&set);
    let machine = BftConsensusMachineV1::new(set, local_id.clone(), 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");
    assert!(
        machine
            .vote_intent(
                local_id.clone(),
                BftVotePhaseV1::Prevote,
                BftVoteValueV1::Nil,
                0
            )
            .expect("local intent")
            .is_some()
    );
    assert!(
        machine
            .vote_intent("not-local", BftVotePhaseV1::Prevote, BftVoteValueV1::Nil, 0)
            .is_err()
    );
    assert!(
        machine
            .vote_intent(
                local_id,
                BftVotePhaseV1::Prevote,
                BftVoteValueV1::Block {
                    block_id: "99".repeat(32),
                },
                0,
            )
            .is_ok()
    );
}

#[test]
fn local_proposal_requires_current_scope_height_identity_and_key() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let machine =
        BftConsensusMachineV1::new(set.clone(), local_id.clone(), 1, None, 0, TEST_TIMEOUTS)
            .expect("machine");
    let base = block(scope());
    let wrong_key_index = (0..keys.len())
        .find(|index| format!("v{index}") != local_id)
        .expect("different validator key");
    assert!(
        machine
            .make_proposal(base.clone(), &local_id, &keys[wrong_key_index])
            .is_err_and(|error| error.contains("key does not match"))
    );
    let mut wrong_scope = base.clone();
    wrong_scope.scope.chain_id.push_str("-other");
    assert!(
        machine
            .make_proposal(wrong_scope, &local_id, &local_key)
            .is_err_and(|error| error.contains("scope"))
    );
    let mut wrong_height = base.clone();
    wrong_height.height = 2;
    assert!(
        machine
            .make_proposal(wrong_height, &local_id, &local_key)
            .is_err_and(|error| error.contains("height"))
    );
    assert!(
        machine
            .make_proposal(base, "not-local", &local_key)
            .is_err_and(|error| error.contains("not the local validator"))
    );
}

#[path = "engine_timeout_tests.rs"]
mod timeout_tests;
