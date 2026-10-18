use super::*;
use oasis7_agent_api::game::{DecimalU64, GAME_TASK_CREATE_DOMAIN, GameTaskCreateV1};

pub(super) fn fixture(id: &str) -> (String, serde_json::Value) {
    let payload = WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: id.into(),
                world_id: "world".into(),
                genesis_digest: "fixture".into(),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(0),
                agent_id: "agent".into(),
                owner_binding: "owner".into(),
                owner_generation: DecimalU64(1),
            },
            &hex::encode([89u8; 32]),
        )
        .unwrap(),
    );
    let correlation = derive_correlation(
        WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "fixture".into(),
        },
        &payload,
    )
    .unwrap();
    let key = key_digest(&correlation.key).unwrap();
    let result = CanonicalIntentResultV1 {
        request: SubmitIntentRequest {
            contract_version: 1,
            correlation,
            deadline_unix_ms: None,
            signed_payload: payload,
        },
        receipt: serde_json::Value::Null,
        rejected: Some("fixture_rejection".into()),
        action_id: 1,
        committed_height: 1,
    };
    (key, serde_json::to_value(result).unwrap())
}

#[test]
fn identical_canonical_result_reuses_only_completed_validation() {
    let cache = std::sync::Mutex::new(result_cache::Cache::default());
    let validate_result_record =
        |key: &str, value: &serde_json::Value| result_cache::validate_with(&cache, key, value);
    let (key, value) = fixture("result-cache-repeat");
    RESULT_VALIDATIONS.with(|count| count.set(0));
    let first = validate_result_record(&key, &value).unwrap();
    let second = validate_result_record(&key, &value).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        RESULT_VALIDATIONS.with(std::cell::Cell::get),
        1,
        "identical immutable canonical receipts must not repeat signature/history codec work"
    );
}

#[test]
fn cached_identity_cannot_hide_changed_signature_key_or_receipt() {
    let cache = std::sync::Mutex::new(result_cache::Cache::default());
    let validate_result_record =
        |key: &str, value: &serde_json::Value| result_cache::validate_with(&cache, key, value);
    let (key, value) = fixture("result-cache-tamper");
    validate_result_record(&key, &value).unwrap();
    let mut changed = value.clone();
    changed["committed_height"] = 0.into();
    assert!(validate_result_record(&key, &changed).is_err());
    changed = value.clone();
    changed["receipt"] = serde_json::json!({"forged":true});
    assert!(validate_result_record(&key, &changed).is_err());
    assert!(validate_result_record("wrong-key", &value).is_err());
    let mut different_world = value.clone();
    different_world["request"]["correlation"]["key"]["world"]["world_id"] = "another-world".into();
    assert!(validate_result_record(&key, &different_world).is_err());
    let mut typed: CanonicalIntentResultV1 = serde_json::from_value(value.clone()).unwrap();
    let WorldServicePayloadV1::GameTaskCreate(signed) = &mut typed.request.signed_payload else {
        unreachable!()
    };
    signed.signature_hex = "00".repeat(64);
    assert!(validate_result_record(&key, &serde_json::to_value(typed).unwrap()).is_err());
    let mut valid_change = value.clone();
    valid_change["action_id"] = 2.into();
    assert_eq!(
        validate_result_record(&key, &valid_change)
            .unwrap()
            .action_id,
        2
    );
    assert_eq!(validate_result_record(&key, &value).unwrap().action_id, 1);
    changed = value.clone();
    // Alter the signed payload structurally, independently of its stored correlation.
    changed["request"]["signed_payload"] = serde_json::Value::Null;
    assert!(validate_result_record(&key, &changed).is_err());
    assert!(validate_result_record(&key, &value).is_ok());
}
