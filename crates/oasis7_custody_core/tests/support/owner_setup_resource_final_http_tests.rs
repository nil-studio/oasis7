//! Core opaque dispatch→exact signed frame only; not a Native payer fixture.
use super::*;
use ed25519_dalek::Signer;
#[test]
fn owner_resource_v2_paid_http_frame_uses_actual_opaque_dispatch_and_full_body_bound() {
    let (mut s, c, pin, _, _) = resource_published();
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
    let payload = opaque.payload().clone();
    let signature = key
        .sign(&payload.signing_bytes().unwrap())
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let proof = PaidResourceReadProofV1 {
        payload,
        signature_hex: signature,
    };
    proof
        .verify_issuer(
            &issuer,
            &PaidResourceReadIssuerFloorV1 {
                issuer_epoch: DecimalU64(2),
                config_generation: DecimalU64(3),
            },
            2111,
        )
        .unwrap();
    let frame = opaque.into_http(proof).unwrap();
    frame.validate_bindings().unwrap();
    let bytes = frame.body_bytes().unwrap();
    let header = frame.proof_header().unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(body.get("paid").is_none());
    assert!(body.get("participant").is_some());
    let decoded = OwnerSetupResourceFinalHttpV2::decode_http(&bytes, &header).unwrap();
    assert_eq!(decoded.body_bytes().unwrap(), bytes);
    assert_eq!(decoded.proof_header().unwrap(), header);
    decoded
        .world_body()
        .unwrap()
        .validate_bindings(&decoded.paid().payload)
        .unwrap();
    let mut unknown = body.clone();
    unknown["authority"] = json!(true);
    assert!(
        OwnerSetupResourceFinalHttpV2::decode_http(&serde_json::to_vec(&unknown).unwrap(), &header)
            .is_err()
    );
    let mut changed = body;
    changed["participant"]["session"]["session_ref"] = json!("foreign-session");
    assert!(
        OwnerSetupResourceFinalHttpV2::decode_http(&serde_json::to_vec(&changed).unwrap(), &header)
            .is_err()
    );
    assert!(OwnerSetupResourceFinalHttpV2::decode_http(&bytes, &format!("{header}=")).is_err());
    let proof_json = serde_json::to_vec(decoded.paid()).unwrap();
    let mut noncanonical = vec![b' '];
    noncanonical.extend(proof_json);
    use base64::Engine;
    let noncanonical = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(noncanonical);
    assert!(OwnerSetupResourceFinalHttpV2::decode_http(&bytes, &noncanonical).is_err());
}
