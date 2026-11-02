//! Genuine signed Native SDK artifacts; no V2 activation or paid caller is inferred.
use super::*;
use oasis7_client_api::world_service::task_context_projection::*;
use oasis7_custody_core::context_body_v2::*;
fn attest<Q: serde::Serialize, T: serde::Serialize>(
    path: &str,
    q: &Q,
    payload: T,
) -> WorldContextAttestationV2<T> {
    let signed = ws::sign_service_response(
        path,
        ws::request_digest(path, q).unwrap(),
        payload,
        &"3f".repeat(32),
    )
    .unwrap();
    WorldContextAttestationV2 {
        request_digest: signed.request_digest,
        payload: signed.payload,
        signature_hex: signed.signature_hex,
    }
}
fn prepared(
    f: &StorageFixture,
) -> (
    ContextBodyScopeV2,
    TaskContextPrepareAttestationRequestV2,
    WorldContextAttestationV2<TaskContextPreparedDeliveryV2>,
) {
    let request = TaskContextPrepareAttestationRequestV2 {
        original: f.original.clone(),
        observation_nonce: "a1".repeat(32),
    };
    let public =
        derive_prepared_projection_v2(&request, &f.prepared, &f.scope.world.service_public_key)
            .unwrap();
    let projection = attest(PREPARED_PROJECTION_DOMAIN_V2, &request, public);
    let scope = ContextBodyScopeV2 {
        api_version: 2,
        format: ContextBodyFormatV2::FullSignedResponseV2,
        identity_scope: f.scope.identity_scope.clone(),
        world: f.scope.world.clone(),
        session_id: f.scope.session_id.clone(),
        context_operation_id: f.scope.context_operation_id.clone(),
        prepare_operation_id: f.original.request.client_operation_id.clone(),
        session_intent_digest: f.scope.session_intent_digest.clone(),
        native_binding_digest: f.scope.native_binding_digest.clone(),
        slot: ContextBodySlotKindV1::Prepared,
        version: f.scope.version,
        write_deadline: f.scope.write_deadline,
        context: f.prepared.payload.identity.clone(),
        public_projection_digest: public_projection_digest_v2(&projection).unwrap(),
        body: ContextBodyCommitmentV2::from_signed_body(&f.prepared).unwrap(),
    };
    let delivery = attest(
        CONTEXT_PREPARE_ATTESTED_PATH_V2,
        &request,
        TaskContextPreparedDeliveryV2 {
            private_body: f.prepared.clone(),
            projection,
        },
    );
    verify_prepared_delivery_v2(&request, &delivery, &scope.world.service_public_key).unwrap();
    (scope, request, delivery)
}
fn record(scope: &ContextBodyScopeV2, bytes: usize) -> ContextBodyRecordV2 {
    ContextBodyRecordV2 {
        scope: scope.clone(),
        envelope: KeyEnvelopeV1 {
            context: scope.envelope_context().unwrap(),
            ciphertext: "ab".repeat(bytes + AES_GCM_TAG_BYTES),
            value_nonce: "01".repeat(12),
            wrapped_dek: "cd".repeat(48),
            wrap_nonce: "02".repeat(12),
            wrapping_key_version: DecimalU64(1),
        },
    }
}
#[test]
fn context_fullsigned_v2_actual_sdk_roundtrip_public_reference_and_cross_format_reject() {
    let f = fixture();
    let before = f.f.state.clone();
    let (scope, q, delivery) = prepared(&f);
    let body =
        validate_fullsigned_prepared_storage_v2(scope.clone(), &f.scope.world, &q, &delivery)
            .unwrap();
    assert!(body.canonical_bytes() == oasis7_wasm_abi::encode_canonical_cbor(&f.prepared).unwrap());
    assert!(
        body.canonical_bytes()
            != oasis7_wasm_abi::encode_canonical_cbor(&f.prepared.payload).unwrap()
    );
    let saved = record(&scope, body.canonical_bytes().len());
    saved.validate().unwrap();
    let reference = body
        .reference(
            AttestedContextEvidenceV2::Prepared {
                request: Box::new(q.clone()),
                projection: Box::new(delivery.payload.projection.clone()),
            },
            &saved,
            &f.scope.world,
        )
        .unwrap();
    assert_evidence_wire_transparent(&reference.evidence);
    let reopened = validate_recovered_fullsigned_storage_v2(
        &reference,
        &f.scope.world,
        body.canonical_bytes(),
    )
    .unwrap();
    assert!(reopened.canonical_bytes() == body.canonical_bytes());
    assert!(scope.derived_actor_name().unwrap() != f.scope.derived_actor_name().unwrap());
    assert!(scope.envelope_context().unwrap() != f.scope.envelope_context().unwrap());
    assert!(
        serde_json::from_value::<ContextBodyRecordV1>(serde_json::to_value(&saved).unwrap())
            .is_err()
    );
    assert!(
        serde_json::from_value::<ContextBodyRecordV2>(
            serde_json::to_value(envelope(&f.scope, 32)).unwrap()
        )
        .is_err()
    );
    let public = serde_json::to_value(&reference).unwrap();
    assert!(public.get("envelope").is_none());
    assert!(public.to_string().find("private_body").is_none());
    assert_eq!(f.f.state, before);
}
#[test]
fn context_fullsigned_v2_actual_captured_and_changed_signed_material_reject() {
    let f = fixture();
    let (scope, q, p) = prepared(&f);
    let body =
        validate_fullsigned_prepared_storage_v2(scope.clone(), &f.scope.world, &q, &p).unwrap();
    let reference = body
        .reference(
            AttestedContextEvidenceV2::Prepared {
                request: Box::new(q),
                projection: Box::new(p.payload.projection.clone()),
            },
            &record(&scope, body.canonical_bytes().len()),
            &f.scope.world,
        )
        .unwrap();
    let (get, _, mut captured) = captured_artifact(&f.f, &f.prepared.payload, &f.original);
    captured.payload.capture_proof.receipt = serde_json::to_value(
        ws::task_context::TaskProviderPrefixReceiptV1::from_context(&scope.context),
    )
    .unwrap();
    captured = ws::sign_service_response(
        TASK_CONTEXT_READ_PATH,
        ws::request_digest(TASK_CONTEXT_READ_PATH, &get).unwrap(),
        captured.payload,
        &"3f".repeat(32),
    )
    .unwrap();
    captured.payload.validate_against(&get).unwrap();
    let request = TaskContextGetAttestationRequestV2 {
        original: get,
        observation_nonce: "a2".repeat(32),
        prepared_projection: p.payload.projection,
    };
    let public =
        derive_captured_projection_v2(&request, &captured, &scope.world.service_public_key)
            .unwrap();
    let projection = attest(CAPTURED_PROJECTION_DOMAIN_V2, &request, public);
    let delivery = attest(
        CONTEXT_READ_ATTESTED_PATH_V2,
        &request,
        TaskContextCapturedDeliveryV2 {
            private_body: captured,
            projection,
        },
    );
    verify_captured_delivery_v2(&request, &delivery, &scope.world.service_public_key).unwrap();
    assert_evidence_wire_transparent(&AttestedContextEvidenceV2::Captured {
        request: Box::new(request.clone()),
        projection: Box::new(delivery.payload.projection.clone()),
    });
    let mut cscope = scope.clone();
    cscope.slot = ContextBodySlotKindV1::Captured;
    cscope.body = delivery.payload.projection.payload.body.clone();
    cscope.public_projection_digest =
        public_projection_digest_v2(&delivery.payload.projection).unwrap();
    let cbody = validate_fullsigned_captured_storage_v2(
        cscope.clone(),
        &scope.world,
        &reference,
        &request,
        &delivery,
    )
    .unwrap();
    let mut bad = delivery.clone();
    bad.payload.private_body.signature_hex = "00".repeat(64);
    assert!(
        validate_fullsigned_captured_storage_v2(
            cscope.clone(),
            &scope.world,
            &reference,
            &request,
            &bad
        )
        .is_err()
    );
    let mut badscope = cscope.clone();
    badscope.body.canonical_bytes.0 += 1;
    assert!(
        validate_fullsigned_captured_storage_v2(
            badscope,
            &scope.world,
            &reference,
            &request,
            &delivery
        )
        .is_err()
    );
    let mut badref = reference.clone();
    badref.envelope_wire_bytes = DecimalU64(0);
    assert!(badref.validate_provenance(&scope.world).is_err());
    assert!(
        validate_recovered_fullsigned_storage_v2(&reference, &scope.world, cbody.canonical_bytes())
            .is_err()
    );
    let mut oldpayload = oasis7_wasm_abi::encode_canonical_cbor(&f.prepared.payload).unwrap();
    assert!(
        validate_recovered_fullsigned_storage_v2(&reference, &scope.world, &oldpayload).is_err()
    );
    oldpayload.resize(64 * 1024 + 1, 0);
    assert!(matches!(
        validate_recovered_fullsigned_storage_v2(&reference, &scope.world, &oldpayload),
        Err(ContextBodyError::ContextTooLarge)
    ));
}

