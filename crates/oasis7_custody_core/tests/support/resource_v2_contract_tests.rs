//! Real approval/verification signatures through the shared existing Book.
//! No claim of V2 Account state, payment, Native dispatch or fresh World admission.
use super::*;
use oasis7_client_api::world_service::resource_basis as wire;
use oasis7_custody_core::provisioning::*;
#[path = "resource_v2_paid_proof_tests.rs"]
mod paid;
#[path = "resource_v2_session_contract_tests.rs"]
mod session;
fn selection() -> wire::ResourceBasisSelectionV1 {
    wire::ResourceBasisSelectionV1::Transfer {
        from: wire::ResourceLedgerV1::Agent {
            agent_id: "agent".into(),
        },
        to: wire::ResourceLedgerV1::Site {
            site_id: "site".into(),
        },
        material: "iron_ore".into(),
        amount: wire::ResourceDecimalI64V1(5),
        distance_km: wire::ResourceDecimalI64V1(0),
        priority: oasis7_agent_api::world_action::MaterialTransitPriority::Standard,
        routing: wire::ResourceRoutingV1::Direct,
    }
}
fn plan() -> OwnerSetupPlanV2 {
    OwnerSetupPlanV2 {
        model_budget: None,
        api_version: 2,
        setup_id: "setup".into(),
        provision_id: "provision".into(),
        participant_receipt_digest: "ab".repeat(32),
        world_identity: ApprovedWorldV1 {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            service_public_key: "cd".repeat(32),
        },
        owner_key_ref: "owner".into(),
        executor_key_ref: "executor".into(),
        bootstrap_config_version: DecimalU64(1),
        goal_content: oasis7_agent_api::game::GoalContentV1 {
            goal_text: "private resource goal".into(),
            soft_constraints: vec![],
        },
        create_operation_id: "create".into(),
        prepare_operation_id: "prepare".into(),
        prepare_nonce: "ef".repeat(32),
        publish_operation_id: "publish".into(),
        delegation_operation_id: "delegation".into(),
        bind_operation_id: "bind".into(),
        executor_id: "executor".into(),
        executor_public_key: "cd".repeat(32),
        delegation_generation: DecimalU64(1),
        delegation_nonce: DecimalU64(1),
        selection: selection().into(),
        deadline: DecimalU64(100_000),
        request_limit: DecimalU64(6),
        transport_limit: DecimalU64(584),
        write_limit: DecimalU64(4),
    }
}
fn v2_input() -> (ApprovalAdmissionInput, ApprovalAdmissionContext) {
    let (mut input, current) = fixture();
    input.operation_id = plan().setup_id;
    input.request.intent = plan().intent().unwrap();
    let mut proof = input.approval.proof;
    proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval = sign_proof(proof);
    (input, current)
}
#[test]
fn resource_v2_shared_book_real_signatures_consume_once_and_preserve_historical_time() {
    let (input, current) = v2_input();
    let mut book = ApprovalAdmissionBook::default();
    StrictCrypto
        .verify(
            &resource_v2_owner_setup_checks(
                &book,
                &input,
                &plan(),
                "exact-v2-verification-nonce",
                &current,
                1000,
            )
            .unwrap(),
        )
        .unwrap();
    let record = book
        .begin(
            input.clone(),
            "exact-v2-verification-nonce".into(),
            &current,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
    let v = verification(&record, &current);
    let consumed = book
        .commit(
            &input.operation_id,
            v.clone(),
            &current,
            1100,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(consumed.stage(), ApprovalAdmissionStage::Consumed);
    assert_eq!(consumed.consumed_at(), Some(1100));
    let before = serde_json::to_vec(&book).unwrap();
    let historical = book
        .commit(&input.operation_id, v, &current, 900_000, &mut StrictCrypto)
        .unwrap();
    assert_eq!(historical.consumed_at(), Some(1100));
    assert_eq!(serde_json::to_vec(&book).unwrap(), before);
    // Callback replay does not issue a paid proof or authorize a second HTTP.
    let mut changed = input.clone();
    if let CustodyIntentV1::OwnerSetupV2 { plan } = &mut changed.request.intent {
        if let ApprovedActionSelectionV2::Resource(wire::ResourceBasisSelectionV1::Transfer {
            amount,
            ..
        }) = &mut plan.selection
        {
            amount.0 += 1;
        }
    }
    assert!(
        book.begin(
            changed,
            "exact-v2-verification-nonce".into(),
            &current,
            1100,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(serde_json::to_vec(&book).unwrap(), before);
}
#[test]
fn resource_v2_proof_cannot_be_repurposed_from_v1_or_resigned_into_other_scope() {
    let (legacy, current) = fixture();
    let mut wrong = legacy.clone();
    wrong.request.intent = plan().intent().unwrap();
    let mut book = ApprovalAdmissionBook::default();
    let before = serde_json::to_vec(&book).unwrap();
    assert!(
        book.begin(wrong, "nonce".into(), &current, 1000, &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&book).unwrap(), before);
    let (mut input, _) = v2_input();
    input.approval.signature = URL_SAFE_NO_PAD.encode(
        SigningKey::from_bytes(&[42; 32])
            .sign(&input.approval.proof.signing_bytes().unwrap())
            .to_bytes(),
    );
    assert!(
        book.begin(input, "nonce".into(), &current, 1000, &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&book).unwrap(), before);
    let (mut input, _) = v2_input();
    input.request.context.custody_account_id = "another-account".into();
    assert!(
        book.begin(input, "nonce".into(), &current, 1000, &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&book).unwrap(), before);
    assert!(
        serde_json::from_value::<SignedCustodyApprovalV1>(
            json!({"id":"ordinary-ceremony","response":{}})
        )
        .is_err()
    );
}
#[test]
fn resource_v2_closed_plan_codec_and_legacy_intent_bytes() {
    let p = plan();
    p.validate().unwrap();
    let original = serde_json::to_string(&p).unwrap();
    let decoded: OwnerSetupPlanV2 = serde_json::from_str(&original).unwrap();
    assert_eq!(p, decoded);
    assert_eq!(p.digest().unwrap(), decoded.digest().unwrap());
    for bad in [
        original.replacen("\"api_version\":2", "\"api_version\":1", 1),
        original.replacen("\"amount\":\"5\"", "\"amount\":5", 1),
        original.replacen("\"5\"", "\"05\"", 1),
    ] {
        assert!(
            serde_json::from_str::<OwnerSetupPlanV2>(&bad)
                .and_then(|v| v.validate().map_err(serde::de::Error::custom))
                .is_err()
        );
    }
    let with_unknown = original.replacen('{', "{\"verified\":true,", 1);
    assert!(serde_json::from_str::<OwnerSetupPlanV2>(&with_unknown).is_err());
    let (legacy, _) = fixture();
    assert_eq!(
        serde_json::to_string(&legacy.request.intent).unwrap(),
        r#"{"kind":"managed_login","native_account_id":"native-one","login_key_ref":"login-one","native_identity_namespace":"identity:test","blocked_operation_ref":"blocked-one","login_request_id":"managed-login-one","transport_limit":"64","deadline":"100000"}"#
    );
    let mut invalid = plan();
    if let ApprovedActionSelectionV2::Resource(wire::ResourceBasisSelectionV1::Transfer {
        from,
        ..
    }) = &mut invalid.selection
    {
        *from = wire::ResourceLedgerV1::World;
    }

    assert!(invalid.validate().is_err());
}

#[test]
fn model_budget_is_explicit_null_and_missing_is_not_a_legacy_upgrade() {
    let original = plan();
    original.validate().unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    assert!(value.as_object().unwrap().contains_key("model_budget"));
    assert!(value["model_budget"].is_null());
    assert_eq!(
        serde_json::from_value::<OwnerSetupPlanV2>(value.clone()).unwrap(),
        original
    );
    value.as_object_mut().unwrap().remove("model_budget");
    assert!(serde_json::from_value::<OwnerSetupPlanV2>(value).is_err());
}

fn model_budget() -> ModelInferenceBudgetV1 {
    ModelInferenceBudgetV1 {
        api_version: 1,
        approval_contract: ModelBudgetApprovalContractV1::NativeCustodyModelBudgetV1,
        native_account_id: "native".into(),
        native_public_key: "11".repeat(32),
        native_instance_public_key: "22".repeat(32),
        ledger_id: "ledger".into(),
        preset_id: "fixed-preset".into(),
        preset_revision: DecimalU64(1),
        model: "fixed-model".into(),
        runtime_digest: "33".repeat(32),
        skill_digest: "44".repeat(32),
        tools_digest: "55".repeat(32),
        context_policy_digest: "66".repeat(32),
        physical_graph_digest: "77".repeat(32),
        probe_physical_http: [DecimalU64(10), DecimalU64(10)],
        inference_physical_http: DecimalU64(10),
        max_calls: DecimalU64(3),
        max_input_tokens: DecimalU64(100),
        max_output_tokens: DecimalU64(50),
        max_total_tokens: DecimalU64(450),
        max_concurrent: DecimalU64(1),
        deadline: DecimalU64(90_000),
        price_policy: ModelPricePolicyV1 {
            currency: "USD".into(),
            input_micro_units_per_million: DecimalU64(1_000_000),
            output_micro_units_per_million: DecimalU64(2_000_000),
            max_total_micro_units: DecimalU64(600),
            quote_digest: "88".repeat(32),
            valid_until: DecimalU64(100_000),
        },
    }
}
#[test]
fn approved_v2_budget_is_inside_actual_native_signature_not_a_caller_grant() {
    let (mut input, current) = v2_input();
    let mut approved = plan();
    approved.model_budget = Some(model_budget());
    approved.validate().unwrap();
    assert_eq!(
        approved
            .model_budget
            .as_ref()
            .unwrap()
            .maximum_call_price()
            .unwrap(),
        200
    );
    input.request.intent = approved.intent().unwrap();
    let mut proof = input.approval.proof;
    proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval = sign_proof(proof);
    let mut book = ApprovalAdmissionBook::default();
    let record = book
        .begin(
            input.clone(),
            "budget-verify".into(),
            &current,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
    let signed = verification(&record, &current);
    book.commit(
        &input.operation_id,
        signed,
        &current,
        1100,
        &mut StrictCrypto,
    )
    .unwrap();
    let before = serde_json::to_vec(&book).unwrap();
    for case in 0..4 {
        let mut altered = input.clone();
        let CustodyIntentV1::OwnerSetupV2 { plan } = &mut altered.request.intent else {
            panic!()
        };
        let b = plan.model_budget.as_mut().unwrap();
        match case {
            0 => b.max_calls.0 += 1,
            1 => b.native_public_key = "99".repeat(32),
            2 => b.physical_graph_digest = "aa".repeat(32),
            _ => b.price_policy.output_micro_units_per_million.0 += 1,
        }
        assert!(
            book.begin(
                altered,
                "budget-verify".into(),
                &current,
                1100,
                &mut StrictCrypto
            )
            .is_err()
        );
        assert_eq!(serde_json::to_vec(&book).unwrap(), before);
    }
    // No capability/ActiveAttempt is produced by consuming this UV proof.
}
#[test]
fn inference_budget_rejects_unsupported_concurrency_expiry_and_overflow() {
    let valid = model_budget();
    valid.validate(100_000).unwrap();
    for case in 0..5 {
        let mut b = valid.clone();
        match case {
            0 => b.max_concurrent.0 = 2,
            1 => b.deadline.0 = 100_001,
            2 => b.max_input_tokens.0 = u64::MAX,
            3 => b.price_policy.valid_until.0 = 89_999,
            _ => b.max_calls.0 = 2,
        }
        assert!(b.validate(100_000).is_err());
    }
}

#[test]
fn opaque_setup_approval_is_not_a_native_model_summary_attestation() {
    let (mut input, current) = v2_input();
    let mut approved = plan();
    approved.model_budget = Some(model_budget());
    input.request.intent = approved.intent().unwrap();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval = sign_proof(input.approval.proof);
    let expected = input
        .request
        .expected_model_budget_summary_contract()
        .unwrap()
        .unwrap();
    assert_eq!(expected.api_version, 1);
    assert!(input.approval.proof.model_budget_summary_contract.is_none());
    assert!(
        !serde_json::to_value(&input.approval.proof)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("model_budget_summary_contract")
    );
    // A trusted Native signer can bind this exact expected summary. The test
    // signature proves protocol binding only, never actual UI rendering.
    input.approval.proof.model_budget_summary_contract = Some(expected);
    input.approval = sign_proof(input.approval.proof);
    let mut altered = input.clone();
    altered
        .approval
        .proof
        .model_budget_summary_contract
        .as_mut()
        .unwrap()
        .summary_digest = "ff".repeat(32);
    let mut book = ApprovalAdmissionBook::default();
    assert!(
        book.begin(
            altered,
            "summary-verify".into(),
            &current,
            1000,
            &mut StrictCrypto
        )
        .is_err()
    );
    book.begin(
        input,
        "summary-verify".into(),
        &current,
        1000,
        &mut StrictCrypto,
    )
    .unwrap();
}

#[test]
fn explicit_move0_selection_is_distinct_from_a_resource_basis() {
    let original = plan();
    original.validate().unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    value["selection"] = serde_json::json!({"kind":"move","position_cm":["1","-2","3"]});
    let accepted: OwnerSetupPlanV2 =
        serde_json::from_value(value.clone()).expect("explicit V2 Move0 selection");
    accepted.validate().unwrap();
    assert_eq!(
        serde_json::to_value(&accepted).unwrap()["selection"],
        value["selection"]
    );
    let mut unknown = value.clone();
    unknown["selection"]["resource_refs"] = serde_json::json!([]);
    assert!(serde_json::from_value::<OwnerSetupPlanV2>(unknown).is_err());
}

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
