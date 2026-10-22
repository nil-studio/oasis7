use oasis7_agent_api::game::*;
use serde_json::json;
fn initial() -> GameTaskControlV1 {
    serde_json::from_value(json!({"schema_revision":1,"task_id":"t","world_id":"w","genesis_digest":"g","branch_id":"b","reorg_epoch":"0","agent_id":"a","owner_binding":"o","owner_generation":"1","owner_public_key":"ab".repeat(32),"task_revision":"2","task_status":"suspended","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"cd".repeat(32),"prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"2"},"executor_epoch":"0","executor_id":null,"created_operation_id":"create"})).unwrap()
}
#[test]
fn executor_control_preserves_legacy_none_and_rejects_noncanonical_or_overflow_fences() {
    let control = initial();
    assert!(
        serde_json::to_value(&control)
            .unwrap()
            .get("executor_binding")
            .is_none()
    );
    let request = GameExecutorControlV1 {
        api_rev: 1,
        client_operation_id: "bind".into(),
        fence: GameTaskFenceV1::from_control(&control),
        operation: GameExecutorOperationV1::Bind {
            executor_id: "executor".into(),
            executor_public_key: "ef".repeat(32),
            delegation_generation: DecimalU64(1),
        },
    };
    request.validate().unwrap();
    let encoded = serde_json::to_value(&request).unwrap();
    for invalid in [json!(2), json!("02"), json!("-1")] {
        let mut value = encoded.clone();
        value["fence"]["task_revision"] = invalid;
        assert!(serde_json::from_value::<GameExecutorControlV1>(value).is_err());
    }
    let mut unknown = encoded;
    unknown["private_body"] = json!("secret");
    assert!(serde_json::from_value::<GameExecutorControlV1>(unknown).is_err());
    for epoch in [false, true] {
        let mut overflow = request.clone();
        if epoch {
            overflow.fence.executor_epoch = DecimalU64(u64::MAX);
        } else {
            overflow.fence.task_revision = DecimalU64(u64::MAX);
        }
        assert!(overflow.validate().is_err());
    }
    let mut bind = request;
    let GameExecutorOperationV1::Bind {
        delegation_generation,
        ..
    } = &mut bind.operation
    else {
        unreachable!()
    };
    *delegation_generation = DecimalU64(0);
    assert!(bind.validate().is_err());
}
#[test]
fn public_task_context_has_only_commitments_and_complete_active_fence() {
    let mut control = initial();
    control.task_status = GameTaskStatusV1::Active;
    control.executor_epoch = DecimalU64(1);
    control.executor_id = Some("executor".into());
    control.executor_binding = Some(GameExecutorBindingV1 {
        executor_id: "executor".into(),
        executor_public_key: "ef".repeat(32),
        delegation_generation: DecimalU64(1),
        executor_epoch: DecimalU64(1),
    });
    let context = TaskContextIdentityV1 {
        authority_mode: None,
        native_authorization_root: None,
        schema_revision: 1,
        fence: GameTaskFenceV1::from_control(&control),
        agent_session_id: "session".into(),
        agent_turn_id: "turn".into(),
        decision_request_id: "decision".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_commitment: format!("blake3:{}", "11".repeat(32)),
        context_commitment: format!("blake3:{}", "22".repeat(32)),
        provider_invocation_commitment: format!("blake3:{}", "33".repeat(32)),
        valid_until_tick: DecimalU64(50),
    };
    context.validate().unwrap();
    assert!(
        serde_json::to_value(&context)
            .unwrap()
            .get("authority_mode")
            .is_none()
    );
    let mut native = context.clone();
    native.authority_mode = Some(TaskContextAuthorityModeV1::NativeExecutorV1);
    native.native_authorization_root = Some(format!("blake3:{}", "44".repeat(32)));
    native.schema_revision = 2;
    native.validate().unwrap();
    native.schema_revision = 1;
    assert!(native.validate().is_err());

    context.validate_capture_deadline(34).unwrap();
    assert!(context.validate_capture_deadline(33).is_err());
    assert!(context.validate_capture_deadline(51).is_err());
    assert!(context.validate_capture_deadline(u64::MAX).is_err());

    let value = serde_json::to_value(&context).unwrap();
    for field in [
        "body",
        "goal_digest",
        "goal_snapshot",
        "base_decision_request",
        "reference_salt",
    ] {
        let mut invalid = value.clone();
        invalid[field] = json!("private");
        assert!(serde_json::from_value::<TaskContextIdentityV1>(invalid).is_err());
    }
    let mut stale = context.clone();
    stale.fence.executor_epoch = DecimalU64(2);
    assert!(stale.validate().is_err());
    let mut unclosable = context.clone();
    unclosable.transport_attempt = u64::from(u16::MAX) + 1;
    assert!(unclosable.validate().is_err());
    let mut raw = context;
    raw.context_commitment = "22".repeat(32);
    assert!(raw.validate().is_err());
}
