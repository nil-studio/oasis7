use oasis7_agent_api::game::{
    ActionSubmissionV1, DecimalU64, MAX_JSON_DEPTH, MAX_REQUEST_BYTES, OperationStatusV1,
    RetryAdviceV1, parse_request,
};
use serde_json::{Value, json};

fn action() -> Value {
    json!({
        "api_revision": 1, "client_operation_id": "operation-1",
        "task_id": "task-1", "executor_epoch": "18446744073709551615",
        "context_id": "context-1", "action_ref": "schedule_recipe",
        "arguments": {"recipe_id": "smelter"},
        "actor_proof": {"challenge_id": "challenge-1", "subject_public_key": "device", "signature_hex": "proof"}
    })
}

#[test]
fn recovery_keeps_original_operation_and_unknown_statuses_fail_closed() {
    let pending: OperationStatusV1 = parse_request(br#""pending""#).unwrap();
    let recovery: OperationStatusV1 = parse_request(br#""recovery_required""#).unwrap();
    assert_eq!(pending.retry_advice(), RetryAdviceV1::Wait);
    assert_eq!(recovery.retry_advice(), RetryAdviceV1::LookupOriginal);
    for terminal in [
        OperationStatusV1::Committed,
        OperationStatusV1::Rejected,
        OperationStatusV1::Failed,
    ] {
        assert_eq!(terminal.retry_advice(), RetryAdviceV1::None);
    }
    for unknown in [
        br#""cancelled""#.as_slice(),
        br#""success""#,
        br#""run_unknown""#,
    ] {
        assert!(parse_request::<OperationStatusV1>(unknown).is_err());
    }
}

#[test]
fn action_roundtrip_preserves_full_u64_and_rejects_unknown_request_fields() {
    let value = action();
    let request: ActionSubmissionV1 = parse_request(&serde_json::to_vec(&value).unwrap()).unwrap();
    request.validate().unwrap();
    assert_eq!(request.executor_epoch, DecimalU64(u64::MAX));
    assert_eq!(serde_json::to_value(&request).unwrap(), value);
    for path in ["root", "proof"] {
        let mut value = action();
        if path == "root" {
            value["authority"] = json!("owner");
        } else {
            value["actor_proof"]["authority"] = json!("owner");
        }
        assert!(parse_request::<ActionSubmissionV1>(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}

#[test]
fn decimal_codec_rejects_numeric_and_noncanonical_epochs() {
    for invalid in [
        json!(1),
        json!("01"),
        json!("+1"),
        json!("-1"),
        json!(""),
        json!("18446744073709551616"),
        json!("1\n"),
        json!("18446744073709551615\n"),
    ] {
        assert!(
            serde_cbor::from_slice::<DecimalU64>(&serde_cbor::to_vec(&invalid).unwrap()).is_err()
        );
        let mut value = action();
        value["executor_epoch"] = invalid;
        assert!(parse_request::<ActionSubmissionV1>(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}

#[test]
fn duplicate_keys_are_rejected_even_inside_dynamic_arguments() {
    for raw in [
        r#"{"api_revision":1,"api_revision":2}"#,
        r#"{"arguments":{"recipe_id":"a","recipe_id":"b"}}"#,
        r#"{"arguments":[{"key":1,"k\u0065y":2}]}"#,
    ] {
        let error = parse_request::<Value>(raw.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("duplicate JSON key"));
    }
}

#[test]
fn parser_enforces_exact_depth_byte_and_single_document_boundaries() {
    let nested = |depth: usize| format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
    assert!(parse_request::<Value>(nested(MAX_JSON_DEPTH).as_bytes()).is_ok());
    assert!(parse_request::<Value>(nested(MAX_JSON_DEPTH + 1).as_bytes()).is_err());
    let mut bytes = b"null".to_vec();
    bytes.resize(MAX_REQUEST_BYTES, b' ');
    assert!(parse_request::<Value>(&bytes).is_ok());
    bytes.push(b' ');
    assert!(parse_request::<Value>(&bytes).is_err());
    for invalid in [b"null null".as_slice(), b"{", b"[NaN]", b"\xff"] {
        assert!(parse_request::<Value>(invalid).is_err());
    }
}

#[test]
fn action_structure_cannot_claim_a_future_revision_or_scalar_arguments() {
    let mut request: ActionSubmissionV1 =
        parse_request(&serde_json::to_vec(&action()).unwrap()).unwrap();
    request.api_revision = 2;
    assert!(request.validate().is_err());
    request.api_revision = 1;
    request.arguments = json!("freeform command");
    assert!(request.validate().is_err());
    request.arguments = json!({});
    request.client_operation_id.clear();
    assert!(request.validate().is_err());
}

#[test]
fn bootstrap_discovery_has_resolvable_schemas_and_enforced_limits() {
    use oasis7_agent_api::game::*;
    let document = read_bootstrap_openapi();
    assert_eq!(document["openapi"], "3.1.0");
    assert_eq!(document["x-oasis7-api-revision"], API_REVISION);
    assert_eq!(document["x-oasis7-schema-revision"], READ_SCHEMA_REVISION);
    assert_eq!(document["x-oasis7-limits"], read_bootstrap_limits());
    assert!(document["paths"].get(READ_OPENAPI_PATH).is_some());
    fn check_refs(value: &serde_json::Value, root: &serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                if let Some(reference) = fields.get("$ref") {
                    let reference = reference.as_str().unwrap();
                    let pointer = reference.strip_prefix('#').expect("only local schemas");
                    assert!(
                        root.pointer(pointer).is_some(),
                        "unresolved schema: {reference}"
                    );
                }
                for value in fields.values() {
                    check_refs(value, root);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    check_refs(value, root);
                }
            }
            _ => {}
        }
    }
    check_refs(&document, &document);
    let info = &document["components"]["schemas"]["ReadInfo"]["properties"];
    assert_eq!(info["openapi_url"]["const"], READ_OPENAPI_PATH);
    assert_eq!(info["schema_revision"]["const"], READ_SCHEMA_REVISION);
    assert_eq!(info["writes_available"]["const"], false);
    for (limit, bound) in read_bootstrap_limits().as_object().unwrap() {
        assert_eq!(&info["limits"]["properties"][limit]["const"], bound);
    }
    assert_eq!(
        document["components"]["schemas"]["ReadWorldViewRequest"]["properties"]["fixed_commit"]["type"],
        "null"
    );
    assert_eq!(document["paths"].as_object().unwrap().len(), 5);
    for path in document["paths"].as_object().unwrap().values() {
        assert_eq!(
            path.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["get"]
        );
    }
}

#[test]
fn task_create_codec_binds_owner_and_history_without_granting_execution() {
    use oasis7_agent_api::game::*;
    let request = GameTaskCreateV1 {
        api_rev: API_REVISION,
        client_operation_id: "create-first".into(),
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(u64::MAX),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(u64::MAX),
    };
    request.validate().unwrap();
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["owner_generation"], u64::MAX.to_string());
    assert_eq!(
        serde_json::from_value::<GameTaskCreateV1>(value.clone()).unwrap(),
        request
    );
    for (field, invalid) in [
        ("owner_generation", serde_json::json!(1)),
        ("reorg_epoch", serde_json::json!("01")),
        ("executor_id", serde_json::json!("pretend-executor")),
    ] {
        let mut altered = value.clone();
        altered[field] = invalid;
        assert!(serde_json::from_value::<GameTaskCreateV1>(altered).is_err());
    }
    let mut altered = request.clone();
    altered.owner_generation = DecimalU64(0);
    assert!(altered.validate().is_err());
    let mut altered = request;
    altered.api_rev += 1;
    assert!(altered.validate().is_err());
}

#[test]
fn decimal_wire_boundaries_are_lossless_in_json_and_cbor() {
    for number in [0, (1 << 53) - 1, 1 << 53, u64::MAX] {
        let value = DecimalU64(number);
        let json = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&json).unwrap(),
            number.to_string()
        );
        assert_eq!(serde_json::from_slice::<DecimalU64>(&json).unwrap(), value);
        let cbor = serde_cbor::to_vec(&value).unwrap();
        let decoded: String = serde_cbor::from_slice(&cbor).unwrap();
        assert_eq!(decoded, number.to_string());
        assert_eq!(serde_cbor::from_slice::<DecimalU64>(&cbor).unwrap(), value);
    }
}

#[test]
fn known_task_decimal_fields_share_the_bootstrap_range() {
    let document = oasis7_agent_api::game::read_bootstrap_openapi();
    let schemas = &document["components"]["schemas"];
    for (schema, fields) in [
        (
            "TaskControlBootstrap",
            &[
                "reorg_epoch",
                "owner_generation",
                "task_revision",
                "latest_goal_revision",
                "active_goal_revision",
                "executor_epoch",
            ][..],
        ),
        (
            "GoalReference",
            &["goal_revision", "activation_committed_height"][..],
        ),
        (
            "ExecutorBinding",
            &["delegation_generation", "executor_epoch"][..],
        ),
    ] {
        for field in fields {
            assert_eq!(
                schemas[schema]["properties"][field]["$ref"],
                "#/components/schemas/DecimalU64"
            );
            if schema != "TaskControlBootstrap" {
                assert_eq!(schemas[schema]["properties"][field]["pattern"], "^[1-9]");
            }
        }
    }
}
