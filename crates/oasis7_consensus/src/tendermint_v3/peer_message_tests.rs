//! DTO vectors prove framing/binding only, not qualified authority or World execution.
use super::*;
use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::v3::{
    BftBlockHeaderV3, BftHeaderCommitCertificateV3, BftProposalHeaderV3, BftVotePhaseV3,
    BftVoteValueV3,
};
fn h(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn context() -> BftPeerContextV3 {
    BftPeerContextV3 {
        activation_digest: h(1),
        target_digest: h(2),
        scope: BftScopeV2 {
            world_id: "peer-vector".into(),
            chain_id: "chain".into(),
            genesis_hash: h(3),
        },
    }
}
fn request() -> BftPeerHistoryRequestV3 {
    BftPeerHistoryRequestV3 {
        nonce: h(4),
        after_height: 8,
        after_header_id: h(5),
        max_entries: 64,
    }
}
fn material() -> (BftPublicProposalV3, BftVoteV3, FormalBftCertifiedHeaderV3) {
    let context = context();
    let key = SigningKey::from_bytes(&[17; 32]);
    let c = super::super::FormalBftExecutionCommitmentV1 {
        domain: "oasis7.formal.bft.execution.v1".into(),
        schema_version: 1,
        profile: "bft".into(),
        profile_version: 1,
        activation_digest: context.activation_digest.clone(),
        scope: context.scope.clone(),
        height: 9,
        parent_commit_ref: h(5),
        parent_tick: 8,
        parent_semantic_state_root: h(6),
        authority_epoch: 1,
        active_validator_set_hash: h(7),
        schedule_policy_hash: h(8),
        manifest_hash: h(9),
        post_manifest_hash: h(10),
        action_root: h(11),
        event_root: h(12),
        execution_state_root: h(13),
        state_root: h(14),
        projection_root: h(15),
        result_root: h(16),
        transition: None,
        transition_ref: None,
    };
    // Only a public digest leaves this fixture, never the private canary bytes.
    let private = b"PRIVATE_ACTION_INPUT_CANARY_never_in_public_wire";
    let header = BftBlockHeaderV3 {
        protocol_version: 3,
        execution_commitment_hash: c.digest().unwrap(),
        actions_cbor_digest: oasis7_proto::distributed_bft::v3::actions_cbor_digest_v3(private)
            .unwrap(),
    };
    let proposal = BftProposalHeaderV3::sign(
        context.scope.clone(),
        9,
        0,
        h(7),
        h(8),
        "validator".into(),
        header.block_id().unwrap(),
        None,
        &key,
    )
    .unwrap();
    let vote = BftVoteV3::sign(
        context.scope,
        9,
        0,
        BftVotePhaseV3::Precommit,
        h(7),
        h(8),
        BftVoteValueV3::Block {
            block_id: header.block_id().unwrap(),
        },
        "validator".into(),
        &key,
    )
    .unwrap();
    let public = BftPublicProposalV3 {
        header: proposal.clone(),
        block: super::super::BftHeaderValueV3 {
            header: header.clone(),
            execution_commitment: c.clone(),
        },
        valid_round_evidence: None,
    };
    let cert = FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence: None,
            precommits: vec![vote.clone()],
        },
        header,
        c,
    )
    .unwrap();
    (public, vote, cert)
}
#[test]
fn every_public_variant_round_trips_without_private_preimages() {
    let (p, v, c) = material();
    let r = request();
    let reply = BftPeerHistoryReplyV3 {
        request: r.clone(),
        certificates: vec![c.clone()],
    };
    for m in [
        BftPublicPeerMessageV3::Proposal(Box::new(p)),
        BftPublicPeerMessageV3::Vote(Box::new(v)),
        BftPublicPeerMessageV3::Commit(Box::new(c)),
        BftPublicPeerMessageV3::HistoryRequest(r),
        BftPublicPeerMessageV3::HistoryReply(reply),
    ] {
        let bytes = encode_bft_public_peer_message_v3(&context(), &m).unwrap();
        assert_eq!(
            decode_bft_public_peer_message_v3(&bytes, &context()).unwrap(),
            m
        );
        let canary = b"PRIVATE_ACTION_INPUT_CANARY_never_in_public_wire";
        assert!(!bytes.windows(canary.len()).any(|w| w == canary));
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("\"actions_cbor\":"));
        assert!(!json.contains("\"payload_cbor\":"));
    }
}
#[test]
fn context_domain_schema_and_canonical_encoding_are_required() {
    let m = BftPublicPeerMessageV3::HistoryRequest(request());
    let bytes = encode_bft_public_peer_message_v3(&context(), &m).unwrap();
    for n in 0..3 {
        let mut wrong = context();
        match n {
            0 => wrong.activation_digest = h(99),
            1 => wrong.target_digest = h(99),
            _ => wrong.scope.world_id = "other".into(),
        }
        assert!(decode_bft_public_peer_message_v3(&bytes, &wrong).is_err());
    }
    for (domain, version) in [
        ("oasis7.bft.public-peer.v2", 3),
        (BFT_PUBLIC_PEER_DOMAIN_V3, 2),
    ] {
        let wrong = canonical(&Envelope {
            domain: domain.into(),
            schema_version: version,
            context: context(),
            message: m.clone(),
        })
        .unwrap();
        assert!(decode_bft_public_peer_message_v3(&wrong, &context()).is_err());
    }
    let noncanonical = serde_cbor::to_vec(&Envelope {
        domain: BFT_PUBLIC_PEER_DOMAIN_V3.into(),
        schema_version: 3,
        context: context(),
        message: m,
    })
    .unwrap();
    assert_ne!(bytes, noncanonical);
    assert!(decode_bft_public_peer_message_v3(&noncanonical, &context()).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode_bft_public_peer_message_v3(&trailing, &context()).is_err());
}
#[test]
fn history_echo_and_exact_prefix_are_not_peer_authority() {
    let (_, _, c) = material();
    let mut reply = BftPeerHistoryReplyV3 {
        request: request(),
        certificates: vec![c],
    };
    reply.validate_for_request(&request()).unwrap();
    let mut other = request();
    other.nonce = h(99);
    assert!(reply.validate_for_request(&other).is_err());
    reply.certificates[0].execution_commitment.height += 1;
    assert!(reply.validate_for_request(&request()).is_err());
    reply.certificates[0].execution_commitment.height -= 1;
    reply.certificates[0].execution_commitment.parent_commit_ref = h(99);
    assert!(reply.validate_for_request(&request()).is_err());
    assert!(
        encode_bft_public_peer_message_v3(&context(), &BftPublicPeerMessageV3::HistoryReply(reply))
            .is_err()
    );
}
#[test]
fn declared_lengths_are_refused_before_elements_or_string_allocation() {
    for bytes in [
        vec![0x9b, 255, 255, 255, 255, 255, 255, 255, 255],
        vec![0x7b, 255, 255, 255, 255, 255, 255, 255, 255],
        vec![0x9f, 0xff],
        vec![0xc1, 0],
    ] {
        assert!(decode_bft_public_peer_message_v3(&bytes, &context()).is_err());
    }
    // The semantic history visitor rejects 65 before asking for its first item.
    let mut bytes = vec![0xa2, 0x67];
    bytes.extend_from_slice(b"request");
    bytes.extend(serde_cbor::to_vec(&request()).unwrap());
    bytes.push(0x6c);
    bytes.extend_from_slice(b"certificates");
    bytes.extend([0x98, 65]);
    let error = serde_cbor::from_slice::<BftPeerHistoryReplyV3>(&bytes).unwrap_err();
    assert!(error.to_string().contains("declared count"), "{error}");
    assert!(
        decode_bft_public_peer_message_v3(&vec![0; BFT_PUBLIC_PEER_MAX_BYTES_V3 + 1], &context())
            .is_err()
    );
    let mut r = request();
    r.max_entries = 65;
    assert!(
        encode_bft_public_peer_message_v3(&context(), &BftPublicPeerMessageV3::HistoryRequest(r))
            .is_err()
    );
}

