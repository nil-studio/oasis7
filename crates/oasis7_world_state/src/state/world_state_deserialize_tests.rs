use super::{FIELDS, WorldState};
use serde_cbor::Value as CborValue;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

fn legacy_oracle() -> Value {
    serde_json::from_str(include_str!("world_state_deserialize_legacy_oracle.json")).unwrap()
}

fn full_default_map() -> Map<String, Value> {
    let mut map = serde_json::to_value(WorldState::default())
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    for field in FIELDS {
        map.entry((*field).to_owned())
            .or_insert_with(|| match *field {
                "world_rules"
                | "retained_history_root"
                | "latest_product_validation"
                | "governance_finality_signer_registry"
                | "governance_main_token_controller_registry" => Value::Null,
                _ => json!({}),
            });
    }
    assert_eq!(map.len(), 100);
    map
}

fn adapter_payloads() -> Vec<(&'static str, Value)> {
    let main_token_config = full_default_map()["main_token_config"].clone();
    vec![
        (
            "factory_production_failure_dispositions",
            json!({"17": {
                "action_id": 17, "requester_agent_id": "r", "factory_id": "f", "recipe_id": "p",
                "blocker_kind": "blocked", "blocker_detail": "detail", "disposition_kind": "lost",
                "consumed_inputs": [], "lost_inputs": [], "consumed_power": 1, "lost_power": 2,
                "next_action": "retry"
            }}),
        ),
        ("product_validation_receipts", json!({"17": [{}]})),
        ("product_validation_attempts", json!({"17": [{}]})),
        (
            "pending_factory_builds",
            json!({"17": {
                "job_id": 17, "builder_agent_id": "a", "site_id": "s",
                "spec": {"factory_id":"f", "display_name":"F", "tier":1,
                    "build_time_ticks":1, "base_power_draw":0, "recipe_slots":1,
                    "throughput_bps":1, "maintenance_per_tick":0}, "ready_at":1
            }}),
        ),
        (
            "pending_recipe_jobs",
            json!({"17": {
                "job_id":17, "requester_agent_id":"a", "factory_id":"f", "recipe_id":"r",
                "accepted_batches":1, "consume":[], "produce":[], "byproducts":[],
                "power_required":0, "duration_ticks":1, "ready_at":1
            }}),
        ),
        ("industry_settlement_orders", json!({"17":19})),
        ("recipe_completion_receipts", json!({"17":{}})),
        (
            "pending_material_transits",
            json!({"17": {
                "job_id":17, "requester_agent_id":"a", "from_ledger":"world", "to_ledger":"world",
                "kind":"ore", "amount":1, "distance_km":0, "loss_bps":0, "ready_at":1
            }}),
        ),
        (
            "module_release_requests",
            json!({"17": {
                "request_id":17, "requester_agent_id":"a", "activate":false,
                "manifest":{"module_id":"m", "name":"M", "version":"1", "kind":"reducer",
                    "wasm_hash":"w", "interface_version":"1"}
            }}),
        ),
        (
            "module_release_manifest_mappings",
            json!({"17": {"request_id":17, "release_id":"r", "module_id":"m"}}),
        ),
        (
            "main_token_epoch_issuance_records",
            json!({"17": {
                "epoch_index":17,"inflation_rate_bps":1,"issued_amount":2,"staking_reward_amount":1,
                "node_service_reward_amount":1,"ecosystem_pool_amount":0,"security_reserve_amount":0
            }}),
        ),
        (
            "main_token_scheduled_policy_updates",
            json!({"17": {"proposal_id":17,"effective_epoch":2,"next_config":main_token_config}}),
        ),
        (
            "main_token_node_points_bridge_records",
            json!({"17": {"epoch_index":17,"settlement_hash":"h","total_amount":1,"distributions":[]}}),
        ),
        (
            "system_order_pool_budgets",
            json!({"17": {
                "epoch_index":17,"total_credit_budget":2,"remaining_credit_budget":1,
                "node_credit_caps":{},"node_credit_allocated":{}
            }}),
        ),
    ]
}

