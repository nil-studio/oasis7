use super::*;
use crate::world_service::{
    CommitRef, ExecutionBinding, SignedServiceResponse, WorldIdentity, private_goal_read::*,
    request_digest, sign_service_response,
};
use oasis7_agent_api::game::GameGoalReferenceV1;

#[test]
fn scoped_current_goal_chain_checks_complete_activation_publication_and_context() {
    let original = sign_goal_prepare(request(), &key()).unwrap();
    let salt = [23u8; 32];
    let receipt = PreparedGoalReceiptV1 {
        schema_revision: 1,
        goal_ref: prepared_goal_commitment(&salt, &original).unwrap(),
        reference_salt: hex::encode(salt),
        owner_proof: original.clone(),
    };
    let world = WorldIdentity {
        world_id: original.request.world_id.clone(),
        genesis_digest: original.request.genesis_digest.clone(),
    };
    let commit = |position| CommitRef {
        world: world.clone(),
        binding: ExecutionBinding {
            provider_world_id: world.world_id.clone(),
            branch_id: original.request.branch_id.clone(),
            finality_ref: "fixture-finality".into(),
            reorg_generation: original.request.reorg_epoch.0,
            governing_manifest_ref: "fixture-manifest".into(),
            authority_generation: 1,
            permission_generation: 1,
        },
        position,
        execution_block_hash: format!("fixture-block-{position}"),
        state_root_ref: format!("fixture-state-{position}"),
    };
    let read = sign_read_request(
        GOAL_CURRENT_PATH,
        CurrentGoalRequest {
            schema_revision: 1,
            world: world.clone(),
            agent_id: original.request.agent_id.clone(),
            task_id: original.request.task_id.clone(),
        },
        &key(),
    )
    .unwrap();
    let mut current = control(&original.request);
    current.task_revision = DecimalU64(2);
    current.latest_goal_revision = DecimalU64(1);
    current.active_goal_revision = DecimalU64(1);
    current.task_status = GameTaskStatusV1::Active;
    current.active_goal_reference = Some(GameGoalReferenceV1 {
        goal_revision: DecimalU64(1),
        goal_ref: receipt.goal_ref.clone(),
        prepare_operation_id: original.request.client_operation_id.clone(),
        publish_operation_id: "fixture-publication".into(),
        activation_committed_height: DecimalU64(2),
    });
    let payload = CurrentGoalResponse {
        schema_revision: 1,
        control: current,
        read_commit: commit(3),
        activation_commit: Some(commit(2)),
        active_goal: Some(receipt),
    };
    let service_key = hex::encode([31; 32]);
    let pin = sign_read_request("pin", (), &service_key)
        .unwrap()
        .subject_public_key;
    let sign_reply = |payload| {
        sign_service_response(
            GOAL_CURRENT_PATH,
            request_digest(GOAL_CURRENT_PATH, &read).unwrap(),
            payload,
            &service_key,
        )
        .unwrap()
    };
    let response: SignedServiceResponse<CurrentGoalResponse> = sign_reply(payload.clone());
    let mut scope = StrictOwnerSignatureScope::default();
    verify_current_response_with_signature_scope(&read, &response, &pin, &mut scope).unwrap();
    verify_current_response(&read, &response, &pin).unwrap();
    assert!(scope.summary().nested_calls > 0);
    assert!(scope.summary().nested_repeats > 0);
    assert_eq!(scope.summary().failures, 0);
    let mut wrong_activation = payload.clone();
    wrong_activation
        .activation_commit
        .as_mut()
        .unwrap()
        .position = 4;
    let mut wrong_publication = payload.clone();
    wrong_publication
        .control
        .active_goal_reference
        .as_mut()
        .unwrap()
        .prepare_operation_id = "foreign-preparation".into();
    let mut wrong_context = payload;
    wrong_context.control.task_id = "foreign-task".into();
    for changed in [wrong_activation, wrong_publication, wrong_context] {
        let actual = sign_reply(changed);
        // These are real, valid World signatures; only typed semantic binding
        // must reject them after the same scope already saw a valid full packet.
        verify_owner_signature_strict(
            GOAL_CURRENT_PATH,
            &(&actual.request_digest, &actual.payload),
            &pin,
            &actual.signature_hex,
        )
        .unwrap();
        let before = scope.summary().strict_attempts;
        assert!(
            verify_current_response_with_signature_scope(&read, &actual, &pin, &mut scope).is_err()
        );
        assert!(scope.summary().strict_attempts > before);
        assert_eq!(scope.summary().failures, 0);
        assert!(verify_current_response(&read, &actual, &pin).is_err());
    }
}
