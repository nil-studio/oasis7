//! Differential compatibility tests for the hand-written LLM payload visitor.
//!
//! Error strings and boundaries below were recorded from the pre-change
//! serde_derive 1.0.229 implementation; no derived mirror remains in scanned Rust.

use super::{FIELD_COUNT, FIELD_NAMES, LlmDecisionPayload};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
enum WrongValue {
    IntegerOne,
    StringBad,
}

impl WrongValue {
    fn value(self) -> Value {
        match self {
            Self::IntegerOne => json!(1),
            Self::StringBad => json!("bad"),
        }
    }
}

struct WrongTypeGroup {
    fields: &'static [&'static str],
    value: WrongValue,
    expected: &'static str,
}

const WRONG_TYPE_GROUPS: &[WrongTypeGroup] = &[
    WrongTypeGroup {
        fields: &[
            "decision",
            "to",
            "from",
            "from_owner",
            "to_owner",
            "kind",
            "buyer",
            "seller",
            "side",
            "owner",
            "location_id",
            "factory_id",
            "factory_kind",
            "facility_id",
            "target_id",
            "blocker_type",
            "recipe_id",
            "publisher",
            "installer",
            "bidder",
            "module_id",
            "module_version",
            "install_target_type",
            "install_target_location_id",
            "manifest_path",
            "wasm_hash",
            "wasm_bytes_hex",
            "price_kind",
            "actor",
            "declarer",
            "subject",
            "object",
            "schema_id",
            "claim",
            "challenger",
            "reason",
            "adjudicator",
            "adjudication",
            "notes",
            "relation_kind",
            "proposer_agent_id",
            "alliance_id",
            "member_agent_id",
            "charter",
            "initiator_agent_id",
            "war_id",
            "aggressor_alliance_id",
            "defender_alliance_id",
            "objective",
            "proposal_key",
            "title",
            "description",
            "voter_agent_id",
            "option",
            "resolver_agent_id",
            "crisis_id",
            "strategy",
            "operator_agent_id",
            "target_agent_id",
            "track",
            "achievement_id",
            "creator_agent_id",
            "contract_id",
            "counterparty_agent_id",
            "settlement_kind",
            "accepter_agent_id",
        ],
        value: WrongValue::IntegerOne,
        expected: "invalid type: integer `1`, expected a string",
    },
    WrongTypeGroup {
        fields: &[
            "max_amount",
            "amount",
            "price_per_pu",
            "limit_price_per_pu",
            "compound_mass_g",
            "batches",
            "price_amount",
            "confidence_ppm",
            "weight_bps",
            "points",
            "settlement_amount",
            "reputation_stake",
        ],
        value: WrongValue::StringBad,
        expected: "invalid type: string \"bad\", expected i64",
    },
    WrongTypeGroup {
        fields: &[
            "ticks",
            "order_id",
            "bid_order_id",
            "ttl_ticks",
            "fact_id",
            "voting_window_ticks",
            "quorum_weight",
            "expires_at",
        ],
        value: WrongValue::StringBad,
        expected: "invalid type: string \"bad\", expected u64",
    },
    WrongTypeGroup {
        fields: &["intensity", "weight"],
        value: WrongValue::StringBad,
        expected: "invalid type: string \"bad\", expected u32",
    },
    WrongTypeGroup {
        fields: &["pass_threshold_bps"],
        value: WrongValue::StringBad,
        expected: "invalid type: string \"bad\", expected u16",
    },
    WrongTypeGroup {
        fields: &["activate", "success"],
        value: WrongValue::StringBad,
        expected: "invalid type: string \"bad\", expected a boolean",
    },
    WrongTypeGroup {
        fields: &[
            "evidence_event_ids",
            "backing_fact_ids",
            "members",
            "options",
        ],
        value: WrongValue::StringBad,
        expected: "invalid type: string \"bad\", expected a sequence",
    },
    WrongTypeGroup {
        fields: &["source_files"],
        value: WrongValue::StringBad,
        expected: "invalid type: string \"bad\", expected a map",
    },
    WrongTypeGroup {
        fields: &["action_kind", "base_cost_class", "base_risk_class"],
        value: WrongValue::IntegerOne,
        expected: "invalid type: integer `1`, expected string or map",
    },
    WrongTypeGroup {
        fields: &["stake"],
        value: WrongValue::IntegerOne,
        expected: "invalid type: integer `1`, expected struct LlmSocialStakePayload",
    },
];

fn parse_value(value: Value) -> Result<LlmDecisionPayload, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn empty_sequence() -> Vec<Value> {
    let mut values = vec![json!("wait")];
    values.extend((1..FIELD_COUNT).map(|_| Value::Null));
    values
}

fn all_fields_unique_payload_json() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/llm_decision_payload_legacy_all_fields.json"
    ))
    .expect("legacy all-fields fixture is valid JSON")
}

