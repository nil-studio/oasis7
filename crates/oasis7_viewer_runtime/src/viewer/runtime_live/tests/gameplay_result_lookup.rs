use super::*;
use crate::viewer::GameplayActionRequest;
use crate::world_service::{AuthenticatedLookup, LOOKUP_PATH, WorldServicePayloadV1, authority};
use oasis7_client_api::world_service::*;

fn fixture() -> (
    ViewerRuntimeLiveServer,
    RuntimeLiveSession,
    GameplayActionRequest,
    TcpListener,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .unwrap();
    let key = ed25519_dalek::SigningKey::from_bytes(&[5; 32]);
    let public = hex::encode(key.verifying_key().to_bytes());
    let service_key = ed25519_dalek::SigningKey::from_bytes(&[9; 32]);
    server.config.world_service = Some(crate::world_service::client::WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint: format!("http://{}", listener.local_addr().unwrap()),
        trusted_service_public_key: hex::encode(service_key.verifying_key().to_bytes()),
        expected_world: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        scope_id: "public".into(),
        read_private_key_hex: hex::encode([7; 32]),
        timeout: Duration::from_secs(2),
        max_response_bytes: 2 * 1024 * 1024,
    });
    server.config.world_id = "world".into();
    let identity = server
        .config
        .world_service
        .as_ref()
        .unwrap()
        .expected_world
        .clone();
    let commit = CommitRef {
        world: identity.clone(),
        binding: ExecutionBinding {
            provider_world_id: "provider".into(),
            branch_id: "branch".into(),
            finality_ref: "finality".into(),
            reorg_generation: 0,
            governing_manifest_ref: "manifest".into(),
            authority_generation: 1,
            permission_generation: 1,
        },
        position: 10,
        execution_block_hash: "block".into(),
        state_root_ref: "root".into(),
    };
    let request = ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: identity,
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let signed_request = authority::sign_read_request(
        crate::world_service::VIEW_PATH,
        request,
        &hex::encode([7; 32]),
    )
    .unwrap();
    let signed_response = authority::sign_service_response(
        crate::world_service::VIEW_PATH,
        authority::request_digest(crate::world_service::VIEW_PATH, &signed_request).unwrap(),
        ReadWorldViewResponse {
            formal_profile: None,
            formal_proof: None,
            execution_evidence_scope: None,
            contract_version: 1,
            version: ProjectionVersion {
                commit: commit.clone(),
                projection_revision: "projection".into(),
                visibility_scope: "public".into(),
            },
            logical_tick: server.world.state().time,
            continuation: EventCursor {
                stream_id: "stream".into(),
                scope_id: "public".into(),
                era: 0,
                sequence: 0,
                commit,
            },
            view: {
                let mut projection =
                    crate::world_service::projection::from_world(&server.world, None).unwrap();
                projection.runtime_binding = None;
                projection
            },
        },
        &hex::encode([9; 32]),
    )
    .unwrap();
    server.verified_world_view = Some(
        crate::world_service::client::RemoteWorldServiceClient::new(
            server.config.world_service.as_ref().unwrap().clone(),
        )
        .unwrap()
        .accept_view_response(signed_response, signed_request)
        .unwrap(),
    );
    let mut session = RuntimeLiveSession::new_with_playing(false);
    session.negotiated_protocol.version = 2;
    session
        .negotiated_protocol
        .capabilities
        .push("gameplay_result_lookup_v1".into());
    let registration = signed_session_register_request(
        AuthoritativeSessionRegisterRequest {
            player_id: "player".into(),
            public_key: None,
            registration_grant: None,
            auth: None,
            requested_agent_id: None,
            force_rebind: false,
        },
        1,
        &public,
        &hex::encode([5; 32]),
    );
    let mut output = Vec::new();
    server
        .handle_request(
            ViewerRequest::AuthoritativeRecovery {
                command: AuthoritativeRecoveryCommand::RegisterSession {
                    request: registration,
                },
            },
            &mut session,
            &mut output,
        )
        .unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("session_registered"), "{output}");
    let original = signed_gameplay_action_request(
        GameplayActionRequest {
            action_id: "claim_first_agent".into(),
            target_agent_id: "starter-agent-0".into(),
            actor_agent_id: None,
            player_id: "player".into(),
            public_key: None,
            auth: None,
        },
        19,
        &public,
        &hex::encode([5; 32]),
    );
    (server, session, original, listener)
}

