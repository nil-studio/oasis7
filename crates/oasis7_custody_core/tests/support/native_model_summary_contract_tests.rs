use super::*;
fn native_resigned_summary_case(missing: bool) {
    let (mut input, current) = v2_input();
    let mut approved = plan();
    approved.model_budget = Some(model_budget());
    input.request.intent = approved.intent().unwrap();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.model_budget_summary_contract = input
        .request
        .expected_model_budget_summary_contract()
        .unwrap();
    input.approval = sign_proof(input.approval.proof);
    let mut positive = ApprovalAdmissionBook::default();
    positive
        .begin(
            input.clone(),
            "summary-positive".into(),
            &current,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
    if missing {
        input.approval.proof.model_budget_summary_contract = None;
    } else {
        let mut contract = serde_json::to_value(
            input
                .approval
                .proof
                .model_budget_summary_contract
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        contract["summary_digest"] = json!("ff".repeat(32));
        input.approval.proof.model_budget_summary_contract =
            Some(serde_json::from_value(contract).unwrap());
    }
    input.approval = sign_proof(input.approval.proof);
    // The Native signature is genuinely valid even on the wrong semantic contract.
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
    let mut book = ApprovalAdmissionBook::default();
    let before = serde_json::to_vec(&book).unwrap();
    assert!(
        book.begin(
            input,
            "summary-negative".into(),
            &current,
            1000,
            &mut StrictCrypto
        )
        .is_err(),
        "valid Native signature must not substitute for the exact summary contract"
    );
    assert_eq!(serde_json::to_vec(&book).unwrap(), before);
}
#[test]
fn native_resigned_missing_summary_contract_is_not_admitted() {
    native_resigned_summary_case(true)
}
#[test]
fn native_resigned_wrong_summary_contract_is_not_admitted() {
    native_resigned_summary_case(false)
}
