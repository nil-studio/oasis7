use super::*;
use crate::execution_bridge_real_tests::real_execution_bridge::governance_authorization_tests as governance;

#[allow(clippy::too_many_arguments)]
fn prepared_recovery_evidence(
    cut: &VerifiedFormalReadAnchorV1,
    trust: &InitialActivationTrustV1,
    epoch: u64,
    next_signers: [u8; 3],
    predecessor_sealers: [u8; 2],
    prior: Option<&VerifiedRecoveryBootstrapV1>,
    floor: &issuer::IssuerAnchorV1,
    candidate_signers: [u8; 3],
    salt: u8,
) -> (
    RecoveryPermissionBootstrapV1,
    VerifiedRecoveryBootstrapV1,
    FormalQualifiedCommitProofV1,
) {
    let proof = normal_execution_proof(cut.authority(), cut.head(), salt, candidate_signers);
    let frozen = proof.decision.frozen.clone().unwrap();
    let reservation_id = frozen.reservation.body.reservation_id().unwrap();
    let record_id = proof.decision.record.record_id().unwrap();
    let candidate = SignedCanonicalCandidateV1 {
        record: proof.decision.record.clone(),
        writer_signature_hex: proof.decision.writer_signature_hex.clone(),
    };
    let witness = RecoveryTailWitnessV1::Prepared { frozen, candidate };
    let sealed_tail = SealedTailV1 {
        kind: SealedTailKindV1::Prepared,
        request: Some(LocalRequestIdentity {
            verified_subject: "formal-request-digest".into(),
            operation_domain: "oasis7.formal.request.v1".into(),
            nonce_scope: cut.authority().authority_id().unwrap(),
            request_id: proof.decision.record.request_digest.clone(),
        }),
        position: Some(proof.decision.record.position),
        frozen_input_digest: Some(reservation_id),
        frozen_certificate_digest: None,
        proposal_digest: Some(record_id),
        replica_prepare_digest: None,
        primary_receipt_digest: None,
        replica_receipt_digest: None,
    };
    let (raw, verified) = permission_evidence_with_tail(
        cut,
        trust,
        epoch,
        next_signers,
        predecessor_sealers,
        witness,
        sealed_tail,
        prior,
        floor,
    );
    (raw, verified, proof)
}

fn resolver_for_startup_history_tests(
    origin: &VerifiedFormalReadAnchorV1,
    permissions: &[VerifiedRecoveryBootstrapV1],
) -> NativeHistoryResolver {
    struct EmptyNodes;
    impl BusinessIndexNodeReader for EmptyNodes {
        fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
    }

    let latest = permissions.last();
    let trusted = latest.map_or_else(
        || origin.clone(),
        |permission| {
            permission
                .external_anchor()
                .with_pending_recovery(permission.recovery())
                .unwrap()
        },
    );
    NativeHistoryResolver {
        addresses: ["127.0.0.1:1".parse().unwrap(); 2],
        timeout: Duration::from_secs(2),
        authentication: fixtures::key(4),
        external_origin: origin.clone(),
        trusted,
        permission: latest.cloned(),
        active: latest.map_or_else(
            || origin.authority().clone(),
            |permission| permission.recovery().successor().clone(),
        ),
        permissions: permissions.to_vec(),
        release: ReleaseSecurityPolicy::default(),
        business_reader: std::sync::Arc::new(EmptyNodes),
        writer_directory: std::path::PathBuf::new(),
        origin_history_directory: None,
        activation_markers: durable_set_activation::DurableSetActivationMarkerStoreV1::new(
            std::path::PathBuf::new(),
        ),
        prewarm_scope: std::sync::Arc::new(()),
        governance_verified: std::sync::Mutex::new(None),
    }
}

