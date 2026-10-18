use super::*;
use oasis7_agent_api::CapabilityInvocationContext;
use oasis7_agent_api::{
    COGNITION_CAPABILITY_CATALOG_DOMAIN, COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
    cognition_legacy_response_digest,
};
use oasis7_wasm_abi::CapabilityCatalogSnapshot;

#[derive(Debug, Clone)]
struct RecordingInvoker {
    response: Result<AgentInvocationOutput, String>,
    invocations: Arc<Mutex<Vec<AgentInvocation>>>,
}

impl AgentInvoker for RecordingInvoker {
    fn invoke(&self, invocation: AgentInvocation) -> Result<AgentInvocationOutput, String> {
        self.invocations
            .lock()
            .expect("recording invocations lock")
            .push(invocation);
        self.response.clone()
    }
}

#[derive(Debug, Clone, Default)]
struct LatencyChangingInvoker {
    calls: Arc<Mutex<usize>>,
}

impl AgentInvoker for LatencyChangingInvoker {
    fn invoke(&self, _invocation: AgentInvocation) -> Result<AgentInvocationOutput, String> {
        let first_call = {
            let mut calls = self.calls.lock().expect("latency invoker lock");
            let first_call = *calls == 0;
            *calls += 1;
            first_call
        };
        if first_call {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(AgentInvocationOutput {
            prompt: "prompt".to_string(),
            text: r#"{"decision":"wait"}"#.to_string(),
            provider_version: Some("provider/test".to_string()),
            duration_ms: Some(1),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            route_note: None,
            upstream_trace: None,
        })
    }
}

#[test]
fn continuous_feedback_holds_gaps_and_drains_in_order_with_bounded_diagnostics() {
    let state = ProviderState::new(CliOptions {
        mode: ProviderMode::Mock,
        ..CliOptions::default()
    })
    .expect("build mock provider state");
    let accepted_context = continuous_context("session-gap", 1);
    state
        .handle_continuous_decision(
            accepted_context.clone(),
            None,
            &RecordingInvoker {
                response: Ok(AgentInvocationOutput {
                    prompt: "prompt".to_string(),
                    text: r#"{"decision":"wait"}"#.to_string(),
                    provider_version: Some("provider/test".to_string()),
                    duration_ms: Some(1),
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                    route_note: None,
                    upstream_trace: None,
                }),
                invocations: Arc::new(Mutex::new(Vec::new())),
            },
        )
        .expect("register the feedback lineage before sequencing it");
    let feedback = |seq: u64| FeedbackEnvelopeV1 {
        feedback_id: format!("feedback-gap-{seq}"),
        feedback_seq: seq,
        agent_subject: accepted_context.agent_subject.clone(),
        agent_session_id: accepted_context.agent_session_id.clone(),
        agent_turn_id: accepted_context.agent_turn_id.clone(),
        decision_request_id: accepted_context.decision_request_id.clone(),
        candidate_action_id: None,
        runtime_receipt_id: None,
        status: "pending".to_string(),
        request_digest: accepted_context.request_digest.clone(),
        reject_reason: None,
        provenance: "runtime_authoritative".to_string(),
    };

    assert_eq!(
        state
            .record_continuous_feedback(feedback(2))
            .expect_err("out-of-order feedback must be held"),
        "feedback_sequence_gap"
    );
    state
        .record_continuous_feedback(feedback(1))
        .expect("first feedback drains the held successor");
    let partitions = state.recent_feedback.lock().expect("feedback lock");
    let partition = &partitions[&("agent-1".to_string(), "session-gap".to_string())];
    assert_eq!(partition.next_seq, 2);
    assert!(partition.held.is_empty());
    assert_eq!(partition.recent.len(), 2);
    drop(partitions);
    for seq in 4..=11 {
        assert_eq!(
            state
                .record_continuous_feedback(feedback(seq))
                .expect_err("bounded gap remains diagnostic"),
            "feedback_sequence_gap"
        );
    }
    let mut held_collision = feedback(4);
    held_collision.feedback_id = "feedback-gap-collision".to_string();
    held_collision.reject_reason = Some("recovery_pending".to_string());
    assert_eq!(
        state
            .record_continuous_feedback(held_collision)
            .expect_err("held sequence collision must fail closed"),
        "feedback_identity_collision"
    );
    assert_eq!(
        state
            .record_continuous_feedback(feedback(13))
            .expect_err("bounded gap overflow must remain diagnostic"),
        "feedback_sequence_overflow"
    );
}

fn feedback_for_context(
    context: &ContinuousAgentRequestContextV1,
    feedback_id: &str,
    feedback_seq: u64,
) -> FeedbackEnvelopeV1 {
    FeedbackEnvelopeV1 {
        feedback_id: feedback_id.to_string(),
        feedback_seq,
        agent_subject: context.agent_subject.clone(),
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        candidate_action_id: None,
        runtime_receipt_id: None,
        status: "pending".to_string(),
        request_digest: context.request_digest.clone(),
        reject_reason: None,
        provenance: "runtime_authoritative".to_string(),
    }
}

fn recording_invoker() -> RecordingInvoker {
    RecordingInvoker {
        response: Ok(AgentInvocationOutput {
            prompt: "prompt".to_string(),
            text: r#"{"decision":"wait"}"#.to_string(),
            provider_version: Some("provider/test".to_string()),
            duration_ms: Some(1),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            route_note: None,
            upstream_trace: None,
        }),
        invocations: Arc::new(Mutex::new(Vec::new())),
    }
}

#[test]
fn continuous_feedback_requires_an_accepted_request_response_lineage() {
    let state = ProviderState::new(CliOptions {
        mode: ProviderMode::Mock,
        ..CliOptions::default()
    })
    .expect("build mock provider state");
    let context = continuous_context("session-correlation", 1);

    assert_eq!(
        state
            .record_continuous_feedback(feedback_for_context(&context, "forged-before-request", 1,))
            .expect_err("unknown feedback must not enter the prompt"),
        "unknown_feedback"
    );

    state
        .handle_continuous_decision(context.clone(), None, &recording_invoker())
        .expect("accepted response registers feedback lineage");

    let mut mismatched = feedback_for_context(&context, "mismatched-turn", 1);
    mismatched.agent_turn_id = "forged-turn".to_string();
    assert_eq!(
        state
            .record_continuous_feedback(mismatched)
            .expect_err("feedback with mismatched turn must fail closed"),
        "feedback_correlation_mismatch"
    );

    let mut forged_digest = feedback_for_context(&context, "forged-digest", 1);
    forged_digest.request_digest = Digest32::from(format!("blake3:{}", "f".repeat(64)));
    assert_eq!(
        state
            .record_continuous_feedback(forged_digest)
            .expect_err("feedback with a forged request digest must fail closed"),
        "feedback_digest_mismatch"
    );

    state
        .record_continuous_feedback(feedback_for_context(&context, "accepted-feedback", 1))
        .expect("correlated feedback is accepted");
}

#[test]
fn continuous_response_lineage_rejects_a_forged_duplicate_response() {
    let state = ProviderState::new(CliOptions {
        mode: ProviderMode::Mock,
        ..CliOptions::default()
    })
    .expect("build mock provider state");
    let context = continuous_context("session-response", 1);
    let mut forged_response = state
        .handle_continuous_decision(context.clone(), None, &recording_invoker())
        .expect("register response lineage");
    forged_response.response_digest = Digest32::from(format!("blake3:{}", "f".repeat(64)));
    assert_eq!(
        super::super::feedback_state::remember_accepted_response(
            &state,
            &context,
            &forged_response
        )
        .expect_err("a changed response must not replace accepted lineage"),
        "response_identity_collision"
    );
}

#[test]
fn continuous_feedback_cursor_and_lineage_survive_provider_restart() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-feedback-restart-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    let context = continuous_context("session-restart", 1);
    let state =
        ProviderState::new_with_feedback_state_path(CliOptions::default(), Some(path.clone()))
            .expect("first bridge state");
    state
        .handle_continuous_decision(context.clone(), None, &recording_invoker())
        .expect("register first response");
    assert_eq!(
        state
            .record_continuous_feedback(feedback_for_context(&context, "restart-feedback-2", 2))
            .expect_err("out-of-order feedback must be persisted as a bounded hold"),
        "feedback_sequence_gap"
    );
    drop(state);

