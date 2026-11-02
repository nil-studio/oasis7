//! Exact wire framing only; this does not attest prepaid/MFA authority.
use super::*;
use crate::world_service::{WorldIdentity, sign_read_request};
use oasis7_agent_api::game::*;
fn request() -> TaskCleanupLookupV2 {
    let owner_private = "11".repeat(32);
    let owner = sign_read_request("key", (), &owner_private)
        .unwrap()
        .subject_public_key;
    let delegate = sign_read_request("key", (), &"33".repeat(32))
        .unwrap()
        .subject_public_key;
    let fence = GameTaskFenceV1 {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
        owner_public_key: owner.clone(),
        task_id: "task".into(),
        task_revision: DecimalU64(1),
        task_status: GameTaskStatusV1::Active,
        latest_goal_revision: DecimalU64(1),
        active_goal_revision: DecimalU64(1),
        active_goal_reference: Some(GameGoalReferenceV1 {
            goal_revision: DecimalU64(1),
            goal_ref: "44".repeat(32),
            prepare_operation_id: "prepare".into(),
            publish_operation_id: "publish".into(),
            activation_committed_height: DecimalU64(1),
        }),
        executor_epoch: DecimalU64(1),
        executor_binding: Some(GameExecutorBindingV1 {
            executor_id: "executor".into(),
            executor_public_key: delegate.clone(),
            delegation_generation: DecimalU64(1),
            executor_epoch: DecimalU64(1),
        }),
    };
    let original = sign_read_request(
        GAME_TASK_CLEANUP_DOMAIN_V2,
        GameTaskRevokeAndCloseV2 {
            api_revision: 2,
            client_operation_id: "close".into(),
            custody_account_id: "account".into(),
            termination_plan_digest: "55".repeat(32),
            permitted_branches: GameTaskCloseConditionsV2 {
                exact_initial_active: fence.clone(),
                own_stop_intent: None,
            },
            signer_public_key: delegate.clone(),
            signer_generation: DecimalU64(1),
            signer_nonce: DecimalU64(1),
            revoke_generation: DecimalU64(2),
            revoke_nonce: DecimalU64(2),
            cleanup_until_ms: DecimalU64(5000),
        },
        &owner_private,
    )
    .unwrap();
    let world = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
    };
    let submitted = TaskCleanupSubmitV2 {
        api_revision: 2,
        expected_world: world.clone(),
        original,
    };
    let request = TaskCleanupLookupV2 {
        api_revision: 2,
        submitted,
        observation_nonce: "66".repeat(32),
    };
    request
}

#[test]
fn cleanup_http_framing_binds_actual_complete_body_and_rejects_nested_unknown() {
    let request = request();
    let body = CleanupTransportBodyV1::Lookup(Box::new(request));
    // Reuse only a real signed issuer payload fixture by building its exact
    // fields from the SDK contract; the issuer role is explicitly local here.
    let world = body.submitted().expected_world.clone();
    let p = PaidCleanupTransportV1 {
        api_revision: 1,
        issuer_namespace: "custody".into(),
        issuer_service_id: "account".into(),
        issuer_environment: "test".into(),
        issuer_key_id: "issuer".into(),
        issuer_epoch: DecimalU64(1),
        issuer_config_generation: DecimalU64(1),
        native: CleanupPeerInstanceV1 {
            origin: "https://native.test".into(),
            incarnation: "11".repeat(32),
        },
        world_peer: CleanupPeerInstanceV1 {
            origin: "https://world.test".into(),
            incarnation: "22".repeat(32),
        },
        world,
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        operation: CleanupHttpOperationV1::Lookup,
        custody_account_id: "account".into(),
        provision_id: "provision".into(),
        setup_id: "setup".into(),
        session_id: "session".into(),
        approval_operation_id: "approve".into(),
        approval_input_digest: "33".repeat(32),
        approved_intent_digest: "34".repeat(32),
        test_session_plan_digest: "35".repeat(32),
        termination_plan_digest: body
            .submitted()
            .original
            .request
            .termination_plan_digest
            .clone(),
        original_digest: body.submitted().original_digest().unwrap(),
        scope_digest: body.scope_digest().unwrap(),
        canonical_operation_id: "close".into(),
        source_operation_id: "source".into(),
        source_record_digest: "44".repeat(32),
        source_security_epoch: DecimalU64(1),
        directory_generation: DecimalU64(1),
        resource_flag_generation: DecimalU64(1),
        attempt_id: "attempt".into(),
        native_transport_id: "native-attempt".into(),
        world_transport_id: "world-attempt".into(),
        paid_record_digest: "45".repeat(32),
        native_body_digest: body.body_digest(CleanupConsumerRoleV1::Native).unwrap(),
        world_body_digest: body.body_digest(CleanupConsumerRoleV1::World).unwrap(),
        native_paid_index: DecimalU64(0),
        world_paid_index: DecimalU64(1),
        native_http_units: DecimalU64(1),
        world_http_units: DecimalU64(1),
        issued_at: DecimalU64(1000),
        expires_at: DecimalU64(4000),
        cleanup_until: DecimalU64(9000),
    };
    let paid = sign_read_request(PAID_CLEANUP_TRANSPORT_DOMAIN_V1, p, &"77".repeat(32))
        .unwrap()
        .into();
    let envelope = PaidCleanupHttpEnvelopeV1 {
        api_revision: 1,
        body,
        paid,
    };
    envelope
        .bound_body_digest(CleanupConsumerRoleV1::World)
        .unwrap();
    envelope
        .bound_body_digest(CleanupConsumerRoleV1::Native)
        .unwrap();
    assert_ne!(
        envelope.paid.request.native_body_digest,
        envelope.paid.request.world_body_digest
    );
    let bytes = serde_json::to_vec(&envelope).unwrap();
    let restored: PaidCleanupHttpEnvelopeV1 = serde_json::from_slice(&bytes).unwrap();
    assert!(restored == envelope);
    let mut altered = envelope.clone();
    let CleanupTransportBodyV1::Lookup(request) = &mut altered.body else {
        panic!("fixture operation");
    };
    request.observation_nonce = "99".repeat(32);
    assert!(
        altered
            .bound_body_digest(CleanupConsumerRoleV1::World)
            .is_err()
    );
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["body"]["request"]["submitted"]["original"]["ignored_authority"] =
        serde_json::json!(true);
    assert!(serde_json::from_value::<PaidCleanupHttpEnvelopeV1>(value).is_err());
}
