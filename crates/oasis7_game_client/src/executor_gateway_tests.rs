use super::*;
use oasis7_agent_api::game::{DecimalU64, GameExecutorBindingV1, GameGoalReferenceV1};
use oasis7_client_storage::identity_store::IdentityStore;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    thread,
    time::Duration,
};
const PASSWORD: &[u8] = b"synthetic executor gateway private unlock";
fn historical_prefix(fence: GameTaskFenceV1, private: &str) -> WorldServicePayloadV1 {
    let generation = fence
        .executor_binding
        .as_ref()
        .unwrap()
        .delegation_generation
        .0;
    let request: TaskSchedulerIntentV1 = SchedulerIntentV1 {
        agent_id: fence.agent_id.clone(),
        request_id: "old-a-prefix".into(),
        delegation_generation: generation,
        captured_base_binding: oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1 {
            world_id: fence.world_id.clone(),
            branch_id: fence.branch_id.clone(),
            finality_epoch: 1,
            finality_block_hash: None,
            finality_status: "provisional".into(),
            base_tick: 0,
            base_world_hash: format!("blake3:{}", "a".repeat(64)),
            reorg_epoch: fence.reorg_epoch.0,
            runtime_manifest_hash: format!("blake3:{}", "b".repeat(64)),
        },
        operation: SchedulerOperationV1::TaskProviderPrefix {
            context: oasis7_agent_api::game::TaskContextIdentityV1 {
                schema_revision: 1,
                fence,
                agent_session_id: "old-a-session".into(),
                agent_turn_id: "old-a-turn".into(),
                decision_request_id: "old-a-decision".into(),
                retry_seq: 1,
                transport_attempt: 1,
                request_commitment: format!("blake3:{}", "1".repeat(64)),
                context_commitment: format!("blake3:{}", "2".repeat(64)),
                provider_invocation_commitment: format!("blake3:{}", "3".repeat(64)),
                valid_until_tick: DecimalU64(16),
            },
        },
    };
    WorldServicePayloadV1::Scheduler(ws::sign_read_request("scheduler", request, private).unwrap())
}
#[test]
fn current_delegate_b_can_only_lookup_a_verified_original_from_old_delegate_a_same_resource() {
    let (_root, identity, vault, mut current) = fixture("https://identity.invalid".into());
    current.task_revision = DecimalU64(5);
    current.executor_epoch = DecimalU64(3);
    let current_binding = current.executor_binding.as_mut().unwrap();
    current_binding.executor_epoch = DecimalU64(3);
    current_binding.delegation_generation = DecimalU64(4);
    let context = ExecutorGatewayContext::open(
        identity,
        "login".into(),
        vault,
        PASSWORD,
        current.clone(),
        service_key(),
        65536,
    )
    .unwrap();
    let old_private = hex::encode([96; 32]);
    let old_public = ws::sign_read_request("fixture", (), &old_private)
        .unwrap()
        .subject_public_key;
    assert_ne!(old_public, context.public_key());
    let mut old = current.clone();
    old.task_revision = DecimalU64(3);
    old.branch_id = "old-branch".into();
    old.executor_epoch = DecimalU64(1);
    let old_binding = old.executor_binding.as_mut().unwrap();
    old_binding.executor_epoch = DecimalU64(1);
    old_binding.delegation_generation = DecimalU64(1);
    old_binding.executor_public_key = old_public.clone();
    old_binding.executor_id = "old-executor-a".into();
    let original = historical_prefix(old.clone(), &old_private);
    let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
        context.world(),
        &original,
    )
    .unwrap();
    let lookup = AuthenticatedLookup {
        request: LookupIntentRequest {
            contract_version: 1,
            key: correlation.key.clone(),
        },
        original: original.clone(),
    };
    assert!(context.operation(LOOKUP_PATH, &lookup).is_ok());
    let submit = SubmitIntentRequest {
        contract_version: 1,
        correlation,
        deadline_unix_ms: None,
        signed_payload: original.clone(),
    };
    assert!(
        context.operation(SUBMIT_PATH, &submit).is_err(),
        "current B must never submit old A's original"
    );
    let old_prepare = ws::sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        TaskContextPrepareV1 {
            schema_revision: 1,
            fence: old.clone(),
            client_operation_id: "old-prepare".into(),
            client_nonce: "e".repeat(64),
        },
        &old_private,
    )
    .unwrap();
    assert!(
        context
            .operation(TASK_CONTEXT_PREPARE_PATH, &old_prepare)
            .is_err(),
        "historical lookup cannot relax private-context signer/fence"
    );
    assert!(
        context
            .verify_payload_kind(
                &historical_prefix(old.clone(), &hex::encode([97; 32])),
                true
            )
            .is_err(),
        "original signer must equal original fence executor"
    );
    let mut invalid = original.clone();
    if let WorldServicePayloadV1::Scheduler(s) = &mut invalid {
        s.signature_hex = "0".repeat(128);
    }
    assert!(context.verify_payload_kind(&invalid, true).is_err());
    let mut wrong_generation = old.clone();
    wrong_generation
        .executor_binding
        .as_mut()
        .unwrap()
        .delegation_generation = DecimalU64(2);
    let mut bad = historical_prefix(wrong_generation, &old_private);
    if let WorldServicePayloadV1::Scheduler(s) = &mut bad {
        s.request.delegation_generation = 1;
        *s = ws::sign_read_request("scheduler", s.request.clone(), &old_private).unwrap();
    }
    assert!(
        context.verify_payload_kind(&bad, true).is_err(),
        "payload generation must match its original binding"
    );
    for field in ["task", "agent", "genesis", "owner_generation"] {
        let mut other = old.clone();
        match field {
            "task" => other.task_id = "different-task".into(),
            "agent" => other.agent_id = "different-agent".into(),
            "genesis" => other.genesis_digest = "different-genesis".into(),
            _ => other.owner_generation.0 += 1,
        };
        assert!(
            context
                .verify_payload_kind(&historical_prefix(other, &old_private), true)
                .is_err(),
            "cross-resource {field}"
        );
    }
    let owner_private = hex::encode([91; 32]);
    let mut owner = old;
    owner.executor_binding.as_mut().unwrap().executor_public_key = owner.owner_public_key.clone();
    assert!(
        context
            .verify_payload_kind(&historical_prefix(owner, &owner_private), true)
            .is_err(),
        "owner must never count as a historical executor"
    );
}
#[test]
fn only_fixed_read_drift_categories_cross_gateway_transport() {
    for (path, code) in [
        (TASK_RECIPE_QUOTE_PATH, TASK_QUOTE_BASE_DRIFT),
        (TASK_FACTORY_QUOTE_PATH, TASK_QUOTE_DELIVERY_DRIFT),
        (TASK_CONTEXT_READ_PATH, TASK_CONTEXT_READ_DELIVERY_DRIFT),
    ] {
        let error = json!({"status":409,"code":code});
        assert_eq!(fixed_read_drift(path, &error), Some(code));
        assert_eq!(fixed_read_drift(SUBMIT_PATH, &error), None);
        let mut private = error.clone();
        private["private_body"] = json!("never surface");
        assert_eq!(fixed_read_drift(path, &private), None);
        let mut wrong_status = error;
        wrong_status["status"] = json!(403);
        assert_eq!(fixed_read_drift(path, &wrong_status), None);
    }
    assert_eq!(
        fixed_read_drift(
            TASK_CONTEXT_READ_PATH,
            &json!({"status":409,"code":TASK_QUOTE_BASE_DRIFT})
        ),
        None
    );
    assert_eq!(
        fixed_read_drift(
            TASK_FACTORY_QUOTE_PATH,
            &json!({"status":409,"code":"untrusted response text"})
        ),
        None
    );
}
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture(endpoint: String) -> (Directory, IdentityClientConfig, PathBuf, GameTaskFenceV1) {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).unwrap();
    let root = Directory(
        std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("executor-gateway-{}", hex::encode(random))),
    );
    std::fs::create_dir(&root.0).unwrap();
    let identity = IdentityClientConfig {
        endpoint,
        identity_service_id: "identity".into(),
        environment_id: "private".into(),
        audience: "https://identity.invalid".into(),
        state_directory: root.0.join("identity"),
        test_loopback: true,
    };
    IdentityClient::initialize(&identity, PASSWORD).unwrap();
    let store = IdentityStore::open(&identity.state_directory, PASSWORD).unwrap();
    let scope = blake3::hash(
        serde_json::to_string(&(
            &identity.endpoint,
            &identity.identity_service_id,
            &identity.environment_id,
            &identity.audience,
        ))
        .unwrap()
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    store
        .put_original(
            &scope,
            "login",
            "login-session",
            &json!({"account_id":"account","cookie":"__Host-oasis7_sid=synthetic"}),
        )
        .unwrap();
    let vault = root.0.join("executor");
    let executor = ExecutorControlStore::create(&vault, PASSWORD).unwrap();
    let public = hex::encode(
        SigningKey::from_bytes(executor.executor_seed())
            .verifying_key()
            .to_bytes(),
    );
    let fence = GameTaskFenceV1 {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        agent_id: "a".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
        owner_public_key: service_key(),
        task_id: "task-a".into(),
        task_revision: DecimalU64(2),
        task_status: GameTaskStatusV1::Active,
        latest_goal_revision: DecimalU64(1),
        active_goal_revision: DecimalU64(1),
        active_goal_reference: Some(GameGoalReferenceV1 {
            goal_revision: DecimalU64(1),
            goal_ref: "a".repeat(64),
            prepare_operation_id: "prepare".into(),
            publish_operation_id: "publish".into(),
            activation_committed_height: DecimalU64(1),
        }),
        executor_epoch: DecimalU64(1),
        executor_binding: Some(GameExecutorBindingV1 {
            executor_id: "executor".into(),
            executor_public_key: public,
            delegation_generation: DecimalU64(1),
            executor_epoch: DecimalU64(1),
        }),
    };
    (root, identity, vault, fence)
}
fn service_key() -> String {
    hex::encode(SigningKey::from_bytes(&[91; 32]).verifying_key().to_bytes())
}
fn transport_config(context: &ExecutorGatewayContext) -> WorldServiceClientConfig {
    WorldServiceClientConfig {
        endpoint: context.identity_origin.clone(),
        trusted_service_public_key: service_key(),
        expected_world: context.world(),
        scope_id: "agent:a".into(),
        read_private_key_hex: String::new(),
        timeout: Duration::from_secs(3),
        max_response_bytes: 65536,
    }
}
fn read_http(socket: &mut std::net::TcpStream) -> (String, Value) {
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let n = socket.read(&mut chunk).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() <= 65536);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length: usize = header
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .map(|v| v.trim().parse().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return (
                    header
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .into(),
                    if length == 0 {
                        Value::Null
                    } else {
                        oasis7_agent_api::game::parse_request(&bytes[end + 4..end + 4 + length])
                            .unwrap()
                    },
                );
            }
        }
    }
}
#[test]
fn native_client_uses_real_gateway_without_exposing_seed_and_rejects_wrong_domain_reply() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (root, identity, vault, fence) =
        fixture(format!("http://{}", listener.local_addr().unwrap()));
    let client = crate::device_client::Client::from_authenticated_config(
        crate::device_client::ExecutorDeviceConfig {
            identity,
            login_operation: "login".into(),
            executor_vault_directory: vault,
            service_public_key: service_key(),
            fence: fence.clone(),
            state_directory: root.0.join("operation-state"),
        },
        PASSWORD,
    )
    .unwrap();
    assert!(client.key_hex.is_empty());
    assert!(client.remote.config().read_private_key_hex.is_empty());
    let expected_fence = fence.clone();
    let server = thread::spawn(move || {
        let mut original = None;
        let mut nonces = std::collections::BTreeSet::new();
        let mut calls = 0;
        for _ in 0..6 {
            let (mut socket, _) = listener.accept().unwrap();
            let (path, body) = read_http(&mut socket);
            let reply = if path.ends_with("/session") {
                json!({"api_version":2,"ok":true,"account":{"account_id":"account"}})
            } else if path.ends_with("/capabilities") {
                json!({"api_version":2,"ok":true,"csrf_token":"c".repeat(64)})
            } else {
                assert!(path.ends_with("/entry-executor/control"));
                let outer: EntryExecutorControlRequestV1 = serde_json::from_value(body).unwrap();
                assert_eq!(outer.access.fence, expected_fence);
                ws::verify_read_request(VIEW_JSON_PATH, &outer.access.read).unwrap();
                assert!(nonces.insert(outer.access.read.request.observation_nonce.unwrap()));
                let EntryExecutorOperationV1::Describe(inner) = outer.operation else {
                    panic!("unexpected gateway operation")
                };
                ws::verify_read_request(DESCRIBE_PATH, &inner).unwrap();
                if let Some(expected) = &original {
                    assert_eq!(expected, &inner);
                } else {
                    original = Some(inner.clone());
                }
                calls += 1;
                let signed = ws::sign_service_response(
                    if calls == 1 {
                        DESCRIBE_PATH
                    } else {
                        LOOKUP_PATH
                    },
                    ws::request_digest(DESCRIBE_PATH, &inner).unwrap(),
                    DescribeWorldResponse {
                        formal_profile: None,
                        execution_evidence_scope: None,
                        contract_version: 1,
                        world: WorldIdentity {
                            world_id: "world".into(),
                            genesis_digest: "genesis".into(),
                        },
                        binding: ExecutionBinding {
                            provider_world_id: "world".into(),
                            branch_id: "main".into(),
                            finality_ref: "fixture".into(),
                            reorg_generation: 0,
                            governing_manifest_ref: "manifest".into(),
                            authority_generation: 1,
                            permission_generation: 1,
                        },
                        capabilities: Vec::new(),
                        availability: WorldAvailability {
                            readable: true,
                            writable: true,
                            recovering: false,
                            reason: None,
                            retry_after_ms: None,
                        },
                        current_view: None,
                    },
                    &hex::encode([91; 32]),
                )
                .unwrap();
                json!({"api_version":2,"ok":true,"world_response":signed})
            };
            let bytes = serde_json::to_vec(&reply).unwrap();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            socket.write_all(&bytes).unwrap();
        }
        assert_eq!(calls, 2);
    });
    let first = client.remote.describe().unwrap();
    assert_eq!(first.world, client.world);
    assert!(matches!(client.remote.describe(), Err(Error::Assurance(_))));
    server.join().unwrap();
}
#[test]
fn vault_is_resource_bound_and_owner_routes_and_other_task_fences_fail_before_network() {
    let (_root, identity, vault, fence) = fixture("https://identity.invalid".into());
    let context = ExecutorGatewayContext::open(
        identity.clone(),
        "login".into(),
        vault.clone(),
        PASSWORD,
        fence.clone(),
        service_key(),
        65536,
    )
    .unwrap();
    assert!(context.sign_read("game_goal_publish", json!({})).is_err());
    assert!(
        context
            .operation("/v1/world/private-goal/prepare", &json!({}))
            .is_err()
    );
    let mut other = fence.clone();
    other.agent_id = "delegated-b".into();
    other.task_id = "task-b".into();
    let proof = ws::sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        TaskContextPrepareV1 {
            schema_revision: 1,
            fence: other.clone(),
            client_operation_id: "prepare".into(),
            client_nonce: "a".repeat(64),
        },
        &context.private_key(),
    )
    .unwrap();
    assert!(
        context
            .operation(TASK_CONTEXT_PREPARE_PATH, &proof)
            .is_err()
    );
    assert!(
        ExecutorGatewayContext::open(
            identity,
            "login".into(),
            vault,
            PASSWORD,
            other,
            service_key(),
            65536
        )
        .is_err()
    );
    let mut exposed = transport_config(&context);
    exposed.read_private_key_hex = "01".repeat(32);
    assert!(crate::WorldServiceTransport::new_authenticated(exposed, Arc::new(context)).is_err());
}