#[test]
fn preserves_required_fields_defaults_and_full_map_sequence_shape() {
    let oracle = legacy_oracle();
    let expected = WorldState {
        time: 7,
        ..WorldState::default()
    };
    let minimal: WorldState = serde_json::from_str(r#"{"time":7,"agents":{}}"#).unwrap();
    assert_eq!(minimal, expected);
    assert_eq!(
        serde_json::to_value(&minimal).unwrap(),
        oracle["minimal_serialized"]
    );
    assert_eq!(
        minimal.material_ledgers,
        super::super::default_material_ledgers()
    );
    assert_eq!(minimal.next_industry_settlement_order, 1);
    assert_eq!(minimal.next_module_release_request_id, 1);
    assert_eq!(minimal.next_module_instance_id, 1);
    assert_eq!(minimal.next_module_market_order_id, 1);
    assert_eq!(minimal.next_module_market_sale_id, 1);

    let full_map = full_default_map();
    let parsed_map: WorldState = serde_json::from_value(Value::Object(full_map.clone())).unwrap();
    assert_eq!(parsed_map, WorldState::default());
    assert_eq!(
        serde_json::to_value(parsed_map).unwrap(),
        oracle["full_map_serialized"]
    );
    let full_sequence = Value::Array(
        FIELDS
            .iter()
            .map(|field| full_map.get(*field).unwrap().clone())
            .collect(),
    );
    let parsed_sequence: WorldState = serde_json::from_value(full_sequence).unwrap();
    assert_eq!(parsed_sequence, WorldState::default());
    assert_eq!(
        serde_json::to_value(parsed_sequence).unwrap(),
        oracle["full_sequence_serialized"]
    );
    for prefix_len in [4, 34] {
        let prefix = Value::Array(
            FIELDS
                .iter()
                .take(prefix_len)
                .map(|field| full_map.get(*field).unwrap().clone())
                .collect(),
        );
        assert_eq!(
            serde_json::from_value::<WorldState>(prefix).unwrap(),
            WorldState::default()
        );
    }

    assert_eq!(
        serde_json::from_str::<WorldState>("[]")
            .unwrap_err()
            .to_string(),
        "invalid length 1, expected struct WorldState with 100 elements at line 1 column 2"
    );
    assert_eq!(
        serde_json::from_str::<WorldState>("[null,7]")
            .unwrap_err()
            .to_string(),
        "invalid length 3, expected struct WorldState with 100 elements at line 1 column 8"
    );
    assert_eq!(
        serde_json::from_str::<WorldState>("[null,7,null]")
            .unwrap_err()
            .to_string(),
        "invalid length 3, expected struct WorldState with 100 elements at line 1 column 13"
    );
    assert_eq!(
        serde_json::from_str::<WorldState>(r#"{"time":7}"#)
            .unwrap_err()
            .to_string(),
        "missing field `agents` at line 1 column 10"
    );
    let mut too_long = match full_sequence_for_oracle() {
        Value::Array(items) => items,
        _ => unreachable!(),
    };
    too_long.push(Value::Null);
    assert_eq!(
        serde_json::from_value::<WorldState>(Value::Array(too_long))
            .unwrap_err()
            .to_string(),
        "invalid length 101, expected fewer elements in array"
    );
}

fn full_sequence_for_oracle() -> Value {
    let map = full_default_map();
    Value::Array(
        FIELDS
            .iter()
            .map(|field| map.get(*field).unwrap().clone())
            .collect(),
    )
}

#[test]
fn preserves_map_order_duplicate_and_unknown_field_behavior() {
    assert_eq!(
        serde_json::from_str::<WorldState>("{}")
            .unwrap_err()
            .to_string(),
        "missing field `time` at line 1 column 2"
    );
    assert_eq!(
        serde_json::from_str::<WorldState>(r#"{"time":"bad","agents":{},"resources":1}"#)
            .unwrap_err()
            .to_string(),
        "invalid type: string \"bad\", expected u64 at line 1 column 13"
    );
    assert_eq!(
        serde_json::from_str::<WorldState>(r#"{"agents":{},"resources":1,"time":"bad"}"#)
            .unwrap_err()
            .to_string(),
        "invalid type: integer `1`, expected a map at line 1 column 26"
    );
    assert_eq!(
        serde_json::from_str::<WorldState>(r#"{"time":1,"time":2,"agents":{}}"#)
            .unwrap_err()
            .to_string(),
        "duplicate field `time` at line 1 column 16"
    );
    assert!(
        serde_json::from_str::<WorldState>(r#"{"time":1,"agents":{},"future_field":{"x":1}}"#)
            .is_ok()
    );
}

#[test]
fn every_numeric_key_adapter_preserves_missing_null_and_nonempty_behavior() {
    let oracle = legacy_oracle();
    let names: Vec<&str> = adapter_payloads()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for name in &names {
        let mut missing = full_default_map();
        missing.remove(*name);
        assert_eq!(
            serde_json::from_value::<WorldState>(Value::Object(missing)).unwrap(),
            WorldState::default(),
            "missing {name}"
        );

        let mut null_value = full_default_map();
        null_value.insert((*name).to_owned(), Value::Null);
        assert!(
            serde_json::from_value::<WorldState>(Value::Object(null_value)).is_err(),
            "null {name}"
        );
    }

    for (name, payload) in adapter_payloads() {
        let mut map = full_default_map();
        map.insert(name.to_owned(), payload);
        let decoded: WorldState = serde_json::from_value(Value::Object(map)).unwrap();
        let encoded = serde_json::to_value(decoded).unwrap();
        assert!(!encoded[name]["17"].is_null(), "nonempty {name}");
        assert_eq!(
            encoded[name], oracle["adapter_nonempty"][name],
            "legacy oracle {name}"
        );
    }
}

#[test]
fn preserves_numeric_key_failures_normalization_and_packed_cbor_fields() {
    let base = full_default_map();
    let value = json!({
        "epoch_index":17,"total_credit_budget":2,"remaining_credit_budget":1,
        "node_credit_caps":{},"node_credit_allocated":{}
    });
    for key in ["not-a-number", "18446744073709551616"] {
        let mut map = base.clone();
        map.insert("system_order_pool_budgets".into(), json!({key: value}));
        assert!(
            serde_json::from_value::<WorldState>(Value::Object(map))
                .unwrap_err()
                .to_string()
                .contains("invalid numeric map key")
        );
    }
    let mut collision = base.clone();
    collision.insert("system_order_pool_budgets".into(), json!({
        "01": {"epoch_index":1,"total_credit_budget":1,"remaining_credit_budget":1,"node_credit_caps":{},"node_credit_allocated":{}},
        "1": {"epoch_index":2,"total_credit_budget":2,"remaining_credit_budget":2,"node_credit_caps":{},"node_credit_allocated":{}}
    }));
    let collision: WorldState = serde_json::from_value(Value::Object(collision)).unwrap();
    assert_eq!(collision.system_order_pool_budgets[&1].epoch_index, 2);

    let payload = serde_cbor::value::to_value(value).unwrap();
    let inner = CborValue::Map(BTreeMap::from([(CborValue::Integer(17), payload.clone())]));
    let outer = CborValue::Map(BTreeMap::from([
        (CborValue::Integer(1), CborValue::Integer(7)),
        (CborValue::Integer(3), CborValue::Map(BTreeMap::new())),
        (CborValue::Integer(92), inner),
    ]));
    let decoded: WorldState = serde_cbor::value::from_value(outer).unwrap();
    assert_eq!(decoded.time, 7);
    assert_eq!(decoded.system_order_pool_budgets[&17].epoch_index, 17);

    let negative = CborValue::Map(BTreeMap::from([
        (CborValue::Integer(1), CborValue::Integer(7)),
        (CborValue::Integer(3), CborValue::Map(BTreeMap::new())),
        (
            CborValue::Integer(92),
            CborValue::Map(BTreeMap::from([(CborValue::Integer(-1), payload)])),
        ),
    ]));
    assert_eq!(
        serde_cbor::value::from_value::<WorldState>(negative)
            .unwrap_err()
            .to_string(),
        "invalid numeric map key `-1`: must be >= 0"
    );
}

#[test]
fn enriched_legacy_map_json_sequence_and_cbor_sequence_match_old_derive() {
    let oracle: Value = serde_json::from_str(include_str!(
        "world_state_deserialize_enriched_legacy_oracle.json"
    ))
    .unwrap();
    assert_eq!(oracle["fieldCount"], FIELDS.len());

    let map_state: WorldState = serde_json::from_value(oracle["inputMap"].clone()).unwrap();
    assert_eq!(
        serde_json::to_string(&map_state).unwrap(),
        oracle["expectedOldDerived"]["map"]["serialize"]
    );
    assert_eq!(
        format!("{map_state:?}"),
        oracle["expectedOldDerived"]["map"]["debug"]
    );

    let json_sequence: WorldState = serde_json::from_value(Value::Array(
        oracle["inputJsonSequence"].as_array().unwrap().clone(),
    ))
    .unwrap();
    assert_eq!(
        serde_json::to_string(&json_sequence).unwrap(),
        oracle["expectedOldDerived"]["jsonSequence"]["serialize"]
    );
    assert_eq!(
        format!("{json_sequence:?}"),
        oracle["expectedOldDerived"]["jsonSequence"]["debug"]
    );

    let cbor_bytes = include_bytes!("world_state_deserialize_enriched_sequence.cbor");
    assert_eq!(
        cbor_bytes.len() as u64,
        oracle["cborSequenceBytes"].as_u64().unwrap()
    );
    let cbor_sequence: WorldState = serde_cbor::from_slice(cbor_bytes).unwrap();
    assert_eq!(
        serde_json::to_string(&cbor_sequence).unwrap(),
        oracle["expectedOldDerived"]["cborSequence"]["serialize"]
    );
    assert_eq!(
        format!("{cbor_sequence:?}"),
        oracle["expectedOldDerived"]["cborSequence"]["debug"]
    );

    assert_eq!(map_state.time, 18);
    assert_eq!(map_state.next_industry_settlement_order, 51);
    assert_eq!(map_state.agent_claim_last_processed_epoch, 65);
    assert_eq!(map_state.next_module_release_request_id, 85);
    assert_eq!(map_state.next_module_instance_id, 88);
    assert_eq!(map_state.next_module_market_order_id, 89);
    assert_eq!(map_state.next_module_market_sale_id, 90);
    let serialized = serde_json::to_value(&map_state).unwrap();
    let adapter_keys = oracle["adapterMarkerKeys"].as_object().unwrap();
    assert_eq!(adapter_keys.len(), 14);
    for (field, marker) in adapter_keys {
        let key = marker["key"].as_str().unwrap();
        assert!(
            !serialized[field.as_str()][key].is_null(),
            "expected marked nonempty adapter entry for {field}[{key}]"
        );
    }
}
