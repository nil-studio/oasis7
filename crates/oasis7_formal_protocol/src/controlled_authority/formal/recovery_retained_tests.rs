use super::*;
fn next_body(
    context: &VerifiedActiveRecoveryContextV2,
    frozen: &FormalFrozenCertificateV1,
    record: Option<&CanonicalRecordV1>,
) -> (RecoveryCertificateBodyV1, [fencing::FencingVerdict; 2]) {
    let active = context.active();
    let mut successor = active.definition().clone();
    successor.epoch += 1;
    successor.writer_public_key = public(13);
    successor.predecessor_authority_id = Some(active.authority_id().unwrap());
    for (binding, n) in [
        (&mut successor.primary, 14u8),
        (&mut successor.replica, 15u8),
    ] {
        binding.instance.authority_epoch = 3;
        binding.instance.endpoint_public_key = public(n);
        binding.instance.endpoint_id = format!("endpoint-{n}");
        binding.instance.instance_id = h(n + 10);
        binding.instance.storage_id = h(n + 20);
        binding.instance.storage_fingerprint = h(n + 30);
        binding.fault_domain_id = format!("domain-{n}");
    }
    let reservation = frozen.reservation.body.reservation_id().unwrap();
    let tail = if let Some(record) = record {
        RecoveryTailV1::Prepared {
            position: 2,
            reservation_id: reservation.clone(),
            record_id: record.record_id().unwrap(),
            handoff: None,
        }
    } else {
        RecoveryTailV1::Frozen {
            position: 2,
            reservation_id: reservation.clone(),
            handoff: None,
        }
    };
    let head = crate::controlled_authority::replicated_protocol::HeadAnchor {
        position: context.cut().head().position,
        decision_hash: context.cut().head().record_id,
        qualified: true,
    };
    let seal = |instance: &fencing::EndpointInstanceBindingV1| {
        fencing::test_verified_seal_with_resource(
            fencing::EpochSealBodyV1 {
                recovery_plan: None,
                writer_seal: None,
                schema_version: 1,
                instance: instance.clone(),
                recovery_operation: h(101),
                highest_complete: head.clone(),
                checkpoint_before: fencing::EndpointCheckpointBindingV1 {
                    revision: 1,
                    commitment: h(102),
                    head: head.clone(),
                },
                tail: fencing::SealedTailV1 {
                    kind: if record.is_some() {
                        fencing::SealedTailKindV1::Prepared
                    } else {
                        fencing::SealedTailKindV1::Frozen
                    },
                    request: Some(crate::controlled_authority::LocalRequestIdentity {
                        verified_subject: "formal-request-digest".into(),
                        operation_domain: "oasis7.formal.request.v1".into(),
                        nonce_scope: active.authority_id().unwrap(),
                        request_id: frozen.reservation.body.request_digest.clone(),
                    }),
                    position: Some(2),
                    frozen_input_digest: Some(reservation.clone()),
                    frozen_certificate_digest: None,
                    proposal_digest: record.map(|record| record.record_id().unwrap()),
                    replica_prepare_digest: None,
                    primary_receipt_digest: None,
                    replica_receipt_digest: None,
                },
                next_epoch_floor: 3,
            },
            public(3),
            active.policy().authorized_fencing_resource_bindings[0].clone(),
        )
    };
    let seals = [
        seal(&active.definition().primary.instance),
        seal(&active.definition().replica.instance),
    ];
    let body = RecoveryCertificateBodyV1 {
        schema_version: 2,
        world: active.definition().world.clone(),
        operation_id: h(101),
        predecessor_authority_id: active.authority_id().unwrap(),
        predecessor_epoch: 2,
        sealed_cut: context.cut().head(),
        tail,
        fencing_evidence_digest: recovery_fencing_digest([&seals[0], &seals[1]]).unwrap(),
        successor,
        first_normal_position: Some(3),
        retained_context: Some(context.commitment().clone()),
    };
    (body, seals)
}
#[test]
fn consecutive_recovery_retains_original_prepared_record_and_rejects_fake_context() {
    let (intent2, original, cut, frozen, record) =
        recovery_fixture_for(FormalCompletionBranchV1::CompletePrepared, None);
    let recovery2 = verified_recovery(&intent2, &original);
    let candidate = SignedCanonicalCandidateV1 {
        record: record.clone(),
        writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), 4),
    };
    let witness = RecoveryTailWitnessV1::Prepared {
        frozen: frozen.clone(),
        candidate: candidate.clone(),
    };
    let context =
        VerifiedActiveRecoveryContextV2::from_previous(&recovery2, &original, &cut, &witness)
            .unwrap();
    assert!(
        VerifiedActiveRecoveryContextV2::from_previous(
            &recovery2,
            recovery2.successor(),
            &cut,
            &witness
        )
        .is_err()
    );
    let (body, seals) = next_body(&context, &frozen, Some(&record));
    assert!(
        VerifiedRecoveryIntent::verify(
            body.clone(),
            recovery2.successor(),
            &cut,
            [&seals[0], &seals[1]],
            &witness
        )
        .is_err()
    );
    let mut changed = body.clone();
    changed
        .retained_context
        .as_mut()
        .unwrap()
        .previous_recovery_certificate_digest = h(103);
    assert!(
        VerifiedRecoveryIntent::verify_retained(changed, &context, [&seals[0], &seals[1]]).is_err()
    );
    let intent3 =
        VerifiedRecoveryIntent::verify_retained(body, &context, [&seals[0], &seals[1]]).unwrap();
    let recovery3 = verified_recovery(&intent3, recovery2.successor());
    let qualified = proof(
        record.clone(),
        recovery3.successor(),
        FormalCompletionBranchV1::CompletePrepared,
        Some(recovery3.certificate_digest().into()),
        Some(frozen),
        4,
        14,
        15,
    );
    let verified =
        verify_formal_qualified(&qualified, &original, Some(&cut), Some(&recovery3)).unwrap();
    assert_eq!(verified.head().record_id, record.record_id().unwrap());
    assert_eq!(
        qualified.decision.writer_signature_hex,
        candidate.writer_signature_hex
    );
}
#[test]
fn consecutive_recovery_replays_frozen_with_new_writer_or_retains_previous_replay_candidate() {
    let (intent2, original, cut, frozen, record) =
        recovery_fixture_for(FormalCompletionBranchV1::ReplayFrozen, None);
    let recovery2 = verified_recovery(&intent2, &original);
    let context = VerifiedActiveRecoveryContextV2::from_previous(
        &recovery2,
        &original,
        &cut,
        &RecoveryTailWitnessV1::Frozen(frozen.clone()),
    )
    .unwrap();
    let (body, seals) = next_body(&context, &frozen, None);
    let coverage =
        VerifiedAppendFencingCoverageV1::derive_from_active_recovery_context(&context, &seals[1])
            .unwrap();
    let mut single = body.clone();
    single.fencing_evidence_digest = coverage.evidence_digest().unwrap();
    let intent3 =
        VerifiedRecoveryIntent::verify_retained_single_survivor(single, &context, &coverage)
            .unwrap();
    let recovery3 = verified_recovery(&intent3, recovery2.successor());
    let replay = proof(
        record.clone(),
        recovery3.successor(),
        FormalCompletionBranchV1::ReplayFrozen,
        Some(recovery3.certificate_digest().into()),
        Some(frozen.clone()),
        13,
        14,
        15,
    );
    verify_formal_qualified(&replay, &original, Some(&cut), Some(&recovery3)).unwrap();
    let candidate2 = SignedCanonicalCandidateV1 {
        record: record.clone(),
        writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), 10),
    };
    let witness = RecoveryTailWitnessV1::Prepared {
        frozen: frozen.clone(),
        candidate: candidate2.clone(),
    };
    let upgraded =
        VerifiedActiveRecoveryContextV2::from_previous(&recovery2, &original, &cut, &witness)
            .unwrap();
    let (body, seals) = next_body(&upgraded, &frozen, Some(&record));
    let intent3 =
        VerifiedRecoveryIntent::verify_retained(body, &upgraded, [&seals[0], &seals[1]]).unwrap();
    let recovery3 = verified_recovery(&intent3, recovery2.successor());
    let retained = proof(
        record.clone(),
        recovery3.successor(),
        FormalCompletionBranchV1::CompletePrepared,
        Some(recovery3.certificate_digest().into()),
        Some(frozen.clone()),
        10,
        14,
        15,
    );
    verify_formal_qualified(&retained, &original, Some(&cut), Some(&recovery3)).unwrap();
    let replaced = proof(
        record,
        recovery3.successor(),
        FormalCompletionBranchV1::CompletePrepared,
        Some(recovery3.certificate_digest().into()),
        Some(frozen),
        4,
        14,
        15,
    );
    assert!(verify_formal_qualified(&replaced, &original, Some(&cut), Some(&recovery3)).is_err());
}
