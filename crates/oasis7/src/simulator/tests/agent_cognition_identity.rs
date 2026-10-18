//! P0.1 RED fixtures for the continuous-agent identity and wire contract.
//!
//! These tests intentionally target the neutral cognition API described by
//! issue #3602.  The API is not present in the legacy provider adapter yet;
//! this file is the immutable behavior contract for the implementation slice.

use super::*;
use crate::runtime::{
    AgentContinuation, AgentDecisionEnvelopeV1, MvccValidator, WakeConditionV1,
    WakeConditionValidator,
};
use crate::simulator::{
    AgentCognitionStore, AsyncAgentRunner, COGNITION_CAPABILITY_CATALOG_DOMAIN,
    COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN, ContinuousAgentRequestContextV1,
    ContinuousAgentResponseContextV1, ContinuousAgentTurnContextV1, Digest32, FeedbackEnvelopeV1,
    FinalityBindingV1, GoalSnapshotV1, MemoryContextSnapshotV1, MemoryWriteIntentV1,
    RuntimeBindingV1, cognition_response_digest, h_v1,
};
use oasis7_wasm_abi::AgentCommandResponse;
use serde_json::{Value, json};

const REQUEST_DOMAIN: &str = "oasis7.cognition.request.v1";
const PROVIDER_INVOCATION_DOMAIN: &str = "oasis7.cognition.provider-invocation.v1";
const CONTEXT_DISCRIMINATOR: &str = "oasis7.continuous-agent-context";

fn inner_request() -> Value {
    serde_json::to_value(
        &golden_decision_provider_fixtures()
            .into_iter()
            .next()
            .expect("golden provider fixture")
            .request,
    )
    .expect("serialize inner decision request")
}

pub(super) fn request_fixture(transport_attempt: u64, timeout_budget_ms: u64) -> Value {
    let mut base_decision_request = inner_request();
    base_decision_request["timeout_budget_ms"] = json!(timeout_budget_ms);
    json!({
        "base_decision_request": base_decision_request,
        "context_discriminator": CONTEXT_DISCRIMINATOR,
        "context_version": 1,
        "protocol_version": "continuous-agent-v1",
        "agent_session_id": "session.agent-1.v1",
        "agent_turn_id": "turn.agent-1.7",
        "decision_request_id": "request.agent-1.7",
        "retry_seq": 0,
        "transport_attempt": transport_attempt,
        "agent_subject": "agent-1",
        "runtime_binding": {
            "world_id": "world-1",
            "branch_id": "main",
            "finality_epoch": 7,
            "finality_block_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "finality_status": "verified",
            "base_tick": 42,
            "base_world_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "reorg_epoch": 3,
            "runtime_manifest_hash": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        },
        "observation_digest": "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "capability_catalog_digest": "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "capability_invocation_context_digest": "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "memory_snapshot_digest": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        "goal_snapshot_digest": "blake3:2222222222222222222222222222222222222222222222222222222222222222",
        "continuation_digest": "blake3:3333333333333333333333333333333333333333333333333333333333333333",
        "adapter_protocol_version": "loopback-http-v1",
        "budget_contract": {"max_latency_ms": 60_000, "max_repair_attempts": 2, "max_model_calls": 4, "max_tool_calls": 3},
        "request_digest": "blake3:4444444444444444444444444444444444444444444444444444444444444444"
    })
}

pub(super) fn production_request_fixture(transport_attempt: u64, timeout_budget_ms: u64) -> Value {
    let mut fixture = request_fixture(transport_attempt, timeout_budget_ms);
    let subject = json!({
        "kind": "agent",
        "agent_id": "agent-1",
        "owner_binding": "owner-1",
        "generation": 1
    });
    let presenter = json!({
        "presenter_id": "provider-1",
        "presenter_kind": "provider",
        "session_id": "session.agent-1.v1"
    });
    let audience = json!({
        "world_id": "world-1",
        "branch_id": "main",
        "finality_epoch": 7,
        "target_kind": "world"
    });
    let catalog = json!({
        "snapshot_id": "catalog.agent-1.v1",
        "world_id": "world-1",
        "world_head": 42,
        "branch_id": "main",
        "finality_epoch": 7,
        "logical_tick": 42,
        "module_registry_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "policy_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "revocation_epoch": 0,
        "subject": subject,
        "presenter": presenter,
        "audience": audience,
        "entries": [],
        "valid_until_tick": 100
    });
    let invocation = json!({
        "grant_id": "grant.agent-1.v1",
        "subject": catalog["subject"].clone(),
        "presenter": catalog["presenter"].clone(),
        "audience": catalog["audience"].clone(),
        "catalog_snapshot_id": "catalog.agent-1.v1",
        "module_id": "",
        "module_version": "",
        "response_nonce": "nonce.agent-1.v1"
    });
    fixture["base_decision_request"]["capability_catalog"] = catalog.clone();
    fixture["base_decision_request"]["capability_invocation_context"] = invocation.clone();
    fixture["capability_catalog_digest"] =
        json!(h_v1(COGNITION_CAPABILITY_CATALOG_DOMAIN, &catalog));
    fixture["capability_invocation_context_digest"] = json!(h_v1(
        COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
        &invocation
    ));
    fixture
}

