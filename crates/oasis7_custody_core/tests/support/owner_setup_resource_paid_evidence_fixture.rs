//! Genuine Core opaque/issuer and Node component originals, not real CF payment.
use super::*;
use ed25519_dalek::Signer;
pub(super) fn result(
    state: &AccountProvisionApprovalState,
    opaque: PreparedOwnerSetupResourceDispatchV2,
    issuer: &SigningKey,
    final_response: resource::SignedResourceResponseV1<
        resource::OwnerSetupFinalResourceObservationV2,
    >,
) -> OwnerSetupPaidResourceResultV2 {
    let payload = opaque.payload().clone();
    let signature = issuer.sign(&payload.signing_bytes().unwrap()).to_bytes();
    let proof = PaidResourceReadProofV1 {
        payload,
        signature_hex: signature.iter().map(|b| format!("{b:02x}")).collect(),
    };
    proof
        .verify_issuer(
            opaque.issuer_pin(),
            &PaidResourceReadIssuerFloorV1 {
                issuer_epoch: opaque.issuer_pin().issuer_epoch,
                config_generation: opaque.issuer_pin().issuer_config_generation,
            },
            proof.payload.issued_at.0,
        )
        .unwrap();
    let frame = opaque.into_http(proof.clone()).unwrap();
    let preflight = state
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
        .unwrap()
        .world
        .as_ref()
        .unwrap()
        .response
        .clone();
    preflight.payload.validate(&frame.request().access).unwrap();
    let bind = frame
        .paid_world_reply_binding(resource::ResourcePaidWorldSubstepV1::OwnerPreflight)
        .unwrap();
    let preflight = ws::sign_service_response(
        resource::RESOURCE_PAID_WORLD_REPLY_DOMAIN,
        bind.request_digest().unwrap(),
        resource::ResourcePaidWorldReplyV1::OwnerPreflight {
            binding: bind,
            observation: Box::new(preflight),
        },
        &"3f".repeat(32),
    )
    .unwrap();
    let bind = frame
        .paid_world_reply_binding(resource::ResourcePaidWorldSubstepV1::ResourceFinal)
        .unwrap();
    let resource_final = ws::sign_service_response(
        resource::RESOURCE_PAID_WORLD_REPLY_DOMAIN,
        bind.request_digest().unwrap(),
        resource::ResourcePaidWorldReplyV1::ResourceFinal {
            binding: bind,
            observation: Box::new(final_response),
        },
        &"3f".repeat(32),
    )
    .unwrap();
    let result = OwnerSetupPaidResourceResultV2 {
        api_version: 1,
        paid_proof: proof,
        world_evidence: resource::PaidOwnerSetupResourceEnvelopeV1 {
            api_version: 1,
            ok: true,
            preflight,
            resource_final,
        },
    };
    result
        .verify_frame(
            &frame,
            &state
                .owner_setup_v2_plan()
                .unwrap()
                .world_identity
                .service_public_key,
        )
        .unwrap();
    result
}

#[test]
fn owner_resource_v2_paid_reply_closed_decoders_reject_known_duplicates() {
    let (mut state, context, pin, canonical, candidate) = resource_published();
    let policy = Policy::new(state.draft());
    let current = policy.current(&context);
    let native = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native,
    };
    let key = SigningKey::from_bytes(&[43; 32]);
    let issuer = PaidResourceReadIssuerPinV1 {
        namespace: "custody:test".into(),
        service_id: "account".into(),
        environment: state.draft().entry.environment.clone(),
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
        native_audience_origin: native.http_origin.into(),
        world_audience: state.owner_setup_v2_plan().unwrap().world_identity.clone(),
        allowed_stage: PaidResourceReadStageV1::OwnerSetupFinal,
    };
    let opaque = state
        .claim_owner_setup_resource_dispatch_v2(
            "setup-one",
            &ResourceFinalIssueContextV2 {
                issuer: &issuer,
                resource_feature_generation: DecimalU64(1),
            },
            &trusted,
            2111,
            &mut StrictCrypto,
        )
        .unwrap();
    let response = fact_fixture::response(&state, &canonical, &candidate.original, false);
    let evidence = result(&state, opaque, &key, response);
    let wire = evidence.encode().unwrap();
    OwnerSetupPaidResourceResultV2::decode(&wire).unwrap();
    let reply = &evidence.world_evidence.preflight;
    let json = serde_json::to_string(reply).unwrap();
    resource::decode_paid_resource_world_reply(json.as_bytes()).unwrap();
    let digest = serde_json::to_string(&reply.request_digest).unwrap();
    let duplicate = format!("{{\"request_digest\":{digest},{}", &json[1..]);
    assert!(
        resource::decode_paid_resource_world_reply(duplicate.as_bytes()).is_err(),
        "closed paid Node reply must reject known duplicate outer fields after genuine strong originals passed"
    );
    let binding = reply.payload.binding();
    let duplicate = json.replacen(
        "\"binding\":{",
        &format!(
            "\"binding\":{{\"proof_digest\":{},",
            serde_json::to_string(&binding.proof_digest).unwrap()
        ),
        1,
    );
    assert!(resource::decode_paid_resource_world_reply(duplicate.as_bytes()).is_err());
    let resource::ResourcePaidWorldReplyV1::OwnerPreflight { observation, .. } = &reply.payload
    else {
        unreachable!()
    };
    let duplicate = json.replacen(
        "\"observation\":{",
        &format!(
            "\"observation\":{{\"request_digest\":{},",
            serde_json::to_string(&observation.request_digest).unwrap()
        ),
        1,
    );
    assert!(resource::decode_paid_resource_world_reply(duplicate.as_bytes()).is_err());
    let json = String::from_utf8(evidence.world_evidence.encode().unwrap()).unwrap();
    resource::PaidOwnerSetupResourceEnvelopeV1::decode(json.as_bytes()).unwrap();
    let duplicate = format!("{{\"api_version\":1,{}", &json[1..]);
    assert!(resource::PaidOwnerSetupResourceEnvelopeV1::decode(duplicate.as_bytes()).is_err());
    let json = String::from_utf8(wire).unwrap();
    let duplicate = format!("{{\"api_version\":1,{}", &json[1..]);
    assert!(OwnerSetupPaidResourceResultV2::decode(duplicate.as_bytes()).is_err());
}
