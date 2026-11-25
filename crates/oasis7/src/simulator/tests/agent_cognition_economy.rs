//! Focused Agent-side admission tests for Runtime-owned cognition leases.

use super::restore_runner_feedback_history;
use crate::runtime::{
    CognitionLeaseQuoteV1, CognitionLeaseRequestV1, CognitionLeaseStatusV1, CognitionLeaseV1, World,
};
use crate::simulator::{
    ActionCatalogEntry, AsyncAgentRunner, AsyncAgentTurnOutcome, DecisionResponse,
    MockDecisionProvider, ProviderBackedAgentBehavior, ProviderExecutionMode,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ACCOUNT_ID: &str = "account-agent-1";
const RESOURCE: &str = "cognition_units";

fn authority_quote(
    request: &crate::simulator::ContinuousAgentRequestContextV1,
    quote_id: &str,
    amount: u64,
) -> CognitionLeaseQuoteV1 {
    CognitionLeaseQuoteV1::new(quote_id, RESOURCE, amount).with_authority(
        ACCOUNT_ID,
        crate::runtime::COGNITION_RESOURCE_VERSION_V1,
        "provider_cognition",
        "agent_turn",
        crate::runtime::COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
        request.capability_invocation_context_digest.to_string(),
        request.runtime_binding.base_world_hash.to_string(),
    )
}

fn request_context() -> crate::simulator::ContinuousAgentRequestContextV1 {
    let mut fixture = super::agent_cognition_identity::production_request_fixture(1, 60_000);
    fixture["retry_seq"] = serde_json::json!(1);
    super::agent_cognition_identity::request_from_value(fixture)
}

fn turn_context(
    request: &crate::simulator::ContinuousAgentRequestContextV1,
) -> crate::simulator::ContinuousAgentTurnContextV1 {
    super::agent_cognition_identity::production_turn_context(request)
}

fn observation() -> oasis7_simulator::Observation {
    super::agent_cognition_identity::production_observation("agent-1", 42)
}

fn runner_with_state() -> (
    AsyncAgentRunner,
    Arc<Mutex<crate::simulator::MockDecisionProviderState>>,
) {
    let provider = MockDecisionProvider::with_scripted_responses(
        "lease-admission-provider",
        vec![Ok(DecisionResponse::wait("lease-admission-provider"))],
    );
    let state = provider.shared_state();
    let behavior = ProviderBackedAgentBehavior::new(
        "agent-1",
        provider,
        vec![ActionCatalogEntry::new("wait", "wait without world effect")],
    )
    .legacy_compatibility()
    .with_execution_mode(ProviderExecutionMode::HeadlessAgent);
    let mut runner = AsyncAgentRunner::new(16).expect("create actor runner");
    runner.register(behavior).expect("register actor");
    (runner, state)
}

fn reserve_lease(
    world: &mut World,
    request: &crate::simulator::ContinuousAgentRequestContextV1,
    idempotency_key: &str,
    quote: CognitionLeaseQuoteV1,
) -> CognitionLeaseV1 {
    world
        .set_cognition_resource_balance(ACCOUNT_ID, RESOURCE, 4)
        .expect("seed Runtime cognition balance");
    world
        .reserve_cognition_lease(CognitionLeaseRequestV1::new(
            idempotency_key,
            ACCOUNT_ID,
            request.agent_subject.clone(),
            request.agent_session_id.clone(),
            request.agent_turn_id.clone(),
            request.decision_request_id.clone(),
            request.request_digest.to_string(),
            quote,
        ))
        .expect("reserve Runtime cognition lease")
}

fn completed_turn(
    runner: &mut AsyncAgentRunner,
    turn_id: crate::simulator::AsyncTurnId,
) -> AsyncAgentTurnOutcome {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll actor")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            return outcome;
        }
        assert!(
            Instant::now() < deadline,
            "actor did not complete before timeout"
        );
        std::thread::yield_now();
    }
}

