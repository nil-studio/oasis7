use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::{BftBlockV1, BftScopeV1, BftVotePhaseV1, BftVoteValueV1};
use serde::Serialize;

use super::*;
use crate::tendermint_v1::{BftProposerPriorityV1, BftValidatorV1};

const TEST_TIMEOUTS: BftTimeoutsV1 = BftTimeoutsV1 {
    propose_ms: 100,
    prevote_ms: 100,
    precommit_ms: 100,
    round_delta_ms: 25,
};

fn fixture() -> (
    BftValidatorSetV1,
    BftHeightProposerScheduleV1,
    SigningKey,
    String,
    BftBlockV1,
) {
    let keys = (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let scope = BftScopeV1 {
        world_id: "signing-journal-world".into(),
        chain_id: "signing-journal-chain".into(),
        genesis_hash: "11".repeat(32),
    };
    let set = BftValidatorSetV1::new(
        scope.clone(),
        keys.iter()
            .enumerate()
            .map(|(index, key)| BftValidatorV1 {
                validator_id: format!("v{index}"),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                voting_power: 1,
            })
            .collect(),
    )
    .expect("valid set");
    let schedule = BftHeightProposerScheduleV1::genesis(&set, 3).expect("genesis schedule");
    let mut priority = BftProposerPriorityV1::new(&set).expect("priority");
    let proposer_id = priority.next_proposer(&set).expect("proposer").validator_id;
    let key_index = proposer_id[1..].parse::<usize>().expect("key index");
    let block = BftBlockV1 {
        scope,
        height: 1,
        parent_commit_ref: "11".repeat(32),
        authority_epoch: 3,
        manifest_hash: "22".repeat(32),
        actions_cbor: vec![0x80],
        action_root: "33".repeat(32),
        event_root: "44".repeat(32),
        execution_block_hash: "55".repeat(32),
        execution_state_root: "66".repeat(32),
        state_root: "77".repeat(32),
    };
    (set, schedule, keys[key_index].clone(), proposer_id, block)
}

fn empty_journal(
    schedule: &BftHeightProposerScheduleV1,
    local_id: &str,
    key: &SigningKey,
) -> BftSigningJournalV1 {
    BftSigningJournalV1::for_schedule(
        schedule,
        local_id,
        hex::encode(key.verifying_key().to_bytes()),
    )
}

fn start_height(
    journal: &BftSigningJournalV1,
    set: &BftValidatorSetV1,
    schedule: &BftHeightProposerScheduleV1,
) -> BftSigningJournalV1 {
    let mut started = journal.clone();
    started.height_started_at_ms = Some(100);
    verify_bft_signing_journal_transition_v1(journal, &started, set, schedule, TEST_TIMEOUTS)
        .expect("first timer transition");
    started
}

#[test]
fn journal_schema_four_encoding_matches_the_previous_node_shape() {
    #[derive(Serialize)]
    struct PreviousState {
        schema_version: u32,
        scope: BftScopeV1,
        authority_epoch: u64,
        validator_set_hash: String,
        local_validator_id: String,
        local_signing_public_key: String,
        height: u64,
        parent_commit_ref: String,
        starting_priority: BTreeMap<String, i128>,
        height_started_at_ms: Option<i64>,
        events: Vec<PreviousEvent>,
        pending_proposal_intent: Option<BftProposalIntentV1>,
        pending_vote_intent: Option<BftVoteIntentV1>,
        pending_commit: Option<oasis7_proto::distributed_bft::BftCommitCertificateV1>,
    }

    #[derive(Serialize)]
    #[serde(
        tag = "kind",
        content = "data",
        rename_all = "snake_case",
        deny_unknown_fields
    )]
    enum PreviousEvent {
        Vote {
            vote: Box<oasis7_proto::distributed_bft::BftVoteV1>,
            local: bool,
            now_ms: i64,
        },
        Timeout {
            now_ms: i64,
        },
    }

    let (set, schedule, key, local_id, _) = fixture();
    let mut journal = empty_journal(&schedule, local_id.as_str(), &key);
    journal.height_started_at_ms = Some(101);
    journal
        .events
        .push(BftSigningJournalEventV1::Timeout { now_ms: 202 });
    journal.events.push(BftSigningJournalEventV1::Vote {
        vote: Box::new(
            oasis7_proto::distributed_bft::BftVoteV1::sign(
                schedule.scope().clone(),
                1,
                0,
                BftVotePhaseV1::Prevote,
                set.set_hash().to_string(),
                BftVoteValueV1::Nil,
                "v1".into(),
                &SigningKey::from_bytes(&[2; 32]),
            )
            .expect("fixture signed vote"),
        ),
        local: false,
        now_ms: 303,
    });
    let previous = PreviousState {
        schema_version: 4,
        scope: journal.scope.clone(),
        authority_epoch: journal.authority_epoch,
        validator_set_hash: journal.validator_set_hash.clone(),
        local_validator_id: journal.local_validator_id.clone(),
        local_signing_public_key: journal.local_signing_public_key.clone(),
        height: journal.height,
        parent_commit_ref: journal.parent_commit_ref.clone(),
        starting_priority: journal.starting_priority.clone(),
        height_started_at_ms: journal.height_started_at_ms,
        events: vec![
            PreviousEvent::Timeout { now_ms: 202 },
            match &journal.events[1] {
                BftSigningJournalEventV1::Vote {
                    vote,
                    local,
                    now_ms,
                } => PreviousEvent::Vote {
                    vote: vote.clone(),
                    local: *local,
                    now_ms: *now_ms,
                },
                _ => unreachable!(),
            },
        ],
        pending_proposal_intent: None,
        pending_vote_intent: None,
        pending_commit: None,
    };
    assert_eq!(
        serde_cbor::to_vec(&journal).expect("new schema bytes"),
        serde_cbor::to_vec(&previous).expect("old schema bytes"),
    );
}

