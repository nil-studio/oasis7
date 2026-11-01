//! The new full-session contract and its nested budget contract are independent obligations.
use super::*;
fn input_with_summary(model: bool) -> (ApprovalAdmissionInput, ApprovalAdmissionContext) {
    let (mut input, current) = fixture();
    let mut plan = session_plan();
    let scope = scope();
    if model {
        let mut b = model_budget();
        b.native_account_id = scope.native_account_id.clone();
        plan.model_budget = Some(b);
    }
    input.operation_id = scope.session_id.clone();
    input.request.intent = plan.intent(scope, 1000).unwrap();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.model_budget_summary_contract = input
        .request
        .expected_model_budget_summary_contract()
        .unwrap();
    input.approval = sign_proof(input.approval.proof);
    (input, current)
}
fn native_signature_is_valid(input: &ApprovalAdmissionInput) {
    let signature = ed25519_dalek::Signature::from_slice(
        &URL_SAFE_NO_PAD.decode(&input.approval.signature).unwrap(),
    )
    .unwrap();
    ed25519_dalek::Verifier::verify(
        &key().verifying_key(),
        &input.approval.proof.signing_bytes().unwrap(),
        &signature,
    )
    .unwrap();
}
fn rejected(mut input: ApprovalAdmissionInput, current: &ApprovalAdmissionContext) {
    input.approval = sign_proof(input.approval.proof);
    native_signature_is_valid(&input);
    let mut book = ApprovalAdmissionBook::default();
    let before = serde_json::to_vec(&book).unwrap();
    assert!(
        book.begin(
            input,
            "full-summary-negative".into(),
            current,
            1000,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert!(
        serde_json::to_vec(&book).unwrap() == before,
        "semantic rejection must not mutate the Book"
    );
}
fn case(which: u8) {
    let (mut input, current) = input_with_summary(true);
    let mut positive = ApprovalAdmissionBook::default();
    positive
        .begin(
            input.clone(),
            "full-summary-positive".into(),
            &current,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
    let NativeCustodySummaryContractV1::FullTestSessionV2(full) = input
        .approval
        .proof
        .model_budget_summary_contract
        .as_mut()
        .unwrap()
    else {
        panic!("not full V2")
    };
    match which {
        0 => full.model_budget_contract = None,
        1 => full.model_budget_contract.as_mut().unwrap().summary_digest = "ef".repeat(32),
        2 => {
            let budget = full.model_budget_contract.clone().unwrap();
            input.approval.proof.model_budget_summary_contract =
                Some(NativeCustodySummaryContractV1::ModelBudgetV1(budget));
        }
        3 => full.summary_digest = "ef".repeat(32),
        _ => unreachable!(),
    }
    rejected(input, &current);
}
#[test]
fn full_correct_but_resigned_budget_missing_rejects() {
    case(0)
}
#[test]
fn full_correct_but_resigned_budget_wrong_rejects() {
    case(1)
}
#[test]
fn budget_correct_but_resigned_full_missing_rejects() {
    case(2)
}
#[test]
fn budget_correct_but_resigned_full_wrong_rejects() {
    case(3)
}
#[test]
fn budget_null_still_requires_full_session_summary_and_explicit_nested_null() {
    let (mut input, current) = input_with_summary(false);
    let contract = input
        .approval
        .proof
        .model_budget_summary_contract
        .clone()
        .unwrap();
    let NativeCustodySummaryContractV1::FullTestSessionV2(full) = &contract else {
        panic!("not full V2")
    };
    assert!(full.model_budget_contract.is_none());
    let mut book = ApprovalAdmissionBook::default();
    book.begin(
        input.clone(),
        "full-null-positive".into(),
        &current,
        1000,
        &mut StrictCrypto,
    )
    .unwrap();
    let mut json = serde_json::to_value(&contract).unwrap();
    assert!(
        json.as_object_mut()
            .unwrap()
            .remove("model_budget_contract")
            .is_some()
    );
    assert!(serde_json::from_value::<NativeCustodySummaryContractV1>(json).is_err());
    input.approval.proof.model_budget_summary_contract = None;
    rejected(input, &current);
}
#[test]
fn legacy_owner_budget_contract_json_and_cbor_wire_are_unchanged() {
    let legacy = NativeModelBudgetSummaryContractV1 {
        api_version: 1,
        summary_digest: "ab".repeat(32),
    };
    let typed = NativeCustodySummaryContractV1::ModelBudgetV1(legacy.clone());
    assert_eq!(
        serde_json::to_vec(&legacy).unwrap(),
        serde_json::to_vec(&typed).unwrap()
    );
    assert_eq!(
        serde_cbor::to_vec(&legacy).unwrap(),
        serde_cbor::to_vec(&typed).unwrap()
    );
}

#[test]
fn termination_metadata_matches_the_exact_approved_parent_scope() {
    let scope = scope();
    let term = termination_fixture(&scope);
    term.validate_scope(&scope).unwrap();
    for account in [true, false] {
        let mut changed = term.clone();
        if account {
            changed.custody_account_id = "other-account".into();
        } else {
            changed.provision_id = "other-provision".into();
        }
        let mut scoped = scope.clone();
        scoped.termination_plan_digest = changed.digest().unwrap();
        changed.validate().unwrap();
        assert!(changed.validate_scope(&scoped).is_err());
    }
}

#[test]
fn emit_full_session_ui_contract_fixture() {
    let mut cases = Vec::new();
    let (proxy, proxy_current) = proxy_basis_tests::proxy_input();
    let proxy = proxy_basis_tests::with_basis(proxy, &proxy_current);
    for (input, current) in [
        input_with_summary(false),
        input_with_summary(true),
        (proxy, proxy_current),
    ] {
        native_signature_is_valid(&input);
        ApprovalAdmissionBook::default()
            .begin(
                input.clone(),
                "full-ui-contract".into(),
                &current,
                1000,
                &mut StrictCrypto,
            )
            .unwrap();
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &input.request.intent else {
            panic!("expected the actual full-session request")
        };
        plan.validate_scope(scope, 1000).unwrap();
        let original = TestSessionPrepareRequestV2 {
            api_version: 2,
            approval_request_id: input.request.approval_request_id.clone(),
            plan: plan.as_ref().clone(),
            scope: scope.as_ref().clone(),
        };
        original.validate().unwrap();
        cases.push(json!({
            "original": original,
            "prepared": input.request,
            "contract": input.approval.proof.model_budget_summary_contract.as_ref().unwrap(),
            "summary": input.request.native_session_summary_v2().unwrap(),
            "budget": plan.model_budget,
            "input": input,
        }));
    }
    let document = json!({
        "api_version": 1, "fixture_only": true, "now_ms": 1000, "cases": cases,
    });
    // The vectors validate Native signatures and the pending Book only. They
    // do not consume MFA, issue roots, render a browser or attest actual UI.
    if let Some(path) = std::env::var_os("OASIS7_SESSION_V2_UI_CONTRACT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
    }
}

#[path = "native_model_proxy_basis_contract_tests.rs"]
mod proxy_basis_tests;
