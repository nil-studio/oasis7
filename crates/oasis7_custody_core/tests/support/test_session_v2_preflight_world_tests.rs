//! Real full-approval/Source prefix with fixed service test keys. This proves
//! Account allocation/admission, not Native HTTP delivery or full Cleanup23.
use super::*;
use oasis7_custody_core::{http::Role, native_cleanup_v2::*};
fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}
fn signing_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[97; 32])
}
fn pin(origin: &str) -> AccountMachineIssuerPinV2 {
    AccountMachineIssuerPinV2 {
        namespace: "account-fixture".into(),
        service_id: "account-service".into(),
        environment: "test".into(),
        key_id: "account-issuer".into(),
        public_key: hex(signing_key().verifying_key().to_bytes()),
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        not_before: DecimalU64(0),
        not_after: DecimalU64(100000),
        native_audience_origin: origin.into(),
        allowed_purpose: AccountMachinePurposeV2::NativePreflightAccessV2,
    }
}
fn issuer(p: &AccountMachineIssuerPinV2) -> AccountMachineIssuerV2 {
    AccountMachineIssuerV2 {
        namespace: p.namespace.clone(),
        service_id: p.service_id.clone(),
        environment: p.environment.clone(),
        key_id: p.key_id.clone(),
        issuer_epoch: p.issuer_epoch,
        config_generation: p.config_generation,
        native_audience_origin: p.native_audience_origin.clone(),
    }
}
fn floor() -> AccountMachineIssuerFloorV2 {
    AccountMachineIssuerFloorV2 {
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
    }
}
fn prefix(start: u64) -> (SessionFixture, TestSessionPreflightDescriptorV2) {
    let mut f = pending_fixture_with_profile(
        80,
        Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2),
    );
    let d = f
        .state
        .claim_test_session_v2_preflight(&f.session_id, UnixMillis::new(start))
        .unwrap();
    activation_tests::source_current(&mut f, &d, start);
    let p = Policy::new(f.state.draft());
    let c = p.current(&f.context);
    let ns = scope(&f.pin);
    for ticket in 1..=2 {
        let now = UnixMillis::new(start + 3 + u64::from(ticket));
        assert_eq!(
            f.state
                .reserve_test_session_v2_source_read(&f.session_id, &d.source_operation_id, now)
                .unwrap(),
            ticket + 1
        );
        assert_eq!(
            f.state
                .reserve_test_session_v2_preflight_read(
                    &f.session_id,
                    OwnerSetupReadPurposeV1::Session,
                    Role::Identity,
                    now
                )
                .unwrap(),
            ticket
        );
        let current = TestSessionPreflightCurrent {
            provision: &c,
            native_scope: &ns,
            source_operation_id: &d.source_operation_id,
            source_ticket: ticket + 1,
            oracle_ticket: ticket,
            observed_at: now,
        };
        f.state
            .test_session_v2_preflight_permit(
                &f.session_id,
                Role::Identity,
                OwnerSetupReadPurposeV1::Session,
                &current,
                now,
                &mut StrictCrypto,
            )
            .unwrap();
    }
    (f, d)
}
fn cold(state: &AccountProvisionApprovalState) {
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(state).unwrap()
        )
        .unwrap(),
        *state
    );
}
fn spent(state: &AccountProvisionApprovalState, id: &str) -> u64 {
    serde_json::to_value(state).unwrap()["test_sessions_v2"][id]["identity_transport"]["spent"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}
#[test]
fn world3_actual_consumed_book_reserves_native_four_and_separate_world_two_once() {
    let start = 9000;
    let (mut f, d) = prefix(start);
    let ns = scope(&f.pin);
    let p = pin(ns.http_origin);
    let floor = floor();
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_v2_preflight_step(
                &f.session_id,
                SessionPreflightStepV1::FreshAccessPair,
                UnixMillis::new(start + 6)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let prepared = f
        .state
        .claim_test_session_v2_preflight_world(
            &f.session_id,
            NativePreflightAccessIssuerV2 {
                issuer: issuer(&p),
                pin: &p,
                floor: &floor,
            },
            &ns,
            UnixMillis::new(start + 6),
        )
        .unwrap();
    assert_eq!(
        spent(&f.state, &f.session_id),
        spent(&before, &f.session_id) + 4
    );
    cold(&f.state);
    let signature = hex(signing_key()
        .sign(&prepared.signing_bytes().unwrap())
        .to_bytes());
    let signed = f
        .state
        .finish_test_session_v2_preflight_world(prepared, signature, UnixMillis::new(start + 6))
        .unwrap();
    signed.verify_issuer(&p, &floor, start + 6).unwrap();
    assert_eq!(signed.certificate.ports.len(), 2);
    assert_ne!(
        signed.certificate.ports[0].physical_attempt_id,
        signed.certificate.ports[1].physical_attempt_id
    );
    cold(&f.state);
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_v2_preflight_world(
                &f.session_id,
                NativePreflightAccessIssuerV2 {
                    issuer: issuer(&p),
                    pin: &p,
                    floor: &floor
                },
                &ns,
                UnixMillis::new(start + 6)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let access = activation_tests::fresh_access(&f.state, &d, start + 7);
    let results = [
        NativePreflightWorldReceiptV2::policy_result_digest(&access.policy).unwrap(),
        NativePreflightWorldReceiptV2::admission_result_digest(&access.admission).unwrap(),
    ];
    let receipts = std::array::from_fn(|i| {
        let port = &signed.certificate.ports[i];
        let receipt = NativePreflightWorldReceiptV2 {
            api_version: 2,
            native_key_id: ns.native_root_key_id.into(),
            certificate_digest: signed.digest().unwrap(),
            operation: port.operation,
            physical_attempt_id: port.physical_attempt_id.clone(),
            body_digest: port.body_digest.clone(),
            result_digest: results[i].clone(),
            completed_at: DecimalU64(start + 7),
        };
        let signature_hex = hex(key().sign(&receipt.signing_bytes().unwrap()).to_bytes());
        SignedNativePreflightWorldReceiptV2 {
            receipt,
            signature_hex,
        }
    });
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(
                &f.session_id,
                &d.source_operation_id,
                UnixMillis::new(start + 8)
            )
            .unwrap(),
        4
    );
    assert_eq!(
        f.state
            .reserve_test_session_v2_preflight_read(
                &f.session_id,
                OwnerSetupReadPurposeV1::Sign,
                Role::Owner,
                UnixMillis::new(start + 8)
            )
            .unwrap(),
        3
    );
    let c = TestSessionPreflightCurrent {
        provision: &current,
        native_scope: &ns,
        source_operation_id: &d.source_operation_id,
        source_ticket: 4,
        oracle_ticket: 3,
        observed_at: UnixMillis::new(start + 8),
    };
    // The old public path cannot promote the new profile from plain facts.
    let before = f.state.clone();
    assert!(
        f.state
            .record_test_session_v2_preflight_access(
                &f.session_id,
                access.clone(),
                &c,
                UnixMillis::new(start + 8),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    // A valid Native signature over the wrong paid tuple must still reject.
    let mut wrong = receipts.clone();
    wrong[1].receipt.physical_attempt_id = "fa".repeat(32);
    wrong[1].signature_hex = hex(key()
        .sign(&wrong[1].receipt.signing_bytes().unwrap())
        .to_bytes());
    assert!(
        f.state
            .record_test_session_v2_preflight_world_access(
                &f.session_id,
                access.clone(),
                wrong,
                &c,
                UnixMillis::new(start + 8),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    f.state
        .record_test_session_v2_preflight_world_access(
            &f.session_id,
            access.clone(),
            receipts.clone(),
            &c,
            UnixMillis::new(start + 8),
            &mut StrictCrypto,
        )
        .unwrap();
    cold(&f.state);
    assert_eq!(
        f.state
            .test_session_v2_snapshot(&f.session_id)
            .unwrap()
            .preflight_world_receipts(),
        Some(receipts.clone())
    );
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(
                &f.session_id,
                &d.source_operation_id,
                UnixMillis::new(start + 9)
            )
            .unwrap(),
        5
    );
    assert_eq!(
        f.state
            .reserve_test_session_v2_preflight_read(
                &f.session_id,
                OwnerSetupReadPurposeV1::Sign,
                Role::Owner,
                UnixMillis::new(start + 9)
            )
            .unwrap(),
        4
    );
    let c = TestSessionPreflightCurrent {
        source_ticket: 5,
        oracle_ticket: 4,
        observed_at: UnixMillis::new(start + 9),
        ..c
    };
    let before = f.state.clone();
    f.state
        .record_test_session_v2_preflight_world_access(
            &f.session_id,
            access,
            receipts,
            &c,
            UnixMillis::new(start + 9),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(f.state, before); // Historical callback4 adds no World or Native claim.
    cold(&f.state);
    let mut missing = serde_json::to_value(&f.state).unwrap();
    missing["test_sessions_v2"][&f.session_id]
        .as_object_mut()
        .unwrap()
        .remove("preflight_world_access");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
}

// Shared actual allocator/receipt fixtures for the final registered-machine gate.
pub(super) fn allocate_world_fixture(
    state: &mut AccountProvisionApprovalState,
    id: &str,
    native: &NativeStageScope<'_>,
    now: u64,
) -> SignedAccountPreflightAccessV2 {
    let p = pin(native.http_origin);
    let floor = floor();
    let prepared = state
        .claim_test_session_v2_preflight_world(
            id,
            NativePreflightAccessIssuerV2 {
                issuer: issuer(&p),
                pin: &p,
                floor: &floor,
            },
            native,
            UnixMillis::new(now),
        )
        .unwrap();
    let signature = hex(signing_key()
        .sign(&prepared.signing_bytes().unwrap())
        .to_bytes());
    state
        .finish_test_session_v2_preflight_world(prepared, signature, UnixMillis::new(now))
        .unwrap()
}
pub(super) fn native_world_receipts(
    original: &SignedAccountPreflightAccessV2,
    access: &OwnerSetupAccessEvidenceV1,
    native: &NativeStageScope<'_>,
    now: u64,
) -> [SignedNativePreflightWorldReceiptV2; 2] {
    let results = [
        NativePreflightWorldReceiptV2::policy_result_digest(&access.policy).unwrap(),
        NativePreflightWorldReceiptV2::admission_result_digest(&access.admission).unwrap(),
    ];
    std::array::from_fn(|i| {
        let port = &original.certificate.ports[i];
        let receipt = NativePreflightWorldReceiptV2 {
            api_version: 2,
            native_key_id: native.native_root_key_id.into(),
            certificate_digest: original.digest().unwrap(),
            operation: port.operation,
            physical_attempt_id: port.physical_attempt_id.clone(),
            body_digest: port.body_digest.clone(),
            result_digest: results[i].clone(),
            completed_at: DecimalU64(now),
        };
        let signature_hex = hex(key().sign(&receipt.signing_bytes().unwrap()).to_bytes());
        SignedNativePreflightWorldReceiptV2 {
            receipt,
            signature_hex,
        }
    })
}
