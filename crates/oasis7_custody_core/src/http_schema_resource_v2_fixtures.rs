//! Typed schema examples only; these local test keys confer no live authority.
use super::*;
use crate::client_enrollment::{ClientPairingBindingV1, PairingInviteTargetRefV1};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_client_api::world_service::resource_basis as wire;
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
        cleanup_until: crate::time_units::UnixMillis::new(200_000),
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
        model_budget: None,
        termination: Box::new(termination_fixture(&scope_without_termination())),
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

fn key43() -> SigningKey {
    SigningKey::from_bytes(&[43; 32])
}
fn hexbytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn signed(payload: PaidResourceReadPayloadV1) -> PaidResourceReadProofV1 {
    let signature_hex = hexbytes(&key43().sign(&payload.signing_bytes().unwrap()).to_bytes());
    PaidResourceReadProofV1 {
        payload,
        signature_hex,
    }
}
fn fixture_paid() -> (
    PaidResourceReadProofV1,
    PaidResourceReadIssuerPinV1,
    PaidResourceReadIssuerFloorV1,
) {
    let payload = PaidResourceReadPayloadV1 {
        api_version: 1,
        issuer_namespace: "custody:test".into(),
        issuer_service_id: "identity".into(),
        issuer_environment: "test".into(),
        issuer_key_id: "resource-issuer".into(),
        issuer_epoch: DecimalU64(2),
        issuer_config_generation: DecimalU64(3),
        native_audience_origin: "https://native.example".into(),
        world_audience: plan().world_identity,
        native_path: wire::NATIVE_OWNER_RESOURCE_VIEW_PATH_V1.into(),
        world_path: wire::OWNER_RESOURCE_VIEW_PATH_V1.into(),
        stage: PaidResourceReadStageV1::TestSessionOwnerRead,
        custody_account_id: "custody-one".into(),
        manager_native_account_id: "manager-one".into(),
        participant_native_account_id: "participant-one".into(),
        provision_id: "provision".into(),
        setup_id: "setup".into(),
        test_session_id: Some("session".into()),
        approval_operation_id: "approval-operation".into(),
        approval_input_digest: "a1".repeat(32),
        plan_digest: "a2".repeat(32),
        prepared_resource_basis_digest: Some("a3".repeat(32)),
        selection_digest: "a4".repeat(32),
        owner_public_key: "a5".repeat(32),
        owner_binding: "owner".into(),
        task_fence_digest: "a6".repeat(32),
        signer_record_digest: "a7".repeat(32),
        native_request_digest: "a8".repeat(32),
        world_request_digest: "a9".repeat(32),
        preflight_world_request_digest: None,
        source_operation_id: "source".into(),
        source_record_digest: "b1".repeat(32),
        source_security_epoch: DecimalU64(2),
        service_epoch: DecimalU64(3),
        directory_generation: DecimalU64(4),
        resource_flag_generation: DecimalU64(5),
        paid_attempt_id: "attempt".into(),
        paid_record_digest: "b2".repeat(32),
        native_http_units: DecimalU64(1),
        world_http_units: DecimalU64(1),
        issued_at: DecimalU64(1000),
        expires_at: DecimalU64(6000),
    };
    let pin = PaidResourceReadIssuerPinV1 {
        namespace: payload.issuer_namespace.clone(),
        service_id: payload.issuer_service_id.clone(),
        environment: payload.issuer_environment.clone(),
        key_id: payload.issuer_key_id.clone(),
        public_key: hexbytes(&key43().verifying_key().to_bytes()),
        issuer_epoch: payload.issuer_epoch,
        issuer_config_generation: payload.issuer_config_generation,
        not_before: DecimalU64(1),
        not_after: DecimalU64(10_000),
        native_audience_origin: payload.native_audience_origin.clone(),
        world_audience: payload.world_audience.clone(),
        allowed_stage: payload.stage,
    };
    (
        signed(payload),
        pin,
        PaidResourceReadIssuerFloorV1 {
            issuer_epoch: DecimalU64(2),
            config_generation: DecimalU64(3),
        },
    )
}

pub(super) fn fixtures() -> Value {
    let owner = plan();
    owner.validate().unwrap();
    let session = session_plan();
    let scope = scope();
    session.validate_scope(&scope, 1000).unwrap();
    let (proof, pin, floor) = fixture_paid();
    proof.verify_issuer(&pin, &floor, 1000).unwrap();
    json!({
        "OwnerSetupPlanV2": [owner],
        "TestSessionPlanV2": [session],
        "ResourceTestSessionScopeV2": [scope],
        "PaidResourceReadProofV1": [proof],
        "PaidResourceReadIssuerPinV1": [pin],
        "PaidResourceReadIssuerFloorV1": [floor]
    })
}
