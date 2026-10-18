use super::*;
use ed25519_dalek::SigningKey;
use oasis7_node::{
    NodeConfig, NodeExecutionCommitContext, NodeExecutionCommitResult, NodeExecutionHook, NodeRole,
    NodeRuntime,
};
use std::io::{BufRead, BufReader, BufWriter};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

#[path = "tests_auth.rs"]
mod tests_auth;

fn set_test_llm_env() -> std::sync::MutexGuard<'static, ()> {
    let guard = crate::viewer::canonical_runtime_provider_env_lock()
        .lock()
        .expect("env lock");
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_MODEL, "gpt-4o-mini");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            crate::simulator::ENV_LLM_BASE_URL,
            "https://api.openai.com/v1",
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_API_KEY, "test-api-key");
    }
    guard
}

fn test_signer(seed: u8) -> (String, String) {
    let private_key = [seed; 32];
    let signing_key = SigningKey::from_bytes(&private_key);
    (
        hex::encode(signing_key.verifying_key().to_bytes()),
        hex::encode(private_key),
    )
}

fn test_writer_pair() -> (BufWriter<TcpStream>, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let addr = listener.local_addr().expect("listener local addr");
    let client = TcpStream::connect(addr).expect("connect test client");
    let (server, _) = listener.accept().expect("accept test peer");
    (BufWriter::new(server), client)
}

fn read_response_line(peer: &TcpStream, timeout: Duration) -> Option<String> {
    let stream = peer.try_clone().expect("clone test peer");
    stream
        .set_read_timeout(Some(timeout))
        .expect("set read timeout");
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => Some(line),
        Err(err) => {
            if matches!(
                err.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                None
            } else {
                panic!("read response line failed: {err}");
            }
        }
    }
}

fn read_control_completion_ack(
    peer: &TcpStream,
    timeout: Duration,
) -> Option<crate::viewer::ControlCompletionAck> {
    let stream = peer.try_clone().expect("clone test peer");
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("set read timeout");
    let mut reader = BufReader::new(stream);
    let start = Instant::now();
    let mut line = String::new();
    while start.elapsed() < timeout {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => continue,
            Ok(_) => {}
            Err(err) => {
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) {
                    continue;
                }
                panic!("read response line failed: {err}");
            }
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(response) = serde_json::from_str::<crate::viewer::ViewerResponse>(trimmed) else {
            continue;
        };
        if let crate::viewer::ViewerResponse::ControlCompletionAck { ack } = response {
            return Some(ack);
        }
    }
    None
}

#[test]
fn live_loop_disconnect_error_normalizes_to_stop() {
    let result = normalize_live_loop_action_result(Err(ViewerLiveServerError::Io(
        std::io::Error::from(std::io::ErrorKind::BrokenPipe),
    )))
    .expect("disconnect is a clean stop");

    assert_eq!(result, LiveLoopIterationAction::Stop);
}

#[test]
fn live_control_metrics_mark_runtime_perf_unknown_when_unavailable() {
    let perf = unsupported_live_control_runtime_perf_snapshot();
    assert_eq!(perf.health, crate::simulator::RuntimePerfHealth::Unknown);
    assert_eq!(perf.tick.samples_total, 0);
    assert_eq!(perf.tick.samples_window, 0);
}

#[test]
fn live_loop_non_disconnect_error_is_preserved() {
    let result = normalize_live_loop_action_result(Err(ViewerLiveServerError::Io(
        std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    )));

    assert!(matches!(result, Err(ViewerLiveServerError::Io(_))));
}

fn signed_prompt_control_apply_request(
    mut request: PromptControlApplyRequest,
    intent: PromptControlAuthIntent,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> PromptControlApplyRequest {
    request.public_key = Some(public_key_hex.to_string());
    let proof = crate::viewer::sign_prompt_control_apply_auth_proof(
        intent,
        &request,
        nonce,
        public_key_hex,
        private_key_hex,
    )
    .expect("sign prompt_control apply auth");
    request.auth = Some(proof);
    request
}

fn signed_agent_chat_request(
    mut request: AgentChatRequest,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> AgentChatRequest {
    request.public_key = Some(public_key_hex.to_string());
    if request.intent_seq.is_none() {
        request.intent_seq = Some(nonce);
    }
    let proof =
        crate::viewer::sign_agent_chat_auth_proof(&request, nonce, public_key_hex, private_key_hex)
            .expect("sign agent_chat auth");
    request.auth = Some(proof);
    request
}

#[derive(Default)]
struct TestNoopExecutionHook;

impl NodeExecutionHook for TestNoopExecutionHook {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: format!("viewer-test-exec-block-{}", context.height),
            execution_state_root: format!("viewer-test-exec-state-{}", context.height),
        })
    }
}

