use super::*;
use crate::runtime::{Action as RuntimeAction, WorldState};
use crate::world_service::*;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

static OBSERVER_FIXTURE_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

fn observer_fixture() -> (PathBuf, RuntimeWorld, String, serde_json::Value) {
    let dir = std::env::temp_dir().join(format!(
        "oasis7-goal-observer-{}-{}-{}",
        std::process::id(),
        OBSERVER_FIXTURE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let private = hex::encode([91u8; 32]);
    let public = sign_read_request("fixture", (), &private)
        .unwrap()
        .subject_public_key;
    let mut world = crate::runtime_host::world::new_with_state(WorldState::default());
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "owner-agent".into(),
        pos: crate::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world.submit_action(RuntimeAction::ClaimStarterOc {
        agent_id: "owner-agent".into(),
        player_id: "owner".into(),
        public_key: Some(public.clone()),
    });
    world.step().unwrap();
    world
        .install_capability_agent_identity("owner-agent", "owner", 1)
        .unwrap();
    world
        .bind_cognition_runtime("privacy-world", "main", 0, None, "pending", 0)
        .unwrap();
    let mut chat = crate::viewer::AgentChatRequest {
        agent_id: "owner-agent".into(),
        message: "PRIVATE_CANONICAL_OWNER_GOAL".into(),
        player_id: Some("owner".into()),
        public_key: Some(public.clone()),
        auth: None,
        intent_tick: Some(2),
        intent_seq: Some(1),
        world_id: Some("privacy-world".into()),
        reorg_epoch: Some(0),
        authority_scope: Some("player_agent_chat".into()),
        replaces_intent_id: None,
        canonical_authority: Some(oasis7_proto::viewer::CanonicalAgentChatAuthorityV1 {
            branch_id: "main".into(),
            agent_identity_generation: 1,
        }),
    };
    chat.auth =
        Some(crate::viewer::sign_agent_chat_auth_proof(&chat, 1, &public, &private).unwrap());
    let payload = WorldServicePayloadV1::AgentChat(chat.clone());
    let receipt = world.apply_authenticated_agent_chat(&chat).unwrap();
    let identity = WorldIdentity {
        world_id: "privacy-world".into(),
        genesis_digest: "fixture".into(),
    };
    let request = SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(identity, &payload).unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request: request.clone(),
            action_id: 1,
            committed_height: 1,
            receipt,
            rejected: None,
        })
        .unwrap();
    let original = serde_json::to_value(request).unwrap();
    // Pre-commit input has a second direct exposure even without a result.
    world.submit_action(RuntimeAction::WorldServiceIntent {
        request: original.clone(),
    });
    world
        .save_to_dir_with_chain_resource_context(
            &dir,
            crate::runtime::ChainResourceDerivationContext {
                world_id: "privacy-world",
                chain_id: "fixture-chain",
                genesis_ref: None,
                created_at_height: 2,
                manifest_height: 2,
                commit_block_hash: None,
                tick: 2,
            },
            "fixture",
            "fixture",
        )
        .unwrap();
    let observer = RuntimeWorld::load_observer_with_control(
        &dir,
        crate::runtime::ObserverReadLimits::default(),
        crate::runtime::ObserverLoadControl::default(),
    )
    .unwrap();
    (dir, observer, public, original)
}
#[test]
fn canonical_goal_full_observer_public_snapshot_omits_raw_signed_input() {
    let (dir, observer, public, _) = observer_fixture();
    let original = serde_json::to_value(observer.snapshot()).unwrap();
    let original_journal = serde_json::to_value(observer.journal()).unwrap();
    assert!(
        original
            .to_string()
            .contains("PRIVATE_CANONICAL_OWNER_GOAL")
    );
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .unwrap();
    server.world = observer;
    let owner_before = projection::WorldServiceProjection::from_authenticated_world(
        &server.world,
        Some("owner-agent"),
        &public,
    )
    .unwrap()
    .canonical_agent_chat
    .unwrap();
    let snapshot = server.compat_snapshot(None);
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("PRIVATE_CANONICAL_OWNER_GOAL")
    );
    assert!(
        snapshot
            .player_gameplay
            .unwrap()
            .canonical_agent_chat
            .is_none()
    );
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::Hello {
                version: VIEWER_PROTOCOL_VERSION,
                client: "public-goal-probe".into(),
            },
            &mut session,
            &mut writer,
        )
        .unwrap();
    server
        .handle_request(ViewerRequest::RequestSnapshot, &mut session, &mut writer)
        .unwrap();
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    assert!(
        responses
            .iter()
            .any(|response| matches!(response, ViewerResponse::Snapshot { .. }))
    );
    assert!(
        !serde_json::to_string(&responses)
            .unwrap()
            .contains("PRIVATE_CANONICAL_OWNER_GOAL")
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        original
    );
    assert_eq!(
        serde_json::to_value(server.world.journal()).unwrap(),
        original_journal
    );
    let owner_after = projection::WorldServiceProjection::from_authenticated_world(
        &server.world,
        Some("owner-agent"),
        &public,
    )
    .unwrap()
    .canonical_agent_chat
    .unwrap();
    assert_eq!(owner_before, owner_after);
    assert_eq!(
        owner_after.goal.unwrap().message,
        "PRIVATE_CANONICAL_OWNER_GOAL"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn canonical_goal_outbound_filter_preserves_non_chat_and_filters_effect_inputs() {
    let (dir, observer, _, request) = observer_fixture();
    let mut snapshot = observer.snapshot();
    snapshot.pending_actions.clear();
    snapshot
        .capability_revocation_state
        .world_service_results
        .clear();
    let preserved = serde_json::to_value(&snapshot).unwrap();
    snapshot_privacy::omit_private_canonical_chat(&mut snapshot);
    assert_eq!(serde_json::to_value(&snapshot).unwrap(), preserved);
    let effect = crate::runtime::EffectIntent {
        birth_era: None,
        intent_id: "chat-effect".into(),
        kind: "world_service".into(),
        params: serde_json::json!({"input": request}),
        cap_ref: "fixture".into(),
        origin: crate::runtime::EffectOrigin::System,
    };
    let mut public_effect = effect.clone();
    public_effect.intent_id = "public-effect".into();
    public_effect.params = serde_json::json!({"kind":"cognition_proposal", "public":true});
    snapshot.pending_effects = vec![effect.clone(), public_effect.clone()];
    snapshot
        .inflight_effects
        .insert(effect.intent_id.clone(), effect);
    snapshot
        .inflight_effects
        .insert(public_effect.intent_id.clone(), public_effect.clone());
    snapshot_privacy::omit_private_canonical_chat(&mut snapshot);
    assert_eq!(snapshot.pending_effects, vec![public_effect.clone()]);
    assert_eq!(snapshot.inflight_effects.len(), 1);
    assert_eq!(
        snapshot.inflight_effects.get("public-effect"),
        Some(&public_effect)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn canonical_goal_owner_browser_read_forwards_original_proof_and_isolates_sessions() {
    use crate::world_service::{authority, client::WorldServiceClientConfig};
    use oasis7_client_api::world_service::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let (dir, world, public, _) = observer_fixture();
    let identity = WorldIdentity {
        world_id: "privacy-world".into(),
        genesis_digest: "fixture".into(),
    };
    let commit = CommitRef {
        world: identity.clone(),
        binding: ExecutionBinding {
            provider_world_id: "privacy-world".into(),
            branch_id: "main".into(),
            finality_ref: "fixture".into(),
            reorg_generation: 0,
            governing_manifest_ref: "fixture".into(),
            authority_generation: 1,
            permission_generation: 1,
        },
        position: 2,
        execution_block_hash: "fixture".into(),
        state_root_ref: "fixture".into(),
    };
    let request = ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: identity.clone(),
        scope_id: "agent:owner-agent".into(),
        min_commit: Some(commit.clone()),
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let mut projection = projection::WorldServiceProjection::from_world(&world, None).unwrap();
    projection.runtime_binding = None;
    let response = ReadWorldViewResponse {
        formal_profile: None,
        formal_proof: None,
        execution_evidence_scope: None,
        contract_version: 1,
        version: ProjectionVersion {
            commit: commit.clone(),
            projection_revision: "fixture".into(),
            visibility_scope: "public".into(),
        },
        logical_tick: world.state().time,
        continuation: EventCursor {
            stream_id: "fixture".into(),
            scope_id: "public".into(),
            era: 0,
            sequence: 0,
            commit,
        },
        view: projection,
    };
    let mut private_response = response.clone();
    private_response.version.visibility_scope = request.scope_id.clone();
    private_response.continuation.scope_id = request.scope_id.clone();
    private_response.view.canonical_agent_chat =
        crate::world_service::agent_chat::owner_view(&world, "owner-agent").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let expected =
        authority::sign_read_request(VIEW_PATH, request.clone(), &hex::encode([91; 32])).unwrap();
    let original = serde_json::to_value(&expected).unwrap();
    let expected_wire = original.clone();
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let (offset, length) = loop {
            let mut buffer = [0; 1024];
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..index]).to_lowercase();
                let length: usize = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if bytes.len() >= index + 4 + length {
                    break (index + 4, length);
                }
            }
        };
        let signed: SignedReadRequest<ReadWorldViewRequest> =
            serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
        assert_eq!(serde_json::to_value(&signed).unwrap(), expected_wire);
        authority::verify_read_request(VIEW_PATH, &signed).unwrap();
        let signed_response = authority::sign_service_response(
            VIEW_PATH,
            authority::request_digest(VIEW_PATH, &signed).unwrap(),
            private_response,
            &hex::encode([9; 32]),
        )
        .unwrap();
        let body = serde_json::to_vec(&signed_response).unwrap();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        socket.write_all(&body).unwrap();
    });
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .unwrap();
    server.world = world;
    server.config.world_service = Some(WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint,
        trusted_service_public_key: authority::sign_read_request(
            "fixture",
            (),
            &hex::encode([9; 32]),
        )
        .unwrap()
        .subject_public_key,
        expected_world: identity,
        scope_id: "public".into(),
        read_private_key_hex: hex::encode([7; 32]),
        timeout: Duration::from_secs(3),
        max_response_bytes: 1024 * 1024,
    });
    let mut public_request = request.clone();
    public_request.scope_id = "public".into();
    server.verified_world_view = Some(verified_fixture_assertion(
        response.clone(),
        &public_request,
    ));
    assert!(
        server
            .verified_world_view
            .as_ref()
            .unwrap()
            .projection()
            .canonical_agent_owner
            .is_none()
    );
    server
        .session_policy
        .register_session("owner", &public)
        .unwrap();
    let unchanged = serde_json::to_value(server.world.snapshot()).unwrap();
    let mut owner = RuntimeLiveSession::new();
    owner.current_player_id = Some("owner".into());
    let mut output = Vec::new();
    server
        .handle_owner_read_context("owner-agent".into(), &mut owner, &mut output)
        .unwrap();
    assert!(
        !String::from_utf8(output.clone())
            .unwrap()
            .contains("PRIVATE_CANONICAL_OWNER_GOAL")
    );
    let shared = Arc::new(std::sync::Mutex::new(server));
    let prepared = ViewerRuntimeLiveServer::prepare_shared_owner_read(
        &shared,
        &ViewerRequest::CanonicalAgentOwnerRead {
            request: original.clone(),
        },
        &owner,
    )
    .unwrap();
    let mut server = shared.lock().unwrap();
    server.prepared_owner_read = prepared.map(Box::new);
    server
        .handle_owner_read(original.clone(), &mut owner, &mut output)
        .unwrap();
    worker.join().unwrap();
    assert!(owner.owner_read_view.as_ref().unwrap().goal.is_some());
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("PRIVATE_CANONICAL_OWNER_GOAL")
    );
    assert!(
        server
            .verified_world_view
            .as_ref()
            .unwrap()
            .projection()
            .canonical_agent_chat
            .is_none()
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        unchanged
    );
    assert!(
        !serde_json::to_string(&server.compat_snapshot(None))
            .unwrap()
            .contains("PRIVATE_CANONICAL_OWNER_GOAL")
    );
    let mut foreign = RuntimeLiveSession::new();
    foreign.current_player_id = Some("foreign".into());
    assert!(
        server
            .handle_owner_read_context("owner-agent".into(), &mut foreign, &mut Vec::new())
            .is_err()
    );
    assert!(
        server
            .handle_owner_read(original, &mut foreign, &mut Vec::new())
            .is_err()
    );
    assert!(foreign.owner_read_view.is_none());
    assert!(
        server
            .llm_sidecar
            .canonical_owner_goal
            .as_ref()
            .unwrap()
            .goal
            .is_some()
    );
    assert!(
        !serde_json::to_string(&server.compat_snapshot(Some("foreign")))
            .unwrap()
            .contains("PRIVATE_CANONICAL_OWNER_GOAL")
    );
    assert_eq!(
        server.llm_sidecar.prompt_profiles["owner-agent"]
            .short_term_goal_override
            .as_deref(),
        Some("PRIVATE_CANONICAL_OWNER_GOAL")
    );
    let mut newer_public = response;
    newer_public.version.commit.position += 1;
    newer_public.continuation.commit = newer_public.version.commit.clone();
    server.verified_world_view = Some(verified_fixture_assertion(newer_public, &public_request));
    server.invalidate_owner_read_session(&mut owner);
    server.configure_service_provider();
    assert!(owner.owner_read_view.is_none());
    assert!(server.llm_sidecar.canonical_owner_goal.is_none());
    assert!(
        server.llm_sidecar.prompt_profiles["owner-agent"]
            .short_term_goal_override
            .is_none()
    );
    owner.current_player_id = None;
    server.invalidate_owner_read_session(&mut owner);
    assert!(owner.owner_read_view.is_none());
    assert!(owner.owner_read_context.is_none());
    server.configure_service_provider();
    let delivered_goal = server.llm_sidecar.prompt_profiles["owner-agent"]
        .short_term_goal_override
        .clone();
    server.configure_service_provider();
    assert_eq!(
        server.llm_sidecar.prompt_profiles["owner-agent"].short_term_goal_override,
        delivered_goal
    );
    owner.current_player_id = Some("owner".into());
    server
        .handle_owner_read_context("owner-agent".into(), &mut owner, &mut Vec::new())
        .unwrap();
    let new_key = authority::sign_read_request("fixture", (), &hex::encode([92; 32]))
        .unwrap()
        .subject_public_key;
    let rotation = server
        .session_policy
        .validate_session_registration("owner", &new_key, true)
        .unwrap();
    server.session_policy.commit_session_registration(rotation);
    server.invalidate_owner_read_session(&mut owner);
    assert!(owner.owner_read_context.is_none());
    assert!(owner.owner_read_view.is_none());
    server.configure_service_provider();
    assert!(server.llm_sidecar.canonical_owner_goal.is_none());
    assert!(
        server.llm_sidecar.prompt_profiles["owner-agent"]
            .short_term_goal_override
            .is_none()
    );

    fs::remove_dir_all(dir).unwrap();
}

fn verified_fixture_assertion(
    response: oasis7_client_api::world_service::ReadWorldViewResponse<
        crate::world_service::projection::WorldServiceProjection,
    >,
    request: &oasis7_client_api::world_service::ReadWorldViewRequest,
) -> crate::world_service::verified_view::VerifiedWorldView {
    use crate::world_service::authority;
    let path = oasis7_client_api::world_service::VIEW_PATH;
    let request =
        authority::sign_read_request(path, request.clone(), &hex::encode([7; 32])).unwrap();
    let digest = authority::request_digest(path, &request).unwrap();
    let response =
        authority::sign_service_response(path, digest, response, &hex::encode([9; 32])).unwrap();
    let key = authority::sign_read_request("fixture", (), &hex::encode([9; 32]))
        .unwrap()
        .subject_public_key;
    crate::world_service::verified_view::VerifiedWorldView::verify_service_assertion(
        response,
        &request,
        &key,
        crate::world_service::verified_view::ReadEvidencePolicy::Ordinary,
    )
    .unwrap()
}
