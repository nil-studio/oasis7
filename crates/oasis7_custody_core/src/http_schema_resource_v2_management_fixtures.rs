//! Typed examples, not live management/UV/Current or activation authority.
//! The new approval examples use actual Native proof signing bytes and the
//! full-session summary contract; progress stays Pending with no roots.
use super::*;
use crate::approval_admission::{
    ApprovalAdmissionBook, ApprovalAdmissionContext, ApprovalAdmissionInput, ApprovalSourceVersions,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
fn input(intent: CustodyIntentV1, operation: &str, name: &str) -> ApprovalAdmissionInput {
    let context = super::super::super::context();
    let request = ApprovalOptionsRequestV1 {
        approval_request_id: format!("schema-{name}-request"),
        context: context.clone(),
        intent,
    };
    request.validate(1000).unwrap();
    let proof = CustodyApprovalProofV1 {
        model_budget_summary_contract: request.expected_model_budget_summary_contract().unwrap(),
        version: 1,
        key_id: "schema-native-key".into(),
        identity_namespace: "identity:test".into(),
        approval_id: format!("schema-{name}-approval"),
        approval_request_id: request.approval_request_id.clone(),
        context: context.clone(),
        intent_digest: request.intent_digest().unwrap(),
        manager_account_id: "manager-one".into(),
        manager_session_ref: "schema-manager-session".into(),
        origin_credential_id: "schema-origin".into(),
        origin_credential_version: DecimalU64(1),
        passkey_credential_id: "schema-passkey".into(),
        passkey_credential_version: DecimalU64(1),
        manager_policy_version: context.policy_version,
        manager_security_version: context.security_version,
        identity_instance_epoch: DecimalU64(3),
        nonce: format!("schema-{name}-native-uv-proof"),
        issued_at: DecimalU64(1000),
        expires_at: DecimalU64(31000),
    };
    let signed = SignedCustodyApprovalV1 {
        signature: URL_SAFE_NO_PAD.encode(key43().sign(&proof.signing_bytes().unwrap()).to_bytes()),
        proof,
    };
    let input = ApprovalAdmissionInput {
        operation_id: operation.into(),
        request,
        approval: signed,
    };
    let current = ApprovalAdmissionContext {
        identity_namespace: "identity:test".into(),
        identity_instance_epoch: DecimalU64(3),
        authority: context.clone(),
        allowed_managers: std::collections::BTreeSet::from(["manager-one".into()]),
        source: ApprovalSourceVersions {
            manager_account_id: "manager-one".into(),
            manager_session_ref: "schema-manager-session".into(),
            origin_credential_id: "schema-origin".into(),
            origin_credential_version: DecimalU64(1),
            passkey_credential_id: "schema-passkey".into(),
            passkey_credential_version: DecimalU64(1),
            manager_policy_version: context.policy_version,
            manager_security_version: context.security_version,
            session_expires_at: DecimalU64(200000),
            session_idle_expires_at: DecimalU64(200000),
        },
        native_key_pins: std::collections::BTreeMap::from([(
            "schema-native-key".into(),
            hexbytes(&key43().verifying_key().to_bytes()),
        )]),
        fence: crate::recovery::RecoveryFence::new(context.service_epoch.0, false),
        frozen: false,
        handoff_until: DecimalU64(200000),
    };
    // Exercise the actual input/summary/pin association validator. No record is
    // accepted/consumed and no real manager session/World Current is claimed.
    let book = ApprovalAdmissionBook::default();
    let checks = book
        .begin_checks(&input, &format!("schema-{name}-verify"), &current, 1000)
        .unwrap();
    assert!(!checks.checks().is_empty());
    for job in checks.checks() {
        assert!(job.public_key_hex() == hexbytes(&key43().verifying_key().to_bytes()));
        let bytes: [u8; 64] = URL_SAFE_NO_PAD
            .decode(job.signature_base64url())
            .unwrap()
            .try_into()
            .unwrap();
        key43()
            .verifying_key()
            .verify_strict(
                job.canonical_bytes(),
                &ed25519_dalek::Signature::from_bytes(&bytes),
            )
            .unwrap();
    }
    assert_eq!(book.records().count(), 0);
    input
}
pub(super) fn fixtures() -> Value {
    let owner = plan();
    owner.validate().unwrap();
    let session = session_plan();
    let scope = scope();
    session.validate_scope(&scope, 1000).unwrap();
    let owner_prepare = OwnerSetupPrepareRequestV2 {
        api_version: 2,
        approval_request_id: "schema-owner-request".into(),
        plan: owner.clone(),
    };
    owner_prepare.validate().unwrap();
    let owner_approve = OwnerSetupApproveRequestV2 {
        api_version: 2,
        input: input(owner.intent().unwrap(), &owner.setup_id, "owner"),
        plan: owner.clone(),
    };
    owner_approve.validate().unwrap();
    let basis_request = TestSessionBasisRequestV2 {
        api_version: 2,
        request_id: session.request_id.clone(),
        enrollment_id: session.enrollment_id.clone(),
        setup_id: owner.setup_id.clone(),
        cleanup_until: crate::time_units::UnixMillis::new(session.cleanup_until.0),
    };
    basis_request.validate().unwrap();
    // Schema metadata only, sharing the already validated example plan's exact
    // Term/scope/digests. This is not the actual Account basis producer.
    let basis = TestSessionBasisV2 {
        api_version: 2,
        request_id: basis_request.request_id.clone(),
        custody_account_id: scope.client_pairing.target.custody_account_id.clone(),
        provision_id: owner.provision_id.clone(),
        scope: scope.clone(),
        owner_setup_plan_digest: owner.digest().unwrap(),
        termination: session.termination.as_ref().clone(),
        basis_digest: session.basis_digest.clone(),
    };
    assert_eq!(
        basis.termination.digest().unwrap(),
        basis.scope.termination_plan_digest
    );
    let prepare = TestSessionPrepareRequestV2 {
        api_version: 2,
        approval_request_id: "schema-session-request".into(),
        plan: session.clone(),
        scope: scope.clone(),
    };
    prepare.validate().unwrap();
    let approve = TestSessionApproveRequestV2 {
        api_version: 2,
        input: input(
            CustodyIntentV1::TestSessionV2 {
                plan: Box::new(session),
                scope: Box::new(scope.clone()),
            },
            &scope.session_id,
            "session",
        ),
    };
    approve.validate().unwrap();
    assert!(matches!(
        approve.input.approval.proof.model_budget_summary_contract,
        Some(NativeCustodySummaryContractV1::FullTestSessionV2(_))
    ));
    let continue_request = TestSessionContinueRequestV2 {
        api_version: 2,
        session_id: scope.session_id.clone(),
    };
    continue_request.validate().unwrap();
    let progress = TestSessionApprovalProgressV2 {
        api_version: 2,
        custody_account_id: basis.custody_account_id.clone(),
        provision_id: basis.provision_id.clone(),
        session_id: scope.session_id.clone(),
        phase: ApprovalPhaseV1::Pending,
    };
    let activation = TestSessionActivationProgressV2 {
        api_version: 2,
        custody_account_id: basis.custody_account_id.clone(),
        provision_id: basis.provision_id.clone(),
        session_id: scope.session_id,
        activated_at: None,
        preflight_pending: false,
        closing: false,
    };
    json!({"OwnerSetupPrepareRequestV2":[owner_prepare],"OwnerSetupApproveRequestV2":[owner_approve],
        "TestSessionBasisRequestV2":[basis_request],"TestSessionBasisV2":[basis],"TestSessionPrepareRequestV2":[prepare],
        "TestSessionApproveRequestV2":[approve],"TestSessionContinueRequestV2":[continue_request],
        "TestSessionApprovalProgressV2":[progress],"TestSessionActivationProgressV2":[activation]})
}
#[test]
fn v2_management_schema_examples_have_nine_closed_roots_and_explicit_null_activation() {
    let examples = fixtures();
    assert_eq!(examples.as_object().unwrap().len(), 9);
    assert!(
        examples["TestSessionActivationProgressV2"][0]
            .get("activated_at")
            .unwrap()
            .is_null()
    );
    let mut unknown = examples["TestSessionApproveRequestV2"][0].clone();
    unknown["verified"] = json!(true);
    assert!(serde_json::from_value::<TestSessionApproveRequestV2>(unknown).is_err());
}
