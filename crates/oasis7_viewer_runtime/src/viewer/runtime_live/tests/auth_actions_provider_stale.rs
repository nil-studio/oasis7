use super::auth_actions::{
    MockHttpResponse, RecordedHttpRequest, provider_context_response,
    spawn_runtime_live_mock_http_server,
};
use super::*;
use std::sync::{Arc, Mutex};
#[test]
fn runtime_background_play_commits_provider_response_without_latency_stale_base() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let decision_count = Arc::new(Mutex::new(0_usize));
    // Keep capacity for the committed Act and replan Wait feedback. The
    // bounded listener must outlive any request already in flight while the
    // test observes the durable result.
    let base_url = spawn_runtime_live_mock_http_server(8, {
        let recorded = Arc::clone(&recorded);
        let decision_count = Arc::clone(&decision_count);
        move |request| {
            recorded
                .lock()
                .expect("recorded lock")
                .push(request.clone());
            match (request.method.as_str(), request.path.as_str()) {
                ("POST", "/v1/world-simulator/decision-context") => {
                    let request_number = {
                        let mut decision_count = decision_count.lock().expect("count lock");
                        *decision_count += 1;
                        *decision_count
                    };
                    let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                        serde_json::from_slice(request.body.as_slice())
                            .expect("decode outer decision request");
                    decoded
                        .validate_production_lane()
                        .expect("provider freshness test request must be complete");
                    let decision = if request_number == 1 {
                        crate::simulator::ProviderDecision::Act {
                            action_ref: "move_agent".to_string(),
                            action: oasis7_simulator::Action::MoveAgent {
                                agent_id: decoded
                                    .base_decision_request
                                    .observation
                                    .agent_id
                                    .clone(),
                                to: decoded
                                    .base_decision_request
                                    .observation
                                    .observation
                                    .self_state
                                    .location_ref
                                    .clone(),
                            },
                        }
                    } else {
                        crate::simulator::ProviderDecision::Wait
                    };
                    let response = crate::simulator::DecisionResponse {
                        decision,
                        module_command: None,
                        provider_error: None,
                        diagnostics: crate::simulator::ProviderDiagnostics::default(),
                        trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                        memory_write_intents: Vec::new(),
                    };
                    MockHttpResponse {
                        status_code: 200,
                        body: serde_json::to_string(&provider_context_response(&decoded, response))
                            .expect("encode decision response"),
                    }
                }
                ("POST", "/v1/world-simulator/feedback-context") => {
                    let feedback: crate::simulator::FeedbackEnvelopeV1 =
                        serde_json::from_slice(request.body.as_slice())
                            .expect("decode Runtime feedback");
                    if feedback.status != "pending" {
                        assert_eq!(
                            feedback.status, "committed",
                            "provider action should commit on the captured base: {feedback:?}"
                        );
                        assert!(feedback.runtime_receipt_id.is_some());
                    }
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
        }
    });
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url);
        // The loopback callback is served by a helper thread. Required-tier
        // runs can temporarily starve that thread while many Rust tests are
        // active; keep the mock request alive long enough to observe the
        // response instead of turning scheduler pressure into a gameplay
        // retry failure.
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV, "15000");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV, "15000");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }

    let test_world_id = format!("live-runtime-{}", WorldScenario::Minimal.as_str());
    let test_finality_block_hash =
        crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &test_world_id).to_string();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_test_cognition_runtime_binding(
                "main",
                0,
                Some(test_finality_block_hash),
                "verified",
                0,
            ),
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
    let (mut writer, _client) = test_writer_pair();
    let mut session = RuntimeLiveSession::new();
    session.playing = true;

    let read_progress = |world: &crate::runtime::World| {
        let recorded = recorded.lock().expect("recorded lock");
        let decisions = recorded
            .iter()
            .filter(|request| request.path == "/v1/world-simulator/decision-context")
            .count();
        let stale_feedback_seen = recorded
            .iter()
            .filter(|request| request.path == "/v1/world-simulator/feedback-context")
            .map(|request| {
                serde_json::from_slice::<crate::simulator::FeedbackEnvelopeV1>(
                    request.body.as_slice(),
                )
                .expect("decode feedback")
            })
            .any(|feedback| {
                feedback.status == "rejected"
                    && feedback.reject_reason.as_deref() == Some("stale_base")
            });
        let committed_feedback_seen = recorded
            .iter()
            .filter(|request| request.path == "/v1/world-simulator/feedback-context")
            .map(|request| {
                serde_json::from_slice::<crate::simulator::FeedbackEnvelopeV1>(
                    request.body.as_slice(),
                )
                .expect("decode committed feedback")
            })
            .any(|feedback| feedback.status == "committed");
        let wait_feedback_seen = recorded
            .iter()
            .filter(|request| request.path == "/v1/world-simulator/feedback-context")
            .map(|request| {
                serde_json::from_slice::<crate::simulator::FeedbackEnvelopeV1>(
                    request.body.as_slice(),
                )
                .expect("decode wait feedback")
            })
            .any(|feedback| {
                feedback.status == "pending"
                    && feedback.reject_reason.as_deref() == Some("retry_scheduled")
            });
        let wait_scheduled = world
            .cognition()
            .get("cognition_journal")
            .and_then(|journal| journal.get("events"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|events| {
                events.iter().any(|event| {
                    event.get("event_kind").and_then(serde_json::Value::as_str)
                        == Some("ContinuationScheduled")
                        && event.get("agent_id").and_then(serde_json::Value::as_str)
                            == Some("agent-0")
                })
            });
        (
            stale_feedback_seen,
            committed_feedback_seen,
            decisions >= 2,
            wait_feedback_seen,
            wait_scheduled,
        )
    };
    let poll_deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < poll_deadline {
        let (
            stale_feedback_seen,
            committed_feedback_seen,
            next_request_seen,
            wait_feedback_seen,
            _wait_scheduled,
        ) = read_progress(&server.world);
        if stale_feedback_seen {
            break;
        }
        if committed_feedback_seen && next_request_seen && wait_feedback_seen {
            break;
        }
        server
            .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
            .expect("provider response should be handled");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let (
        stale_feedback_seen,
        committed_feedback_seen,
        next_request_seen,
        _wait_feedback_seen,
        _wait_scheduled,
    ) = read_progress(&server.world);
    // No provider work remains after the bounded poll. Release the process
    // environment lock before inspecting the recorded evidence so a later
    // assertion failure cannot poison the shared test lock and cascade.
    clear_runtime_provider_env();
    drop(_guard);
    assert!(
        !stale_feedback_seen,
        "provider latency must not invalidate its captured Runtime base"
    );
    assert!(
        committed_feedback_seen,
        "provider action should produce committed feedback"
    );
    assert!(
        next_request_seen,
        "the next provider turn should be dispatched"
    );
    let recorded = recorded.lock().expect("recorded lock");
    let decisions: Vec<crate::simulator::ContinuousAgentRequestContextV1> = recorded
        .iter()
        .filter(|request| request.path == "/v1/world-simulator/decision-context")
        .map(|request| {
            serde_json::from_slice(request.body.as_slice()).expect("decode decision request")
        })
        .collect();
    assert_eq!(
        session.transient_play_failures,
        0,
        "provider polling left transient failures; decisions={}",
        decisions.len()
    );
    assert!(decisions.len() >= 2);
    assert_ne!(decisions[0].agent_turn_id, decisions[1].agent_turn_id);
    assert_ne!(
        decisions[0].decision_request_id,
        decisions[1].decision_request_id
    );
    assert_eq!(decisions[1].transport_attempt, 1);
    let recent_events = &decisions[1]
        .base_decision_request
        .observation
        .recent_event_summary;
    assert!(
        recent_events
            .iter()
            .all(|summary| !summary.contains("stale_base_replan")),
        "normal next turn must not carry a stale-base replan marker: {recent_events:?}"
    );
    let feedbacks: Vec<crate::simulator::FeedbackEnvelopeV1> = recorded
        .iter()
        .filter(|request| request.path == "/v1/world-simulator/feedback-context")
        .map(|request| serde_json::from_slice(request.body.as_slice()).expect("decode feedback"))
        .collect();
    assert!(
        feedbacks.iter().any(|feedback| {
            feedback.status == "pending"
                && feedback.reject_reason.as_deref() == Some("retry_scheduled")
        }),
        "a provider Wait turn must carry pending feedback: {feedbacks:?}"
    );
    assert!(
        server
            .world
            .cognition()
            .get("cognition_journal")
            .and_then(|journal| journal.get("events"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|events| {
                events.iter().any(|event| {
                    event.get("event_kind").and_then(serde_json::Value::as_str)
                        == Some("ContinuationScheduled")
                        && event.get("agent_id").and_then(serde_json::Value::as_str)
                            == Some("agent-0")
                })
            }),
        "a provider Wait turn must be durably scheduled by Runtime"
    );
    assert!(
        feedbacks.iter().all(|feedback| {
            feedback.reject_reason.as_deref()
                != Some("provider wait elapsed without a Runtime action")
        }),
        "provider Wait must not emit the legacy ad-hoc expiry reason: {feedbacks:?}"
    );
    assert!(!server.world.journal().events.iter().any(|event| matches!(
        event.body,
        crate::runtime::WorldEventBody::Domain(crate::runtime::DomainEvent::ActionAccepted { .. })
            | crate::runtime::WorldEventBody::EffectQueued(_)
            | crate::runtime::WorldEventBody::ReceiptAppended(_)
    )));
}

#[test]
fn exhausted_stale_replan_disposition_never_claims_a_queued_replacement() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .unwrap();
    let before = server.world.snapshot();
    for count in 1..=3 {
        let (reason, queued) = server.schedule_provider_stale_replan_disposition(
            "agent-0",
            &format!("turn-{count}"),
            &format!("request-{count}"),
        );
        assert_eq!(reason, "stale_base");
        assert!(queued);
        assert_eq!(
            server
                .llm_sidecar
                .provider_stale_replan_cause("agent-0")
                .unwrap()
                .parent_decision_request_id,
            format!("request-{count}")
        );
        server
            .llm_sidecar
            .mark_provider_stale_replan_dispatched("agent-0");
    }
    let (reason, queued) =
        server.schedule_provider_stale_replan_disposition("agent-0", "turn-4", "request-4");
    assert_eq!(reason, "provider_stale_replan_exhausted");
    assert!(!queued);
    assert!(
        server
            .llm_sidecar
            .provider_stale_replan_cause("agent-0")
            .is_none()
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert!(
        server
            .world
            .agent_causal_receipts("agent-0")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn live_stale_parent_correction_rebinds_only_after_real_rejected_terminal() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let _guard = runtime_provider_env_lock().lock().unwrap();
    clear_runtime_provider_env();
    let count = Arc::new(AtomicUsize::new(0));
    let waiting = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let records = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let url = spawn_runtime_live_mock_http_server(24, {
        let count = count.clone();
        let waiting = waiting.clone();
        let release = release.clone();
        let records = records.clone();
        move |request| {
            records.lock().unwrap().push(request.clone());
            if request.path == "/v1/world-simulator/decision-context" {
                let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                    serde_json::from_slice(&request.body).unwrap();
                let ordinal = count.fetch_add(1, Ordering::SeqCst) + 1;
                if ordinal == 3 {
                    waiting.store(true, Ordering::SeqCst);
                    let until = Instant::now() + Duration::from_secs(5);
                    while !release.load(Ordering::SeqCst) && Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
                let response = crate::simulator::DecisionResponse {
                    decision: crate::simulator::ProviderDecision::Act {
                        action_ref: "move_agent".into(),
                        action: oasis7_simulator::Action::MoveAgent {
                            agent_id: decoded.agent_subject.clone(),
                            to: decoded
                                .base_decision_request
                                .observation
                                .observation
                                .self_state
                                .location_ref
                                .clone(),
                        },
                    },
                    module_command: None,
                    provider_error: None,
                    diagnostics: Default::default(),
                    trace_payload: Default::default(),
                    memory_write_intents: if ordinal == 1 {
                        vec![crate::simulator::MemoryWriteIntent {
                            scope: "session_private".into(),
                            summary: "observation before stale parent".into(),
                            tags: vec![],
                        }]
                    } else {
                        vec![]
                    },
                };
                MockHttpResponse {
                    status_code: 200,
                    body: serde_json::to_string(&provider_context_response(&decoded, response))
                        .unwrap(),
                }
            } else {
                MockHttpResponse {
                    status_code: 200,
                    body: "{\"ok\":true}".into(),
                }
            }
        }
    });
    // SAFETY: shared provider environment lock serializes these variables.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, url);
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    let path = std::env::temp_dir().join(format!(
        "oasis7-live-stale-correction-{}.json",
        std::process::id()
    ));
    let world_id = format!("live-runtime-{}", WorldScenario::Minimal.as_str());
    let hash =
        crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &world_id).to_string();
    let config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
        .with_decision_mode(ViewerLiveDecisionMode::Llm)
        .with_provider_lineage_store(path.clone())
        .with_test_cognition_runtime_binding("stale-correction", 0, Some(hash), "verified", 0);
    let (mut server, agent_id, public_key, private_key) =
        super::agent_memory_correction_control::owner_server_with_config(99, config);
    super::provider_continuation_drains::install_cognition_scheduler(&mut server);
    server
        .world
        .install_test_provider_capability_fixture(&agent_id)
        .unwrap();
    super::agent_memory_correction_control::drive_provider_action(&mut server, false).unwrap();
    super::agent_memory_correction_control::drive_provider_action(&mut server, false).unwrap();
    let memory = server
        .llm_sidecar
        .agent_referenced_memory_context(&agent_id)
        .unwrap();
    server
        .llm_sidecar
        .correct_agent_memory(
            &agent_id,
            "live-stale-correction".into(),
            memory["sources"][0]["memory_id"].as_str().unwrap().into(),
            memory["revision"].as_u64().unwrap(),
            "corrected observation".into(),
        )
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !waiting.load(Ordering::SeqCst) {
        server.llm_sidecar.request_decision();
        let _ = server.enqueue_llm_action_from_sidecar();
        assert!(
            Instant::now() < until,
            "third provider request not dispatched"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let receipts_before = server.world.agent_causal_receipts(&agent_id).unwrap().len();
    server.world.step().unwrap();
    // Fail the exact terminal's strict write before the delayed response is
    // released. The previous checkpoint and prepared actor must survive.
    let checkpoint_before_failed_write = std::fs::read(&path).unwrap();
    server
        .llm_sidecar
        .configure_provider_lineage_store(path.join("blocked-checkpoint.json"));
    let write_error = server
        .llm_sidecar
        .fail_provider_turn_with_feedback(&agent_id, "rejected", "stale_base")
        .unwrap_err();
    assert!(write_error.contains("checkpoint"), "{write_error}");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        checkpoint_before_failed_write
    );
    assert!(!server.llm_sidecar.provider_contexts_empty());
    let old_checkpoint: serde_json::Value =
        serde_json::from_slice(&checkpoint_before_failed_write).unwrap();
    assert_eq!(old_checkpoint["provider_feedback_seq"][&agent_id], 3);
    server
        .llm_sidecar
        .configure_provider_lineage_store(path.clone());
    RuntimeLlmSidecar::cut_terminal_checkpoint_for_test();
    release.store(true, Ordering::SeqCst);
    let until = Instant::now() + Duration::from_secs(5);
    let mut saw_terminal = false;
    while Instant::now() < until {
        server.llm_sidecar.request_decision();
        let outcome = server.enqueue_llm_action_from_sidecar();
        if let Err(trace) = &outcome {
            assert!(
                trace
                    .llm_error
                    .as_deref()
                    .unwrap_or_default()
                    .contains("test crash cut"),
                "{trace:?}"
            );
        }
        let checkpoint: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let terminal = &checkpoint["provider_terminal_states"][&agent_id];
        if terminal["status"] == "rejected" && terminal["reject_reason"] == "stale_base" {
            saw_terminal = true;
            assert_eq!(
                server.world.agent_causal_receipts(&agent_id).unwrap().len(),
                receipts_before
            );
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        saw_terminal,
        "actual stale parent must persist rejected/stale_base"
    );
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let saved_terminal = checkpoint["provider_terminal_states"][&agent_id].clone();
    let saved_feedback: crate::simulator::FeedbackEnvelopeV1 =
        serde_json::from_value(saved_terminal["feedback"].clone()).unwrap();
    assert_eq!(saved_feedback.feedback_seq, 3);
    assert_eq!(saved_feedback.status, "rejected");
    assert_eq!(saved_feedback.reject_reason.as_deref(), Some("stale_base"));
    assert_eq!(
        saved_feedback.decision_request_id,
        saved_terminal["decision_request_id"].as_str().unwrap()
    );
    assert_eq!(
        server.world.runtime_feedback_outbox().unwrap().len(),
        2,
        "crash cut leaves seq3 only in the exact terminal checkpoint"
    );
    let sequence_before = checkpoint["provider_feedback_seq"].clone();
    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored.restore_provider_lineage(&server.world).unwrap();
    server.llm_sidecar = restored;
    register_runtime_session(
        &mut server,
        "agency-owner",
        Some(&agent_id),
        2_000,
        &public_key,
        &private_key,
    );
    assert!(
        server
            .llm_sidecar
            .fail_provider_turn_with_feedback(&agent_id, "rejected", "stale_base")
            .unwrap()
            .is_none()
    );
    let replayed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(replayed["provider_feedback_seq"], sequence_before);
    assert_eq!(
        replayed["provider_terminal_states"][&agent_id]["feedback"],
        saved_terminal["feedback"]
    );
    let replacement_result =
        super::agent_memory_correction_control::drive_provider_action(&mut server, false);
    assert!(
        replacement_result.is_ok(),
        "replacement failed: {replacement_result:?}; recorded paths: {:?}; decision count: {}",
        records
            .lock()
            .unwrap()
            .iter()
            .map(|row| row.path.clone())
            .collect::<Vec<_>>(),
        count.load(Ordering::SeqCst)
    );
    let mut acknowledged = server
        .world
        .runtime_feedback_outbox()
        .unwrap()
        .into_iter()
        .filter(|row| row.agent_subject == agent_id)
        .collect::<Vec<_>>();
    acknowledged.sort_by_key(|row| row.feedback_seq);
    assert_eq!(
        acknowledged
            .iter()
            .map(|row| row.feedback_seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert!(acknowledged.iter().all(|row| row.state == "acked"));
    let correction = server
        .llm_sidecar
        .agent_memory_corrections(&agent_id)
        .pop()
        .unwrap();
    assert_eq!(correction.status, "applied");
    let final_checkpoint: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(final_checkpoint["provider_feedback_seq"][&agent_id], 5);
    let captured: Vec<crate::simulator::ContinuousAgentRequestContextV1> = records
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request.path == "/v1/world-simulator/decision-context")
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect();
    let replacement = captured
        .iter()
        .find(|request| {
            Some(&request.decision_request_id) == correction.committed_decision_request_id.as_ref()
        })
        .unwrap();
    assert_eq!(
        Some(replacement.request_digest.to_string()),
        correction.committed_request_digest
    );
    assert!(
        replacement
            .base_decision_request
            .observation
            .memory_summary
            .as_deref()
            .unwrap()
            .contains("corrected observation")
    );
    let stale_parent = captured
        .iter()
        .find(|request| {
            Some(&request.decision_request_id) == correction.earliest_decision_request_id.as_ref()
        })
        .unwrap();
    assert_eq!(
        stale_parent.decision_request_id,
        saved_feedback.decision_request_id
    );
    assert_eq!(stale_parent.request_digest, saved_feedback.request_digest);

    assert_ne!(
        correction.earliest_decision_request_id,
        correction.committed_decision_request_id
    );
    let receipt = server
        .world
        .agent_causal_receipts(&agent_id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        correction.runtime_receipt_id.as_deref(),
        Some(receipt.receipt_id.as_str())
    );
    assert!(receipt.correction_refs.contains(&correction.correction_id));
    assert_eq!(receipt.authorization.unwrap().cost_units, 0);
    if let Ok(output) = std::env::var("OASIS7_AGENCY_STALE_SNAPSHOT_OUT") {
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&server.compat_snapshot(Some("agency-owner"))).unwrap(),
        )
        .unwrap();
    }
    clear_runtime_provider_env();
    let _ = std::fs::remove_file(path);
}
