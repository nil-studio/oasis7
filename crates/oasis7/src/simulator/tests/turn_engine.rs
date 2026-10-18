use crate::simulator::{
    AgentBehavior, AgentDecision, AgentDecisionTrace, AsyncAgentTurnOutcome, AsyncTurnFeedback,
    AsyncTurnId, AsyncTurnLifecycle, AsyncWorldEffect, FeedbackEnvelopeV1, MemoryWriteStore,
    TurnEngine, TurnEngineError, TurnEnginePhase, TurnRequest,
};

use super::agent_cognition_identity::{
    production_observation, production_request_fixture, production_turn_context, request_from_value,
};
use crate::runtime::{
    COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION, COGNITION_RESOURCE_VERSION_V1,
    CognitionLeaseQuoteV1, CognitionLeaseRequestV1,
};
use crate::simulator::{ContinuousAgentRequestContextV1, ContinuousAgentTurnContextV1};
use oasis7_simulator::Observation;
use serde_json::json;
use std::sync::{Arc, Mutex};

struct WaitBehavior;

impl AgentBehavior for WaitBehavior {
    fn agent_id(&self) -> &str {
        "agent-1"
    }

    fn decide(&mut self, _observation: &oasis7_simulator::Observation) -> AgentDecision {
        AgentDecision::Wait
    }
}

#[derive(Default)]
struct CapturedTurn {
    observation: Option<Observation>,
    turn_context: Option<ContinuousAgentTurnContextV1>,
    request_context_hook: Option<Option<ContinuousAgentRequestContextV1>>,
}

struct CapturingWaitBehavior(Arc<Mutex<CapturedTurn>>);

impl AgentBehavior for CapturingWaitBehavior {
    fn agent_id(&self) -> &str {
        "agent-1"
    }

    fn decide(&mut self, observation: &Observation) -> AgentDecision {
        self.0.lock().expect("capture lock").observation = Some(observation.clone());
        AgentDecision::Wait
    }

    fn set_continuous_turn_context(&mut self, context: Option<&ContinuousAgentTurnContextV1>) {
        self.0.lock().expect("capture lock").turn_context = context.cloned();
    }

    fn set_continuous_request_context(
        &mut self,
        context: Option<&ContinuousAgentRequestContextV1>,
    ) {
        self.0.lock().expect("capture lock").request_context_hook = Some(context.cloned());
    }
}

fn request() -> TurnRequest {
    let mut fixture = production_request_fixture(1, 60_000);
    fixture["retry_seq"] = json!(1);
    let request_context = request_from_value(fixture);
    let turn_context = production_turn_context(&request_context);
    TurnRequest::new(
        production_observation("agent-1", 42),
        turn_context,
        request_context,
    )
}

fn legacy_compatibility_request() -> TurnRequest {
    let mut fixture = production_request_fixture(0, 60_000);
    fixture["retry_seq"] = json!(0);
    let base_decision_request = fixture["base_decision_request"]
        .as_object_mut()
        .expect("base decision request object");
    base_decision_request.remove("capability_catalog");
    base_decision_request.remove("capability_invocation_context");
    let request_context = request_from_value(fixture);
    let turn_context = production_turn_context(&request_context);
    TurnRequest::new(
        production_observation("agent-1", 42),
        turn_context,
        request_context,
    )
}