fn derived_request_digest(fixture: &Value) -> Digest32 {
    let mut canonical = fixture.clone();
    let object = canonical
        .as_object_mut()
        .expect("request fixture is an object");
    object.remove("request_digest");
    object.remove("transport_attempt");
    let base = object
        .get_mut("base_decision_request")
        .and_then(Value::as_object_mut)
        .expect("base decision request is an object");
    base.remove("timeout_budget_ms");
    base.get_mut("observation")
        .and_then(Value::as_object_mut)
        .expect("provider observation envelope is an object")
        .remove("timeout_budget_ms");
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&canonical)
        .expect("request fixture must be canonically encodable");
    h_v1(REQUEST_DOMAIN, &bytes)
}

pub(super) fn request_from_value(mut fixture: Value) -> ContinuousAgentRequestContextV1 {
    fixture["request_digest"] = json!(derived_request_digest(&fixture));
    serde_json::from_value(fixture).expect("decode ContinuousAgentRequestContextV1 fixture")
}

fn request(transport_attempt: u64, timeout_budget_ms: u64) -> ContinuousAgentRequestContextV1 {
    request_from_value(request_fixture(transport_attempt, timeout_budget_ms))
}

pub(super) fn production_turn_context(
    request_context: &ContinuousAgentRequestContextV1,
) -> ContinuousAgentTurnContextV1 {
    ContinuousAgentTurnContextV1 {
        agent_id: request_context.agent_subject.clone(),
        agent_session_id: request_context.agent_session_id.clone(),
        agent_turn_id: request_context.agent_turn_id.clone(),
        decision_request_id: request_context.decision_request_id.clone(),
        request_digest: request_context.request_digest.clone(),
        memory_snapshot: MemoryContextSnapshotV1::empty("session_private"),
        goal_snapshot: GoalSnapshotV1::empty(),
        continuation: None,
    }
}

pub(super) fn production_observation(agent_id: &str, time: u64) -> Observation {
    Observation {
        time,
        agent_id: agent_id.to_string(),
        pos: crate::geometry::GeoPos::new(0, 0, 0),
        self_resources: Default::default(),
        visibility_range_cm: 0,
        visible_agents: Vec::new(),
        visible_locations: Vec::new(),
        module_lifecycle: Default::default(),
        module_market: Default::default(),
        power_market: Default::default(),
        social_state: Default::default(),
    }
}

fn runtime_condition(value: Value) -> WakeConditionV1 {
    serde_json::from_value(value).expect("decode runtime WakeConditionV1 fixture")
}

fn runtime_continuation() -> AgentContinuation {
    serde_json::from_value(json!({
        "schema_version": "agent-continuation.v1",
        "continuation_id": "continuation-identity-1",
        "wake_id": "wake-identity-1",
        "world_id": "world-1",
        "branch_id": "main",
        "finality_epoch": 7,
        "finality_block_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "finality_status": "verified",
        "reorg_epoch": 3,
        "runtime_manifest_hash": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "agent_id": "agent-1",
        "agent_session_id": "session.agent-1.v1",
        "agent_turn_id": "turn.agent-1.7",
        "decision_request_id": "request.agent-1.7",
        "origin_turn_id": "turn.agent-1.6",
        "origin_request_digest": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        "continuation_proposal_id": "proposal-identity-1",
        "proposal_digest": "blake3:2222222222222222222222222222222222222222222222222222222222222222",
        "action_or_envelope_digest": null,
        "wake_conditions": [{
            "schema_version": "wake-condition.v1",
            "kind": "at_or_after_tick",
            "logical_tick": 42
        }],
        "next_wake_tick": 42,
        "remaining_budget": {"unit": "steps", "value": 2},
        "valid_until_tick": 100,
        "precondition_digest": "blake3:3333333333333333333333333333333333333333333333333333333333333333",
        "wake_seq": 1,
        "status": "pending",
        "terminal_disposition": null
    }))
    .expect("decode runtime continuation fixture")
}

fn runtime_envelope_with_finality(
    finality_status: &str,
    finality_block_hash: &str,
) -> AgentDecisionEnvelopeV1 {
    serde_json::from_value(json!({
        "schema_version": "agent-decision-envelope.v1",
        "world_id": "world-identity-finality",
        "agent_id": "agent-identity-finality",
        "branch_id": "main",
        "finality_epoch": 0,
        "finality_block_hash": finality_block_hash,
        "finality_status": finality_status,
        "agent_session_id": "session.identity-finality",
        "agent_turn_id": "turn.identity-finality",
        "decision_request_id": "request.identity-finality",
        "retry_seq": 0,
        "base_tick": 0,
        "base_world_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "reorg_epoch": 0,
        "runtime_manifest_hash": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "capability_snapshot_hash": "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "authority_context_hash": "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "observation_digest": "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "context_digest": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        "issued_at_tick": 0,
        "valid_until_tick": 10,
        "preconditions": [],
        "decision_kind": "wait",
        "action": {"type": "wait"},
        "request_digest": "blake3:2222222222222222222222222222222222222222222222222222222222222222",
        "decision_digest": "blake3:3333333333333333333333333333333333333333333333333333333333333333",
        "envelope_digest": "blake3:4444444444444444444444444444444444444444444444444444444444444444",
        "provider_invocation_key": "blake3:5555555555555555555555555555555555555555555555555555555555555555",
        "envelope_idempotency_key": "blake3:6666666666666666666666666666666666666666666666666666666666666666",
        "origin_intent_ref": null,
        "source": "provider"
    }))
    .expect("decode runtime finality envelope fixture")
}

