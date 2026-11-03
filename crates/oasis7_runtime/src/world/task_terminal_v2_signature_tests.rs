//! Pure V2 signature-validator boundary, not a canonical World-owner bypass.
use super::*;
use admission::sign_read_request;
fn fixture() -> TaskTerminalRecordV2 {
    let private = "11".repeat(32);
    let owner = sign_read_request("key", (), &private)
        .unwrap()
        .subject_public_key;
    let delegate = sign_read_request("key", (), &"22".repeat(32))
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
            signer_public_key: delegate.clone(),
            signer_generation: DecimalU64(1),
            signer_nonce: DecimalU64(1),
            revoke_generation: DecimalU64(2),
            revoke_nonce: DecimalU64(2),
            cleanup_until_ms: DecimalU64(5000),
        },
        &private,
    )
    .unwrap();
    TaskTerminalRecordV2 {
        schema_revision: 2,
        committed_height: 3,
        committed_at_unix_ms: 1000,
        stop_predecessor_canonical_cbor: None,
        closed: GameTaskClosedV2 {
            schema_revision: 2,
            close_operation_id: "close".into(),
            close_original_digest: hash(&original).unwrap(),
            termination_plan_digest: "44".repeat(32),
            initial_fence: fence.clone(),
            actual_before_fence: fence,
            selected_predicate: GameTaskClosePredicateV2::InitialActive,
            revoked_signer_public_key: delegate,
            revoked_generation: DecimalU64(2),
            permanent_nonce_high_water: DecimalU64(2),
        },
        original,
    }
}
#[test]
fn terminal_v2_signature_validator_rejects_real_weak_key_after_legacy_accepts() {
    let strong = fixture();
    strong.original.request.validate().unwrap();
    admission::verify_read_request(GAME_TASK_CLEANUP_DOMAIN_V2, &strong.original).unwrap();
    signature(&strong).unwrap();
    let weak_key = format!("01{}", "00".repeat(31));
    let weak_signature = format!("01{}", "00".repeat(63));
    let mut weak = strong;
    weak.original.subject_public_key = weak_key.clone();
    weak.original.signature_hex = weak_signature;
    weak.original
        .request
        .permitted_branches
        .exact_initial_active
        .owner_public_key = weak_key;
    weak.closed.initial_fence = weak
        .original
        .request
        .permitted_branches
        .exact_initial_active
        .clone();
    weak.closed.actual_before_fence = weak.closed.initial_fence.clone();
    weak.closed.close_original_digest = hash(&weak.original).unwrap();
    weak.original.request.validate().unwrap();
    // Same domain, complete typed message and actual Ed25519 weak-key equation.
    // If this precondition fails, it is not the intended signature-boundary RED.
    admission::verify_read_request(GAME_TASK_CLEANUP_DOMAIN_V2, &weak.original).unwrap();
    assert!(signature(&weak).is_err());
}
