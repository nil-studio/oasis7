use super::auth_actions::{
    MockHttpResponse, RecordedHttpRequest, provider_context_response,
    spawn_runtime_live_mock_http_server,
};
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn wait_for_provider_phase(
    label: &str,
    timeout: Duration,
    mut poll: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if poll()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("{label} did not complete within {timeout:?}"));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn restart_provider_server(
    config: ViewerRuntimeLiveServerConfig,
    world: RuntimeWorld,
    lineage_path: &std::path::Path,
) -> ViewerRuntimeLiveServer {
    // A restart must restore the persisted sidecar only after the authoritative
    // Runtime world is loaded. The bootstrap Minimal world intentionally has
    // no provider capability identity and must never be used for payer lookup.
    let mut server = ViewerRuntimeLiveServer::new(config).expect("restarted Runtime live server");
    server.world = world;
    server
        .llm_sidecar
        .configure_provider_lineage_store(lineage_path.to_path_buf());
    server
        .llm_sidecar
        .restore_provider_lineage(&server.world)
        .expect("restore provider lineage against authoritative Runtime world");
    server
}

fn assert_provider_wait_post_admission_fault_is_compensated(
    fault_name: &str,
    fault_message: &str,
    expect_recovery_fence: bool,
) {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let base_url = spawn_runtime_live_mock_http_server(4, {
        let recorded = Arc::clone(&recorded);
        move |request| {
            recorded
                .lock()
                .expect("recorded lock")
                .push(request.clone());
            if request.path == "/v1/world-simulator/feedback-context" {
                return MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true}).to_string(),
                };
            }
            let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                serde_json::from_slice(request.body.as_slice())
                    .expect("decode provider decision request");
            let response = crate::simulator::DecisionResponse {
                decision: crate::simulator::ProviderDecision::Wait,
                module_command: None,
                provider_error: None,
                diagnostics: crate::simulator::ProviderDiagnostics::default(),
                trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                memory_write_intents: Vec::new(),
            };
            MockHttpResponse {
                status_code: 200,
                body: serde_json::to_string(&provider_context_response(&decoded, response))
                    .expect("encode provider response"),
            }
        }
    });
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url);
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    let world_id = format!("wait-compensation-{fault_name}");
    let finality_block_hash =
        crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &world_id).to_string();
    let lineage_path = std::env::temp_dir().join(format!(
        "oasis7-runtime-live-provider-wait-compensation-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let runtime_config = || {
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_test_cognition_runtime_binding(
                "continuation-branch",
                0,
                Some(finality_block_hash.clone()),
                "verified",
                0,
            )
    };
    let mut server = ViewerRuntimeLiveServer::new(
        runtime_config().with_provider_lineage_store(lineage_path.clone()),
    )
    .expect("runtime server");
    server.world = server.world.clone().with_cognition_scheduler(
        serde_json::from_value(serde_json::json!({
            "schema_version": "scheduler-policy.v1",
            "max_total_wakes_per_tick": 8,
            "max_wakes_per_agent_per_tick": 1,
            "aging_after_ticks": 2,
            "max_starvation_ticks": 4,
            "initial_priority": 0,
            "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
            "service_order": "stable_round_robin"
        }))
        .expect("decode continuation scheduler policy"),
        8,
    );
    server
        .world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime provider capability fixture");
    // SAFETY: This test holds the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::set_var("OASIS7_TEST_PROVIDER_WAIT_FAULT", fault_name);
    }
    wait_for_provider_phase(
        "faulted provider Wait admission",
        Duration::from_secs(5),
        || {
            server.llm_sidecar.request_decision();
            match server.enqueue_llm_action_from_sidecar() {
                Err(trace)
                    if trace
                        .llm_error
                        .as_deref()
                        .is_some_and(|error| error.contains(fault_message)) =>
                {
                    Ok(true)
                }
                Ok(Some(trace)) => Err(format!(
                    "faulted provider Wait unexpectedly succeeded: {trace:?}"
                )),
                Ok(None) => Ok(false),
                Err(trace) => Err(format!("unexpected provider Wait fault: {trace:?}")),
            }
        },
    )
    .expect("fault injection must reach post-admission error path");

    let checkpoint_path = if fault_name == "persistence" {
        let blocked_backup =
            lineage_path.with_extension(format!("blocked-backup-{}", std::process::id()));
        assert!(
            lineage_path.is_dir(),
            "real persistence fault must leave the checkpoint path blocked"
        );
        assert!(
            blocked_backup.is_file(),
            "real persistence fault must retain the previous checkpoint"
        );
        blocked_backup
    } else {
        lineage_path.clone()
    };
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&checkpoint_path).expect("read compensated provider lineage checkpoint"),
    )
    .expect("decode compensated provider lineage checkpoint");
    if expect_recovery_fence {
        for field in [
            "provider_continuation_proposals",
            "provider_active_turns",
            "provider_contexts",
        ] {
            assert!(
                checkpoint[field]
                    .as_object()
                    .is_some_and(|values| !values.is_empty()),
                "failed compensation must retain {field}: {checkpoint:?}"
            );
        }
        if fault_name == "persistence" {
            assert!(
                server.llm_sidecar.has_provider_wait_recovery("agent-0"),
                "real checkpoint failure must retain an in-memory recovery fence"
            );
        } else {
            assert!(
                checkpoint["provider_continuation_recovery_pending"]
                    .as_object()
                    .is_some_and(|values| values.contains_key("agent-0")),
                "failed compensation must persist an inspectable recovery fence: {checkpoint:?}"
            );
        }
    } else {
        for field in [
            "provider_continuation_proposals",
            "provider_active_turns",
            "provider_contexts",
            "provider_held_decisions",
            "provider_wait_until",
        ] {
            assert!(
                checkpoint[field]
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty),
                "compensation must clear {field}"
            );
        }
        assert!(
            server
                .world
                .cognition_in_flight_wakes()
                .expect("Runtime in-flight wakes")
                .is_empty(),
            "compensation must not leave a Runtime wake for compatibility handling"
        );
        assert!(
            server.world.cognition()["continuations"]
                .as_array()
                .is_some_and(|continuations| {
                    continuations.iter().all(|continuation| {
                        matches!(
                            continuation["status"].as_str(),
                            Some("rejected")
                                | Some("cancelled")
                                | Some("invalidated")
                                | Some("expired")
                        )
                    })
                })
        );
    }
    let decision_requests = recorded
        .lock()
        .expect("recorded requests")
        .iter()
        .filter(|request| request.path == "/v1/world-simulator/decision-context")
        .count();
    assert_eq!(
        decision_requests, 1,
        "compensation must not schedule another provider call"
    );

    // SAFETY: This test holds the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::remove_var("OASIS7_TEST_PROVIDER_WAIT_FAULT");
    }
    if expect_recovery_fence {
        if fault_name == "persistence" {
            let blocked_backup =
                lineage_path.with_extension(format!("blocked-backup-{}", std::process::id()));
            std::fs::remove_dir(&lineage_path)
                .expect("remove persistence fault checkpoint blocker before reload");
            std::fs::rename(&blocked_backup, &lineage_path)
                .expect("restore checkpoint after persistence fault");
            assert!(
                server
                    .llm_sidecar
                    .retry_provider_wait_compensation(&mut server.world)
                    .expect("retry real persistence fault after blocker removal"),
                "real persistence fault must be retryable on the same server"
            );
        }
        let mut restarted =
            restart_provider_server(runtime_config(), server.world.clone(), &lineage_path);
        wait_for_provider_phase(
            "provider Wait recovery after restore",
            Duration::from_secs(5),
            || {
                restarted.llm_sidecar.request_decision();
                let _ = restarted.enqueue_llm_action_from_sidecar();
                let checkpoint: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(&lineage_path)
                        .expect("read provider lineage checkpoint after restore"),
                )
                .expect("decode provider lineage checkpoint after restore");
                Ok(checkpoint["provider_continuation_recovery_pending"]
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty))
            },
        )
        .expect("provider Wait recovery must replay after restore");
        let replayed_checkpoint: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&lineage_path).expect("read replayed provider lineage checkpoint"),
        )
        .expect("decode replayed provider lineage checkpoint");
        assert!(
            replayed_checkpoint["provider_continuation_recovery_pending"]
                .as_object()
                .is_some_and(serde_json::Map::is_empty),
            "successful recovery replay must clear the recovery fence: {replayed_checkpoint:?}"
        );
        assert!(
            restarted
                .world
                .cognition_in_flight_wakes()
                .expect("Runtime in-flight wakes after recovery")
                .is_empty(),
            "successful recovery replay must not leave a Runtime wake in flight"
        );
        wait_for_provider_phase(
            "provider Wait later admission after recovery",
            Duration::from_secs(5),
            || {
                restarted.llm_sidecar.request_decision();
                let _ = restarted.enqueue_llm_action_from_sidecar();
                let requests = recorded
                    .lock()
                    .expect("recorded requests")
                    .iter()
                    .filter(|request| request.path == "/v1/world-simulator/decision-context")
                    .count();
                let checkpoint: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(&lineage_path)
                        .expect("read provider lineage after later admission"),
                )
                .expect("decode provider lineage after later admission");
                Ok(requests >= 2
                    && checkpoint["provider_continuation_proposals"]
                        .as_object()
                        .is_some_and(|values| !values.is_empty())
                    && checkpoint["provider_active_turns"]
                        .as_object()
                        .is_some_and(serde_json::Map::is_empty)
                    && checkpoint["provider_contexts"]
                        .as_object()
                        .is_some_and(serde_json::Map::is_empty))
            },
        )
        .expect("provider Wait recovery must allow one later admission");
        let requests = recorded
            .lock()
            .expect("recorded requests after recovery")
            .iter()
            .filter(|request| request.path == "/v1/world-simulator/decision-context")
            .count();
        assert_eq!(
            requests, 2,
            "recovery must admit exactly one later provider request"
        );
    }
    let _ = std::fs::remove_file(&lineage_path);
    clear_runtime_provider_env();
}

