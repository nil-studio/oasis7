use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::v2::{
    BftBlockV2, BftProposalHeaderV2, BftProposalV2, BftScopeV2, BftValidatorV2, BftVotePhaseV2,
    BftVoteV2, BftVoteValueV2,
};

use super::*;
use crate::tendermint_v1::BftTimeoutsV1;
use crate::tendermint_v2::BftSchedulePolicyV2;

const TIMEOUTS: BftTimeoutsV1 = BftTimeoutsV1 {
    propose_ms: 100,
    prevote_ms: 100,
    precommit_ms: 100,
    round_delta_ms: 25,
};

#[test]
fn v2_journal_cbor_round_trip_preserves_proposer_priorities() {
    // The provider carries the journal inside an adjacently tagged command.
    // Serde's buffered enum deserializer differs from a bare CBOR struct.
    #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    #[serde(tag = "kind", content = "body", deny_unknown_fields)]
    enum Command {
        Advance { journal: BftSigningJournalV2 },
    }
    let fixture = Fixture::new();
    let (id, key) = &fixture.keys[0];
    let journal = fixture.journal(id, key);
    let command = Command::Advance {
        journal: journal.clone(),
    };
    let bytes = oasis7_cas::to_canonical_cbor(&command)
        .expect("encode actual V2 journal with the remote provider codec");
    let restored: Command =
        serde_cbor::from_slice(&bytes).expect("decode tagged V2 command at remote boundary");
    assert_eq!(restored, command);
    let Command::Advance { journal: restored } = restored;
    assert_eq!(restored, journal);
    replay_bft_signing_journal_v2(&restored, &fixture.set, &fixture.schedule, TIMEOUTS)
        .expect("decoded journal restores actual consensus machine");
}

struct Fixture {
    schedule: BftHeightProposerScheduleV2,
    set: BftValidatorSetV2,
    keys: Vec<(String, SigningKey)>,
}

impl Fixture {
    fn new() -> Self {
        let scope = BftScopeV2 {
            world_id: "v2-signing-journal-world".into(),
            chain_id: "v2-signing-journal-chain".into(),
            genesis_hash: "11".repeat(32),
        };
        let keys = (1..=4)
            .map(|seed| (format!("v{seed}"), SigningKey::from_bytes(&[seed; 32])))
            .collect::<Vec<_>>();
        let validators = keys
            .iter()
            .map(|(validator_id, key)| BftValidatorV2 {
                validator_id: validator_id.clone(),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                voting_power: 1,
            })
            .collect();
        let set = BftValidatorSetV2::new(scope, validators).expect("valid V2 validator set");
        let policy = BftSchedulePolicyV2::new(8, "controlled-bft-v2").expect("policy");
        let schedule = BftHeightProposerScheduleV2::genesis(
            set.clone(),
            policy,
            7,
            "22".repeat(32),
            "33".repeat(32),
        )
        .expect("genesis schedule");
        Self {
            schedule,
            set,
            keys,
        }
    }

    fn journal(&self, local_id: &str, key: &SigningKey) -> BftSigningJournalV2 {
        BftSigningJournalV2::for_schedule(
            &self.schedule,
            local_id,
            hex::encode(key.verifying_key().to_bytes()),
        )
        .expect("journal matches schedule")
    }

    fn block(&self) -> BftBlockV2 {
        BftBlockV2 {
            protocol_version: oasis7_proto::distributed_bft::v2::BFT_PROTOCOL_VERSION_V2,
            scope: self.schedule.scope().clone(),
            height: self.schedule.height(),
            parent_commit_ref: self.schedule.parent_commit_ref().to_string(),
            authority_epoch: self.schedule.authority_epoch(),
            active_validator_set_hash: self.set.set_hash().to_string(),
            schedule_policy_hash: self.schedule.policy_hash().to_string(),
            manifest_hash: self.schedule.parent_manifest_hash().to_string(),
            post_manifest_hash: self.schedule.parent_manifest_hash().to_string(),
            actions_cbor: vec![0x80],
            action_root: "44".repeat(32),
            event_root: "55".repeat(32),
            execution_block_hash: "66".repeat(32),
            execution_state_root: "77".repeat(32),
            state_root: "88".repeat(32),
            transition: None,
            transition_ref: None,
        }
    }