fn response_fixture(decision: Value) -> Value {
    json!({
        "base_decision_response": {
            "decision": decision,
            "provider_error": null,
            "diagnostics": {},
            "trace_payload": {},
            "memory_write_intents": []
        },
        "context_discriminator": CONTEXT_DISCRIMINATOR,
        "context_version": 1,
        "agent_session_id": "session.agent-1.v1",
        "agent_turn_id": "turn.agent-1.7",
        "decision_request_id": "request.agent-1.7",
        "retry_seq": 0,
        "transport_attempt": 0,
        "request_digest": "blake3:4444444444444444444444444444444444444444444444444444444444444444"
    })
}

#[test]
fn request_digest_golden_excludes_request_digest_and_transport_attempt() {
    let first = request(0, 60_000);
    let retried = request(1, 60_000);

    assert_eq!(
        first
            .canonical_request_bytes()
            .expect("canonical request bytes"),
        retried
            .canonical_request_bytes()
            .expect("canonical retry bytes")
    );
    assert_eq!(
        first.request_digest().expect("request digest"),
        retried.request_digest()
    );
    assert_eq!(
        first
            .provider_invocation_key()
            .expect("provider invocation key"),
        retried.provider_invocation_key()
    );
    assert_eq!(
        first
            .provider_invocation_key()
            .expect("provider invocation key"),
        h_v1(
            PROVIDER_INVOCATION_DOMAIN,
            &first.request_digest().expect("request digest")
        )
    );
    assert_eq!(
        h_v1(
            REQUEST_DOMAIN,
            &first.canonical_request_bytes().expect("canonical bytes")
        ),
        first.request_digest().expect("request digest")
    );
}

#[test]
fn provider_key_is_invariant_to_timeout_and_transport_attempt() {
    let baseline = request(0, 100);
    let timeout_changed = request(0, 5_000);
    let transport_retry = request(9, 100);

    assert_eq!(
        baseline.provider_invocation_key().expect("provider key"),
        timeout_changed
            .provider_invocation_key()
            .expect("provider key")
    );
    assert_eq!(
        baseline.provider_invocation_key().expect("provider key"),
        transport_retry
            .provider_invocation_key()
            .expect("provider key")
    );
}

#[test]
fn provider_key_is_invariant_to_nested_observation_timeout_and_transport_attempt() {
    let baseline = request(0, 100);
    let mut nested_timeout = request_fixture(0, 100);
    nested_timeout["base_decision_request"]["observation"]["timeout_budget_ms"] = json!(5_000);
    let nested_timeout = request_from_value(nested_timeout);

    assert!(
        baseline
            .canonical_request_bytes()
            .expect("canonical baseline bytes")
            == nested_timeout
                .canonical_request_bytes()
                .expect("canonical retry bytes"),
        "nested provider observation timeout must be excluded from canonical bytes"
    );
    assert_eq!(
        baseline.request_digest(),
        nested_timeout.request_digest(),
        "nested provider observation timeout is transport metadata"
    );
    assert_eq!(
        baseline.provider_invocation_key(),
        nested_timeout.provider_invocation_key(),
        "nested provider observation timeout must not split provider dedupe"
    );
}

#[test]
fn runtime_wake_condition_identity_is_shared_and_order_insensitive() {
    let tick = runtime_condition(json!({
        "schema_version": "wake-condition.v1",
        "kind": "at_or_after_tick",
        "logical_tick": 42
    }));
    let receipt = runtime_condition(json!({
        "schema_version": "wake-condition.v1",
        "kind": "receipt_linked",
        "receipt_id": "blake3:9999999999999999999999999999999999999999999999999999999999999999"
    }));
    let canonical = WakeConditionValidator::canonicalize(vec![tick, receipt])
        .expect("canonicalize runtime wake conditions");
    let mut reversed = canonical.clone();
    reversed.reverse();

    let digest = WakeConditionValidator::conditions_digest(&canonical);
    assert!(
        digest.starts_with("blake3:"),
        "wake condition identity must use the shared BLAKE3 rendering"
    );
    assert_eq!(
        digest,
        WakeConditionValidator::conditions_digest(&reversed),
        "wake condition identity must hash the canonical sorted list"
    );
}

#[test]
fn runtime_continuation_status_identity_covers_binding_fields_and_uses_blake3() {
    let continuation = runtime_continuation();
    let digest = continuation.status_digest();
    assert!(
        digest.starts_with("blake3:"),
        "continuation status identity must use the shared BLAKE3 rendering"
    );

    let mut finality_changed = continuation.clone();
    finality_changed.finality_epoch += 1;
    assert_ne!(
        digest,
        finality_changed.status_digest(),
        "status identity must bind the Runtime finality tuple"
    );

    let mut proposal_changed = continuation;
    proposal_changed.proposal_digest =
        "blake3:9999999999999999999999999999999999999999999999999999999999999999".to_string();
    assert_ne!(
        digest,
        proposal_changed.status_digest(),
        "status identity must bind the continuation proposal digest"
    );
}

