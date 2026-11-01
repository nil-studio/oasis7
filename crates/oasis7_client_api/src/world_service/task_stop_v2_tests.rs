//! Real crypto/closed codec component fixture, not Runtime or paid admission.
use super::*;
use crate::world_service::{
    ExecutionBinding, request_digest, sign_read_request, sign_service_response,
};
use oasis7_agent_api::game::*;
fn fixture() -> (
    TaskStopLookupV2,
    SignedServiceResponse<TaskStopObservationV2>,
    String,
) {
    let owner = sign_read_request("key", (), &"11".repeat(32))
        .unwrap()
        .subject_public_key;
    let executor = sign_read_request("key", (), &"33".repeat(32))
        .unwrap()
        .subject_public_key;
    let before:GameTaskControlV1=serde_json::from_value(serde_json::json!({
        "schema_revision":1,"task_id":"task","world_id":"world","genesis_digest":"genesis",
        "branch_id":"main","reorg_epoch":"0","agent_id":"agent","owner_binding":"owner",
        "owner_generation":"1","owner_public_key":owner,"task_revision":"3","task_status":"active",
        "latest_goal_revision":"1","active_goal_revision":"1","executor_epoch":"1","executor_id":"executor",
        "created_operation_id":"create","active_goal_reference":{"goal_revision":"1","goal_ref":"44".repeat(32),"prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"2"},
        "executor_binding":{"executor_id":"executor","executor_public_key":executor,"delegation_generation":"1","executor_epoch":"1"}
    })).unwrap();
    let raw = sign_read_request(
        GAME_TASK_STOP_DOMAIN_V2,
        GameTaskStopV2 {
            api_revision: 2,
            client_operation_id: "stop".into(),
            custody_account_id: "account".into(),
            termination_plan_digest: "55".repeat(32),
            initial_fence: GameTaskFenceV1::from_control(&before),
            target_executor_public_key: executor,
            target_executor_generation: DecimalU64(1),
            target_executor_nonce: DecimalU64(1),
            execute_until_ms: DecimalU64(5000),
            observation_grant: None,
        },
        &"11".repeat(32),
    )
    .unwrap();
    let original = SignedGameTaskStopV2 {
        request: raw.request,
        subject_public_key: raw.subject_public_key,
        signature_hex: raw.signature_hex,
    };
    let after = original.request.derive_post_control(&before).unwrap();
    let mut record = GameTaskStopRecordV2 {
        schema_revision: 2,
        original: original.clone(),
        before_control: before,
        after_control: after.clone(),
        committed_height: 4,
        committed_at_unix_ms: 1000,
        result_core_digest: String::new(),
    };
    record.result_core_digest = record.compute_result_core_digest().unwrap();
    let world = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
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
        position: 4,
        execution_block_hash: "block".into(),
        state_root_ref: "root".into(),
    };
    let request = TaskStopLookupV2 {
        api_revision: 2,
        submitted: TaskStopSubmitV2 {
            api_revision: 2,
            expected_world: world.clone(),
            original,
            observation_nonce: "aa".repeat(32),
        },
        observation_nonce: "66".repeat(32),
    };
    let observation = TaskStopObservationV2 {
        api_revision: 2,
        world,
        original_digest: request.submitted.original_digest().unwrap(),
        observation_nonce: request.observation_nonce.clone(),
        delivery_commit: commit.clone(),
        outcome: TaskStopOutcomeV2::KnownStopped {
            fact: Box::new(TaskStopFactV2 {
                record,
                stopped_at_commit: commit,
            }),
            delivery: TaskStopDeliveryV2::ExactPostFence(Box::new(GameTaskFenceV1::from_control(
                &after,
            ))),
        },
    };
    let signed = sign_service_response(
        TASK_STOP_LOOKUP_V2,
        request_digest(TASK_STOP_LOOKUP_V2, &request).unwrap(),
        observation,
        &"22".repeat(32),
    )
    .unwrap();
    let node = sign_read_request("key", (), &"22".repeat(32))
        .unwrap()
        .subject_public_key;
    (request, signed, node)
}
#[test]
fn guarded_stop_v2_observation_strong_signature_exact_original_and_commit() {
    let (request, response, node) = fixture();
    verify_stop_observation_v2(&request, &response, &node).unwrap();
    let mut changed = request.clone();
    changed.observation_nonce = "77".repeat(32);
    assert!(verify_stop_observation_v2(&changed, &response, &node).is_err());
    let wrong = sign_read_request("key", (), &"88".repeat(32))
        .unwrap()
        .subject_public_key;
    assert!(verify_stop_observation_v2(&request, &response, &wrong).is_err());
    let mut payload = response.payload.clone();
    if let TaskStopOutcomeV2::KnownStopped { fact, .. } = &mut payload.outcome {
        fact.stopped_at_commit.state_root_ref = "different-root".into();
    }
    let resigned = sign_service_response(
        TASK_STOP_LOOKUP_V2,
        request_digest(TASK_STOP_LOOKUP_V2, &request).unwrap(),
        payload,
        &"22".repeat(32),
    )
    .unwrap();
    assert!(verify_stop_observation_v2(&request, &resigned, &node).is_err());
}
#[test]
fn guarded_stop_v2_known_fact_survives_current_drift_but_is_not_exact_post_current() {
    let (request, response, node) = fixture();
    let mut payload = response.payload;
    if let TaskStopOutcomeV2::KnownStopped { delivery, .. } = &mut payload.outcome {
        *delivery = TaskStopDeliveryV2::Drift;
    }
    let signed = sign_service_response(
        TASK_STOP_LOOKUP_V2,
        request_digest(TASK_STOP_LOOKUP_V2, &request).unwrap(),
        payload,
        &"22".repeat(32),
    )
    .unwrap();
    verify_stop_observation_v2(&request, &signed, &node).unwrap();
    assert!(matches!(
        signed.payload.outcome,
        TaskStopOutcomeV2::KnownStopped {
            delivery: TaskStopDeliveryV2::Drift,
            ..
        }
    ));
    let mut wire = serde_json::to_value(&request).unwrap();
    wire["free_retry"] = true.into();
    assert!(serde_json::from_value::<TaskStopLookupV2>(wire).is_err());
}