#[test]
fn live_script_moves_between_locations() {
    let mut config = WorldConfig::default();
    config.physics.max_move_distance_cm_per_tick = i64::MAX;
    config.physics.max_move_speed_cm_per_s = i64::MAX;
    config.move_cost_per_km_electricity = 0;
    let init = WorldInitConfig::from_scenario(WorldScenario::TwinRegionBootstrap, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("init ok");

    let mut script = LiveScript::new(&kernel);
    let initial_location = kernel
        .model()
        .agents
        .get("agent-0")
        .expect("agent exists")
        .location_id
        .clone();
    let mut moved = false;
    for _ in 0..2 {
        let action = script.next_action(&kernel).expect("action");
        kernel.submit_action(action);
        kernel.step_until_empty();

        let agent = kernel.model().agents.get("agent-0").expect("agent exists");
        if agent.location_id != initial_location {
            moved = true;
            break;
        }
    }

    assert!(moved);
}

#[test]
fn live_world_reset_rebuilds_kernel() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");

    for _ in 0..5 {
        let _ = world.step().expect("step");
        if world.kernel.time() > 0 {
            break;
        }
    }
    assert!(world.kernel.time() > 0);

    world.reset().expect("reset ok");
    assert_eq!(world.kernel.time(), 0);
}

#[test]
fn live_server_config_supports_llm_mode() {
    let config = ViewerLiveServerConfig::new(WorldScenario::Minimal);
    assert_eq!(config.decision_mode, ViewerLiveDecisionMode::Script);
    assert_eq!(config.play_step_interval, Duration::from_millis(200));
    assert!(config.consensus_gate_max_tick.is_none());
    assert!(config.consensus_runtime.is_none());

    let llm_config = config.clone().with_llm_mode(true);
    assert_eq!(llm_config.decision_mode, ViewerLiveDecisionMode::Llm);

    let script_config = llm_config.with_decision_mode(ViewerLiveDecisionMode::Script);
    assert_eq!(script_config.decision_mode, ViewerLiveDecisionMode::Script);
}

fn viewer_live_llm_timeout_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn viewer_live_llm_timeout_defaults_to_trust_floor_budget() {
    let _guard = viewer_live_llm_timeout_env_lock().lock().expect("env lock");
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var("OASIS7_VIEWER_LIVE_LLM_TIMEOUT_MS");
    }
    assert_eq!(resolve_viewer_live_llm_timeout_ms(180_000), 30_000);
    assert_eq!(resolve_viewer_live_llm_timeout_ms(8_000), 8_000);
}

#[test]
fn viewer_live_llm_timeout_respects_env_ceiling_without_expanding_budget() {
    let _guard = viewer_live_llm_timeout_env_lock().lock().expect("env lock");
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var("OASIS7_VIEWER_LIVE_LLM_TIMEOUT_MS", "9000");
    }
    assert_eq!(resolve_viewer_live_llm_timeout_ms(180_000), 9_000);
    assert_eq!(resolve_viewer_live_llm_timeout_ms(4_000), 4_000);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var("OASIS7_VIEWER_LIVE_LLM_TIMEOUT_MS");
    }
}

#[test]
fn live_server_config_play_step_interval_clamps_to_minimum_budget() {
    let config = ViewerLiveServerConfig::new(WorldScenario::Minimal)
        .with_play_step_interval(Duration::from_millis(5));
    assert_eq!(config.play_step_interval, Duration::from_millis(20));
}

#[test]
fn viewer_live_session_play_schedule_scales_with_tick_progress() {
    let mut session = ViewerLiveSession::new();
    session.playing = true;
    let started_at = Instant::now();
    session.schedule_next_play_drive(Duration::from_millis(40), 4);
    let scheduled_at = session
        .next_play_step_at
        .expect("schedule should set next play drive timestamp");
    let delay = scheduled_at.saturating_duration_since(started_at);
    assert!(delay >= Duration::from_millis(160));
}

