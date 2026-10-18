use oasis7::capability_invocation_context::CapabilityInvocationContext;
use oasis7::simulator::{
    BudgetContractV1, COGNITION_CAPABILITY_CATALOG_DOMAIN,
    COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN, CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR,
    CONTINUOUS_AGENT_CONTEXT_VERSION, ContinuationBudgetV1, ContinuationProposalV1,
    ContinuousAgentRequestContextV1, ContinuousAgentTurnContextV1, DecisionRequest, Digest32,
    GoalSnapshotInputV1, GoalSnapshotProjector, MemoryContextSnapshotV1, RuntimeBindingV1,
    WakeConditionV1, h_v1,
};
use oasis7_simulator::Observation;
use oasis7_wasm_abi::CapabilityCatalogSnapshot;
use serde_json::json;

use super::DEFAULT_PROTOCOL_VERSION;
use super::recovery_ledger::RecoveryLineage;

fn retry_observation_digest(observation: &Observation) -> String {
    let mut stable = observation.clone();
    stable.time = 0;
    h_v1("oasis7.cognition.observation.v1", &stable).to_string()
}

/// Stable host-owned chain binding for one target-context benchmark agent.
/// The chain is created once per fixture/session and then carried by every
/// request identity; it is never inferred from a successful action.
pub(super) fn recovery_chain_id(fixture_id: &str, session_id: &str, agent_id: &str) -> String {
    format!("parity-recovery-chain:{fixture_id}:{session_id}:{agent_id}")
}

pub(super) fn recovery_lineage(
    request_context: &ContinuousAgentRequestContextV1,
    recovery_chain_id: &str,
) -> RecoveryLineage {
    RecoveryLineage {
        agent_id: request_context.agent_subject.clone(),
        agent_session_id: request_context.agent_session_id.clone(),
        recovery_chain_id: recovery_chain_id.to_string(),
        agent_turn_id: request_context.agent_turn_id.clone(),
        decision_request_id: request_context.decision_request_id.clone(),
        request_digest: request_context.request_digest.to_string(),
    }
}

pub(super) fn build_target_context(
    base_decision_request: DecisionRequest,
    observation: &Observation,
    fixture_id: &str,
    session_id: &str,
    turn: u64,
) -> (
    ContinuousAgentTurnContextV1,
    ContinuousAgentRequestContextV1,
) {
    build_target_context_inner(
        base_decision_request,
        observation,
        fixture_id,
        session_id,
        turn,
        1,
        None,
    )
}

/// Build a new logical request after a recoverable provider failure. A
/// semantic retry is deliberately distinct from a transport retry: it gets
/// fresh turn/request identities, increments retry_seq, and carries an
/// explicit continuation proposal back to the originating request.
pub(super) fn build_target_context_for_retry(
    base_decision_request: DecisionRequest,
    observation: &Observation,
    fixture_id: &str,
    session_id: &str,
    turn: u64,
    retry_seq: u64,
    origin: &RecoveryLineage,
) -> (
    ContinuousAgentTurnContextV1,
    ContinuousAgentRequestContextV1,
) {
    assert!(retry_seq > 1, "semantic retry_seq must be greater than one");
    build_target_context_inner(
        base_decision_request,
        observation,
        fixture_id,
        session_id,
        turn,
        retry_seq,
        Some(origin),
    )
}

