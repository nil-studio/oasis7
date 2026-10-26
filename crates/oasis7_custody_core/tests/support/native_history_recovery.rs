use super::*;
use oasis7_custody_core::native_history::*;
fn history_scope(
    s: &AccountProvisionApprovalState,
    pin: &NativeBindingPinV1,
) -> NativeHistoryScopeV1 {
    let d = s.native_stage_descriptor(&scope(pin)).unwrap();
    NativeHistoryScopeV1 {
        role_scope: s.role_key_results()["identity"].scope.clone(),
        identity_namespace: d.registration.identity_namespace,
        identity_instance_epoch: d.registration.identity_instance_epoch,
        registration_operation_id: d.registration.registration_operation_id,
        initial_login_operation_id: d.login_request_id,
    }
}
fn archive_entry(
    s: &AccountProvisionApprovalState,
    pin: &NativeBindingPinV1,
    phase: NativeHistoryPhaseV1,
    permit: &str,
    native_operation: String,
    result: NativeStageResultV1,
    generation: u64,
) -> NativeHistoryEntryV1 {
    let scope = history_scope(s, pin);
    let session_attestation = match &result {
        NativeStageResultV1::SessionStatus(v) => Some(NativeSessionStorageAttestationV1 {
            status_digest: v.status.digest().unwrap(),
            session_ref: v.status.session_ref.clone(),
            private_sid_generation: DecimalU64(generation),
        }),
        _ => None,
    };
    NativeHistoryEntryV1 {
        api_version: 1,
        environment: s.draft().entry.environment.clone(),
        custody_account_id: s.draft().entry.custody_account_id.clone(),
        provision_id: s.draft().entry.provision_id.clone(),
        stable_intent_digest: stable_native_intent_digest(&scope, phase, &native_operation)
            .unwrap(),
        scope,
        phase,
        native_operation_id: native_operation,
        original_permit_id: permit.into(),
        result_permit_id: permit.into(),
        result,
        session_attestation,
    }
}
fn public_history_context(pin: &NativeBindingPinV1) -> NativeHistoryVerificationContext<'_> {
    NativeHistoryVerificationContext {
        native_root_key_id: "native-key",
        native_root_public_key: &pin.public_key,
        http_origin: "https://identity.test",
    }
}
fn archived_initial() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    NativeHistoryEntryV1,
) {
    let (mut s, c, pin) = binding_ready();
    let permit = claim(
        &mut s,
        &c,
        &pin,
        "archived-login",
        "binding-one",
        NativeStageKindV1::Login,
        1420,
    );
    let receipt = s
        .native_stages()
        .values()
        .find_map(|v| match &v.result {
            Some(NativeStageResultV1::BindingReceipt(v)) => Some(v.receipt.clone()),
            _ => None,
        })
        .unwrap();
    let status = ManagedNativeSessionStatusV1 {
        api_version: 1,
        receipt,
        service_epoch: c.authority.service_epoch,
        native_account_security_epoch: DecimalU64(1),
        native_credential_version: DecimalU64(1),
        session_ref: "initial-cookie-session".into(),
        nonce: permit.nonce.clone(),
        generation: DecimalU64(1),
        active: true,
        frozen: false,
        pending: false,
        observed_at: DecimalU64(1424),
        expires_at: DecimalU64(6424),
    };
    let signed = SignedManagedNativeSessionStatusV1 {
        key_id: "native-key".into(),
        signature: URL_SAFE_NO_PAD.encode(key().sign(&status.signing_bytes().unwrap()).to_bytes()),
        status,
    };
    let entry = archive_entry(
        &s,
        &pin,
        NativeHistoryPhaseV1::InitialLogin,
        "archived-login",
        permit.descriptor.login_request_id,
        NativeStageResultV1::SessionStatus(signed),
        1,
    );
    s.archive_native_history(
        entry.clone(),
        &public_history_context(&pin),
        &mut StrictCrypto,
    )
    .unwrap();
    (s, c, pin, entry)
}
#[test]
fn historical_root_status_archives_without_liveness_budget_epoch_or_unfreeze() {
    let (mut s, _c, pin, entry) = archived_initial();
    assert_eq!(s.provision().stage(), ProvisionStage::NativeRegistered);
    let before = s.provision().clone();
    s.freeze(2).unwrap();
    let frozen = s.provision().clone();
    let mut newfact = entry.clone();
    newfact.session_attestation = None;
    s.archive_native_history(newfact, &public_history_context(&pin), &mut StrictCrypto)
        .unwrap();
    assert_eq!(s.provision(), &frozen);
    assert_eq!(
        before.transport_remaining(),
        s.provision().transport_remaining()
    );
    assert!(s.frozen());
    s.archive_native_history(
        entry.clone(),
        &public_history_context(&pin),
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision(), &frozen);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
    let mut bad = entry;
    bad.stable_intent_digest = "fe".repeat(32);
    assert!(
        s.archive_native_history(bad, &public_history_context(&pin), &mut StrictCrypto)
            .is_err()
    );
}
fn recovery_request(
    s: &AccountProvisionApprovalState,
    entry: &NativeHistoryEntryV1,
    id: &str,
) -> SessionStatusRecoveryRequestV1 {
    SessionStatusRecoveryRequestV1 {
        api_version: 1,
        custody_account_id: s.draft().entry.custody_account_id.clone(),
        provision_id: s.draft().entry.provision_id.clone(),
        operation_id: id.into(),
        history_digest: entry.digest().unwrap(),
    }
}
fn signed_recovery_origin(
    s: &AccountProvisionApprovalState,
    c: &ApprovalAdmissionContext,
    id: &str,
    now: u64,
) -> SignedApprovalVerificationV1 {
    let r = &s.session_status_recoveries()[id];
    let record = match &r.funding {
        SessionStatusRecoveryFundingV1::Provision => s.accepted_approval().unwrap(),
        SessionStatusRecoveryFundingV1::ManagedLogin { operation_id } => {
            s.managed_login_original(operation_id).unwrap()
        }
    };
    let mut v = verification(record, c).verification;
    v.purpose = VerificationPurposeV1::InspectOrigin;
    v.verify_nonce = r.source_inspection.verify_nonce.clone();
    v.observed_at = DecimalU64(now);
    v.proof_state = if now >= v.proof_expires_at.0 {
        ProofStateV1::Expired
    } else {
        ProofStateV1::Active
    };
    sign_verification(v)
}
#[test]
fn paid_readonly_recovery_uses_new_nonce_and_fresh_root_status_only() {
    let (mut s, c, pin, entry) = archived_initial();
    let before = s.provision().transport_remaining().unwrap();
    let request = recovery_request(&s, &entry, "status-recovery");
    s.reserve_session_status_recovery(
        request,
        "d1".repeat(32).as_str(),
        "d2".repeat(32).as_str(),
        &scope(&pin),
        40_000,
    )
    .unwrap();
    assert_eq!(s.provision().transport_remaining().unwrap(), before - 64);
    let p = Policy::new(s.draft());
    assert_eq!(
        s.reserve_session_status_recovery_read("status-recovery", 40_000)
            .unwrap(),
        1
    );
    assert!(
        s.authorize_session_status_recovery_read(
            "status-recovery",
            1,
            &scope(&pin),
            &p.current(&c),
            40_000,
            &mut StrictCrypto
        )
        .is_err()
    );
    s.claim_session_recovery_origin("status-recovery", &p.current(&c), 40_000)
        .unwrap();
    let signed = signed_recovery_origin(&s, &c, "status-recovery", 40_001);
    s.complete_session_recovery_origin(
        "status-recovery",
        signed,
        &p.current(&c),
        40_002,
        &mut StrictCrypto,
    )
    .unwrap();
    let view = s
        .claim_session_status_recovery(
            "status-recovery",
            &scope(&pin),
            &p.current(&c),
            40_003,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(view.history, entry);
    assert_eq!(view.private_sid_generation, DecimalU64(1));
    let NativeStageResultV1::SessionStatus(old) = &entry.result else {
        panic!()
    };
    assert!(
        s.session_status_recovery_result_checks(
            "status-recovery",
            old,
            &scope(&pin),
            &p.current(&c),
            40_004
        )
        .is_err()
    );
    assert_eq!(s.provision().stage(), ProvisionStage::NativeRegistered);
    let mut status = old.status.clone();
    status.nonce = view.nonce;
    status.observed_at = DecimalU64(40_004);
    status.expires_at = DecimalU64(45_004);
    let signed = SignedManagedNativeSessionStatusV1 {
        key_id: "native-key".into(),
        signature: URL_SAFE_NO_PAD.encode(key().sign(&status.signing_bytes().unwrap()).to_bytes()),
        status: status.clone(),
    };
    let digest = status.digest().unwrap();
    let before = s.clone();
    assert!(
        s.complete_session_status_recovery(
            "status-recovery",
            signed.clone(),
            &scope(&pin),
            &p.current(&c),
            40_005,
            &PersistedNativeSessionAttestation {
                status_digest: &digest,
                session_ref: &status.session_ref,
                private_sid_generation: 2
            },
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(s, before);
    s.complete_session_status_recovery(
        "status-recovery",
        signed,
        &scope(&pin),
        &p.current(&c),
        40_005,
        &PersistedNativeSessionAttestation {
            status_digest: &digest,
            session_ref: &status.session_ref,
            private_sid_generation: 1,
        },
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision().stage(), ProvisionStage::NativeSessionReady);
    assert_eq!(s.native_history()[&entry.digest().unwrap()], entry);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}
#[test]
fn recovery_rejects_frozen_expired_source_and_exhausted_budget_without_minting_credit() {
    let (mut s, c, pin, entry) = archived_initial();
    let request = recovery_request(&s, &entry, "frozen-recovery");
    let mut frozen = s.clone();
    frozen.freeze(2).unwrap();
    let before = frozen.clone();
    assert!(
        frozen
            .reserve_session_status_recovery(
                request.clone(),
                &"e1".repeat(32),
                &"e2".repeat(32),
                &scope(&pin),
                40_000
            )
            .is_err()
    );
    assert_eq!(frozen, before);
    let before = s.clone();
    assert!(
        s.reserve_session_status_recovery(
            request,
            &"e1".repeat(32),
            &"e2".repeat(32),
            &scope(&pin),
            100_000
        )
        .is_err()
    );
    assert_eq!(s, before);
    for index in 0..3 {
        s.reserve_session_status_recovery(
            recovery_request(&s, &entry, &format!("recovery-{index}")),
            &format!("{:064x}", index + 0x100),
            &format!("{:064x}", index + 0x200),
            &scope(&pin),
            40_000,
        )
        .unwrap();
    }
    let before = s.clone();
    assert!(
        s.reserve_session_status_recovery(
            recovery_request(&s, &entry, "exhausted-recovery"),
            &"e3".repeat(32),
            &"e4".repeat(32),
            &scope(&pin),
            40_000
        )
        .is_err()
    );
    assert_eq!(s, before);
    let p = Policy::new(s.draft());
    let mut revoked = c;
    revoked.frozen = true;
    assert!(
        s.claim_session_recovery_origin("recovery-0", &p.current(&revoked), 40_001)
            .is_err()
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}

#[test]
fn managed_recovery_uses_only_that_approval_balance_never_provision_credit() {
    for limit in [64, 128] {
        let (mut s, c, pin, status) = super::managed_native_login::managed_unknown_status(limit);
        let task = s.managed_login_task("fresh-managed-login").unwrap();
        let entry = archive_entry(
            &s,
            &pin,
            NativeHistoryPhaseV1::ManagedLogin,
            "fresh-managed-login",
            task.login_operation_id.clone(),
            NativeStageResultV1::SessionStatus(status),
            2,
        );
        s.archive_native_history(
            entry.clone(),
            &public_history_context(&pin),
            &mut StrictCrypto,
        )
        .unwrap();
        let provision_balance = s.provision().transport_remaining().unwrap();
        let before = s.clone();
        let result = s.reserve_session_status_recovery(
            recovery_request(&s, &entry, "managed-status-recovery"),
            &"f1".repeat(32),
            &"f2".repeat(32),
            &scope(&pin),
            10_000,
        );
        if limit == 64 {
            assert!(result.is_err());
            assert_eq!(s, before);
        } else {
            result.unwrap();
            assert_eq!(
                s.provision().transport_remaining().unwrap(),
                provision_balance
            );
            assert_eq!(
                s.managed_login_task("fresh-managed-login")
                    .unwrap()
                    .transport
                    .remaining()
                    .unwrap(),
                0
            );
            let p = Policy::new(s.draft());
            s.claim_session_recovery_origin("managed-status-recovery", &p.current(&c), 10_000)
                .unwrap();
            let signed = signed_recovery_origin(&s, &c, "managed-status-recovery", 10_001);
            s.complete_session_recovery_origin(
                "managed-status-recovery",
                signed,
                &p.current(&c),
                10_002,
                &mut StrictCrypto,
            )
            .unwrap();
            let view = s
                .claim_session_status_recovery(
                    "managed-status-recovery",
                    &scope(&pin),
                    &p.current(&c),
                    10_003,
                    &mut StrictCrypto,
                )
                .unwrap();
            assert_eq!(view.private_sid_generation, DecimalU64(2));
            assert_eq!(view.native_binding_generation, DecimalU64(1));
            assert_eq!(
                serde_json::from_value::<AccountProvisionApprovalState>(
                    serde_json::to_value(&s).unwrap()
                )
                .unwrap(),
                s
            );
        }
    }
}

#[path = "native_history_contract.rs"]
mod native_history_contract;
