//! LIVE-5 RED tests for the production Harness/Runtime handoff.
//!
//! The policy fixtures prove bounded normalization in isolation.  These tests
//! deliberately enter through the target actor runner and therefore remain
//! RED until Builtin and ProviderBacked actors share the host projection,
//! Runtime receipt gate, and Runtime-owned continuation status.  They do not
//! grant any authority to provider output and do not cover deferred
//! GoalGraph, shared memory, beliefs, preference correction, or billing.

use crate::runtime::{
    AgentContinuation, ContinuationBudgetV1, ContinuationStatusV1, RuntimeReceiptLineageV1,
    WakeConditionV1,
};
use crate::simulator::{
    ActionCatalogEntry, AsyncAgentRunner, AsyncTurnFeedback, AsyncTurnLifecycle, AsyncWorldEffect,
    ContinuationProposalV1, ContinuousAgentTurnContextV1, DecisionProviderError, DecisionResponse,
    FeedbackEnvelopeV1, GoalSnapshotInputV1, GoalSnapshotProjector, MemoryContextEntryV1,
    MemoryContextSnapshotV1, MemoryWriteIntent, MemoryWriteStore, MockDecisionProvider,
    ProviderBackedAgentBehavior, ProviderDecision, ProviderDiagnostics, ProviderExecutionMode,
    ProviderTraceEnvelope, ProviderTranscriptEntry, ResponseArtifactIdentityV1,
    RuntimeReceiptReadbackHandleV1, RuntimeReceiptReadbackVerifier,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

mod replay_regressions;
use self::replay_regressions::VerifiedWorldReadback;

const AGENT_ID: &str = "agent-live-harness";
const SESSION_ID: &str = "session-live-harness";
const TURN_ID: &str = "turn-live-harness";
const REQUEST_ID: &str = "request-live-harness";
const REQUEST_DIGEST: &str =
    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MAX_PROVIDER_TRANSCRIPT_ENTRIES: usize = 64;
const MAX_PROVIDER_TOOL_TRACE_ENTRIES: usize = 64;
const MAX_PROVIDER_TRANSCRIPT_ENTRY_BYTES: usize = 2 * 1024;
const MAX_PROVIDER_TOOL_TRACE_ENTRY_BYTES: usize = 1024;
const MAX_PROVIDER_SUMMARY_BYTES: usize = 1024;
const MAX_PROVIDER_TRACE_BYTES: usize = 32 * 1024;
const ACTOR_COMPLETION_TIMEOUT: Duration = Duration::from_secs(10);

fn host_memory() -> MemoryContextSnapshotV1 {
    let mut snapshot = MemoryContextSnapshotV1 {
        revision: 7,
        entries: vec![MemoryContextEntryV1 {
            id: "memory-1".to_string(),
            summary: "the last committed receipt was receipt-previous".to_string(),
            tags: vec!["receipt".to_string()],
        }],
        scope: "session_private".to_string(),
        digest: String::new(),
    };
    snapshot.digest = snapshot.computed_digest();
    snapshot
}

fn host_goal() -> crate::simulator::GoalSnapshotV1 {
    GoalSnapshotProjector::project(
        Some(GoalSnapshotInputV1 {
            revision: 11,
            short_term_summary: "inspect the newly committed site".to_string(),
            long_term_summary: "establish a durable industrial route".to_string(),
            blocked_reason: None,
            provenance: "harness_projection".to_string(),
        }),
        None,
    )
    .expect("valid host goal projection")
}

fn host_context() -> ContinuousAgentTurnContextV1 {
    serde_json::from_value(json!({
        "agent_id": AGENT_ID,
        "agent_session_id": SESSION_ID,
        "agent_turn_id": TURN_ID,
        "decision_request_id": REQUEST_ID,
        "request_digest": REQUEST_DIGEST,
        "memory_snapshot": host_memory(),
        "goal_snapshot": host_goal(),
        "continuation": null
    }))
    .expect("decode host cognition context")
}

fn wait_response_with_memory_intent() -> DecisionResponse {
    DecisionResponse {
        decision: ProviderDecision::Wait,
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics {
            provider_id: Some("live-harness-provider".to_string()),
            ..ProviderDiagnostics::default()
        },
        trace_payload: ProviderTraceEnvelope {
            provider_id: Some("live-harness-provider".to_string()),
            output_summary: Some("decision=wait".to_string()),
            ..ProviderTraceEnvelope::default()
        },
        memory_write_intents: vec![MemoryWriteIntent {
            scope: "session_private".to_string(),
            summary: "candidate observation awaiting Runtime receipt".to_string(),
            tags: vec!["candidate".to_string()],
        }],
    }
}

#[test]
fn provider_agency_explanation_is_bounded_public_metadata_and_never_an_effect_receipt() {
    let mut response = wait_response_with_memory_intent();
    response.trace_payload.upstream_trace = Some(json!({
        "private_prompt": "must-not-project",
        "decision_explanation": {
            "reason": "wait until production capacity is available",
            "dissent": "prefer waiting over the requested production step",
            "evidence_refs": ["observation:capacity", "bearer secret-credential"],
            "stakes": "avoid consuming scarce material",
            "expected_consequence": "next quote may permit production",
            "alternatives": ["gather available feedstock"]
        }
    }));
    let mut runner = provider_backed_runner_with_response(response);
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .unwrap();
    let outcome = completed_turn(&mut runner);
    let trace = outcome.decision_trace.unwrap();
    let explanation = trace.llm_diagnostics.unwrap().agency_explanation.unwrap();
    assert_eq!(explanation.status, "available");
    assert_eq!(explanation.provenance, "agent_explanation_unverified");
    assert_eq!(
        explanation.dissent.as_deref(),
        Some("prefer waiting over the requested production step")
    );
    assert_eq!(explanation.evidence_refs, ["observation:capacity"]);
    assert_eq!(
        explanation.expected_consequence.as_deref(),
        Some("next quote may permit production")
    );
    assert!(trace.llm_effect_receipts.is_empty());
    let projected = serde_json::to_string(&explanation).unwrap();
    assert!(!projected.contains("must-not-project"));
    assert!(!projected.contains("secret-credential"));

    let mut response = wait_response_with_memory_intent();
    response.trace_payload.upstream_trace = Some(json!({"decision_explanation": {
        "reason": "bearer private-credential", "actual_result": "forged applied"
    }}));
    let mut runner = provider_backed_runner_with_response(response);
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .unwrap();
    let explanation = completed_turn(&mut runner)
        .decision_trace
        .unwrap()
        .llm_diagnostics
        .unwrap()
        .agency_explanation
        .unwrap();
    assert_eq!(explanation.status, "unavailable");
    assert!(explanation.reason.is_none());
    assert_eq!(
        explanation.next_step,
        "inspect_runtime_result_or_request_agent_explanation"
    );
}

fn provider_backed_runner() -> AsyncAgentRunner {
    provider_backed_runner_with_state().0
}

fn provider_backed_runner_with_response(response: DecisionResponse) -> AsyncAgentRunner {
    let provider =
        MockDecisionProvider::with_scripted_responses("live-harness-provider", vec![Ok(response)]);
    let behavior = ProviderBackedAgentBehavior::new(
        AGENT_ID,
        provider,
        vec![ActionCatalogEntry::new("wait", "wait without world effect")],
    )
    .legacy_compatibility()
    .with_execution_mode(ProviderExecutionMode::HeadlessAgent);
    let mut runner = AsyncAgentRunner::new(16).expect("create target actor runner");
    runner
        .register(behavior)
        .expect("register provider-backed actor");
    runner
}

fn provider_backed_runner_with_state() -> (
    AsyncAgentRunner,
    std::sync::Arc<std::sync::Mutex<crate::simulator::MockDecisionProviderState>>,
) {
    let provider = MockDecisionProvider::with_scripted_responses(
        "live-harness-provider",
        vec![
            Ok(wait_response_with_memory_intent()),
            Ok(wait_response_with_memory_intent()),
        ],
    );
    let shared_state = provider.shared_state();
    let behavior = ProviderBackedAgentBehavior::new(
        AGENT_ID,
        provider,
        vec![ActionCatalogEntry::new("wait", "wait without world effect")],
    )
    .legacy_compatibility()
    .with_execution_mode(ProviderExecutionMode::HeadlessAgent);
    let mut runner = AsyncAgentRunner::new(16).expect("create target actor runner");
    runner
        .register(behavior)
        .expect("register provider-backed actor");
    (runner, shared_state)
}

fn provider_failure_runner() -> AsyncAgentRunner {
    let provider = MockDecisionProvider::with_scripted_responses(
        "live-harness-provider",
        vec![Err(DecisionProviderError::new(
            "provider_timeout",
            "provider exceeded the turn budget",
            true,
        ))],
    );
    let behavior = ProviderBackedAgentBehavior::new(
        AGENT_ID,
        provider,
        vec![ActionCatalogEntry::new("wait", "wait without world effect")],
    )
    .legacy_compatibility()
    .with_execution_mode(ProviderExecutionMode::HeadlessAgent);
    let mut runner = AsyncAgentRunner::new(16).expect("create target actor runner");
    runner
        .register(behavior)
        .expect("register provider-backed actor");
    runner
}

fn completed_turn(runner: &mut AsyncAgentRunner) -> crate::simulator::AsyncAgentTurnOutcome {
    let deadline = Instant::now() + ACTOR_COMPLETION_TIMEOUT;
    loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll target actor")
            .into_iter()
            .next()
        {
            return outcome;
        }
        if Instant::now() >= deadline {
            panic!("target actor did not complete before {ACTOR_COMPLETION_TIMEOUT:?} deadline");
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn assert_provider_record_has<T: serde::Serialize>(recorded_wire: &str, field: &str, expected: &T) {
    let recorded: Value = serde_json::from_str(recorded_wire).expect("decode provider recording");
    let expected = serde_json::to_value(expected).expect("serialize expected projection");
    fn contains_field(value: &Value, field: &str, expected: &Value) -> bool {
        match value {
            Value::Object(object) => {
                object.get(field).is_some_and(|value| value == expected)
                    || object
                        .values()
                        .any(|value| contains_field(value, field, expected))
            }
            Value::Array(values) => values
                .iter()
                .any(|value| contains_field(value, field, expected)),
            _ => false,
        }
    }
    assert!(
        contains_field(&recorded, field, &expected),
        "provider recording must contain frozen {field} field; expected={expected}, recorded={recorded_wire}"
    );
}

#[test]
fn provider_boundary_preserves_frozen_context_and_response_lineage() {
    let (mut runner, shared_state) = provider_backed_runner_with_state();
    let mut context = host_context();
    context.continuation = Some(continuation_proposal());

    runner
        .start_turn_with_context(AGENT_ID, context.clone())
        .expect("open provider-backed target turn");
    let outcome = completed_turn(&mut runner);

    let state = shared_state.lock().expect("mock state lock").clone();
    assert_eq!(state.recorded_requests.len(), 1);
    let recorded_wire =
        serde_json::to_string(&state).expect("serialize MockDecisionProvider recording");
    for (field, expected) in [
        (
            "memory_snapshot",
            serde_json::to_value(&context.memory_snapshot).expect("serialize memory snapshot"),
        ),
        (
            "continuation",
            serde_json::to_value(&context.continuation).expect("serialize continuation"),
        ),
        (
            "goal_snapshot",
            serde_json::to_value(&context.goal_snapshot).expect("serialize goal snapshot"),
        ),
    ] {
        assert_provider_record_has(&recorded_wire, field, &expected);
    }
    for (field, expected) in [
        ("agent_session_id", SESSION_ID),
        ("agent_turn_id", TURN_ID),
        ("decision_request_id", REQUEST_ID),
        ("request_digest", REQUEST_DIGEST),
    ] {
        assert_provider_record_has(&recorded_wire, field, &expected);
    }

    let response_wire = outcome
        .decision_trace
        .and_then(|trace| trace.llm_output)
        .expect("provider response trace");
    let response: Value = serde_json::from_str(&response_wire).expect("decode provider response");
    assert_eq!(
        response.get("context_discriminator"),
        Some(&json!("oasis7.continuous-agent-context")),
        "provider response must carry the versioned cognition envelope"
    );
    assert_eq!(response.get("context_version"), Some(&json!(1)));
    for (field, expected) in [
        ("agent_session_id", SESSION_ID),
        ("agent_turn_id", TURN_ID),
        ("decision_request_id", REQUEST_ID),
        ("request_digest", REQUEST_DIGEST),
    ] {
        assert_eq!(
            response.get(field),
            Some(&json!(expected)),
            "provider response lineage field {field} must echo the request"
        );
    }
    assert!(
        response.get("base_decision_response").is_some(),
        "provider response must retain the inner DecisionResponse under the outer lineage"
    );
}

#[test]
fn provider_trace_is_bounded_redacted_and_non_authoritative() {
    let sensitive_summary = json!({
        "credential": "credential-secret",
        "token": "token-secret",
        "authorization": "authorization-secret",
        "private_key": "private-key-secret",
        "path": "/private/path/secret",
        "cookie": "cookie-secret",
        "access_key": "access-key-secret",
        "refreshKey": "refresh-key-secret",
        "session_key": "session-key-secret"
    })
    .to_string();
    let mut redaction_response = wait_response_with_memory_intent();
    redaction_response.decision = ProviderDecision::WaitTicks { ticks: 3 };
    redaction_response.trace_payload = ProviderTraceEnvelope {
        provider_id: Some("live-harness-provider".to_string()),
        input_summary: Some(sensitive_summary.clone()),
        output_summary: Some(sensitive_summary.clone()),
        transcript: vec![ProviderTranscriptEntry {
            role: "tool".to_string(),
            content: sensitive_summary.clone(),
        }],
        tool_trace: vec![sensitive_summary.clone()],
        upstream_trace: Some(json!({
            "credential": "credential-secret",
            "token": "token-secret",
            "authorization": "authorization-secret",
            "private_key": "private-key-secret",
            "path": "/private/path/secret",
            "cookie": "cookie-secret",
            "access_key": "access-key-secret",
            "refreshKey": "refresh-key-secret",
            "session_key": "session-key-secret"
        })),
        ..ProviderTraceEnvelope::default()
    };

    let mut redaction_runner = provider_backed_runner_with_response(redaction_response);
    redaction_runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open redaction fixture turn");
    let redaction_outcome = completed_turn(&mut redaction_runner);
    assert_eq!(
        redaction_outcome.decision,
        Some(crate::simulator::AgentDecision::WaitTicks(3)),
        "non-authority redaction diagnostics must not change the candidate decision"
    );
    let redacted_trace = redaction_outcome
        .decision_trace
        .expect("redaction fixture retains a decision trace");
    let redacted_wire = serde_json::to_string(&redacted_trace).expect("serialize redacted trace");
    assert!(
        redacted_wire.contains("<redacted>"),
        "sensitive provider trace fields must use the fixed redaction marker"
    );
    for secret in [
        "credential-secret",
        "token-secret",
        "authorization-secret",
        "private-key-secret",
        "/private/path/secret",
        "cookie-secret",
        "access-key-secret",
        "refresh-key-secret",
        "session-key-secret",
    ] {
        assert!(
            !redacted_wire.contains(secret),
            "provider trace must not retain sensitive value {secret:?}"
        );
    }

    let mut overflow_response = wait_response_with_memory_intent();
    overflow_response.decision = ProviderDecision::WaitTicks { ticks: 7 };
    overflow_response.trace_payload.input_summary =
        Some("i".repeat(MAX_PROVIDER_SUMMARY_BYTES + 1));
    overflow_response.trace_payload.output_summary =
        Some("o".repeat(MAX_PROVIDER_SUMMARY_BYTES + 1));
    overflow_response.trace_payload.transcript = (0..=MAX_PROVIDER_TRANSCRIPT_ENTRIES)
        .map(|index| ProviderTranscriptEntry {
            role: "agent".to_string(),
            content: format!(
                "transcript-{index}-{}",
                "t".repeat(MAX_PROVIDER_TRANSCRIPT_ENTRY_BYTES)
            ),
        })
        .collect();
    overflow_response.trace_payload.tool_trace = (0..=MAX_PROVIDER_TOOL_TRACE_ENTRIES)
        .map(|index| {
            format!(
                "tool-{index}-{}",
                "u".repeat(MAX_PROVIDER_TOOL_TRACE_ENTRY_BYTES)
            )
        })
        .collect();
    overflow_response.trace_payload.upstream_trace = Some(json!({
        "trace": "x".repeat(MAX_PROVIDER_TRACE_BYTES)
    }));

    let mut overflow_runner = provider_backed_runner_with_response(overflow_response);
    overflow_runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open overflow fixture turn");
    let overflow_outcome = completed_turn(&mut overflow_runner);
    assert_eq!(
        overflow_outcome.decision,
        Some(crate::simulator::AgentDecision::WaitTicks(7)),
        "non-authority trace overflow must not change the candidate decision"
    );
    let overflow_trace = overflow_outcome
        .decision_trace
        .expect("overflow fixture retains a bounded decision trace");
    assert!(overflow_trace.llm_chat_messages.len() <= MAX_PROVIDER_TRANSCRIPT_ENTRIES);
    assert!(overflow_trace.llm_step_trace.len() <= MAX_PROVIDER_TOOL_TRACE_ENTRIES);
    assert!(
        overflow_trace
            .llm_chat_messages
            .iter()
            .all(|entry| entry.content.len() <= MAX_PROVIDER_TRANSCRIPT_ENTRY_BYTES)
    );
    assert!(overflow_trace.llm_step_trace.iter().all(|entry| {
        entry.input_summary.len() <= MAX_PROVIDER_TOOL_TRACE_ENTRY_BYTES
            && entry.output_summary.len() <= MAX_PROVIDER_TOOL_TRACE_ENTRY_BYTES
    }));
    if let Some(input) = overflow_trace.llm_input.as_ref() {
        assert!(input.len() <= MAX_PROVIDER_SUMMARY_BYTES);
    }
    if let Some(output) = overflow_trace.llm_output.as_ref() {
        assert!(output.len() <= MAX_PROVIDER_SUMMARY_BYTES);
    }
    assert!(
        serde_json::to_vec(&overflow_trace)
            .expect("serialize bounded trace")
            .len()
            <= MAX_PROVIDER_TRACE_BYTES,
        "retained provider trace must stay within the canonical aggregate bound"
    );
    assert!(
        overflow_trace
            .llm_error
            .as_deref()
            .unwrap_or_default()
            .contains("trace_payload_too_large"),
        "overflow must retain the stable trace_payload_too_large diagnostic"
    );
}

#[test]
fn upstream_trace_over_four_kib_retains_overflow_diagnostic() {
    let mut response = wait_response_with_memory_intent();
    response.trace_payload.upstream_trace = Some(json!({
        "provider_payload": format!("{}{}", "x".repeat(8 * 1024), "late-provider-secret"),
    }));
    let mut runner = provider_backed_runner_with_response(response);
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open upstream overflow fixture turn");
    let outcome = completed_turn(&mut runner);
    let trace = outcome
        .decision_trace
        .expect("overflow fixture retains trace");
    let serialized_trace = serde_json::to_string(&trace).expect("serialize bounded trace");
    assert!(
        !serialized_trace.contains("late-provider-secret"),
        "an oversized provider trace must be omitted before a truncated JSON prefix can leak it"
    );
    assert!(
        trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.contains("trace_payload_too_large"))
    );
}

#[test]
fn oversized_provider_trace_is_bounded_before_redaction_and_summary_copy() {
    let mut response = wait_response_with_memory_intent();
    response.trace_payload.upstream_trace = Some(json!({
        "provider_payload": format!(
            "{}{}",
            "x".repeat(64 * 1024),
            "late-provider-secret"
        ),
    }));
    let mut runner = provider_backed_runner_with_response(response);
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open oversized trace fixture turn");
    let outcome = completed_turn(&mut runner);
    let trace = outcome
        .decision_trace
        .expect("oversized trace still produces bounded diagnostics");
    let serialized_trace = serde_json::to_string(&trace).expect("serialize bounded trace");
    assert!(!serialized_trace.contains("late-provider-secret"));
    assert!(trace.llm_output.as_ref().is_some_and(|output| {
        output.len() <= MAX_PROVIDER_SUMMARY_BYTES && output.contains("omitted")
    }));
    assert!(
        trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.contains("trace_payload_too_large"))
    );
}

#[test]
fn provider_failure_is_not_reported_as_a_successful_wait() {
    let mut runner = provider_failure_runner();
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open provider-backed target turn");

    let outcome = completed_turn(&mut runner);

    assert_eq!(outcome.lifecycle, AsyncTurnLifecycle::Failed);
    assert_eq!(
        outcome.feedback,
        AsyncTurnFeedback::ProviderError {
            code: "provider_timeout".to_string()
        }
    );
    assert_eq!(outcome.world_effect, AsyncWorldEffect::NoEffect);
    assert!(outcome.decision.is_none());
}

#[test]
fn builtin_and_provider_backed_target_actors_consume_host_goal_not_memory_as_goal() {
    let context = host_context();
    let expected_goal = context.goal_snapshot.clone();
    let expected_memory = context.memory_snapshot.clone();

    for mut runner in [
        AsyncAgentRunner::builtin_fixture(AGENT_ID),
        provider_backed_runner(),
    ] {
        let _turn_id = runner
            .start_turn_with_context(AGENT_ID, context.clone())
            .expect("open target cognition turn");
        let outcome = completed_turn(&mut runner);
        let prepared = outcome
            .prepared_context
            .expect("target actor must retain prepared host context");

        assert_eq!(prepared.goal_snapshot, expected_goal);
        assert_eq!(prepared.memory_snapshot, expected_memory);
        assert_ne!(
            prepared.goal_snapshot.short_term_summary, prepared.memory_snapshot.entries[0].summary,
            "memory retrieval must never be projected as mission goal"
        );
        assert_eq!(
            outcome.world_effect,
            crate::simulator::AsyncWorldEffect::NoEffect
        );
    }
}

#[test]
fn invalid_memory_after_committed_receipt_preserves_runtime_commit_disposition() {
    let mut response = wait_response_with_memory_intent();
    response.memory_write_intents[0].scope = "global_shared".to_string();
    let mut runner = provider_backed_runner_with_response(response);
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open provider-backed target turn");
    let outcome = completed_turn(&mut runner);
    let mut committed = outcome
        .feedback_for_runtime_status("committed", Some("receipt-invalid-memory-1"))
        .expect("build committed feedback for invalid memory projection");
    committed.candidate_action_id = Some(7);
    committed.provenance = "runtime_authoritative".to_string();
    let receipt = runtime_receipt_for_feedback(&committed);
    let response_identity = outcome
        .prepared_response_context
        .as_ref()
        .expect("response context retained")
        .response_artifact_identity();
    let mut store = MemoryWriteStore::default();

    runner
        .consume_runtime_feedback_with_world_readback(
            AGENT_ID,
            committed,
            &receipt,
            &response_identity,
            &VerifiedWorldReadback,
            &mut store,
        )
        .expect("invalid provider memory must not rewrite committed Runtime feedback");
    assert!(
        store.entries().is_empty(),
        "invalid post-commit memory intent must be skipped without a write"
    );
}

#[test]
fn committed_feedback_without_awaiting_outcome_cannot_use_compatibility_lineage() {
    let mut runner = provider_backed_runner();
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open provider-backed target turn");
    let outcome = completed_turn(&mut runner);
    let mut committed = outcome
        .feedback_for_runtime_status("committed", Some("receipt-readback-missing"))
        .expect("build correlated committed feedback");
    committed.candidate_action_id = Some(7);
    committed.provenance = "runtime_authoritative".to_string();
    let context = outcome
        .prepared_context
        .as_ref()
        .expect("outcome retains host context");
    runner
        .expire_runtime_turn(
            AGENT_ID,
            context.agent_session_id.as_str(),
            context.agent_turn_id.as_str(),
            context.decision_request_id.as_str(),
            context.request_digest.to_string().as_str(),
        )
        .expect("explicit lease expiry removes the awaiting outcome");
    let receipt = runtime_receipt_for_feedback(&committed);
    let response_identity = outcome
        .prepared_response_context
        .as_ref()
        .expect("response context retained")
        .response_artifact_identity();
    let mut store = MemoryWriteStore::default();
    let error = runner
        .consume_runtime_feedback_with_world_readback(
            AGENT_ID,
            committed,
            &receipt,
            &response_identity,
            &VerifiedWorldReadback,
            &mut store,
        )
        .expect_err("missing awaiting outcome must not use compatibility lineage");
    assert!(
        error
            .to_string()
            .contains("Runtime readback requires an awaiting outcome"),
        "unexpected missing-outcome error: {error}"
    );
    assert!(store.entries().is_empty());
}

fn assert_non_committed_status_does_not_write(status: &str) {
    let mut runner = provider_backed_runner();
    let _turn_id = runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open provider-backed target turn");
    let outcome = completed_turn(&mut runner);
    assert_eq!(outcome.memory_write_intents.len(), 1);

    let mut store = MemoryWriteStore::default();
    let mut feedback = outcome
        .feedback_for_runtime_status(status, None)
        .expect("build correlated non-committed feedback");
    feedback.provenance = "runtime_authoritative".to_string();
    let context = outcome
        .prepared_context
        .as_ref()
        .expect("outcome retains host context");
    assert_eq!(feedback.agent_turn_id, context.agent_turn_id);
    assert_eq!(feedback.decision_request_id, context.decision_request_id);
    runner
        .consume_runtime_feedback(AGENT_ID, feedback, &mut store)
        .expect("non-committed feedback is a no-write outcome");
    assert!(
        store.entries().is_empty(),
        "status={status} must not write memory"
    );
}

#[test]
fn target_actor_continuation_is_runtime_owned_and_rejected_continuation_cannot_reenter() {
    let proposal = continuation_proposal();
    let mut runner = AsyncAgentRunner::builtin_fixture(AGENT_ID);

    let accepted = runner
        .submit_continuation_proposal(AGENT_ID, proposal.clone())
        .expect("submit Harness continuation proposal");
    assert_eq!(accepted.status, "scheduled");
    assert!(accepted.continuation_id.is_empty());
    assert!(accepted.wake_id.is_empty());
    assert_eq!(accepted.wake_seq, 0);
    assert!(accepted.continuation_digest.is_empty());
    assert!(accepted.continuation_status_digest.is_empty());
    assert_eq!(accepted.provenance, "harness_policy");
    assert!(!accepted.world_effect);
    assert_ne!(
        accepted.continuation_status_digest, proposal.proposal_digest,
        "Runtime must own status digest instead of echoing Harness proposal digest"
    );

    let error = runner
        .apply_runtime_continuation_projection(
            AGENT_ID,
            rejected_runtime_projection(&accepted.proposal),
        )
        .expect_err("legacy projection cannot claim Runtime authority");
    assert!(error.to_string().contains("strict continuation"));
}

#[test]
fn terminal_feedback_invalidates_continuation_and_cannot_resurrect_old_turn() {
    let proposal = continuation_proposal();
    let mut runner = AsyncAgentRunner::builtin_fixture(AGENT_ID);
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open turn");
    let outcome = completed_turn(&mut runner);
    runner
        .submit_continuation_proposal(AGENT_ID, proposal.clone())
        .expect("admit compatibility continuation after the outcome");
    let mut feedback = outcome
        .feedback_for_runtime_status("rejected", None)
        .expect("build terminal feedback");
    feedback.provenance = "runtime_authoritative".to_string();
    runner
        .consume_runtime_feedback(AGENT_ID, feedback, &mut MemoryWriteStore::default())
        .expect("terminal feedback releases the old turn");
    let replay_error = runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect_err("terminal feedback must close the original request lineage");
    assert!(
        replay_error.to_string().contains("request_replay"),
        "terminal feedback must not revive the same request: {replay_error}"
    );
    let error = runner
        .submit_continuation_proposal(AGENT_ID, proposal)
        .expect_err("terminal chain cannot be resurrected by the same proposal");
    assert!(error.to_string().contains("terminal"));
}

fn runtime_receipt_for_feedback(feedback: &FeedbackEnvelopeV1) -> RuntimeReceiptLineageV1 {
    RuntimeReceiptLineageV1 {
        schema_version: RuntimeReceiptLineageV1::SCHEMA_VERSION.to_string(),
        status: "committed".to_string(),
        receipt_id: feedback
            .runtime_receipt_id
            .clone()
            .expect("committed feedback carries Runtime receipt ID"),
        receipt_digest: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
            .to_string(),
        envelope_digest: "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            .to_string(),
        action_id: feedback
            .candidate_action_id
            .expect("committed feedback carries Runtime action ID")
            .to_string(),
        agent_id: feedback.agent_subject.clone(),
        agent_session_id: feedback.agent_session_id.clone(),
        agent_turn_id: feedback.agent_turn_id.clone(),
        decision_request_id: feedback.decision_request_id.clone(),
        request_digest: feedback.request_digest.to_string(),
        feedback_id: feedback.feedback_id.clone(),
    }
}

#[test]
fn memory_correction_links_only_matching_committed_decision_across_restart_and_retry() {
    let mut runner = provider_backed_runner();
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .unwrap();
    let outcome = completed_turn(&mut runner);
    let mut feedback = outcome
        .feedback_for_runtime_status("committed", Some("receipt-correction-source"))
        .unwrap();
    feedback.candidate_action_id = Some(91);
    feedback.provenance = "runtime_authoritative".into();
    let receipt = runtime_receipt_for_feedback(&feedback);
    let response_identity = outcome
        .prepared_response_context
        .as_ref()
        .expect("response context retained for committed turn")
        .response_artifact_identity();
    let mut store = MemoryWriteStore::default();
    runner
        .consume_runtime_feedback_with_world_readback(
            AGENT_ID,
            feedback,
            &receipt,
            &response_identity,
            &VerifiedWorldReadback,
            &mut store,
        )
        .unwrap();
    let snapshot = store.context_snapshot(AGENT_ID, SESSION_ID, "session_private", 8);
    let input = json!({
        "correction_id": "correction-1", "agent_id": AGENT_ID,
        "agent_session_id": SESSION_ID, "scope": "session_private",
        "target_memory_id": snapshot.entries[0].id, "expected_revision": snapshot.revision,
        "replacement_summary": "the previous prediction was wrong", "status": "applied",
        "reason": "untrusted", "memory_revision": 999,
        "earliest_decision_request_id": null, "earliest_request_digest": null,
        "runtime_receipt_id": "forged", "action_id": "forged"
    });
    let correction = serde_json::from_value(input.clone()).unwrap();
    let accepted = store.correct_memory(correction).unwrap();
    assert_eq!(accepted.status, "accepted");
    assert!(accepted.runtime_receipt_id.is_none());
    let mut turn = host_context();
    turn.decision_request_id = "next-corrected-decision".into();
    turn.memory_snapshot = store.context_snapshot(AGENT_ID, SESSION_ID, "session_private", 8);
    store.bind_corrections_to_decision(&turn).unwrap();
    let mut restored: MemoryWriteStore =
        serde_json::from_value(serde_json::to_value(&store).unwrap()).unwrap();
    restored.finalize_corrections(&receipt).unwrap();
    assert_eq!(
        restored.corrections()[0].status,
        "accepted",
        "old receipt cannot apply correction"
    );
    let mut matching = receipt.clone();
    matching.decision_request_id = turn.decision_request_id.clone();
    matching.receipt_id = "receipt-corrected-decision".into();
    matching.action_id = "92".into();
    let mut wrong = matching.clone();
    wrong.agent_session_id = "other-session".into();
    restored.finalize_corrections(&wrong).unwrap();
    assert_eq!(restored.corrections()[0].status, "accepted");
    wrong = matching.clone();
    wrong.request_digest =
        "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
    restored.finalize_corrections(&wrong).unwrap();
    assert_eq!(restored.corrections()[0].status, "accepted");
    // New unrelated turns cannot hijack the first prepared binding.
    let mut replacement = turn.clone();
    replacement.decision_request_id = "stale-parent-replacement".into();
    restored.bind_corrections_to_decision(&replacement).unwrap();
    assert_eq!(
        restored.corrections()[0]
            .active_decision_request_id
            .as_deref(),
        Some(turn.decision_request_id.as_str())
    );
    restored
        .rebind_corrections_after_stale_parent(
            "wrong-parent",
            &turn.request_digest.to_string(),
            &replacement,
        )
        .unwrap();
    assert_eq!(
        restored.corrections()[0]
            .active_decision_request_id
            .as_deref(),
        Some(turn.decision_request_id.as_str())
    );
    restored
        .rebind_corrections_after_stale_parent(
            &turn.decision_request_id,
            "wrong-digest",
            &replacement,
        )
        .unwrap();
    assert_eq!(
        restored.corrections()[0]
            .active_decision_request_id
            .as_deref(),
        Some(turn.decision_request_id.as_str())
    );
    // Older serialized records use earliest identity as the active fallback.
    let mut legacy = serde_json::to_value(&restored).unwrap();
    let rows = legacy
        .get_mut("corrections")
        .unwrap()
        .as_array_mut()
        .unwrap();
    rows[0]
        .as_object_mut()
        .unwrap()
        .remove("active_decision_request_id");
    rows[0]
        .as_object_mut()
        .unwrap()
        .remove("active_request_digest");
    restored = serde_json::from_value(legacy).unwrap();
    restored
        .rebind_corrections_after_stale_parent(
            &turn.decision_request_id,
            &turn.request_digest.to_string(),
            &replacement,
        )
        .unwrap();
    restored.bind_corrections_to_decision(&replacement).unwrap();
    restored = serde_json::from_value(serde_json::to_value(&restored).unwrap()).unwrap();
    restored.ignore_corrections_for_decision(
        AGENT_ID,
        &turn.decision_request_id,
        &turn.request_digest.to_string(),
        "old-parent-failure",
    );
    restored.finalize_corrections(&matching).unwrap();
    assert_eq!(
        restored.corrections()[0].status,
        "accepted",
        "old parent cannot finalize a replacement"
    );
    matching.decision_request_id = replacement.decision_request_id.clone();
    restored.finalize_corrections(&matching).unwrap();
    restored.finalize_corrections(&matching).unwrap();
    assert_eq!(restored.corrections().len(), 1);
    assert_eq!(restored.corrections()[0].status, "applied");
    assert_eq!(
        restored.corrections()[0]
            .earliest_decision_request_id
            .as_deref(),
        Some(turn.decision_request_id.as_str())
    );
    assert_eq!(
        restored.corrections()[0]
            .committed_decision_request_id
            .as_deref(),
        Some(replacement.decision_request_id.as_str())
    );
    assert_eq!(
        restored.corrections()[0].runtime_receipt_id.as_deref(),
        Some("receipt-corrected-decision")
    );
    let replay = restored
        .correct_memory(serde_json::from_value(input.clone()).unwrap())
        .unwrap();
    assert_eq!(replay.status, "applied");
    let mut stale = input.clone();
    stale["correction_id"] = json!("correction-stale");
    assert_eq!(
        restored
            .correct_memory(serde_json::from_value(stale).unwrap())
            .unwrap()
            .status,
        "stale"
    );
    let mut ignored = input;
    ignored["correction_id"] = json!("correction-other-agent");
    ignored["agent_id"] = json!("other-agent");
    ignored["expected_revision"] = json!(turn.memory_snapshot.revision);
    assert_eq!(
        restored
            .correct_memory(serde_json::from_value(ignored).unwrap())
            .unwrap()
            .status,
        "ignored"
    );
    assert_eq!(
        restored
            .context_snapshot(AGENT_ID, SESSION_ID, "session_private", 8)
            .entries[0]
            .summary,
        "the previous prediction was wrong"
    );
    let mut failed_input = serde_json::to_value(restored.corrections()[0].clone()).unwrap();
    failed_input["correction_id"] = json!("correction-no-effect");
    failed_input["expected_revision"] = json!(turn.memory_snapshot.revision);
    failed_input["replacement_summary"] = json!("new bounded correction");
    restored
        .correct_memory(serde_json::from_value(failed_input).unwrap())
        .unwrap();
    turn.decision_request_id = "failed-next-decision".into();
    turn.memory_snapshot = restored.context_snapshot(AGENT_ID, SESSION_ID, "session_private", 8);
    restored.bind_corrections_to_decision(&turn).unwrap();
    restored.ignore_corrections_for_decision(
        AGENT_ID,
        "wrong-decision",
        turn.request_digest.as_str(),
        "failed",
    );
    assert_eq!(restored.corrections().last().unwrap().status, "accepted");
    restored.ignore_corrections_for_decision(
        AGENT_ID,
        &turn.decision_request_id,
        turn.request_digest.as_str(),
        "provider_turn_failed_without_runtime_effect",
    );
    let ignored = restored.corrections().last().unwrap();
    assert_eq!(ignored.status, "ignored");
    assert!(ignored.runtime_receipt_id.is_none());
    assert_eq!(
        ignored.reason,
        "provider_turn_failed_without_runtime_effect"
    );
}

fn rejected_runtime_projection(proposal: &ContinuationProposalV1) -> AgentContinuation {
    let wake_conditions: Vec<WakeConditionV1> = serde_json::from_value(
        serde_json::to_value(&proposal.wake_conditions).expect("encode proposal wake conditions"),
    )
    .expect("decode Runtime wake conditions");
    let mut runtime = AgentContinuation {
        schema_version: "agent-continuation.v1".to_string(),
        continuation_id: "continuation-runtime-live-harness-1".to_string(),
        wake_id: "wake-runtime-live-harness-1".to_string(),
        world_id: proposal.world_id.clone(),
        branch_id: "branch-live-harness".to_string(),
        finality_epoch: 1,
        finality_block_hash: Some(
            "blake3:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        ),
        finality_status: "verified".to_string(),
        reorg_epoch: 0,
        runtime_manifest_hash:
            "blake3:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
        agent_id: proposal.agent_id.clone(),
        agent_session_id: proposal.agent_session_id.clone(),
        agent_turn_id: proposal.agent_turn_id.clone(),
        decision_request_id: proposal.decision_request_id.clone(),
        origin_turn_id: proposal.origin_turn_id.clone(),
        origin_request_digest: proposal.origin_request_digest.clone(),
        continuation_proposal_id: proposal.continuation_proposal_id.clone(),
        proposal_digest: proposal.proposal_digest.clone(),
        action_or_envelope_digest: proposal.action_or_envelope_digest.clone(),
        wake_conditions,
        next_wake_tick: Some(10),
        remaining_budget: ContinuationBudgetV1 {
            unit: proposal.remaining_budget.unit.clone(),
            value: proposal.remaining_budget.value,
        },
        valid_until_tick: proposal.valid_until_tick,
        precondition_digest: proposal.precondition_digest.clone(),
        wake_seq: 1,
        logical_tick: 10,
        status: ContinuationStatusV1::Rejected,
        continuation_status_digest: None,
        terminal_disposition: Some("runtime_denied".to_string()),
    };
    runtime.continuation_status_digest = Some(runtime.status_digest());
    runtime
}

fn continuation_proposal() -> ContinuationProposalV1 {
    let mut value: Value = json!({
        "schema_version": 1,
        "continuation_proposal_id": "proposal-live-harness-1",
        "world_id": "world-live-harness",
        "agent_id": AGENT_ID,
        "agent_session_id": SESSION_ID,
        "agent_turn_id": TURN_ID,
        "decision_request_id": REQUEST_ID,
        "origin_turn_id": TURN_ID,
        "origin_request_digest": REQUEST_DIGEST,
        "action_or_plan_kind": "wait_for_receipt",
        "action_or_envelope_digest": null,
        "remaining_budget": {"unit": "steps", "value": 2},
        "baseline_observation_digest": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "goal_digest": host_goal().digest,
        "policy_digest": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "policy_revision": 1,
        "precondition_summary": "receipt pending",
        "precondition_digest": "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "wake_conditions": [{
            "schema_version": "wake-condition.v1",
            "kind": "receipt_linked",
            "receipt_id": "blake3:9999999999999999999999999999999999999999999999999999999999999999"
        }],
        "valid_until_tick": 100,
        "source": "harness",
        "proposal_digest": null
    });
    let mut proposal: ContinuationProposalV1 = serde_json::from_value({
        value["proposal_digest"] = json!("");
        value
    })
    .expect("decode continuation proposal");
    proposal.proposal_digest = proposal
        .proposal_digest()
        .expect("canonical continuation proposal digest")
        .to_string();
    proposal
}
