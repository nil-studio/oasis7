use ed25519_dalek::SigningKey;
use oasis7_formal_protocol::{
    bft::{
        BFT_FORMAL_EXECUTION_DOMAIN_V1, BftHeaderHeightScheduleV3, BftProfileTargetV2,
        BftSchedulePolicyV2, FormalBftCertifiedHeaderV3, FormalBftExecutionCommitmentV1,
        verify_formal_bft_handoff_v2,
    },
    controlled_authority::formal::{
        self as formal, FormalCompletionBranchV1, InitialActivationTrustV1, ProfileHandoffV1,
        VerifiedFormalReadAnchorV1, test_fixtures as fixtures,
    },
};
use oasis7_proto::distributed_bft::{
    v2::{BftScopeV2, BftValidatorV2},
    v3::{
        BFT_PROTOCOL_VERSION_V3, BftBlockHeaderV3, BftHeaderCommitCertificateV3,
        BftProposalHeaderV3, BftVotePhaseV3, BftVoteV3, BftVoteValueV3,
    },
};

use crate::authority_commit_verifier::{
    AuthorityCommitProof, AuthorityCommitReject, AuthorityCommitVerifier,
    BftV3AuthorityCommitVerifier, CommitCandidateRef, CompositeAuthorityCommitVerifier,
    ControlledAuthorityCommitVerifier, TrustedCommitAnchor,
};

