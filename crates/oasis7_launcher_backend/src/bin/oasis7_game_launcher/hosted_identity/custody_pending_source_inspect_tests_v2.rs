//! Actual guarded Native UV/registration/activation/current/once component.
//! Account81 signed payment/grant metadata and Node82 World remain explicit
//! fixtures. This is the real static Native receiver, not CF4-hop/139 evidence.
use super::*;
use oasis7_custody_core::{
    execution_pending_source_inspect_v2::*,
    execution_pending_source_result_v2::*,
    provisioning::{SessionActionRequestV2, SessionToolActionV1},
};
fn source_request(ready: &ReadyNativeRegistryV2) -> PendingSourceNativeRequestV2 {
    let c = &ready.pending.certificate;
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &c.input.request.intent else {
        unreachable!()
    };
    let nonce = "eb".repeat(32);
    let request = ApprovalVerifyRequestV1 {
        approval: c.input.approval.clone(),
        proof_digest: c.input.approval.digest().unwrap(),
        expected_audience: c.input.request.context.audience.clone(),
        expected_intent_digest: c.intent_digest.clone(),
        purpose: VerificationPurposeV1::InspectOrigin,
        verify_nonce: nonce.clone(),
    };
    let issued = now_ms();
    let until = (issued + 5000)
        .min(c.input.approval.proof.expires_at.0)
        .min(plan.write_deadline.0);
    let binding = PendingSourceInspectBindingV2 {
        api_version: 2,
        issuer: c.issuer.clone(),
        scope: c.participant_session.scope.role_scope.clone(),
        session_id: ready.active.certificate.session_id.clone(),
        ingress_attempt_id: "native-source-fixture-ingress".into(),
        signed_attempt_digest: bytes_digest(
            b"explicit Account fixture H attempt, not actual HTTP PoP",
        ),
        action: SessionActionRequestV2 {
            api_version: 2,
            session_id: ready.active.certificate.session_id.clone(),
            request_id: "native-source-fixture-action".into(),
            action: SessionToolActionV1::Execute(plan.actions[0].clone()),
        },
        approved_input_digest: c.approved_input_digest.clone(),
        master_digest: bytes_digest(b"explicit Account fixture master, not actual fee evidence"),
        source_operation_id: "native-source-fixture-source".into(),
        source_verify_nonce: nonce,
        opaque_registration_ref: ready.active.certificate.opaque_registration_ref.clone(),
        active_certificate_digest: ready.active.digest().unwrap(),
        native_binding: c.native_lifetime.lifetime.status.receipt.clone(),
        native_session_ref: c.participant_session.session_ref.clone(),
        native_binding_generation: c.participant_session.native_binding_generation,
        session: c.participant_session.clone(),
        source_request_digest: bytes_digest(&serde_json::to_vec(&request).unwrap()),
        expected_service_epoch: c.input.request.context.service_epoch,
        accepted_ordinal: DecimalU64(1),
        dispatcher_claim_ordinal: DecimalU64(2),
        dispatcher_attempt_id: "ec".repeat(32),
        native_attempt_id: "ed".repeat(32),
        dispatcher_object_id: "ee".repeat(32),
        issued_at: DecimalU64(issued),
        expires_at: DecimalU64(until),
    };
    let account = SigningKey::from_bytes(&[81; 32]);
    let dispatch = SignedPendingSourceInspectDispatchV2 {
        signature_hex: hex::encode(
            account
                .sign(&SignedPendingSourceInspectDispatchV2::signing_bytes(&binding).unwrap())
                .to_bytes(),
        ),
        binding,
    };
    let claim = PendingSourceNativeLeafV2 {
        dispatch,
        native_claim_ordinal: DecimalU64(5),
        claimed_at: DecimalU64(issued),
        expires_at: DecimalU64(until),
    };
    let leaf = SignedPendingSourceNativeLeafV2 {
        signature_hex: hex::encode(
            account
                .sign(&SignedPendingSourceNativeLeafV2::signing_bytes(&claim).unwrap())
                .to_bytes(),
        ),
        claim,
    };
    PendingSourceNativeRequestV2 { leaf, request }
}
#[test]
fn pending_source_native_real_ready_fixed_pin_outer_once_and_cold() {
    if std::env::var("OASIS7_EXECUTION_READY_CHILD")
        .ok()
        .as_deref()
        != Some("true")
    {
        child_for(
            "pending_source_native_real_ready_fixed_pin_outer_once_and_cold",
            "https://identity.fixture",
            &[(
                "OASIS7_CUSTODY_EXECUTION_SOURCE_INSPECT_V2_ENABLED",
                "true".into(),
            )],
        );
        return;
    }
    with_ready_registry(3, |ready, server| {
        let q = source_request(&ready);
        let (headers, native_key_id, native_public, path, namespace, rp) = {
            let lock = ready.service.lock().unwrap();
            let service = lock.as_ref().unwrap();
            let config = service.custody.as_ref().unwrap();
            (
                format!(
                    "POST / HTTP/1.1\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\n\r\n",
                    hex::encode(config.adapter_secret)
                ),
                config.key_id.clone(),
                hex::encode(config.key.verifying_key().to_bytes()),
                service.config.sqlite_path.clone(),
                service.config.namespace.clone(),
                service.config.rp.clone(),
            )
        };
        assert!(
            exchange(&ready.service, SOURCE_NATIVE_PATH_V2, &ready.headers, &q).is_err(),
            "managed browser Cookie/Origin does not replace private service auth"
        );
        let machine = identity::custody::cleanup_config::MachineConfig::from_env().unwrap();
        let fixed_pin = machine.source_inspect_pin().unwrap();
        let mut wrong = q.clone();
        wrong.leaf.signature_hex = "00".repeat(64);
        assert!(
            exchange(&ready.service, SOURCE_NATIVE_PATH_V2, &headers, &wrong).is_err(),
            "wrong Account grant rejected before permanent claim"
        );
        let body = exchange(&ready.service, SOURCE_NATIVE_PATH_V2, &headers, &q).unwrap();
        let reply = SignedPendingSourceInspectResultV2::decode(&serde_json::to_vec(&body).unwrap())
            .unwrap();
        reply
            .verify_for(&q, &native_key_id, &native_public)
            .unwrap();
        assert_eq!(
            reply.result.inner.verification.purpose,
            VerificationPurposeV1::InspectOrigin
        );
        q.leaf
            .verify(
                fixed_pin,
                &machine.issuer_floor,
                &fixed_pin.native_audience_origin,
                now_ms(),
            )
            .unwrap();
        assert!(
            exchange(&ready.service, SOURCE_NATIVE_PATH_V2, &headers, &q).is_err(),
            "same paid attempt cannot reobserve or return a cached Current"
        );
        let mut cold = identity::store::IdentityStore::open(&path, namespace, rp).unwrap();
        cold.attach_machine_restore_guard_from_env(&path).unwrap();
        cold.require_guarded_machine_storage().unwrap();
        {
            let mut lock = ready.service.lock().unwrap();
            lock.as_mut().unwrap().store = cold;
        }
        q.leaf
            .verify(
                fixed_pin,
                &machine.issuer_floor,
                &fixed_pin.native_audience_origin,
                now_ms(),
            )
            .unwrap();
        assert!(
            exchange(&ready.service, SOURCE_NATIVE_PATH_V2, &headers, &q).is_err(),
            "cold reopen preserves permanent attempt and never remints first token"
        );
        assert_eq!(
            server.finish(),
            3,
            "new Source inspects local Current only, no new World read"
        );
    });
}
