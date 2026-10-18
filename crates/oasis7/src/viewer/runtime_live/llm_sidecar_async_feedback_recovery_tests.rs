//! Async provider feedback-history restoration and recovery admission proofs.
use super::*;
use crate::simulator::{AsyncAgentRunner, Digest32, FeedbackEnvelopeV1};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct ProviderEnvSnapshot(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl ProviderEnvSnapshot {
    fn clear() -> Self {
        let keys = [
            VIEWER_AGENT_DECISION_SOURCE_ENV,
            VIEWER_AGENT_PROVIDER_BACKEND_ENV,
            VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
            VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
            VIEWER_AGENT_PROVIDER_URL_ENV,
            VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
            VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
            VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
            VIEWER_AGENT_PROVIDER_PROFILE_ENV,
            VIEWER_AGENT_EXECUTION_LANE_ENV,
            VIEWER_AGENT_PROVIDER_MODE_ENV,
        ];
        let previous = keys
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect();
        for key in keys {
            // SAFETY: The caller holds the canonical provider environment lock.
            unsafe { oasis7::env_mut::remove_var(key) };
        }
        Self(previous)
    }
}

impl Drop for ProviderEnvSnapshot {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            // SAFETY: The canonical provider environment lock remains held through drop.
            unsafe {
                match value {
                    Some(value) => oasis7::env_mut::set_var(key, value),
                    None => oasis7::env_mut::remove_var(key),
                }
            }
        }
    }
}

fn recovery_feedback(agent: &str, session: &str, seq: u64, id: &str) -> FeedbackEnvelopeV1 {
    FeedbackEnvelopeV1 {
        feedback_id: id.to_string(),
        feedback_seq: seq,
        agent_subject: agent.to_string(),
        agent_session_id: session.to_string(),
        agent_turn_id: format!("terminal-turn-{seq}"),
        decision_request_id: format!("terminal-request-{seq}"),
        candidate_action_id: None,
        runtime_receipt_id: None,
        status: "rejected".to_string(),
        request_digest: Digest32(format!("blake3:{}", "a".repeat(64))),
        reject_reason: Some("stale_base".to_string()),
        provenance: "runtime_authoritative".to_string(),
    }
}

fn enqueue_acknowledged_feedback(world: &mut RuntimeWorld, feedback: FeedbackEnvelopeV1) {
    let feedback_id = feedback.feedback_id.clone();
    world
        .enqueue_runtime_feedback(crate::simulator::agent_api_compat::feedback_to_api(
            &feedback,
        ))
        .expect("Runtime stores feedback history");
    world
        .claim_runtime_feedback(&feedback_id)
        .expect("Runtime claims feedback for delivery");
    world
        .ack_runtime_feedback(&feedback_id)
        .expect("Runtime retains acknowledged feedback history");
}

fn register_mock_provider(
    runner: &mut AsyncAgentRunner,
    agent: &str,
) -> Arc<Mutex<crate::simulator::MockDecisionProviderState>> {
    let provider = crate::simulator::MockDecisionProvider::new(format!("{agent}-provider"));
    let state = provider.shared_state();
    let behavior = ProviderBackedAgentBehavior::new_legacy_compatibility(
        agent,
        provider,
        vec![ActionCatalogEntry::new("wait", "wait")],
    );
    runner.register(behavior).expect("register provider actor");
    state
}