fn runtime_lease(request: &TurnRequest) -> crate::runtime::CognitionLeaseV1 {
    const ACCOUNT_ID: &str = "account-agent-1";
    const RESOURCE: &str = "cognition_units";

    let mut world = crate::runtime_host::world::new();
    world
        .set_cognition_resource_balance(ACCOUNT_ID, RESOURCE, 4)
        .expect("seed Runtime cognition balance");
    let quote = CognitionLeaseQuoteV1::new("turn-engine-lease-quote", RESOURCE, 2).with_authority(
        ACCOUNT_ID,
        COGNITION_RESOURCE_VERSION_V1,
        "provider_cognition",
        "agent_turn",
        COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
        request
            .request_context
            .capability_invocation_context_digest
            .to_string(),
        request
            .request_context
            .runtime_binding
            .base_world_hash
            .to_string(),
    );
    world
        .reserve_cognition_lease(CognitionLeaseRequestV1::new(
            "turn-engine-lease-idempotency",
            ACCOUNT_ID,
            request.agent_id.clone(),
            request.request_context.agent_session_id.clone(),
            request.request_context.agent_turn_id.clone(),
            request.request_context.decision_request_id.clone(),
            request.request_context.request_digest.to_string(),
            quote,
        ))
        .expect("reserve Runtime cognition lease")
}

fn completed_wait_turn() -> (TurnEngine, AsyncTurnId, AsyncAgentTurnOutcome) {
    let mut engine = TurnEngine::with_default_capacity();
    engine.register(WaitBehavior).expect("register actor");
    let turn_id = engine.start_turn(request()).expect("start turn");
    let completion = loop {
        if let Some(completion) = engine.poll_completed().expect("poll actor").pop() {
            break completion;
        }
        std::thread::yield_now();
    };
    (engine, turn_id, completion.outcome)
}

#[test]
fn turn_engine_keeps_one_in_flight_turn_and_exposes_pending_phase() {
    let mut engine = TurnEngine::with_default_capacity();
    engine.register(WaitBehavior).expect("register actor");
    let turn_id = engine.start_turn(request()).expect("start turn");
    assert_eq!(engine.phase(turn_id), Some(TurnEnginePhase::InFlight));
    assert_eq!(engine.active_turn_count(), 1);
    let second = engine.start_turn(request());
    assert!(second.is_err(), "same agent must remain single-flight");

    let completion = loop {
        if let Some(completion) = engine.poll_completed().expect("poll actor").pop() {
            break completion;
        }
        std::thread::yield_now();
    };
    assert_eq!(completion.turn_id, turn_id);
    assert_eq!(completion.phase, TurnEnginePhase::Pending);
    assert_eq!(completion.outcome.agent_id, "agent-1");
    assert_eq!(completion.outcome.lifecycle, AsyncTurnLifecycle::Completed);
    assert_eq!(completion.outcome.feedback, AsyncTurnFeedback::Wait);
    assert_eq!(completion.outcome.world_effect, AsyncWorldEffect::NoEffect);
    assert_eq!(
        engine.active_turn_count(),
        1,
        "Runtime disposition is pending"
    );
}

#[test]
fn committed_feedback_requires_runtime_world_readback_proof() {
    let (mut engine, turn_id, outcome) = completed_wait_turn();
    let mut feedback = outcome
        .feedback_for_runtime_status("committed", Some("receipt-1"))
        .expect("build committed feedback");
    feedback.candidate_action_id = Some(7);
    feedback.provenance = "runtime_authoritative".to_string();
    let error = engine
        .accept_runtime_feedback(turn_id, feedback, None, &mut MemoryWriteStore::default())
        .expect_err("caller receipt alone cannot authorize committed feedback");
    assert_eq!(error, TurnEngineError::RuntimeReadbackRequired(turn_id));
}

#[test]
fn stale_base_feedback_enters_stale_phase() {
    let (mut engine, turn_id, outcome) = completed_wait_turn();
    let mut feedback = outcome
        .feedback_for_runtime_status("rejected", None)
        .expect("build rejected feedback");
    feedback.reject_reason = Some("stale_base".to_string());
    feedback.provenance = "runtime_authoritative".to_string();
    let phase = engine
        .accept_runtime_feedback(
            turn_id,
            FeedbackEnvelopeV1 { ..feedback },
            None,
            &mut MemoryWriteStore::default(),
        )
        .expect("accept authoritative stale feedback");
    assert_eq!(phase, TurnEnginePhase::Stale);
    assert_eq!(engine.phase(turn_id), Some(TurnEnginePhase::Stale));
}