#[test]
fn valid_runtime_lease_is_required_before_provider_dispatch_and_carried_to_outcome() {
    let request = request_context();
    let context = turn_context(&request);
    let mut world = crate::runtime_host::world::new();
    let lease = reserve_lease(
        &mut world,
        &request,
        &request.provider_invocation_key().to_string(),
        authority_quote(&request, "lease-admission-quote", 1),
    );
    let (mut runner, state) = runner_with_state();
    restore_runner_feedback_history(&mut runner, &world);

    let turn_id = runner
        .start_turn_with_request_context_and_observation_and_lease(
            "agent-1",
            observation(),
            context,
            request,
            lease.clone(),
        )
        .expect("Runtime-reserved lease admits provider dispatch");
    let outcome = completed_turn(&mut runner, turn_id);
    assert_eq!(outcome.cognition_lease, Some(lease.clone()));
    assert_eq!(
        outcome.world_effect,
        crate::simulator::AsyncWorldEffect::NoEffect
    );
    assert_eq!(
        state
            .lock()
            .expect("provider state lock")
            .recorded_requests
            .len(),
        1,
        "a valid Runtime lease admits exactly one provider call"
    );

    let economy = world.cognition_economy().expect("read Runtime economy");
    assert_eq!(economy.reserved_balance(ACCOUNT_ID, RESOURCE), 1);
    assert_eq!(
        economy.receipts.len(),
        1,
        "Runtime reserve emits the admission receipt before the Agent runner settles"
    );
    assert!(economy.receipts.values().any(|receipt| {
        receipt.operation == "reserve" && receipt.status == CognitionLeaseStatusV1::Reserved
    }));
    let receipt = world
        .settle_cognition_lease(&lease.lease_id, 1)
        .expect("Runtime settles the lease");
    assert_eq!(receipt.status, CognitionLeaseStatusV1::Settled);
    assert_eq!(
        world
            .cognition_economy()
            .expect("read settled economy")
            .receipts
            .len(),
        2,
        "settlement appends one terminal receipt to the reserve receipt"
    );
}

#[test]
fn invalid_or_closed_lease_fences_dispatch_without_provider_call() {
    let request = request_context();
    let context = turn_context(&request);
    let mut world = crate::runtime_host::world::new();
    let lease = reserve_lease(
        &mut world,
        &request,
        &request.provider_invocation_key().to_string(),
        authority_quote(&request, "lease-closed-quote", 1),
    );
    world
        .release_cognition_lease(&lease.lease_id)
        .expect("Runtime closes lease");
    let closed_lease = world
        .cognition_economy()
        .expect("read closed lease")
        .leases
        .get(&lease.lease_id)
        .cloned()
        .expect("closed lease remains durable");
    let (mut runner, state) = runner_with_state();

    let error = runner
        .start_turn_with_request_context_and_observation_and_lease(
            "agent-1",
            observation(),
            context,
            request,
            closed_lease,
        )
        .expect_err("closed lease must not reach provider dispatch");
    assert!(error.to_string().contains("cognition_lease_not_reserved"));
    assert_eq!(runner.active_turn_count(), 0);
    assert_eq!(
        state
            .lock()
            .expect("provider state lock")
            .recorded_requests
            .len(),
        0,
        "invalid lease must prevent provider calls"
    );
}