#[test]
fn authenticated_device_retains_exact_private_original_after_restart_and_rejects_new_cache_directory()
 {
    let (root, identity, vault, fence) = fixture("https://identity.invalid".into());
    let config = || crate::device_client::ExecutorDeviceConfig {
        identity: identity.clone(),
        login_operation: "login".into(),
        executor_vault_directory: vault.clone(),
        service_public_key: service_key(),
        fence: fence.clone(),
        state_directory: root.0.join("operation-state"),
    };
    let client =
        crate::device_client::Client::from_authenticated_config(config(), PASSWORD).unwrap();
    assert!(client.key_hex.is_empty());
    assert!(client.sign("game_goal_publish", json!({})).is_err());
    let proof = client
        .sign(
            TASK_CONTEXT_PREPARE_PATH,
            TaskContextPrepareV1 {
                schema_revision: 1,
                fence: fence.clone(),
                client_operation_id: "original-prepare".into(),
                client_nonce: "b".repeat(64),
            },
        )
        .unwrap();
    use crate::operation_store::{ClientStateKind, ClientStateV1};
    client
        .state
        .put_client_state(
            ClientStateKind::Prepare,
            "original-prepare",
            &ClientStateV1::ContextPrepare(Box::new(proof.clone())),
        )
        .unwrap();
    let before = serde_json::to_value(&proof).unwrap();
    drop(client);
    let restarted =
        crate::device_client::Client::from_authenticated_config(config(), PASSWORD).unwrap();
    let recovered = restarted.original_prepare("original-prepare").unwrap();
    assert_eq!(before, serde_json::to_value(&recovered).unwrap());
    let mut moved = config();
    moved.state_directory = root.0.join("other-operation-state");
    assert!(crate::device_client::Client::from_authenticated_config(moved, PASSWORD).is_err());
}

