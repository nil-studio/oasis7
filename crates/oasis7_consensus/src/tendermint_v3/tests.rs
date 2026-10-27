//! Protocol vectors rooted in a truly signed qualified H. The semantic roots
//! are explicit vectors; actual World execution is exercised by Node consumers.
use super::*;
use crate::tendermint_v1::BftTimeoutsV1;
use ed25519_dalek::SigningKey;
use oasis7_formal_protocol::{
    bft::{
        BFT_FORMAL_EXECUTION_DOMAIN_V1, BftProfileTargetV2, BftSchedulePolicyV2,
        verify_formal_bft_handoff_v2,
    },
    controlled_authority::formal::{
        self as formal, FormalCompletionBranchV1, InitialActivationTrustV1, ProfileHandoffV1,
        VerifiedFormalReadAnchorV1, test_fixtures as fixtures,
    },
};
use oasis7_proto::distributed_bft::{
    v2::{BftScopeV2, BftValidatorV2},
    v3::{BFT_PROTOCOL_VERSION_V3, BftBlockHeaderV3, BftVotePhaseV3, BftVoteV3, BftVoteValueV3},
};

const TIMEOUTS: BftTimeoutsV1 = BftTimeoutsV1 {
    propose_ms: 100,
    prevote_ms: 100,
    precommit_ms: 100,
    round_delta_ms: 25,
};

#[test]
fn header_journal_rejects_declared_event_excess_before_reading_elements() {
    let mut bytes = vec![0xa1, 0x66];
    bytes.extend_from_slice(b"events");
    bytes.push(0x9a);
    bytes.extend_from_slice(&((BFT_MAX_SIGNING_JOURNAL_EVENTS_V3 + 1) as u32).to_be_bytes());
    let error = serde_cbor::from_slice::<BftSigningJournalV3>(&bytes).unwrap_err();
    assert!(
        error.to_string().contains("event count exceeds bound"),
        "{error}"
    );
}

fn setup(weights: &[u64]) -> (BftHeaderHeightScheduleV3, Vec<(String, SigningKey)>) {
    let keys = weights
        .iter()
        .enumerate()
        .map(|(i, _)| {
            (
                format!("validator-{i}"),
                SigningKey::from_bytes(&[31 + i as u8; 32]),
            )
        })
        .collect::<Vec<_>>();
    let target = BftProfileTargetV2::new(
        BftScopeV2 {
            world_id: "world".into(),
            chain_id: "chain".into(),
            genesis_hash: fixtures::h(8),
        },
        2,
        keys.iter()
            .zip(weights)
            .map(|((id, key), power)| BftValidatorV2 {
                validator_id: id.clone(),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                voting_power: *power,
            })
            .collect(),
        BftSchedulePolicyV2::new(4, "bft").unwrap(),
    )
    .unwrap();
    let (certificate, authority, initial_proof, initial_commit) = fixtures::initial();
    let trust = InitialActivationTrustV1 {
        issuer_public_key: fixtures::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let anchor =
        VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &initial_proof).unwrap();
    let frozen = fixtures::frozen(
        &authority,
        &initial_commit,
        Some(ProfileHandoffV1 {
            next_profile: "bft".into(),
            next_profile_version: 1,
            next_authority_digest: target.target_digest().unwrap(),
            valid_from_position: 3,
        }),
    );
    let record = fixtures::execution(&authority, &frozen);
    let proof = fixtures::proof(
        record,
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let commit =
        formal::verify_formal_qualified(&proof, &authority, Some(&initial_commit), None).unwrap();
    let anchor = anchor.after_verified_commit(authority, commit).unwrap();
    let handoff = verify_formal_bft_handoff_v2(&anchor, target).unwrap();
    let schedule = handoff
        .initial_schedule_after_handoff_replay(&fixtures::h(101), &fixtures::h(102))
        .unwrap();
    assert_eq!(
        schedule.height(),
        3,
        "qualified H starts after controlled position 2"
    );
    (schedule, keys)
}

fn value(schedule: &BftHeaderHeightScheduleV3) -> BftHeaderValueV3 {
    let commitment = FormalBftExecutionCommitmentV1 {
        domain: BFT_FORMAL_EXECUTION_DOMAIN_V1.into(),
        schema_version: 1,
        profile: "bft".into(),
        profile_version: 1,
        activation_digest: schedule.activation_digest().into(),
        scope: schedule.scope().clone(),
        height: schedule.height(),
        parent_commit_ref: schedule.parent_commit_ref().into(),
        parent_tick: schedule.parent_tick(),
        parent_semantic_state_root: schedule.parent_state_root().into(),
        authority_epoch: schedule.authority_epoch(),
        active_validator_set_hash: schedule.active_set().set_hash().into(),
        schedule_policy_hash: schedule.policy_hash().into(),
        manifest_hash: schedule.parent_manifest_hash().into(),
        post_manifest_hash: fixtures::h(103),
        action_root: fixtures::h(104),
        event_root: fixtures::h(105),
        execution_state_root: fixtures::h(106),
        state_root: fixtures::h(107),
        projection_root: fixtures::h(108),
        result_root: fixtures::h(109),
        transition: None,
        transition_ref: None,
    };
    BftHeaderValueV3 {
        header: BftBlockHeaderV3 {
            protocol_version: BFT_PROTOCOL_VERSION_V3,
            execution_commitment_hash: commitment.digest().unwrap(),
            actions_cbor_digest: oasis7_proto::distributed_bft::v3::actions_cbor_digest_v3(&[0x80])
                .unwrap(),
        },
        execution_commitment: commitment,
    }
}
fn key<'a>(keys: &'a [(String, SigningKey)], id: &str) -> &'a SigningKey {
    &keys.iter().find(|(known, _)| known == id).unwrap().1
}
fn signed_vote(
    schedule: &BftHeaderHeightScheduleV3,
    id: &str,
    key: &SigningKey,
    phase: BftVotePhaseV3,
    value: BftVoteValueV3,
) -> BftVoteV3 {
    BftVoteV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        phase,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        value,
        id.into(),
        key,
    )
    .unwrap()
}
fn vote_effect(effects: Vec<BftConsensusActionV3>, phase: BftVotePhaseV3) -> BftVoteIntentV3 {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            BftConsensusActionV3::SignVote(intent) if intent.phase == phase => Some(intent),
            _ => None,
        })
        .expect("phase requests a deterministic local vote")
}