#[test]
fn lease_identity_mismatch_and_expiry_fence_provider_without_world_effect() {
    let request = request_context();
    let context = turn_context(&request);

    let mut world = crate::runtime_host::world::new();
    let mismatched = world
        .set_cognition_resource_balance(ACCOUNT_ID, RESOURCE, 4)
        .and_then(|_| {
            world.reserve_cognition_lease(CognitionLeaseRequestV1::new(
                request.provider_invocation_key().to_string(),
                ACCOUNT_ID,
                "agent-1",
                request.agent_session_id.clone(),
                request.agent_turn_id.clone(),
                request.decision_request_id.clone(),
                "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                authority_quote(&request, "lease-mismatch-quote", 1),
            ))
        })
        .expect("reserve mismatched Runtime lease");
    let (mut runner, state) = runner_with_state();
    let error = runner
        .start_turn_with_request_context_and_observation_and_lease(
            "agent-1",
            observation(),
            context.clone(),
            request.clone(),
            mismatched,
        )
        .expect_err("lease from another request must be fenced");
    assert!(
        error
            .to_string()
            .contains("cognition_lease_identity_mismatch")
    );

    let mut expiring_world = crate::runtime_host::world::new();
    let expiring = reserve_lease(
        &mut expiring_world,
        &request,
        &request.provider_invocation_key().to_string(),
        authority_quote(&request, "lease-expiry-quote", 1).with_valid_until_tick(0),
    );
    runner
        .step_world_without_waiting_for_provider()
        .expect("advance runner clock");
    let error = runner
        .start_turn_with_request_context_and_observation_and_lease(
            "agent-1",
            observation(),
            context,
            request,
            expiring,
        )
        .expect_err("expired lease must be fenced");
    assert!(error.to_string().contains("cognition_lease_expired"));
    assert_eq!(runner.active_turn_count(), 0);
    assert_eq!(
        state
            .lock()
            .expect("provider state lock")
            .recorded_requests
            .len(),
        0,
        "identity and expiry denial must not call provider"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn api_lease_view_requires_the_complete_runtime_admission_validator() {
    let request = request_context();
    let mut world = crate::runtime_host::world::new();
    let lease = reserve_lease(
        &mut world,
        &request,
        &request.provider_invocation_key().to_string(),
        authority_quote(&request, "lease-api-view", 1),
    );

    let view = crate::simulator::validated_cognition_lease_consumption_view(
        "agent-1", &request, &lease, 42,
    )
    .expect("valid Runtime lease projects to read-only API view");
    assert_eq!(view.schema_version, "cognition-lease-consumption-view.v1");
    assert_eq!(view.lease_id, lease.lease_id);
    assert_eq!(
        view.status,
        crate::runtime::CognitionLeaseStatusV1::Reserved
    );
    let wire = serde_json::to_value(view).expect("encode correlation view");
    for authority_field in [
        "account_id",
        "quote",
        "settled_amount",
        "released_amount",
        "refunded_amount",
        "compensated_amount",
        "net_amount",
        "receipt_id",
    ] {
        assert!(
            wire.get(authority_field).is_none(),
            "leaked {authority_field}"
        );
    }

    let mut mismatched_world = crate::runtime_host::world::new();
    mismatched_world
        .set_cognition_resource_balance(ACCOUNT_ID, RESOURCE, 4)
        .expect("seed Runtime cognition balance for mismatched lease");
    let wrong_identity = mismatched_world
        .reserve_cognition_lease(CognitionLeaseRequestV1::new(
            request.provider_invocation_key().to_string(),
            ACCOUNT_ID,
            "agent-1",
            request.agent_session_id.clone(),
            request.agent_turn_id.clone(),
            request.decision_request_id.clone(),
            "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            authority_quote(&request, "lease-api-view-mismatch", 1),
        ))
        .expect("reserve valid Runtime lease with a mismatched request digest");
    assert!(
        crate::simulator::validated_cognition_lease_consumption_view(
            "agent-1",
            &request,
            &wrong_identity,
            42,
        )
        .expect_err("a nonmatching idempotency key is not projectable")
        .to_string()
        .contains("cognition_lease_identity_mismatch")
    );

    let mut expired_world = crate::runtime_host::world::new();
    let expired = reserve_lease(
        &mut expired_world,
        &request,
        &request.provider_invocation_key().to_string(),
        authority_quote(&request, "lease-api-view-expired", 1).with_valid_until_tick(41),
    );
    assert!(
        crate::simulator::validated_cognition_lease_consumption_view(
            "agent-1", &request, &expired, 42,
        )
        .expect_err("expired quote is not projectable")
        .to_string()
        .contains("cognition_lease_expired")
    );

    world
        .release_cognition_lease(&lease.lease_id)
        .expect("Runtime closes the reserved lease");
    let closed = world
        .cognition_economy()
        .expect("read Runtime lease economy")
        .leases
        .get(&lease.lease_id)
        .cloned()
        .expect("closed Runtime lease remains durable");
    assert!(
        crate::simulator::validated_cognition_lease_consumption_view(
            "agent-1", &request, &closed, 42,
        )
        .expect_err("closed lease is not projectable")
        .to_string()
        .contains("cognition_lease_not_reserved")
    );
}
