//! Codec/real-crypto tests only; these literal facts do not attest a Runtime.
use super::*;
use crate::world_service::{
    ExecutionBinding, SignedServiceResponse, request_digest, sign_read_request,
    sign_service_response,
};
use oasis7_agent_api::game::*;
fn fixture() -> (
    TaskCleanupLookupV2,
    SignedServiceResponse<TaskCleanupObservationV2>,
    String,
) {
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
    let digest = request.submitted.original_digest().unwrap();
    let closed = GameTaskClosedV2 {
        schema_revision: 2,
        close_operation_id: "close".into(),
        close_original_digest: digest.clone(),
        termination_plan_digest: "55".repeat(32),
        initial_fence: fence.clone(),
        actual_before_fence: fence,
        selected_predicate: GameTaskClosePredicateV2::InitialActive,
        revoked_signer_public_key: delegate.clone(),
        revoked_generation: DecimalU64(2),
        permanent_nonce_high_water: DecimalU64(2),
    };
    let commit = CommitRef {
        world: world.clone(),
        binding: ExecutionBinding {
            provider_world_id: "world".into(),
            branch_id: "main".into(),
            finality_ref: "finality".into(),
            reorg_generation: 0,
            governing_manifest_ref: "manifest".into(),
            authority_generation: 1,
            permission_generation: 1,
        },
        position: 3,
        execution_block_hash: "block".into(),
        state_root_ref: "root".into(),
    };
    let response = TaskCleanupObservationV2 {
        api_revision: 2,
        world,
        original_digest: digest,
        observation_nonce: request.observation_nonce.clone(),
        delivery_commit: commit.clone(),
        outcome: TaskCleanupOutcomeV2::KnownClosed {
            fact: Box::new(TaskCleanupFactV2 {
                closed,
                committed_height: DecimalU64(3),
                committed_at_unix_ms: DecimalU64(1000),
                closed_at_commit: commit,
                stop_predecessor_canonical_cbor: None,
            }),
            delivery: TaskCleanupDeliveryV2::ExactClosed(TaskCleanupCurrentV2 {
                current_owner_public_key: owner,
                current_owner_binding: "owner".into(),
                current_owner_generation: DecimalU64(1),
                current_revoked_signer_public_key: delegate,
                current_revoked_generation: DecimalU64(2),
                current_nonce_high_water: DecimalU64(2),
            }),
        },
    };
    request.validate().unwrap();
    response.validate(&request).unwrap();
    let node_private = "22".repeat(32);
    let node_public = sign_read_request("key", (), &node_private)
        .unwrap()
        .subject_public_key;
    let signed = sign_service_response(
        TASK_CLEANUP_LOOKUP_V2,
        request_digest(TASK_CLEANUP_LOOKUP_V2, &request).unwrap(),
        response,
        &node_private,
    )
    .unwrap();
    (request, signed, node_public)
}
#[test]
fn cleanup_v2_service_signature_nonce_and_whole_commit_are_bound() {
    let (request, signed, node_key) = fixture();
    verify_cleanup_observation_v2(&request, &signed, &node_key).unwrap();
    let mut other_nonce = request.clone();
    other_nonce.observation_nonce = "77".repeat(32);
    assert!(verify_cleanup_observation_v2(&other_nonce, &signed, &node_key).is_err());
    assert!(
        verify_cleanup_observation_v2(
            &request,
            &signed,
            &request.submitted.original.subject_public_key
        )
        .is_err()
    );
    for field in 0..12 {
        let mut altered = signed.clone();
        let c = &mut altered.payload.delivery_commit;
        match field {
            0 => c.position += 1,
            1 => c.execution_block_hash.push('x'),
            2 => c.state_root_ref.push('x'),
            3 => c.binding.finality_ref.push('x'),
            4 => c.binding.reorg_generation += 1,
            5 => c.binding.governing_manifest_ref.push('x'),
            6 => c.binding.authority_generation += 1,
            7 => c.binding.permission_generation += 1,
            8 => c.binding.provider_world_id.push('x'),
            9 => c.binding.branch_id.push('x'),
            10 => c.world.world_id.push('x'),
            _ => c.world.genesis_digest.push('x'),
        }
        assert!(verify_cleanup_observation_v2(&request, &altered, &node_key).is_err());
    }
}
#[test]
fn cleanup_v2_closed_wire_rejects_missing_extra_and_bad_nonce() {
    let (request, _, _) = fixture();
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(
        serde_json::from_value::<TaskCleanupLookupV2>(value.clone()).unwrap(),
        request
    );
    let mut extra = value.clone();
    extra["verified"] = true.into();
    assert!(serde_json::from_value::<TaskCleanupLookupV2>(extra).is_err());
    let mut missing = value;
    missing.as_object_mut().unwrap().remove("observation_nonce");
    assert!(serde_json::from_value::<TaskCleanupLookupV2>(missing).is_err());
    let mut wrong = request;
    wrong.observation_nonce = "nonce".into();
    assert!(wrong.validate().is_err());
}

#[test]
fn cleanup_v2_node_signature_is_strict_even_when_legacy_equation_accepts() {
    let (request, signed, node_key) = fixture();
    verify_cleanup_observation_v2(&request, &signed, &node_key).unwrap();
    let weak_key = format!("01{}", "00".repeat(31));
    let mut weak = signed;
    weak.signature_hex = format!("01{}", "00".repeat(63));
    crate::world_service::verify_service_response(
        TASK_CLEANUP_LOOKUP_V2,
        &request_digest(TASK_CLEANUP_LOOKUP_V2, &request).unwrap(),
        &weak,
        &weak_key,
    )
    .unwrap();
    assert!(verify_cleanup_observation_v2(&request, &weak, &weak_key).is_err());
}
