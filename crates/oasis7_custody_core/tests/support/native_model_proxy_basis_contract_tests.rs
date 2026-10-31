//! Real Native signatures validate server-basis semantics, not provider runtime qualification.
use super::*;
fn proxy_input() -> (ApprovalAdmissionInput, ApprovalAdmissionContext) {
    let (mut input, current) = input_with_summary(true);
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &mut input.request.intent else {
        panic!("fixture");
    };
    let budget = plan.model_budget.as_mut().unwrap();
    budget.approval_contract =
        oasis7_custody_core::provisioning::ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2;
    budget.native_account_id = current.source.manager_account_id.clone();
    budget.native_public_key = current.native_key_pins[&input.approval.proof.key_id].clone();
    budget.native_instance_public_key = key_from_seed(91);
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.model_budget_summary_contract = input
        .request
        .expected_model_budget_summary_contract()
        .unwrap();
    input.approval = sign_proof(input.approval.proof);
    (input, current)
}
fn key_from_seed(seed: u8) -> String {
    ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn with_basis(
    mut input: ApprovalAdmissionInput,
    current: &ApprovalAdmissionContext,
) -> ApprovalAdmissionInput {
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &input.request.intent else {
        panic!("fixture");
    };
    let budget = plan.model_budget.as_ref().unwrap();
    let basis = NativeModelProxyBasisV2::for_request(
        &input.request,
        &NativeModelProxyConnectionBindingV2 {
            native_account_id: current.source.manager_account_id.clone(),
            native_instance_public_key: budget.native_instance_public_key.clone(),
            connection_ref_digest: "61".repeat(32),
            connection_generation: DecimalU64(1),
            endpoint_policy_digest: "62".repeat(32),
        },
        1000,
        2000,
    )
    .unwrap();
    let NativeCustodySummaryContractV1::FullTestSessionV2(base) = input
        .request
        .expected_model_budget_summary_contract()
        .unwrap()
        .unwrap()
    else {
        panic!("fixture");
    };
    input.approval.proof.model_budget_summary_contract =
        Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(
            Box::new(NativeFullSessionProxySummaryContractV2 {
                api_version: 2,
                contract: NativeFullSessionProxyProfileV2::FullTestSessionProxyV2,
                full_session: Box::new(base),
                basis_digest: basis.digest().unwrap(),
                basis: Box::new(basis),
            }),
        ));
    input.approval = sign_proof(input.approval.proof);
    input
}
#[test]
fn native_model_proxy_requires_actual_basis_even_with_valid_legacy_full_summary_signature() {
    let (consent, current) = input_with_summary(true);
    native_signature_is_valid(&consent);
    ApprovalAdmissionBook::default()
        .begin(
            consent,
            "old-consent-positive".into(),
            &current,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
    let (input, current) = proxy_input();
    native_signature_is_valid(&input);
    rejected(input, &current);
}
#[test]
fn native_model_proxy_actual_full_basis_contract_is_accepted_as_consent_evidence() {
    let (input, current) = proxy_input();
    let valid = with_basis(input, &current);
    native_signature_is_valid(&valid);
    ApprovalAdmissionBook::default()
        .begin(
            valid,
            "actual-proxy-basis-positive".into(),
            &current,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
}

#[test]
fn native_model_proxy_native_resigned_wrong_basis_rejects_without_mutation() {
    let (input, current) = proxy_input();
    let valid = with_basis(input, &current);
    for which in 0..4 {
        let mut changed = valid.clone();
        let Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(contract)) = changed
            .approval
            .proof
            .model_budget_summary_contract
            .as_mut()
        else {
            panic!("fixture");
        };
        match which {
            0 => contract.basis.native_account_id = "other-native-account".into(),
            1 => contract.basis.native_instance_public_key = key_from_seed(92),
            2 => contract.basis.budget_digest = "63".repeat(32),
            _ => contract.basis.physical_graph_digest = "64".repeat(32),
        }
        contract.basis_digest = contract.basis.digest().unwrap();
        rejected(changed, &current);
    }
}
#[test]
fn native_model_proxy_old_budget_version_is_consent_only_even_with_new_basis_wrapper() {
    let (input, current) = proxy_input();
    let mut changed = with_basis(input, &current);
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &mut changed.request.intent else {
        panic!("fixture");
    };
    plan.model_budget.as_mut().unwrap().approval_contract=
        oasis7_custody_core::provisioning::ModelBudgetApprovalContractV1::NativeCustodyModelBudgetV1;
    changed.approval.proof.intent_digest = changed.request.intent_digest().unwrap();
    let expected = changed
        .request
        .expected_model_budget_summary_contract()
        .unwrap()
        .unwrap();
    let NativeCustodySummaryContractV1::FullTestSessionV2(base) = expected.clone() else {
        panic!("fixture");
    };
    let mut consent_only = changed.clone();
    consent_only.approval.proof.model_budget_summary_contract = Some(expected);
    consent_only.approval = sign_proof(consent_only.approval.proof);
    native_signature_is_valid(&consent_only);
    ApprovalAdmissionBook::default()
        .begin(
            consent_only,
            "old-some-consent-positive".into(),
            &current,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &changed.request.intent else {
        panic!("fixture");
    };
    let Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(contract)) = changed
        .approval
        .proof
        .model_budget_summary_contract
        .as_mut()
    else {
        panic!("fixture");
    };
    *contract.full_session = base;
    contract.basis.intent_digest = changed.request.intent_digest().unwrap();
    contract.basis.budget_digest = plan
        .model_budget
        .as_ref()
        .unwrap()
        .digest(plan.write_deadline.0)
        .unwrap();
    contract.basis_digest = contract.basis.digest().unwrap();
    rejected(changed, &current);
}
