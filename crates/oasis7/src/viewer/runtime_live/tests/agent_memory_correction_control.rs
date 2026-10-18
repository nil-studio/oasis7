use super::auth_actions::{
    MockHttpResponse, RuntimeLiveMockHttpServer, provider_context_response,
    spawn_runtime_live_mock_http_server_until_drop,
};
use super::*;
use crate::viewer::runtime_live::agency_control::{
    AGENCY_CONTROL_REQUEST_TYPE, AgencyControlOperation, AgencyControlRequest,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
const PLAYER_ID: &str = "agency-owner";
#[path = "agent_memory_correction_control_service_mode.rs"]
mod service_mode;
fn root_identity(world: &crate::runtime::World) -> String {
    world
        .execution_state_identity()
        .expect("world state identity")
        .state_root_hash
}

fn owner_server(seed: u8) -> (ViewerRuntimeLiveServer, String, String, String) {
    owner_server_with_config(
        seed,
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
}

pub(super) fn owner_server_with_config(
    seed: u8,
    config: ViewerRuntimeLiveServerConfig,
) -> (ViewerRuntimeLiveServer, String, String, String) {
    let mut server = ViewerRuntimeLiveServer::new(config).expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(seed);
    register_runtime_session(
        &mut server,
        PLAYER_ID,
        Some(agent_id.as_str()),
        1_000,
        public_key.as_str(),
        private_key.as_str(),
    );
    let claim = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_STARTER_OC.to_string(),
            target_agent_id: agent_id.clone(),
            actor_agent_id: None,
            player_id: PLAYER_ID.to_string(),
            public_key: None,
            auth: None,
        },
        1_001,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .handle_gameplay_action(claim)
        .expect("owner starter claim accepted");
    server.world.step().expect("apply owner starter claim");
    let mut chat = crate::viewer::AgentChatRequest {
        agent_id: agent_id.clone(),
        message: "work on the assigned task".into(),
        player_id: Some(PLAYER_ID.into()),
        public_key: Some(public_key.clone()),
        auth: None,
        intent_tick: Some(server.world.state().time),
        intent_seq: Some(1_002),
        world_id: Some(server.config.world_id.clone()),
        reorg_epoch: Some(server.reorg_epoch),
        authority_scope: Some("player_agent_chat".into()),
        replaces_intent_id: None,
        canonical_authority: None,
    };
    chat.auth = Some(
        crate::viewer::sign_agent_chat_auth_proof(&chat, 1_002, &public_key, &private_key).unwrap(),
    );
    let mut candidate = server.world.clone();
    candidate
        .record_signed_local_agent_chat(&chat, &server.current_owner_authority())
        .expect("record canonical pending owner intent");
    server.world = candidate;
    let starter_claim = server
        .world
        .state()
        .starter_oc_claims
        .get(agent_id.as_str())
        .expect("signed starter OC action creates the owner claim");
    assert_eq!(starter_claim.player_id, PLAYER_ID);
    assert_eq!(
        starter_claim.public_key.as_deref(),
        Some(public_key.as_str())
    );
    let install = signed_agency_request(
        &server,
        &agent_id,
        "fixture-owner-grant",
        AgencyControlOperation::InstallDelegation {
            grant_id: "owner-grant".into(),
            object_id: agent_id.clone(),
            action_kinds: vec!["MoveAgent".into()],
            period_id: "owner-test-period".into(),
            valid_from_tick: server.world.state().time,
            valid_until_tick: server.world.state().time.saturating_add(100),
            limit_units: 4,
        },
        1_003,
        PLAYER_ID,
        &public_key,
        &private_key,
    );
    let mut candidate = server.world.clone();
    candidate
        .apply_signed_delegation_control(&install, &server.current_owner_authority())
        .expect("install exact source owner grant");
    server.world = candidate;
    (server, agent_id, public_key, private_key)
}

pub(super) fn drive_provider_action(
    server: &mut ViewerRuntimeLiveServer,
    expect_failure: bool,
) -> Result<Option<String>, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        server.llm_sidecar.request_decision();
        match server.enqueue_llm_action_from_sidecar() {
            Ok(Some(_trace)) if !expect_failure => return Ok(None),
            Err(trace) if expect_failure => {
                // Recovery/settlement failures are not the requested provider
                // disposition. Preserve their full trace instead of letting a
                // later correction-state assertion hide the first failure.
                if super::super::decision_trace::decision_trace_provider_error_retryable(&trace)
                    != Some(false)
                {
                    return Err(format!(
                        "provider did not return a typed nonretryable error: {trace:?}"
                    ));
                }
                return Ok(Some(
                    trace.llm_error.unwrap_or_else(|| "provider failed".into()),
                ));
            }
            Ok(None) => {}
            Ok(Some(trace)) => {
                return Err(format!(
                    "provider returned an action instead of failing: {trace:?}"
                ));
            }
            Err(trace) => return Err(format!("provider action failed unexpectedly: {trace:?}")),
        }
        if Instant::now() >= deadline {
            return Err("provider action did not reach its expected terminal state".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

struct ProviderEnvironmentCleanup;

impl Drop for ProviderEnvironmentCleanup {
    fn drop(&mut self) {
        clear_runtime_provider_env();
    }
}

#[allow(clippy::too_many_arguments)]
fn signed_agency_request(
    server: &ViewerRuntimeLiveServer,
    agent_id: &str,
    request_id: &str,
    command: AgencyControlOperation,
    nonce: u64,
    player_id: &str,
    public_key: &str,
    private_key: &str,
) -> AgencyControlRequest {
    let command_value = serde_json::to_value(&command).expect("serialize command");
    let proof = crate::viewer::auth::sign_agency_control_auth_proof_for_test(
        request_id,
        player_id,
        public_key,
        agent_id,
        server.config.world_id.as_str(),
        server.reorg_epoch,
        &command_value,
        nonce,
        public_key,
        private_key,
    )
    .expect("sign typed agency control");
    AgencyControlRequest {
        frame_type: AGENCY_CONTROL_REQUEST_TYPE.to_string(),
        request_id: request_id.to_string(),
        player_id: player_id.to_string(),
        public_key: public_key.to_string(),
        agent_id: agent_id.to_string(),
        world_id: server.config.world_id.clone(),
        reorg_epoch: server.reorg_epoch,
        command,
        auth: Some(proof),
    }
}

fn current_intent(server: &ViewerRuntimeLiveServer, agent_id: &str) -> (String, String) {
    let intent = server
        .world
        .state()
        .agents
        .get(agent_id)
        .and_then(|cell| cell.intent.as_ref())
        .expect("current canonical Agent intent");
    (intent.intent_id.clone(), intent.request_digest.clone())
}

#[test]
fn owner_interrupt_cancels_only_exact_pending_intent_and_replay_is_read_only() {
    let _provider_lock = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) = owner_server(91);
    let (intent_id, request_digest) = current_intent(&server, agent_id.as_str());
    let command = AgencyControlOperation::InterruptPendingIntent {
        control_id: "interrupt-1".to_string(),
        intent_id: intent_id.clone(),
        request_digest,
        grant_id: None,
        expected_grant_revision: None,
    };
    let request = signed_agency_request(
        &server,
        agent_id.as_str(),
        "owner-interrupt-r1",
        command,
        1_002,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    let first = server.handle_agency_control_request(request.clone());
    assert_eq!(first.status, "ok", "{first:?}");
    assert_eq!(
        first.data.as_ref().unwrap()["intent"]["status"],
        "cancelled"
    );
    assert_eq!(
        first.data.as_ref().unwrap()["intent"]["reason_code"],
        "owner_interrupted_replan_required"
    );

    let after_interrupt = current_intent(&server, agent_id.as_str());
    assert_eq!(after_interrupt.0, intent_id);
    let replay = server.handle_agency_control_request(request);
    assert_eq!(replay.status, "ok", "{replay:?}");
    assert_eq!(replay.data.as_ref().unwrap()["idempotent_replay"], true);
    assert_eq!(
        replay.data.as_ref().unwrap()["intent"]["status"],
        "cancelled"
    );

    let replay_conflict = signed_agency_request(
        &server,
        agent_id.as_str(),
        "owner-interrupt-r2",
        AgencyControlOperation::InterruptPendingIntent {
            control_id: "interrupt-1".to_string(),
            intent_id,
            request_digest: "different-digest".to_string(),
            grant_id: None,
            expected_grant_revision: None,
        },
        1_003,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    let conflict = server.handle_agency_control_request(replay_conflict);
    assert_eq!(conflict.status, "error");
    assert_eq!(
        conflict.error.as_ref().unwrap().code,
        "owner_control_rejected"
    );
    assert!(
        server
            .llm_sidecar
            .validate_player_auth_nonce(PLAYER_ID, 1_003)
            .is_ok()
    );
}

#[test]
fn agency_control_owner_auth_is_bound_to_every_signed_field_and_never_mutates_on_denial() {
    let _provider_lock = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) = owner_server(92);
    let before = root_identity(&server.world);
    let (intent_id, request_digest) = current_intent(&server, agent_id.as_str());
    let mut request = signed_agency_request(
        &server,
        agent_id.as_str(),
        "owner-override-r1",
        AgencyControlOperation::OverridePendingIntent {
            control_id: "override-1".to_string(),
            intent_id,
            request_digest,
            grant_id: "owner-grant".to_string(),
            expected_grant_revision: 1,
        },
        1_002,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    request.reorg_epoch = request.reorg_epoch.saturating_add(1);
    let stale = server.handle_agency_control_request(request);
    assert_eq!(stale.status, "error");
    assert_eq!(stale.error.as_ref().unwrap().code, "stale_world_position");
    assert!(
        server
            .llm_sidecar
            .validate_player_auth_nonce(PLAYER_ID, 1_002)
            .is_ok()
    );
    assert_eq!(root_identity(&server.world), before);

    let (intruder_public, intruder_private) = test_signer(93);
    register_runtime_session(
        &mut server,
        "intruder-player",
        None,
        1_100,
        intruder_public.as_str(),
        intruder_private.as_str(),
    );
    let unauthorized = signed_agency_request(
        &server,
        agent_id.as_str(),
        "intruder-inspect-r1",
        AgencyControlOperation::Inspect,
        1_101,
        "intruder-player",
        intruder_public.as_str(),
        intruder_private.as_str(),
    );
    let response = server.handle_agency_control_request(unauthorized);
    assert_eq!(response.status, "error");
    assert_eq!(
        response.error.as_ref().unwrap().code,
        "agent_control_forbidden"
    );
    assert!(
        server
            .llm_sidecar
            .validate_player_auth_nonce("intruder-player", 1_101)
            .is_ok()
    );
    assert_eq!(root_identity(&server.world), before);

    let mut forged = signed_agency_request(
        &server,
        agent_id.as_str(),
        "owner-override-r2",
        AgencyControlOperation::OverridePendingIntent {
            control_id: "override-2".to_string(),
            intent_id: current_intent(&server, agent_id.as_str()).0,
            request_digest: current_intent(&server, agent_id.as_str()).1,
            grant_id: "owner-grant".to_string(),
            expected_grant_revision: 1,
        },
        1_002,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    forged.command = AgencyControlOperation::InterruptPendingIntent {
        control_id: "forged-control".to_string(),
        intent_id: "other-intent".to_string(),
        request_digest: "other-digest".to_string(),
        grant_id: None,
        expected_grant_revision: None,
    };
    let bad_signature = server.handle_agency_control_request(forged);
    assert_eq!(bad_signature.status, "error");
    assert_eq!(
        bad_signature.error.as_ref().unwrap().code,
        "auth_proof_invalid"
    );
    assert!(
        server
            .llm_sidecar
            .validate_player_auth_nonce(PLAYER_ID, 1_002)
            .is_ok()
    );
    assert_eq!(root_identity(&server.world), before);
}

#[test]
fn inspection_and_player_snapshot_share_owner_projection_but_unauthenticated_snapshots_do_not() {
    let _provider_lock = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) = owner_server(94);
    let owner_snapshot = server.compat_snapshot(Some(PLAYER_ID));
    let owner_model = owner_snapshot
        .player_gameplay
        .as_ref()
        .and_then(|gameplay| gameplay.primary_intent.as_ref())
        .and_then(|intent| intent.agency_read_model.as_ref())
        .expect("owner projection");
    assert_eq!(owner_model.delegation_authorizations.len(), 1);
    assert_eq!(
        owner_model.delegation_authorizations[0].grant.resource_kind,
        "electricity"
    );
    assert!(owner_model.referenced_memory_context.is_none());

    let unauthed_snapshot = server.compat_snapshot(None);
    assert!(
        unauthed_snapshot
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.primary_intent.as_ref())
            .and_then(|intent| intent.agency_read_model.as_ref())
            .is_none()
    );
    assert!(
        server
            .compat_snapshot(Some("intruder-player"))
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.primary_intent.as_ref())
            .and_then(|intent| intent.agency_read_model.as_ref())
            .is_none()
    );

    // Network snapshots start anonymous. A player becomes the session
    // principal only after the existing signed session-register acknowledgement.
    let mut anonymous_session = RuntimeLiveSession::new();
    let (mut anonymous_writer, anonymous_peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::RequestSnapshot,
            &mut anonymous_session,
            &mut anonymous_writer,
        )
        .expect("anonymous snapshot request");
    let anonymous = read_runtime_live_snapshot(&mut BufReader::new(
        anonymous_peer.try_clone().expect("clone anonymous peer"),
    ));
    assert!(anonymous_session.current_player_id.is_none());
    assert!(
        anonymous
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.primary_intent.as_ref())
            .and_then(|intent| intent.agency_read_model.as_ref())
            .is_none()
    );

    let mut owner_session = RuntimeLiveSession::new();
    let (mut owner_writer, _) = test_writer_pair();
    let registration = signed_session_register_request(
        crate::viewer::AuthoritativeSessionRegisterRequest {
            player_id: PLAYER_ID.to_string(),
            public_key: None,
            registration_grant: None,
            auth: None,
            requested_agent_id: Some(agent_id.clone()),
            force_rebind: false,
        },
        1_003,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .handle_request(
            ViewerRequest::AuthoritativeRecovery {
                command: AuthoritativeRecoveryCommand::RegisterSession {
                    request: registration,
                },
            },
            &mut owner_session,
            &mut owner_writer,
        )
        .expect("signed owner session registration");
    assert_eq!(owner_session.current_player_id.as_deref(), Some(PLAYER_ID));
    let (mut owner_snapshot_writer, owner_snapshot_peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::RequestSnapshot,
            &mut owner_session,
            &mut owner_snapshot_writer,
        )
        .expect("owner session snapshot");
    let authenticated_snapshot = read_runtime_live_snapshot(&mut BufReader::new(
        owner_snapshot_peer.try_clone().expect("clone owner peer"),
    ));
    assert!(
        authenticated_snapshot
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.primary_intent.as_ref())
            .and_then(|intent| intent.agency_read_model.as_ref())
            .is_some_and(|model| model.delegation_authorizations.len() == 1)
    );

    // A signed registration ACK establishes only its original live session.
    // Replaying that consumed ACK on a fresh connection must not set the new
    // session's private snapshot identity.
    let registration_replay = signed_session_register_request(
        crate::viewer::AuthoritativeSessionRegisterRequest {
            player_id: PLAYER_ID.to_string(),
            public_key: None,
            registration_grant: None,
            auth: None,
            requested_agent_id: Some(agent_id.clone()),
            force_rebind: false,
        },
        1_003,
        public_key.as_str(),
        private_key.as_str(),
    );
    let mut replay_session = RuntimeLiveSession::new();
    let (mut replay_writer, replay_peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::AuthoritativeRecovery {
                command: AuthoritativeRecoveryCommand::RegisterSession {
                    request: registration_replay,
                },
            },
            &mut replay_session,
            &mut replay_writer,
        )
        .expect("replayed registration is reported as a protocol error");
    assert!(replay_session.current_player_id.is_none());
    assert!(matches!(
        read_runtime_live_response(&mut BufReader::new(
            replay_peer.try_clone().expect("clone replay peer"),
        )),
        ViewerResponse::AuthoritativeRecoveryError { .. }
    ));
    let (mut replay_snapshot_writer, replay_snapshot_peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::RequestSnapshot,
            &mut replay_session,
            &mut replay_snapshot_writer,
        )
        .expect("anonymous replay connection snapshot");
    let replay_snapshot = read_runtime_live_snapshot(&mut BufReader::new(
        replay_snapshot_peer
            .try_clone()
            .expect("clone replay snapshot peer"),
    ));
    assert!(
        replay_snapshot
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.primary_intent.as_ref())
            .and_then(|intent| intent.agency_read_model.as_ref())
            .is_none()
    );

    let inspect = signed_agency_request(
        &server,
        agent_id.as_str(),
        "owner-inspect-r1",
        AgencyControlOperation::Inspect,
        1_004,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    let response = server.handle_agency_control_request(inspect.clone());
    assert_eq!(response.status, "ok", "{response:?}");
    let data = response.data.as_ref().unwrap();
    assert_eq!(
        data["agency"]["delegation_authorizations"][0]["grant"]["resource_kind"],
        "electricity"
    );
    assert_eq!(
        data["player_gameplay"]["primary_intent"]["agency_read_model"]["delegation_authorizations"],
        data["agency"]["delegation_authorizations"]
    );
    assert!(data["agency"].get("private_prompt").is_none());
    assert!(data["agency"].get("raw_trace").is_none());

    // The exact-tag control path does not establish a session identity. A
    // replay from a fresh session fails nonce validation and cannot make the
    // following ordinary snapshot owner-private.
    let replay = server.handle_agency_control_request(inspect);
    assert_eq!(replay.status, "error");
    assert_eq!(replay.error.as_ref().unwrap().code, "auth_nonce_replay");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();
    server
        .handle_request(ViewerRequest::RequestSnapshot, &mut session, &mut writer)
        .expect("fresh connection snapshot");
    let snapshot = read_runtime_live_snapshot(&mut BufReader::new(
        peer.try_clone().expect("clone test peer"),
    ));
    assert!(session.current_player_id.is_none());
    assert!(
        snapshot
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.primary_intent.as_ref())
            .and_then(|intent| intent.agency_read_model.as_ref())
            .is_none()
    );
}