#[test]
fn context_fullsigned_v2_exact_webcrypto_manifest_rebuild_rejects_changed_scope() {
    use oasis7_custody_core::approval_admission::*;
    let f = fixture();
    let (scope, q, delivery) = prepared(&f);
    let checks =
        fullsigned_prepared_storage_checks_v2(&scope, &scope.world, &q, &delivery).unwrap();
    assert_eq!(checks.checks().len(), 5);
    let body = validate_fullsigned_prepared_checked_v2(
        scope.clone(),
        &scope.world,
        &q,
        &delivery,
        &mut ExactCache::verified(checks.clone()),
    )
    .unwrap();
    assert!(!body.canonical_bytes().is_empty());
    let mut changed = scope.clone();
    changed.context_operation_id = "different-public-context-operation".into();
    // Still a legitimate historical signed body. Only the complete manifest's
    // scope binding differs, so an old async cache must not authorize it.
    validate_fullsigned_prepared_storage_v2(changed.clone(), &scope.world, &q, &delivery).unwrap();
    assert!(
        validate_fullsigned_prepared_checked_v2(
            changed,
            &scope.world,
            &q,
            &delivery,
            &mut ExactCache::verified(checks)
        )
        .is_err()
    );
    let mut unknown = serde_json::to_value(&delivery).unwrap();
    unknown["verified"] = serde_json::json!(true);
    assert!(
        serde_json::from_value::<WorldContextAttestationV2<TaskContextPreparedDeliveryV2>>(unknown)
            .is_err()
    );
}

