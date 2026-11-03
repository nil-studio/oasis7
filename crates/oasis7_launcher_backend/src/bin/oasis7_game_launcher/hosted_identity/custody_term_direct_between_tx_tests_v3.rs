//! Actual UV/Consumed/World3/Register/Active; Account paid signatures are fixtures.
//! No physical CF payment or production V3 installation is claimed.
use super::*;
#[test]
fn term_direct_v3_two_transactions_reject_expiry_after_permanent_claim() {
    if std::env::var("OASIS7_EXECUTION_READY_CHILD")
        .ok()
        .as_deref()
        != Some("true")
    {
        child_for(
            "term_direct_v3_two_transactions_reject_expiry_after_permanent_claim",
            "https://identity.fixture",
            &[("OASIS7_TERM_DIRECT_V3_FIXTURE", "ordinary-denied".into())],
        );
        child_for(
            "term_direct_v3_two_transactions_reject_expiry_after_permanent_claim",
            "https://identity.fixture",
            &[(
                "OASIS7_TERM_DIRECT_V3_FIXTURE",
                "genuine-before-uv-v3".into(),
            )],
        );
        return;
    }
    if !identity::custody::guarded_consent::term_direct_v3_fixture_enabled() {
        let f = native_fixture::managed_fixture();
        let mut request = native_certificate::options_for(&f, now_ms());
        let CustodyIntentV1::TestSessionV2 { plan, .. } = &mut request.intent else {
            unreachable!()
        };
        plan.termination.cleanup_transport_policy=oasis7_custody_core::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedDirectCurrent5V3;
        assert!(
            identity::custody::guarded_consent::require_supported_profile(
                f.f.service.custody.as_ref().unwrap(),
                &request
            )
            .is_err()
        );
        return;
    }
    with_ready_registry(3, |ready, server| {
        let CustodyIntentV1::TestSessionV2 { plan, .. } =
            &ready.pending.certificate.input.request.intent
        else {
            unreachable!()
        };
        assert_eq!(plan.termination.cleanup_transport_policy,oasis7_custody_core::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedDirectCurrent5V3);
        let make = |suffix: &str| {
            let now = now_ms();
            let body = NativeTermDirectBodyV3 {
                api_version: 3,
                session_id: ready.pending.certificate.input.operation_id.clone(),
                owned_invocation_id: format!("owned-{suffix}"),
                source_operation_id: format!("source-{suffix}"),
                stop_operation_id: plan.termination.stop_operation_id.clone(),
                close_operation_id: plan.termination.close_operation_id.clone(),
                nonce: "01".repeat(32),
                lifetime_nonce: "02".repeat(32),
                origin_nonce: "03".repeat(32),
                metadata_session: ready.pending.certificate.participant_session.clone(),
                metadata_lifetime: ready.pending.certificate.native_lifetime.clone(),
            };
            let issuer = AccountMachineIssuerV2 {
                namespace: "account".into(),
                service_id: "account-service".into(),
                environment: "test".into(),
                key_id: "account-key".into(),
                issuer_epoch: DecimalU64(1),
                config_generation: DecimalU64(1),
                native_audience_origin: "https://localhost".into(),
            };
            let binding = TermDirectPaidBindingV3 {
                api_version: 3,
                issuer,
                session_id: body.session_id.clone(),
                approved_input_digest: ready.pending.certificate.approved_input_digest.clone(),
                termination_plan_digest: plan.termination.digest().unwrap(),
                active_certificate_digest: ready.active.digest().unwrap(),
                opaque_registration_ref: ready.registration.ack.opaque_registration_ref.clone(),
                native_key_id: ready.registration.ack.native_key_id.clone(),
                identity_namespace: ready.registration.ack.identity_namespace.clone(),
                identity_epoch: ready.registration.ack.identity_epoch,
                observation_round: oasis7_custody_core::provisioning::TermObservationRoundV3::First,
                round_id: format!("round-{suffix}"),
                actor_object_id: "ab".repeat(32),
                actor_attempt_id: format!("actor-{suffix}"),
                native_attempt_id: format!("native-{suffix}"),
                actor_paid_ordinal: DecimalU64(4),
                native_paid_ordinal: DecimalU64(5),
                graph_digest: "11".repeat(32),
                actor_paid_digest: "12".repeat(32),
                native_paid_digest: "13".repeat(32),
                body_digest: body.digest().unwrap(),
                observation_started_at: DecimalU64(now),
                issued_at: DecimalU64(now),
                expires_at: DecimalU64((now + 250).min(ready.active.certificate.expires_at.0)),
                cleanup_until: DecimalU64(plan.termination.cleanup_until.get()),
            };
            binding
                .validate_relative(&body, &ready.pending, &ready.active, &ready.registration)
                .unwrap();
            let key = SigningKey::from_bytes(&[81; 32]);
            let actor = SignedTermDirectActorV3 {
                signature_hex: hex::encode(
                    key.sign(&binding.actor_signing_bytes().unwrap()).to_bytes(),
                ),
                binding,
            };
            let claim = TermDirectNativeClaimV3 {
                binding: actor.binding.clone(),
                actor_proof_digest: actor.digest().unwrap(),
            };
            let native = SignedTermDirectNativeV3 {
                signature_hex: hex::encode(key.sign(&claim.signing_bytes().unwrap()).to_bytes()),
                claim,
            };
            NativeTermDirectFrameV3 {
                body,
                actor,
                native,
            }
        };
        let normal = make("normal");
        let mut lock = ready.service.lock().unwrap();
        let service = lock.as_mut().unwrap();
        let actual = service
            .observe_cleanup_term_direct_v3("POST", &normal.wire_bytes().unwrap())
            .unwrap();
        let config = service.custody.as_ref().unwrap();
        actual
            .verify_for(
                &normal,
                TermDirectFactContextV3 {
                    pending: &ready.pending,
                    active: &ready.active,
                    registration: &ready.registration,
                    native_key_id: &config.key_id,
                    native_public_key: &hex::encode(config.key.verifying_key().to_bytes()),
                    now: now_ms(),
                },
            )
            .unwrap();
        let expiring = make("expires-after-claim");
        let material = serde_json::to_vec(&(
            &expiring.actor.binding.issuer.namespace,
            &expiring.actor.binding.issuer.service_id,
            &expiring.actor.binding.issuer.environment,
            &expiring.actor.binding.native_attempt_id,
        ))
        .unwrap();
        let once_key = format!(
            "custody_cleanup_physical_attempt_v2:{}",
            blake3::hash(&material).to_hex()
        );
        let result =
            service.observe_cleanup_term_direct_v3("POST", &expiring.wire_bytes().unwrap());
        assert!(now_ms() > expiring.actor.binding.expires_at.0);
        let once = service
            .store
            .transaction(|tx| Ok(tx.get::<serde_json::Value>(&once_key)?.map(|r| r.value)))
            .unwrap();
        assert_eq!(
            once.as_ref().unwrap()["stage"],
            "term_direct_current_native_v3"
        );
        assert_eq!(
            once.as_ref().unwrap()["certificate_digest"],
            expiring.digest().unwrap()
        );
        assert!(
            result.is_err(),
            "TX2 must observe fresh time after the permanent TX1 claim expires"
        );
        assert!(
            service
                .observe_cleanup_term_direct_v3("POST", &expiring.wire_bytes().unwrap())
                .is_err()
        );
        let same = service
            .store
            .transaction(|tx| Ok(tx.get::<serde_json::Value>(&once_key)?.map(|r| r.value)))
            .unwrap();
        assert_eq!(once, same);
        assert_eq!(server.finish(), 3);
    });
}