#[test]
fn live_world_consensus_gate_limits_step_budget() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let gate = Arc::new(AtomicU64::new(0));
    let mut world = LiveWorld::new_with_consensus_gate(
        config,
        init,
        ViewerLiveDecisionMode::Script,
        Some(Arc::clone(&gate)),
        None,
    )
    .expect("init ok");

    assert!(!world.can_step_for_consensus());
    gate.store(1, Ordering::SeqCst);
    assert!(world.can_step_for_consensus());

    let _ = world.step().expect("step");
    assert_eq!(world.kernel.time(), 1);
    assert!(!world.can_step_for_consensus());
}

#[test]
fn step_control_is_deferred_from_request_handler() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");
    let mut session = ViewerLiveSession::new();
    let (mut writer, _peer) = test_writer_pair();

    let outcome = session
        .handle_request(
            ViewerRequest::Control {
                mode: ViewerControl::Step { count: 3 },
                request_id: None,
            },
            &mut writer,
            &mut world,
            "test-world",
        )
        .expect("handle step control");

    assert_eq!(world.kernel.time(), 0);
    assert!(matches!(
        outcome.deferred_control,
        Some(ViewerLiveDeferredControl::Step {
            count: 3,
            request_id: None,
        })
    ));
}

#[test]
fn legacy_live_server_rejects_market_quote_decision_with_stable_code() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world =
        LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init world");
    let mut session = ViewerLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    session
        .handle_request(
            ViewerRequest::QuoteMarketDecision {
                request: crate::viewer::MarketQuoteDecisionRequest {
                    consume: vec![],
                    player_id: "player".to_string(),
                    public_key: None,
                    auth: None,
                },
            },
            &mut writer,
            &mut world,
            "test-world",
        )
        .expect("unsupported request is a response");

    let line = read_response_line(&peer, Duration::from_secs(1)).expect("response line");
    let response: crate::viewer::ViewerResponse =
        serde_json::from_str(line.trim()).expect("decode response");
    assert!(
        matches!(response, crate::viewer::ViewerResponse::GameplayActionError { error } if error.code == "unsupported_in_live_server" && error.action_id.as_deref() == Some("quote_market_decision"))
    );
}

#[test]
fn seek_control_is_ignored_in_live_request_handler() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");
    let mut session = ViewerLiveSession::new();
    let (mut writer, _peer) = test_writer_pair();

    let outcome = session
        .handle_request(
            ViewerRequest::Control {
                mode: ViewerControl::Seek { tick: 5 },
                request_id: None,
            },
            &mut writer,
            &mut world,
            "test-world",
        )
        .expect("handle seek control");

    assert_eq!(world.kernel.time(), 0);
    assert!(outcome.deferred_control.is_none());
}