    let restarted =
        ProviderState::new_with_feedback_state_path(CliOptions::default(), Some(path.clone()))
            .expect("restarted bridge state");
    assert!(
        restarted
            .recent_feedback
            .lock()
            .expect("feedback lock")
            .get(&("agent-1".to_string(), "session-restart".to_string()))
            .is_some_and(|partition| partition.held.contains_key(&2)),
        "persisted state must retain the held successor across restart"
    );
    restarted
        .record_continuous_feedback(feedback_for_context(&context, "restart-feedback-1", 1))
        .expect("persisted predecessor drains the held successor");

    let partitions = restarted.recent_feedback.lock().expect("feedback lock");
    let partition = &partitions[&("agent-1".to_string(), "session-restart".to_string())];
    assert_eq!(partition.next_seq, 2);
    assert!(partition.held.is_empty());
    drop(partitions);
    restarted
        .record_continuous_feedback(feedback_for_context(&context, "restart-feedback-2", 2))
        .expect("duplicate drained successor is idempotent after restart");

    let _ = std::fs::remove_file(path);
}

#[test]
fn held_feedback_rolls_back_when_configured_state_persistence_fails() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-feedback-rollback-{}/state.json",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(path.parent().expect("rollback parent"));
    let state =
        ProviderState::new_with_feedback_state_path(CliOptions::default(), Some(path.clone()))
            .expect("bridge state with missing persistence parent");
    let context = continuous_context("session-rollback", 1);
    state
        .accepted_requests
        .lock()
        .expect("accepted request lock")
        .insert(
            context.decision_request_id.clone(),
            AcceptedRequestIdentity {
                agent_subject: context.agent_subject.clone(),
                agent_session_id: context.agent_session_id.clone(),
                agent_turn_id: context.agent_turn_id.clone(),
                decision_request_id: context.decision_request_id.clone(),
                request_digest: context.request_digest.clone(),
                response_digest: Digest32::from(format!("blake3:{}", "b".repeat(64))),
                accepted_order: 0,
            },
        );

    let error = state
        .record_continuous_feedback(feedback_for_context(&context, "rollback-feedback", 2))
        .expect_err("persistence failure must be surfaced");
    assert!(
        error.starts_with("feedback_state_persist_failed:"),
        "unexpected persistence error: {error}"
    );
    assert!(
        state
            .recent_feedback
            .lock()
            .expect("feedback lock")
            .get(&("agent-1".to_string(), "session-rollback".to_string()))
            .is_none_or(|partition| partition.held.is_empty()),
        "a failed persist must roll back the held successor"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("rollback parent"));
}