#[test]
fn runtime_provider_wait_projection_fault_is_compensated_without_double_schedule() {
    assert_provider_wait_post_admission_fault_is_compensated(
        "projection",
        "provider Wait Runtime projection compensation failure",
        false,
    );
}

#[test]
fn runtime_provider_wait_release_fault_is_compensated_without_double_schedule() {
    assert_provider_wait_post_admission_fault_is_compensated(
        "release",
        "provider Wait actor turn release failed",
        false,
    );
}

#[test]
fn runtime_provider_wait_compensation_faults_retain_identity_for_replay() {
    for (fault_name, fault_message) in [
        ("runtime", "Runtime rejection returned error"),
        ("harness", "Harness validation returned error"),
        ("actor", "actor turn cleanup failed"),
        (
            "persistence",
            "provider Wait lineage cleanup persistence failed",
        ),
    ] {
        assert_provider_wait_post_admission_fault_is_compensated(fault_name, fault_message, true);
    }
}

#[test]
fn runtime_provider_wait_real_checkpoint_blocker_retries_same_server_then_reload() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let base_url = spawn_runtime_live_mock_http_server(4, {
        let recorded = Arc::clone(&recorded);
        move |request| {
            recorded
                .lock()
                .expect("recorded lock")
                .push(request.clone());
            if request.path == "/v1/world-simulator/feedback-context" {
                return MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true}).to_string(),
                };
            }
            let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                serde_json::from_slice(request.body.as_slice())
                    .expect("decode provider decision request");
            let response = crate::simulator::DecisionResponse {
                decision: crate::simulator::ProviderDecision::Wait,
                module_command: None,
                provider_error: None,
                diagnostics: crate::simulator::ProviderDiagnostics::default(),
                trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                memory_write_intents: Vec::new(),
            };
            MockHttpResponse {
                status_code: 200,
                body: serde_json::to_string(&provider_context_response(&decoded, response))
                    .expect("encode provider response"),
            }
        }
    });
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url);
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
        oasis7::env_mut::set_var("OASIS7_TEST_PROVIDER_WAIT_FAULT", "persistence_blocker");
    }
    let world_id = "wait-real-checkpoint-blocker";
    let finality_block_hash =
        crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &world_id).to_string();
    let lineage_path = std::env::temp_dir().join(format!(
        "oasis7-runtime-live-provider-wait-real-blocker-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let blocked_backup =
        lineage_path.with_extension(format!("blocked-backup-{}", std::process::id()));
    let runtime_config = || {
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_test_cognition_runtime_binding(
                "continuation-branch",
                0,
                Some(finality_block_hash.clone()),
                "verified",
                0,
            )
    };
    let mut server = ViewerRuntimeLiveServer::new(
        runtime_config().with_provider_lineage_store(lineage_path.clone()),
    )
    .expect("runtime server");
    server.world = server.world.clone().with_cognition_scheduler(
        serde_json::from_value(serde_json::json!({
            "schema_version": "scheduler-policy.v1",
            "max_total_wakes_per_tick": 8,
            "max_wakes_per_agent_per_tick": 1,
            "aging_after_ticks": 2,
            "max_starvation_ticks": 4,
            "initial_priority": 0,
            "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
            "service_order": "stable_round_robin"
        }))
        .expect("decode continuation scheduler policy"),
        8,
    );
    server
        .world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime provider capability fixture");

    wait_for_provider_phase(
        "real checkpoint blocker compensation",
        Duration::from_secs(5),
        || {
            server.llm_sidecar.request_decision();
            match server.enqueue_llm_action_from_sidecar() {
                Err(trace)
                    if trace.llm_error.as_deref().is_some_and(|error| {
                        error.contains("provider Wait compensation incomplete")
                    }) =>
                {
                    Ok(true)
                }
                Ok(Some(trace)) => Err(format!(
                    "real checkpoint blocker unexpectedly succeeded: {trace:?}"
                )),
                Ok(None) => Ok(false),
                Err(trace) => Err(format!("unexpected checkpoint blocker trace: {trace:?}")),
            }
        },
    )
    .expect("real checkpoint blocker must reach compensation persistence failure");
    assert!(
        lineage_path.is_dir(),
        "the real checkpoint blocker must be active"
    );
    assert!(
        blocked_backup.is_file(),
        "the original checkpoint must be retained"
    );
    assert!(
        server.llm_sidecar.has_provider_wait_recovery("agent-0"),
        "the same server must retain a recovery fence after checkpoint failure"
    );
    assert!(
        !server.llm_sidecar.provider_contexts_empty(),
        "the same server must retain provider identity after checkpoint failure"
    );
    let decision_requests = || {
        recorded
            .lock()
            .expect("recorded requests")
            .iter()
            .filter(|request| request.path == "/v1/world-simulator/decision-context")
            .count()
    };
    assert_eq!(decision_requests(), 1);

    // A second real control pass must retry the failed cleanup while the
    // blocker remains active, without readmitting the provider request.
    let second = server
        .enqueue_llm_action_from_sidecar()
        .expect_err("blocked recovery must remain fenced on the second control pass");
    assert!(
        second
            .llm_error
            .as_deref()
            .is_some_and(|error| error.contains("recovery remains pending")),
        "second control pass must report the durable recovery fence: {second:?}"
    );
    assert_eq!(decision_requests(), 1);
    assert!(server.llm_sidecar.has_provider_wait_recovery("agent-0"));

    // Remove the filesystem blocker and let the same server commit cleanup.
    unsafe {
        oasis7::env_mut::remove_var("OASIS7_TEST_PROVIDER_WAIT_FAULT");
    }
    std::fs::remove_dir(&lineage_path).expect("remove checkpoint blocker directory");
    assert!(
        server
            .llm_sidecar
            .retry_provider_wait_compensation(&mut server.world)
            .expect("retry real checkpoint cleanup")
    );
    let cleaned: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&lineage_path).expect("read cleaned checkpoint after same-server retry"),
    )
    .expect("decode cleaned checkpoint after same-server retry");
    assert!(
        cleaned["provider_continuation_recovery_pending"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    );
    assert!(
        cleaned["provider_continuation_proposals"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    );
    assert_eq!(decision_requests(), 1);

    // Reload the persisted cleanup before allowing a later fresh provider turn.
    let mut restarted =
        restart_provider_server(runtime_config(), server.world.clone(), &lineage_path);
    assert!(
        !restarted.llm_sidecar.has_provider_wait_recovery("agent-0"),
        "reload must observe persisted compensation cleanup"
    );
    assert_eq!(
        decision_requests(),
        1,
        "reload must not readmit the old request"
    );
    wait_for_provider_phase(
        "later fresh provider Wait admission after real cleanup",
        Duration::from_secs(5),
        || {
            restarted.llm_sidecar.request_decision();
            let _ = restarted.enqueue_llm_action_from_sidecar();
            let checkpoint: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&lineage_path).expect("read checkpoint after later admission"),
            )
            .expect("decode checkpoint after later admission");
            Ok(decision_requests() >= 2
                && checkpoint["provider_continuation_proposals"]
                    .as_object()
                    .is_some_and(|values| !values.is_empty())
                && checkpoint["provider_active_turns"]
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty)
                && checkpoint["provider_contexts"]
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty))
        },
    )
    .expect("later fresh provider admission must succeed exactly once");
    assert_eq!(
        decision_requests(),
        2,
        "real checkpoint recovery must permit exactly one later provider request"
    );
    let _ = std::fs::remove_file(&lineage_path);
    let _ = std::fs::remove_file(&blocked_backup);
    clear_runtime_provider_env();
}