#[test]
fn outer_request_roundtrips_and_unknown_version_fails_closed() {
    let fixture = request_fixture(0, 60_000);
    let context: ContinuousAgentRequestContextV1 =
        serde_json::from_value(fixture.clone()).expect("decode outer request");
    let encoded = serde_json::to_value(&context).expect("encode outer request");
    let decoded: ContinuousAgentRequestContextV1 =
        serde_json::from_value(encoded).expect("round-trip outer request");
    assert_eq!(context, decoded);

    let mut unknown_version = fixture;
    unknown_version["context_version"] = json!(99);
    let error = request_from_value(unknown_version)
        .validate()
        .expect_err("unknown context version must fail closed");
    assert_eq!(error.code(), "unsupported_context_version");
}

#[test]
fn outer_request_rejects_unknown_authority_fields_during_decode() {
    let mut fixture = request_fixture(0, 60_000);
    fixture["unknown_authority_field"] = json!("must-fail-closed");
    let error = serde_json::from_value::<ContinuousAgentRequestContextV1>(fixture)
        .expect_err("unknown outer request authority fields must not be discarded");
    assert!(error.to_string().contains("unknown field"), "{error}");
}

#[test]
fn declared_request_digest_mismatch_fails_closed_at_harness_ingress() {
    let mut context = request_from_value(request_fixture(0, 60_000));
    context.request_digest =
        Digest32::from("blake3:9999999999999999999999999999999999999999999999999999999999999999");
    let mut store = AgentCognitionStore::default();
    let error = store
        .begin_request(context)
        .expect_err("declared digest mismatch must fail closed before acceptance");
    assert_eq!(error.code(), "request_digest_mismatch");
}

#[test]
fn harness_finality_binding_rejects_unknown_status_and_missing_verified_hash() {
    let mut unknown_status = request_fixture(0, 60_000);
    unknown_status["runtime_binding"]["finality_status"] = json!("unknown");
    let unknown_status = request_from_value(unknown_status);
    assert_eq!(
        unknown_status
            .validate()
            .expect_err("unknown finality status must fail closed")
            .code(),
        "recovery_pending"
    );

    let mut missing_verified_hash = request_fixture(0, 60_000);
    missing_verified_hash["runtime_binding"]["finality_block_hash"] = Value::Null;
    let missing_verified_hash = request_from_value(missing_verified_hash);
    assert_eq!(
        missing_verified_hash
            .validate()
            .expect_err("verified finality requires a block hash")
            .code(),
        "recovery_pending"
    );

    let mut pending_without_hash = request_fixture(0, 60_000);
    pending_without_hash["runtime_binding"]["finality_status"] = json!("pending");
    pending_without_hash["runtime_binding"]["finality_block_hash"] = Value::Null;
    let pending_without_hash = request_from_value(pending_without_hash);
    pending_without_hash
        .validate()
        .expect("pending finality may omit its block hash");
}

#[test]
fn runtime_finality_digest_type_rejects_unprefixed_block_hash() {
    let candidate = runtime_envelope_with_finality("verified", &"a".repeat(64));
    let error = MvccValidator::validate(&crate::runtime_host::world::new(), &candidate)
        .expect_err("runtime must reject an untyped finality block hash");
    assert_eq!(error.code(), "recovery_pending");
}

#[test]
fn outer_response_preserves_all_six_tagged_provider_decisions() {
    let fixture = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("golden provider fixture");
    let query = ProviderDecision::Query {
        query_ref: "query-1".to_string(),
        query: AgentQuery::QuoteMicroDepotInstall(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-2".to_string(),
        }),
    };
    let module_command = ProviderDecision::ModuleCommand {
        module_command: ProviderModuleCommand {
            module_id: "m.weather".to_string(),
            module_version: "0.1.0".to_string(),
            namespace: "weather".to_string(),
            name: "observe".to_string(),
            schema_version: 1,
            schema_hash: "blake3:module".to_string(),
            payload: vec![1, 2, 3],
        },
    };
    let command_response: AgentCommandResponse = serde_json::from_value(json!({
        "response_nonce": "nonce-1",
        "subject": {"kind": "agent", "agent_id": "agent-1", "owner_binding": "owner-1", "generation": 1},
        "presenter": {"presenter_id": "provider-1", "presenter_kind": "provider"},
        "audience": {
            "world_id": "world-1", "branch_id": "main", "finality_epoch": 1,
            "target_kind": "module_instance", "target_id": "instance-1"
        },
        "catalog_snapshot_id": "catalog-1",
        "selected_entry": {
            "module_id": "m.weather", "module_version": "0.1.0", "namespace": "weather",
            "command": "observe", "schema_version": 1, "schema_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "max_payload_bytes": 128
        },
        "envelope": {
            "namespace": "weather", "name": "observe", "schema_version": 1,
            "schema_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "payload": [1, 2, 3]
        },
        "provider_id": "provider-1", "trace_id": "trace-1"
    }))
    .expect("decode typed module response");
    let decisions: Vec<Value> = vec![
        serde_json::to_value(ProviderDecision::Wait).expect("encode wait decision"),
        serde_json::to_value(ProviderDecision::WaitTicks { ticks: 3 })
            .expect("encode wait-ticks decision"),
        serde_json::to_value(fixture.expected_decision).expect("encode act decision"),
        serde_json::to_value(query).expect("encode query decision"),
        serde_json::to_value(module_command).expect("encode module command decision"),
        serde_json::to_value(ProviderDecision::ModuleCommandResponse {
            response: command_response,
        })
        .expect("encode module response decision"),
    ];

    for decision in decisions {
        let response: ContinuousAgentResponseContextV1 =
            serde_json::from_value(response_fixture(decision.clone()))
                .expect("decode outer response");
        let encoded = serde_json::to_value(response).expect("encode outer response");
        assert_eq!(encoded["base_decision_response"]["decision"], decision);
    }
}