#[test]
fn inaccessible_or_stale_memory_correction_does_not_consume_nonce_or_change_runtime() {
    let _provider_lock = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) = owner_server(95);
    let before = root_identity(&server.world);
    let correction = signed_agency_request(
        &server,
        agent_id.as_str(),
        "owner-memory-correction-r1",
        AgencyControlOperation::CorrectMemory {
            correction_id: "correction-1".to_string(),
            target_memory_id: "unreferenced-memory".to_string(),
            expected_revision: 0,
            replacement_summary: "corrected memory summary".to_string(),
        },
        1_002,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    let response = server.handle_agency_control_request(correction);
    assert_eq!(response.status, "error");
    assert_eq!(
        response.error.as_ref().unwrap().code,
        "memory_context_unavailable"
    );
    assert!(
        server
            .llm_sidecar
            .validate_player_auth_nonce(PLAYER_ID, 1_002)
            .is_ok()
    );
    assert_eq!(root_identity(&server.world), before);
    let owner_corrections = server
        .llm_sidecar
        .agent_memory_corrections(agent_id.as_str());
    assert!(owner_corrections.is_empty());
    let public = server.compat_snapshot(Some(PLAYER_ID));
    assert!(
        public
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.primary_intent.as_ref())
            .and_then(|intent| intent.agency_read_model.as_ref())
            .is_some_and(|model| model.memory_corrections.is_empty())
    );

    // A different player cannot use the authenticated endpoint's client
    // identity to inspect or modify the target Agent's private memory.
    let (other_public, other_private) = test_signer(96);
    register_runtime_session(
        &mut server,
        "other-owner",
        None,
        1_200,
        other_public.as_str(),
        other_private.as_str(),
    );
    let other_write = signed_agency_request(
        &server,
        agent_id.as_str(),
        "other-memory-correction-r1",
        AgencyControlOperation::CorrectMemory {
            correction_id: "other-correction".to_string(),
            target_memory_id: "unreferenced-memory".to_string(),
            expected_revision: 0,
            replacement_summary: "attempted private edit".to_string(),
        },
        1_201,
        "other-owner",
        other_public.as_str(),
        other_private.as_str(),
    );
    let rejected = server.handle_agency_control_request(other_write);
    assert_eq!(rejected.status, "error");
    assert_eq!(
        rejected.error.as_ref().unwrap().code,
        "agent_control_forbidden"
    );
    assert!(
        server
            .llm_sidecar
            .validate_player_auth_nonce("other-owner", 1_201)
            .is_ok()
    );
    assert!(
        server
            .llm_sidecar
            .agent_memory_corrections(agent_id.as_str())
            .is_empty()
    );
}