#[test]
fn every_map_and_sequence_field_matches_legacy_wrong_type_oracle() {
    let mut tested = 0;
    for group in WRONG_TYPE_GROUPS {
        for &field in group.fields {
            assert!(FIELD_NAMES.contains(&field), "unknown test field {field}");
            let bad_value = group.value.value();
            let mut object = Map::new();
            object.insert("decision".into(), json!("wait"));
            object.insert(field.into(), bad_value.clone());
            assert_eq!(
                parse_value(Value::Object(object)).unwrap_err(),
                group.expected,
                "map field {field}",
            );

            let index = FIELD_NAMES
                .iter()
                .position(|name| *name == field)
                .expect("field name has source-order index");
            let mut sequence = empty_sequence();
            sequence[index] = bad_value;
            assert_eq!(
                parse_value(Value::Array(sequence)).unwrap_err(),
                group.expected,
                "sequence field {field} at index {index}",
            );
            tested += 1;
        }
    }
    assert_eq!(tested, FIELD_COUNT - 1);
}

#[test]
fn action_accepts_arbitrary_json_value_in_map_and_sequence() {
    let action = json!({"nested": [1, null, {"key": true}]});
    let payload = parse_value(json!({"decision": "wait", "action": action})).unwrap();
    assert_eq!(
        payload.action,
        Some(json!({"nested": [1, null, {"key": true}]}))
    );

    let mut sequence = empty_sequence();
    sequence[1] = json!({"nested": [1, null, {"key": true}]});
    let payload = parse_value(Value::Array(sequence)).unwrap();
    assert_eq!(
        payload.action,
        Some(json!({"nested": [1, null, {"key": true}]}))
    );
}

#[test]
fn explicit_null_is_seen_before_duplicate_value_for_every_optional_field() {
    let mut tested = 0;
    for group in WRONG_TYPE_GROUPS {
        for &field in group.fields {
            if field == "decision" {
                continue;
            }
            let key = serde_json::to_string(field).unwrap();
            let input = format!("{{\"decision\":\"wait\",{key}:null,{key}:\"bad\"}}");
            let error = serde_json::from_str::<LlmDecisionPayload>(&input)
                .unwrap_err()
                .to_string();
            assert!(
                error.starts_with(&format!("duplicate field `{field}`")),
                "{error}"
            );
            tested += 1;
        }
    }
    let input = r#"{"decision":"wait","action":null,"action":"bad"}"#;
    let error = serde_json::from_str::<LlmDecisionPayload>(input)
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("duplicate field `action`"), "{error}");
    assert_eq!(tested + 1, FIELD_COUNT - 1);
}

#[test]
fn aliases_unknown_fields_and_map_error_order_match_legacy() {
    for alias in ["adjudication", "adjudication_decision", "verdict"] {
        let mut object = Map::new();
        object.insert("decision".into(), json!("wait"));
        object.insert(alias.into(), json!("accepted"));
        let payload = parse_value(Value::Object(object)).unwrap();
        assert_eq!(payload.adjudication.as_deref(), Some("accepted"));
    }

    for input in [
        r#"{"decision":"wait","adjudication":null,"verdict":"accepted"}"#,
        r#"{"decision":"wait","adjudication_decision":null,"adjudication":"accepted"}"#,
        r#"{"decision":"wait","verdict":null,"adjudication_decision":"accepted"}"#,
    ] {
        let error = serde_json::from_str::<LlmDecisionPayload>(input)
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("duplicate field `adjudication`"),
            "{error}"
        );
    }

    let duplicate = serde_json::from_str::<LlmDecisionPayload>(
        r#"{"decision":"wait","ticks":1,"ticks":"bad"}"#,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(duplicate, "duplicate field `ticks` at line 1 column 36");

    let unknown = parse_value(json!({
        "decision": "wait",
        "future": {"bad": [1, {"nested": true}]}
    }))
    .unwrap();
    assert_eq!(unknown.decision, "wait");

    assert_eq!(
        parse_value(json!({"decision": "wait", "ticks": "bad", "weight": "bad"})).unwrap_err(),
        "invalid type: string \"bad\", expected u64",
    );
    assert_eq!(
        parse_value(json!({"ticks": "bad"})).unwrap_err(),
        "invalid type: string \"bad\", expected u64",
    );
    let reversed = r#"{"decision":"wait","weight":"bad","ticks":"bad"}"#;
    assert!(
        serde_json::from_str::<LlmDecisionPayload>(reversed)
            .unwrap_err()
            .to_string()
            .starts_with("invalid type: string \"bad\", expected u32"),
    );
}