#[test]
fn outer_response_and_feedback_reject_unknown_authority_fields_during_decode() {
    let mut response = response_fixture(
        serde_json::to_value(ProviderDecision::Wait).expect("encode wait decision"),
    );
    response["unknown_authority_field"] = json!(true);
    assert_eq!(
        ContinuousAgentResponseContextV1::validate_value(&response)
            .expect_err("unknown response field must use the stable contract error")
            .code(),
        "unknown_context_field"
    );

    let feedback = json!({
        "feedback_id": "feedback-unknown-field",
        "feedback_seq": 1,
        "agent_subject": "agent-1",
        "agent_session_id": "session-1",
        "agent_turn_id": "turn-1",
        "decision_request_id": "request-1",
        "status": "pending",
        "request_digest": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "provenance": "runtime_authoritative",
        "unknown_authority_field": "must-fail-closed"
    });
    assert_eq!(
        FeedbackEnvelopeV1::validate_value(&feedback)
            .expect_err("unknown outer feedback authority fields must not be discarded")
            .code(),
        "unknown_context_field"
    );

    let mut request = request_fixture(0, 60_000);
    request["unknown_authority_field"] = json!(true);
    let request_error = ContinuousAgentRequestContextV1::validate_value(&request)
        .expect_err("unknown request field must use the stable contract error");
    assert_eq!(request_error.code(), "unknown_context_field");
}

#[test]
fn same_key_with_different_digest_is_a_fail_closed_collision() {
    let mut store = AgentCognitionStore::default();
    let first = request(0, 60_000);
    let mut collision = request_fixture(0, 60_000);
    collision["observation_digest"] =
        json!("blake3:9999999999999999999999999999999999999999999999999999999999999999");
    let second = request_from_value(collision);

    store.begin_request(first).expect("first request accepted");
    let error = store
        .begin_request(second)
        .expect_err("same key/different digest must fail closed");
    assert_eq!(error.code(), "request_identity_collision");
}

#[test]
fn one_agent_cannot_hold_two_sessions_but_two_agents_are_partitioned() {
    let mut store = AgentCognitionStore::default();
    store
        .begin_request(request(0, 60_000))
        .expect("first session accepted");

    let mut second_session = request_fixture(0, 60_000);
    second_session["agent_session_id"] = json!("session.agent-1.v2");
    second_session["agent_turn_id"] = json!("turn.agent-1.8");
    second_session["decision_request_id"] = json!("request.agent-1.8");
    let second_session = request_from_value(second_session);
    assert_eq!(
        store
            .begin_request(second_session)
            .expect_err("global per-agent single-flight")
            .code(),
        "agent_busy"
    );

    let mut other_agent = request_fixture(0, 60_000);
    other_agent["agent_subject"] = json!("agent-2");
    other_agent["base_decision_request"]["observation"]["agent_id"] = json!("agent-2");
    other_agent["agent_session_id"] = json!("session.agent-2.v1");
    other_agent["agent_turn_id"] = json!("turn.agent-2.1");
    other_agent["decision_request_id"] = json!("request.agent-2.1");
    store
        .begin_request(request_from_value(other_agent))
        .expect("different agents remain independent");
}

#[test]
fn feedback_partition_rejects_cross_agent_and_preserves_runtime_lineage() {
    let mut store = AgentCognitionStore::default();
    store
        .begin_request(request(0, 60_000))
        .expect("request accepted");
    let feedback: FeedbackEnvelopeV1 = serde_json::from_value(json!({
        "feedback_id": "feedback-1", "feedback_seq": 1,
        "agent_subject": "agent-2", "agent_session_id": "session.agent-2.v1",
        "agent_turn_id": "turn.agent-2.1", "decision_request_id": "request.agent-2.1",
        "candidate_action_id": null, "runtime_receipt_id": null,
        "status": "rejected", "request_digest": "blake3:cross-agent",
        "reject_reason": "no_effect", "provenance": "runtime_authoritative"
    }))
    .expect("decode feedback fixture");

    let diagnostic = store
        .accept_feedback(feedback)
        .expect_err("cross-agent feedback");
    assert_eq!(diagnostic.code(), "cross_agent_feedback");
}

#[test]
fn duplicate_terminal_feedback_id_is_idempotent_after_first_acceptance() {
    let mut store = AgentCognitionStore::default();
    let request_context = request(0, 60_000);
    let request_digest = request_context.request_digest().to_string();
    store
        .begin_request(request_context)
        .expect("request accepted");

    let feedback: FeedbackEnvelopeV1 = serde_json::from_value(json!({
        "feedback_id": "feedback-idempotent-1", "feedback_seq": 1,
        "agent_subject": "agent-1", "agent_session_id": "session.agent-1.v1",
        "agent_turn_id": "turn.agent-1.7", "decision_request_id": "request.agent-1.7",
        "candidate_action_id": 7, "runtime_receipt_id": null,
        "status": "rejected", "request_digest": request_digest,
        "reject_reason": "no_effect", "provenance": "runtime_authoritative"
    }))
    .expect("decode feedback fixture");

    store
        .accept_feedback(feedback.clone())
        .expect("first terminal feedback accepted");
    store
        .accept_feedback(feedback)
        .expect("duplicate feedback_id must be an idempotent no-op");
}