#[test]
fn header_machine_requires_all_three_equal_votes_and_applies_only_verified_successor() {
    let (schedule, keys) = setup(&[1, 1, 1]);
    let local = schedule.expected_proposer_for_round(0).unwrap();
    let mut machine =
        BftConsensusMachineV3::new(schedule.clone(), local.clone(), 0, TIMEOUTS).unwrap();
    let intent = machine.proposal_intent(value(&schedule), &local).unwrap();
    let proposal = machine
        .sign_proposal_intent(&intent, key(&keys, &local))
        .unwrap();
    let id = proposal.header.block_id.clone();
    let prevote = vote_effect(
        machine.receive_proposal(proposal, true, 1).unwrap(),
        BftVotePhaseV3::Prevote,
    );
    machine
        .record_local_vote(
            prevote
                .sign(schedule.active_set(), key(&keys, &local))
                .unwrap(),
            2,
        )
        .unwrap();
    let remotes = keys
        .iter()
        .filter(|(id, _)| id != &local)
        .collect::<Vec<_>>();
    let block = BftVoteValueV3::Block {
        block_id: id.clone(),
    };
    let first = signed_vote(
        &schedule,
        &remotes[0].0,
        &remotes[0].1,
        BftVotePhaseV3::Prevote,
        block.clone(),
    );
    assert!(machine.receive_vote(first.clone(), 3).unwrap().is_empty());
    assert!(
        machine.receive_vote(first, 3).unwrap().is_empty(),
        "duplicate power does not count twice"
    );
    assert!(
        machine.locked().is_none(),
        "two of three is below strict quorum"
    );
    let effects = machine
        .receive_vote(
            signed_vote(
                &schedule,
                &remotes[1].0,
                &remotes[1].1,
                BftVotePhaseV3::Prevote,
                block.clone(),
            ),
            4,
        )
        .unwrap();
    let precommit = vote_effect(effects, BftVotePhaseV3::Precommit);
    assert_eq!(machine.locked(), Some((id.as_str(), 0)));
    machine
        .record_local_vote(
            precommit
                .sign(schedule.active_set(), key(&keys, &local))
                .unwrap(),
            5,
        )
        .unwrap();
    assert!(
        machine
            .receive_vote(
                signed_vote(
                    &schedule,
                    &remotes[0].0,
                    &remotes[0].1,
                    BftVotePhaseV3::Precommit,
                    block.clone()
                ),
                6
            )
            .unwrap()
            .is_empty()
    );
    let effects = machine
        .receive_vote(
            signed_vote(
                &schedule,
                &remotes[1].0,
                &remotes[1].1,
                BftVotePhaseV3::Precommit,
                block,
            ),
            7,
        )
        .unwrap();
    let certificate = effects
        .into_iter()
        .find_map(|effect| match effect {
            BftConsensusActionV3::Commit(qc) => Some(qc),
            _ => None,
        })
        .unwrap();
    let verified = verify_commit_certificate_token_v3(&schedule, &certificate).unwrap();
    machine.mark_commit_applied(&verified, 8, TIMEOUTS).unwrap();
    assert_eq!(machine.height(), 4);
    assert_eq!(machine.schedule(), verified.next_height_schedule());
}