#[test]
fn live_world_llm_bootstrap_script_mode_advances_tick() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::LlmBootstrap, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");

    for _ in 0..24 {
        let _ = world.step().expect("step");
    }

    assert!(world.kernel.time() > 0);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn live_world_llm_event_driven_gate_avoids_repeated_empty_ticks() {
    let _env_guard = set_test_llm_env();
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.agents = oasis7_simulator::AgentSpawnConfig {
        count: 0,
        ..oasis7_simulator::AgentSpawnConfig::default()
    };
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Llm).expect("init ok");

    let first = world.step().expect("first step");
    assert!(first.event.is_none());
    assert!(first.decision_trace.is_none());
    assert_eq!(world.metrics().total_ticks, 1);

    let second = world.step().expect("second step");
    assert!(second.event.is_none());
    assert!(second.decision_trace.is_none());
    assert_eq!(world.metrics().total_ticks, 1);

    world.request_llm_decision();
    let third = world.step().expect("third step");
    assert!(third.event.is_none());
    assert!(third.decision_trace.is_none());
    assert_eq!(world.metrics().total_ticks, 2);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn live_world_llm_mailbox_preserves_multiple_requests() {
    let _env_guard = set_test_llm_env();
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.agents = oasis7_simulator::AgentSpawnConfig {
        count: 0,
        ..oasis7_simulator::AgentSpawnConfig::default()
    };
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Llm).expect("init ok");

    let first = world.step().expect("first step");
    assert!(first.event.is_none());
    assert!(first.decision_trace.is_none());
    assert_eq!(world.metrics().total_ticks, 1);

    world.request_llm_decision();
    world.request_llm_decision();

    let second = world.step().expect("second step");
    assert!(second.event.is_none());
    assert!(second.decision_trace.is_none());
    assert_eq!(world.metrics().total_ticks, 2);

    let third = world.step().expect("third step");
    assert!(third.event.is_none());
    assert!(third.decision_trace.is_none());
    assert_eq!(world.metrics().total_ticks, 3);

    let fourth = world.step().expect("fourth step");
    assert!(fourth.event.is_none());
    assert!(fourth.decision_trace.is_none());
    assert_eq!(world.metrics().total_ticks, 3);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn live_world_event_drive_gate_tracks_llm_mailbox() {
    let _env_guard = set_test_llm_env();
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.agents = oasis7_simulator::AgentSpawnConfig {
        count: 0,
        ..oasis7_simulator::AgentSpawnConfig::default()
    };
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Llm).expect("init ok");

    assert!(world.should_step_on_event_drive());
    let _ = world.step().expect("step consumes bootstrap mailbox token");
    assert!(!world.should_step_on_event_drive());

    world.request_llm_decision();
    assert!(world.should_step_on_event_drive());
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn live_world_non_consensus_path_is_event_drive_only() {
    let script_config = WorldConfig::default();
    let script_init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &script_config);
    let script_world =
        LiveWorld::new(script_config, script_init, ViewerLiveDecisionMode::Script).expect("init");
    assert!(script_world.uses_non_consensus_event_drive());

    let _env_guard = set_test_llm_env();
    let llm_config = WorldConfig::default();
    let mut llm_init = WorldInitConfig::default();
    llm_init.agents = oasis7_simulator::AgentSpawnConfig {
        count: 0,
        ..oasis7_simulator::AgentSpawnConfig::default()
    };
    let llm_world =
        LiveWorld::new(llm_config, llm_init, ViewerLiveDecisionMode::Llm).expect("init");
    assert!(llm_world.uses_non_consensus_event_drive());
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn live_world_script_event_drive_stops_requeue_on_idle_step() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.agents = oasis7_simulator::AgentSpawnConfig {
        count: 0,
        ..oasis7_simulator::AgentSpawnConfig::default()
    };
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init");

    let step = world.step().expect("step");
    assert!(step.event.is_none());
    assert!(step.decision_trace.is_none());
    assert!(!world.should_requeue_non_consensus_drive(&step));
}

#[test]
fn emit_step_outcome_skips_idle_metrics_when_disabled() {
    let config = ViewerLiveServerConfig::new(WorldScenario::Minimal);
    let mut server = ViewerLiveServer::new(config).expect("server");
    let mut session = ViewerLiveSession::new();
    session.subscribed.insert(ViewerStream::Metrics);
    let (mut writer, peer) = test_writer_pair();

    server
        .emit_step_outcome(
            &mut session,
            &mut writer,
            LiveStepResult {
                event: None,
                decision_trace: None,
            },
            false,
        )
        .expect("emit throttled outcome");
    assert!(read_response_line(&peer, Duration::from_millis(50)).is_none());

    server
        .emit_step_outcome(
            &mut session,
            &mut writer,
            LiveStepResult {
                event: None,
                decision_trace: None,
            },
            true,
        )
        .expect("emit forced metrics");
    let line = read_response_line(&peer, Duration::from_millis(200))
        .expect("metrics response should be present");
    assert!(line.contains("\"metrics\""));
}

#[test]
fn restore_behavior_long_term_memory_from_model_applies_persisted_entries() {
    let _env_guard = set_test_llm_env();
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("init ok");

    let persisted = oasis7_simulator::LongTermMemoryEntry::new("mem-3", 7, "persisted insight")
        .with_tag("persisted");
    kernel
        .set_agent_long_term_memory("agent-0", vec![persisted.clone()])
        .expect("set persisted memory");

    let mut behavior = LlmAgentBehavior::from_env("agent-0").expect("build llm behavior");
    assert!(behavior.export_long_term_memory_entries().is_empty());

    restore_behavior_long_term_memory_from_model(&mut behavior, &kernel, "agent-0");
    let restored = behavior.export_long_term_memory_entries();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, persisted.id);
    assert_eq!(restored[0].content, persisted.content);
}