    fn certificate(&self) -> BftCommitCertificateV2 {
        let proposer_id = self
            .schedule
            .expected_proposer_for_round(0)
            .expect("round zero proposer");
        let proposer_key = self
            .keys
            .iter()
            .find(|(id, _)| id == &proposer_id)
            .expect("proposer key");
        let block = self.block();
        let header = BftProposalHeaderV2::sign(
            block.scope.clone(),
            block.height,
            0,
            self.set.set_hash().to_string(),
            self.schedule.policy_hash().to_string(),
            proposer_id,
            block.block_id().expect("block id"),
            None,
            &proposer_key.1,
        )
        .expect("signed header");
        let proposal = BftProposalV2 {
            header,
            block,
            valid_round_evidence: None,
        };
        let value = BftVoteValueV2::Block {
            block_id: proposal.header.block_id.clone(),
        };
        let precommits = self
            .keys
            .iter()
            .take(3)
            .map(|(validator_id, key)| {
                BftVoteV2::sign(
                    self.schedule.scope().clone(),
                    self.schedule.height(),
                    0,
                    BftVotePhaseV2::Precommit,
                    self.set.set_hash().to_string(),
                    self.schedule.policy_hash().to_string(),
                    value.clone(),
                    validator_id.clone(),
                    key,
                )
                .expect("signed precommit")
            })
            .collect();
        BftCommitCertificateV2 {
            proposal: Box::new(proposal),
            precommits,
        }
    }

    fn remote_proposal(&self, marker: u8, round: u64) -> BftProposalV2 {
        let mut block = self.block();
        block.actions_cbor = vec![0x80, marker];
        block.action_root = format!("{marker:02x}").repeat(32);
        let proposer_id = self
            .schedule
            .expected_proposer_for_round(round)
            .expect("deterministic round proposer");
        let proposer_key = self
            .keys
            .iter()
            .find(|(id, _)| id == &proposer_id)
            .expect("proposer key");
        let header = BftProposalHeaderV2::sign(
            block.scope.clone(),
            block.height,
            round,
            self.set.set_hash().to_string(),
            self.schedule.policy_hash().to_string(),
            proposer_id,
            block.block_id().expect("block id"),
            None,
            &proposer_key.1,
        )
        .expect("signed proposal header");
        BftProposalV2 {
            header,
            block,
            valid_round_evidence: None,
        }
    }
}