fn serve_lookup(
    listener: TcpListener,
    original: GameplayActionRequest,
    response_kind: &'static str,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        let (mut stream, _) = loop {
            if let Ok(pair) = listener.accept() {
                break pair;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "expected one original lookup"
            );
            thread::sleep(Duration::from_millis(5));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        let (offset, length) = loop {
            let mut chunk = [0; 4096];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(offset) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..offset]);
                assert!(
                    header.starts_with(&format!("POST {LOOKUP_PATH} ")),
                    "never submit"
                );
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= offset + 4 + length {
                    break (offset + 4, length);
                }
            }
        };
        let lookup: AuthenticatedLookup =
            serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
        assert_eq!(
            serde_json::to_value(&lookup.original).unwrap(),
            serde_json::to_value(WorldServicePayloadV1::GameplayJson(
                serde_json::to_vec(&original).unwrap()
            ))
            .unwrap()
        );
        let correlation = crate::world_service::derive_correlation(
            WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
            &lookup.original,
        )
        .unwrap();
        assert_eq!(lookup.request.key, correlation.key);
        if response_kind == "overload" {
            stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            return;
        }
        let mut correlation = correlation;
        if response_kind == "mismatch" {
            correlation.payload_digest = "wrong-digest".into();
        }
        let response = authority::sign_service_response(
            LOOKUP_PATH,
            authority::request_digest(LOOKUP_PATH, &lookup).unwrap(),
            IntentResponse::<serde_json::Value> {
                formal_profile: None,
                execution_evidence_scope: None,
                contract_version: 1,
                correlation,
                outcome: if response_kind == "history" {
                    IntentOutcome::HistoryUnavailable
                } else if response_kind == "committed" {
                    IntentOutcome::Committed {
                        commit: Box::new(CommitRef {
                            world: WorldIdentity {
                                world_id: "world".into(),
                                genesis_digest: "genesis".into(),
                            },
                            binding: ExecutionBinding {
                                provider_world_id: "provider".into(),
                                branch_id: "branch".into(),
                                finality_ref: "finality".into(),
                                reorg_generation: 0,
                                governing_manifest_ref: "manifest".into(),
                                authority_generation: 1,
                                permission_generation: 1,
                            },
                            position: 10,
                            execution_block_hash: "block".into(),
                            state_root_ref: "root".into(),
                        }),
                        receipt: serde_json::json!({"action_id":101}),
                    }
                } else {
                    IntentOutcome::Rejected {
                        reason: WorldServiceErrorKind::Conflict,
                    }
                },
            },
            &hex::encode([9; 32]),
        )
        .unwrap();
        let body = serde_json::to_vec(&response).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(&body).unwrap();
    })
}

