use super::test_fixtures as fixtures;
use super::*;
pub(crate) use fixtures::recovery_fixture;
use fixtures::*;
#[path = "recovery_retained_tests.rs"]
mod retained_tests;

#[test]
fn initial_authorization_requires_external_policy_and_committed_receipts() {
    let (certificate, authority, mut proof, _) = initial();
    let mut trust = InitialActivationTrustV1 {
        issuer_public_key: public(9),
        world: authority.definition.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: h(81),
        recovery_policy_digest: policy().digest().unwrap(),
    };
    trust.issuer_public_key = public(8);
    assert!(verify_initial_activation(&certificate, &trust).is_err());
    trust.issuer_public_key = public(9);
    trust.recovery_policy_digest = h(90);
    assert!(verify_initial_activation(&certificate, &trust).is_err());
    proof.decision.replica.signature_hex = proof.decision.primary.signature_hex.clone();
    assert!(verify_formal_qualified(&proof, &authority, None, None).is_err());
}
#[test]
fn policy_rejects_weak_keys_and_shared_external_roles() {
    for field in 0..3 {
        let mut policy = policy();
        match field {
            0 => policy.recovery_issuer_public_key = h(0),
            1 => policy.fencing_provider_public_key = h(0),
            _ => policy.issuer_anchor_public_key = h(0),
        }
        assert!(policy.validate().is_err());
    }
    let mut shared = policy();
    shared.fencing_provider_public_key = shared.recovery_issuer_public_key.clone();
    assert!(shared.validate().is_err());
    let (_, authority, _, _) = initial();
    let mut policy = policy();
    policy.fencing_provider_public_key = authority.definition.writer_public_key.clone();
    let mut definition = authority.definition.clone();
    definition.recovery_policy_digest = policy.digest().unwrap();
    assert!(definition.verify_policy(&policy).is_err());
}
#[test]
fn decision_quorum_is_not_dual_proof_complete_qualification() {
    let (_, authority, mut proof, _) = initial();
    let decision = verify_formal_decision(&proof.decision, &authority, None, None).unwrap();
    proof.replica_complete.signature_hex = proof.primary_complete.signature_hex.clone();
    assert!(verify_formal_qualified_from_decision(&proof, &decision).is_err());
    proof.replica_complete.body.decision_proof_digest = h(91);
    assert!(verify_formal_qualified(&proof, &authority, None, None).is_err());
}
#[test]
fn signed_frozen_cannot_silently_change_input_governance_or_execution_policy() {
    let (_, authority, _, cut) = initial();
    for change_policy in [false, true] {
        let mut frozen = fixtures::frozen(&authority, &cut, None);
        if change_policy {
            frozen.reservation.body.execution_policy_digest = h(90);
        } else {
            frozen.reservation.body.governing_manifest_digest = h(90);
        }
        frozen.reservation.signature_hex =
            sign(frozen_signing_bytes(&frozen.reservation.body).unwrap(), 4);
        for (receipt, signer) in [(&mut frozen.primary, 5), (&mut frozen.replica, 6)] {
            receipt.body.reservation_id = frozen.reservation.body.reservation_id().unwrap();
            receipt.signature_hex =
                sign(frozen_receipt_signing_bytes(&receipt.body).unwrap(), signer);
        }
        assert!(
            frozen
                .verify(authority.definition(), authority.activation_ref())
                .is_ok()
        );
        assert!(verify_formal_reservation(&frozen.reservation, &authority, &cut).is_err());
        let record = execution(&authority, &frozen);
        let proof = proof(
            record,
            &authority,
            FormalCompletionBranchV1::Normal,
            None,
            Some(frozen),
            4,
            5,
            6,
        );
        assert!(verify_formal_qualified(&proof, &authority, Some(&cut), None).is_err());
    }
}
#[test]
fn logical_record_identity_survives_all_completion_envelopes() {
    for branch in [
        FormalCompletionBranchV1::CompletePrepared,
        FormalCompletionBranchV1::ReplayFrozen,
    ] {
        let (intent, authority, cut, frozen, record) = recovery_fixture_for(branch, None);
        let recovery = verified_recovery(&intent, &authority);
        let normal = proof(
            record.clone(),
            &authority,
            FormalCompletionBranchV1::Normal,
            None,
            Some(frozen.clone()),
            4,
            5,
            6,
        );
        let completed = proof(
            record.clone(),
            recovery.successor(),
            branch,
            Some(recovery.certificate_digest().into()),
            Some(frozen.clone()),
            if branch == FormalCompletionBranchV1::ReplayFrozen {
                10
            } else {
                4
            },
            11,
            12,
        );
        let a = verify_formal_qualified(&normal, &authority, Some(&cut), None).unwrap();
        let b =
            verify_formal_qualified(&completed, &authority, Some(&cut), Some(&recovery)).unwrap();
        assert_eq!(a.head(), b.head());
        assert_ne!(a.finalizing_authority_id(), b.finalizing_authority_id());
        if branch == FormalCompletionBranchV1::CompletePrepared {
            let next_frozen = fixtures::frozen(recovery.successor(), &a, None);
            let next_record = execution(recovery.successor(), &next_frozen);
            let next = proof(
                next_record,
                recovery.successor(),
                FormalCompletionBranchV1::Normal,
                None,
                Some(next_frozen),
                10,
                11,
                12,
            );
            let from_old =
                verify_formal_qualified(&next, recovery.successor(), Some(&a), None).unwrap();
            let from_recovery =
                verify_formal_qualified(&next, recovery.successor(), Some(&b), None).unwrap();
            assert_eq!(from_old.head(), from_recovery.head());
        }
        let mut wrong = completed.clone();
        wrong.decision.primary.body.branch = FormalCompletionBranchV1::Normal;
        assert!(verify_formal_qualified(&wrong, &authority, Some(&cut), Some(&recovery)).is_err());
        let mut downgrade = completed;
        downgrade.decision.branch = if branch == FormalCompletionBranchV1::CompletePrepared {
            FormalCompletionBranchV1::ReplayFrozen
        } else {
            FormalCompletionBranchV1::CompletePrepared
        };
        assert!(
            verify_formal_qualified(&downgrade, &authority, Some(&cut), Some(&recovery)).is_err()
        );
    }
}
#[test]
fn recovery_structural_boundaries_and_handoff_fresh_writes_fail_closed() {
    let (intent, authority, _, _, _) = recovery_fixture();
    for edit in 0..4 {
        let mut body = intent.body().clone();
        match edit {
            0 => body.successor.epoch = 3,
            1 => body.world.chain_id = "other".into(),
            2 => body.first_normal_position = Some(2),
            _ => {
                body.successor.primary.instance.storage_id =
                    authority.definition.primary.instance.storage_id.clone()
            }
        };
        assert!(
            recovery::validate_recovery_body(&body, authority.definition(), authority.policy())
                .is_err()
        );
    }
    let handoff = ProfileHandoffV1 {
        next_profile: "future_bft".into(),
        next_profile_version: 1,
        next_authority_digest: h(80),
        valid_from_position: 3,
    };
    let (intent, authority, cut, frozen, record) =
        recovery_fixture_for(FormalCompletionBranchV1::CompletePrepared, Some(handoff));
    assert_eq!(intent.body().first_normal_position, None);
    let recovery = verified_recovery(&intent, &authority);
    let completed = proof(
        record,
        recovery.successor(),
        FormalCompletionBranchV1::CompletePrepared,
        Some(recovery.certificate_digest().into()),
        Some(frozen),
        4,
        11,
        12,
    );
    let commit =
        verify_formal_qualified(&completed, &authority, Some(&cut), Some(&recovery)).unwrap();
    let mut next = completed;
    next.decision.record.position = 3;
    next.decision.record.original_authority_id = recovery.successor().authority_id().unwrap();
    next.decision.record.original_epoch = 2;
    next.decision.branch = FormalCompletionBranchV1::Normal;
    assert!(verify_formal_qualified(&next, recovery.successor(), Some(&commit), None).is_err());
}

