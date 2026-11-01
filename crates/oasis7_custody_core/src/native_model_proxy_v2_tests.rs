//! Canonical codec/strong-signature components only, not runtime graph/price authority.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn fixture() -> (
    NativeModelAccountFloorV2,
    AccountMachineIssuerPinV2,
    AccountMachineIssuerFloorV2,
) {
    let issuer = AccountMachineIssuerV2 {
        namespace: "account-issuer".into(),
        service_id: "account-service".into(),
        environment: "test".into(),
        key_id: "account-key".into(),
        issuer_epoch: DecimalU64(7),
        config_generation: DecimalU64(4),
        native_audience_origin: "https://native.example".into(),
    };
    let pin = AccountMachineIssuerPinV2 {
        namespace: issuer.namespace.clone(),
        service_id: issuer.service_id.clone(),
        environment: issuer.environment.clone(),
        key_id: issuer.key_id.clone(),
        public_key: hex(key(31).verifying_key().as_bytes()),
        issuer_epoch: issuer.issuer_epoch,
        config_generation: issuer.config_generation,
        not_before: DecimalU64(1),
        not_after: DecimalU64(20000),
        native_audience_origin: issuer.native_audience_origin.clone(),
        allowed_purpose: AccountMachinePurposeV2::NativeModelReservationV2,
    };
    let edge = NativeModelReservedEdgeV2 {
        api_version: 2,
        custody_account_id: "custody-account".into(),
        native_account_id: "native-owner".into(),
        session_id: "session".into(),
        approved_input_digest: "11".repeat(32),
        budget_digest: "22".repeat(32),
        reservation_id: "reservation".into(),
        edge_id: "provider-1".into(),
        call_kind: NativeModelCallKindV2::Inference,
        graph_digest: "33".repeat(32),
        provider_request_digest: "34".repeat(32),
        price_policy_digest: "66".repeat(32),
        native_instance_public_key: hex(key(91).verifying_key().as_bytes()),
        connection_ref_digest: "44".repeat(32),
        connection_generation: DecimalU64(1),
        preset_id: "fixed".into(),
        preset_revision: DecimalU64(1),
        model: "fixed-model".into(),
        ordinal: DecimalU64(3),
        floor: DecimalU64(1),
        max_input_tokens: DecimalU64(100),
        max_output_tokens: DecimalU64(50),
        max_tokens: DecimalU64(150),
        dispatch_until: DecimalU64(9000),
        completion_until: DecimalU64(12000),
    };
    (
        NativeModelAccountFloorV2 {
            api_version: 2,
            issuer,
            edge,
            query_nonce: base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                [55u8; 32],
            ),
            observed_at: DecimalU64(1000),
            expires_at: DecimalU64(5000),
        },
        pin,
        AccountMachineIssuerFloorV2 {
            issuer_epoch: DecimalU64(7),
            config_generation: DecimalU64(4),
        },
    )
}
fn sign(fact: NativeModelAccountFloorV2) -> SignedNativeModelAccountFloorV2 {
    let signature_hex = hex(&key(31).sign(&fact.signing_bytes().unwrap()).to_bytes());
    SignedNativeModelAccountFloorV2 {
        fact,
        signature_hex,
    }
}
#[test]
fn native_model_shared_floor_real_signature_enforces_configured_issuer_purpose_and_time() {
    let (fact, pin, floor) = fixture();
    let signed = sign(fact.clone());
    signed.verify(&pin, &floor, 1001).unwrap();
    let json = serde_json::to_value(&signed).unwrap();
    let restored: SignedNativeModelAccountFloorV2 = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(restored, signed);
    assert!(signed.verify(&pin, &floor, 5000).is_err());
    let mut wrong_pin = pin.clone();
    wrong_pin.allowed_purpose = AccountMachinePurposeV2::ConsumedPending;
    assert!(signed.verify(&wrong_pin, &floor, 1001).is_err());
    let mut old = fact;
    old.issuer.issuer_epoch = DecimalU64(6);
    let old = sign(old);
    key(31)
        .verifying_key()
        .verify_strict(
            &old.fact.signing_bytes().unwrap(),
            &ed25519_dalek::Signature::from_bytes(
                &crate::native_cleanup_v2::decode::<64>(&old.signature_hex).unwrap(),
            ),
        )
        .unwrap();
    assert!(old.verify(&pin, &floor, 1001).is_err());
    let mut unknown = json;
    unknown["fact"]["caller_verified"] = serde_json::json!(true);
    assert!(serde_json::from_value::<SignedNativeModelAccountFloorV2>(unknown).is_err());
}
fn usage(edge: &NativeModelReservedEdgeV2) -> AuthenticatedProviderUsageV2 {
    AuthenticatedProviderUsageV2 {
        api_version: 2,
        approved_input_digest: edge.approved_input_digest.clone(),
        budget_digest: edge.budget_digest.clone(),
        reservation_id: edge.reservation_id.clone(),
        edge_id: edge.edge_id.clone(),
        call_kind: edge.call_kind,
        ordinal: edge.ordinal,
        floor: edge.floor,
        native_account_id: edge.native_account_id.clone(),
        native_instance_public_key: edge.native_instance_public_key.clone(),
        preset_id: edge.preset_id.clone(),
        preset_revision: edge.preset_revision,
        model: edge.model.clone(),
        price_policy_digest: edge.price_policy_digest.clone(),
        input_tokens: DecimalU64(15),
        output_tokens: DecimalU64(20),
        total_tokens: DecimalU64(35),
        observed_at: DecimalU64(13000),
        protocol_result: NativeModelProtocolResultV2::InferenceComplete,
    }
}
fn signed_usage(
    usage: AuthenticatedProviderUsageV2,
    k: &SigningKey,
) -> SignedAuthenticatedProviderUsageV2 {
    let signature_hex = hex(&k.sign(&usage.signing_bytes().unwrap()).to_bytes());
    SignedAuthenticatedProviderUsageV2 {
        usage,
        signature_hex,
    }
}
#[test]
fn native_model_tls_usage_requires_actual_n_key_and_exact_reserved_scope_not_m_totals() {
    let (fact, _, _) = fixture();
    let edge = fact.edge;
    let good = usage(&edge);
    // A late authenticated fact may be archived; it is never a Current permit.
    let signed = signed_usage(good.clone(), &key(91));
    signed.verify_edge(&edge).unwrap();
    assert!(
        signed_usage(good.clone(), &key(92))
            .verify_edge(&edge)
            .is_err()
    );
    for field in 0..5 {
        let mut changed = good.clone();
        match field {
            0 => changed.reservation_id = "other".into(),
            1 => changed.price_policy_digest = "ff".repeat(32),
            2 => {
                changed.input_tokens = DecimalU64(101);
                changed.total_tokens = DecimalU64(121);
            }
            3 => changed.floor = DecimalU64(2),
            _ => changed.protocol_result = NativeModelProtocolResultV2::FirstProbeEcho,
        }
        let s = signed_usage(changed, &key(91));
        key(91)
            .verifying_key()
            .verify_strict(
                &s.usage.signing_bytes().unwrap(),
                &ed25519_dalek::Signature::from_bytes(
                    &crate::native_cleanup_v2::decode::<64>(&s.signature_hex).unwrap(),
                ),
            )
            .unwrap();
        assert!(s.verify_edge(&edge).is_err());
    }
    let mut amount = serde_json::to_value(&signed).unwrap();
    amount["usage"]["vendor_amount_usd"] = serde_json::json!(0);
    assert!(serde_json::from_value::<SignedAuthenticatedProviderUsageV2>(amount).is_err());
}

#[test]
fn native_model_shared_floor_canonical_draft_zero_is_distinct_from_nonzero_issuer_epochs() {
    let (mut fact, pin, floor) = fixture();
    fact.edge.connection_generation = DecimalU64(0);
    let signed = sign(fact);
    signed.verify(&pin, &floor, 1001).unwrap();
    let raw = serde_json::to_value(&signed).unwrap();
    assert_eq!(
        raw["fact"]["edge"]["connection_generation"],
        serde_json::json!("0")
    );
    for value in [serde_json::json!(0), serde_json::json!("00")] {
        let mut changed = raw.clone();
        changed["fact"]["edge"]["connection_generation"] = value;
        assert!(serde_json::from_value::<SignedNativeModelAccountFloorV2>(changed).is_err());
    }
    let mut missing = raw;
    missing["fact"]["edge"]
        .as_object_mut()
        .unwrap()
        .remove("connection_generation");
    assert!(serde_json::from_value::<SignedNativeModelAccountFloorV2>(missing).is_err());
    let mut bad = signed.fact;
    bad.issuer.issuer_epoch = DecimalU64(0);
    assert!(sign(bad).verify(&pin, &floor, 1001).is_err());
}
