//! Actual Native UV/Consumed/managed SID → registration/activation APIs.
//! Account certificates and World responses remain explicit component fixtures;
//! this never seeds an Active row, claims CF payment or qualifies the45 graph.
use super::*;
use oasis7_client_api::world_service::{
    self as ws, participant_discovery::ParticipantOwnerViewRequestV1,
};
use oasis7_custody_core::{
    approval::bytes_digest, native_binding::SignedManagedNativeSessionLifetimeV1,
};
pub(super) struct ReadyNativeRegistryV2 {
    pub service: Arc<std::sync::Mutex<Option<identity::IdentityService>>>,
    pub pending: SignedAccountConsumedPendingV2,
    pub active: SignedAccountActiveThreeRootsV2,
    pub registration: SignedNativeRegistrationAckV2,
    pub activation: SignedNativeActivationAckV2,
    pub headers: String,
}
/// The caller must first complete the two actual Native World3 preflight
/// routes for this exact proof and keep its synthetic Node server available
/// for the original third OwnerRead. This helper invokes real receivers.
pub(super) fn register_and_activate(
    service: Arc<std::sync::Mutex<Option<identity::IdentityService>>>,
    proof: SignedAccountPreflightAccessV2,
    owner_read: ParticipantOwnerViewRequestV1,
    sid: &str,
) -> ReadyNativeRegistryV2 {
    let (lifetime, headers, native_key, native_public, namespace, epoch) = {
        let mut lock = service.lock().unwrap();
        let service = lock.as_mut().unwrap();
        let lifetime: SignedManagedNativeSessionLifetimeV1 = serde_json::from_value(
            service
                .managed_session_lifetime(Some(sid), &json!({"nonce":"d1".repeat(32)}), now_ms())
                .unwrap(),
        )
        .unwrap();
        let csrf = digest(&service.config.session_key, "csrf", sid);
        let headers = format!(
            "POST / HTTP/1.1\r\nOrigin: https://localhost\r\nSec-Fetch-Site: same-origin\r\nContent-Type: application/json\r\nX-Oasis7-CSRF: {csrf}\r\nCookie: __Host-oasis7_sid={sid}\r\n\r\n"
        );
        let config = service.custody.as_ref().unwrap();
        (
            lifetime,
            headers,
            config.key_id.clone(),
            hex::encode(config.key.verifying_key().to_bytes()),
            service.config.namespace.key(),
            service.config.rp.identity_instance_epoch,
        )
    };
    let issued = now_ms();
    let c = &proof.certificate;
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &c.input.request.intent else {
        panic!("V2 session");
    };
    let certificate = AccountConsumedPendingV2 {
        api_version: 2,
        issuer: c.issuer.clone(),
        certificate_id: "d2".repeat(32),
        input: c.input.clone(),
        approved_input_digest: c.approved_input_digest.clone(),
        intent_digest: c.input.request.intent_digest().unwrap(),
        plan_digest: plan.digest().unwrap(),
        participant_session: c.descriptor.session.clone(),
        native_lifetime: lifetime,
        preflight_read_claim: NativeRegistrationReadClaimV2 {
            profile: NativeRegistrationReadProfileV2::ExactMove0OwnerView,
            source_operation_id: c.descriptor.source_operation_id.clone(),
            source_record_digest: c.source_record_digest.clone(),
            preflight_record_digest: proof.digest().unwrap(),
            owner_read_original_digest: registration_read_digest(&owner_read).unwrap(),
            native_transport_attempt_id: "d3".repeat(32),
            world_transport_attempt_id: "d4".repeat(32),
            native_http_units: DecimalU64(1),
            world_http_units: DecimalU64(1),
            claimed_at: DecimalU64(issued),
        },
        consumed_at: c.consumed_at,
        issued_at: DecimalU64(issued),
        expires_at: DecimalU64(
            (issued + 5000)
                .min(c.input.approval.proof.expires_at.0)
                .min(plan.write_deadline.0),
        ),
    };
    // Actual LF may have a narrower signed boundary than the fixture's5s cap.
    let mut certificate = certificate;
    certificate.expires_at.0 = certificate
        .expires_at
        .0
        .min(certificate.native_lifetime.lifetime.status.expires_at.0);
    let account = SigningKey::from_bytes(&[81; 32]);
    let pending = SignedAccountConsumedPendingV2 {
        signature_hex: hex::encode(
            account
                .sign(&certificate.signing_bytes().unwrap())
                .to_bytes(),
        ),
        certificate,
    };
    let request = NativeReadAndRegisterRequestV2 {
        api_version: 2,
        pending: pending.compact_reference().unwrap(),
        owner_read,
        registration_nonce: native_registration_nonce_v2(&pending).unwrap(),
    };
    let registered: NativeReadAndRegisterReplyV2 = serde_json::from_value(
        exchange(&service, NATIVE_REGISTER_V2_PATH, &headers, &request).unwrap(),
    )
    .unwrap();
    registered.verify_world(&request, &pending).unwrap();
    registered
        .registration_ack
        .verify_native(
            &pending,
            &native_key,
            &native_public,
            &namespace,
            epoch,
            now_ms(),
        )
        .unwrap();
    let issued = now_ms();
    let CustodyIntentV1::TestSessionV2 { plan, scope } = &pending.certificate.input.request.intent
    else {
        unreachable!()
    };
    let cert = AccountActiveThreeRootsV2 {
        api_version: 2,
        issuer: pending.certificate.issuer.clone(),
        certificate_id: "d5".repeat(32),
        session_id: scope.session_id.clone(),
        approved_input_digest: pending.certificate.approved_input_digest.clone(),
        pending_certificate_digest: pending.digest().unwrap(),
        native_registration_ack_digest: registered.registration_ack.digest().unwrap(),
        opaque_registration_ref: registered
            .registration_ack
            .ack
            .opaque_registration_ref
            .clone(),
        root_birth_digest: bytes_digest(
            b"explicit Account component fixture root birth, not CF SQL evidence",
        ),
        activation_transport: NativeActivationPaidClaimV2 {
            native_transport_attempt_id: "d6".repeat(32),
            native_paid_record_digest: bytes_digest(
                b"explicit Account component fixture paid fact, not CF payment",
            ),
            activation_nonce: "d7".repeat(32),
            native_http_units: 1,
            claimed_at: DecimalU64(issued),
        },
        issued_at: DecimalU64(issued),
        expires_at: DecimalU64(
            plan.termination
                .cleanup_until
                .get()
                .min(
                    pending
                        .certificate
                        .native_lifetime
                        .lifetime
                        .session_expires_at
                        .0,
                )
                .min(
                    pending
                        .certificate
                        .native_lifetime
                        .lifetime
                        .session_idle_expires_at
                        .0,
                ),
        ),
    };
    let active = SignedAccountActiveThreeRootsV2 {
        signature_hex: hex::encode(
            account
                .sign(&cert.signing_bytes(&pending).unwrap())
                .to_bytes(),
        ),
        certificate: cert,
    };
    let machine = identity::custody::cleanup_config::MachineConfig::from_env().unwrap();
    // Independent existing verifier consumes the original domain/preimage;
    // the new codec cannot change either a valid signature or its digest.
    active
        .verify_issuer(&pending, &machine.active_pin, &machine.issuer_floor, issued)
        .unwrap();
    let mut changed = active.clone();
    changed.certificate.root_birth_digest = "e1".repeat(32);
    assert!(
        changed
            .verify_issuer(&pending, &machine.active_pin, &machine.issuer_floor, issued)
            .is_err()
    );
    let q = NativeActivateRequestV2 {
        api_version: 2,
        opaque_registration_ref: active.certificate.opaque_registration_ref.clone(),
        active: active.clone(),
    };
    let activation: SignedNativeActivationAckV2 =
        serde_json::from_value(exchange(&service, NATIVE_ACTIVATE_V2_PATH, &headers, &q).unwrap())
            .unwrap();
    activation
        .verify_native(
            &pending,
            &active,
            &registered.registration_ack,
            &native_key,
            &native_public,
        )
        .unwrap();
    ReadyNativeRegistryV2 {
        service,
        pending,
        active,
        registration: registered.registration_ack,
        activation,
        headers,
    }
}

#[path = "custody_execution_native_ready_case_v2.rs"]
mod ready_case;
