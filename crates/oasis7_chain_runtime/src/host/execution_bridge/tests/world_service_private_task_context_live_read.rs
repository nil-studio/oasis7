//! Real World HTTP reads with the unchanged authorized original GET.
use super::*;
use oasis7_client_api::world_service::task_context::{
    TASK_CONTEXT_LIVE_READ_PATH, TaskContextLiveReadV1, verify_live_context_read,
    verify_live_context_reply,
};

fn read(
    f: &ActiveContextReadFixture,
    request: &TaskContextLiveReadV1,
) -> SignedServiceResponse<CapturedTaskContextV1> {
    let (status, body) = http_with_hook(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        TASK_CONTEXT_LIVE_READ_PATH,
        request,
        "",
        || {},
    );
    assert_eq!(status, 200);
    let response = serde_json::from_value(body).unwrap();
    verify_live_context_reply(request, &response, &f.signer.public_key_hex).unwrap();
    response
}

#[test]
fn live_context_read_binds_nonce_domain_exact_get_and_reloads_world() {
    let mut f = active_context_read_fixture(true);
    let original_bytes = serde_json::to_vec(&f.get).unwrap();
    let a = TaskContextLiveReadV1 {
        original: f.get.clone(),
        observation_nonce: "a1".repeat(32),
    };
    verify_live_context_read(&a).unwrap();
    let first = read(&f, &a);
    let b = TaskContextLiveReadV1 {
        observation_nonce: "b2".repeat(32),
        ..a.clone()
    };
    assert!(verify_live_context_reply(&b, &first, &f.signer.public_key_hex).is_err());

    let (status, old) = http_with_hook(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        TASK_CONTEXT_READ_PATH,
        &f.get,
        "",
        || {},
    );
    assert_eq!(status, 200);
    let old: SignedServiceResponse<CapturedTaskContextV1> = serde_json::from_value(old).unwrap();
    verify_private_reply(
        TASK_CONTEXT_READ_PATH,
        &f.get,
        &old,
        &f.signer.public_key_hex,
    )
    .unwrap();
    old.payload.validate_against(&f.get).unwrap();
    assert!(verify_live_context_reply(&a, &old, &f.signer.public_key_hex).is_err());

    let mut different = a.clone();
    let mut get = f.get.request.clone();
    get.prepare_operation_id.push_str("-different");
    different.original =
        sign_read_request(TASK_CONTEXT_READ_PATH, get, &hex::encode([73; 32])).unwrap();
    verify_live_context_read(&different).unwrap();
    assert!(verify_live_context_reply(&different, &first, &f.signer.public_key_hex).is_err());

    let height = first.payload.current_public_context.read_commit.position + 1;
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
    let second = read(&f, &b);
    assert!(
        second.payload.current_public_context.read_commit.position
            > first.payload.current_public_context.read_commit.position
    );
    assert_eq!(
        second.payload.prepared.identity,
        first.payload.prepared.identity
    );
    assert_eq!(serde_json::to_vec(&f.get).unwrap(), original_bytes);
    // Repeating a nonce is not a World-global once claim; caller accounting owns that boundary.
    read(&f, &b);

    for nonce in ["".into(), "aa".repeat(31), "AA".repeat(32), "gg".repeat(32)] {
        let bad = TaskContextLiveReadV1 {
            observation_nonce: nonce,
            ..a.clone()
        };
        assert!(verify_live_context_read(&bad).is_err());
        let (status, body) = http_with_hook(
            &f.dir,
            &f.runtime,
            &f.signer,
            Some(&f.store),
            TASK_CONTEXT_LIVE_READ_PATH,
            &bad,
            "",
            || panic!("invalid nonce reached delivery"),
        );
        assert_eq!(status, 400);
        assert!(body.get("payload").is_none());
    }
    let mut unknown = serde_json::to_value(&a).unwrap();
    unknown["extra"] = Value::Bool(true);
    assert!(serde_json::from_value::<TaskContextLiveReadV1>(unknown).is_err());
}

#[test]
fn live_context_read_preserves_delivery_drift_and_stop_fence() {
    let mut f = active_context_read_fixture(true);
    let request = TaskContextLiveReadV1 {
        original: f.get.clone(),
        observation_nonce: "c3".repeat(32),
    };
    let first = read(&f, &request);
    let height = first.payload.current_public_context.read_commit.position + 1;
    let (status, body) = http_with_hook(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        TASK_CONTEXT_LIVE_READ_PATH,
        &request,
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
    let fresh = read(&f, &request);
    let stop = super::request(
        &f.world,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "live-read-stop".into(),
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
        TASK_CONTEXT_LIVE_READ_PATH,
        &request,
        "",
        || {
            commit(
                &mut f.driver,
                fresh.payload.current_public_context.read_commit.position + 1,
                stop,
            )
        },
    );
    assert_eq!(status, 403);
    assert!(body.get("payload").is_none());
}