#[test]
fn feedback_partitions_are_bounded_and_restartable_after_many_sessions() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-feedback-partitions-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let state = ProviderState::new_with_feedback_state_path(
        CliOptions {
            mode: ProviderMode::Mock,
            ..CliOptions::default()
        },
        Some(path.clone()),
    )
    .expect("bridge state");
    let invoker = recording_invoker();

    for index in 0..=MAX_ACCEPTED_REQUESTS {
        let context = continuous_context(&format!("session-partition-{index:03}"), 1);
        state
            .handle_continuous_decision(context.clone(), None, &invoker)
            .expect("register feedback lineage");
        state
            .record_continuous_feedback(feedback_for_context(
                &context,
                &format!("partition-feedback-{index:03}"),
                1,
            ))
            .expect("first feedback is accepted");
    }

    let partitions = state.recent_feedback.lock().expect("feedback lock");
    assert_eq!(partitions.len(), MAX_ACCEPTED_REQUESTS);
    drop(partitions);
    drop(state);

    let restarted = ProviderState::new_with_feedback_state_path(
        CliOptions {
            mode: ProviderMode::Mock,
            ..CliOptions::default()
        },
        Some(path.clone()),
    )
    .expect("bounded persisted state remains loadable");
    assert_eq!(
        restarted
            .recent_feedback
            .lock()
            .expect("restarted feedback lock")
            .len(),
        MAX_ACCEPTED_REQUESTS
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn accepted_feedback_lineage_evicts_oldest_request_by_acceptance_age() {
    let state = ProviderState::new(CliOptions {
        mode: ProviderMode::Mock,
        ..CliOptions::default()
    })
    .expect("bridge state");
    let invoker = recording_invoker();
    let oldest = continuous_context("session-z-oldest", 1);
    state
        .handle_continuous_decision(oldest.clone(), None, &invoker)
        .expect("register oldest response");

    for index in 0..(MAX_ACCEPTED_REQUESTS - 1) {
        let context = continuous_context(&format!("session-middle-{index:03}"), 1);
        state
            .handle_continuous_decision(context, None, &invoker)
            .expect("register middle response");
    }
    let newest = continuous_context("session-a-newest", 1);
    state
        .handle_continuous_decision(newest.clone(), None, &invoker)
        .expect("register newest response");

    let accepted_requests = state
        .accepted_requests
        .lock()
        .expect("accepted request lock");
    assert!(!accepted_requests.contains_key(&oldest.decision_request_id));
    assert!(accepted_requests.contains_key(&newest.decision_request_id));
    drop(accepted_requests);
    state
        .record_continuous_feedback(feedback_for_context(&newest, "newest-feedback", 1))
        .expect("newest accepted response remains available for feedback");
}

#[test]
fn target_decision_rejects_heuristic_model_output_but_legacy_keeps_compatibility() {
    for (index, raw) in [
        "```json\n{\"decision\":\"wait\"}\n```",
        "Here is the decision: {\"decision\":\"wait\"}",
        "not-json",
    ]
    .into_iter()
    .enumerate()
    {
        let state = ProviderState::new(CliOptions::default()).expect("provider state");
        let context = continuous_context(&format!("session-strict-{index}"), 1);
        let invoker = RecordingInvoker {
            response: Ok(AgentInvocationOutput {
                prompt: "prompt".to_string(),
                text: raw.to_string(),
                provider_version: Some("provider/test".to_string()),
                duration_ms: Some(1),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
                route_note: None,
                upstream_trace: None,
            }),
            invocations: Arc::new(Mutex::new(Vec::new())),
        };
        let response = state
            .handle_continuous_decision(context, None, &invoker)
            .expect("strict target response is still structurally returned");
        assert_eq!(
            response.base_decision_response.decision,
            ProviderDecision::Wait
        );
        let error = response
            .base_decision_response
            .provider_error
            .as_ref()
            .expect("strict target malformed output must be an explicit provider error");
        assert_eq!(error.code, "invalid_action_schema");
        assert_eq!(
            response
                .base_decision_response
                .trace_payload
                .schema_repair_count,
            0
        );
    }

    let state = ProviderState::new(CliOptions::default()).expect("provider state");
    let invoker = RecordingInvoker {
        response: Ok(AgentInvocationOutput {
            prompt: "prompt".to_string(),
            text: "```json\n{\"decision\":\"wait\"}\n```".to_string(),
            provider_version: Some("provider/test".to_string()),
            duration_ms: Some(1),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            route_note: None,
            upstream_trace: None,
        }),
        invocations: Arc::new(Mutex::new(Vec::new())),
    };
    let legacy = state.handle_decision(sample_request(), None, &invoker);
    assert_eq!(legacy.decision, ProviderDecision::Wait);
    assert!(legacy.provider_error.is_none());
    assert_eq!(legacy.trace_payload.schema_repair_count, 1);
}

#[test]
fn target_feedback_admits_only_correlated_bounded_diagnostics() {
    let context = continuous_context("session-feedback-policy", 1);
    for (status, reason) in [
        ("pending", None),
        ("pending", Some("scheduler_backpressure")),
        ("rejected", Some("no_effect")),
        ("failed", Some("provider_unavailable")),
    ] {
        let state = ProviderState::new(CliOptions {
            mode: ProviderMode::Mock,
            ..CliOptions::default()
        })
        .expect("provider state");
        state
            .handle_continuous_decision(context.clone(), None, &recording_invoker())
            .expect("register response lineage");
        let mut feedback = feedback_for_context(
            &context,
            &format!("feedback-{status}-{}", reason.unwrap_or("none")),
            1,
        );
        feedback.status = status.to_string();
        feedback.reject_reason = reason.map(ToOwned::to_owned);
        state
            .record_continuous_feedback(feedback)
            .expect("allowlisted correlated diagnostic");
    }

    for authority_kind in 0..3 {
        let state = ProviderState::new(CliOptions {
            mode: ProviderMode::Mock,
            ..CliOptions::default()
        })
        .expect("provider state");
        state
            .handle_continuous_decision(context.clone(), None, &recording_invoker())
            .expect("register response lineage");
        let mut feedback = feedback_for_context(&context, "feedback-unverifiable", 1);
        match authority_kind {
            0 => {
                feedback.status = "committed".to_string();
                feedback.candidate_action_id = Some(7);
                feedback.runtime_receipt_id = Some("receipt-forged".to_string());
            }
            1 => feedback.candidate_action_id = Some(7),
            2 => feedback.runtime_receipt_id = Some("receipt-forged".to_string()),
            _ => unreachable!(),
        }
        assert_eq!(
            state
                .record_continuous_feedback(feedback)
                .expect_err("unverifiable authority must not enter prompts"),
            "feedback_disposition_unverifiable"
        );
    }
}

fn continuous_context(
    agent_session_id: &str,
    transport_attempt: u64,
) -> ContinuousAgentRequestContextV1 {
    let mut base_decision_request = sample_request();
    let (catalog, invocation) = production_capabilities(agent_session_id);
    base_decision_request.capability_catalog = Some(catalog.clone());
    base_decision_request.capability_invocation_context = Some(invocation.clone());
    let mut context = ContinuousAgentRequestContextV1 {
        base_decision_request,
        context_discriminator: oasis7_agent_api::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.to_string(),
        context_version: oasis7_agent_api::CONTINUOUS_AGENT_CONTEXT_VERSION,
        protocol_version: "continuous-agent-v1".to_string(),
        agent_session_id: agent_session_id.to_string(),
        agent_turn_id: "turn-bridge".to_string(),
        decision_request_id: format!("request-{agent_session_id}"),
        retry_seq: 1,
        transport_attempt,
        agent_subject: "agent-1".to_string(),
        runtime_binding: RuntimeBindingV1 {
            world_id: "world-bridge".to_string(),
            branch_id: "main".to_string(),
            finality_epoch: 1,
            finality_block_hash: Some(Digest32::from(format!("blake3:{}", "a".repeat(64)))),
            finality_status: "verified".to_string(),
            base_tick: 7,
            base_world_hash: Digest32::from(format!("blake3:{}", "b".repeat(64))),
            reorg_epoch: 0,
            runtime_manifest_hash: Digest32::from(format!("blake3:{}", "c".repeat(64))),
        },
        observation_digest: Digest32::from(format!("blake3:{}", "d".repeat(64))),
        capability_catalog_digest: oasis7_agent_api::h_v1(
            COGNITION_CAPABILITY_CATALOG_DOMAIN,
            &catalog,
        ),
        capability_invocation_context_digest: oasis7_agent_api::h_v1(
            COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
            &invocation,
        ),
        memory_snapshot_digest: Digest32::from(format!("blake3:{}", "1".repeat(64))),
        goal_snapshot_digest: Digest32::from(format!("blake3:{}", "2".repeat(64))),
        continuation_digest: Digest32::from(format!("blake3:{}", "3".repeat(64))),
        adapter_protocol_version: "loopback-http-v1".to_string(),
        budget_contract: oasis7_agent_api::BudgetContractV1 {
            max_latency_ms: 7_000,
            max_repair_attempts: 1,
            max_model_calls: 4,
            max_tool_calls: 3,
        },
        request_digest: Digest32::default(),
    };
    context.request_digest = context.request_digest();
    context
}

fn production_capabilities(
    session_id: &str,
) -> (CapabilityCatalogSnapshot, CapabilityInvocationContext) {
    let subject = serde_json::json!({
        "kind": "agent",
        "agent_id": "agent-1",
        "owner_binding": "owner-1",
        "generation": 1
    });
    let presenter = serde_json::json!({
        "presenter_id": "provider-1",
        "presenter_kind": "provider",
        "session_id": session_id
    });
    let audience = serde_json::json!({
        "world_id": "world-bridge",
        "branch_id": "main",
        "finality_epoch": 1,
        "target_kind": "world",
        "target_id": null
    });
    let catalog: CapabilityCatalogSnapshot = serde_json::from_value(serde_json::json!({
        "snapshot_id": format!("catalog.{session_id}"),
        "world_id": "world-bridge",
        "world_head": 7,
        "branch_id": "main",
        "finality_epoch": 1,
        "logical_tick": 7,
        "module_registry_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "policy_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "revocation_epoch": 0,
        "subject": subject,
        "presenter": presenter,
        "audience": audience,
        "entries": [],
        "valid_until_tick": 100
    }))
    .expect("decode production capability catalog");
    let invocation = CapabilityInvocationContext {
        grant_id: format!("grant.{session_id}"),
        subject: catalog.subject.clone(),
        presenter: catalog.presenter.clone(),
        audience: catalog.audience.clone(),
        catalog_snapshot_id: catalog.snapshot_id.clone(),
        module_id: String::new(),
        module_version: String::new(),
        response_nonce: format!("nonce.{session_id}"),
    };
    (catalog, invocation)
}

#[test]
fn continuous_bridge_uses_outer_identity_for_session_and_invocation_key() {
    let state = ProviderState::new(CliOptions::default()).expect("build provider state");
    let invocations = Arc::new(Mutex::new(Vec::new()));
    let invoker = RecordingInvoker {
        response: Ok(AgentInvocationOutput {
            prompt: "prompt".to_string(),
            text: r#"{"decision":"wait"}"#.to_string(),
            provider_version: Some("provider/test".to_string()),
            duration_ms: Some(1),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            route_note: None,
            upstream_trace: None,
        }),
        invocations: Arc::clone(&invocations),
    };
    let first = continuous_context("session-one", 1);
    let retry = continuous_context("session-one", 2);
    let other_outer = continuous_context("session-two", 1);
    let first_key = first.provider_invocation_key().to_string();
    let other_key = other_outer.provider_invocation_key().to_string();
    state
        .handle_continuous_decision(first.clone(), None, &invoker)
        .expect("first continuous invocation");
    state
        .handle_continuous_decision(retry, None, &invoker)
        .expect("transport retry continuous invocation");
    state
        .handle_continuous_decision(other_outer.clone(), None, &invoker)
        .expect("different outer identity invocation");

    let invocations = invocations.lock().expect("recording invocations lock");
    assert_eq!(invocations.len(), 3);
    assert_eq!(
        invocations[0].provider_invocation_key.as_deref(),
        Some(first_key.as_str())
    );
    assert_eq!(
        invocations[0].agent_session_id.as_deref(),
        Some("session-one")
    );
    assert_eq!(
        invocations[0].idempotency_key,
        invocations[1].idempotency_key
    );
    assert_eq!(invocations[0].session_key, invocations[1].session_key);
    assert_ne!(
        invocations[0].idempotency_key,
        invocations[2].idempotency_key
    );
    assert_ne!(invocations[0].session_key, invocations[2].session_key);
    assert_eq!(
        invocations[2].provider_invocation_key.as_deref(),
        Some(other_key.as_str())
    );
}

#[test]
fn continuous_bridge_retry_ignores_non_authority_latency_in_response_identity() {
    let state = ProviderState::new(CliOptions::default()).expect("build provider state");
    let invoker = LatencyChangingInvoker::default();
    let first = continuous_context("session-latency", 1);
    let retry = continuous_context("session-latency", 2);

    state
        .handle_continuous_decision(first, None, &invoker)
        .expect("first continuous invocation");
    state
        .handle_continuous_decision(retry, None, &invoker)
        .expect("transport retry with different diagnostic latency");
}

#[test]
fn continuous_bridge_rejects_legacy_full_response_digest_explicitly() {
    let state = ProviderState::new(CliOptions::default()).expect("build provider state");
    let current = state
        .handle_continuous_decision(
            continuous_context("session-legacy", 1),
            None,
            &recording_invoker(),
        )
        .expect("current response");
    let mut legacy = current;
    legacy.response_digest = cognition_legacy_response_digest(&legacy.base_decision_response);
    let identity = legacy.response_artifact_identity();

    assert_eq!(
        legacy
            .validate_response_artifact_identity(&identity)
            .expect_err("legacy full response digest must not enter target validation")
            .code(),
        "legacy_response_digest_unsupported"
    );
}

#[test]
fn continuous_bridge_rejects_old_persisted_feedback_state_schema() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-feedback-legacy-schema-{}.json",
        std::process::id()
    ));
    let response = DecisionResponse::wait("legacy-provider");
    let legacy_digest = cognition_legacy_response_digest(&response);
    let state = serde_json::json!({
        "schema_version": 1,
        "accepted_requests": {
            "request-legacy": {
                "agent_subject": "agent-legacy",
                "agent_session_id": "session-legacy",
                "agent_turn_id": "turn-legacy",
                "decision_request_id": "request-legacy",
                "request_digest": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "response_digest": legacy_digest,
                "accepted_order": 1
            }
        },
        "partitions": []
    });
    std::fs::write(
        &path,
        serde_json::to_vec(&state).expect("encode legacy state"),
    )
    .expect("write legacy state");

    let error =
        ProviderState::new_with_feedback_state_path(CliOptions::default(), Some(path.clone()))
            .expect_err("old persisted full-response digest state must fail closed");
    assert!(
        error.contains("unsupported schema"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_file(path);
}
