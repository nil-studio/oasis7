use super::*;
use serde_json::json;
fn work() -> RuleDecisionPayloadDecodeWorkV1 {
    RuleDecisionPayloadDecodeWorkV1 {
        structure: CborPreflightWorkV1 {
            bytes_remaining: 1 << 20,
            values_remaining: 100000,
            max_depth: 64,
        },
        max_payload_bytes: 1 << 16,
        max_heap_bytes: 64 << 20,
    }
}
#[test]
fn rule_payload_matches_original_json_projection_and_retains_fee() {
    let memory = MemoryBudget::new(128 << 20, 1 << 20).unwrap();
    let move_action = crate::Action::MoveAgent {
        agent_id: "a".into(),
        to: oasis7_agent_api::world_action::GeoPos {
            x_cm: 1,
            y_cm: 2,
            z_cm: 3,
        },
    };
    let query_action = crate::Action::QueryObservation {
        agent_id: "a".into(),
    };
    let mut cases = vec![
        json!({"action_id":1,"verdict":"allow"}),
        json!({"action_id":2,"verdict":"deny","notes":["reason"],"cost":{"entries":{}}}),
        json!({"action_id":3,"verdict":"modify","override_action":move_action,"extra":{"unknown":[1,2,3]}}),
        json!({"action_id":4,"verdict":"modify","override_action":query_action}),
        json!({"action_id":null,"verdict":"allow"}),
        json!({"action_id":"wrong","verdict":"deny"}),
        json!({"action_id":5,"verdict":"allow","notes":null}),
        json!({"action_id":6,"verdict":"deny","override_action":null}),
    ];
    for value in cases.drain(..) {
        let expected = serde_json::from_value::<RuleDecision>(value.clone());
        let raw = serde_cbor::to_vec(&value).unwrap();
        let actual = decode_rule_decision_payload_v1(
            &raw,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false,
        );
        assert_eq!(actual.is_ok(), expected.is_ok(), "{value}");
        if let (Ok(actual), Ok(expected)) = (actual, expected) {
            assert_eq!(*actual, expected);
            assert!(actual.uses_budget(&memory));
            assert!(actual.reserved_bytes() >= std::mem::size_of::<RuleDecision>());
        }
    }
}
#[test]
fn rule_payload_refuses_heap_work_class_and_cancellation() {
    let memory = MemoryBudget::new(128 << 20, 1 << 20).unwrap();
    let raw = serde_cbor::to_vec(&json!({"action_id":1,"verdict":"allow"})).unwrap();
    let mut low = work();
    low.max_heap_bytes = 1;
    assert!(matches!(
        decode_rule_decision_payload_v1(&raw, &memory, MemoryClass::Ordinary, &mut low, || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    let mut low = work();
    low.structure.values_remaining = 0;
    assert!(matches!(
        decode_rule_decision_payload_v1(&raw, &memory, MemoryClass::Ordinary, &mut low, || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(matches!(
        decode_rule_decision_payload_v1(&raw, &memory, MemoryClass::Recovery, &mut work(), || {
            false
        }),
        Err(ValueTreeError::Invalid(_))
    ));
    let mut calls = 0;
    assert!(matches!(
        decode_rule_decision_payload_v1(&raw, &memory, MemoryClass::Ordinary, &mut work(), || {
            calls += 1;
            calls > 2
        }),
        Err(ValueTreeError::Cancelled)
    ));
}

#[test]
fn owned_rule_payload_transfers_plain_dto_with_its_fee() {
    let memory = MemoryBudget::new(128 << 20, 1 << 20).unwrap();
    let value = json!({"action_id":9,"verdict":"deny","notes":["retain across merge"]});
    let raw = serde_cbor::to_vec(&value).unwrap();
    let owner = decode_rule_decision_payload_owned_v1(
        &raw,
        &memory,
        MemoryClass::Ordinary,
        &mut work(),
        || false,
    )
    .unwrap();
    let (decision, fee) = owner.into_parts();
    assert_eq!(
        decision,
        serde_json::from_value::<RuleDecision>(value).unwrap()
    );
    assert!(fee.bytes() >= std::mem::size_of::<RuleDecision>());
    drop(decision);
    drop(fee);
}