#[test]
fn only_exact_timer_and_reserved_signed_proposal_transitions_replay() {
    let (set, schedule, key, local_id, block) = fixture();
    let journal = empty_journal(&schedule, local_id.as_str(), &key);
    assert_eq!(
        replay_bft_signing_journal_v1(&journal, &set, &schedule, TEST_TIMEOUTS)
            .expect("empty journal replay")
            .height(),
        1
    );
    let started = start_height(&journal, &set, &schedule);
    let mut changed_timer = started.clone();
    changed_timer.height_started_at_ms = Some(101);
    assert!(
        verify_bft_signing_journal_transition_v1(
            &started,
            &changed_timer,
            &set,
            &schedule,
            TEST_TIMEOUTS,
        )
        .is_err()
    );

    let machine = BftConsensusMachineV1::new(
        set.clone(),
        local_id.clone(),
        schedule.height(),
        Some(schedule.starting_priority().clone()),
        100,
        TEST_TIMEOUTS,
    )
    .expect("machine");
    let intent = machine
        .proposal_intent(block, local_id.as_str())
        .expect("proposal decision");
    let mut reserved = started.clone();
    reserved.pending_proposal_intent = Some(intent.clone());
    verify_bft_signing_journal_transition_v1(&started, &reserved, &set, &schedule, TEST_TIMEOUTS)
        .expect("exact reservation");

    let mut wrong = reserved.clone();
    wrong
        .pending_proposal_intent
        .as_mut()
        .unwrap()
        .block
        .state_root = "99".repeat(32);
    assert!(replay_bft_signing_journal_v1(&wrong, &set, &schedule, TEST_TIMEOUTS).is_err());

    let signed = machine
        .sign_proposal_intent(&intent, &key)
        .expect("sign reserved proposal");
    let completed =
        complete_reserved_bft_proposal_v1(&reserved, signed, 101, &set, &schedule, TEST_TIMEOUTS)
            .expect("exact proposal completion");
    assert!(completed.pending_proposal_intent().is_none());
    assert_eq!(completed.events().len(), 1);
    replay_bft_signing_journal_v1(&completed, &set, &schedule, TEST_TIMEOUTS)
        .expect("completed journal replay");

    let mut duplicate_event = completed.clone();
    duplicate_event.events.push(completed.events[0].clone());
    assert!(
        replay_bft_signing_journal_v1(&duplicate_event, &set, &schedule, TEST_TIMEOUTS).is_err()
    );

    let mut deleted = completed.clone();
    deleted.events.clear();
    assert!(
        verify_bft_signing_journal_transition_v1(
            &completed,
            &deleted,
            &set,
            &schedule,
            TEST_TIMEOUTS,
        )
        .is_err()
    );
}