fn build_bft_schedule_and_keys(
    weights: &[u64],
) -> (BftHeaderHeightScheduleV3, Vec<(String, SigningKey)>) {
    let keys = weights
        .iter()
        .enumerate()
        .map(|(i, _)| {
            (
                format!("validator-{i}"),
                SigningKey::from_bytes(&[41 + i as u8; 32]),
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
    (schedule, keys)
}

fn build_certified_bft_header(
    schedule: &BftHeaderHeightScheduleV3,
    keys: &[(String, SigningKey)],
    signer_indices: &[usize],
) -> FormalBftCertifiedHeaderV3 {
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
    let proposer_id = schedule.expected_proposer_for_round(0).unwrap();
    let (_, proposer_key) = keys
        .iter()
        .find(|(id, _)| id == &proposer_id)
        .expect("proposer key");
    let header = BftBlockHeaderV3 {
        protocol_version: BFT_PROTOCOL_VERSION_V3,
        execution_commitment_hash: commitment.digest().unwrap(),
        actions_cbor_digest: oasis7_proto::distributed_bft::v3::actions_cbor_digest_v3(&[0x80])
            .unwrap(),
    };
    let block_id = header.block_id().unwrap();
    let proposal = BftProposalHeaderV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        proposer_id,
        block_id.clone(),
        None,
        proposer_key,
    )
    .unwrap();
    let precommits = signer_indices
        .iter()
        .map(|&idx| {
            let (id, key) = &keys[idx];
            BftVoteV3::sign(
                schedule.scope().clone(),
                schedule.height(),
                0,
                BftVotePhaseV3::Precommit,
                schedule.active_set().set_hash().into(),
                schedule.policy_hash().into(),
                BftVoteValueV3::Block {
                    block_id: block_id.clone(),
                },
                id.clone(),
                key,
            )
            .unwrap()
        })
        .collect();
    FormalBftCertifiedHeaderV3 {
        schema_version: 1,
        header,
        execution_commitment: commitment,
        certificate: BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence: None,
            precommits,
        },
    }
}

#[test]
fn controlled_qualified_commit_passes_and_preserves_decision_vs_qualified_distinction() {
    let (_, authority, initial_proof, initial_commit) = fixtures::initial();
    let anchor = TrustedCommitAnchor::controlled(authority.clone(), None, None);
    let initial_record_id = initial_commit.head().record_id;
    let candidate = CommitCandidateRef::controlled(
        Some(initial_commit.record().position),
        Some(&initial_record_id),
    );

    let qualified_verifier = CompositeAuthorityCommitVerifier::qualified_only();
    let verified = qualified_verifier
        .verify_authority_commit(
            &anchor,
            &candidate,
            &AuthorityCommitProof::controlled_qualified(initial_proof.clone()),
        )
        .expect("qualified commit should verify");
    assert!(verified.is_qualified_commit());
    assert_eq!(
        verified.height_or_position(),
        initial_commit.record().position
    );
    let commit = verified.into_controlled_qualified().unwrap();
    assert_eq!(commit.head().record_id, initial_record_id);

    // Decision-only proof (without P/R complete receipts) MUST be rejected by qualified_only verifier.
    let decision_only_err = qualified_verifier
        .verify_authority_commit(
            &anchor,
            &candidate,
            &AuthorityCommitProof::controlled_decision(initial_proof.decision.clone()),
        )
        .expect_err("decision-only proof must not pass qualified-only verifier");
    assert_eq!(
        decision_only_err,
        AuthorityCommitReject::DecisionOnlyNotQualified {
            position: initial_commit.record().position,
        }
    );

    // Explicit decision-only verifier accepts it as ControlledDecision, which is NOT a qualified commit.
    let decision_verifier = ControlledAuthorityCommitVerifier::allowing_decision_only();
    let verified_decision = decision_verifier
        .verify_authority_commit(
            &anchor,
            &candidate,
            &AuthorityCommitProof::controlled_decision(initial_proof.decision.clone()),
        )
        .expect("decision-only verifier should accept valid decision");
    assert!(!verified_decision.is_qualified_commit());
    assert!(matches!(
        verified_decision.into_controlled_qualified(),
        Err(AuthorityCommitReject::DecisionOnlyNotQualified { .. })
    ));

    // Tampered P/R complete receipt signature MUST be rejected.
    let mut tampered = initial_proof;
    tampered.replica_complete.signature_hex = "00".repeat(64);
    let tampered_err = qualified_verifier
        .verify_authority_commit(
            &anchor,
            &candidate,
            &AuthorityCommitProof::controlled_qualified(tampered),
        )
        .expect_err("tampered replica complete receipt must be rejected");
    assert!(matches!(
        tampered_err,
        AuthorityCommitReject::ControlledVerificationFailed(_)
    ));
}

#[test]
fn controlled_recovery_commit_requires_original_authority_and_exact_context() {
    let verifier = CompositeAuthorityCommitVerifier::qualified_only();
    for branch in [
        FormalCompletionBranchV1::ReplayFrozen,
        FormalCompletionBranchV1::CompletePrepared,
    ] {
        let (intent, original, cut, frozen, record) = fixtures::recovery_fixture_for(branch, None);
        let recovery = fixtures::verified_recovery(&intent, &original);
        let record_id = record.record_id().unwrap();
        let candidate = CommitCandidateRef::controlled(Some(record.position), Some(&record_id));
        let proof = AuthorityCommitProof::controlled_qualified(fixtures::proof(
            record.clone(),
            recovery.successor(),
            branch,
            Some(recovery.certificate_digest().into()),
            Some(frozen),
            if branch == FormalCompletionBranchV1::ReplayFrozen {
                10
            } else {
                4
            },
            11,
            12,
        ));
        let anchor = TrustedCommitAnchor::controlled(
            original.clone(),
            Some(cut.clone()),
            Some(recovery.clone()),
        );
        let verified = verifier
            .verify_authority_commit(&anchor, &candidate, &proof)
            .expect("signed recovery commit verifies against its original authority")
            .into_controlled_qualified()
            .unwrap();
        assert_eq!(verified.record(), &record);
        assert_eq!(
            verified.finalizing_authority_id(),
            recovery.successor().authority_id().unwrap()
        );

        let (other_intent, _, _, _, _) =
            fixtures::recovery_fixture_with_profile(branch, None, 13, 1);
        let other_recovery = fixtures::verified_recovery(&other_intent, &original);
        for invalid_anchor in [
            TrustedCommitAnchor::controlled(original.clone(), Some(cut.clone()), None),
            TrustedCommitAnchor::controlled(
                recovery.successor().clone(),
                Some(cut.clone()),
                Some(recovery.clone()),
            ),
            TrustedCommitAnchor::controlled(original.clone(), Some(cut), Some(other_recovery)),
        ] {
            assert!(matches!(
                verifier.verify_authority_commit(&invalid_anchor, &candidate, &proof),
                Err(AuthorityCommitReject::ControlledVerificationFailed(_))
            ));
        }
        assert!(matches!(
            verifier.verify_authority_commit(
                &anchor,
                &CommitCandidateRef::controlled(Some(record.position + 1), Some(&record_id)),
                &proof,
            ),
            Err(AuthorityCommitReject::CandidateMismatch {
                field: "position",
                ..
            })
        ));
        assert!(matches!(
            verifier.verify_authority_commit(
                &anchor,
                &CommitCandidateRef::controlled(Some(record.position), Some(&fixtures::h(99))),
                &proof,
            ),
            Err(AuthorityCommitReject::CandidateMismatch {
                field: "record_id",
                ..
            })
        ));
    }
}

#[test]
fn bft_v3_verifier_accepts_valid_quorum_and_rejects_insufficient_weight_or_height_mismatch() {
    let (schedule, keys) = build_bft_schedule_and_keys(&[1, 1, 1, 1]);
    let valid_cert = build_certified_bft_header(&schedule, &keys, &[0, 1, 2]);
    let expected_block_id = valid_cert.header.block_id().unwrap();

    let verifier = CompositeAuthorityCommitVerifier::qualified_only();
    let anchor = TrustedCommitAnchor::bft_v3(schedule.clone());
    let candidate = CommitCandidateRef::bft_v3(Some(schedule.height()), Some(&expected_block_id));

    let verified = verifier
        .verify_authority_commit(
            &anchor,
            &candidate,
            &AuthorityCommitProof::bft_v3(valid_cert.clone()),
        )
        .expect("3/4 voting power quorum must verify");
    assert!(verified.is_qualified_commit());
    assert_eq!(verified.height_or_position(), schedule.height());
    let token = verified.into_bft_v3().unwrap();
    let next_schedule = token.next_height_schedule().clone();
    assert_eq!(next_schedule.height(), schedule.height() + 1);

    // Insufficient voting power (2/4 < floor(2*4/3)+1 = 3) MUST be rejected.
    let weak_cert = build_certified_bft_header(&schedule, &keys, &[0, 1]);
    let weak_err = verifier
        .verify_authority_commit(
            &anchor,
            &CommitCandidateRef::bft_v3(Some(schedule.height()), None),
            &AuthorityCommitProof::bft_v3(weak_cert),
        )
        .expect_err("sub-quorum precommits must be rejected");
    assert!(matches!(
        weak_err,
        AuthorityCommitReject::BftVerificationFailed(_)
    ));

    // Replaying height H certificate against height H+1 schedule MUST be rejected.
    let wrong_height_err = BftV3AuthorityCommitVerifier::new()
        .verify_authority_commit(
            &TrustedCommitAnchor::bft_v3(next_schedule),
            &CommitCandidateRef::bft_v3(None, None),
            &AuthorityCommitProof::bft_v3(valid_cert.clone()),
        )
        .expect_err("height H certificate must be rejected at H+1 schedule");
    assert!(matches!(
        wrong_height_err,
        AuthorityCommitReject::BftVerificationFailed(_)
    ));

    // Candidate block_id mismatch MUST be rejected deterministically.
    let bad_candidate_err = BftV3AuthorityCommitVerifier::new()
        .verify_authority_commit(
            &anchor,
            &CommitCandidateRef::bft_v3(Some(schedule.height()), Some("deadbeef")),
            &AuthorityCommitProof::bft_v3(valid_cert),
        )
        .expect_err("candidate block_id mismatch must be rejected");
    assert!(matches!(
        bad_candidate_err,
        AuthorityCommitReject::CandidateMismatch {
            field: "block_id",
            ..
        }
    ));
}

#[test]
fn profile_mismatch_between_anchor_and_proof_is_rejected() {
    let (_, authority, initial_proof, _) = fixtures::initial();
    let (schedule, keys) = build_bft_schedule_and_keys(&[1, 1, 1, 1]);
    let bft_cert = build_certified_bft_header(&schedule, &keys, &[0, 1, 2]);
    let verifier = CompositeAuthorityCommitVerifier::qualified_only();

    let err = verifier
        .verify_authority_commit(
            &TrustedCommitAnchor::controlled(authority, None, None),
            &CommitCandidateRef::controlled(None, None),
            &AuthorityCommitProof::bft_v3(bft_cert),
        )
        .expect_err("controlled anchor with BFT proof must be rejected");
    assert_eq!(
        err,
        AuthorityCommitReject::ProfileMismatch {
            expected: "controlled_single_authority",
            actual: "bft_v3",
        }
    );

    let err2 = verifier
        .verify_authority_commit(
            &TrustedCommitAnchor::bft_v3(schedule),
            &CommitCandidateRef::bft_v3(None, None),
            &AuthorityCommitProof::controlled_qualified(initial_proof),
        )
        .expect_err("BFT anchor with controlled proof must be rejected");
    assert_eq!(
        err2,
        AuthorityCommitReject::ProfileMismatch {
            expected: "bft_v3",
            actual: "controlled_single_authority",
        }
    );
}