#[test]
fn native_turn_engine_admits_reduced_compatibility_context_without_production_lineage() {
    let request = legacy_compatibility_request();
    request
        .request_context
        .validate()
        .expect("outer context remains valid");
    assert_eq!(request.request_context.retry_seq, 0);
    assert_eq!(request.request_context.transport_attempt, 0);
    assert!(
        request
            .request_context
            .base_decision_request
            .capability_catalog
            .is_none()
    );

    let mut engine = TurnEngine::with_default_capacity();
    engine.register(WaitBehavior).expect("register actor");
    let turn_id = engine
        .start_turn(request)
        .expect("legacy native facade admits its reduced compatibility lane");
    assert_eq!(engine.phase(turn_id), Some(TurnEnginePhase::InFlight));
}

#[test]
fn native_turn_engine_preserves_observation_and_reduced_context_without_request_promotion() {
    let request = legacy_compatibility_request();
    let expected_observation = request.observation.clone();
    let expected_turn_context = request.turn_context.clone();
    let captured = Arc::new(Mutex::new(CapturedTurn::default()));

    let mut engine = TurnEngine::with_default_capacity();
    engine
        .register(CapturingWaitBehavior(Arc::clone(&captured)))
        .expect("register actor");
    let turn_id = engine
        .start_turn(request)
        .expect("start compatibility turn");
    let completion = loop {
        if let Some(completion) = engine.poll_completed().expect("poll actor").pop() {
            break completion;
        }
        std::thread::yield_now();
    };

    assert_eq!(completion.turn_id, turn_id);
    assert_eq!(
        completion.outcome.prepared_context,
        Some(expected_turn_context.clone())
    );
    assert!(completion.outcome.prepared_request_context.is_none());
    assert!(completion.outcome.cognition_lease.is_none());
    let captured = captured.lock().expect("capture lock");
    assert_eq!(captured.observation.as_ref(), Some(&expected_observation));
    assert_eq!(captured.turn_context.as_ref(), Some(&expected_turn_context));
    assert_eq!(captured.request_context_hook, Some(None));
}

#[test]
fn native_turn_engine_rejects_outer_context_mismatch_before_dispatch() {
    let mut request = legacy_compatibility_request();
    request.request_context.runtime_binding.branch_id = "different-branch".to_string();
    request.request_context.request_digest = request.request_context.request_digest();
    let captured = Arc::new(Mutex::new(CapturedTurn::default()));

    let mut engine = TurnEngine::with_default_capacity();
    engine
        .register(CapturingWaitBehavior(Arc::clone(&captured)))
        .expect("register actor");
    let error = engine
        .start_turn(request)
        .expect_err("outer/reduced identities must remain correlated");
    assert!(matches!(error, TurnEngineError::Runner(_)));
    assert_eq!(engine.active_turn_count(), 0);
    assert!(captured.lock().expect("capture lock").observation.is_none());
}

#[test]
fn native_turn_engine_rejects_runtime_lease_before_compatibility_dispatch() {
    let mut request = request();
    request.cognition_lease = Some(runtime_lease(&request));
    let captured = Arc::new(Mutex::new(CapturedTurn::default()));

    let mut engine = TurnEngine::with_default_capacity();
    engine
        .register(CapturingWaitBehavior(Arc::clone(&captured)))
        .expect("register actor");
    let error = engine
        .start_turn(request)
        .expect_err("native compatibility facade cannot admit Runtime leases");
    assert!(matches!(error, TurnEngineError::Runner(_)));
    assert_eq!(engine.active_turn_count(), 0);
    assert!(captured.lock().expect("capture lock").observation.is_none());
}

#[allow(dead_code)]
fn _outcome_shape_is_stable(outcome: AsyncAgentTurnOutcome) -> Option<AgentDecisionTrace> {
    outcome.decision_trace
}
