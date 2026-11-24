//! Exact shared signed-cognition fixtures from the original umbrella identity tests.
use super::*;
use serde_json::{Value, json};

const REQUEST_DOMAIN: &str = "oasis7.cognition.request.v1";
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
        pos: oasis7_simulator::GeoPos::new(0, 0, 0),
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
