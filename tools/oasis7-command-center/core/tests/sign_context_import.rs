use oasis7_command_center_core::tools::import_sign_context;
use serde_json::{Value, json};

fn context() -> Value {
    json!({
        "deployment_id": "existing-deployment",
        "network_id": "existing-network",
        "task_uid": "existing-task",
        "source_head_oid": "a".repeat(40),
        "protocol_context": {
            "authority_id": "existing-authority",
            "rollback_ticket": "existing-ticket",
            "receipt_id": "existing-receipt",
            "nonce": "existing-nonce"
        }
    })
}

#[test]
fn imports_exact_existing_context_without_adding_authorization() {
    let original = context();
    assert_eq!(
        import_sign_context(&original.to_string()).unwrap(),
        original
    );
}

#[test]
fn rejects_unknown_fields_at_both_protocol_levels() {
    let mut extra = context();
    extra["grant_id"] = json!("not-an-authorized-grant");
    assert!(import_sign_context(&extra.to_string()).is_err());
    let mut nested = context();
    nested["protocol_context"]["policy"] = json!("not-a-policy");
    assert!(import_sign_context(&nested.to_string()).is_err());
}

#[test]
fn rejects_missing_malformed_unbounded_and_semantically_invalid_context() {
    for text in ["{", "null", "{}"] {
        assert!(import_sign_context(text).is_err());
    }
    for (field, invalid) in [
        ("source_head_oid", json!("not-an-oid")),
        ("deployment_id", json!(" ")),
        ("network_id", json!("x".repeat(129))),
    ] {
        let mut value = context();
        value[field] = invalid;
        assert!(import_sign_context(&value.to_string()).is_err(), "{field}");
    }
    let mut missing = context();
    missing["protocol_context"]
        .as_object_mut()
        .unwrap()
        .remove("receipt_id");
    assert!(import_sign_context(&missing.to_string()).is_err());
    let mut empty = context();
    empty["protocol_context"]["nonce"] = json!("");
    assert!(import_sign_context(&empty.to_string()).is_err());
    assert!(import_sign_context(&" ".repeat(65537)).is_err());
}