#[test]
fn startup_pending_prepared_recovery_uses_verified_c0_and_preserves_completed_writer() {
    let (_world, origin, _, _, initial, trust) = governance::history_origin_with_trust(1);
    let floor = issuer::IssuerAnchorV1::initial(origin.authority().policy()).unwrap();
    let (raw, permission, proof) = prepared_recovery_evidence(
        &origin,
        &trust,
        2,
        [14, 15, 16],
        [5, 6],
        None,
        &floor,
        [4, 5, 6],
        111,
    );
    let pending_writer = permission
        .external_anchor()
        .with_pending_recovery(permission.recovery())
        .unwrap();
    let verified_base =
        super::super::super::formal_live_runtime::startup_governance_history_anchor(
            &pending_writer,
            Some(&permission),
            &trust,
            &initial,
        )
        .unwrap();
    assert_eq!(verified_base.checkpoint(), origin.checkpoint());
    assert_eq!(verified_base.head().head(), origin.head().head());
    assert_eq!(
        verified_base.authority().authority_id().unwrap(),
        origin.authority().authority_id().unwrap()
    );

    // Exercise the production verifier against the returned qualified base.
    resolver_for_startup_history_tests(&origin, std::slice::from_ref(&permission))
        .verify_governance_history(&verified_base)
        .unwrap();

    // A writer that has already qualified the retained original is a normal
    // descendant and must stay on its exact ordinary replay path.
    assert!(matches!(
        raw.certificate.body.tail,
        RecoveryTailV1::Prepared { .. }
    ));
    let completed = advance_history_completion(&origin, &permission, &proof).unwrap();
    assert!(completed.head().record().position > permission.cut().record().position);
    let completed_base =
        super::super::super::formal_live_runtime::startup_governance_history_anchor(
            &completed,
            Some(&permission),
            &trust,
            &initial,
        )
        .unwrap();
    assert_eq!(completed_base.checkpoint(), completed.checkpoint());
}

#[test]
fn startup_pending_retained_schema2_chain_resolves_to_oldest_qualified_predecessor() {
    let (_world, origin, _, _, initial, trust) = governance::history_origin_with_trust(1);
    let floor = issuer::IssuerAnchorV1::initial(origin.authority().policy()).unwrap();
    let (first_raw, first) =
        permission_evidence_at_cut(&origin, &trust, 2, [14, 15, 16], [5, 6], None, &floor);
    let active_predecessor = origin.with_empty_recovery(first.recovery()).unwrap();
    let retained_floor = first_raw.issuer_witness.body.current_anchor.clone();
    let (_second_raw, second, _) = prepared_recovery_evidence(
        &active_predecessor,
        &trust,
        3,
        [24, 25, 26],
        [15, 16],
        Some(&first),
        &retained_floor,
        [14, 15, 16],
        112,
    );
    assert_eq!(second.recovery().body().schema_version, 2);
    let pending_writer = second
        .external_anchor()
        .with_pending_recovery(second.recovery())
        .unwrap();
    let verified_base =
        super::super::super::formal_live_runtime::startup_governance_history_anchor(
            &pending_writer,
            Some(&second),
            &trust,
            &initial,
        )
        .unwrap();
    assert_eq!(verified_base.checkpoint(), origin.checkpoint());
    assert_eq!(
        verified_base.authority().authority_id().unwrap(),
        origin.authority().authority_id().unwrap()
    );
    resolver_for_startup_history_tests(&origin, &[first, second])
        .verify_governance_history(&verified_base)
        .unwrap();
}

#[test]
fn startup_pending_recovery_rejects_same_head_wrong_permission_context_and_earlier_writer() {
    let (_world, origin, _, _, initial, trust) = governance::history_origin_with_trust(1);
    let floor = issuer::IssuerAnchorV1::initial(origin.authority().policy()).unwrap();
    let (_raw_a, permission_a, _) = prepared_recovery_evidence(
        &origin,
        &trust,
        2,
        [14, 15, 16],
        [5, 6],
        None,
        &floor,
        [4, 5, 6],
        113,
    );
    let (_raw_b, permission_b, _) = prepared_recovery_evidence(
        &origin,
        &trust,
        2,
        [24, 25, 26],
        [5, 6],
        None,
        &floor,
        [4, 5, 6],
        114,
    );
    let wrong_context_writer = permission_b
        .external_anchor()
        .with_pending_recovery(permission_b.recovery())
        .unwrap();
    assert_eq!(
        wrong_context_writer.head().head(),
        permission_a.cut().head(),
        "negative case keeps the same sealed position and record"
    );
    assert!(
        super::super::super::formal_live_runtime::startup_governance_history_anchor(
            &wrong_context_writer,
            Some(&permission_a),
            &trust,
            &initial,
        )
        .unwrap_err()
        .contains("restored writer authority differs from recovery successor")
    );

    let later_proof = normal_execution_proof(origin.authority(), origin.head(), 115, [4, 5, 6]);
    let later_cut = advance_normal_history_proof(&origin, &later_proof).unwrap();
    let (later_raw, later_permission) =
        permission_evidence_at_cut(&later_cut, &trust, 2, [34, 35, 36], [5, 6], None, &floor);
    assert_eq!(later_raw.certificate.body.tail, RecoveryTailV1::Empty);
    assert_eq!(
        origin.head().record().position + 1,
        later_permission.cut().record().position
    );
    assert!(
        super::super::super::formal_live_runtime::startup_governance_history_anchor(
            &origin,
            Some(&later_permission),
            &trust,
            &initial,
        )
        .unwrap_err()
        .contains("restored writer precedes its recovery cut")
    );
}