#[test]
fn feedback_requires_runtime_provenance_canonical_status_and_nonzero_sequence() {
    let mut store = AgentCognitionStore::default();
    let request_context = request(0, 60_000);
    let request_digest = request_context.request_digest().to_string();
    store
        .begin_request(request_context)
        .expect("request accepted");

    let mut feedback: Value = json!({
        "feedback_id": "feedback-invalid-contract",
        "feedback_seq": 0,
        "agent_subject": "agent-1",
        "agent_session_id": "session.agent-1.v1",
        "agent_turn_id": "turn.agent-1.7",
        "decision_request_id": "request.agent-1.7",
        "candidate_action_id": null,
        "runtime_receipt_id": null,
        "status": "bogus",
        "request_digest": request_digest,
        "reject_reason": null,
        "provenance": "provider_unverified"
    });
    let error = store
        .accept_feedback(serde_json::from_value(feedback.clone()).expect("decode feedback"))
        .expect_err("untrusted status/provenance/sequence must fail closed");
    assert_eq!(error.code(), "feedback_contract_invalid");

    feedback["feedback_seq"] = json!(1);
    feedback["status"] = json!("rejected");
    feedback["provenance"] = json!("runtime_authoritative");
    store
        .accept_feedback(serde_json::from_value(feedback).expect("decode valid feedback"))
        .expect("canonical Runtime feedback accepted");
}

#[test]
fn feedback_sequence_gap_and_same_sequence_collision_fail_closed() {
    let mut store = AgentCognitionStore::default();
    let request_context = request(0, 60_000);
    let request_digest = request_context.request_digest().to_string();
    store
        .begin_request(request_context)
        .expect("request accepted");

    let feedback = |id: &str, seq: u64, reason: &str| {
        serde_json::from_value(json!({
            "feedback_id": id,
            "feedback_seq": seq,
            "agent_subject": "agent-1",
            "agent_session_id": "session.agent-1.v1",
            "agent_turn_id": "turn.agent-1.7",
            "decision_request_id": "request.agent-1.7",
            "candidate_action_id": 7,
            "runtime_receipt_id": null,
            "status": "pending",
            "request_digest": request_digest,
            "reject_reason": reason,
            "provenance": "runtime_authoritative"
        }))
        .expect("decode feedback")
    };

    let gap = store
        .accept_feedback(feedback("feedback-seq-3", 3, "gap"))
        .expect_err("a sequence gap must be held/reported");
    assert_eq!(gap.code(), "feedback_sequence_gap");

    let held_collision = store
        .accept_feedback(feedback("feedback-seq-3b", 3, "different-held"))
        .expect_err("a held sequence reused with a different envelope must collide");
    assert_eq!(held_collision.code(), "feedback_identity_collision");

    store
        .accept_feedback(feedback("feedback-seq-1", 1, "first"))
        .expect("first sequence accepted");
    let collision = store
        .accept_feedback(feedback("feedback-seq-1b", 1, "different"))
        .expect_err("same sequence with a different digest must collide");
    assert_eq!(collision.code(), "feedback_identity_collision");
}

#[test]
fn held_feedback_id_collision_across_future_sequences_does_not_corrupt_replay_state() {
    let mut store = AgentCognitionStore::default();
    let request_context = request(0, 60_000);
    let request_digest = request_context.request_digest().to_string();
    store
        .begin_request(request_context)
        .expect("request accepted");

    let feedback = |id: &str, seq: u64, reason: &str| {
        serde_json::from_value(json!({
            "feedback_id": id,
            "feedback_seq": seq,
            "agent_subject": "agent-1",
            "agent_session_id": "session.agent-1.v1",
            "agent_turn_id": "turn.agent-1.7",
            "decision_request_id": "request.agent-1.7",
            "candidate_action_id": 7,
            "runtime_receipt_id": null,
            "status": "pending",
            "request_digest": request_digest,
            "reject_reason": reason,
            "provenance": "runtime_authoritative"
        }))
        .expect("decode feedback")
    };

    let first_gap = store
        .accept_feedback(feedback("feedback-held-id", 4, "first"))
        .expect_err("the first future envelope must be held");
    assert_eq!(first_gap.code(), "feedback_sequence_gap");

    let exact_hold_replay = store
        .accept_feedback(feedback("feedback-held-id", 4, "first"))
        .expect_err("an exact held replay must remain pending behind the gap");
    assert_eq!(exact_hold_replay.code(), "feedback_sequence_gap");

    let duplicate_id = store
        .accept_feedback(feedback("feedback-held-id", 5, "same ID, new sequence"))
        .expect_err("a held feedback_id reused at another sequence must collide");
    assert_eq!(duplicate_id.code(), "feedback_id_conflict");

    for sequence in 1..=3 {
        store
            .accept_feedback(feedback(
                &format!("feedback-seq-{sequence}"),
                sequence,
                "drain",
            ))
            .expect("contiguous feedback must advance the hold window");
    }

    store
        .accept_feedback(feedback("feedback-seq-5", 5, "after collision"))
        .expect("rejected duplicate must not occupy the later sequence slot");
}

