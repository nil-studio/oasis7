use oasis7_agent_api::*;
use serde_json::{Value, json};

pub fn production_request_fixture(attempt: u64, budget_ms: u64) -> Value {
    let mut base = serde_json::to_value(
        oasis7_runtime_host::decision_provider::golden_decision_provider_fixtures()
            .into_iter()
            .next()
            .unwrap()
            .request,
    )
    .unwrap();
    base["timeout_budget_ms"] = json!(budget_ms);
    let mut v: Value = serde_json::from_str(r#"{"context_discriminator":"oasis7.continuous-agent-context","context_version":1,"protocol_version":"continuous-agent-v1","agent_session_id":"session.agent-1.v1","agent_turn_id":"turn.agent-1.7","decision_request_id":"request.agent-1.7","retry_seq":0,"agent_subject":"agent-1","runtime_binding":{"world_id":"world-1","branch_id":"main","finality_epoch":7,"finality_block_hash":"blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","finality_status":"verified","base_tick":42,"base_world_hash":"blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","reorg_epoch":3,"runtime_manifest_hash":"blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"observation_digest":"blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","memory_snapshot_digest":"blake3:1111111111111111111111111111111111111111111111111111111111111111","goal_snapshot_digest":"blake3:2222222222222222222222222222222222222222222222222222222222222222","continuation_digest":"blake3:3333333333333333333333333333333333333333333333333333333333333333","adapter_protocol_version":"loopback-http-v1","budget_contract":{"max_latency_ms":60000,"max_repair_attempts":2,"max_model_calls":4,"max_tool_calls":3},"request_digest":"blake3:4444444444444444444444444444444444444444444444444444444444444444"}"#).unwrap();
    let cat: Value = serde_json::from_str(r#"{"snapshot_id":"catalog.agent-1.v1","world_id":"world-1","world_head":42,"branch_id":"main","finality_epoch":7,"logical_tick":42,"module_registry_hash":"blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","policy_hash":"blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","revocation_epoch":0,"subject":{"kind":"agent","agent_id":"agent-1","owner_binding":"owner-1","generation":1},"presenter":{"presenter_id":"provider-1","presenter_kind":"provider","session_id":"session.agent-1.v1"},"audience":{"world_id":"world-1","branch_id":"main","finality_epoch":7,"target_kind":"world"},"entries":[],"valid_until_tick":100}"#).unwrap();
    let inv: Value = serde_json::from_str(r#"{"grant_id":"grant.agent-1.v1","subject":{"kind":"agent","agent_id":"agent-1","owner_binding":"owner-1","generation":1},"presenter":{"presenter_id":"provider-1","presenter_kind":"provider","session_id":"session.agent-1.v1"},"audience":{"world_id":"world-1","branch_id":"main","finality_epoch":7,"target_kind":"world"},"catalog_snapshot_id":"catalog.agent-1.v1","module_id":"","module_version":"","response_nonce":"nonce.agent-1.v1"}"#).unwrap();
    (
        base["capability_catalog"],
        base["capability_invocation_context"],
    ) = (cat.clone(), inv.clone());
    (v["base_decision_request"], v["transport_attempt"]) = (base, json!(attempt));
    v["capability_catalog_digest"] = json!(h_v1(COGNITION_CAPABILITY_CATALOG_DOMAIN, &cat));
    v["capability_invocation_context_digest"] =
        json!(h_v1(COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN, &inv));
    v
}

pub fn request_from_value(mut f: Value) -> ContinuousAgentRequestContextV1 {
    let mut c = f.clone();
    let o = c.as_object_mut().unwrap();
    o.remove("request_digest");
    o.remove("transport_attempt");
    let b = o
        .get_mut("base_decision_request")
        .unwrap()
        .as_object_mut()
        .unwrap();
    b.remove("timeout_budget_ms");
    b.get_mut("observation")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("timeout_budget_ms");
    f["request_digest"] = json!(h_v1(
        "oasis7.cognition.request.v1",
        &oasis7_wasm_abi::encode_canonical_cbor(&c).unwrap()
    ));
    serde_json::from_value(f).unwrap()
}

pub fn production_turn_context(
    r: &ContinuousAgentRequestContextV1,
) -> ContinuousAgentTurnContextV1 {
    ContinuousAgentTurnContextV1 {
        agent_id: r.agent_subject.clone(),
        agent_session_id: r.agent_session_id.clone(),
        agent_turn_id: r.agent_turn_id.clone(),
        decision_request_id: r.decision_request_id.clone(),
        request_digest: r.request_digest.clone(),
        memory_snapshot: MemoryContextSnapshotV1::empty("session_private"),
        goal_snapshot: GoalSnapshotV1::empty(),
        continuation: None,
    }
}