#[test]
fn async_poll_delivers_completed_result_then_skips_only_fenced_session() {
    let _env_lock = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .expect("provider env lock");
    let _env_snapshot = ProviderEnvSnapshot::clear();
    // SAFETY: The test owns the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }

    let blocked_agent = "agent-a-blocked";
    let allowed_agent = "agent-z-allowed";
    let world_id = "feedback-admission-fence-world";
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime(
            world_id,
            "feedback-admission-fence-branch",
            0,
            None,
            "pending",
            0,
        )
        .expect("bind Runtime cognition");
    for agent in [blocked_agent, allowed_agent] {
        world.submit_action(RuntimeAction::RegisterAgent {
            agent_id: agent.to_string(),
            pos: GeoPos::new(0, 0, 0),
        });
    }
    world.step().expect("register test agents");
    for agent in [blocked_agent, allowed_agent] {
        world
            .install_test_provider_capability_fixture(agent)
            .expect("install native candidate capability");
    }

    let mut runner = AsyncAgentRunner::with_default_capacity();
    let blocked_state = register_mock_provider(&mut runner, blocked_agent);
    let allowed_state = register_mock_provider(&mut runner, allowed_agent);
    runner
        .start_turn(blocked_agent)
        .expect("start real legacy provider turn to exercise completion polling");
    let completion_deadline = Instant::now() + Duration::from_secs(2);
    while blocked_state
        .lock()
        .expect("blocked provider state lock")
        .recorded_requests
        .is_empty()
        && Instant::now() < completion_deadline
    {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        blocked_state
            .lock()
            .expect("blocked provider state lock")
            .recorded_requests
            .len(),
        1,
        "the fixture must create an actual provider completion to poll"
    );
    std::thread::sleep(Duration::from_millis(20));

    let checkpoint_path = std::env::temp_dir().join(format!(
        "oasis7-feedback-admission-fence-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.configure_provider_lineage_store(checkpoint_path.clone());
    sidecar.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    sidecar.provider_agent_ids.insert(blocked_agent.to_string());
    sidecar.provider_agent_ids.insert(allowed_agent.to_string());
    sidecar
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync provider shadow kernel");
    let mut kernel = sidecar
        .shadow_kernel
        .take()
        .expect("provider shadow kernel");
    sidecar
        .prepare_provider_request_contexts(&mut world, &mut kernel, world_id)
        .expect("prepare current Agent/session identities");
    let blocked_session = sidecar
        .provider_contexts
        .get(blocked_agent)
        .expect("blocked Agent request context")
        .request_context
        .agent_session_id
        .clone();
    for feedback in [
        recovery_feedback(blocked_agent, &blocked_session, 1, "blocked-feedback-1"),
        recovery_feedback(blocked_agent, &blocked_session, 3, "blocked-feedback-3"),
    ] {
        enqueue_acknowledged_feedback(&mut world, feedback);
    }
    sidecar.provider_contexts.remove(blocked_agent);
    sidecar.shadow_kernel = Some(kernel);
    let runner = sidecar
        .runner
        .as_ref()
        .and_then(RuntimeDecisionRunner::async_runner)
        .expect("native provider runner");
    assert_eq!(runner.active_turn_count(), 1);
    assert!(runner.needs_runtime_feedback_recovery());

    sidecar.llm_decision_mailbox = 1;
    let legacy_completion = sidecar
        .next_llm_decision(&mut world, &WorldConfig::default(), world_id)
        .expect("the active compatibility turn completion remains deliverable");
    assert_eq!(
        legacy_completion.agent_id, blocked_agent,
        "an active compatibility turn defers recovery without losing its completion"
    );
    let runner = sidecar
        .runner
        .as_ref()
        .and_then(RuntimeDecisionRunner::async_runner)
        .expect("native provider runner");
    assert!(
        runner.needs_runtime_feedback_recovery(),
        "compatibility admission must not initialize production recovery"
    );
    assert!(
        world
            .cognition_economy()
            .expect("Runtime cognition economy remains readable")
            .leases
            .is_empty(),
        "missing feedback history must fence production before reserving a Runtime lease"
    );
    assert_eq!(
        sidecar
            .runner
            .as_ref()
            .and_then(RuntimeDecisionRunner::async_runner)
            .expect("native provider runner")
            .active_turn_count(),
        0,
        "polling the compatibility completion must release the active turn"
    );
    let runner = sidecar
        .runner
        .as_ref()
        .and_then(RuntimeDecisionRunner::async_runner)
        .expect("native provider runner");
    assert_eq!(runner.active_turn_count(), 0);
    assert!(
        runner.needs_runtime_feedback_recovery(),
        "compatibility completion cannot satisfy production history recovery"
    );

    sidecar.llm_decision_mailbox = 1;
    assert!(
        sidecar
            .next_llm_decision(&mut world, &WorldConfig::default(), world_id)
            .is_none(),
        "the allowed Agent should start asynchronously after the blocked candidate is skipped"
    );
    let runner = sidecar
        .runner
        .as_ref()
        .and_then(RuntimeDecisionRunner::async_runner)
        .expect("native provider runner");
    assert!(runner.runtime_feedback_history_loaded());
    assert!(
        runner.needs_runtime_feedback_recovery(),
        "a validated but gapped history remains refreshable without globally blocking siblings"
    );
    assert!(
        runner.feedback_recovery_blocked(blocked_agent, blocked_session.as_str()),
        "the real RuntimeWorld authority must restore the exact gapped session fence"
    );
    let allowed_deadline = Instant::now() + Duration::from_secs(2);
    while allowed_state
        .lock()
        .expect("allowed provider state lock")
        .recorded_requests
        .is_empty()
        && Instant::now() < allowed_deadline
    {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        allowed_state
            .lock()
            .expect("allowed provider state lock")
            .recorded_requests
            .len(),
        1,
        "the healthy sibling Agent must dispatch through the native candidate loop"
    );
    assert_eq!(
        blocked_state
            .lock()
            .expect("blocked provider state lock")
            .recorded_requests
            .len(),
        1,
        "the exact blocked Agent/session must issue no fresh provider request"
    );
    let _ = std::fs::remove_file(checkpoint_path);
}