#[test]
fn runtime_provider_feedback_delivery_failure_survives_reload_without_readmission() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let feedback_available = Arc::new(AtomicBool::new(false));
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    // Failed feedback is retried on each control pass. Keep enough listener
    // capacity for those retries plus the post-reload fresh decision request.
    let base_url = spawn_runtime_live_mock_http_server(16, {
        let feedback_available = Arc::clone(&feedback_available);
        let recorded = Arc::clone(&recorded);
        move |request| {
            recorded
                .lock()
                .expect("recorded lock")
                .push(request.clone());
            if request.path == "/v1/world-simulator/feedback-context" {
                return if feedback_available.load(Ordering::Acquire) {
                    MockHttpResponse {
                        status_code: 200,
                        body: serde_json::json!({"ok": true}).to_string(),
                    }
                } else {
                    MockHttpResponse {
                        status_code: 503,
                        body: serde_json::json!({"ok": false, "error": "feedback_down"})
                            .to_string(),
                    }
                };
            }
            if !feedback_available.load(Ordering::Acquire) {
                return MockHttpResponse {
                    status_code: 503,
                    body: serde_json::json!({"ok": false, "error": "decision_down"}).to_string(),
                };
            }
            let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                serde_json::from_slice(request.body.as_slice())
                    .expect("decode recovered provider decision request");
            let response = crate::simulator::DecisionResponse {
                decision: crate::simulator::ProviderDecision::Wait,
                module_command: None,
                provider_error: None,
                diagnostics: crate::simulator::ProviderDiagnostics::default(),
                trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                memory_write_intents: Vec::new(),
            };
            MockHttpResponse {
                status_code: 200,
                body: serde_json::to_string(&provider_context_response(&decoded, response))
                    .expect("encode recovered provider response"),
            }
        }
    });
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url);
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    let world_id = "feedback-delivery-recovery";
    let finality_block_hash =
        crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &world_id).to_string();
    let lineage_path = std::env::temp_dir().join(format!(
        "oasis7-runtime-live-provider-feedback-failure-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let runtime_config = || {
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_test_cognition_runtime_binding(
                "continuation-branch",
                0,
                Some(finality_block_hash.clone()),
                "verified",
                0,
            )
    };
    let mut server = ViewerRuntimeLiveServer::new(
        runtime_config().with_provider_lineage_store(lineage_path.clone()),
    )
    .expect("runtime server");
    server
        .world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime provider capability fixture");

    wait_for_provider_phase(
        "provider transport exhaustion with feedback delivery failure",
        Duration::from_secs(5),
        || {
            server.llm_sidecar.request_decision();
            match server.enqueue_llm_action_from_sidecar() {
                Err(trace)
                    if trace
                        .llm_error
                        .as_deref()
                        .is_some_and(|error| error.contains("failed_provider")) =>
                {
                    Ok(true)
                }
                Err(trace)
                    if trace
                        .llm_error
                        .as_deref()
                        .is_some_and(|error| error.contains("provider_http_503")) =>
                {
                    Ok(false)
                }
                Ok(Some(trace)) => Err(format!(
                    "provider transport failure unexpectedly succeeded: {trace:?}"
                )),
                Ok(None) => Ok(false),
                Err(trace) => Err(format!("unexpected provider transport trace: {trace:?}")),
            }
        },
    )
    .expect("provider transport must reach terminal feedback path");
    let decision_requests = || {
        recorded
            .lock()
            .expect("recorded requests")
            .iter()
            .filter(|request| request.path == "/v1/world-simulator/decision-context")
            .count()
    };
    let feedback_requests = || {
        recorded
            .lock()
            .expect("recorded requests")
            .iter()
            .filter(|request| request.path == "/v1/world-simulator/feedback-context")
            .count()
    };
    let exhausted_requests = decision_requests();
    assert!(
        exhausted_requests >= 2,
        "transport exhaustion must issue bounded provider attempts"
    );
    assert!(
        feedback_requests() >= 1,
        "terminal feedback must be attempted"
    );
    assert_eq!(
        server.world.pending_runtime_feedback().unwrap().len(),
        1,
        "failed feedback delivery must remain in the Runtime outbox"
    );
    assert_eq!(
        server.world.cognition_in_flight_wakes().unwrap().len(),
        0,
        "feedback transport failure must not leave a Runtime wake in flight"
    );
    let exhausted_economy = server
        .world
        .cognition_economy()
        .expect("read economy after exhausted provider transport");
    assert!(
        exhausted_economy
            .leases
            .values()
            .all(|lease| lease.status == crate::runtime::CognitionLeaseStatusV1::Released),
        "an exhausted transport with no provider response must release its reserved unit"
    );
    assert_eq!(
        exhausted_economy
            .receipts
            .values()
            .filter(|receipt| receipt.operation == "release")
            .count(),
        exhausted_economy.leases.len(),
        "exhausted transport cleanup must emit one release receipt per lease"
    );

    // A second control pass retries only the pending feedback outbox record;
    // it cannot readmit the terminal provider request.
    let _ = server.enqueue_llm_action_from_sidecar();
    assert_eq!(
        decision_requests(),
        exhausted_requests,
        "feedback retry must not readmit the exhausted provider turn"
    );
    assert!(feedback_requests() >= 2);

    // Reload while the feedback record is still pending to model a crash
    // after Runtime queued the terminal disposition but before transport ack.
    let runtime_world_dir = std::env::temp_dir().join(format!(
        "oasis7-runtime-live-provider-feedback-world-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&runtime_world_dir).expect("create Runtime world snapshot directory");
    let runtime_snapshot = server.world.snapshot_with_chain_resource_context(
        crate::runtime::ChainResourceDerivationContext {
            world_id: "live-runtime-minimal",
            chain_id: "runtime-chain",
            genesis_ref: None,
            created_at_height: server.world.journal().len() as u64,
            manifest_height: server.world.journal().len() as u64,
            commit_block_hash: None,
            tick: server.world.state().time,
        },
        "runtime-live-test-world-config",
        "runtime-live-test-generation",
    );
    runtime_snapshot
        .save_json(runtime_world_dir.join("snapshot.json"))
        .expect("persist Runtime snapshot with pending terminal feedback");
    server
        .world
        .journal()
        .save_json(runtime_world_dir.join("journal.json"))
        .expect("persist Runtime journal with pending terminal feedback");
    let persisted_world = crate::runtime_host::world::load_from_dir(&runtime_world_dir)
        .expect("restore Runtime world with pending terminal feedback");
    assert_eq!(
        persisted_world.pending_runtime_feedback().unwrap().len(),
        1,
        "persisted Runtime snapshot must retain the pending feedback outbox"
    );
    let mut restarted = restart_provider_server(runtime_config(), persisted_world, &lineage_path);
    let _ = restarted.enqueue_llm_action_from_sidecar();
    assert_eq!(restarted.world.pending_runtime_feedback().unwrap().len(), 1);
    assert_eq!(
        decision_requests(),
        exhausted_requests,
        "reload with pending feedback must not replay the terminal turn"
    );

    feedback_available.store(true, Ordering::Release);
    wait_for_provider_phase(
        "terminal feedback acknowledgement after reload",
        Duration::from_secs(5),
        || {
            let _ = restarted.enqueue_llm_action_from_sidecar();
            Ok(restarted
                .world
                .pending_runtime_feedback()
                .unwrap()
                .is_empty())
        },
    )
    .expect("feedback outbox must recover after transport restoration");
    assert_eq!(
        decision_requests(),
        exhausted_requests,
        "feedback acknowledgement must not invoke the provider"
    );

    // A later explicit request is allowed to create one fresh provider turn.
    restarted.llm_sidecar.request_decision();
    wait_for_provider_phase(
        "later fresh provider request after feedback recovery",
        Duration::from_secs(5),
        || match restarted.enqueue_llm_action_from_sidecar() {
            Err(trace) => Err(format!("fresh provider admission trace: {trace:?}")),
            Ok(_) => Ok(decision_requests() > exhausted_requests),
        },
    )
    .expect("feedback recovery must allow a later fresh provider request");
    assert_eq!(
        decision_requests(),
        exhausted_requests + 1,
        "feedback recovery must admit exactly one later provider request"
    );
    let _ = std::fs::remove_file(&lineage_path);
    let _ = std::fs::remove_dir_all(&runtime_world_dir);
    clear_runtime_provider_env();
}

#[test]
fn runtime_provider_backed_wake_resumes_with_fresh_request_and_origin_lineage() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let decision_count = Arc::new(Mutex::new(0_usize));
    let base_url = spawn_runtime_live_mock_http_server(4, {
        let recorded = Arc::clone(&recorded);
        let decision_count = Arc::clone(&decision_count);
        move |request| {
            recorded
                .lock()
                .expect("recorded lock")
                .push(request.clone());
            if request.path == "/v1/world-simulator/feedback-context" {
                return MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true}).to_string(),
                };
            }
            if request.path != "/v1/world-simulator/decision-context" {
                return MockHttpResponse {
                    status_code: 404,
                    body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
                };
            }
            let request_number = {
                let mut count = decision_count.lock().expect("decision count lock");
                *count += 1;
                *count
            };
            let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                serde_json::from_slice(request.body.as_slice())
                    .expect("decode provider decision request");
            decoded
                .validate_production_lane()
                .expect("provider request must be production-valid");
            let response = crate::simulator::DecisionResponse {
                decision: if request_number == 1 {
                    crate::simulator::ProviderDecision::Wait
                } else {
                    crate::simulator::ProviderDecision::Act {
                        action_ref: "move_agent".to_string(),
                        action: oasis7_simulator::Action::MoveAgent {
                            agent_id: decoded.base_decision_request.observation.agent_id.clone(),
                            to: "runtime:0:0:0".to_string(),
                        },
                    }
                },
                module_command: None,
                provider_error: None,
                diagnostics: crate::simulator::ProviderDiagnostics::default(),
                trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                memory_write_intents: Vec::new(),
            };
            MockHttpResponse {
                status_code: 200,
                body: serde_json::to_string(&provider_context_response(&decoded, response))
                    .expect("encode provider response"),
            }
        }
    });
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url);
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    let world_id = format!("wake-runtime-{}", WorldScenario::Minimal.as_str());
    let finality_block_hash =
        crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &world_id).to_string();
    let lineage_path = std::env::temp_dir().join(format!(
        "oasis7-runtime-live-provider-restart-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let runtime_config = || {
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_test_cognition_runtime_binding(
                "continuation-branch",
                0,
                Some(finality_block_hash.clone()),
                "verified",
                0,
            )
    };
    let mut server = ViewerRuntimeLiveServer::new(
        runtime_config().with_provider_lineage_store(lineage_path.clone()),
    )
    .expect("runtime server");
    server.world = server.world.clone().with_cognition_scheduler(
        serde_json::from_value(serde_json::json!({
            "schema_version": "scheduler-policy.v1",
            "max_total_wakes_per_tick": 8,
            "max_wakes_per_agent_per_tick": 1,
            "aging_after_ticks": 2,
            "max_starvation_ticks": 4,
            "initial_priority": 0,
            "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
            "service_order": "stable_round_robin"
        }))
        .expect("decode continuation scheduler policy"),
        8,
    );
    server
        .world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime provider capability fixture");
    let mut wait_trace = None;
    wait_for_provider_phase("provider Wait admission", Duration::from_secs(5), || {
        server.llm_sidecar.request_decision();
        match server.enqueue_llm_action_from_sidecar() {
            Ok(Some(trace)) if matches!(trace.decision, crate::simulator::AgentDecision::Wait) => {
                wait_trace = Some(trace);
                Ok(true)
            }
            Ok(Some(_)) | Ok(None) => Ok(false),
            Err(trace) => Err(format!("provider Wait admission failed: {trace:?}")),
        }
    })
    .expect("ordinary provider Wait must be admitted through Harness and Runtime");
    assert!(wait_trace.is_some());
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&lineage_path).expect("provider lineage checkpoint after Wait"),
    )
    .expect("decode provider lineage checkpoint after Wait");
    assert_eq!(
        checkpoint["provider_continuation_proposals"]
            .as_object()
            .map(serde_json::Map::len),
        Some(1),
        "Wait must persist the exact Harness proposal before Runtime restart"
    );
    assert!(
        server.world.cognition()["continuations"]
            .as_array()
            .is_some_and(|continuations| continuations.iter().any(|value| {
                matches!(
                    value["status"].as_str(),
                    Some("scheduled") | Some("pending")
                )
            })),
        "provider Wait must leave a durable Runtime continuation: {}",
        server.world.cognition()
    );
    let wait_economy = server
        .world
        .cognition_economy()
        .expect("read cognition economy after provider Wait");
    assert_eq!(
        wait_economy.leases.len(),
        1,
        "the successful provider Wait must have one admitted cognition lease"
    );
    assert_eq!(
        wait_economy
            .leases
            .values()
            .next()
            .expect("provider Wait lease")
            .status,
        crate::runtime::CognitionLeaseStatusV1::Settled,
        "provider I/O completion must settle the fixed cognition unit"
    );
    assert_eq!(
        wait_economy
            .receipts
            .values()
            .filter(|receipt| receipt.operation == "settle")
            .count(),
        1,
        "provider Wait settlement must emit one terminal settle receipt"
    );
    server.world.step().expect("normal Runtime tick wakes Wait");
    let restarted_world = server.world.clone();
    let mut restarted = restart_provider_server(runtime_config(), restarted_world, &lineage_path);
    restarted
        .sync_runtime_wake_projection()
        .expect("mirror Runtime-selected wake into restarted Viewer");
    let mut action_trace = None;
    wait_for_provider_phase("provider wake resume", Duration::from_secs(5), || {
        restarted.llm_sidecar.request_decision();
        match restarted.enqueue_llm_action_from_sidecar() {
            Ok(Some(trace))
                if matches!(trace.decision, crate::simulator::AgentDecision::Act(_)) =>
            {
                action_trace = Some(trace);
                Ok(true)
            }
            Ok(Some(_)) | Ok(None) => Ok(false),
            Err(trace) => Err(format!("provider wake resume failed: {trace:?}")),
        }
    })
    .expect("Runtime wake must resume one fresh provider turn");
    assert!(action_trace.is_some());
    super::provider_continuation_drains::drain_final_continuation(&mut restarted)
        .expect("final continuation wake must terminate at Runtime budget exhaustion");
    let requests = recorded.lock().expect("recorded requests");
    let decisions = requests
        .iter()
        .filter(|request| request.path == "/v1/world-simulator/decision-context")
        .map(|request| {
            serde_json::from_slice::<crate::simulator::ContinuousAgentRequestContextV1>(
                request.body.as_slice(),
            )
            .expect("decode recorded provider request")
        })
        .collect::<Vec<_>>();
    assert_eq!(decisions.len(), 2);
    assert_ne!(
        decisions[0].continuation_digest,
        crate::simulator::Digest32::default(),
        "normal provider request must carry the canonical no-continuation sentinel"
    );
    assert_ne!(
        decisions[1].continuation_digest,
        crate::simulator::Digest32::default(),
        "resumed provider request must carry the Runtime-selected continuation"
    );
    assert_ne!(decisions[0].agent_turn_id, decisions[1].agent_turn_id);
    assert_ne!(decisions[0].agent_session_id, decisions[1].agent_session_id);
    let cognition = restarted.world.cognition();
    let events = cognition["cognition_journal"]["events"]
        .as_array()
        .expect("Runtime cognition journal events");
    assert!(
        events
            .iter()
            .any(|event| event["event_kind"] == "ContinuationScheduled"),
        "ordinary Wait must append Runtime continuation admission evidence"
    );
    assert!(
        events
            .iter()
            .any(|event| event["event_kind"] == "ContinuationReplanned"),
        "normal wake must append Runtime continuation replan evidence"
    );
    assert!(
        cognition["commit_records"]
            .as_array()
            .is_some_and(|records| records.iter().any(|record| record["status"] == "committed")),
        "resumed provider action must commit through Runtime"
    );
    drop(requests);
    restarted.llm_sidecar.request_decision();
    restarted
        .enqueue_llm_action_from_sidecar()
        .expect("ordinary next turn AgentBusy");
    let _ = std::fs::remove_file(&lineage_path);
    clear_runtime_provider_env();
    drop(_guard);
}