#[test]
fn sequence_length_and_top_level_errors_match_legacy() {
    for length in 0..FIELD_COUNT {
        let values: Vec<_> = empty_sequence().into_iter().take(length).collect();
        assert_eq!(
            parse_value(Value::Array(values)).unwrap_err(),
            format!(
                "invalid length {length}, expected struct LlmDecisionPayload with 101 elements"
            ),
        );
    }
    assert!(parse_value(Value::Array(empty_sequence())).is_ok());

    let mut trailing = empty_sequence();
    trailing.push(json!("trailing value"));
    assert_eq!(
        parse_value(Value::Array(trailing)).unwrap_err(),
        "invalid length 102, expected fewer elements in array",
    );

    for (value, expected) in [
        (
            Value::Null,
            "invalid type: null, expected struct LlmDecisionPayload",
        ),
        (
            json!(false),
            "invalid type: boolean `false`, expected struct LlmDecisionPayload",
        ),
        (
            json!(0),
            "invalid type: integer `0`, expected struct LlmDecisionPayload",
        ),
        (
            json!("wait"),
            "invalid type: string \"wait\", expected struct LlmDecisionPayload",
        ),
    ] {
        assert_eq!(parse_value(value).unwrap_err(), expected);
    }
}

#[test]
fn cbor_packed_numeric_field_ids_keep_global_declaration_order() {
    use serde_cbor::Value as CborValue;

    let mut fields = BTreeMap::new();
    fields.insert(CborValue::Integer(0), CborValue::Text("wait".into()));
    fields.insert(CborValue::Integer(2), CborValue::Integer(7));
    fields.insert(
        CborValue::Integer(37),
        CborValue::Map(BTreeMap::from([(
            CborValue::Text("a.rs".into()),
            CborValue::Text("source".into()),
        )])),
    );
    fields.insert(CborValue::Integer(58), CborValue::Text("accepted".into()));
    fields.insert(CborValue::Integer(100), CborValue::Text("agent-9".into()));
    fields.insert(CborValue::Integer(101), CborValue::Text("unknown".into()));

    let bytes = serde_cbor::to_vec(&CborValue::Map(fields)).unwrap();
    let payload = serde_cbor::from_slice::<LlmDecisionPayload>(&bytes).unwrap();
    assert_eq!(payload.decision, "wait");
    assert_eq!(payload.ticks, Some(7));
    assert_eq!(
        payload
            .source_files
            .unwrap()
            .get("a.rs")
            .map(String::as_str),
        Some("source")
    );
    assert_eq!(payload.adjudication.as_deref(), Some("accepted"));
    assert_eq!(payload.accepter_agent_id.as_deref(), Some("agent-9"));

    let mut negative_fields = BTreeMap::new();
    negative_fields.insert(CborValue::Integer(-1), CborValue::Text("bad".into()));
    negative_fields.insert(CborValue::Integer(0), CborValue::Text("wait".into()));
    let bytes = serde_cbor::to_vec(&CborValue::Map(negative_fields)).unwrap();
    assert_eq!(
        serde_cbor::from_slice::<LlmDecisionPayload>(&bytes)
            .unwrap_err()
            .to_string(),
        "invalid type: integer `-1`, expected field identifier",
    );
}

#[test]
fn decision_wait_and_json_error_mapping_are_unchanged() {
    let (decision, error) = super::super::parse_llm_decision_value_with_error(
        json!({"decision": "wait"}),
        "agent-1",
        false,
    );
    assert!(matches!(decision, super::super::AgentDecision::Wait));
    assert_eq!(error, None);

    let (decision, error) =
        super::super::parse_llm_decision_value_with_error(json!({"ticks": 1}), "agent-1", false);
    assert!(matches!(decision, super::super::AgentDecision::Wait));
    assert_eq!(
        error.as_deref(),
        Some("json parse failed: missing field `decision`")
    );
}

#[test]
fn all_fields_unique_map_and_sequence_match_legacy_pretty_debug() {
    let input = all_fields_unique_payload_json();
    let fields = input.as_object().expect("legacy fixture is a JSON object");
    assert_eq!(fields.len(), FIELD_COUNT);
    for field in FIELD_NAMES {
        assert!(fields.contains_key(field), "missing fixture field {field}");
    }
    assert!(fields.values().all(|value| !value.is_null()));

    let sequence = FIELD_NAMES
        .iter()
        .map(|field| {
            fields
                .get(*field)
                .cloned()
                .unwrap_or_else(|| panic!("missing sequence fixture field {field}"))
        })
        .collect::<Vec<_>>();
    assert_eq!(sequence.len(), FIELD_COUNT);

    let expected_debug =
        include_str!("fixtures/llm_decision_payload_legacy_pretty_debug.txt").trim_end();
    let map_payload = parse_value(input).expect("all 101 typed sentinel map fields parse");
    assert_eq!(
        format!("{map_payload:#?}"),
        expected_debug,
        "map deserialization preserves each old-derived field value",
    );
    let sequence_payload =
        parse_value(Value::Array(sequence)).expect("all 101 typed sentinel sequence fields parse");
    assert_eq!(
        format!("{sequence_payload:#?}"),
        expected_debug,
        "sequence deserialization preserves source-order field values",
    );
}
