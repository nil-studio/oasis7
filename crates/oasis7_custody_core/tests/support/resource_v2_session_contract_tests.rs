//! Scope fixtures validate a signed contract only, not actual enrollment/World.
use super::*;
use oasis7_custody_core::client_enrollment::{ClientPairingBindingV1, PairingInviteTargetRefV1};
fn scope() -> ResourceTestSessionScopeV2 {
    let task=serde_json::from_value(json!({"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","agent_id":"agent","owner_binding":"owner","owner_generation":"1","owner_public_key":"bb".repeat(32),"task_id":"task","task_revision":"1","task_status":"active","latest_goal_revision":"0","active_goal_revision":"0","active_goal_reference":null,"executor_epoch":"1","executor_binding":{"executor_id":"executor","executor_public_key":"cc".repeat(32),"delegation_generation":"1","executor_epoch":"1"}})).unwrap();
    ResourceTestSessionScopeV2 {
        session_id: "session".into(),
        native_account_id: "native-one".into(),
        world_identity: plan().world_identity,
        task_fence: task,
        executor_key_ref: "executor".into(),
        client_binding: ApprovedClientBindingV1::NativeBrokerCustodyV1 {
            client_key: "dd".repeat(32),
        },
        client_pairing: ClientPairingBindingV1 {
            api_version: 1,
            target: PairingInviteTargetRefV1 {
                custody_account_id: "custody-one".into(),
                provision_id: "provision".into(),
                invite_id: "invite".into(),
                invite_digest: "aa".repeat(32),
            },
            enrollment_id: "enrollment".into(),
            enrollment_digest: "ab".repeat(32),
            host_instance_id: "host".into(),
            descriptor_digest: "ac".repeat(32),
            client_key_id: "client-key".into(),
            client_public_key: "dd".repeat(32),
            connector_public_key: None,
        },
        runtime_ref: "codex".into(),
        skill_ref: "skill".into(),
        source_ref: "source".into(),
        termination_plan_digest: "ee".repeat(32),
    }
}
fn session_plan() -> TestSessionPlanV2 {
    TestSessionPlanV2 {
        api_version: 2,
        request_id: "session-request".into(),
        enrollment_id: "enrollment".into(),
        basis_digest: "aa".repeat(32),
        owner_setup_plan_digest: plan().digest().unwrap(),
        prepared_resource_basis_digest: "ab".repeat(32),
        selection: selection(),
        actions: vec![ApprovedActionV1::Transfer {
            from: oasis7_agent_api::world_action::MaterialLedgerId::agent("agent"),
            to: oasis7_agent_api::world_action::MaterialLedgerId::site("site"),
            material: "iron_ore".into(),
            amount: DecimalI64(5),
            distance_km: DecimalI64(0),
            priority: oasis7_agent_api::world_action::MaterialTransitPriority::Standard,
            routing: ApprovedRoutingV1::Direct,
        }],
        allowed_resource_refs: vec!["agent:agent".into(), "site:site".into(), "iron_ore".into()],
        write_deadline: DecimalU64(100_000),
        read_until: DecimalU64(200_000),
        cleanup_until: DecimalU64(200_000),
        request_limit: DecimalU64(100),
        transport_limit: DecimalU64(400),
        business_limit: DecimalU64(100),
        read_budget: PurposeBudgetV1 {
            requests: DecimalU64(100),
            transport: DecimalU64(300),
            business: DecimalU64(0),
        },
        cleanup_budget: PurposeBudgetV1 {
            requests: DecimalU64(2),
            transport: DecimalU64(24),
            business: DecimalU64(0),
        },
        control_budget: ControlBudgetV1 {
            requests: DecimalU64(80),
            transport_per_hop: DecimalU64(240),
            identity_transport: DecimalU64(160),
            transport_hops: 1,
        },
    }
}
#[test]
fn resource_v2_test_session_exact_scope_uses_shared_book_and_actual_signatures() {
    let plan = session_plan();
    let scope = scope();
    plan.validate_scope(&scope, 1000).unwrap();
    let (mut input, current) = fixture();
    input.operation_id = scope.session_id.clone();
    input.request.intent = plan.intent(scope.clone(), 1000).unwrap();
    let mut proof = input.approval.proof;
    proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval = sign_proof(proof);
    let mut book = ApprovalAdmissionBook::default();
    StrictCrypto
        .verify(
            &resource_v2_test_session_checks(
                &book,
                &input,
                &plan,
                &scope,
                "session-nonce",
                &current,
                1000,
            )
            .unwrap(),
        )
        .unwrap();
    let record = book
        .begin(
            input.clone(),
            "session-nonce".into(),
            &current,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
    let v = verification(&record, &current);
    book.commit(&input.operation_id, v, &current, 1100, &mut StrictCrypto)
        .unwrap();
    let before = serde_json::to_vec(&book).unwrap();
    let mut changed = plan.clone();
    changed.prepared_resource_basis_digest = "ff".repeat(32);
    assert!(
        resource_v2_test_session_checks(
            &book,
            &input,
            &changed,
            &scope,
            "session-nonce",
            &current,
            1100
        )
        .is_err()
    );
    assert_eq!(serde_json::to_vec(&book).unwrap(), before);
    let mut changed = scope;
    changed.client_pairing.client_public_key = "aa".repeat(32);
    assert!(plan.validate_scope(&changed, 1000).is_err());
    let mut changed = plan.clone();
    changed.actions.push(ApprovedActionV1::Move {
        position_cm: [DecimalI64(0); 3],
    });
    assert!(changed.validate().is_err());
    let mut changed = plan;
    changed.cleanup_budget.transport.0 = 25;
    assert!(changed.validate().is_err());
}