#[test]
fn public_framing_does_not_replace_schedule_signature_authentication() {
    let (_, mut vote, _) = material();
    vote.signature_hex = "00".repeat(64);
    let message = BftPublicPeerMessageV3::Vote(Box::new(vote.clone()));
    let bytes = encode_bft_public_peer_message_v3(&context(), &message).unwrap();
    assert_eq!(
        decode_bft_public_peer_message_v3(&bytes, &context()).unwrap(),
        message
    );
    assert!(vote.verify_signature().is_err());
    // An attacker cannot extend the typed body with a private-input field.
    let mut value = serde_cbor::value::to_value(Envelope {
        domain: BFT_PUBLIC_PEER_DOMAIN_V3.into(),
        schema_version: 3,
        context: context(),
        message,
    })
    .unwrap();
    if let serde_cbor::Value::Map(ref mut map) = value {
        map.insert(
            serde_cbor::Value::Text("actions_cbor".into()),
            serde_cbor::Value::Bytes(b"private-input".to_vec()),
        );
    } else {
        panic!("envelope map");
    }
    assert!(decode_bft_public_peer_message_v3(&canonical(&value).unwrap(), &context()).is_err());
}

#[test]
fn maximum_vote_and_transition_counts_round_trip_without_scanner_false_refusal() {
    use oasis7_proto::distributed_bft::{
        v2::{BftValidatorTransitionV2, BftValidatorV2},
        v3::{BFT_MAX_VALIDATORS_V3, BftValidRoundEvidenceV3},
    };
    // Legal wire shapes only; this fixture does not claim governance approval,
    // admitted schedule membership or actual execution of the transition.
    let (mut public, _, _) = material();
    let keys: Vec<_> = (0..BFT_MAX_VALIDATORS_V3)
        .map(|i| {
            let mut seed = [41; 32];
            seed[..2].copy_from_slice(&(i as u16).to_be_bytes());
            SigningKey::from_bytes(&seed)
        })
        .collect();
    let validators: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| BftValidatorV2 {
            validator_id: format!("validator-{i:03}"),
            public_key_hex: hex::encode(k.verifying_key().to_bytes()),
            voting_power: 1,
        })
        .collect();
    let commitment = &mut public.block.execution_commitment;
    commitment.transition = Some(BftValidatorTransitionV2 {
        source_commit_ref: commitment.parent_commit_ref.clone(),
        source_state_root: commitment.parent_semantic_state_root.clone(),
        source_profile: "controlled-bft".into(),
        source_epoch: 1,
        target_epoch: 11,
        epoch_length_ticks: 1,
        mapping_ruleset_hash: h(20),
        scheduled_height: 9,
        activation_height: 11,
        old_validator_set_hash: h(7),
        new_validators: validators.clone(),
        new_validator_set_hash: h(21),
        governance_action_digest: h(22),
        projected_registry_digest: h(23),
    });
    public.block.header.execution_commitment_hash = commitment.digest().unwrap();
    let block_id = public.block.header.block_id().unwrap();
    let old = BftProposalHeaderV3::sign(
        context().scope,
        9,
        0,
        h(7),
        h(8),
        validators[0].validator_id.clone(),
        block_id.clone(),
        None,
        &keys[0],
    )
    .unwrap();
    let votes = |round, phase| {
        keys.iter()
            .zip(&validators)
            .map(|(key, v)| {
                BftVoteV3::sign(
                    context().scope,
                    9,
                    round,
                    phase,
                    h(7),
                    h(8),
                    BftVoteValueV3::Block {
                        block_id: block_id.clone(),
                    },
                    v.validator_id.clone(),
                    key,
                )
                .unwrap()
            })
            .collect::<Vec<_>>()
    };
    let evidence = BftValidRoundEvidenceV3 {
        proposal: old,
        prevotes: votes(0, BftVotePhaseV3::Prevote),
    };
    public.header = BftProposalHeaderV3::sign(
        context().scope,
        9,
        1,
        h(7),
        h(8),
        validators[0].validator_id.clone(),
        block_id.clone(),
        Some(0),
        &keys[0],
    )
    .unwrap();
    public.valid_round_evidence = Some(evidence.clone());
    let cert = FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal: public.header.clone(),
            valid_round_evidence: Some(evidence),
            precommits: votes(1, BftVotePhaseV3::Precommit),
        },
        public.block.header.clone(),
        public.block.execution_commitment.clone(),
    )
    .unwrap();
    for message in [
        BftPublicPeerMessageV3::Proposal(Box::new(public)),
        BftPublicPeerMessageV3::Commit(Box::new(cert)),
    ] {
        let bytes = encode_bft_public_peer_message_v3(&context(), &message).unwrap();
        assert_eq!(
            decode_bft_public_peer_message_v3(&bytes, &context()).unwrap(),
            message
        );
    }
}