#[test]
fn vote_intent_requires_a_replayed_timeout_effect_and_signed_completion() {
    let (set, schedule, key, local_id, _) = fixture();
    let journal = empty_journal(&schedule, local_id.as_str(), &key);
    let started = start_height(&journal, &set, &schedule);
    let mut timed_out = started.clone();
    timed_out
        .events
        .push(BftSigningJournalEventV1::Timeout { now_ms: 199 });
    assert!(
        verify_bft_signing_journal_transition_v1(
            &started,
            &timed_out,
            &set,
            &schedule,
            TEST_TIMEOUTS,
        )
        .is_err()
    );

    timed_out.events[0] = BftSigningJournalEventV1::Timeout { now_ms: 200 };
    verify_bft_signing_journal_transition_v1(&started, &timed_out, &set, &schedule, TEST_TIMEOUTS)
        .expect("deadline timeout transition");
    let machine = replay_bft_signing_journal_v1(&timed_out, &set, &schedule, TEST_TIMEOUTS)
        .expect("timed out replay");
    let intent = machine
        .vote_intent(
            local_id.clone(),
            BftVotePhaseV1::Prevote,
            BftVoteValueV1::Nil,
            machine.round(),
        )
        .expect("vote intent query")
        .expect("timeout emits prevote intent");
    let mut reserved = timed_out.clone();
    reserved.pending_vote_intent = Some(intent.clone());
    verify_bft_signing_journal_transition_v1(&timed_out, &reserved, &set, &schedule, TEST_TIMEOUTS)
        .expect("exact vote reservation");
    let vote = intent.sign(&set, &key).expect("signed vote");
    let completed =
        complete_reserved_bft_vote_v1(&reserved, vote, 201, &set, &schedule, TEST_TIMEOUTS)
            .expect("exact vote completion");
    assert!(completed.pending_vote_intent().is_none());
    replay_bft_signing_journal_v1(&completed, &set, &schedule, TEST_TIMEOUTS)
        .expect("completed vote replay");
}

#[test]
fn verified_qc_is_the_only_transition_that_advances_the_height_schedule() {
    let (set, schedule, _local_key, proposer_id, block) = fixture();
    let keys = (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let local_key = keys[proposer_id[1..].parse::<usize>().expect("proposer index")].clone();
    let journal = empty_journal(&schedule, proposer_id.as_str(), &local_key);
    let started = start_height(&journal, &set, &schedule);
    let machine = BftConsensusMachineV1::new(
        set.clone(),
        proposer_id.clone(),
        schedule.height(),
        Some(schedule.starting_priority().clone()),
        100,
        TEST_TIMEOUTS,
    )
    .expect("machine");
    let intent = machine
        .proposal_intent(block, proposer_id.as_str())
        .expect("proposal intent");
    let proposal = machine
        .sign_proposal_intent(&intent, &local_key)
        .expect("proposal signature");
    let block_id = proposal.block.block_id().expect("block id");
    let precommits = keys
        .iter()
        .enumerate()
        .take(3)
        .map(|(index, key)| {
            BftVoteV1::sign(
                schedule.scope().clone(),
                schedule.height(),
                proposal.header.round,
                BftVotePhaseV1::Precommit,
                schedule.validator_set_hash().to_string(),
                BftVoteValueV1::Block {
                    block_id: block_id.clone(),
                },
                format!("v{index}"),
                key,
            )
            .expect("precommit")
        })
        .collect();
    let certificate = BftCommitCertificateV1 {
        proposal: Box::new(proposal),
        precommits,
    };
    let mut reserved = started.clone();
    reserved.pending_commit = Some(certificate.clone());
    assert_eq!(
        verify_bft_signing_journal_transition_v1(
            &started,
            &reserved,
            &set,
            &schedule,
            TEST_TIMEOUTS,
        )
        .expect("valid QC reservation"),
        schedule,
    );

    let token =
        verify_commit_certificate_token_v1(&set, &schedule, &certificate).expect("verified QC");
    let next = BftSigningJournalV1::for_schedule(
        token.next_height_schedule(),
        proposer_id,
        hex::encode(local_key.verifying_key().to_bytes()),
    );
    let advanced =
        verify_bft_signing_journal_transition_v1(&reserved, &next, &set, &schedule, TEST_TIMEOUTS)
            .expect("exact QC height completion");
    assert_eq!(advanced.height(), 2);
    assert_eq!(advanced.parent_commit_ref(), token.block_id());

    let mut forged = next;
    forged.parent_commit_ref = "99".repeat(32);
    assert!(
        verify_bft_signing_journal_transition_v1(
            &reserved,
            &forged,
            &set,
            &schedule,
            TEST_TIMEOUTS,
        )
        .is_err()
    );
}

#[test]
fn journal_digest_is_domain_separated_and_bounded_state_is_checked() {
    let (set, schedule, key, local_id, _) = fixture();
    let journal = empty_journal(&schedule, local_id.as_str(), &key);
    let first = journal.digest().expect("digest");
    assert_eq!(first.len(), 64);
    assert_eq!(
        journal.encoded_len().expect("bounded encoding"),
        serde_cbor::to_vec(&journal).unwrap().len()
    );

    let mut changed = journal.clone();
    changed.local_validator_id = "v2".into();
    assert_ne!(first, changed.digest().expect("changed digest"));
    assert!(replay_bft_signing_journal_v1(&changed, &set, &schedule, TEST_TIMEOUTS).is_err());
}