#[test]
fn original_lookup_after_restart_uses_signed_registration_and_never_submits() {
    for revoke_before_publish in [false, true] {
        let (server, mut session, original, listener) = fixture();
        assert!(
            server.pending_world_service_gameplay.is_empty(),
            "fresh runtime has no original queue"
        );
        let baseline = serde_json::to_value(server.world.state()).unwrap();
        let shared = Arc::new(Mutex::new(server));
        let http = serve_lookup(listener, original.clone(), "committed");
        let wire = serde_json::to_string(&ViewerRequest::LookupGameplayAction {
            request: original.clone(),
        })
        .unwrap();
        let decoded = oasis7_viewer_codec::request_from_json(&wire).unwrap();
        let prepared =
            ViewerRuntimeLiveServer::prepare_shared_gameplay_lookup(&shared, &decoded, &session)
                .unwrap();
        http.join().unwrap();
        let mut server = shared.lock().unwrap();
        if revoke_before_publish {
            server
                .session_policy
                .revoke_session("player", original.public_key.as_deref())
                .unwrap();
        }
        server.prepared_gameplay_lookup = Some(prepared);
        let mut output = Vec::new();
        server
            .handle_request(
                ViewerRequest::LookupGameplayAction { request: original },
                &mut session,
                &mut output,
            )
            .unwrap();
        let result: serde_json::Value =
            serde_json::from_slice(output.strip_suffix(b"\n").unwrap_or(&output)).unwrap();
        assert_eq!(result["type"], "gameplay_action_result");
        assert_eq!(result["request_auth_nonce"], 19);
        assert_eq!(
            result["status"],
            if revoke_before_publish {
                "unauthorized"
            } else {
                "committed"
            }
        );
        if revoke_before_publish {
            assert!(result.get("receipt").is_none());
        } else {
            assert_eq!(result["receipt"]["action_id"], 101);
        }
        assert_eq!(
            serde_json::to_value(server.world.state()).unwrap(),
            baseline
        );
        assert!(server.pending_world_service_gameplay.is_empty());
        assert!(
            server
                .llm_sidecar
                .validate_player_auth_nonce("player", 19)
                .is_ok(),
            "lookup never consumes the action nonce"
        );
    }
}

#[test]
fn original_lookup_denies_asserted_foreign_revoked_and_tampered_contexts_without_io() {
    for failure in [
        "asserted",
        "foreign",
        "revoked",
        "tampered",
        "world",
        "unsupported",
    ] {
        let (mut server, mut session, mut original, listener) = fixture();
        match failure {
            "asserted" => {
                server.clear_session_gameplay_feedback(&mut session);
                session.current_player_id = Some("player".into());
            }
            "foreign" => session.current_player_id = Some("other".into()),
            "revoked" => {
                server
                    .session_policy
                    .revoke_session("player", original.public_key.as_deref())
                    .unwrap();
            }
            "tampered" => original.target_agent_id = "other-agent".into(),
            "world" => server.config.world_id = "other-world".into(),
            "unsupported" => session.negotiated_protocol.capabilities.clear(),
            _ => unreachable!(),
        }
        let shared = Arc::new(Mutex::new(server));
        let wire = serde_json::to_string(&ViewerRequest::LookupGameplayAction {
            request: original.clone(),
        })
        .unwrap();
        let decoded = oasis7_viewer_codec::request_from_json(&wire).unwrap();
        let prepared =
            ViewerRuntimeLiveServer::prepare_shared_gameplay_lookup(&shared, &decoded, &session)
                .unwrap();
        let mut server = shared.lock().unwrap();
        server.prepared_gameplay_lookup = Some(prepared);
        let mut output = Vec::new();
        server
            .handle_request(
                ViewerRequest::LookupGameplayAction { request: original },
                &mut session,
                &mut output,
            )
            .unwrap();
        assert!(String::from_utf8(output).unwrap().contains("unauthorized"));
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "unauthorized query must not cross the service boundary"
        );
    }
}

#[test]
fn original_lookup_overload_history_and_mismatched_receipts_never_become_rejections() {
    for response in ["overload", "history", "mismatch"] {
        let (server, mut session, original, listener) = fixture();
        let shared = Arc::new(Mutex::new(server));
        let http = serve_lookup(listener, original.clone(), response);
        let prepared = ViewerRuntimeLiveServer::prepare_shared_gameplay_lookup(
            &shared,
            &ViewerRequest::LookupGameplayAction {
                request: original.clone(),
            },
            &session,
        )
        .unwrap();
        http.join().unwrap();
        let mut server = shared.lock().unwrap();
        server.prepared_gameplay_lookup = Some(prepared);
        let mut output = Vec::new();
        server
            .handle_request(
                ViewerRequest::LookupGameplayAction { request: original },
                &mut session,
                &mut output,
            )
            .unwrap();
        let result: serde_json::Value =
            serde_json::from_slice(output.strip_suffix(b"\n").unwrap_or(&output)).unwrap();
        assert_eq!(
            result["status"],
            if response == "history" {
                "history_unavailable"
            } else {
                "unavailable"
            }
        );
    }
}
