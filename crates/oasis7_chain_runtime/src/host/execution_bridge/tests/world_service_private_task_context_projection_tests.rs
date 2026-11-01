//! Actual World HTTP producer tests, not a Native/Account/MFA/paid graph proof.
use super::*;
use oasis7_client_api::world_service::task_context_projection::*;

fn call<Q: serde::Serialize, T: serde::de::DeserializeOwned>(
    f: &ActiveContextReadFixture,
    path: &str,
    request: &Q,
) -> T {
    let (status, value) = http_with_hook(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        path,
        request,
        "",
        || {},
    );
    assert_eq!(status, 200);
    serde_json::from_value(value).unwrap()
}
struct PublicChain {
    prepare_request: TaskContextPrepareAttestationRequestV2,
    prepared: WorldContextAttestationV2<TaskContextPreparedDeliveryV2>,
    get_request: TaskContextGetAttestationRequestV2,
    captured: WorldContextAttestationV2<TaskContextCapturedDeliveryV2>,
    move_request: TaskContextMoveProjectionRequestV2,
}
fn chain(f: &ActiveContextReadFixture) -> PublicChain {
    let record = f.store.get_task_context(&f.get).unwrap();
    verify_prepare(&record.prepare_proof).unwrap();
    record.validate_against(&record.prepare_proof).unwrap();
    let prepare_request = TaskContextPrepareAttestationRequestV2 {
        original: record.prepare_proof,
        observation_nonce: "11".repeat(32),
    };
    // Genuine old World/private/SDK success and the complete64KiB bound are
    // explicit prerequisites before the new capability's200 target assertion.
    let old: SignedServiceResponse<PreparedTaskContextV1> =
        call(f, TASK_CONTEXT_PREPARE_PATH, &prepare_request.original);
    verify_private_reply(
        TASK_CONTEXT_PREPARE_PATH,
        &prepare_request.original,
        &old,
        &f.signer.public_key_hex,
    )
    .unwrap();
    old.payload
        .validate_against(&prepare_request.original)
        .unwrap();
    ContextBodyCommitmentV2::from_signed_body(&old).unwrap();
    let prepared: WorldContextAttestationV2<TaskContextPreparedDeliveryV2> =
        call(f, CONTEXT_PREPARE_ATTESTED_PATH_V2, &prepare_request);
    verify_prepared_delivery_v2(&prepare_request, &prepared, &f.signer.public_key_hex).unwrap();
    let get_request = TaskContextGetAttestationRequestV2 {
        original: f.get.clone(),
        observation_nonce: "22".repeat(32),
        prepared_projection: prepared.payload.projection.clone(),
    };
    let captured: WorldContextAttestationV2<TaskContextCapturedDeliveryV2> =
        call(f, CONTEXT_READ_ATTESTED_PATH_V2, &get_request);
    verify_captured_delivery_v2(&get_request, &captured, &f.signer.public_key_hex).unwrap();
    let move_request = TaskContextMoveProjectionRequestV2 {
        original: f.get.clone(),
        observation_nonce: "33".repeat(32),
        prepared_projection: prepared.payload.projection.clone(),
        captured_projection: captured.payload.projection.clone(),
    };
    PublicChain {
        prepare_request,
        prepared,
        get_request,
        captured,
        move_request,
    }
}
fn private_key_absent(value: &serde_json::Value) -> bool {
    const FORBIDDEN: &[&str] = &[
        "private_body",
        "active_goal",
        "goal_text",
        "soft_constraints",
        "reference_salt",
        "actor_observation",
    ];
    match value {
        serde_json::Value::Object(fields) => fields
            .iter()
            .all(|(key, value)| !FORBIDDEN.contains(&key.as_str()) && private_key_absent(value)),
        serde_json::Value::Array(values) => values.iter().all(private_key_absent),
        _ => true,
    }
}
#[test]
fn context_public_projection_v2_real_world_body_split_and_exact_sdk_move() {
    let f = active_context_read_fixture(true);
    let c = chain(&f);
    // The private old route remains unchanged. Only this test compares both
    // explicit HTTP calls; production does not fetch a second projection.
    let old_prepared: SignedServiceResponse<PreparedTaskContextV1> =
        call(&f, TASK_CONTEXT_PREPARE_PATH, &c.prepare_request.original);
    assert!(
        serde_json::to_value(&old_prepared).unwrap()
            == serde_json::to_value(&c.prepared.payload.private_body).unwrap()
    );
    let old_captured: SignedServiceResponse<CapturedTaskContextV1> =
        call(&f, TASK_CONTEXT_READ_PATH, &c.get_request.original);
    assert!(
        serde_json::to_value(&old_captured).unwrap()
            == serde_json::to_value(&c.captured.payload.private_body).unwrap()
    );
    let prepared = verify_prepared_projection_v2(
        &c.prepare_request,
        &c.prepared.payload.projection,
        &f.signer.public_key_hex,
    )
    .unwrap();
    assert!(
        serde_json::to_value(prepared.prefix_candidate().unwrap()).unwrap()
            == serde_json::to_value(
                public_prefix::<
                    oasis7_client_api::world_service_wire::UnsupportedWireVariant,
                    oasis7_client_api::world_service_wire::UnsupportedWireVariant,
                    oasis7_client_api::world_service_wire::UnsupportedWireVariant,
                    oasis7_client_api::world_service_wire::UnsupportedWireVariant,
                    oasis7_client_api::world_service_wire::UnsupportedWireVariant,
                    oasis7_client_api::world_service_wire::UnsupportedWireVariant,
                    oasis7_client_api::world_service_wire::UnsupportedWireVariant,
                >(&old_prepared.payload)
                .unwrap()
            )
            .unwrap()
    );
    assert!(prepared.get_candidate() == f.get.request);
    let move_projection: WorldContextAttestationV2<TaskContextMoveProjectionV2> =
        call(&f, CONTEXT_MOVE_PROJECTION_PATH_V2, &c.move_request);
    let verified =
        verify_move_projection_v2(&c.move_request, &move_projection, &f.signer.public_key_hex)
            .unwrap();
    let candidate = verified.move_candidate([100, 200, 0]).unwrap();
    let expected = cognition_intent(
        &old_captured.payload,
        oasis7_agent_api::world_action::Action::MoveAgent {
            agent_id: f.fence.agent_id.clone(),
            to: oasis7_agent_api::world_action::GeoPos::new(100, 200, 0),
        },
    )
    .unwrap();
    assert!(serde_json::to_value(candidate).unwrap() == serde_json::to_value(expected).unwrap());
    for public in [
        serde_json::to_value(&c.prepared.payload.projection).unwrap(),
        serde_json::to_value(&c.captured.payload.projection).unwrap(),
        serde_json::to_value(&move_projection).unwrap(),
    ] {
        assert!(private_key_absent(&public));
    }
    drop(f.driver);
    fs::remove_dir_all(f.dir).unwrap();
}
#[test]
fn context_public_projection_v2_rejects_inner_body_tamper_and_unknown_public_fields() {
    let f = active_context_read_fixture(true);
    let c = chain(&f);
    let mut bad = c.prepared.payload.clone();
    bad.private_body.signature_hex.replace_range(
        0..2,
        if bad.private_body.signature_hex.starts_with("00") {
            "01"
        } else {
            "00"
        },
    );
    // Real valid outer World signature is a prerequisite: the rejection must
    // be the inner old proof/body commitment, not a broken outer signature.
    let bad = sign_service_response(
        CONTEXT_PREPARE_ATTESTED_PATH_V2,
        request_digest(CONTEXT_PREPARE_ATTESTED_PATH_V2, &c.prepare_request).unwrap(),
        bad,
        &f.signer.private_key_hex,
    )
    .unwrap();
    verify_private_reply(
        CONTEXT_PREPARE_ATTESTED_PATH_V2,
        &c.prepare_request,
        &bad,
        &f.signer.public_key_hex,
    )
    .unwrap();
    let bad = serde_json::from_value(serde_json::to_value(bad).unwrap()).unwrap();
    assert!(
        verify_prepared_delivery_v2(&c.prepare_request, &bad, &f.signer.public_key_hex).is_err()
    );
    let mut unknown = serde_json::to_value(&c.captured.payload.projection).unwrap();
    unknown["verified"] = serde_json::Value::Bool(true);
    assert!(
        serde_json::from_value::<WorldContextAttestationV2<TaskContextCapturedProjectionV2>>(
            unknown
        )
        .is_err()
    );
    let projection: WorldContextAttestationV2<TaskContextMoveProjectionV2> =
        call(&f, CONTEXT_MOVE_PROJECTION_PATH_V2, &c.move_request);
    let mut other = c.move_request.clone();
    other.observation_nonce = "44".repeat(32);
    assert!(verify_move_projection_v2(&other, &projection, &f.signer.public_key_hex).is_err());
    let before = serde_json::to_value(f.driver.execution_world.snapshot()).unwrap();
    let mut bad_request = c.move_request;
    bad_request.observation_nonce = "FF".repeat(32);
    let (status, body) = http_with_hook(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        CONTEXT_MOVE_PROJECTION_PATH_V2,
        &bad_request,
        "",
        || panic!("invalid projection nonce reached delivery"),
    );
    assert_eq!(status, 400);
    assert!(body.get("payload").is_none());
    assert!(serde_json::to_value(f.driver.execution_world.snapshot()).unwrap() == before);
    drop(f.driver);
    fs::remove_dir_all(f.dir).unwrap();
}

