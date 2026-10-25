use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::{
    BftConsensusMachineV1, BftHeightProposerScheduleV1, BftProposerPriorityV1, BftTimeoutsV1,
    BftValidatorSetV1, BftValidatorV1, BftVoteIntentV1, VerifiedBftCommitCertificateV1,
    verify_commit_certificate_token_v1,
};
use oasis7_proto::distributed_bft::{
    BftBlockV1, BftCommitCertificateV1, BftScopeV1, BftVotePhaseV1, BftVoteV1, BftVoteValueV1,
};

use super::{
    BFT_WAL_FILE, BftWalEventV1, BftWalStateV1, BftWalStore, read_bounded_file_with_limit,
    read_bounded_reader,
};

static NEXT_TEMP_DIR: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let id = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "oasis7-bft-wal-{label}-{}-{id}",
        std::process::id()
    ))
}

fn fixture() -> (
    BftHeightProposerScheduleV1,
    BftConsensusMachineV1,
    BftBlockV1,
    SigningKey,
    String,
) {
    let keys = (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let scope = BftScopeV1 {
        world_id: "wal-test-world".into(),
        chain_id: "wal-test-chain".into(),
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
    let schedule = BftHeightProposerScheduleV1::genesis(&set, 1).expect("genesis schedule");
    let proposer = BftProposerPriorityV1::new(&set)
        .expect("priority")
        .next_proposer(&set)
        .expect("proposer");
    let key_index = proposer.validator_id[1..]
        .parse::<usize>()
        .expect("validator index");
    let proposer_id = proposer.validator_id.clone();
    let machine = BftConsensusMachineV1::new(
        set,
        proposer_id.clone(),
        1,
        Some(schedule.starting_priority().clone()),
        0,
        BftTimeoutsV1 {
            propose_ms: 100,
            prevote_ms: 100,
            precommit_ms: 100,
            round_delta_ms: 25,
        },
    )
    .expect("machine");
    let block = BftBlockV1 {
        scope,
        height: 1,
        parent_commit_ref: "11".repeat(32),
        authority_epoch: 1,
        manifest_hash: "22".repeat(32),
        actions_cbor: vec![0x80],
        action_root: "33".repeat(32),
        event_root: "44".repeat(32),
        execution_block_hash: "55".repeat(32),
        execution_state_root: "66".repeat(32),
        state_root: "77".repeat(32),
    };
    (
        schedule,
        machine,
        block,
        keys[key_index].clone(),
        proposer_id,
    )
}

fn verified_commit(
    schedule: &BftHeightProposerScheduleV1,
    machine: &BftConsensusMachineV1,
    block: BftBlockV1,
    keys: &[SigningKey],
    proposer_id: &str,
) -> VerifiedBftCommitCertificateV1 {
    let intent = machine
        .proposal_intent(block, proposer_id)
        .expect("proposal intent");
    let proposer_index = proposer_id[1..]
        .parse::<usize>()
        .expect("proposer validator index");
    let proposal = machine
        .sign_proposal_intent(&intent, &keys[proposer_index])
        .expect("signed proposal");
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
            .expect("signed precommit")
        })
        .collect();
    let certificate = BftCommitCertificateV1 {
        proposal: Box::new(proposal),
        precommits,
    };
    verify_commit_certificate_token_v1(machine.set(), schedule, &certificate)
        .expect("verified commit")
}