#[test]
fn production_outer_context_preserves_retry_transport_and_runtime_binding() {
    let mut fixture = production_request_fixture(2, 60_000);
    fixture["retry_seq"] = json!(2);
    let request_context = request_from_value(fixture);
    let turn_context = ContinuousAgentTurnContextV1 {
        agent_id: request_context.agent_subject.clone(),
        agent_session_id: request_context.agent_session_id.clone(),
        agent_turn_id: request_context.agent_turn_id.clone(),
        decision_request_id: request_context.decision_request_id.clone(),
        request_digest: request_context.request_digest.clone(),
        memory_snapshot: MemoryContextSnapshotV1::empty("session_private"),
        goal_snapshot: GoalSnapshotV1::empty(),
        continuation: None,
    };
    let mut runner = provider_backed_runner_with_empty_runtime_history("agent-1");
    let turn_id = runner
        .start_turn_with_request_context("agent-1", turn_context, request_context.clone())
        .expect("production outer request accepted");
    let outcome = loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll provider completion")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            break outcome;
        }
        std::thread::yield_now();
    };
    let prepared = outcome
        .prepared_request_context
        .expect("outer context retained through completion");
    assert_eq!(prepared.retry_seq, 2);
    assert_eq!(prepared.transport_attempt, 2);
    assert_eq!(prepared.runtime_binding, request_context.runtime_binding);
    let response = outcome
        .prepared_response_context
        .expect("response identity retained for replay/artifact binding");
    assert_eq!(response.retry_seq, prepared.retry_seq);
    assert_eq!(response.transport_attempt, prepared.transport_attempt);
    assert_eq!(
        response.response_digest,
        cognition_response_digest(&response.base_decision_response)
    );
    let artifact_identity = response.response_artifact_identity();
    response
        .validate_response_artifact_identity(&artifact_identity)
        .expect("response artifact identity binds complete lineage");
    let identity_payload = response
        .response_artifact_identity_payload()
        .expect("encode response artifact identity payload");
    assert_eq!(
        identity_payload["artifact_digest"],
        artifact_identity.artifact_digest.as_str()
    );
}

#[test]
fn retry_awaiting_turn_preserves_identity_and_increments_only_transport_attempt() {
    let request_context = request_from_value({
        let mut fixture = production_request_fixture(1, 60_000);
        fixture["retry_seq"] = json!(1);
        fixture
    });
    let turn_context = production_turn_context(&request_context);
    let mut runner = provider_backed_runner_with_empty_runtime_history("agent-1");
    let turn_id = runner
        .start_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 42),
            turn_context.clone(),
            request_context.clone(),
        )
        .expect("open production provider turn");
    let first = loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll production turn")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            break outcome;
        }
        std::thread::yield_now();
    };
    assert_eq!(
        first
            .prepared_request_context
            .as_ref()
            .map(|context| context.transport_attempt),
        Some(1)
    );

    let retried_turn_id = runner
        .retry_awaiting_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 43),
            turn_context.clone(),
            request_context.clone(),
        )
        .expect("awaiting production turn can retry without re-admission");
    assert_eq!(retried_turn_id, turn_id);
    assert!(runner.provider_is_still_in_flight("agent-1"));
    assert_eq!(runner.active_turn_count(), 1);
    let reentry = runner
        .start_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 43),
            turn_context,
            request_context.clone(),
        )
        .expect_err("retry must preserve the awaiting single-flight guard");
    assert_eq!(reentry.code(), "agent_busy");
    let retry_outcome = loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll retried production turn")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            break outcome;
        }
        std::thread::yield_now();
    };
    let retried_context = retry_outcome
        .prepared_request_context
        .expect("retry retains outer request context");
    assert_eq!(retried_context.transport_attempt, 2);
    assert_eq!(retried_context.retry_seq, 1);
    assert_eq!(
        retried_context.request_digest,
        request_context.request_digest
    );
}

#[test]
fn retry_awaiting_turn_rejects_mismatched_identity_and_digest() {
    let request_context = request_from_value({
        let mut fixture = production_request_fixture(1, 60_000);
        fixture["retry_seq"] = json!(1);
        fixture
    });
    let turn_context = production_turn_context(&request_context);
    let mut runner = provider_backed_runner_with_empty_runtime_history("agent-1");
    let turn_id = runner
        .start_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 42),
            turn_context.clone(),
            request_context.clone(),
        )
        .expect("open production provider turn");
    let _ = loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll production turn")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            break outcome;
        }
        std::thread::yield_now();
    };

    let mut mismatched_context = turn_context.clone();
    mismatched_context.agent_turn_id = "turn.other".to_string();
    let identity_error = runner
        .retry_awaiting_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 43),
            mismatched_context,
            request_context.clone(),
        )
        .expect_err("retry must reject a different turn identity");
    assert!(identity_error.to_string().contains("retry"));

    let mut mismatched_request = request_context.clone();
    mismatched_request.request_digest = Digest32::from(format!("blake3:{}", "9".repeat(64)));
    let digest_error = runner
        .retry_awaiting_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 43),
            turn_context,
            mismatched_request,
        )
        .expect_err("retry must reject a different request digest");
    assert!(digest_error.to_string().contains("request_digest"));
    assert!(!runner.provider_is_still_in_flight("agent-1"));
}