#[test]
fn context_public_projection_v2_keeps_actual_delivery_drift_and_stop_gate() {
    let mut f = active_context_read_fixture(true);
    let c = chain(&f);
    let height = c.captured.payload.projection.payload.read_commit.position + 1;
    let (status, body) = http_with_hook(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        CONTEXT_MOVE_PROJECTION_PATH_V2,
        &c.move_request,
        "",
        || {
            f.driver
                .on_commit(NodeExecutionCommitContext {
                    world_id: "w1".into(),
                    node_id: "node-a".into(),
                    proposer_id: "node-a".into(),
                    height,
                    slot: height,
                    epoch: 0,
                    node_block_hash: format!("node-h{height}"),
                    action_root: compute_consensus_action_root(&[]).unwrap(),
                    committed_actions: vec![],
                    committed_at_unix_ms: height as i64 * 1000,
                })
                .unwrap();
        },
    );
    assert_eq!(status, 409);
    assert_eq!(body["error"], TASK_CONTEXT_READ_DELIVERY_DRIFT);
    assert!(body.get("payload").is_none());
    let next = TaskContextMoveProjectionRequestV2 {
        observation_nonce: "55".repeat(32),
        ..c.move_request
    };
    let fresh: WorldContextAttestationV2<TaskContextMoveProjectionV2> =
        call(&f, CONTEXT_MOVE_PROJECTION_PATH_V2, &next);
    verify_move_projection_v2(&next, &fresh, &f.signer.public_key_hex).unwrap();
    assert!(
        fresh.payload.read_commit.position
            > c.captured.payload.projection.payload.read_commit.position
    );
    assert!(fresh.payload.context == c.captured.payload.projection.payload.context);
    let stop = super::request(
        &f.world,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "public-projection-stop".into(),
                    fence: f.fence.clone(),
                    operation: GameExecutorOperationV1::Stop,
                },
                &f.owner,
            )
            .unwrap(),
        ),
    );
    let (status, body) = http_with_hook(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        CONTEXT_MOVE_PROJECTION_PATH_V2,
        &next,
        "",
        || {
            commit(&mut f.driver, fresh.payload.read_commit.position + 1, stop);
        },
    );
    assert_eq!(status, 403);
    assert!(body.get("payload").is_none());
    drop(f.driver);
    fs::remove_dir_all(f.dir).unwrap();
}

#[path = "world_service_private_task_context_large_reply_tests.rs"]
mod large_reply;
