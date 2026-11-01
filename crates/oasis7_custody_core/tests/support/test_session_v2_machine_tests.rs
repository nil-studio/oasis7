//! Actual Core Book/Source/LF/Root transitions; service-signing test keys only.
//! This is not Native HTTP, environment issuer provisioning or 23-edge acceptance.
use super::*;
use oasis7_custody_core::native_cleanup_v2::*;
// Same lowercase fixture encoding as resource_v2_paid_proof_tests::hexbytes.
mod hex {
    pub(super) fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
    pub(super) fn decode(value: &str) -> Result<Vec<u8>, ()> {
        if value.len() % 2 != 0
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(());
        }
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let digit = |b: u8| {
                    if b.is_ascii_digit() {
                        b - b'0'
                    } else {
                        b - b'a' + 10
                    }
                };
                Ok((digit(pair[0]) << 4) | digit(pair[1]))
            })
            .collect()
    }
}

fn issuer_pin(origin: &str, purpose: AccountMachinePurposeV2) -> AccountMachineIssuerPinV2 {
    AccountMachineIssuerPinV2 {
        namespace: "account-fixture".into(),
        service_id: "account-service".into(),
        environment: "test".into(),
        key_id: "account-issuer".into(),
        public_key: hex::encode(issuer_key().verifying_key().to_bytes()),
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        not_before: DecimalU64(0),
        not_after: DecimalU64(100_000),
        native_audience_origin: origin.into(),
        allowed_purpose: purpose,
    }
}
fn issuer_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[97; 32])
}
fn issuer(pin: &AccountMachineIssuerPinV2) -> AccountMachineIssuerV2 {
    AccountMachineIssuerV2 {
        namespace: pin.namespace.clone(),
        service_id: pin.service_id.clone(),
        environment: pin.environment.clone(),
        key_id: pin.key_id.clone(),
        issuer_epoch: pin.issuer_epoch,
        config_generation: pin.config_generation,
        native_audience_origin: pin.native_audience_origin.clone(),
    }
}
fn floor() -> AccountMachineIssuerFloorV2 {
    AccountMachineIssuerFloorV2 {
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
    }
}
fn cold(s: &AccountProvisionApprovalState) {
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(s).unwrap())
            .unwrap(),
        *s
    );
}
#[test]
fn machine_v2_real_book_pending_registered_await_native_and_ready_are_once_and_restore_exact() {
    let start = 9000;
    let (mut f, d, life) = activation_tests::preactivated(start);
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let ns = scope(&f.pin);
    let pin = issuer_pin(ns.http_origin, AccountMachinePurposeV2::ConsumedPending);
    let pre = TestSessionPreflightCurrent {
        provision: &c,
        native_scope: &ns,
        source_operation_id: &d.source_operation_id,
        source_ticket: 7,
        oracle_ticket: 5,
        observed_at: UnixMillis::new(start + 14),
    };
    let prepared = f
        .state
        .prepare_test_session_v2_machine_pending(
            &f.session_id,
            NativeMachinePendingIssuerV2 {
                issuer: issuer(&pin),
                pin: &pin,
                floor: &floor(),
            },
            &pre,
            UnixMillis::new(start + 14),
            &mut StrictCrypto,
        )
        .unwrap();
    let signature = hex::encode(
        issuer_key()
            .sign(&prepared.signing_bytes().unwrap())
            .to_bytes(),
    );
    let pending = f
        .state
        .finish_test_session_v2_machine_pending(
            prepared,
            signature,
            &pre,
            UnixMillis::new(start + 14),
            &mut StrictCrypto,
        )
        .unwrap();
    pending.verify_issuer(&pin, &floor(), start + 14).unwrap();
    cold(&f.state);
    assert!(
        f.state
            .test_session_v2_snapshot(&f.session_id)
            .unwrap()
            .roots()
            .is_empty()
    );
    let before = f.state.clone();
    assert!(
        f.state
            .prepare_test_session_v2_machine_pending(
                &f.session_id,
                NativeMachinePendingIssuerV2 {
                    issuer: issuer(&pin),
                    pin: &pin,
                    floor: &floor()
                },
                &pre,
                UnixMillis::new(start + 14),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let original = f
        .state
        .test_session_v2_machine_read_original(&f.session_id)
        .unwrap();
    let Some(OwnerSetupResultV1::FinalObservation { owner, .. }) = f
        .state
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
        .unwrap()
        .result
        .as_ref()
    else {
        unreachable!()
    };
    let response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &original.owner_read).unwrap(),
        owner.payload.clone(),
        &"3f".repeat(32),
    )
    .unwrap();
    let reg = NativeRegistrationAckV2 {
        api_version: 2,
        native_key_id: ns.native_root_key_id.into(),
        identity_namespace: pending
            .certificate
            .participant_session
            .scope
            .identity_namespace
            .clone(),
        identity_epoch: pending
            .certificate
            .participant_session
            .scope
            .identity_instance_epoch,
        session_id: f.session_id.clone(),
        approved_input_digest: pending.certificate.approved_input_digest.clone(),
        pending_certificate_digest: pending.digest().unwrap(),
        owner_read_response_digest: registration_response_digest(&response).unwrap(),
        opaque_registration_ref: "native-opaque-ref".into(),
        managed_session_ref: pending.certificate.participant_session.session_ref.clone(),
        managed_binding_generation: pending
            .certificate
            .participant_session
            .native_binding_generation,
        registration_nonce: original.registration_nonce.clone(),
        storage_head: NativeStorageHeadV2 {
            instance_id: "ad".repeat(32),
            revision: DecimalU64(1),
            commitment: "ae".repeat(32),
        },
        registry_cursor: DecimalU64(1),
        issued_at: DecimalU64(start + 15),
        expires_at: pending.certificate.expires_at,
    };
    let registration = SignedNativeRegistrationAckV2 {
        signature_hex: hex::encode(key().sign(&reg.signing_bytes(&pending).unwrap()).to_bytes()),
        ack: reg,
    };
    registration
        .verify_native(
            &pending,
            ns.native_root_key_id,
            ns.native_root_public_key,
            &pending
                .certificate
                .participant_session
                .scope
                .identity_namespace,
            pending
                .certificate
                .participant_session
                .scope
                .identity_instance_epoch
                .0,
            start + 15,
        )
        .unwrap();
    let reply = NativeReadAndRegisterReplyV2 {
        api_version: 2,
        world_response: response.clone(),
        registration_ack: registration.clone(),
    };
    let world = OwnerSetupWorldEvidenceV1 {
        request: original.owner_read,
        response,
    };
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(
                &f.session_id,
                &d.source_operation_id,
                UnixMillis::new(start + 15)
            )
            .unwrap(),
        8
    );
    let enrollment = ProvedEnrollmentObservation {
        actual_actor_id: "actor-one",
        expected_actor_id: "actor-one",
        actual_realm: "enrollment",
        expected_realm: "enrollment",
        expected_public_origin: "https://custody.example",
        fact: &f.fact,
    };
    macro_rules! current {
        ($at:expr) => {
            TestSessionCurrent {
                approval: TestSessionApprovalCurrent {
                    provision: &c,
                    native_scope: &ns,
                    enrollment: &enrollment,
                    deployments: &f.deployments,
                    observed_at: UnixMillis::new($at),
                },
                lifetime: &life,
                private_sid_generation: DecimalU64(1),
                world: &world,
                source_operation_id: &d.source_operation_id,
            }
        };
    }
    let born = f
        .state
        .activate_test_session_v2_registered_machine(
            &f.session_id,
            reply,
            &current!(start + 15),
            UnixMillis::new(start + 15),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(born.roots().len(), 3);
    assert!(born.approved_at().is_none());
    cold(&f.state);
    let before = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_v2_source(
                &f.session_id,
                SessionSourceReservation {
                    operation_id: "machine-read-source",
                    nonce: &"b7".repeat(32),
                    funding: SessionSourceFundingV1::ReconcileRead
                },
                UnixMillis::new(start + 15)
            )
            .is_err()
    );
    assert_eq!(f.state, before);

    let roots = serde_json::to_value(&f.state).unwrap()["test_sessions_v2"][f.session_id.as_str()]
        ["roots"]
        .clone();
    let before = f.state.clone();
    let active_pin = issuer_pin(ns.http_origin, AccountMachinePurposeV2::ActiveThreeRoots);
    let prepared = f
        .state
        .prepare_test_session_v2_machine_active(
            &f.session_id,
            &active_pin,
            &floor(),
            &current!(start + 16),
            UnixMillis::new(start + 16),
            &mut StrictCrypto,
        )
        .unwrap();
    let signature = hex::encode(
        issuer_key()
            .sign(&prepared.signing_bytes().unwrap())
            .to_bytes(),
    );
    let dispatch = f
        .state
        .finish_test_session_v2_machine_active(prepared, signature, UnixMillis::new(start + 16))
        .unwrap();
    let request = dispatch.request();
    let active = request.active.clone();
    active
        .verify_issuer(&pending, &active_pin, &floor(), start + 16)
        .unwrap();
    cold(&f.state);
    let after = serde_json::to_value(&f.state).unwrap();
    let prior = serde_json::to_value(&before).unwrap();
    let spent = |v: &serde_json::Value| {
        v["test_sessions_v2"][f.session_id.as_str()]["identity_transport"]["spent"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    assert_eq!(spent(&after), spent(&prior) + 1);
    assert_eq!(
        after["test_sessions_v2"][f.session_id.as_str()]["roots"],
        roots
    );
    let saved = f.state.clone();
    assert!(
        f.state
            .prepare_test_session_v2_machine_active(
                &f.session_id,
                &active_pin,
                &floor(),
                &current!(start + 17),
                UnixMillis::new(start + 17),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, saved);
    let ack = NativeActivationAckV2 {
        api_version: 2,
        native_key_id: ns.native_root_key_id.into(),
        identity_namespace: registration.ack.identity_namespace.clone(),
        identity_epoch: registration.ack.identity_epoch,
        session_id: f.session_id.clone(),
        approved_input_digest: active.certificate.approved_input_digest.clone(),
        opaque_registration_ref: registration.ack.opaque_registration_ref.clone(),
        pending_certificate_digest: pending.digest().unwrap(),
        active_certificate_digest: active.digest().unwrap(),
        root_birth_digest: active.certificate.root_birth_digest.clone(),
        native_transport_attempt_id: active
            .certificate
            .activation_transport
            .native_transport_attempt_id
            .clone(),
        activation_nonce: active
            .certificate
            .activation_transport
            .activation_nonce
            .clone(),
        storage_head: NativeStorageHeadV2 {
            instance_id: registration.ack.storage_head.instance_id.clone(),
            revision: DecimalU64(2),
            commitment: "af".repeat(32),
        },
        registry_cursor: DecimalU64(2),
        activated_at: DecimalU64(start + 17),
    };
    let signed = SignedNativeActivationAckV2 {
        signature_hex: hex::encode(
            key()
                .sign(&ack.signing_bytes(&pending, &active, &registration).unwrap())
                .to_bytes(),
        ),
        ack,
    };
    let mut bad = signed.clone();
    bad.ack.root_birth_digest = "ab".repeat(32);
    bad.signature_hex = hex::encode(
        key()
            .sign(
                &oasis7_wasm_abi::encode_canonical_cbor(&(
                    NATIVE_ACTIVATION_ACK_V2_DOMAIN,
                    &bad.ack,
                ))
                .unwrap(),
            )
            .to_bytes(),
    );
    key()
        .verifying_key()
        .verify_strict(
            &oasis7_wasm_abi::encode_canonical_cbor(&(NATIVE_ACTIVATION_ACK_V2_DOMAIN, &bad.ack))
                .unwrap(),
            &ed25519_dalek::Signature::from_bytes(
                &hex::decode(&bad.signature_hex).unwrap().try_into().unwrap(),
            ),
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .admit_test_session_v2_native_activation(
                &f.session_id,
                bad,
                &current!(start + 18),
                UnixMillis::new(start + 18),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    f.state
        .admit_test_session_v2_native_activation(
            &f.session_id,
            signed.clone(),
            &current!(start + 18),
            UnixMillis::new(start + 18),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(
        f.state
            .test_session_v2_snapshot(&f.session_id)
            .unwrap()
            .approved_at(),
        Some(UnixMillis::new(start + 15))
    );
    cold(&f.state);
    f.state
        .reserve_test_session_v2_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: "machine-read-source",
                nonce: &"b7".repeat(32),
                funding: SessionSourceFundingV1::ReconcileRead,
            },
            UnixMillis::new(start + 18),
        )
        .unwrap();
    // Legitimate later spend must not change the immutable root-birth identity.
    cold(&f.state);
    let before = f.state.clone();
    assert!(
        f.state
            .admit_test_session_v2_native_activation(
                &f.session_id,
                signed,
                &current!(start + 19),
                UnixMillis::new(start + 19),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
}
