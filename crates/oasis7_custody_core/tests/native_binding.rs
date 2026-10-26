use oasis7_custody_core::{DecimalU64 as D, native_binding::*};
fn challenge() -> NativeBindingChallengeV1 {
    NativeBindingChallengeV1 {
        api_version: 1,
        challenge_id: "challenge".into(),
        nonce: "12".repeat(32),
        identity_namespace: "native".into(),
        identity_instance_epoch: D(1),
        registration_operation_id: "registration".into(),
        registration_request_digest: "13".repeat(32),
        registration_receipt_digest: "14".repeat(32),
        native_account_id: "account".into(),
        native_credential_ref: "credential".into(),
        login_public_key: "15".repeat(32),
        owner_context_ref: "16".repeat(32),
        issued_at: D(100),
        expires_at: D(30_100),
    }
}
fn statement() -> OwnerNativeBindingStatementV1 {
    OwnerNativeBindingStatementV1 {
        challenge: challenge(),
        environment: "test".into(),
        custody_account_id: "custody".into(),
        provision_id: "provision".into(),
        approved_plan_digest: "17".repeat(32),
        login_key_ref: "login".into(),
        binding_operation_id: "challenge".into(),
        permit_id: "permit".into(),
        source_revision: "18".repeat(32),
        account_authority_revision: "19".repeat(32),
        service_epoch: D(1),
        service_identity: "owner".into(),
        signer_key_id: "service-key".into(),
        authority_observed_at: D(200),
        expires_at: D(5200),
    }
}
#[test]
fn nonce_and_statement_have_independent_fixed_deadlines() {
    let c = challenge();
    assert!(c.validate_at(10_000).is_ok());
    let mut s = statement();
    assert!(s.validate_at(5199).is_ok());
    assert!(s.validate_at(5200).is_err());
    s.expires_at = D(5201);
    assert!(s.validate().is_err());
    let mut c = c;
    c.expires_at = D(30_101);
    assert!(c.validate().is_err());
}
#[test]
fn domains_and_full_width_integers_are_lossless() {
    let c = challenge();
    let s = statement();
    assert_ne!(c.signing_bytes().unwrap(), s.signing_bytes().unwrap());
    let mut v = serde_json::to_value(&c).unwrap();
    v["identity_instance_epoch"] = serde_json::json!("18446744073709551615");
    let restored: NativeBindingChallengeV1 = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(restored.identity_instance_epoch.0, u64::MAX);
    v["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<NativeBindingChallengeV1>(v).is_err());
}
#[test]
fn current_pin_is_external_and_exact() {
    let s = statement();
    let mut p = NativeBindingPinV1 {
        key_id: s.signer_key_id.clone(),
        public_key: "20".repeat(32),
        environment: s.environment.clone(),
        service_identity: s.service_identity.clone(),
        service_epoch: s.service_epoch,
    };
    assert!(p.validate_statement(&s, 200).is_ok());
    p.service_epoch = D(2);
    assert!(p.validate_statement(&s, 200).is_err());
}
#[test]
fn signatures_require_canonical_unpadded_base64url() {
    let mut c = SignedNativeBindingChallengeV1 {
        challenge: challenge(),
        key_id: "native-key".into(),
        signature: "A".repeat(86),
    };
    assert!(c.validate().is_ok());
    c.signature.replace_range(85.., "B");
    assert!(c.validate().is_err());
}

#[test]
fn statement_scope_must_match_registered_identity_slot_and_original() {
    let s = statement();
    let mut scope = NativeBindingScopeV1 {
        environment: s.environment.clone(),
        custody_account_id: s.custody_account_id.clone(),
        provision_id: s.provision_id.clone(),
        approved_plan_digest: s.approved_plan_digest.clone(),
        identity_namespace: s.challenge.identity_namespace.clone(),
        identity_instance_epoch: s.challenge.identity_instance_epoch,
        registration_operation_id: s.challenge.registration_operation_id.clone(),
        registration_request_digest: s.challenge.registration_request_digest.clone(),
        login_key: oasis7_custody_core::provisioning::PublicRoleKey {
            key_id: s.login_key_ref.clone(),
            key_version: D(1),
            public_key: s.challenge.login_public_key.clone(),
        },
        plan_expires_at: D(6000),
        source_expires_at: D(6000),
    };
    assert!(s.validate_for(&scope, 200).is_ok());
    scope.login_key.public_key = "22".repeat(32);
    assert!(s.validate_for(&scope, 200).is_err());
    scope.login_key.public_key = s.challenge.login_public_key.clone();
    scope.source_expires_at = D(5199);
    assert!(s.validate_for(&scope, 200).is_err());
}
#[test]
fn receipt_recovers_historical_fact_without_renewing_statement() {
    let s = statement();
    let mut r = NativeBindingReceiptV1 {
        api_version: 1,
        binding_operation_id: s.binding_operation_id.clone(),
        statement_digest: s.digest().unwrap(),
        environment: s.environment.clone(),
        custody_account_id: s.custody_account_id.clone(),
        provision_id: s.provision_id.clone(),
        identity_namespace: s.challenge.identity_namespace.clone(),
        identity_instance_epoch: s.challenge.identity_instance_epoch,
        native_account_id: s.challenge.native_account_id.clone(),
        native_credential_ref: s.challenge.native_credential_ref.clone(),
        login_public_key: s.challenge.login_public_key.clone(),
        login_key_ref: s.login_key_ref.clone(),
        registration_operation_id: s.challenge.registration_operation_id.clone(),
        registration_receipt_digest: s.challenge.registration_receipt_digest.clone(),
        native_account_security_epoch: D(1),
        native_credential_version: D(1),
        committed_at: D(300),
    };
    assert!(r.validate_for(&s).is_ok());
    assert_ne!(r.signing_bytes().unwrap(), s.signing_bytes().unwrap());
    r.committed_at = D(5200);
    assert!(r.validate_for(&s).is_err());
    r.committed_at = D(300);
    r.native_account_id = "other".into();
    assert!(r.validate_for(&s).is_err());
}

#[test]
fn public_registration_digest_and_possession_have_fixed_domains() {
    let s = statement();
    let mut request = NativeRegistrationRequestV1 {
        identity_namespace: s.challenge.identity_namespace.clone(),
        identity_instance_epoch: D(1),
        registration_operation_id: s.challenge.registration_operation_id.clone(),
        login_public_key: s.challenge.login_public_key.clone(),
        display_name: "Managed account".into(),
        audience: "https://identity.example".into(),
    };
    let original = request.digest().unwrap();
    request.display_name = "Other account".into();
    assert_ne!(request.digest().unwrap(), original);
    assert_ne!(
        s.signing_bytes().unwrap(),
        s.possession_signing_bytes().unwrap()
    );
    let mut changed = s.clone();
    changed.permit_id = "other-permit".into();
    assert_ne!(
        s.possession_signing_bytes().unwrap(),
        changed.possession_signing_bytes().unwrap()
    );
    let mut complete = NativeBindingCompleteV1 {
        api_version: 1,
        statement: SignedOwnerNativeBindingStatementV1 {
            statement: s,
            signature: "A".repeat(86),
        },
        login_key_signature: "A".repeat(86),
    };
    assert!(complete.validate().is_ok());
    complete.login_key_signature.push('=');
    assert!(complete.validate().is_err());
}
fn receipt_fixture() -> NativeBindingReceiptV1 {
    let s = statement();
    NativeBindingReceiptV1 {
        api_version: 1,
        binding_operation_id: s.binding_operation_id.clone(),
        statement_digest: s.digest().unwrap(),
        environment: s.environment.clone(),
        custody_account_id: s.custody_account_id.clone(),
        provision_id: s.provision_id.clone(),
        identity_namespace: s.challenge.identity_namespace.clone(),
        identity_instance_epoch: s.challenge.identity_instance_epoch,
        native_account_id: s.challenge.native_account_id.clone(),
        native_credential_ref: s.challenge.native_credential_ref.clone(),
        login_public_key: s.challenge.login_public_key.clone(),
        login_key_ref: s.login_key_ref.clone(),
        registration_operation_id: s.challenge.registration_operation_id.clone(),
        registration_receipt_digest: s.challenge.registration_receipt_digest.clone(),
        native_account_security_epoch: D(1),
        native_credential_version: D(1),
        committed_at: D(300),
    }
}
#[test]
fn online_status_binds_exact_nonce_generation_and_deadline() {
    let r = receipt_fixture();
    let mut v = ManagedNativeSessionStatusV1 {
        api_version: 1,
        receipt: r.clone(),
        service_epoch: D(1),
        native_account_security_epoch: D(1),
        native_credential_version: D(1),
        session_ref: "session-ref".into(),
        nonce: "ab".repeat(32),
        generation: D(1),
        active: true,
        frozen: false,
        pending: false,
        observed_at: D(400),
        expires_at: D(5400),
    };
    assert!(v.validate_for(&r, &v.nonce, D(1), D(1), 5399).is_ok());
    assert!(v.validate_for(&r, &v.nonce, D(1), D(1), 5400).is_err());
    assert!(
        v.validate_for(&r, &"cd".repeat(32), D(1), D(1), 400)
            .is_err()
    );
    assert!(v.validate_for(&r, &v.nonce, D(2), D(1), 400).is_err());
    v.pending = true;
    assert!(v.validate().is_err());
}
#[test]
fn invalidation_is_permanent_monotonic_and_cannot_regress_security_versions() {
    let r = receipt_fixture();
    let mut v = ManagedNativeInvalidationV1 {
        api_version: 1,
        receipt: r.clone(),
        service_epoch: D(1),
        native_account_security_epoch: D(2),
        native_credential_version: D(1),
        session_ref: None,
        generation: D(1),
        reason: "account_invalidated".into(),
        sequence: D(2),
        changed_at: D(500),
    };
    assert!(v.validate_for(&r, D(1), D(1)).is_ok());
    assert!(v.validate_for(&r, D(1), D(2)).is_err());
    v.native_account_security_epoch = D(0);
    assert!(v.validate().is_err());
    v.native_account_security_epoch = D(2);
    v.reason = "active".into();
    assert!(v.validate().is_err());
}