#[test]
fn guarded_stop_v2_full_witness_is_required_by_closed_cleanup_observation() {
    use crate::world_service::task_cleanup_v2::*;
    let (stop_request, stop_response, node) = fixture();
    let TaskStopOutcomeV2::KnownStopped { fact, .. } = stop_response.payload.outcome else {
        panic!("fixture Stop fact");
    };
    let record = fact.record;
    let initial = record.original.request.initial_fence.clone();
    let post = GameTaskFenceV1::from_control(&record.after_control);
    let intent_digest = oasis7_wasm_abi::canonical_hash(&(
        GAME_TASK_STOP_DOMAIN_V2,
        &record.original.subject_public_key,
        &record.original.request,
    ))
    .unwrap();
    let original_digest = stop_request.submitted.original_digest().unwrap();
    let original = sign_read_request(
        GAME_TASK_CLEANUP_DOMAIN_V2,
        GameTaskRevokeAndCloseV2 {
            api_revision: 2,
            client_operation_id: "close".into(),
            custody_account_id: "account".into(),
            termination_plan_digest: record.original.request.termination_plan_digest.clone(),
            permitted_branches: GameTaskCloseConditionsV2 {
                exact_initial_active: initial.clone(),
                own_stop_intent: Some(GameOwnStopIntentV2 {
                    profile: GameStopIntentProfileV2::GuardedTaskStopV2,
                    operation_id: "stop".into(),
                    intent_digest,
                }),
            },
            signer_public_key: record.original.request.target_executor_public_key.clone(),
            signer_generation: DecimalU64(1),
            signer_nonce: DecimalU64(1),
            revoke_generation: DecimalU64(2),
            revoke_nonce: DecimalU64(2),
            cleanup_until_ms: DecimalU64(5000),
        },
        &"11".repeat(32),
    )
    .unwrap();
    let lookup = TaskCleanupLookupV2 {
        api_revision: 2,
        submitted: TaskCleanupSubmitV2 {
            api_revision: 2,
            expected_world: stop_request.submitted.expected_world.clone(),
            original,
        },
        observation_nonce: "99".repeat(32),
    };
    let mut commit = fact.stopped_at_commit;
    commit.position = 5;
    commit.execution_block_hash = "close-block".into();
    commit.state_root_ref = "close-root".into();
    let closed = GameTaskClosedV2 {
        schema_revision: 2,
        close_operation_id: "close".into(),
        close_original_digest: lookup.submitted.original_digest().unwrap(),
        termination_plan_digest: record.original.request.termination_plan_digest.clone(),
        initial_fence: initial,
        actual_before_fence: post.clone(),
        selected_predicate: GameTaskClosePredicateV2::OwnStopSuspended(Box::new(
            GameOwnStopReferenceV2 {
                profile: GameStopIntentProfileV2::GuardedTaskStopV2,
                operation_id: "stop".into(),
                original_digest,
                canonical_result_digest: record.result_core_digest.clone(),
                post_fence: post,
            },
        )),
        revoked_signer_public_key: record.original.request.target_executor_public_key.clone(),
        revoked_generation: DecimalU64(2),
        permanent_nonce_high_water: DecimalU64(2),
    };
    let payload = TaskCleanupObservationV2 {
        api_revision: 2,
        world: lookup.submitted.expected_world.clone(),
        original_digest: lookup.submitted.original_digest().unwrap(),
        observation_nonce: lookup.observation_nonce.clone(),
        delivery_commit: commit.clone(),
        outcome: TaskCleanupOutcomeV2::KnownClosed {
            fact: Box::new(TaskCleanupFactV2 {
                closed,
                committed_height: DecimalU64(5),
                committed_at_unix_ms: DecimalU64(2000),
                closed_at_commit: commit,
                stop_predecessor_canonical_cbor: Some(serde_cbor::to_vec(&record).unwrap()),
            }),
            delivery: TaskCleanupDeliveryV2::ExactClosed(TaskCleanupCurrentV2 {
                current_owner_public_key: record.original.subject_public_key.clone(),
                current_owner_binding: "owner".into(),
                current_owner_generation: DecimalU64(1),
                current_revoked_signer_public_key: record
                    .original
                    .request
                    .target_executor_public_key
                    .clone(),
                current_revoked_generation: DecimalU64(2),
                current_nonce_high_water: DecimalU64(2),
            }),
        },
    };
    let signed = sign_service_response(
        TASK_CLEANUP_LOOKUP_V2,
        request_digest(TASK_CLEANUP_LOOKUP_V2, &lookup).unwrap(),
        payload.clone(),
        &"22".repeat(32),
    )
    .unwrap();
    verify_cleanup_observation_v2(&lookup, &signed, &node).unwrap();
    let mut changed = payload;
    if let TaskCleanupOutcomeV2::KnownClosed { fact, .. } = &mut changed.outcome {
        let mut bad = record;
        bad.original.signature_hex = "00".repeat(64);
        bad.result_core_digest = bad.compute_result_core_digest().unwrap();
        fact.stop_predecessor_canonical_cbor = Some(serde_cbor::to_vec(&bad).unwrap());
    }
    // A fresh genuine Node signature cannot authenticate a forged Owner predecessor.
    let resigned = sign_service_response(
        TASK_CLEANUP_LOOKUP_V2,
        request_digest(TASK_CLEANUP_LOOKUP_V2, &lookup).unwrap(),
        changed,
        &"22".repeat(32),
    )
    .unwrap();
    assert!(verify_cleanup_observation_v2(&lookup, &resigned, &node).is_err());
}

#[test]
fn guarded_stop_v2_submit_and_lookup_signatures_are_not_interchangeable() {
    let (request, lookup, node) = fixture();
    let mut payload = lookup.payload.clone();
    payload.observation_nonce = request.submitted.observation_nonce.clone();
    let submit = sign_service_response(
        TASK_STOP_SUBMIT_V2,
        request_digest(TASK_STOP_SUBMIT_V2, &request.submitted).unwrap(),
        payload,
        &"22".repeat(32),
    )
    .unwrap();
    verify_stop_submit_observation_v2(&request.submitted, &submit, &node).unwrap();
    assert!(verify_stop_observation_v2(&request, &submit, &node).is_err());
    assert!(verify_stop_submit_observation_v2(&request.submitted, &lookup, &node).is_err());
}

#[path = "task_stop_observation_grant_tests.rs"]
mod observation_grant;