#[test]
fn sync_llm_runner_long_term_memory_writes_back_to_world_model() {
    let _env_guard = set_test_llm_env();
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("init ok");

    let mut behavior = LlmAgentBehavior::from_env("agent-0").expect("build llm behavior");
    let runtime_entry = oasis7_simulator::LongTermMemoryEntry::new("mem-9", 15, "runtime memory")
        .with_tag("runtime");
    behavior.restore_long_term_memory_entries(std::slice::from_ref(&runtime_entry));

    let mut runner = AgentRunner::new();
    runner.register(behavior);

    sync_llm_runner_long_term_memory(&mut kernel, &runner);
    let restored = kernel
        .long_term_memory_for_agent("agent-0")
        .expect("agent memory exists");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, runtime_entry.id);
    assert_eq!(restored[0].content, runtime_entry.content);
}

#[test]
fn live_world_consensus_bridge_applies_only_committed_actions() {
    let node_config = NodeConfig::new("node-live-bridge", "live-minimal", NodeRole::Sequencer)
        .expect("node config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("node tick interval");
    let mut node_runtime = NodeRuntime::new(node_config).with_execution_hook(TestNoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    let shared_runtime = Arc::new(Mutex::new(node_runtime));

    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new_with_consensus_gate(
        config,
        init,
        ViewerLiveDecisionMode::Script,
        None,
        Some(Arc::clone(&shared_runtime)),
    )
    .expect("init world");

    assert_eq!(world.kernel.time(), 0);
    let first = world.step().expect("submit step");
    assert!(first.event.is_none());
    assert_eq!(world.kernel.time(), 0);

    let mut observed_commit_event = false;
    for _ in 0..40 {
        thread::sleep(Duration::from_millis(20));
        let step = world.step().expect("consensus replay step");
        if step.event.is_some() {
            observed_commit_event = true;
            break;
        }
    }

    assert!(observed_commit_event);
    assert!(world.kernel.time() > 0);

    let mut locked = shared_runtime.lock().expect("lock node runtime");
    locked.stop().expect("stop node runtime");
}

#[test]
fn consensus_committed_advances_world_even_when_session_paused() {
    let node_config = NodeConfig::new(
        "node-live-paused-commit",
        "live-minimal",
        NodeRole::Sequencer,
    )
    .expect("node config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("node tick interval");
    let mut node_runtime = NodeRuntime::new(node_config).with_execution_hook(TestNoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    let shared_runtime = Arc::new(Mutex::new(node_runtime));

    let config = ViewerLiveServerConfig::new(WorldScenario::Minimal)
        .with_decision_mode(ViewerLiveDecisionMode::Script)
        .with_consensus_runtime(Arc::clone(&shared_runtime));
    let mut server = ViewerLiveServer::new(config).expect("init server");
    let mut session = ViewerLiveSession::new();
    session.playing = false;
    let (mut writer, _peer) = test_writer_pair();

    let submit = server.world.step().expect("submit step");
    assert!(submit.event.is_none());
    assert_eq!(server.world.kernel().time(), 0);

    let mut advanced = false;
    for _ in 0..80 {
        thread::sleep(Duration::from_millis(20));
        server
            .handle_consensus_committed(&mut session, &mut writer)
            .expect("apply committed actions while paused");
        if server.world.kernel().time() > 0 {
            advanced = true;
            break;
        }
    }
    assert!(
        advanced,
        "consensus commit should advance world while paused"
    );

    let mut locked = shared_runtime.lock().expect("lock node runtime");
    locked.stop().expect("stop node runtime");
}

#[test]
fn consensus_committed_paused_keeps_viewer_quiet() {
    let node_config = NodeConfig::new(
        "node-live-paused-quiet",
        "live-minimal",
        NodeRole::Sequencer,
    )
    .expect("node config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("node tick interval");
    let mut node_runtime = NodeRuntime::new(node_config).with_execution_hook(TestNoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    let shared_runtime = Arc::new(Mutex::new(node_runtime));

    let config = ViewerLiveServerConfig::new(WorldScenario::Minimal)
        .with_decision_mode(ViewerLiveDecisionMode::Script)
        .with_consensus_runtime(Arc::clone(&shared_runtime));
    let mut server = ViewerLiveServer::new(config).expect("init server");
    let mut session = ViewerLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    session.subscribed.insert(ViewerStream::Metrics);
    let (mut writer, peer) = test_writer_pair();

    let submit = server.world.step().expect("submit step");
    assert!(submit.event.is_none());
    assert_eq!(server.world.kernel().time(), 0);

    let mut advanced = false;
    for _ in 0..80 {
        thread::sleep(Duration::from_millis(20));
        server
            .handle_consensus_committed(&mut session, &mut writer)
            .expect("apply committed actions while paused");
        if server.world.kernel().time() > 0 {
            advanced = true;
            break;
        }
    }
    assert!(advanced, "consensus commit should still advance world");
    assert!(read_response_line(&peer, Duration::from_millis(50)).is_none());

    let mut locked = shared_runtime.lock().expect("lock node runtime");
    locked.stop().expect("stop node runtime");
}

#[test]
fn step_request_emits_completion_ack_advanced_when_world_progresses() {
    let config = ViewerLiveServerConfig::new(WorldScenario::Minimal)
        .with_decision_mode(ViewerLiveDecisionMode::Script);
    let mut server = ViewerLiveServer::new(config).expect("init server");
    let mut session = ViewerLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_step_request(&mut session, &mut writer, 1, Some(2001))
        .expect("step request should complete");

    let ack = read_control_completion_ack(&peer, Duration::from_secs(1))
        .expect("control completion ack should be emitted");
    assert_eq!(ack.request_id, 2001);
    assert_eq!(ack.status, ControlCompletionStatus::Advanced);
    assert!(ack.delta_logical_time > 0 || ack.delta_event_seq > 0);
}

#[test]
fn step_request_emits_completion_ack_timeout_when_consensus_has_no_commit() {
    let node_config = NodeConfig::new(
        "node-live-step-timeout",
        "live-minimal",
        NodeRole::Sequencer,
    )
    .expect("node config")
    .with_tick_interval(Duration::from_secs(5))
    .expect("node tick interval");
    let mut node_runtime = NodeRuntime::new(node_config).with_execution_hook(TestNoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    let shared_runtime = Arc::new(Mutex::new(node_runtime));

    let config = ViewerLiveServerConfig::new(WorldScenario::Minimal)
        .with_decision_mode(ViewerLiveDecisionMode::Script)
        .with_consensus_runtime(Arc::clone(&shared_runtime));
    let mut server = ViewerLiveServer::new(config).expect("init server");
    let mut session = ViewerLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_step_request(&mut session, &mut writer, 1, Some(2002))
        .expect("step request should complete");

    let ack = read_control_completion_ack(&peer, Duration::from_secs(3))
        .expect("control completion ack should be emitted");
    assert_eq!(ack.request_id, 2002);
    assert_eq!(ack.status, ControlCompletionStatus::TimeoutNoProgress);
    assert_eq!(ack.delta_logical_time, 0);
    assert_eq!(ack.delta_event_seq, 0);

    let mut locked = shared_runtime.lock().expect("lock node runtime");
    locked.stop().expect("stop node runtime");
}

#[test]
fn enqueue_coalesced_signal_merges_duplicate_llm_decision_requests() {
    let (tx, rx) = mpsc::sync_channel(4);
    let queued = Arc::new(AtomicBool::new(false));
    let backpressure = LiveLoopBackpressure::default();

    enqueue_coalesced_signal(
        &tx,
        LiveLoopSignal::LlmDecisionRequested,
        &queued,
        CoalescedSignalKind::LlmDecisionRequested,
        &backpressure,
    );
    enqueue_coalesced_signal(
        &tx,
        LiveLoopSignal::LlmDecisionRequested,
        &queued,
        CoalescedSignalKind::LlmDecisionRequested,
        &backpressure,
    );

    let signal = rx
        .recv_timeout(Duration::from_millis(50))
        .expect("should receive one llm signal");
    assert!(matches!(signal, LiveLoopSignal::LlmDecisionRequested));
    assert!(matches!(
        rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    let snapshot = backpressure.snapshot();
    assert_eq!(snapshot.merged_llm_decision_requested, 1);
    assert_eq!(snapshot.dropped_llm_decision_requested, 0);
    let llm_stats = snapshot.signal_stats(LiveLoopSignalKind::LlmDecisionRequested);
    assert_eq!(llm_stats.enqueued, 1);
    assert_eq!(llm_stats.handled, 0);
}

#[test]
fn enqueue_coalesced_signal_drops_when_queue_is_full() {
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(LiveLoopSignal::StepRequested {
        count: 1,
        request_id: None,
    })
    .expect("fill queue");
    let queued = Arc::new(AtomicBool::new(false));
    let backpressure = LiveLoopBackpressure::default();

    enqueue_coalesced_signal(
        &tx,
        LiveLoopSignal::ConsensusDriveRequested,
        &queued,
        CoalescedSignalKind::ConsensusDriveRequested,
        &backpressure,
    );

    let queued_signal = rx
        .recv_timeout(Duration::from_millis(50))
        .expect("receive original signal");
    assert!(matches!(
        queued_signal,
        LiveLoopSignal::StepRequested { .. }
    ));
    assert!(!queued.load(Ordering::SeqCst));

    let snapshot = backpressure.snapshot();
    assert_eq!(snapshot.merged_consensus_drive_requested, 0);
    assert_eq!(snapshot.dropped_consensus_drive_requested, 1);
    let drive_stats = snapshot.signal_stats(LiveLoopSignalKind::ConsensusDriveRequested);
    assert_eq!(drive_stats.enqueued, 0);
    assert_eq!(drive_stats.handled, 0);
}

#[test]
fn live_loop_backpressure_records_per_signal_latency_metrics() {
    let backpressure = LiveLoopBackpressure::default();

    backpressure.record_enqueued(LiveLoopSignalKind::Request);
    backpressure.record_handled(LiveLoopSignalKind::Request, Duration::from_micros(120));
    backpressure.record_handled(LiveLoopSignalKind::Request, Duration::from_micros(380));

    let snapshot = backpressure.snapshot();
    let request_stats = snapshot.signal_stats(LiveLoopSignalKind::Request);
    assert_eq!(request_stats.enqueued, 1);
    assert_eq!(request_stats.handled, 2);
    assert_eq!(request_stats.avg_handle_us, 250);
    assert_eq!(request_stats.max_handle_us, 380);
    assert!(snapshot.has_activity());

    let summary = format_live_loop_signal_stats(&snapshot);
    assert!(summary.contains("request={in:1, handled:2, avg_us:250, max_us:380}"));
}

#[test]
fn consensus_commit_signal_thread_emits_on_committed_batches() {
    let node_config = NodeConfig::new(
        "node-live-commit-signal",
        "live-minimal",
        NodeRole::Sequencer,
    )
    .expect("node config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("node tick interval");
    let mut node_runtime = NodeRuntime::new(node_config).with_execution_hook(TestNoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    let shared_runtime = Arc::new(Mutex::new(node_runtime));
    let committed_batches = {
        let runtime = shared_runtime.lock().expect("lock node runtime");
        runtime.committed_action_batches_handle()
    };

    let (tx, rx) = mpsc::sync_channel(8);
    let loop_running = Arc::new(AtomicBool::new(true));
    let signal_queued = Arc::new(AtomicBool::new(false));
    let backpressure = Arc::new(LiveLoopBackpressure::default());
    let signal_loop_flag = Arc::clone(&loop_running);
    let signal_queued_flag = Arc::clone(&signal_queued);
    let signal_backpressure = Arc::clone(&backpressure);
    let signal_thread = thread::spawn(move || {
        emit_consensus_commit_signals(
            tx,
            signal_loop_flag,
            committed_batches,
            signal_queued_flag,
            signal_backpressure,
        )
    });

    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new_with_consensus_gate(
        config,
        init,
        ViewerLiveDecisionMode::Script,
        None,
        Some(Arc::clone(&shared_runtime)),
    )
    .expect("init world");
    let submit = world.step().expect("submit consensus action");
    assert!(submit.event.is_none());

    let signal = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("should receive consensus committed signal");
    assert!(matches!(signal, LiveLoopSignal::ConsensusCommitted));

    loop_running.store(false, Ordering::SeqCst);
    signal_thread.join().expect("signal thread exits");

    let mut runtime = shared_runtime.lock().expect("lock node runtime");
    runtime.stop().expect("stop node runtime");
}
