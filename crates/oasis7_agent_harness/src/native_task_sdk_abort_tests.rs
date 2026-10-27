//! Actual SDK projection of a service-authenticated, no-effect terminal outcome.
use super::*;
use oasis7_client_api::world_service::{self as authority, *};
use oasis7_client_api::world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1};
use std::{
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
};

#[test]
fn actual_sdk_abort_committed_outcome_is_not_a_committed_action() {
    let private = "07".repeat(32);
    let public = authority::sign_read_request("key", (), &private)
        .unwrap()
        .subject_public_key;
    let mut value = serde_json::to_value(super::super::tests::fence()).unwrap();
    value["owner_public_key"] = json!(public);
    value["executor_binding"]["executor_public_key"] = json!(public);
    value["latest_goal_revision"] = json!("1");
    value["active_goal_revision"] = json!("1");
    value["active_goal_reference"] = json!({"goal_revision":"1","goal_ref":"cd".repeat(32),
        "prepare_operation_id":"goal","publish_operation_id":"publish","activation_committed_height":"2"});
    let fence: GameTaskFenceV1 = serde_json::from_value(value).unwrap();
    let context: oasis7_agent_api::game::TaskContextIdentityV1 = serde_json::from_value(json!({
        "schema_revision":1,"fence":fence,"agent_session_id":"session","agent_turn_id":"turn",
        "decision_request_id":"request","retry_seq":1,"transport_attempt":1,
        "request_commitment":format!("blake3:{}","11".repeat(32)),
        "context_commitment":format!("blake3:{}","22".repeat(32)),
        "provider_invocation_commitment":format!("blake3:{}","33".repeat(32)),"valid_until_tick":"16"})).unwrap();
    let base = serde_json::from_value(
        json!({"world_id":"world","branch_id":"main","finality_epoch":0,
        "finality_block_hash":null,"finality_status":"pending","base_tick":0,
        "base_world_hash":format!("blake3:{}","44".repeat(32)),"reorg_epoch":0,
        "runtime_manifest_hash":format!("blake3:{}","55".repeat(32))}),
    )
    .unwrap();
    let payload = WorldServicePayloadV1::Scheduler(
        authority::sign_read_request(
            "scheduler",
            oasis7_client_api::world_service::task_context::public_outcome_intent(
                &context,
                base,
                "executor_aborted",
            )
            .unwrap(),
            &private,
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-sdk-abort-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let signer = root.join("signer");
    std::fs::write(&signer, [7; 32]).unwrap();
    std::fs::set_permissions(&signer, std::fs::Permissions::from_mode(0o600)).unwrap();
    let config = root.join("device.json");
    std::fs::write(&config, serde_json::to_vec(&json!({"endpoint":format!("http://{}",listener.local_addr().unwrap()),
        "world_id":"world","genesis_digest":"genesis","agent_id":"agent",
        "service_public_key":authority::sign_read_request("key",(), &"09".repeat(32)).unwrap().subject_public_key,
        "signer_file":signer,"state_directory":root.join("state")})).unwrap()).unwrap();
    let client = Arc::new(Client::open(&config).unwrap());
    let original = client
        .cache_intent(
            oasis7_game_client::operation_store::ClientStateKind::Outcome,
            "slot",
            payload,
        )
        .unwrap();
    let expected = original.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        let (end, length, path) = loop {
            let n = stream.read(&mut buffer).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(at) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..at]);
                let length: usize = head
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= at + 4 + length {
                    break (
                        at + 4,
                        length,
                        head.split_whitespace().nth(1).unwrap().to_owned(),
                    );
                }
            }
        };
        let digest = if path == SUBMIT_PATH {
            let request: SubmitIntentRequest<WorldServicePayloadV1> =
                serde_json::from_slice(&bytes[end..end + length]).unwrap();
            assert_eq!(request, expected);
            authority::request_digest(&path, &request).unwrap()
        } else {
            assert_eq!(path, LOOKUP_PATH);
            let request: AuthenticatedLookup =
                serde_json::from_slice(&bytes[end..end + length]).unwrap();
            assert_eq!(request.original, expected.signed_payload);
            authority::request_digest(&path, &request).unwrap()
        };
        let response = IntentResponse {
            contract_version: 1,
            formal_profile: None,
            execution_evidence_scope: None,
            correlation: expected.correlation.clone(),
            outcome: IntentOutcome::Committed {
                commit: Box::new(CommitRef {
                    world: expected.correlation.key.world,
                    binding: ExecutionBinding {
                        provider_world_id: "world".into(),
                        branch_id: "main".into(),
                        finality_ref: "finality".into(),
                        reorg_generation: 0,
                        governing_manifest_ref: "manifest".into(),
                        authority_generation: 1,
                        permission_generation: 1,
                    },
                    position: 7,
                    execution_block_hash: "block".into(),
                    state_root_ref: "root".into(),
                }),
                receipt: json!({"terminated":true}),
            },
        };
        let response =
            authority::sign_service_response(&path, digest, response, &"09".repeat(32)).unwrap();
        let encoded = serde_json::to_vec(&response).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            encoded.len()
        )
        .unwrap();
        stream.write_all(&encoded).unwrap();
    });
    let host = SdkNativeTaskHost::new(client.clone());
    *host.run_binding.lock().unwrap() = Some(json!({"task_fence":fence}));
    let context = NativeTaskContext {
        operation: "slot".into(),
        fence,
        valid_until_tick: 16,
        model_context: json!({}),
    };
    let result = host
        .invoke(
            &context,
            &NativeTaskTool::Abort(serde_json::from_value(json!({})).unwrap()),
        )
        .unwrap();
    server.join().unwrap();
    drop(host);
    drop(client);
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(result.effect, Some(NativeTaskRecovery::Aborted));
}
