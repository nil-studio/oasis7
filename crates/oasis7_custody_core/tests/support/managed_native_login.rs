use super::*;
#[test]
fn lost_cookie_requires_fresh_positive_managed_login_and_independent_budget() {
    let (mut s, c, pin) = binding_ready();
    let p = Policy::new(s.draft());
    claim(
        &mut s,
        &c,
        &pin,
        "lost-login",
        "binding-one",
        NativeStageKindV1::Login,
        1420,
    );
    s.complete_native_stage(
        "lost-login",
        NativeStageResultV1::ExplicitLoginRequired,
        &scope(&pin),
        &p.current(&c),
        1424,
        None,
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision().stage(), ProvisionStage::NativeRegistered);
    assert!(
        s.claim_native_stage(
            "lost-login",
            &scope(&pin),
            &p.current(&c),
            1424,
            &mut StrictCrypto
        )
        .is_err()
    );
    let provision_balance = s.provision().transport_remaining().unwrap();
    let (mut input, _) = super::super::super::fixture();
    input.operation_id = "fresh-managed-login".into();
    input.request.approval_request_id = "managed-approval-request".into();
    input.request.context = c.authority.clone();
    let receipt = s
        .native_stages()
        .values()
        .find_map(|v| match &v.result {
            Some(NativeStageResultV1::BindingReceipt(v)) => Some(v.receipt.clone()),
            _ => None,
        })
        .unwrap();
    input.request.intent = CustodyIntentV1::ManagedLogin {
        native_account_id: receipt.native_account_id.clone(),
        login_key_ref: receipt.login_key_ref.clone(),
        native_identity_namespace: receipt.identity_namespace.clone(),
        blocked_operation_ref: "lost-login".into(),
        login_request_id: "new-explicit-login-operation".into(),
        transport_limit: DecimalU64(64),
        deadline: DecimalU64(20_000),
    };
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.approval_request_id = input.request.approval_request_id.clone();
    input.approval.proof.approval_id = "fresh-managed-approval".into();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.nonce = "fresh-managed-proof-nonce".into();
    input.approval.proof.issued_at = DecimalU64(1430);
    input.approval.proof.expires_at = DecimalU64(31_430);
    input.approval = sign_proof(input.approval.proof);
    let pending = s
        .begin_managed_login(
            input,
            "new-native-verify-nonce".into(),
            &p.current(&c),
            1431,
            &mut StrictCrypto,
        )
        .unwrap();
    let mut signed = verification(&pending, &c).verification;
    signed.observed_at = DecimalU64(1432);
    let signed = sign_verification(signed);
    s.commit_managed_login(
        "fresh-managed-login",
        signed,
        &p.current(&c),
        1433,
        &"91".repeat(32),
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(
        s.provision().transport_remaining().unwrap(),
        provision_balance
    );
    assert_eq!(
        s.reserve_managed_login_read("fresh-managed-login", 1433)
            .unwrap(),
        1
    );
    assert!(
        s.authorize_managed_login_read(
            "fresh-managed-login",
            1,
            &scope(&pin),
            &p.current(&c),
            1433,
            &mut StrictCrypto
        )
        .is_err()
    );
    for ticket in 2..=8 {
        assert_eq!(
            s.reserve_managed_login_read("fresh-managed-login", 1433)
                .unwrap(),
            ticket
        );
    }
    assert!(
        s.reserve_managed_login_read("fresh-managed-login", 1433)
            .is_err()
    );
    let task = s
        .claim_managed_login(
            "fresh-managed-login",
            &scope(&pin),
            &p.current(&c),
            1434,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(task.transport.remaining().unwrap(), 0);
    assert!(
        s.claim_managed_login(
            "fresh-managed-login",
            &scope(&pin),
            &p.current(&c),
            1434,
            &mut StrictCrypto
        )
        .is_err()
    );
    let status = ManagedNativeSessionStatusV1 {
        api_version: 1,
        receipt,
        service_epoch: c.authority.service_epoch,
        native_account_security_epoch: DecimalU64(1),
        native_credential_version: DecimalU64(1),
        session_ref: "fresh-native-session".into(),
        nonce: task.status_nonce.clone(),
        generation: DecimalU64(1),
        active: true,
        frozen: false,
        pending: false,
        observed_at: DecimalU64(1435),
        expires_at: DecimalU64(6435),
    };
    let signed = SignedManagedNativeSessionStatusV1 {
        signature: URL_SAFE_NO_PAD.encode(key().sign(&status.signing_bytes().unwrap()).to_bytes()),
        key_id: "native-key".into(),
        status: status.clone(),
    };
    let digest = status.digest().unwrap();
    s.complete_managed_login(
        "fresh-managed-login",
        NativeStageResultV1::SessionStatus(signed),
        &scope(&pin),
        &p.current(&c),
        1436,
        Some(&PersistedNativeSessionAttestation {
            status_digest: &digest,
            session_ref: &status.session_ref,
            private_sid_generation: 2,
        }),
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision().stage(), ProvisionStage::NativeSessionReady);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}

#[test]
fn managed_login_proof_is_unique_across_account_books_pending_and_consumed() {
    let (mut s, c, pin) = binding_ready();
    let p = Policy::new(s.draft());
    claim(
        &mut s,
        &c,
        &pin,
        "lost-login",
        "binding-one",
        NativeStageKindV1::Login,
        1420,
    );
    s.complete_native_stage(
        "lost-login",
        NativeStageResultV1::ExplicitLoginRequired,
        &scope(&pin),
        &p.current(&c),
        1424,
        None,
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision().stage(), ProvisionStage::NativeRegistered);
    assert!(
        s.claim_native_stage(
            "lost-login",
            &scope(&pin),
            &p.current(&c),
            1424,
            &mut StrictCrypto
        )
        .is_err()
    );
    let provision_balance = s.provision().transport_remaining().unwrap();
    let (mut input, _) = super::super::super::fixture();
    input.operation_id = "fresh-managed-login".into();
    input.request.approval_request_id = "managed-approval-request".into();
    input.request.context = c.authority.clone();
    let receipt = s
        .native_stages()
        .values()
        .find_map(|v| match &v.result {
            Some(NativeStageResultV1::BindingReceipt(v)) => Some(v.receipt.clone()),
            _ => None,
        })
        .unwrap();
    input.request.intent = CustodyIntentV1::ManagedLogin {
        native_account_id: receipt.native_account_id.clone(),
        login_key_ref: receipt.login_key_ref.clone(),
        native_identity_namespace: receipt.identity_namespace.clone(),
        blocked_operation_ref: "lost-login".into(),
        login_request_id: "new-explicit-login-operation".into(),
        transport_limit: DecimalU64(64),
        deadline: DecimalU64(20_000),
    };
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.approval_request_id = input.request.approval_request_id.clone();
    input.approval.proof.approval_id = "fresh-managed-approval".into();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.nonce = "fresh-managed-proof-nonce".into();
    input.approval.proof.issued_at = DecimalU64(1430);
    input.approval.proof.expires_at = DecimalU64(31_430);
    input.approval = sign_proof(input.approval.proof);
    let pending = s
        .begin_managed_login(
            input,
            "new-native-verify-nonce".into(),
            &p.current(&c),
            1431,
            &mut StrictCrypto,
        )
        .unwrap();
    let mut replay = pending.input().clone();
    replay.operation_id = "changed-operation-id".into();
    let mut independent = ApprovalAdmissionBook::default();
    independent
        .begin(
            replay.clone(),
            "independent-verify".into(),
            &c,
            1432,
            &mut StrictCrypto,
        )
        .unwrap();
    let mut persisted = serde_json::to_value(&s).unwrap();
    persisted["managed_logins"]["changed-operation-id"] = json!({"book":independent,"task":null});
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(persisted).is_err(),
        "restore must reject otherwise-valid duplicate books"
    );
    let mut alias = replay.clone();
    alias.operation_id = "changed-proof-nonce".into();
    alias.approval.proof.nonce = "different-nonce-same-native-proof-reference".into();
    alias.approval = sign_proof(alias.approval.proof);
    assert!(
        s.begin_managed_login_checks(&alias, "alias-verify", &p.current(&c), 1432)
            .is_err(),
        "the same native proof reference cannot be renamed with another nonce"
    );
    let before = s.clone();
    assert!(
        s.begin_managed_login_checks(&replay, "other-verify-nonce", &p.current(&c), 1432)
            .is_err(),
        "same signed request/proof cannot be admitted into another pending book"
    );
    assert!(
        s.begin_managed_login(
            replay.clone(),
            "other-verify-nonce".into(),
            &p.current(&c),
            1432,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(s, before);
    let mut signed = verification(&pending, &c).verification;
    signed.observed_at = DecimalU64(1432);
    s.commit_managed_login(
        "fresh-managed-login",
        sign_verification(signed),
        &p.current(&c),
        1433,
        &"b1".repeat(32),
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(
        s.provision().transport_remaining().unwrap(),
        provision_balance
    );
    assert!(
        s.begin_managed_login(
            replay,
            "third-verify-nonce".into(),
            &p.current(&c),
            1434,
            &mut StrictCrypto
        )
        .is_err(),
        "consumption cannot free the original nonce"
    );
    let exact = pending.input().clone();
    assert!(
        s.begin_managed_login(
            exact,
            pending.verify_nonce().into(),
            &p.current(&c),
            1434,
            &mut StrictCrypto
        )
        .is_ok()
    );
}
