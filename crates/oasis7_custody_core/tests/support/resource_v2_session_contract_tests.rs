//! Scope fixtures validate a signed contract only, not actual enrollment/World.
use super::*;
use oasis7_custody_core::client_enrollment::{ClientPairingBindingV1, PairingInviteTargetRefV1};
fn scope_without_termination() -> ResourceTestSessionScopeV2 {
    let task=serde_json::from_value(json!({"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","agent_id":"agent","owner_binding":"owner","owner_generation":"1","owner_public_key":"bb".repeat(32),"task_id":"task","task_revision":"1","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"11".repeat(32),"prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"1"},"executor_epoch":"1","executor_binding":{"executor_id":"executor","executor_public_key":"cc".repeat(32),"delegation_generation":"1","executor_epoch":"1"}})).unwrap();
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
fn termination_fixture(s: &ResourceTestSessionScopeV2) -> TerminationPlanV2 {
    let f = &s.task_fence;
    let mut term = TerminationPlanV2 {
        api_version: 2,
        session_id: s.session_id.clone(),
        custody_account_id: s.client_pairing.target.custody_account_id.clone(),
        provision_id: s.client_pairing.target.provision_id.clone(),
        setup_id: "setup".into(),
        prepared_result_digest: "ab".repeat(32),
        world: s.world_identity.clone(),
        owner_key_ref: "owner".into(),
        executor_key_ref: s.executor_key_ref.clone(),
        initial_fence: f.clone(),
        original_delegation: oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1 {
            world_id: f.world_id.clone(),
            branch_id: f.branch_id.clone(),
            agent_id: f.agent_id.clone(),
            owner_binding: f.owner_binding.clone(),
            agent_identity_generation: f.owner_generation.0,
            generation: 1,
            delegate_public_key: "cc".repeat(32),
            revoked: false,
            nonce: 1,
        },
        stop_operation_id: String::new(),
        close_operation_id: String::new(),
        cleanup_until: oasis7_custody_core::time_units::UnixMillis::new(200_000),
        request_limit: DecimalU64(2),
        transport_limit: DecimalU64(24),
        trigger: TerminationTriggerV1::ClosingOnlyV1,
        nonce_policy: TerminationNoncePolicyV1::AccountAllocatedHighWaterV1,
        close_policy: WorldTaskClosePolicyV2::LiteralPersistentClosedV2,
    };
    term.stop_operation_id = term.expected_stop_operation_id().unwrap();
    term.close_operation_id = term.expected_close_operation_id().unwrap();
    term.validate().unwrap();
    term
}
fn scope() -> ResourceTestSessionScopeV2 {
    let mut s = scope_without_termination();
    s.termination_plan_digest = termination_fixture(&s).digest().unwrap();
    s
}
fn session_plan() -> TestSessionPlanV2 {
    TestSessionPlanV2 {
        termination: Box::new(termination_fixture(&scope_without_termination())),
        model_budget: None,
        api_version: 2,
        request_id: "session-request".into(),
        enrollment_id: "enrollment".into(),
        basis_digest: "aa".repeat(32),
        owner_setup_plan_digest: plan().digest().unwrap(),
        prepared_resource_basis_digest: Some("ab".repeat(32)),
        selection: selection().into(),
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
    proof.model_budget_summary_contract = input
        .request
        .expected_model_budget_summary_contract()
        .unwrap();
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
    let mut changed = serde_json::to_value(&plan).unwrap();
    changed["prepared_resource_basis_digest"] = serde_json::json!("ff".repeat(32));
    let changed: TestSessionPlanV2 = serde_json::from_value(changed).unwrap();
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

#[test]
fn model_budget_is_explicit_null_and_missing_is_not_a_legacy_upgrade() {
    let original = session_plan();
    original.validate().unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    assert!(value.as_object().unwrap().contains_key("model_budget"));
    assert!(value["model_budget"].is_null());
    assert_eq!(
        serde_json::from_value::<TestSessionPlanV2>(value.clone()).unwrap(),
        original
    );
    value.as_object_mut().unwrap().remove("model_budget");
    assert!(serde_json::from_value::<TestSessionPlanV2>(value).is_err());
}

#[test]
fn explicit_move0_selection_is_distinct_from_a_resource_basis() {
    let original = session_plan();
    original.validate().unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    value["selection"] = serde_json::json!({"kind":"move","position_cm":["1","-2","3"]});
    value["actions"] = serde_json::json!([{"tool":"move","position_cm":["1","-2","3"]}]);
    value["allowed_resource_refs"] = serde_json::json!([]);
    value["business_limit"] = serde_json::json!("0");
    value["prepared_resource_basis_digest"] = serde_json::Value::Null;
    let accepted: TestSessionPlanV2 =
        serde_json::from_value(value.clone()).expect("explicit V2 Move0 selection");
    accepted.validate().unwrap();
    assert_eq!(
        serde_json::to_value(&accepted).unwrap()["selection"],
        value["selection"]
    );
    let mut unknown = value.clone();
    unknown["selection"]["resource_refs"] = serde_json::json!([]);
    assert!(serde_json::from_value::<TestSessionPlanV2>(unknown).is_err());
    for case in 0..4 {
        let mut changed = value.clone();
        match case {
            0 => changed["business_limit"] = serde_json::json!("1"),
            1 => changed["allowed_resource_refs"] = serde_json::json!(["factory"]),
            2 => changed["prepared_resource_basis_digest"] = serde_json::json!("aa".repeat(32)),
            _ => changed["actions"][0]["position_cm"][0] = serde_json::json!("2"),
        }
        let changed: TestSessionPlanV2 = serde_json::from_value(changed).unwrap();
        assert!(changed.validate().is_err());
    }
    let mut resource = serde_json::to_value(original).unwrap();
    resource["allowed_resource_refs"] = serde_json::json!([]);
    assert!(
        serde_json::from_value::<TestSessionPlanV2>(resource)
            .unwrap()
            .validate()
            .is_err()
    );
}

#[path = "native_full_session_summary_contract_tests.rs"]
mod full_summary_tests;
