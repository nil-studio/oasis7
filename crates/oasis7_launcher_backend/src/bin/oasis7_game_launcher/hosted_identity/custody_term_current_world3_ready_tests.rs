//! Real Native UV→Consumed→World3 preflight/register/activate, then actual
//! Native term-current receiver. Account paid certificates and World are
//! explicit strong component fixtures, not CF payment or whole cleanup graph.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::native_cleanup_v2::{
    NATIVE_TERM_CURRENT_V2_PATH, NativeTermCurrentEnvelopeV2, NativeTermCurrentPaidEdgeV2,
    NativeTermCurrentRequestV2, SignedNativeTermCurrentFactV2, SignedNativeTermCurrentPaidEdgeV2,
};
use oasis7_custody_core::{DecimalU64, approval::bytes_digest};
#[test]
fn native_term_current_accepts_actual_new_world3_ready_profile() {
    if std::env::var("OASIS7_EXECUTION_READY_CHILD")
        .ok()
        .as_deref()
        != Some("true")
    {
        child_for(
            "native_term_current_accepts_actual_new_world3_ready_profile",
            "https://identity.fixture",
            &[],
        );
        return;
    }
    with_ready_registry(3, |ready, server| {
        ready
            .active
            .validate_registration(&ready.pending, &ready.registration)
            .unwrap();
        assert_eq!(
            ready.activation.ack.active_certificate_digest,
            ready.active.digest().unwrap()
        );
        let CustodyIntentV1::TestSessionV2 { plan, .. } =
            &ready.pending.certificate.input.request.intent
        else {
            unreachable!()
        };
        assert_eq!(plan.termination.cleanup_transport_policy,
            oasis7_custody_core::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2,
            "the actual complete UV approval must already contain World3; never mutate a saved approval");
        let now = now_ms();
        let body = NativeTermCurrentRequestV2 {
            api_version: 2,
            session_id: ready.active.certificate.session_id.clone(),
            owned_invocation_id: "term-world3-owned".into(),
            source_operation_id: "term-world3-source".into(),
            stop_operation_id: plan.termination.stop_operation_id.clone(),
            close_operation_id: plan.termination.close_operation_id.clone(),
            nonce: "e1".repeat(32),
            lifetime_nonce: "e2".repeat(32),
            origin_nonce: "e3".repeat(32),
        };
        body.validate().unwrap();
        let edge = NativeTermCurrentPaidEdgeV2 {
            api_version: 2,
            issuer: ready.pending.certificate.issuer.clone(),
            session_id: body.session_id.clone(),
            approved_input_digest: ready.pending.certificate.approved_input_digest.clone(),
            termination_plan_digest: plan.termination.digest().unwrap(),
            active_certificate_digest: ready.active.digest().unwrap(),
            opaque_registration_ref: ready.registration.ack.opaque_registration_ref.clone(),
            native_key_id: ready.registration.ack.native_key_id.clone(),
            identity_namespace: ready.registration.ack.identity_namespace.clone(),
            identity_epoch: ready.registration.ack.identity_epoch,
            native_body_digest: body.body_digest().unwrap(),
            graph_reservation_digest: bytes_digest(
                b"explicit Account component fixture: Term graph, not CF reservation",
            ),
            paid_record_digest: bytes_digest(
                b"explicit Account component fixture: one paid Native leaf, not CF payment",
            ),
            native_transport_attempt_id: "term-world3-native-attempt".into(),
            native_paid_ordinal: DecimalU64(1),
            native_http_units: DecimalU64(1),
            observation_started_at: DecimalU64(now),
            issued_at: DecimalU64(now),
            expires_at: DecimalU64(
                (now + 5000)
                    .min(ready.active.certificate.expires_at.0)
                    .min(plan.termination.cleanup_until.get()),
            ),
            cleanup_until: DecimalU64(plan.termination.cleanup_until.get()),
        };
        let account = SigningKey::from_bytes(&[81; 32]);
        let bytes = edge.signing_bytes().unwrap();
        let signature = account.sign(&bytes);
        account
            .verifying_key()
            .verify_strict(&bytes, &signature)
            .unwrap();
        let request = NativeTermCurrentEnvelopeV2 {
            body,
            paid: SignedNativeTermCurrentPaidEdgeV2 {
                edge,
                signature_hex: hex::encode(signature.to_bytes()),
            },
        };
        let observed = exchange(
            &ready.service,
            NATIVE_TERM_CURRENT_V2_PATH,
            &ready.headers,
            &request,
        );
        assert!(
            observed.is_ok(),
            "actual freshly approved World3 Ready must support the inherited TermCurrent receiver"
        );
        let fact: SignedNativeTermCurrentFactV2 =
            serde_json::from_value(observed.unwrap()).unwrap();
        let (id, public) = {
            let lock = ready.service.lock().unwrap();
            let config = lock.as_ref().unwrap().custody.as_ref().unwrap();
            (
                config.key_id.clone(),
                hex::encode(config.key.verifying_key().to_bytes()),
            )
        };
        fact.verify_native(&id, &public, now_ms()).unwrap();
        fact.fact.validate_envelope(&request).unwrap();
        fact.fact
            .validate_approval(&ready.pending, &ready.active, &ready.registration)
            .unwrap();
        assert!(
            exchange(
                &ready.service,
                NATIVE_TERM_CURRENT_V2_PATH,
                &ready.headers,
                &request
            )
            .is_err(),
            "the real physical term-current attempt is permanently once"
        );
        assert_eq!(
            server.finish(),
            3,
            "term-current adds no World HTTP to the three real Ready reads"
        );
    });
}