#[test]
fn retry_awaiting_turn_rejects_after_terminal_feedback() {
    let request_context = request_from_value({
        let mut fixture = production_request_fixture(1, 60_000);
        fixture["retry_seq"] = json!(1);
        fixture
    });
    let turn_context = production_turn_context(&request_context);
    let mut runner = provider_backed_runner_with_empty_runtime_history("agent-1");
    let turn_id = runner
        .start_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 42),
            turn_context.clone(),
            request_context.clone(),
        )
        .expect("open production provider turn");
    let outcome = loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll production turn")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            break outcome;
        }
        std::thread::yield_now();
    };
    let mut terminal = outcome
        .feedback_for_runtime_status("failed", None)
        .expect("build terminal feedback");
    terminal.provenance = "runtime_authoritative".to_string();
    runner
        .consume_runtime_feedback(
            "agent-1",
            terminal,
            &mut crate::simulator::MemoryWriteStore::default(),
        )
        .expect("Runtime terminal feedback releases the turn");

    let error = runner
        .retry_awaiting_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 43),
            turn_context,
            request_context,
        )
        .expect_err("terminal feedback must make retry unavailable");
    assert_eq!(error.code(), "cognition_error");
    assert!(error.to_string().contains("awaiting"));
}

#[test]
fn production_context_rejects_null_capability_discovery_context() {
    let mut fixture = request_fixture(1, 60_000);
    fixture["retry_seq"] = json!(1);
    let request_context = request_from_value(fixture);
    let error = request_context
        .validate_production_lane()
        .expect_err("production context must carry Runtime capability discovery");
    assert_eq!(error.code(), "missing_capability_context");
}

#[test]
fn production_context_rejects_capability_for_a_different_agent() {
    let mut fixture = production_request_fixture(1, 60_000);
    fixture["retry_seq"] = json!(1);
    fixture["base_decision_request"]["capability_catalog"]["subject"]["agent_id"] =
        json!("agent-2");
    fixture["base_decision_request"]["capability_invocation_context"]["subject"]["agent_id"] =
        json!("agent-2");
    let catalog = fixture["base_decision_request"]["capability_catalog"].clone();
    let invocation = fixture["base_decision_request"]["capability_invocation_context"].clone();
    fixture["capability_catalog_digest"] =
        json!(h_v1(COGNITION_CAPABILITY_CATALOG_DOMAIN, &catalog));
    fixture["capability_invocation_context_digest"] = json!(h_v1(
        COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
        &invocation
    ));
    let request_context = request_from_value(fixture);
    let error = request_context
        .validate_production_lane()
        .expect_err("production context must bind capabilities to the requesting agent");
    assert_eq!(error.code(), "capability_subject_mismatch");
}

#[test]
fn production_provider_behavior_fences_contextless_legacy_entry() {
    let behavior = ProviderBackedAgentBehavior::new(
        "agent-1",
        MockDecisionProvider::new("fenced-provider"),
        vec![ActionCatalogEntry::new("wait", "wait")],
    );
    let mut runner = AsyncAgentRunner::new(1).expect("create runner");
    runner.register(behavior).expect("register fenced provider");
    let outcome = runner
        .run_one_turn()
        .expect("fenced turn completes diagnostically");
    assert_eq!(outcome.lifecycle, AsyncTurnLifecycle::Failed);
    assert_eq!(
        outcome.feedback,
        AsyncTurnFeedback::ProviderError {
            code: "continuous_context_required".to_string()
        }
    );
}

#[test]
fn legacy_provider_lane_requires_explicit_compatibility_selection() {
    let behavior = ProviderBackedAgentBehavior::new_legacy_compatibility(
        "agent-1",
        MockDecisionProvider::new("legacy-provider"),
        vec![ActionCatalogEntry::new("wait", "wait")],
    );
    let mut runner = AsyncAgentRunner::new(1).expect("create runner");
    runner.register(behavior).expect("register legacy provider");
    let outcome = runner
        .run_one_turn()
        .expect("legacy compatibility turn completes");
    assert_eq!(outcome.lifecycle, AsyncTurnLifecycle::Completed);
}

#[test]
fn legacy_feedback_without_committed_receipt_is_ambiguous() {
    let legacy = json!({
        "action_id": 7,
        "success": true,
        "reject_reason": null,
        "emitted_events": [],
        "world_delta_summary": "legacy response"
    });
    let error = FeedbackEnvelopeV1::from_legacy_value(legacy)
        .expect_err("legacy success without committed receipt is ambiguous");
    assert_eq!(error.code(), "legacy_feedback_ambiguous");
}

#[test]
fn cognition_wire_exports_the_shared_v1_types() {
    fn assert_wire_types<
        D: serde::Serialize + serde::de::DeserializeOwned,
        F: serde::Serialize + serde::de::DeserializeOwned,
        M: serde::Serialize + serde::de::DeserializeOwned,
        R: serde::Serialize + serde::de::DeserializeOwned,
    >() {
    }

    assert_wire_types::<Digest32, FinalityBindingV1, MemoryWriteIntentV1, RuntimeBindingV1>();
}