#[test]
fn context_fullsigned_v2_export_synthetic_storage_fixture() {
    let Ok(path) = std::env::var("OASIS7_CONTEXT_FULLSIGNED_V2_FIXTURE") else {
        return;
    };
    let f = fixture();
    let (mut scope, q, p) = prepared(&f);
    // This is an explicit storage-only scope, not an approved Account root.
    // Historical SDK signatures are unchanged. No Current/World authority is
    // inferred from this future synthetic storage deadline.
    let wall = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    scope.write_deadline = DecimalU64(u64::try_from(wall).unwrap().checked_add(600_000).unwrap());
    let (get, _, mut captured) = captured_artifact(&f.f, &f.prepared.payload, &f.original);
    captured.payload.capture_proof.receipt = serde_json::to_value(
        ws::task_context::TaskProviderPrefixReceiptV1::from_context(&scope.context),
    )
    .unwrap();
    captured = ws::sign_service_response(
        TASK_CONTEXT_READ_PATH,
        ws::request_digest(TASK_CONTEXT_READ_PATH, &get).unwrap(),
        captured.payload,
        &"3f".repeat(32),
    )
    .unwrap();
    let request = TaskContextGetAttestationRequestV2 {
        original: get,
        observation_nonce: "a2".repeat(32),
        prepared_projection: p.payload.projection.clone(),
    };
    let public =
        derive_captured_projection_v2(&request, &captured, &scope.world.service_public_key)
            .unwrap();
    let projection = attest(CAPTURED_PROJECTION_DOMAIN_V2, &request, public);
    let captured = attest(
        CONTEXT_READ_ATTESTED_PATH_V2,
        &request,
        TaskContextCapturedDeliveryV2 {
            private_body: captured,
            projection,
        },
    );
    let mut cscope = scope.clone();
    cscope.slot = ContextBodySlotKindV1::Captured;
    cscope.body = captured.payload.projection.payload.body.clone();
    cscope.public_projection_digest =
        public_projection_digest_v2(&captured.payload.projection).unwrap();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "label":"synthetic-historical-fullsigned-storage-not-authority", "world":scope.world,
        "prepared":{"operation":"prepared","scope":scope,"request":q,"delivery":p},
        "captured":{"operation":"captured","scope":cscope,"request":request,"delivery":captured}
    }))
    .unwrap();
    std::fs::write(path, bytes).unwrap();
}

// The heap layout has no wire/domain effect. Compare each real strong fixture
// against the exact former by-value enum shape, not a handwritten JSON blob.
fn assert_evidence_wire_transparent(evidence: &AttestedContextEvidenceV2) {
    #[derive(serde::Serialize)]
    #[serde(tag = "slot", rename_all = "snake_case")]
    enum FormerEvidence<'a> {
        Prepared {
            request: &'a TaskContextPrepareAttestationRequestV2,
            projection: &'a WorldContextAttestationV2<TaskContextPreparedProjectionV2>,
        },
        Captured {
            request: &'a TaskContextGetAttestationRequestV2,
            projection: &'a WorldContextAttestationV2<TaskContextCapturedProjectionV2>,
        },
    }
    let former = match evidence {
        AttestedContextEvidenceV2::Prepared {
            request,
            projection,
        } => FormerEvidence::Prepared {
            request: request.as_ref(),
            projection: projection.as_ref(),
        },
        AttestedContextEvidenceV2::Captured {
            request,
            projection,
        } => FormerEvidence::Captured {
            request: request.as_ref(),
            projection: projection.as_ref(),
        },
    };
    assert!(serde_json::to_vec(evidence).unwrap() == serde_json::to_vec(&former).unwrap());
    assert!(
        oasis7_wasm_abi::encode_canonical_cbor(evidence).unwrap()
            == oasis7_wasm_abi::encode_canonical_cbor(&former).unwrap()
    );
    let roundtrip: AttestedContextEvidenceV2 =
        serde_json::from_value(serde_json::to_value(evidence).unwrap()).unwrap();
    assert!(&roundtrip == evidence);
}
