//! Catalog/mixed-batch boundaries only, not Native approval or World success.
use super::*;
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{SubmitIntentRequest, sign_read_request};
fn fixture() -> (
    TaskCleanupSubmitV2,
    SubmitIntentRequest<WorldServicePayloadV1>,
) {
    let private = "11".repeat(32);
    let owner = sign_read_request("key", (), &private)
        .unwrap()
        .subject_public_key;
    let delegate = sign_read_request("key", (), &"22".repeat(32))
        .unwrap()
        .subject_public_key;
    let world = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
    };
    let fence = GameTaskFenceV1 {
        world_id: world.world_id.clone(),
        genesis_digest: world.genesis_digest.clone(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
        owner_public_key: owner,
        task_id: "task".into(),
        task_revision: DecimalU64(1),
        task_status: GameTaskStatusV1::Active,
        latest_goal_revision: DecimalU64(1),
        active_goal_revision: DecimalU64(1),
        active_goal_reference: Some(GameGoalReferenceV1 {
            goal_revision: DecimalU64(1),
            goal_ref: "33".repeat(32),
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
            termination_plan_digest: "44".repeat(32),
            permitted_branches: GameTaskCloseConditionsV2 {
                exact_initial_active: fence.clone(),
                own_stop_intent: None,
            },
            signer_public_key: delegate,
            signer_generation: DecimalU64(1),
            signer_nonce: DecimalU64(1),
            revoke_generation: DecimalU64(2),
            revoke_nonce: DecimalU64(2),
            cleanup_until_ms: DecimalU64(5000),
        },
        &private,
    )
    .unwrap();
    let close = TaskCleanupSubmitV2 {
        api_revision: 2,
        expected_world: world.clone(),
        original,
    };
    close.verify_owner_signature().unwrap();
    let stop = sign_read_request(
        GAME_EXECUTOR_CONTROL_DOMAIN,
        GameExecutorControlV1 {
            api_rev: 1,
            client_operation_id: "stop".into(),
            fence,
            operation: GameExecutorOperationV1::Stop,
        },
        &private,
    )
    .unwrap();
    oasis7_client_api::legacy_task_results::verify_executor_control(&stop).unwrap();
    let payload = WorldServicePayloadV1::GameExecutorControl(stop);
    let legacy = SubmitIntentRequest {
        contract_version: 1,
        correlation: oasis7::world_service::correlation::derive_correlation(world, &payload)
            .unwrap(),
        signed_payload: payload,
        deadline_unix_ms: None,
    };
    (close, legacy)
}
fn context(actions: Vec<oasis7_node::NodeConsensusAction>) -> NodeExecutionCommitContext {
    NodeExecutionCommitContext {
        world_id: "world".into(),
        node_id: "node".into(),
        proposer_id: "node".into(),
        height: 1,
        slot: 0,
        epoch: 0,
        node_block_hash: "block".into(),
        action_root: oasis7_node::compute_consensus_action_root(&actions).unwrap(),
        committed_actions: actions,
        committed_at_unix_ms: 1000,
    }
}
#[test]
fn cleanup_v2_actual_catalog_decodes_explicit_body_and_keeps_legacy_capture_bytes() {
    let (close, legacy) = fixture();
    let bytes = oasis7::world_service::correlation::encode_cleanup_consensus_v2(&close).unwrap();
    let action = oasis7_node::NodeConsensusAction::from_payload(1, "transport", bytes).unwrap();
    let (runtime, simulator, bootstrap, inputs) =
        super::super::driver_replicated_input::decode_committed_actions(&context(vec![action]))
            .unwrap();
    assert!(runtime.is_empty() && simulator.is_empty() && bootstrap.is_none());
    assert_eq!(inputs.len(), 1);
    assert!(
        matches!(&inputs[0].1, RegisteredServiceIntent::CleanupV2(request) if **request == close)
    );
    assert_eq!(
        serde_cbor::to_vec(&RegisteredServiceIntent::LegacyV1(Box::new(legacy.clone()))).unwrap(),
        serde_cbor::to_vec(&legacy).unwrap()
    );
}
#[test]
fn cleanup_v2_mixed_inputs_are_typed_rejections_before_any_world_mutation() {
    let (close, legacy) = fixture();
    for reverse in [false, true] {
        let mut inputs = vec![
            (
                1,
                RegisteredServiceIntent::CleanupV2(Box::new(close.clone())),
            ),
            (
                2,
                RegisteredServiceIntent::LegacyV1(Box::new(legacy.clone())),
            ),
        ];
        if reverse {
            inputs.reverse();
        }
        assert_eq!(
            validate_batch(&inputs),
            Err(CleanupBatchErrorV2::UnsupportedMixedV2)
        );
        let mut world = World::new();
        let before = world.snapshot();
        let error = apply_registered_intents(
            &mut world,
            &context(Vec::new()),
            Some(&close.expected_world),
            inputs,
        )
        .err()
        .unwrap();
        assert_eq!(error, "UnsupportedMixedV2");
        assert_eq!(world.snapshot(), before);
    }
}
#[test]
fn cleanup_v2_requires_runtime_begun_context_without_caller_clock_escape() {
    let (close, _) = fixture();
    let mut world = World::new();
    let before = world.snapshot();
    let expected_world = close.expected_world.clone();
    let result = apply_registered_intents(
        &mut world,
        &context(Vec::new()),
        Some(&expected_world),
        vec![(1, RegisteredServiceIntent::CleanupV2(Box::new(close)))],
    );
    assert!(result.is_err());
    assert_eq!(world.snapshot(), before);
}

#[test]
fn cleanup_v2_catalog_rejects_ignored_additional_cbor_fields() {
    use oasis7::consensus_action_payload::*;
    let (close, _) = fixture();
    let bytes = oasis7::world_service::correlation::encode_cleanup_consensus_v2(&close).unwrap();
    let positive = decode_consensus_action_payload_envelope(&bytes).unwrap();
    let mut value: serde_cbor::Value = serde_cbor::from_slice(&bytes).unwrap();
    let serde_cbor::Value::Map(top) = &mut value else {
        unreachable!()
    };
    let serde_cbor::Value::Map(body) = top
        .get_mut(&serde_cbor::Value::Text("body".into()))
        .unwrap()
    else {
        unreachable!()
    };
    body.insert(
        serde_cbor::Value::Text("unexpected".into()),
        serde_cbor::Value::Bool(true),
    );
    let altered = serde_cbor::to_vec(&value).unwrap();
    let ignored: ConsensusActionPayloadEnvelope = serde_cbor::from_slice(&altered).unwrap();
    assert_eq!(ignored, positive); // Actual old typed Serde would ignore the extra field.
    assert!(decode_consensus_action_payload_envelope(&altered).is_err());
}