fn build_target_context_inner(
    mut base_decision_request: DecisionRequest,
    observation: &Observation,
    fixture_id: &str,
    session_id: &str,
    turn: u64,
    retry_seq: u64,
    retry_origin: Option<&RecoveryLineage>,
) -> (
    ContinuousAgentTurnContextV1,
    ContinuousAgentRequestContextV1,
) {
    assert!(retry_seq > 0, "target request retry_seq must be nonzero");
    let synthetic_world_id = format!("parity-world-{fixture_id}");
    let synthetic_branch_id = "main".to_string();
    let logical_tick = observation.time;
    let agent_id = observation.agent_id.clone();
    let turn_id = format!("{fixture_id}-turn-{turn}");
    let request_id = format!("{fixture_id}-request-{turn}");
    let runtime_binding = RuntimeBindingV1 {
        world_id: synthetic_world_id,
        branch_id: synthetic_branch_id.clone(),
        finality_epoch: 0,
        finality_block_hash: None,
        finality_status: "pending".to_string(),
        base_tick: logical_tick,
        base_world_hash: h_v1(
            "oasis7.parity.world-binding.v1",
            &base_decision_request.observation,
        ),
        reorg_epoch: 0,
        runtime_manifest_hash: h_v1(
            "oasis7.parity.runtime-manifest.v1",
            &json!({"fixture_id": fixture_id, "branch_id": synthetic_branch_id}),
        ),
    };
    let world_id = runtime_binding.world_id.clone();
    let branch_id = runtime_binding.branch_id.clone();
    let subject = json!({
        "kind": "agent",
        "agent_id": agent_id.clone(),
        "owner_binding": "parity-benchmark",
        "generation": 1
    });
    let presenter = json!({
        "presenter_id": "oasis7_provider_parity_bench",
        "presenter_kind": "provider",
        "session_id": session_id
    });
    let audience = json!({
        "world_id": world_id,
        "branch_id": branch_id,
        "finality_epoch": 0,
        "target_kind": "world",
        "target_id": null
    });
    let catalog: CapabilityCatalogSnapshot = serde_json::from_value(json!({
        "snapshot_id": format!("{fixture_id}-catalog-{turn}"),
        "world_id": runtime_binding.world_id.clone(),
        "world_head": logical_tick,
        "branch_id": runtime_binding.branch_id.clone(),
        "finality_epoch": runtime_binding.finality_epoch,
        "logical_tick": logical_tick,
        "module_registry_hash": h_v1("oasis7.parity.module-registry.v1", &fixture_id),
        "policy_hash": h_v1("oasis7.parity.policy.v1", &fixture_id),
        "revocation_epoch": 0,
        "subject": subject,
        "presenter": presenter,
        "audience": audience,
        "entries": [],
        "valid_until_tick": logical_tick
    }))
    .expect("parity capability catalog");
    let invocation = CapabilityInvocationContext {
        grant_id: format!("{fixture_id}-grant-{turn}"),
        subject: catalog.subject.clone(),
        presenter: catalog.presenter.clone(),
        audience: catalog.audience.clone(),
        catalog_snapshot_id: catalog.snapshot_id.clone(),
        module_id: String::new(),
        module_version: String::new(),
        response_nonce: format!("{fixture_id}-nonce-{turn}"),
    };
    base_decision_request.capability_catalog = Some(catalog.clone());
    base_decision_request.capability_invocation_context = Some(invocation.clone());
    let mut memory_snapshot = MemoryContextSnapshotV1 {
        revision: turn,
        entries: Vec::new(),
        scope: format!("agent:{agent_id}"),
        digest: String::new(),
    };
    memory_snapshot.digest = memory_snapshot.computed_digest();
    let goal_snapshot = GoalSnapshotProjector::project(
        Some(GoalSnapshotInputV1 {
            revision: turn,
            short_term_summary: "preserve deterministic forward progress".to_string(),
            long_term_summary: format!("complete parity fixture {fixture_id}"),
            blocked_reason: None,
            provenance: "harness_projection".to_string(),
        }),
        None,
    )
    .expect("parity goal snapshot");
    let continuation = retry_origin.map(|origin| {
        let mut proposal = ContinuationProposalV1 {
            schema_version: 1,
            continuation_proposal_id: format!(
                "parity-continuation:{fixture_id}:{session_id}:{retry_seq}"
            ),
            world_id: world_id.clone(),
            agent_id: agent_id.clone(),
            agent_session_id: session_id.to_string(),
            agent_turn_id: turn_id.clone(),
            decision_request_id: request_id.clone(),
            origin_turn_id: origin.agent_turn_id.clone(),
            origin_request_digest: origin.request_digest.clone(),
            action_or_plan_kind: "provider_recovery_retry".to_string(),
            action_or_envelope_digest: None,
            remaining_budget: ContinuationBudgetV1 {
                unit: "steps".to_string(),
                // Runtime charges one step while handing the leased wake off
                // to this retry. Keep one residual step so the typed Runtime
                // replan has a strictly monotonic budget.
                value: 2,
            },
            baseline_observation_digest: retry_observation_digest(observation),
            goal_digest: goal_snapshot.digest.to_string(),
            policy_digest: h_v1(
                "oasis7.parity.retry-policy.v1",
                &json!({"fixture_id": fixture_id, "recovery_chain_id": origin.recovery_chain_id}),
            )
            .to_string(),
            policy_revision: retry_seq,
            precondition_summary: "retry after the originating recoverable provider error"
                .to_string(),
            precondition_digest: h_v1(
                "oasis7.parity.retry-precondition.v1",
                &json!({
                    "origin_turn_id": origin.agent_turn_id,
                    "origin_request_digest": origin.request_digest,
                }),
            )
            .to_string(),
            wake_conditions: vec![WakeConditionV1 {
                schema_version: "wake-condition.v1".to_string(),
                kind: "at_or_after_tick".to_string(),
                logical_tick: Some(logical_tick),
                event_digest: None,
                receipt_id: None,
                subject: None,
                path_or_rule: None,
                operator: None,
                expected_value_bytes: None,
            }],
            valid_until_tick: Some(logical_tick.saturating_add(1)),
            source: "parity_benchmark_host".to_string(),
            proposal_digest: String::new(),
        };
        proposal.proposal_digest = proposal
            .proposal_digest()
            .expect("parity retry proposal digest")
            .to_string();
        proposal
    });
    let continuation_digest = continuation
        .as_ref()
        .map(|value| h_v1("oasis7.cognition.continuation-context.v1", value))
        .unwrap_or_else(|| {
            h_v1(
                "oasis7.cognition.continuation-context.v1",
                &Option::<()>::None,
            )
        });
    let mut request_context = ContinuousAgentRequestContextV1 {
        base_decision_request,
        context_discriminator: CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.to_string(),
        context_version: CONTINUOUS_AGENT_CONTEXT_VERSION,
        protocol_version: "world-simulator-provider-loopback-http-v1".to_string(),
        agent_session_id: session_id.to_string(),
        agent_turn_id: turn_id.clone(),
        decision_request_id: request_id.clone(),
        retry_seq,
        // A semantic retry is a new logical request. Its transport attempt
        // starts at one; only an in-request transport replay increments this
        // field.
        transport_attempt: 1,
        agent_subject: observation.agent_id.clone(),
        runtime_binding,
        observation_digest: Digest32::default(),
        capability_catalog_digest: h_v1(COGNITION_CAPABILITY_CATALOG_DOMAIN, &catalog),
        capability_invocation_context_digest: h_v1(
            COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
            &invocation,
        ),
        memory_snapshot_digest: Digest32::default(),
        goal_snapshot_digest: goal_snapshot.digest.clone().into(),
        continuation_digest,
        adapter_protocol_version: DEFAULT_PROTOCOL_VERSION.to_string(),
        budget_contract: BudgetContractV1 {
            max_latency_ms: 15_000,
            max_repair_attempts: 1,
            max_model_calls: 4,
            max_tool_calls: 3,
        },
        request_digest: Digest32::default(),
    };
    request_context.observation_digest = h_v1(
        "oasis7.parity.observation.v1",
        &request_context.base_decision_request.observation,
    );
    request_context.memory_snapshot_digest = memory_snapshot.digest.clone().into();
    request_context.request_digest = request_context.request_digest();
    let turn_context = ContinuousAgentTurnContextV1 {
        agent_id: observation.agent_id.clone(),
        agent_session_id: session_id.to_string(),
        agent_turn_id: turn_id,
        decision_request_id: request_id,
        request_digest: request_context.request_digest.clone(),
        memory_snapshot,
        goal_snapshot,
        continuation,
    };
    (turn_context, request_context)
}