#[test]
fn private_gateway_verifies_original_typed_cbor_not_normalized_json_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (_root, identity, vault, fence) =
        fixture(format!("http://{}", listener.local_addr().unwrap()));
    let context = Arc::new(
        ExecutorGatewayContext::open(
            identity,
            "login".into(),
            vault,
            PASSWORD,
            fence.clone(),
            service_key(),
            65536,
        )
        .unwrap(),
    );
    let proof = context
        .sign_native_request(
            TASK_CONTEXT_PREPARE_PATH,
            TaskContextPrepareV1 {
                schema_revision: 1,
                fence,
                client_operation_id: "typed-prepare".into(),
                client_nonce: "c".repeat(64),
            },
        )
        .unwrap();
    let expected = serde_json::to_value(&proof).unwrap();
    let transport =
        crate::WorldServiceTransport::new_authenticated(transport_config(&context), context)
            .unwrap();
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().unwrap();
            let (path, body) = read_http(&mut socket);
            let reply = if path.ends_with("/session") {
                json!({"api_version":2,"ok":true,"account":{"account_id":"account"}})
            } else if path.ends_with("/capabilities") {
                json!({"api_version":2,"ok":true,"csrf_token":"c".repeat(64)})
            } else {
                let outer: EntryExecutorControlRequestV1 = serde_json::from_value(body).unwrap();
                let EntryExecutorOperationV1::PrepareContext(original) = outer.operation else {
                    panic!("wrong private route")
                };
                assert_eq!(serde_json::to_value(&original).unwrap(), expected);
                // Integer map keys have a different canonical CBOR preimage
                // from JSON object strings. The transport must retain R.
                let payload = std::collections::BTreeMap::from([(7u64, "typed".to_string())]);
                let signed = ws::sign_service_response(
                    TASK_CONTEXT_PREPARE_PATH,
                    ws::request_digest(TASK_CONTEXT_PREPARE_PATH, &original).unwrap(),
                    payload,
                    &hex::encode([91; 32]),
                )
                .unwrap();
                let normalized: SignedServiceResponse<Value> =
                    serde_json::from_value(serde_json::to_value(&signed).unwrap()).unwrap();
                assert!(
                    ws::verify_service_response(
                        TASK_CONTEXT_PREPARE_PATH,
                        &signed.request_digest,
                        &normalized,
                        &service_key()
                    )
                    .is_err()
                );
                json!({"api_version":2,"ok":true,"world_response":signed})
            };
            let bytes = serde_json::to_vec(&reply).unwrap();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            socket.write_all(&bytes).unwrap();
        }
    });
    let response: SignedServiceResponse<std::collections::BTreeMap<u64, String>> = transport
        .private_signed_response(TASK_CONTEXT_PREPARE_PATH, &proof, 65536)
        .unwrap();
    assert_eq!(response.payload.get(&7).map(String::as_str), Some("typed"));
    server.join().unwrap();
}