#[test]
fn recovery_rejects_shadow_fencer_and_asymmetric_prepared_downgrade() {
    let (intent, authority, cut, frozen, record) =
        recovery_fixture_for(FormalCompletionBranchV1::CompletePrepared, None);
    let head = crate::controlled_authority::replicated_protocol::HeadAnchor {
        position: 1,
        decision_hash: cut.head().record_id,
        qualified: true,
    };
    let tail = fencing::SealedTailV1 {
        kind: fencing::SealedTailKindV1::Prepared,
        request: Some(crate::controlled_authority::LocalRequestIdentity {
            verified_subject: "writer".into(),
            operation_domain: "formal-execution".into(),
            nonce_scope: "world".into(),
            request_id: "request-2".into(),
        }),
        position: Some(2),
        frozen_input_digest: Some(frozen.reservation.body.reservation_id().unwrap()),
        frozen_certificate_digest: None,
        proposal_digest: Some(record.record_id().unwrap()),
        replica_prepare_digest: None,
        primary_receipt_digest: None,
        replica_receipt_digest: None,
    };
    let seal_body = |instance, tail| fencing::EpochSealBodyV1 {
        schema_version: 1,
        instance,
        recovery_operation: h(70),
        highest_complete: head.clone(),
        checkpoint_before: fencing::EndpointCheckpointBindingV1 {
            revision: 1,
            commitment: h(71),
            head: head.clone(),
        },
        tail,
        next_epoch_floor: 2,
    };
    let mut frozen_tail = tail.clone();
    frozen_tail.kind = fencing::SealedTailKindV1::Frozen;
    frozen_tail.proposal_digest = None;
    let primary = fencing::test_verified_seal(
        seal_body(authority.definition.primary.instance.clone(), frozen_tail),
        public(3),
    );
    let replica = fencing::test_verified_seal(
        seal_body(authority.definition.replica.instance.clone(), tail),
        public(3),
    );
    let candidate = SignedCanonicalCandidateV1 {
        writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), 4),
        record,
    };
    let witness = RecoveryTailWitnessV1::Prepared {
        frozen: frozen.clone(),
        candidate,
    };
    let mut body = intent.body().clone();
    body.fencing_evidence_digest = recovery_fencing_digest([&primary, &replica]).unwrap();
    assert!(
        VerifiedRecoveryIntent::verify(
            body.clone(),
            &authority,
            &cut,
            [&primary, &replica],
            &witness
        )
        .is_ok()
    );
    let coverage = VerifiedAppendFencingCoverageV1::derive_from_necessary_endpoint_seal(
        &authority, &cut, &replica,
    )
    .unwrap();
    let mut single_body = body.clone();
    single_body.fencing_evidence_digest = coverage.evidence_digest().unwrap();
    assert!(
        VerifiedRecoveryIntent::verify_single_survivor(
            single_body.clone(),
            &authority,
            &cut,
            &coverage,
            &witness
        )
        .is_ok()
    );
    single_body.tail = RecoveryTailV1::Frozen {
        position: 2,
        reservation_id: frozen.reservation.body.reservation_id().unwrap(),
        handoff: None,
    };
    assert!(
        VerifiedRecoveryIntent::verify_single_survivor(
            single_body,
            &authority,
            &cut,
            &coverage,
            &RecoveryTailWitnessV1::Frozen(frozen.clone())
        )
        .is_err()
    );
    let shadow = fencing::test_verified_seal(primary.seal_body().clone(), public(90));
    body.fencing_evidence_digest = recovery_fencing_digest([&shadow, &replica]).unwrap();
    assert_eq!(
        VerifiedRecoveryIntent::verify(
            body.clone(),
            &authority,
            &cut,
            [&shadow, &replica],
            &witness
        )
        .unwrap_err(),
        FormalError::FencingUnproven
    );
    body.fencing_evidence_digest = recovery_fencing_digest([&primary, &replica]).unwrap();
    body.tail = RecoveryTailV1::Frozen {
        position: 2,
        reservation_id: frozen.reservation.body.reservation_id().unwrap(),
        handoff: None,
    };
    let frozen_coverage = VerifiedAppendFencingCoverageV1::derive_from_necessary_endpoint_seal(
        &authority, &cut, &primary,
    )
    .unwrap();
    let mut replay_body = body.clone();
    replay_body.fencing_evidence_digest = frozen_coverage.evidence_digest().unwrap();
    assert!(
        VerifiedRecoveryIntent::verify_single_survivor(
            replay_body.clone(),
            &authority,
            &cut,
            &frozen_coverage,
            &RecoveryTailWitnessV1::Frozen(frozen.clone())
        )
        .is_ok()
    );
    // Permanent P loss: R's trusted current Frozen seal covers the necessary
    // dual-receipt append path without using any network-failure assertion.
    let mut survivor_body = primary.seal_body().clone();
    survivor_body.instance = authority.definition.replica.instance.clone();
    let survivor = fencing::test_verified_seal(survivor_body, public(3));
    let survivor_coverage = VerifiedAppendFencingCoverageV1::derive_from_necessary_endpoint_seal(
        &authority, &cut, &survivor,
    )
    .unwrap();
    replay_body.fencing_evidence_digest = survivor_coverage.evidence_digest().unwrap();
    let replay_intent = VerifiedRecoveryIntent::verify_single_survivor(
        replay_body,
        &authority,
        &cut,
        &survivor_coverage,
        &RecoveryTailWitnessV1::Frozen(frozen.clone()),
    )
    .unwrap();
    let replay = verified_recovery(&replay_intent, &authority);
    let record = match &witness {
        RecoveryTailWitnessV1::Prepared { candidate, .. } => candidate.record.clone(),
        _ => unreachable!(),
    };
    let replay_proof = proof(
        record,
        replay.successor(),
        FormalCompletionBranchV1::ReplayFrozen,
        Some(replay.certificate_digest().into()),
        Some(frozen.clone()),
        10,
        11,
        12,
    );
    assert!(verify_formal_qualified(&replay_proof, &authority, Some(&cut), Some(&replay)).is_ok());
    assert!(
        VerifiedRecoveryIntent::verify(
            body,
            &authority,
            &cut,
            [&primary, &replica],
            &RecoveryTailWitnessV1::Frozen(frozen)
        )
        .is_err()
    );
}

#[path = "business_history_recovery_tests.rs"]
mod business_history_recovery_tests;

#[path = "durable_set_tests.rs"]
mod durable_set_tests;

#[path = "snapshot_artifact_protocol_tests.rs"]
mod snapshot_artifact_protocol_tests;

#[test]
fn initial_native_root_requires_explicit_activation_schema() {
    use crate::controlled_authority::replicated_protocol::ArtifactRole;
    let (certificate, _, _, _) = initial();
    let mut body = certificate.body;
    body.validate().unwrap();
    body.artifact_roots
        .insert(ArtifactRole::NativeExecutionEnvironment, h(94));
    assert!(body.validate().is_err());
    body.schema_version = 2;
    body.validate().unwrap();
    body.artifact_roots
        .remove(&ArtifactRole::NativeExecutionEnvironment);
    assert!(body.validate().is_err());
}
