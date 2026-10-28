//! Public-wire page budgeting vectors; these do not attest quorum or execution.
use super::*;
use oasis7_proto::distributed_bft::{
    v2::BftScopeV2,
    v3::{
        BftBlockHeaderV3, BftHeaderCommitCertificateV3, BftProposalHeaderV3,
        BftValidRoundEvidenceV3, BftVotePhaseV3, BftVoteV3, BftVoteValueV3,
    },
};
fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn large_page(
    count: u64,
) -> (
    BftPeerContextV3,
    BftPeerHistoryRequestV3,
    Vec<FormalBftCertifiedHeaderV3>,
) {
    let scope = BftScopeV2 {
        world_id: "w".repeat(256),
        chain_id: "c".repeat(256),
        genesis_hash: hash(1),
    };
    let context = BftPeerContextV3 {
        scope: scope.clone(),
        activation_digest: hash(2),
        target_digest: hash(3),
    };
    let request = BftPeerHistoryRequestV3 {
        nonce: hash(4),
        after_height: 8,
        after_header_id: hash(5),
        max_entries: 64,
    };
    let key_hex = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&[18; 32])
            .verifying_key()
            .to_bytes(),
    );
    let mut parent = request.after_header_id.clone();
    let mut certificates = Vec::new();
    for height in 9..9 + count {
        let commitment = FormalBftExecutionCommitmentV1 {
            domain: "oasis7.formal.bft.execution.v1".into(),
            schema_version: 1,
            profile: "bft".into(),
            profile_version: 1,
            activation_digest: context.activation_digest.clone(),
            scope: scope.clone(),
            height,
            parent_tick: height - 1,
            parent_commit_ref: parent,
            parent_semantic_state_root: hash(6),
            authority_epoch: 1,
            active_validator_set_hash: hash(7),
            schedule_policy_hash: hash(8),
            manifest_hash: hash(9),
            post_manifest_hash: hash(10),
            action_root: hash(11),
            event_root: hash(12),
            execution_state_root: hash(13),
            state_root: hash(14),
            projection_root: hash(15),
            result_root: hash(16),
            transition: None,
            transition_ref: None,
        };
        let header = BftBlockHeaderV3 {
            protocol_version: 3,
            execution_commitment_hash: commitment.digest().unwrap(),
            actions_cbor_digest: hash(17),
        };
        parent = header.block_id().unwrap();
        let original = BftProposalHeaderV3 {
            protocol_version: 3,
            scope: scope.clone(),
            height,
            round: 0,
            active_validator_set_hash: hash(7),
            schedule_policy_hash: hash(8),
            proposer_id: "p".repeat(256),
            block_id: parent.clone(),
            valid_round: None,
            public_key_hex: key_hex.clone(),
            signature_hex: "00".repeat(64),
        };
        let votes = |round, phase| {
            (0..256)
                .map(|id| BftVoteV3 {
                    protocol_version: 3,
                    scope: scope.clone(),
                    height,
                    round,
                    phase,
                    active_validator_set_hash: hash(7),
                    schedule_policy_hash: hash(8),
                    value: BftVoteValueV3::Block {
                        block_id: parent.clone(),
                    },
                    validator_id: format!("{id:03}{}", "v".repeat(253)),
                    public_key_hex: key_hex.clone(),
                    signature_hex: "00".repeat(64),
                })
                .collect()
        };
        let evidence = BftValidRoundEvidenceV3 {
            proposal: original.clone(),
            prevotes: votes(0, BftVotePhaseV3::Prevote),
        };
        let mut proposal = original;
        proposal.round = 1;
        proposal.valid_round = Some(0);
        certificates.push(
            FormalBftCertifiedHeaderV3::new(
                BftHeaderCommitCertificateV3 {
                    proposal,
                    valid_round_evidence: Some(evidence),
                    precommits: votes(1, BftVotePhaseV3::Precommit),
                },
                header,
                commitment,
            )
            .unwrap(),
        );
    }
    (context, request, certificates)
}
#[test]
fn history_page_is_an_encodable_maximal_contiguous_prefix_under_the_real_wire_budget() {
    let (context, request, certificates) = large_page(64);
    let whole = BftPublicPeerMessageV3::HistoryReply(BftPeerHistoryReplyV3 {
        request: request.clone(),
        certificates: certificates.clone(),
    });
    assert!(
        encode_bft_public_peer_message_v3(&context, &whole).is_err(),
        "64 individually valid large certificates must exceed the real 16MiB page budget"
    );
    let reply = bounded_history_reply(&context, request.clone(), certificates.clone()).unwrap();
    assert!(!reply.certificates.is_empty() && reply.certificates.len() < certificates.len());
    assert_eq!(reply.request, request);
    assert_eq!(reply.certificates, certificates[..reply.certificates.len()]);
    reply.validate_for_request(&request).unwrap();
    let bytes = encode_bft_public_peer_message_v3(
        &context,
        &BftPublicPeerMessageV3::HistoryReply(reply.clone()),
    )
    .unwrap();
    assert_eq!(
        decode_bft_public_peer_message_v3(&bytes, &context).unwrap(),
        BftPublicPeerMessageV3::HistoryReply(reply.clone())
    );
    let too_many = BftPublicPeerMessageV3::HistoryReply(BftPeerHistoryReplyV3 {
        request,
        certificates: certificates[..reply.certificates.len() + 1].to_vec(),
    });
    assert!(
        encode_bft_public_peer_message_v3(&context, &too_many).is_err(),
        "the selected prefix is maximal, not merely an arbitrary small page"
    );
}
#[test]
fn original_history_request_deadline_covers_the_maximum_paced_page_without_renewal() {
    let started = Instant::now();
    // Public transport uses 16KiB chunks and 75ms pacing; 16MiB needs ~77s.
    let paced_page =
        Duration::from_millis((BFT_PUBLIC_PEER_MAX_BYTES_V3 as u64).div_ceil(16 * 1024) * 75);
    let request = BftPeerHistoryRequestV3 {
        nonce: hash(1),
        after_height: 8,
        after_header_id: hash(2),
        max_entries: 64,
    };
    let outstanding = HistoryRequest {
        peer: "peer".into(),
        request,
        deadline: started + HISTORY_RESPONSE_TIMEOUT,
    };
    assert!(paced_page > Duration::from_secs(5));
    assert!(outstanding.deadline > started + paced_page);
    assert_eq!(
        outstanding.deadline.duration_since(started),
        Duration::from_secs(120)
    );
    // A late chunk uses this same deadline; neither polling nor peer retries
    // are allowed to turn its already elapsed time into a new window.
    assert!(started + Duration::from_secs(121) >= outstanding.deadline);
}