#[test]
fn header_machine_uses_weighted_power_and_rejects_unexecuted_values() {
    let (schedule, keys) = setup(&[4, 1, 1, 1]);
    let local = schedule.expected_proposer_for_round(0).unwrap();
    assert_eq!(
        schedule
            .active_set()
            .validator(&local)
            .unwrap()
            .voting_power,
        4
    );
    let mut machine =
        BftConsensusMachineV3::new(schedule.clone(), local.clone(), 0, TIMEOUTS).unwrap();
    let intent = machine.proposal_intent(value(&schedule), &local).unwrap();
    let proposal = machine
        .sign_proposal_intent(&intent, key(&keys, &local))
        .unwrap();
    assert!(
        machine
            .receive_proposal(proposal.clone(), false, 1)
            .is_err()
    );
    assert!(machine.proposal_for_round(0).is_none());
    assert!(machine.locked().is_none());
    let id = proposal.header.block_id.clone();
    let prevote = vote_effect(
        machine.receive_proposal(proposal, true, 1).unwrap(),
        BftVotePhaseV3::Prevote,
    );
    assert!(
        machine
            .record_local_vote(
                prevote
                    .sign(schedule.active_set(), key(&keys, &local))
                    .unwrap(),
                2
            )
            .unwrap()
            .is_empty()
    );
    let (remote, remote_key) = keys.iter().find(|(id, _)| id != &local).unwrap();
    let effects = machine
        .receive_vote(
            signed_vote(
                &schedule,
                remote,
                remote_key,
                BftVotePhaseV3::Prevote,
                BftVoteValueV3::Block { block_id: id },
            ),
            3,
        )
        .unwrap();
    vote_effect(effects, BftVotePhaseV3::Precommit);
}