#[test]
fn agency_control_parser_rejects_wrong_frame_and_unknown_fields() {
    use crate::viewer::runtime_live::agency_control::{
        ParsedAgencyControlFrame, parse_agency_control_frame,
    };

    assert!(matches!(
        parse_agency_control_frame(r#"{"type":"viewer_request"}"#),
        ParsedAgencyControlFrame::NotAgencyControl
    ));
    let malformed = parse_agency_control_frame(
        r#"{"type":"agency_control_request","request_id":"r1","unexpected":true}"#,
    );
    assert!(matches!(malformed, ParsedAgencyControlFrame::Invalid(_)));
}

#[test]
fn owner_signed_memory_correction_applies_to_matching_provider_receipt_and_reloads() {
    let _provider_lock = runtime_provider_env_lock()
        .lock()
        .expect("provider env lock");
    clear_runtime_provider_env();
    let _provider_env_cleanup = ProviderEnvironmentCleanup;
    let decision_count = std::sync::Arc::new(AtomicUsize::new(0));
    let feedback_statuses = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mock_server: RuntimeLiveMockHttpServer = spawn_runtime_live_mock_http_server_until_drop({
        let decision_count = std::sync::Arc::clone(&decision_count);
        let feedback_statuses = std::sync::Arc::clone(&feedback_statuses);
        move |request| match request.path.as_str() {
            "/v1/world-simulator/decision-context" => {
                let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                    serde_json::from_slice(request.body.as_slice())
                        .expect("decode provider decision context");
                let ordinal = decision_count.fetch_add(1, Ordering::SeqCst) + 1;
                let response = if ordinal == 4 {
                    crate::simulator::DecisionResponse {
                        decision: crate::simulator::ProviderDecision::Wait,
                        module_command: None,
                        provider_error: Some(crate::simulator::ProviderErrorEnvelope {
                            code: "provider_unauthorized".to_string(),
                            message: "test provider rejects the next decision".to_string(),
                            retryable: false,
                        }),
                        diagnostics: crate::simulator::ProviderDiagnostics::default(),
                        trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                        memory_write_intents: Vec::new(),
                    }
                } else {
                    crate::simulator::DecisionResponse {
                        decision: crate::simulator::ProviderDecision::Act {
                            action_ref: "move_agent".to_string(),
                            action: oasis7_simulator::Action::MoveAgent {
                                agent_id: decoded
                                    .base_decision_request
                                    .observation
                                    .agent_id
                                    .clone(),
                                to: format!("runtime:{ordinal}:0:0"),
                            },
                        },
                        module_command: None,
                        provider_error: None,
                        diagnostics: crate::simulator::ProviderDiagnostics::default(),
                        trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                        memory_write_intents: if ordinal == 1 {
                            vec![crate::simulator::MemoryWriteIntent {
                                scope: "session_private".to_string(),
                                summary: "initial committed observation".to_string(),
                                tags: vec!["agency-test".to_string()],
                            }]
                        } else {
                            Vec::new()
                        },
                    }
                };
                MockHttpResponse {
                    status_code: 200,
                    body: serde_json::to_string(&provider_context_response(&decoded, response))
                        .expect("encode provider decision response"),
                }
            }
            "/v1/world-simulator/feedback-context" => {
                let feedback: crate::simulator::FeedbackEnvelopeV1 =
                    serde_json::from_slice(request.body.as_slice()).expect("decode feedback");
                feedback_statuses
                    .lock()
                    .expect("feedback status lock")
                    .push(feedback.status);
                MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true}).to_string(),
                }
            }
            _ => MockHttpResponse {
                status_code: 404,
                body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
            },
        }
    });
    // SAFETY: the shared provider environment lock serializes this test lane.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, mock_server.base_url());
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }

    let lineage_path = std::env::temp_dir().join(format!(
        "oasis7-owner-memory-correction-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let world_id = format!("live-runtime-{}", WorldScenario::Minimal.as_str());
    let finality_hash =
        crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &world_id).to_string();
    let config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
        .with_decision_mode(ViewerLiveDecisionMode::Llm)
        .with_provider_lineage_store(lineage_path.clone())
        .with_test_cognition_runtime_binding(
            "agency-correction-branch",
            0,
            Some(finality_hash.clone()),
            "verified",
            0,
        );
    let (mut server, agent_id, public_key, private_key) = owner_server_with_config(97, config);
    super::provider_continuation_drains::install_cognition_scheduler(&mut server);
    server
        .world
        .install_test_provider_capability_fixture(agent_id.as_str())
        .expect("install Runtime provider capability fixture");

    drive_provider_action(&mut server, false).expect("first provider action commits memory");
    assert_eq!(
        server.llm_sidecar.provider_memory_store().entries().len(),
        1
    );
    drive_provider_action(&mut server, false).expect("second provider action reads the memory");
    let committed_context = server
        .llm_sidecar
        .agent_referenced_memory_context(agent_id.as_str())
        .expect("owner private context used by second decision");
    assert_eq!(committed_context["used_for_decision"], true);
    assert_eq!(
        committed_context["current_use"],
        "committed_decision_context"
    );
    let target_memory_id = committed_context["sources"][0]["memory_id"]
        .as_str()
        .expect("retrieval source identity")
        .to_string();
    let expected_revision = committed_context["revision"]
        .as_u64()
        .expect("memory revision");

    let first_correction = signed_agency_request(
        &server,
        agent_id.as_str(),
        "owner-memory-correction-success-r1",
        AgencyControlOperation::CorrectMemory {
            correction_id: "correction-applied".to_string(),
            target_memory_id: target_memory_id.clone(),
            expected_revision,
            replacement_summary: "  the initial observation was inaccurate cafe\u{301}  "
                .to_string(),
        },
        1_003,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    let accepted = server.handle_agency_control_request(first_correction.clone());
    assert_eq!(accepted.status, "ok", "{accepted:?}");
    assert_eq!(
        accepted.data.as_ref().unwrap()["correction"]["status"],
        "accepted"
    );
    assert!(accepted.data.as_ref().unwrap()["correction"]["runtime_receipt_id"].is_null());
    let accepted_spend = server
        .world
        .agent_delegation_authorizations(agent_id.as_str())
        .expect("delegation usage")
        .first()
        .expect("owner grant")
        .spent_units;
    let replay = server.handle_agency_control_request(first_correction.clone());
    assert_eq!(replay.status, "ok", "{replay:?}");
    assert_eq!(replay.data.as_ref().unwrap()["idempotent_replay"], true);
    assert_eq!(
        server
            .world
            .agent_delegation_authorizations(agent_id.as_str())
            .expect("delegation usage after replay")[0]
            .spent_units,
        accepted_spend,
        "correction replay does not debit the world grant"
    );

    let spend_before_matching_commit = accepted_spend;
    drive_provider_action(&mut server, false).expect("corrected-memory provider action commits");
    let applied_context = server
        .llm_sidecar
        .agent_referenced_memory_context(agent_id.as_str())
        .expect("committed corrected retrieval context");
    assert_eq!(applied_context["used_for_decision"], true);
    assert_eq!(
        applied_context["entries"][0]["summary"],
        "the initial observation was inaccurate café"
    );
    assert!(
        applied_context["sources"][0]["correction_refs"]
            .as_array()
            .is_some_and(|refs| refs.iter().any(|id| id == "correction-applied"))
    );
    let corrections = server
        .llm_sidecar
        .agent_memory_corrections(agent_id.as_str());
    let applied = corrections
        .iter()
        .find(|row| row.correction_id == "correction-applied")
        .expect("applied correction");
    assert_eq!(applied.status, "applied");
    let applied_receipt_id = applied
        .runtime_receipt_id
        .as_deref()
        .expect("actual committed Runtime receipt");
    let runtime_receipts = server
        .world
        .agent_causal_receipts(agent_id.as_str())
        .expect("committed causal receipts");
    let applied_receipt = runtime_receipts
        .iter()
        .find(|receipt| receipt.receipt_id == applied_receipt_id)
        .expect("receipt joined to correction");
    assert_eq!(applied_receipt.disposition, "applied");
    assert!(!applied_receipt.domain_event_refs.is_empty());
    assert!(
        applied_receipt
            .correction_refs
            .iter()
            .any(|id| id == "correction-applied")
    );
    let spent_after_matching_commit = server
        .world
        .agent_delegation_authorizations(agent_id.as_str())
        .expect("delegation usage after corrected action")[0]
        .spent_units;
    assert_eq!(
        spent_after_matching_commit, spend_before_matching_commit,
        "native MoveAgent authorization quotes zero electricity units"
    );
    assert_eq!(
        applied_receipt.authorization.as_ref().unwrap().cost_units,
        0
    );

    let inspect_applied = signed_agency_request(
        &server,
        agent_id.as_str(),
        "owner-memory-inspect-applied-r1",
        AgencyControlOperation::Inspect,
        1_004,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    let inspected = server.handle_agency_control_request(inspect_applied);
    assert_eq!(inspected.status, "ok", "{inspected:?}");
    let inspected_data = inspected.data.as_ref().unwrap();
    assert_eq!(
        inspected_data["agency"]["memory_corrections"][0]["status"],
        "applied"
    );
    assert_eq!(
        inspected_data["player_gameplay"]["primary_intent"]["agency_read_model"],
        inspected_data["agency"],
        "owner Inspect and snapshot project the same causal agency DTO"
    );
    assert_eq!(
        inspected_data["agency"]["causal_receipt"]["receipt_id"],
        applied_receipt_id
    );
    assert_eq!(
        inspected_data["agency"]["causal_receipt"]["authorization"]["spent_units"],
        spent_after_matching_commit
    );
    assert_eq!(
        inspected_data["agency"]["causal_receipt"]["authorization"]["cost_units"],
        0
    );
    let snapshot_after_applied = server.compat_snapshot(Some(PLAYER_ID));
    assert_eq!(
        serde_json::to_value(
            snapshot_after_applied
                .player_gameplay
                .as_ref()
                .and_then(|gameplay| gameplay.primary_intent.as_ref())
                .and_then(|intent| intent.agency_read_model.as_ref())
                .expect("owner snapshot agency projection")
        )
        .unwrap(),
        inspected_data["agency"]
    );

    let next_revision = applied_context["revision"]
        .as_u64()
        .expect("updated revision");
    let second_correction = signed_agency_request(
        &server,
        agent_id.as_str(),
        "owner-memory-correction-ignored-r1",
        AgencyControlOperation::CorrectMemory {
            correction_id: "correction-ignored".to_string(),
            target_memory_id,
            expected_revision: next_revision,
            replacement_summary: "this follow-up context will not commit".to_string(),
        },
        1_005,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    let accepted_ignored = server.handle_agency_control_request(second_correction);
    assert_eq!(accepted_ignored.status, "ok", "{accepted_ignored:?}");
    assert_eq!(
        accepted_ignored.data.as_ref().unwrap()["correction"]["status"],
        "accepted"
    );
    let failed = drive_provider_action(&mut server, true)
        .expect("nonretryable provider error terminalizes the correction");
    println!("nonretryable_provider_failure_actual={failed:?}");
    assert!(failed
        .as_deref()
        .is_some_and(|message| message.contains("unauthorized") || message.contains("provider")));
    let final_corrections = server
        .llm_sidecar
        .agent_memory_corrections(agent_id.as_str());
    assert_eq!(
        final_corrections
            .iter()
            .find(|row| row.correction_id == "correction-applied")
            .unwrap()
            .status,
        "applied"
    );
    assert_eq!(
        final_corrections
            .iter()
            .find(|row| row.correction_id == "correction-ignored")
            .unwrap()
            .status,
        "ignored"
    );
    assert_eq!(decision_count.load(Ordering::SeqCst), 4);

    let expected_receipt_ids = server
        .world
        .agent_causal_receipts(agent_id.as_str())
        .expect("pre-restart receipts")
        .into_iter()
        .map(|receipt| receipt.receipt_id)
        .collect::<Vec<_>>();
    let final_spend = server
        .world
        .agent_delegation_authorizations(agent_id.as_str())
        .expect("final grant usage")[0]
        .spent_units;
    assert_eq!(final_spend, spent_after_matching_commit);

    let restart_config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
        .with_decision_mode(ViewerLiveDecisionMode::Llm)
        .with_test_cognition_runtime_binding(
            "agency-correction-branch",
            0,
            Some(finality_hash),
            "verified",
            0,
        );
    let mut restarted = ViewerRuntimeLiveServer::new(restart_config).expect("restart server");
    restarted.world = server.world.clone();
    restarted
        .llm_sidecar
        .configure_provider_lineage_store(lineage_path.clone());
    restarted
        .llm_sidecar
        .restore_provider_lineage(&restarted.world)
        .expect("restore memory corrections and provider lineage");
    register_runtime_session(
        &mut restarted,
        PLAYER_ID,
        Some(agent_id.as_str()),
        1_007,
        public_key.as_str(),
        private_key.as_str(),
    );
    let restart_replay = restarted.handle_agency_control_request(first_correction);
    assert_eq!(restart_replay.status, "ok", "{restart_replay:?}");
    assert_eq!(
        restart_replay.data.as_ref().unwrap()["idempotent_replay"],
        true
    );
    assert_eq!(
        restarted
            .world
            .agent_delegation_authorizations(agent_id.as_str())
            .expect("reloaded grant usage")[0]
            .spent_units,
        final_spend,
        "replaying a durable correction does not repeat Runtime effect or debit"
    );
    assert_eq!(
        restarted
            .world
            .agent_causal_receipts(agent_id.as_str())
            .expect("reloaded receipts")
            .into_iter()
            .map(|receipt| receipt.receipt_id)
            .collect::<Vec<_>>(),
        expected_receipt_ids
    );
    let inspect_reloaded = signed_agency_request(
        &restarted,
        agent_id.as_str(),
        "owner-memory-inspect-reloaded-r1",
        AgencyControlOperation::Inspect,
        1_008,
        PLAYER_ID,
        public_key.as_str(),
        private_key.as_str(),
    );
    let reloaded = restarted.handle_agency_control_request(inspect_reloaded);
    assert_eq!(reloaded.status, "ok", "{reloaded:?}");
    let reloaded_data = reloaded.data.as_ref().unwrap();
    let reloaded_rows = reloaded_data["agency"]["memory_corrections"]
        .as_array()
        .expect("reloaded correction rows");
    assert_eq!(reloaded_rows[0]["status"], "applied");
    assert_eq!(reloaded_rows[1]["status"], "ignored");
    assert_eq!(
        reloaded_data["player_gameplay"]["primary_intent"]["agency_read_model"],
        reloaded_data["agency"]
    );
    let durable_snapshot = restarted.compat_snapshot(Some(PLAYER_ID));
    assert_eq!(
        serde_json::to_value(
            durable_snapshot
                .player_gameplay
                .as_ref()
                .and_then(|gameplay| gameplay.primary_intent.as_ref())
                .and_then(|intent| intent.agency_read_model.as_ref())
                .expect("reloaded owner snapshot agency projection")
        )
        .unwrap(),
        reloaded_data["agency"]
    );
    if let Some(output_path) = std::env::var_os("OASIS7_AGENCY_TEST_SNAPSHOT_OUT") {
        std::fs::write(
            output_path,
            serde_json::to_vec_pretty(&durable_snapshot).expect("serialize actual owner snapshot"),
        )
        .expect("write optional UI test snapshot");
    }
    assert!(
        feedback_statuses
            .lock()
            .expect("feedback statuses")
            .iter()
            .any(|status| status == "committed")
    );
    let _ = std::fs::remove_file(&lineage_path);
}
