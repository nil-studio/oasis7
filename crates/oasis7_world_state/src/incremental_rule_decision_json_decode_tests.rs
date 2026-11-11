use super::*;
use serde_json::json;
fn work() -> RuleDecisionPayloadDecodeWorkV1 {
    RuleDecisionPayloadDecodeWorkV1 {
        structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
            bytes_remaining: 1 << 20,
            values_remaining: 100000,
            max_depth: 64,
        },
        max_payload_bytes: 1 << 16,
        max_heap_bytes: 64 << 20,
    }
}
fn canonical() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 100000,
        bytes_remaining: 1 << 20,
        max_depth: 64,
        max_map_keys: 64,
        max_key_bytes: 1024,
    }
}
#[test]
fn borrowed_json_rule_payload_preserves_original_value_semantics_and_error_text() {
    let memory = MemoryBudget::new(128 << 20, 1 << 20).unwrap();
    let complex = crate::Action::ScheduleRecipe {
        requester_agent_id: "agent".into(),
        factory_id: "factory".into(),
        recipe_id: "recipe".into(),
        plan: oasis7_wasm_abi::RecipeExecutionPlan {
            accepted_batches: 2,
            consume: vec![],
            produce: vec![],
            byproducts: vec![],
            power_required: 17,
            duration_ticks: 3,
            reject_reason: None,
        },
        logistics_route_ids: vec!["route".into()],
        logistics_path_ids: vec!["path".into()],
    };
    for value in [
        json!({"action_id":7,"verdict":"modify","override_action":complex,"notes":["quote"],"cost":{"entries":{"electricity":-4}}}),
        json!({"action_id":1,"verdict":"allow","unknown":{"list":[1,2,3]}}),
        json!({"action_id":"bad","verdict":"deny"}),
        json!({"action_id":null,"verdict":"allow"}),
        json!({"action_id":2,"verdict":"modify","override_action":{"type":"MoveAgent","data":{"agent_id":"x","to":{"x_cm":1,"y_cm":2,"z_cm":3}}}}),
        json!({"action_id":3,"verdict":"deny","notes":null}),
    ] {
        let expected = serde_json::from_value::<crate::rules::RuleDecision>(value.clone());
        let actual = decode_rule_decision_json_payload_owned_v1(
            &value,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            &mut canonical(),
            || false,
        );
        match (expected, actual) {
            (Ok(expected), Ok(actual)) => {
                let (value, fee) = actual.into_parts();
                assert_eq!(value, expected);
                drop(value);
                drop(fee);
            }
            (Err(expected), Err(RuleDecisionJsonDecodeErrorV1::Payload(actual))) => {
                assert_eq!(actual.to_string(), expected.to_string())
            }
            _ => panic!("JSON projection classification differs"),
        }
    }
}
#[test]
fn json_rule_payload_budget_and_cancellation_are_host_errors() {
    let memory = MemoryBudget::new(128 << 20, 1 << 20).unwrap();
    let value = json!({"action_id":1,"verdict":"allow"});
    for mode in 0..4 {
        let mut work = work();
        let mut canonical = canonical();
        if mode == 0 {
            work.max_payload_bytes = 1;
        }
        if mode == 1 {
            work.max_heap_bytes = 1;
        }
        if mode == 2 {
            canonical.visits_remaining = 0;
        }
        let result = decode_rule_decision_json_payload_owned_v1(
            &value,
            &memory,
            MemoryClass::Ordinary,
            &mut work,
            &mut canonical,
            || mode == 3,
        );
        assert!(matches!(
            result,
            Err(RuleDecisionJsonDecodeErrorV1::Host(_))
        ));
    }
}