#[test]
fn header_journal_reserves_exact_decisions_and_nil_survives_replay() {
    let (schedule, keys) = setup(&[1]);
    let local = keys[0].0.clone();
    let public = hex::encode(keys[0].1.verifying_key().to_bytes());
    let empty = BftSigningJournalV3::for_schedule(&schedule, &local, &public).unwrap();
    let mut started = empty.clone();
    started.height_started_at_ms = Some(0);
    verify_bft_signing_journal_transition_v3(
        &empty,
        &started,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    let mut timed = started.clone();
    timed
        .events
        .push(BftSigningJournalEventV3::Timeout { now_ms: 100 });
    verify_bft_signing_journal_transition_v3(
        &started,
        &timed,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    let (_, effects) = replay_bft_signing_journal_with_effects_v3(
        &timed,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    let intent = vote_effect(effects, BftVotePhaseV3::Prevote);
    assert_eq!(intent.value, BftVoteValueV3::Nil);
    let mut reserved = timed.clone();
    reserved.pending_vote_intent = Some(intent.clone());
    verify_bft_signing_journal_transition_v3(
        &timed,
        &reserved,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    let raw = serde_cbor::to_vec(&reserved).unwrap();
    let reopened: BftSigningJournalV3 = serde_cbor::from_slice(&raw).unwrap();
    assert_eq!(reopened.digest().unwrap(), reserved.digest().unwrap());
    assert!(
        verify_bft_signing_journal_transition_v3(
            &reopened,
            &timed,
            schedule.active_set(),
            &schedule,
            TIMEOUTS
        )
        .is_err(),
        "reservation cannot be erased after restart"
    );
    let mut conflict = reserved.clone();
    conflict.pending_vote_intent.as_mut().unwrap().value = BftVoteValueV3::Block {
        block_id: fixtures::h(99),
    };
    assert!(
        verify_bft_signing_journal_transition_v3(
            &reserved,
            &conflict,
            schedule.active_set(),
            &schedule,
            TIMEOUTS
        )
        .is_err()
    );
    let vote = intent.sign(schedule.active_set(), &keys[0].1).unwrap();
    let completed = complete_reserved_bft_vote_v3(
        &reopened,
        vote,
        101,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    replay_bft_signing_journal_v3(&completed, schedule.active_set(), &schedule, TIMEOUTS).unwrap();
    let mut old = completed;
    old.schema_version = 2;
    assert!(
        replay_bft_signing_journal_v3(&old, schedule.active_set(), &schedule, TIMEOUTS).is_err()
    );
}

#[test]
fn header_journal_proposal_and_commit_resume_original_public_decisions() {
    let (schedule, keys) = setup(&[1]);
    let local = keys[0].0.clone();
    let signer = &keys[0].1;
    let mut journal = BftSigningJournalV3::for_schedule(
        &schedule,
        &local,
        hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    journal.height_started_at_ms = Some(0);
    let machine =
        replay_bft_signing_journal_v3(&journal, schedule.active_set(), &schedule, TIMEOUTS)
            .unwrap();
    let intent = machine.proposal_intent(value(&schedule), &local).unwrap();
    let mut reserved = journal.clone();
    reserved.pending_proposal_intent = Some(intent.clone());
    verify_bft_signing_journal_transition_v3(
        &journal,
        &reserved,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    assert!(
        verify_bft_signing_journal_transition_v3(
            &reserved,
            &journal,
            schedule.active_set(),
            &schedule,
            TIMEOUTS
        )
        .is_err()
    );
    let proposal = machine.sign_proposal_intent(&intent, signer).unwrap();
    journal = complete_reserved_bft_proposal_v3(
        &reserved,
        proposal,
        1,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    for (phase, time) in [(BftVotePhaseV3::Prevote, 2), (BftVotePhaseV3::Precommit, 3)] {
        let (_, effects) = replay_bft_signing_journal_with_effects_v3(
            &journal,
            schedule.active_set(),
            &schedule,
            TIMEOUTS,
        )
        .unwrap();
        let intent = vote_effect(effects, phase);
        let mut next = journal.clone();
        next.pending_vote_intent = Some(intent.clone());
        verify_bft_signing_journal_transition_v3(
            &journal,
            &next,
            schedule.active_set(),
            &schedule,
            TIMEOUTS,
        )
        .unwrap();
        journal = complete_reserved_bft_vote_v3(
            &next,
            intent.sign(schedule.active_set(), signer).unwrap(),
            time,
            schedule.active_set(),
            &schedule,
            TIMEOUTS,
        )
        .unwrap();
    }
    let (_, effects) = replay_bft_signing_journal_with_effects_v3(
        &journal,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    let qc = effects
        .into_iter()
        .find_map(|effect| match effect {
            BftConsensusActionV3::Commit(qc) => Some(qc),
            _ => None,
        })
        .unwrap();
    let mut reserved = journal.clone();
    reserved.pending_commit = Some(qc.clone());
    verify_bft_signing_journal_transition_v3(
        &journal,
        &reserved,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    let successor = verify_reserved_bft_commit_transition_v3(
        &reserved,
        &qc,
        schedule.active_set(),
        &schedule,
        TIMEOUTS,
    )
    .unwrap();
    let next = BftSigningJournalV3::for_schedule(
        &successor,
        &local,
        hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    assert_eq!(
        verify_bft_signing_journal_transition_v3(
            &reserved,
            &next,
            schedule.active_set(),
            &schedule,
            TIMEOUTS
        )
        .unwrap(),
        successor
    );
    assert_eq!(next.height, 4);
    let json = serde_json::to_string(&reserved).unwrap();
    assert!(!json.contains("\"actions_cbor\":"));
    assert!(!json.contains("\"payload_cbor\":"));
}