#[test]
fn proposal_signing_requires_a_durable_exact_intent_and_restart_reuses_it() {
    let (schedule, machine, block, key, proposer_id) = fixture();
    let intent = machine
        .proposal_intent(block, proposer_id.as_str())
        .expect("unsigned proposal intent");
    let root = temp_dir("proposal-intent");
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let mut wal = BftWalStore::open(
        root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("open WAL");
    assert_eq!(
        wal.ensure_height_started_at(100).expect("height origin"),
        100
    );
    assert!(
        wal.append_event(BftWalEventV1::Proposal {
            proposal: Box::new(
                machine
                    .sign_proposal_intent(&intent, &key)
                    .expect("proposal"),
            ),
            local: true,
            now_ms: 1,
        })
        .is_err()
    );
    wal.reserve_proposal_intent(intent.clone())
        .expect("persist unsigned intent");
    assert_eq!(wal.state().pending_proposal_intent.as_ref(), Some(&intent));
    assert!(
        wal.reserve_proposal_intent({
            let mut conflicting = intent.clone();
            conflicting.block.state_root = "99".repeat(32);
            conflicting
        })
        .is_err()
    );
    drop(wal);

    let mut reopened = BftWalStore::open(
        root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("reopen WAL");
    let restored = reopened
        .state()
        .pending_proposal_intent
        .as_ref()
        .expect("durable pending proposal")
        .clone();
    assert_eq!(restored, intent);
    let signed = machine
        .sign_proposal_intent(&restored, &key)
        .expect("sign exact recovered intent");
    reopened
        .append_local_proposal(&restored, signed.clone(), 101)
        .expect("persist signed proposal before publication");
    assert!(reopened.state().pending_proposal_intent.is_none());
    assert!(matches!(
        reopened.state().events.as_slice(),
        [BftWalEventV1::Proposal {
            proposal,
            local: true,
            now_ms: 101
        }] if proposal.as_ref() == &signed
    ));
    drop(reopened);

    let mut reopened = BftWalStore::open(
        root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("final reopen");
    assert_eq!(reopened.state().events.len(), 1);
    assert!(reopened.state().pending_proposal_intent.is_none());
    let mut conflicting_intent = intent.clone();
    conflicting_intent.block.state_root = "99".repeat(32);
    assert!(
        reopened
            .reserve_proposal_intent(conflicting_intent)
            .is_err()
    );
    assert!(reopened.reserve_proposal_intent(intent.clone()).is_ok());
    let exact_retry = machine
        .sign_proposal_intent(&intent, &key)
        .expect("repeat deterministic signature");
    reopened
        .append_local_proposal(&intent, exact_retry, 102)
        .expect("exact completed proposal retry is idempotent");
    assert_eq!(reopened.state().events.len(), 1);
    drop(reopened);

    let other_id = machine
        .set()
        .validators()
        .keys()
        .find(|id| *id != &proposer_id)
        .expect("another validator")
        .clone();
    let other_key = (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .find(|candidate| {
            hex::encode(candidate.verifying_key().to_bytes())
                == machine
                    .set()
                    .validator(other_id.as_str())
                    .expect("other member")
                    .public_key_hex
        })
        .expect("other validator key");
    assert!(
        BftWalStore::open(
            root.as_path(),
            &schedule,
            other_id.as_str(),
            hex::encode(other_key.verifying_key().to_bytes()).as_str(),
            false,
        )
        .is_err()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn completed_vote_slot_survives_reopen_and_rejects_conflict_or_bad_signature() {
    let (schedule, machine, _block, key, validator_id) = fixture();
    let vote_intent = machine
        .vote_intent(
            validator_id.clone(),
            BftVotePhaseV1::Prevote,
            BftVoteValueV1::Nil,
            0,
        )
        .expect("local vote intent")
        .expect("unused local slot");
    let root = temp_dir("vote-slot");
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let mut wal = BftWalStore::open(
        root.as_path(),
        &schedule,
        validator_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("open WAL");
    wal.ensure_height_started_at(0).expect("height origin");
    wal.reserve_vote_intent(vote_intent.clone())
        .expect("persist vote intent");
    let mut invalid = vote_intent.sign(machine.set(), &key).expect("signed vote");
    invalid.signature_hex = "00".repeat(64);
    assert!(wal.append_local_vote(&vote_intent, invalid, 1).is_err());
    let vote = vote_intent
        .sign(machine.set(), &key)
        .expect("valid signed vote");
    wal.append_local_vote(&vote_intent, vote.clone(), 1)
        .expect("persist exact signature");
    drop(wal);

    let mut reopened = BftWalStore::open(
        root.as_path(),
        &schedule,
        validator_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("reopen WAL");
    let conflicting = BftVoteIntentV1 {
        value: BftVoteValueV1::Block {
            block_id: "88".repeat(32),
        },
        ..vote_intent.clone()
    };
    assert!(reopened.reserve_vote_intent(conflicting).is_err());
    reopened
        .reserve_vote_intent(vote_intent.clone())
        .expect("exact slot retry");
    reopened
        .append_local_vote(&vote_intent, vote, 2)
        .expect("exact completed vote retry");
    assert_eq!(reopened.state().events.len(), 1);
    drop(reopened);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bounded_wal_reader_checks_bytes_actually_read_not_only_initial_capacity() {
    let raced = std::io::Cursor::new([1, 2, 3, 4]);
    assert!(read_bounded_reader(raced, "test WAL", 3, 0).is_err());

    let root = temp_dir("bounded-read");
    fs::create_dir_all(root.as_path()).expect("temp directory");
    let path = root.join(BFT_WAL_FILE);
    fs::write(path.as_path(), [1, 2, 3, 4]).expect("write test payload");
    assert!(read_bounded_file_with_limit(path.as_path(), "test WAL", 3).is_err());
    assert_eq!(
        read_bounded_file_with_limit(path.as_path(), "test WAL", 4).expect("read within bound"),
        [1, 2, 3, 4]
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn recovery_opens_from_genesis_and_reconciles_only_the_exact_history_qc() {
    let (schedule, machine, block, _local_key, proposer_id) = fixture();
    let keys = (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let verified = verified_commit(
        &schedule,
        &machine,
        block,
        keys.as_slice(),
        proposer_id.as_str(),
    );
    let local_index = proposer_id[1..]
        .parse::<usize>()
        .expect("proposer validator index");
    let local_key = &keys[local_index];
    let public_key = hex::encode(local_key.verifying_key().to_bytes());
    let root = temp_dir("recovery-exact-qc");
    let mut wal = BftWalStore::open_for_recovery(
        root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("initialize genesis signer WAL");
    wal.ensure_height_started_at(123)
        .expect("persist original timer origin");
    wal.reserve_commit(&verified)
        .expect("persist exact pending QC before execution commit");
    drop(wal);

    let mut recovered = BftWalStore::open_for_recovery(
        root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        true,
    )
    .expect("read existing WAL from pinned genesis identity");
    assert_eq!(
        recovered
            .ensure_height_started_at(999)
            .expect("reuse origin"),
        123
    );
    assert!(recovered.validate_schedule(verified.schedule()).is_ok());
    recovered
        .complete_history_commit(&verified)
        .expect("advance only after complete history reverified exact pending QC");
    assert!(recovered.state().pending_commit.is_none());
    assert!(
        recovered
            .validate_schedule(verified.next_height_schedule())
            .is_ok()
    );
    drop(recovered);

    let reopened = BftWalStore::open_for_recovery(
        root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        true,
    )
    .expect("reopen recovered state without trusting tip as initial anchor");
    assert!(
        reopened
            .validate_schedule(verified.next_height_schedule())
            .is_ok()
    );
    assert!(reopened.state().pending_commit.is_none());
    drop(reopened);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn recovery_never_initializes_an_empty_wal_from_an_advanced_schedule() {
    let (schedule, machine, block, _local_key, proposer_id) = fixture();
    let keys = (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let verified = verified_commit(
        &schedule,
        &machine,
        block,
        keys.as_slice(),
        proposer_id.as_str(),
    );
    let local_index = proposer_id[1..]
        .parse::<usize>()
        .expect("proposer validator index");
    let local_key = &keys[local_index];
    let public_key = hex::encode(local_key.verifying_key().to_bytes());
    let root = temp_dir("recovery-no-advanced-init");
    assert!(
        BftWalStore::open_for_recovery(
            root.as_path(),
            verified.next_height_schedule(),
            proposer_id.as_str(),
            public_key.as_str(),
            false,
        )
        .is_err()
    );
    assert!(!root.join("tendermint-v1.anchor.cbor").exists());
    assert!(!root.join(BFT_WAL_FILE).exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn height_timer_origin_is_durable_before_slots_and_event_times_are_monotonic() {
    let (schedule, machine, block, key, proposer_id) = fixture();
    let intent = machine
        .proposal_intent(block, proposer_id.as_str())
        .expect("proposal intent");
    let root = temp_dir("height-timer-origin");
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let mut wal = BftWalStore::open_for_recovery(
        root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("open genesis WAL");
    assert!(wal.reserve_proposal_intent(intent.clone()).is_err());
    assert_eq!(
        wal.ensure_height_started_at(200).expect("persist origin"),
        200
    );
    assert_eq!(
        wal.ensure_height_started_at(500).expect("reuse origin"),
        200
    );
    assert!(
        wal.append_event(BftWalEventV1::Timeout { now_ms: 199 })
            .is_err()
    );
    wal.append_event(BftWalEventV1::Timeout { now_ms: 201 })
        .expect("append monotonic timeout");
    assert!(
        wal.append_event(BftWalEventV1::Timeout { now_ms: 200 })
            .is_err()
    );
    drop(wal);

    let reopened = BftWalStore::open_for_recovery(
        root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("reopen height state");
    assert_eq!(reopened.state().height_started_at_ms, Some(200));
    assert_eq!(reopened.state().events.len(), 1);
    drop(reopened);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pending_intents_are_bound_to_the_pinned_wal_height_and_parent() {
    let (schedule, machine, block, key, proposer_id) = fixture();
    let public_key = hex::encode(key.verifying_key().to_bytes());

    let proposal_root = temp_dir("pending-proposal-binding");
    let mut proposal_wal = BftWalStore::open_for_recovery(
        proposal_root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("open proposal WAL");
    proposal_wal
        .ensure_height_started_at(100)
        .expect("persist proposal height origin");
    let mut wrong_parent = machine
        .proposal_intent(block.clone(), proposer_id.as_str())
        .expect("proposal intent");
    wrong_parent.block.parent_commit_ref = "88".repeat(32);
    wrong_parent.header.block_id = wrong_parent.block.block_id().expect("changed block id");
    assert!(proposal_wal.reserve_proposal_intent(wrong_parent).is_err());

    let proposal_intent = machine
        .proposal_intent(block, proposer_id.as_str())
        .expect("valid proposal intent");
    proposal_wal
        .reserve_proposal_intent(proposal_intent)
        .expect("reserve valid proposal");
    drop(proposal_wal);

    let proposal_path = proposal_root.join(BFT_WAL_FILE);
    let bytes = fs::read(proposal_path.as_path()).expect("read proposal WAL fixture");
    let mut persisted: BftWalStateV1 =
        serde_cbor::from_slice(bytes.as_slice()).expect("decode proposal WAL fixture");
    let pending = persisted
        .pending_proposal_intent
        .as_mut()
        .expect("pending proposal intent");
    pending.block.parent_commit_ref = "99".repeat(32);
    pending.header.block_id = pending.block.block_id().expect("tampered block id");
    fs::write(
        proposal_path.as_path(),
        serde_cbor::to_vec(&persisted).expect("encode corrupted proposal WAL"),
    )
    .expect("write corrupted proposal WAL");
    assert!(
        BftWalStore::open_for_recovery(
            proposal_root.as_path(),
            &schedule,
            proposer_id.as_str(),
            public_key.as_str(),
            false,
        )
        .is_err()
    );
    let _ = fs::remove_dir_all(proposal_root);

    let vote_root = temp_dir("pending-vote-binding");
    let vote_intent = machine
        .vote_intent(
            proposer_id.clone(),
            BftVotePhaseV1::Prevote,
            BftVoteValueV1::Nil,
            0,
        )
        .expect("vote intent")
        .expect("unused vote slot");
    let mut vote_wal = BftWalStore::open_for_recovery(
        vote_root.as_path(),
        &schedule,
        proposer_id.as_str(),
        public_key.as_str(),
        false,
    )
    .expect("open vote WAL");
    vote_wal
        .ensure_height_started_at(100)
        .expect("persist vote height origin");
    let wrong_height = oasis7_consensus::tendermint_v1::BftVoteIntentV1 {
        height: schedule.height() + 1,
        ..vote_intent.clone()
    };
    assert!(vote_wal.reserve_vote_intent(wrong_height).is_err());
    vote_wal
        .reserve_vote_intent(vote_intent)
        .expect("reserve valid vote");
    drop(vote_wal);

    let vote_path = vote_root.join(BFT_WAL_FILE);
    let bytes = fs::read(vote_path.as_path()).expect("read vote WAL fixture");
    let mut persisted: BftWalStateV1 =
        serde_cbor::from_slice(bytes.as_slice()).expect("decode vote WAL fixture");
    persisted
        .pending_vote_intent
        .as_mut()
        .expect("pending vote intent")
        .height += 1;
    fs::write(
        vote_path.as_path(),
        serde_cbor::to_vec(&persisted).expect("encode corrupted vote WAL"),
    )
    .expect("write corrupted vote WAL");
    assert!(
        BftWalStore::open_for_recovery(
            vote_root.as_path(),
            &schedule,
            proposer_id.as_str(),
            public_key.as_str(),
            false,
        )
        .is_err()
    );
    let _ = fs::remove_dir_all(vote_root);
}
