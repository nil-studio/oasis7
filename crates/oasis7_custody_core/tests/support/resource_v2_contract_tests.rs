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
        selection: selection(),
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
        if let wire::ResourceBasisSelectionV1::Transfer { amount, .. } = &mut plan.selection {
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
    if let wire::ResourceBasisSelectionV1::Transfer { from, .. } = &mut invalid.selection {
        *from = wire::ResourceLedgerV1::World;
    }

    assert!(invalid.validate().is_err());
}