#[test]
fn journal_retains_only_authenticated_proposal_equivocation_evidence() {
    let fixture = Fixture::new();
    let first = fixture.remote_proposal(1, 0);
    let conflicting = fixture.remote_proposal(2, 0);
    let inactive_key = SigningKey::from_bytes(&[99; 32]);
    let mut started = fixture.journal("observer", &inactive_key);
    started.height_started_at_ms = Some(100);
    let mut accepted = started.clone();
    accepted.events.push(BftSigningJournalEventV2::Proposal {
        proposal: Box::new(first.clone()),
        local: false,
        now_ms: 101,
    });
    verify_bft_signing_journal_transition_v2(
        &started,
        &accepted,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("the first signed proposal is a legal remote event");

    let mut equivocation = accepted.clone();
    equivocation
        .events
        .push(BftSigningJournalEventV2::ProposalEquivocation {
            first: Box::new(first.header.clone()),
            second: Box::new(conflicting.header.clone()),
            now_ms: 102,
        });
    verify_bft_signing_journal_transition_v2(
        &accepted,
        &equivocation,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("the authenticated pair appends as evidence without accepting a second proposal");
    let restored =
        replay_bft_signing_journal_v2(&equivocation, &fixture.set, &fixture.schedule, TIMEOUTS)
            .expect("durable equivocation evidence replays");
    assert_eq!(restored.proposal_equivocation_evidence().count(), 1);
    assert_eq!(
        restored.proposal_for_round(0).unwrap().header,
        first.header,
        "the conflicting proposal must not replace accepted proposal state"
    );

    let mut bad_signature = equivocation.clone();
    if let BftSigningJournalEventV2::ProposalEquivocation { second, .. } =
        bad_signature.events.last_mut().unwrap()
    {
        second.signature_hex = "00".into();
    }
    assert!(
        replay_bft_signing_journal_v2(&bad_signature, &fixture.set, &fixture.schedule, TIMEOUTS,)
            .is_err(),
        "a durable evidence event cannot smuggle an unsigned header"
    );

    let mut unanchored = started.clone();
    unanchored
        .events
        .push(BftSigningJournalEventV2::ProposalEquivocation {
            first: Box::new(first.header.clone()),
            second: Box::new(conflicting.header.clone()),
            now_ms: 101,
        });
    assert!(
        replay_bft_signing_journal_v2(&unanchored, &fixture.set, &fixture.schedule, TIMEOUTS)
            .is_err()
    );
}

#[test]
fn journal_equivocation_evidence_preserves_pending_vote_across_reopen() {
    let fixture = Fixture::new();
    let proposer_id = fixture
        .schedule
        .expected_proposer_for_round(0)
        .expect("proposer");
    let (local_id, local_key) = fixture
        .keys
        .iter()
        .find(|(id, _)| id != &proposer_id)
        .expect("a non-proposer active validator");
    let first = fixture.remote_proposal(1, 0);
    let conflicting = fixture.remote_proposal(2, 0);
    let mut started = fixture.journal(local_id, local_key);
    started.height_started_at_ms = Some(100);

    let mut accepted = started.clone();
    accepted.events.push(BftSigningJournalEventV2::Proposal {
        proposal: Box::new(first.clone()),
        local: false,
        now_ms: 101,
    });
    let machine =
        replay_bft_signing_journal_v2(&accepted, &fixture.set, &fixture.schedule, TIMEOUTS)
            .expect("the first proposal creates the deterministic prevote intent");
    let pending_vote = machine
        .vote_intent(
            local_id.clone(),
            BftVotePhaseV2::Prevote,
            BftVoteValueV2::Block {
                block_id: first.header.block_id.clone(),
            },
            0,
        )
        .expect("derive local prevote intent")
        .expect("local active validator can vote");
    verify_bft_signing_journal_transition_v2(
        &started,
        &accepted,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("persist the first signed proposal");

    let mut reserved = accepted.clone();
    reserved.pending_vote_intent = Some(pending_vote.clone());
    verify_bft_signing_journal_transition_v2(
        &accepted,
        &reserved,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("reserve the exact prevote intent emitted by replay");

    let mut with_evidence = reserved.clone();
    with_evidence
        .events
        .push(BftSigningJournalEventV2::ProposalEquivocation {
            first: Box::new(first.header.clone()),
            second: Box::new(conflicting.header.clone()),
            now_ms: 102,
        });
    verify_bft_signing_journal_transition_v2(
        &reserved,
        &with_evidence,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("authenticated evidence may append without consuming a pending vote intent");

    let bytes = oasis7_cas::to_canonical_cbor(&with_evidence)
        .expect("encode durable journal after evidence append");
    let reopened: BftSigningJournalV2 =
        serde_cbor::from_slice(&bytes).expect("reopen canonical protected journal bytes");
    let restored =
        replay_bft_signing_journal_v2(&reopened, &fixture.set, &fixture.schedule, TIMEOUTS)
            .expect("reopened journal preserves protocol state and evidence");
    assert_eq!(reopened.pending_vote_intent(), Some(&pending_vote));
    assert_eq!(restored.proposal_equivocation_evidence().count(), 1);
    assert_eq!(restored.proposal_for_round(0).unwrap().header, first.header);
}

#[test]
fn v2_journal_replays_exact_timer_and_reserved_local_proposal() {
    let fixture = Fixture::new();
    let proposer_id = fixture
        .schedule
        .expected_proposer_for_round(0)
        .expect("proposer");
    let proposer_key = fixture
        .keys
        .iter()
        .find(|(id, _)| id == &proposer_id)
        .expect("proposer key");
    let empty = fixture.journal(&proposer_id, &proposer_key.1);
    assert_eq!(empty.height(), 1);
    assert_eq!(empty.schedule_digest(), fixture.schedule.digest().unwrap());
    assert_eq!(
        empty.encoded_len().unwrap(),
        serde_cbor::to_vec(&empty).unwrap().len()
    );
    assert_eq!(
        replay_bft_signing_journal_v2(&empty, &fixture.set, &fixture.schedule, TIMEOUTS)
            .unwrap()
            .height(),
        1
    );

    let mut started = empty.clone();
    started.height_started_at_ms = Some(100);
    verify_bft_signing_journal_transition_v2(
        &empty,
        &started,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("first timer is an allowed transition");
    let machine =
        BftConsensusMachineV2::new(fixture.schedule.clone(), proposer_id.clone(), 100, TIMEOUTS)
            .unwrap();
    let intent = machine
        .proposal_intent(fixture.block(), &proposer_id)
        .unwrap();
    let mut reserved = started.clone();
    reserved.pending_proposal_intent = Some(intent.clone());
    verify_bft_signing_journal_transition_v2(
        &started,
        &reserved,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("deterministic proposal intent is reservable");

    let signed = machine
        .sign_proposal_intent(&intent, &proposer_key.1)
        .unwrap();
    let complete = complete_reserved_bft_proposal_v2(
        &reserved,
        signed,
        101,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .unwrap();
    assert!(complete.pending_proposal_intent().is_none());
    assert_eq!(complete.events().len(), 1);
    replay_bft_signing_journal_v2(&complete, &fixture.set, &fixture.schedule, TIMEOUTS)
        .expect("completed local proposal replays");

    let mut tampered = complete.clone();
    if let BftSigningJournalEventV2::Proposal { proposal, .. } = &mut tampered.events[0] {
        proposal.header.signature_hex = "00".into();
    }
    assert!(
        replay_bft_signing_journal_v2(&tampered, &fixture.set, &fixture.schedule, TIMEOUTS,)
            .is_err()
    );
}

#[test]
fn v2_journal_accepts_inactive_local_identity_for_read_only_replay() {
    let fixture = Fixture::new();
    let inactive_key = SigningKey::from_bytes(&[99; 32]);
    let journal = BftSigningJournalV2::for_schedule(
        &fixture.schedule,
        "future-validator",
        hex::encode(inactive_key.verifying_key().to_bytes()),
    )
    .unwrap();
    let machine =
        replay_bft_signing_journal_v2(&journal, &fixture.set, &fixture.schedule, TIMEOUTS).unwrap();
    assert!(
        machine
            .vote_intent(
                "future-validator",
                BftVotePhaseV2::Prevote,
                BftVoteValueV2::Nil,
                0,
            )
            .unwrap()
            .is_none()
    );

    let mut old_schema = journal;
    old_schema.schema_version = 1;
    assert!(
        replay_bft_signing_journal_v2(&old_schema, &fixture.set, &fixture.schedule, TIMEOUTS,)
            .is_err()
    );
}

#[test]
fn v2_journal_advances_only_after_exact_verified_qc() {
    let fixture = Fixture::new();
    let local_id = "v1";
    let local_key = &fixture.keys[0].1;
    let empty = fixture.journal(local_id, local_key);
    let mut started = empty.clone();
    started.height_started_at_ms = Some(100);
    verify_bft_signing_journal_transition_v2(
        &empty,
        &started,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .unwrap();

    let certificate = fixture.certificate();
    let mut pending = started.clone();
    pending.pending_commit = Some(certificate.clone());
    verify_bft_signing_journal_transition_v2(
        &started,
        &pending,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("only a complete, valid QC may be reserved");
    let token = verify_commit_certificate_token_v2(&fixture.schedule, &certificate).unwrap();
    let next = BftSigningJournalV2::for_schedule(
        token.next_height_schedule(),
        local_id,
        hex::encode(local_key.verifying_key().to_bytes()),
    )
    .unwrap();
    let successor = verify_bft_signing_journal_transition_v2(
        &pending,
        &next,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("exact QC advances schedule and journal by one height");
    assert_eq!(successor.height(), 2);
    assert_eq!(successor.parent_commit_ref(), token.block_id());
    replay_bft_signing_journal_v2(&next, &fixture.set, &successor, TIMEOUTS)
        .expect("next-height empty journal replays");

    let mut forged = next;
    forged.parent_state_root = "99".repeat(32);
    assert!(
        verify_bft_signing_journal_transition_v2(
            &pending,
            &forged,
            &fixture.set,
            &fixture.schedule,
            TIMEOUTS,
        )
        .is_err()
    );
}

#[test]
fn reserved_qc_derives_successor_without_requiring_old_identity_in_new_set() {
    let fixture = Fixture::new();
    let local_id = "v1";
    let local_key = &fixture.keys[0].1;
    let empty = fixture.journal(local_id, local_key);
    let mut started = empty.clone();
    started.height_started_at_ms = Some(100);
    verify_bft_signing_journal_transition_v2(
        &empty,
        &started,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("height start is journaled before a commit reservation");

    let certificate = fixture.certificate();
    let mut pending = started.clone();
    pending.pending_commit = Some(certificate.clone());
    verify_bft_signing_journal_transition_v2(
        &started,
        &pending,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("the certificate is durably reserved by this exact journal");

    let successor = verify_reserved_bft_commit_transition_v2(
        &pending,
        &certificate,
        &fixture.set,
        &fixture.schedule,
        TIMEOUTS,
    )
    .expect("successor derivation does not require the old key in the next set");
    assert_eq!(successor.height(), 2);
    assert_eq!(
        successor.parent_commit_ref(),
        certificate.proposal.header.block_id
    );

    let mut unreserved = pending.clone();
    unreserved.pending_commit = None;
    assert!(
        verify_reserved_bft_commit_transition_v2(
            &unreserved,
            &certificate,
            &fixture.set,
            &fixture.schedule,
            TIMEOUTS,
        )
        .is_err(),
        "a valid QC alone cannot bypass the exact local reservation"
    );

    let mut different_certificate = certificate.clone();
    different_certificate.precommits.pop();
    assert!(
        verify_reserved_bft_commit_transition_v2(
            &pending,
            &different_certificate,
            &fixture.set,
            &fixture.schedule,
            TIMEOUTS,
        )
        .is_err(),
        "a different certificate cannot consume the reserved slot"
    );
}
