//! Strong old result path, deliberately no new result-callback API invoked.
use super::*;
#[test]
fn owner_resource_v2_complete_requires_independent_result_callback_before_acceptance() {
    let (mut s, c, pin, canonical, candidate) = resource_published();
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    let key = SigningKey::from_bytes(&[43; 32]);
    let issuer = PaidResourceReadIssuerPinV1 {
        namespace: "custody:test".into(),
        service_id: "account".into(),
        environment: s.draft().entry.environment.clone(),
        key_id: "resource-issuer".into(),
        public_key: key
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        issuer_epoch: DecimalU64(2),
        issuer_config_generation: DecimalU64(3),
        not_before: DecimalU64(1),
        not_after: DecimalU64(10000),
        native_audience_origin: ns.http_origin.into(),
        world_audience: s.owner_setup_v2_plan().unwrap().world_identity.clone(),
        allowed_stage: PaidResourceReadStageV1::OwnerSetupFinal,
    };
    let config = ResourceFinalIssueContextV2 {
        issuer: &issuer,
        resource_feature_generation: DecimalU64(1),
    };
    let opaque = s
        .claim_owner_setup_resource_dispatch_v2(
            "setup-one",
            &config,
            &trusted,
            2111,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(opaque.payload().native_http_units, DecimalU64(1));
    assert_eq!(opaque.payload().world_http_units, DecimalU64(2));
    let response = fact_fixture::response(&s, &canonical, &candidate.original, true);
    resource::verify_resource_goal_response(
        &candidate.original,
        &response,
        &s.owner_setup_v2_plan()
            .unwrap()
            .world_identity
            .service_public_key,
    )
    .unwrap();
    let outbox = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
        .unwrap();
    assert!(
        outbox.claimed_at.is_none() && outbox.callback_claimed_at.is_none(),
        "Resource cannot borrow old common once fields"
    );
    let session = fixture::root_session(&s, outbox.session_nonce.clone(), 2112);
    let participant = s.participant_original("participant-one").unwrap();
    session
        .status
        .status
        .validate_for(
            &participant.descriptor.native_binding,
            &outbox.session_nonce,
            participant.descriptor.session.native_binding_generation,
            c.authority.service_epoch,
            2112,
        )
        .unwrap();
    assert!(session.status.key_id == ns.native_root_key_id);
    let public: [u8; 32] = std::array::from_fn(|i| {
        u8::from_str_radix(&ns.native_root_public_key[i * 2..i * 2 + 2], 16).unwrap()
    });
    use base64::Engine;
    let signature = ed25519_dalek::Signature::from_slice(
        &URL_SAFE_NO_PAD.decode(&session.status.signature).unwrap(),
    )
    .unwrap();
    ed25519_dalek::VerifyingKey::from_bytes(&public)
        .unwrap()
        .verify_strict(&session.status.status.signing_bytes().unwrap(), &signature)
        .unwrap();
    StrictCrypto
        .verify(
            &s.owner_setup_stored_admission_checks(
                "setup-one",
                OwnerSetupStageV1::FinalObservation,
                &trusted,
                2112,
            )
            .unwrap(),
        )
        .unwrap();
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert!(cold == s);
    let before = serde_json::to_value(&s).unwrap();
    assert!(
        s.complete_owner_setup_resource_final_v2(
            "setup-one",
            CompleteOwnerSetupResourceFinalV2 { response, session },
            &trusted,
            2112,
            &mut StrictCrypto
        )
        .is_err(),
        "strong final result must not accept without its independent Resource result callback claim"
    );
    assert!(serde_json::to_value(&s).unwrap() == before);
}