#[test]
fn local_slot_echoes_never_become_external_events_but_commits_remain_admissible() {
    let (_, _, mut page) = large_page(1);
    let certificate = page.remove(0);
    let vote = certificate.certificate.precommits[0].clone();
    let local = vote.validator_id.clone();
    assert!(is_local_echo(
        &BftPublicPeerMessageV3::Vote(Box::new(vote)),
        &local
    ));
    assert!(!is_local_echo(
        &BftPublicPeerMessageV3::Commit(Box::new(certificate.clone())),
        &certificate.certificate.proposal.proposer_id
    ));
    let proposal = BftPublicProposalV3 {
        header: certificate.certificate.proposal.clone(),
        block: BftHeaderValueV3 {
            header: certificate.header,
            execution_commitment: certificate.execution_commitment,
        },
        valid_round_evidence: certificate.certificate.valid_round_evidence,
    };
    let proposer = proposal.header.proposer_id.clone();
    assert!(is_local_echo(
        &BftPublicPeerMessageV3::Proposal(Box::new(proposal.clone())),
        &proposer
    ));
    assert!(!is_local_echo(
        &BftPublicPeerMessageV3::Proposal(Box::new(proposal)),
        "another-validator"
    ));
}

#[test]
fn a_hostile_fetch_route_cannot_consume_another_configured_peers_share() {
    let configured = vec!["noise-a".into(), "noise-b".into()];
    let mut budget = FetchBudget::default();
    for value in 0..16 {
        for _ in 0..4 {
            assert!(budget.reserve(&configured, "noise-a", &hash(value)));
        }
        assert!(!budget.reserve(&configured, "noise-a", &hash(value)));
    }
    assert!(!budget.reserve(&configured, "noise-a", &hash(90)));
    assert!(!budget.reserve(&configured, "unconfigured-noise", &hash(90)));
    assert!(
        budget.reserve(&configured, "noise-b", &hash(0)),
        "B can fetch the same QC after A exhausted its share"
    );
    assert_eq!(budget.peers.get("noise-a"), Some(&64));
    assert_eq!(budget.peers.get("noise-b"), Some(&1));
}

#[test]
fn only_unreserved_vote_effects_at_retired_positions_are_discarded() {
    let intent = BftVoteIntentV3 {
        height: 9,
        round: 0,
        phase: BftVotePhaseV3::Prevote,
        value: BftVoteValueV3::Nil,
        validator_id: "local".into(),
        schedule_policy_hash: hash(1),
    };
    assert!(retired_unreserved_vote(&intent, 9, 1, None));
    assert!(retired_unreserved_vote(&intent, 10, 0, None));
    assert!(!retired_unreserved_vote(&intent, 9, 0, None));
    assert!(
        !retired_unreserved_vote(&intent, 9, 1, Some(&intent)),
        "a protected original decision must never be silently dropped"
    );
}
