//! Correct provider envelopes and current checkpoints cannot replace original
//! validator result membership. The signed peer below intentionally lies in DTOs.
use super::*;
use crate::protected_bft_v3::wire::{
    BftSignerReplyKindV3, SignedBftSignerReplyV3, SignedBftSignerRequestV3, encode_frame,
    read_frame,
};
fn run(archived: bool, vote: bool) {
    let (binding, snapshot, archive, calls) = if archived {
        activated()
    } else {
        let (binding, snapshot) = fixture_kind(true);
        snapshot.lock().unwrap().successor_archive_transfer =
            Some(BftSuccessorArchiveTransferLedgerV1::provisioned());
        (
            binding,
            snapshot,
            Arc::new(Archive::default()),
            Arc::new(AtomicUsize::new(0)),
        )
    };
    let original = if archived {
        archive
            .states
            .lock()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone()
    } else {
        snapshot.lock().unwrap().state.clone()
    };
    let proposal = original.completed_journals[0]
        .as_ref()
        .unwrap()
        .events
        .iter()
        .find_map(|e| {
            if let BftSigningJournalEventV3::Proposal {
                proposal,
                local: true,
                ..
            } = e
            {
                Some(proposal.as_ref().clone())
            } else {
                None
            }
        })
        .unwrap();
    let original_vote = original.completed_journals[0]
        .as_ref()
        .unwrap()
        .events
        .iter()
        .find_map(|e| {
            if let BftSigningJournalEventV3::Vote {
                vote, local: true, ..
            } = e
            {
                Some(vote.as_ref().clone())
            } else {
                None
            }
        })
        .unwrap();
    let (_, upstream, upstream_address) = native_successor_counted_mode_endpoint(
        &binding,
        &snapshot,
        archive.clone(),
        Some(Arc::new(Verifier(archive))),
        Arc::new(AtomicUsize::new(0)),
        calls,
        true,
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let proxy = thread::spawn(move || {
        let (mut downstream, _) = listener.accept().unwrap();
        let mut upstream = std::net::TcpStream::connect(upstream_address).unwrap();
        while let Ok(request) = read_frame::<SignedBftSignerRequestV3>(&mut downstream) {
            std::io::Write::write_all(&mut upstream, &encode_frame(&request).unwrap()).unwrap();
            let mut reply: SignedBftSignerReplyV3 = read_frame(&mut upstream).unwrap();
            let altered = match &reply.body.result {
                BftSignerReplyKindV3::ProposalSigned { proposal } if !vote => {
                    let mut proposal = proposal.clone();
                    proposal.header.signature_hex = "00".repeat(64);
                    Some(BftSignerReplyKindV3::ProposalSigned { proposal })
                }
                BftSignerReplyKindV3::VoteSigned { vote: original } if vote => {
                    // All six request intent fields remain exact and this is a
                    // valid provisioned validator signature, but key32 did not
                    // sign this historical slot in the proved original State.
                    let replacement = BftVoteV3::sign(
                        original.scope.clone(),
                        original.height,
                        original.round,
                        original.phase,
                        original.active_validator_set_hash.clone(),
                        original.schedule_policy_hash.clone(),
                        original.value.clone(),
                        original.validator_id.clone(),
                        &key(32),
                    )
                    .unwrap();
                    replacement.verify_signature().unwrap();
                    Some(BftSignerReplyKindV3::VoteSigned {
                        vote: Box::new(replacement),
                    })
                }
                _ => None,
            };
            if let Some(altered) = altered {
                reply.body.result = altered;
                reply = SignedBftSignerReplyV3::sign(
                    reply.body,
                    &Handle {
                        key: key(33),
                        disabled: None,
                    },
                )
                .unwrap();
            }
            if std::io::Write::write_all(&mut downstream, &encode_frame(&reply).unwrap()).is_err() {
                break;
            }
        }
    });
    let provider = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(10),
        binding.clone(),
        key(34),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: public(33),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap();
    let mut lease = provider.acquire(&binding).unwrap();
    if vote {
        let intent = BftVoteIntentV3 {
            height: original_vote.height,
            round: original_vote.round,
            phase: original_vote.phase,
            value: original_vote.value.clone(),
            validator_id: original_vote.validator_id.clone(),
            schedule_policy_hash: original_vote.schedule_policy_hash.clone(),
        };
        assert!(
            lease.sign_vote(0, &intent).is_err(),
            "valid same-intent signed DTO absent from proved local slot must fail (archived={archived})"
        );
    } else {
        let mut header = proposal.header.clone();
        header.signature_hex.clear();
        let intent = BftProposalIntentV3 {
            header,
            block: proposal.block,
            valid_round_evidence: proposal.valid_round_evidence,
        };
        assert!(
            lease.sign_proposal(0, &intent).is_err(),
            "signed provider reply cannot authenticate a junk validator signature (archived={archived})"
        );
    }
    drop(lease);
    proxy.join().unwrap();
    upstream.join().unwrap();
}
#[test]
fn signed_result_client_rejects_junk_proposal_in_valid_hot_checkpoint_reply() {
    run(false, false);
}
#[test]
fn signed_result_client_rejects_junk_proposal_in_valid_archived_checkpoint_reply() {
    run(true, false);
}
#[test]
fn signed_result_client_rejects_valid_same_intent_vote_absent_from_hot_local_journal() {
    run(false, true);
}
#[test]
fn signed_result_client_rejects_valid_same_intent_vote_absent_from_archived_local_journal() {
    run(true, true);
}
