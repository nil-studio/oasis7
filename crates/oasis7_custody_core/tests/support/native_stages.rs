use super::*;
use oasis7_custody_core::native_binding::*;
fn ready() -> (AccountProvisionApprovalState, ApprovalAdmissionContext) {
    let (mut s, c) = attempt_approved();
    let p = Policy::new(s.draft());
    for (role, byte) in [("identity", "11"), ("owner", "22"), ("executor", "33")] {
        let r = role_result(&s, role, byte);
        s.record_role_key_result(r, &p.current(&c), 1300).unwrap();
    }
    (s, c)
}
fn owner_pin() -> NativeBindingPinV1 {
    NativeBindingPinV1 {
        key_id: "owner-service".into(),
        public_key: key()
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect(),
        environment: "test".into(),
        service_identity: "owner".into(),
        service_epoch: DecimalU64(3),
    }
}
fn request(s: &AccountProvisionApprovalState) -> NativeStageRequestV1 {
    NativeStageRequestV1 {
        api_version: 1,
        custody_account_id: s.draft().entry.custody_account_id.clone(),
        provision_id: s.draft().entry.provision_id.clone(),
        operation_id: "native-attempt".into(),
        binding_operation_id: "binding-attempt".into(),
        stage: NativeStageKindV1::Registration,
    }
}
#[test]
fn native_budget_is_atomic_64_with_distinct_8_read_quota_and_no_repeat_claim() {
    let (mut s, c) = ready();
    let p = Policy::new(s.draft());
    let pin = owner_pin();
    let rootkey = pin.public_key.clone();
    let scope = NativeStageScope {
        http_origin: "https://identity.test",
        native_root_key_id: "native-key",
        native_root_public_key: &rootkey,
        owner_binding_pin: &pin,
    };
    let before = s.provision().transport_remaining().unwrap();
    let req = request(&s);
    s.reserve_native_stage(
        req.clone(),
        "native-inspect",
        &"51".repeat(32),
        &scope,
        1400,
    )
    .unwrap();
    assert_eq!(s.provision().transport_remaining().unwrap(), before - 64);
    assert!(
        s.reserve_native_stage(req, "other-inspect", &"52".repeat(32), &scope, 1400)
            .is_err()
    );
    assert!(
        s.claim_native_stage(
            "native-attempt",
            &scope,
            &p.current(&c),
            1400,
            &mut StrictCrypto
        )
        .is_err()
    );
    s.claim_native_origin_inspection("native-attempt", &p.current(&c), 1401)
        .unwrap();
    let signed = inspect_signed(&s, &c, "native-inspect", 1402);
    s.complete_native_origin_inspection(
        "native-attempt",
        signed,
        &p.current(&c),
        1403,
        &mut StrictCrypto,
    )
    .unwrap();
    s.claim_native_stage(
        "native-attempt",
        &scope,
        &p.current(&c),
        1404,
        &mut StrictCrypto,
    )
    .unwrap();
    assert!(
        s.claim_native_stage(
            "native-attempt",
            &scope,
            &p.current(&c),
            1404,
            &mut StrictCrypto
        )
        .is_err()
    );
    for expected in 1..=8 {
        assert_eq!(
            s.reserve_native_stage_read("native-attempt", 1405).unwrap(),
            expected
        );
    }
    assert!(s.reserve_native_stage_read("native-attempt", 1405).is_err());
    let restored: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(s, restored);
}
#[test]
fn native_reservation_insufficient_budget_rolls_back_both_halves() {
    let (mut s, _) = ready();
    let pin = owner_pin();
    let scope = NativeStageScope {
        http_origin: "https://identity.test",
        native_root_key_id: "native-key",
        native_root_public_key: &pin.public_key,
        owner_binding_pin: &pin,
    };
    s.reserve_native_stage(
        request(&s),
        "first-native-inspect",
        &"61".repeat(32),
        &scope,
        1400,
    )
    .unwrap();
    let mut second = request(&s);
    second.operation_id = "second-native".into();
    s.reserve_native_stage(
        second,
        "second-native-inspect",
        &"62".repeat(32),
        &scope,
        1401,
    )
    .unwrap();
    let before = s.clone();
    let mut third = request(&s);
    third.operation_id = "third-native".into();
    assert!(
        s.reserve_native_stage(
            third,
            "third-native-inspect",
            &"63".repeat(32),
            &scope,
            1402
        )
        .is_err()
    );
    assert_eq!(s, before);
}
#[test]
fn descriptor_is_bootstrap_fixed_and_http_origin_is_distinct_from_source_audience() {
    let (s, _) = ready();
    let pin = owner_pin();
    let scope = NativeStageScope {
        http_origin: "https://identity.test",
        native_root_key_id: "native-key",
        native_root_public_key: &pin.public_key,
        owner_binding_pin: &pin,
    };
    let d = s.native_stage_descriptor(&scope).unwrap();
    assert_eq!(d.registration.display_name, "Oasis7 managed account");
    assert_eq!(d.registration.audience, scope.http_origin);
    assert_ne!(d.registration.audience, d.source_audience);
    assert_eq!(d.registration.registration_operation_id.len(), 43);
    assert_ne!(d.registration.registration_operation_id, d.login_request_id);
}
fn ready_full_budget() -> (AccountProvisionApprovalState, ApprovalAdmissionContext) {
    let (mut draft, mut input, c) = fixture();
    draft.plan.transport_limit = DecimalU64(400);
    draft.entry.plan_digest = draft.plan.digest().unwrap();
    if let CustodyIntentV1::Provision {
        plan_digest,
        transport_limit,
        ..
    } = &mut input.request.intent
    {
        *plan_digest = draft.entry.plan_digest.clone();
        *transport_limit = DecimalU64(400);
    }
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval = sign_proof(input.approval.proof);
    let mut s = AccountProvisionApprovalState::new(draft).unwrap();
    let pending = begin(&mut s, input, &c);
    let signed = verification(&pending, &c);
    let p = Policy::new(s.draft());
    s.commit(
        "operation-one",
        signed,
        &p.current(&c),
        1200,
        &mut StrictCrypto,
    )
    .unwrap();
    for (role, byte) in [("identity", "11"), ("owner", "22"), ("executor", "33")] {
        let r = role_result(&s, role, byte);
        s.record_role_key_result(r, &p.current(&c), 1300).unwrap();
    }
    (s, c)
}
fn scope(pin: &NativeBindingPinV1) -> NativeStageScope<'_> {
    NativeStageScope {
        http_origin: "https://identity.test",
        native_root_key_id: "native-key",
        native_root_public_key: &pin.public_key,
        owner_binding_pin: pin,
    }
}
fn claim(
    s: &mut AccountProvisionApprovalState,
    c: &ApprovalAdmissionContext,
    pin: &NativeBindingPinV1,
    id: &str,
    bind: &str,
    stage: NativeStageKindV1,
    now: u64,
) -> NativeStagePermitViewV1 {
    let p = Policy::new(s.draft());
    let scope = scope(pin);
    s.reserve_native_stage(
        NativeStageRequestV1 {
            api_version: 1,
            custody_account_id: s.draft().entry.custody_account_id.clone(),
            provision_id: s.draft().entry.provision_id.clone(),
            operation_id: id.into(),
            binding_operation_id: bind.into(),
            stage,
        },
        &format!("inspect-{id}"),
        &format!("{now:064x}"),
        &scope,
        now,
    )
    .unwrap();
    s.claim_native_origin_inspection(id, &p.current(c), now)
        .unwrap();
    let signed = inspect_signed(s, c, &format!("inspect-{id}"), now + 1);
    s.complete_native_origin_inspection(id, signed, &p.current(c), now + 2, &mut StrictCrypto)
        .unwrap();
    s.claim_native_stage(id, &scope, &p.current(c), now + 3, &mut StrictCrypto)
        .unwrap()
}
fn binding_ready_with_response_loss(
    lose: bool,
) -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    SignedNativeBindingReceiptV1,
) {
    let (mut s, mut c) = ready_full_budget();
    let pin = owner_pin();
    let permit = claim(
        &mut s,
        &c,
        &pin,
        "registration-attempt",
        "binding-one",
        NativeStageKindV1::Registration,
        1400,
    );
    let r = &permit.descriptor.registration;
    let challenge = NativeBindingChallengeV1 {
        api_version: 1,
        challenge_id: "binding-one".into(),
        nonce: "81".repeat(32),
        identity_namespace: r.identity_namespace.clone(),
        identity_instance_epoch: r.identity_instance_epoch,
        registration_operation_id: r.registration_operation_id.clone(),
        registration_request_digest: r.digest().unwrap(),
        registration_receipt_digest: "82".repeat(32),
        native_account_id: "managed-native".into(),
        native_credential_ref: "managed-credential".into(),
        login_public_key: r.login_public_key.clone(),
        owner_context_ref: "83".repeat(32),
        issued_at: DecimalU64(1404),
        expires_at: DecimalU64(31_404),
    };
    let signed = SignedNativeBindingChallengeV1 {
        signature: URL_SAFE_NO_PAD
            .encode(key().sign(&challenge.signing_bytes().unwrap()).to_bytes()),
        key_id: "native-key".into(),
        challenge: challenge.clone(),
    };
    let p = Policy::new(s.draft());
    s.complete_native_stage(
        "registration-attempt",
        NativeStageResultV1::RegistrationChallenge(signed),
        &scope(&pin),
        &p.current(&c),
        1405,
        None,
        &mut StrictCrypto,
    )
    .unwrap();
    let permit = claim(
        &mut s,
        &c,
        &pin,
        "binding-attempt",
        "binding-one",
        NativeStageKindV1::Binding,
        1410,
    );
    let statement = OwnerNativeBindingStatementV1 {
        challenge,
        environment: s.draft().entry.environment.clone(),
        custody_account_id: s.draft().entry.custody_account_id.clone(),
        provision_id: s.draft().entry.provision_id.clone(),
        approved_plan_digest: s.draft().entry.plan_digest.clone(),
        login_key_ref: permit.descriptor.identity_key.key_id.clone(),
        binding_operation_id: "binding-one".into(),
        permit_id: "binding-attempt".into(),
        source_revision: permit.source_revision.clone(),
        account_authority_revision: permit.authority_revision.clone(),
        service_epoch: pin.service_epoch,
        service_identity: pin.service_identity.clone(),
        signer_key_id: pin.key_id.clone(),
        authority_observed_at: DecimalU64(1411),
        expires_at: permit.expires_at,
    };
    let signed = SignedOwnerNativeBindingStatementV1 {
        signature: URL_SAFE_NO_PAD
            .encode(key().sign(&statement.signing_bytes().unwrap()).to_bytes()),
        statement: statement.clone(),
    };
    s.register_native_binding_statement(
        "binding-attempt",
        signed,
        &scope(&pin),
        &p.current(&c),
        1414,
        &mut StrictCrypto,
    )
    .unwrap();
    let receipt = NativeBindingReceiptV1 {
        api_version: 1,
        binding_operation_id: "binding-one".into(),
        statement_digest: statement.digest().unwrap(),
        environment: statement.environment.clone(),
        custody_account_id: statement.custody_account_id.clone(),
        provision_id: statement.provision_id.clone(),
        identity_namespace: statement.challenge.identity_namespace.clone(),
        identity_instance_epoch: statement.challenge.identity_instance_epoch,
        native_account_id: statement.challenge.native_account_id.clone(),
        native_credential_ref: statement.challenge.native_credential_ref.clone(),
        login_public_key: statement.challenge.login_public_key.clone(),
        login_key_ref: statement.login_key_ref.clone(),
        registration_operation_id: statement.challenge.registration_operation_id.clone(),
        registration_receipt_digest: statement.challenge.registration_receipt_digest.clone(),
        native_account_security_epoch: DecimalU64(1),
        native_credential_version: DecimalU64(1),
        committed_at: DecimalU64(1415),
    };
    let signed = SignedNativeBindingReceiptV1 {
        key_id: "native-key".into(),
        signature: URL_SAFE_NO_PAD.encode(key().sign(&receipt.signing_bytes().unwrap()).to_bytes()),
        receipt: receipt.clone(),
    };
    if lose {
        return (s, c, pin, signed);
    }
    s.complete_native_stage(
        "binding-attempt",
        NativeStageResultV1::BindingReceipt(signed.clone()),
        &scope(&pin),
        &p.current(&c),
        1415,
        None,
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision().security_epoch(), DecimalU64(1));
    assert_eq!(s.provision().stage(), ProvisionStage::NativeRegistered);
    assert!(s.check_role_key_current(&p.current(&c), 1416).is_err());
    c.authority.security_version = DecimalU64(1);
    (s, c, pin, signed)
}
fn binding_ready() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
) {
    let (s, c, p, _) = binding_ready_with_response_loss(false);
    (s, c, p)
}
#[test]
fn root_signed_binding_alone_activates_zero_to_one_and_session_status_restores() {
    let (mut s, c, pin) = binding_ready();
    let p = Policy::new(s.draft());
    let receipt = s
        .native_stages()
        .values()
        .find_map(|v| match &v.result {
            Some(NativeStageResultV1::BindingReceipt(v)) => Some(v.receipt.clone()),
            _ => None,
        })
        .unwrap();
    let permit = claim(
        &mut s,
        &c,
        &pin,
        "login-attempt",
        "binding-one",
        NativeStageKindV1::Login,
        1420,
    );
    let status = ManagedNativeSessionStatusV1 {
        api_version: 1,
        receipt,
        service_epoch: c.authority.service_epoch,
        native_account_security_epoch: DecimalU64(1),
        native_credential_version: DecimalU64(1),
        session_ref: "native-session".into(),
        nonce: permit.nonce,
        generation: DecimalU64(1),
        active: true,
        frozen: false,
        pending: false,
        observed_at: DecimalU64(1424),
        expires_at: DecimalU64(6424),
    };
    let signed = SignedManagedNativeSessionStatusV1 {
        signature: URL_SAFE_NO_PAD.encode(key().sign(&status.signing_bytes().unwrap()).to_bytes()),
        key_id: "native-key".into(),
        status: status.clone(),
    };
    let digest = status.digest().unwrap();
    s.complete_native_stage(
        "login-attempt",
        NativeStageResultV1::SessionStatus(signed),
        &scope(&pin),
        &p.current(&c),
        1425,
        Some(&PersistedNativeSessionAttestation {
            status_digest: &digest,
            session_ref: &status.session_ref,
            private_sid_generation: 1,
        }),
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision().stage(), ProvisionStage::NativeSessionReady);
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(v.clone()).unwrap(),
        s
    );
    let mut bad = v;
    bad["bootstrap_activation"]["to_epoch"] = json!("2");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
}

#[path = "managed_native_login.rs"]
mod managed_native_login;

#[path = "native_binding_recovery.rs"]
mod native_binding_recovery;

#[test]
fn reserved_native_read_credit_precedes_io_and_never_authorizes_unclaimed_original() {
    let (mut s, c) = ready();
    let p = Policy::new(s.draft());
    let pin = owner_pin();
    let scope = scope(&pin);
    s.reserve_native_stage(
        request(&s),
        "unclaimed-inspect",
        &"c1".repeat(32),
        &scope,
        1400,
    )
    .unwrap();
    for ticket in 1..=8 {
        assert_eq!(
            s.reserve_native_stage_read("native-attempt", 1401).unwrap(),
            ticket
        );
        assert!(
            s.authorize_native_stage_read(
                "native-attempt",
                ticket,
                &scope,
                &p.current(&c),
                1401,
                &mut StrictCrypto
            )
            .is_err()
        );
    }
    assert!(s.reserve_native_stage_read("native-attempt", 1401).is_err());
    assert_eq!(s.native_stages()["native-attempt"].claimed_at, None);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}
