//! New catalog Owner signature boundary, not full World/paid Native authority.
use super::*;
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{
    WorldIdentity, sign_read_request, task_cleanup_v2::TaskCleanupSubmitV2,
};
pub(crate) fn request() -> TaskCleanupSubmitV2 {
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
    close
}
#[test]
fn cleanup_v2_node_catalog_authenticates_real_owner_before_queue() {
    let config = NodeConfig::new("node", "world", crate::NodeRole::Observer).unwrap();
    let mut envelope = LocalConsensusActionPayloadEnvelope {
        version: 1,
        gameplay_submission_origin: None,
        auth: None,
        body: LocalConsensusActionPayloadBody::WorldCleanupV2 {
            request: Box::new(request()),
        },
    };
    let bytes = cleanup_v2_catalog::encode_closed_task_catalog(&envelope).unwrap();
    governance_auth::validate_consensus_action_payload_auth(&config, &bytes).unwrap();
    let LocalConsensusActionPayloadBody::WorldCleanupV2 { request } = &mut envelope.body else {
        unreachable!()
    };
    request.original.request.cleanup_until_ms.0 += 1;
    assert!(
        governance_auth::validate_consensus_action_payload_auth(
            &config,
            &cleanup_v2_catalog::encode_closed_task_catalog(&envelope).unwrap()
        )
        .is_err()
    );
}

#[test]
fn cleanup_v2_without_qualified_execution_hook_never_enters_actual_queue() {
    let config = NodeConfig::new("node", "world", crate::NodeRole::Observer).unwrap();
    let envelope = LocalConsensusActionPayloadEnvelope {
        version: 1,
        gameplay_submission_origin: None,
        auth: None,
        body: LocalConsensusActionPayloadBody::WorldCleanupV2 {
            request: Box::new(request()),
        },
    };
    let bytes = cleanup_v2_catalog::encode_closed_task_catalog(&envelope).unwrap();
    governance_auth::validate_consensus_action_payload_auth(&config, &bytes).unwrap();
    let runtime = NodeRuntime::new(config);
    assert!(runtime.submit_consensus_action_payload(1, bytes).is_err());
    assert!(runtime.pending_consensus_actions.lock().unwrap().is_empty());
}

pub(crate) fn stop_catalog(
    request: oasis7_client_api::world_service::task_stop_v2::TaskStopSubmitV2,
) -> Vec<u8> {
    cleanup_v2_catalog::encode_closed_task_catalog(&LocalConsensusActionPayloadEnvelope {
        version: 1,
        auth: None,
        gameplay_submission_origin: None,
        body: LocalConsensusActionPayloadBody::WorldGuardedStopV2 {
            request: Box::new(request),
        },
    })
    .unwrap()
}
pub(crate) fn close_catalog(request: TaskCleanupSubmitV2) -> Vec<u8> {
    cleanup_v2_catalog::encode_closed_task_catalog(&LocalConsensusActionPayloadEnvelope {
        version: 1,
        auth: None,
        gameplay_submission_origin: None,
        body: LocalConsensusActionPayloadBody::WorldCleanupV2 {
            request: Box::new(request),
        },
    })
    .unwrap()
}

#[path = "node_runtime_closed_preflight_contention_tests.rs"]
mod contention_tests;

#[test]
fn closed_task_official_shape_rejects_null_auth_origin_and_unknown_fields_without_changing_v1() {
    let envelope = LocalConsensusActionPayloadEnvelope {
        version: 1,
        auth: None,
        gameplay_submission_origin: None,
        body: LocalConsensusActionPayloadBody::WorldCleanupV2 {
            request: Box::new(request()),
        },
    };
    let canonical = cleanup_v2_catalog::encode_closed_task_catalog(&envelope).unwrap();
    decode_local_consensus_action_payload_envelope(&canonical).unwrap();
    assert_eq!(canonical[0], 0xa2);
    let mut duplicate = canonical.clone();
    duplicate[0] = 0xa3;
    duplicate.extend(serde_cbor::to_vec(&"version").unwrap());
    duplicate.extend(serde_cbor::to_vec(&1u8).unwrap());
    assert!(decode_local_consensus_action_payload_envelope(&duplicate).is_err());
    let with_null_auth = serde_cbor::to_vec(&envelope).unwrap();
    assert_ne!(canonical, with_null_auth);
    assert!(decode_local_consensus_action_payload_envelope(&with_null_auth).is_err());
    #[derive(serde::Serialize)]
    struct Extra<'a> {
        version: u8,
        body: &'a LocalConsensusActionPayloadBody,
        trusted: bool,
    }
    let extra = serde_cbor::to_vec(&Extra {
        version: 1,
        body: &envelope.body,
        trusted: true,
    })
    .unwrap();
    assert!(decode_local_consensus_action_payload_envelope(&extra).is_err());
    #[derive(serde::Serialize)]
    struct NullOrigin<'a> {
        version: u8,
        gameplay_submission_origin: Option<()>,
        body: &'a LocalConsensusActionPayloadBody,
    }
    let null_origin = serde_cbor::to_vec(&NullOrigin {
        version: 1,
        gameplay_submission_origin: None,
        body: &envelope.body,
    })
    .unwrap();
    assert!(decode_local_consensus_action_payload_envelope(&null_origin).is_err());
    let v1 = LocalConsensusActionPayloadEnvelope {
        version: 1,
        auth: None,
        gameplay_submission_origin: None,
        body: LocalConsensusActionPayloadBody::RuntimeAction {
            action: serde_json::json!({"type":"Noop","data":{}}),
        },
    };
    let legacy = serde_cbor::to_vec(&v1).unwrap();
    let serde_cbor::Value::Map(map) = serde_cbor::from_slice(&legacy).unwrap() else {
        unreachable!()
    };
    assert_eq!(
        map.get(&serde_cbor::Value::Text("auth".into())),
        Some(&serde_cbor::Value::Null)
    );
    decode_local_consensus_action_payload_envelope(&legacy).unwrap();
}

#[path = "node_runtime_closed_inner_lock_tests.rs"]
mod inner_lock;
