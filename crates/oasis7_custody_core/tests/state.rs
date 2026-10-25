use oasis7_custody_core::{
    DecimalU64,
    budget::BudgetLedger,
    lifecycle::{SessionLifecycle, SessionWindow},
    operations::Operation,
    recovery::RecoveryFence,
};
use serde::{Serialize, de::DeserializeOwned};

fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T) {
    let json = serde_json::to_string(value).unwrap();
    assert_eq!(serde_json::from_str::<T>(&json).unwrap(), *value);
}

#[test]
fn persisted_state_preserves_full_width_decimal_integers() {
    let mut budget = BudgetLedger::new(u64::MAX);
    budget.reserve("pending", 9_007_199_254_740_993).unwrap();
    roundtrip(&budget);
    let encoded = serde_json::to_string(&budget).unwrap();
    assert!(encoded.contains("\"18446744073709551615\""));
    assert!(encoded.contains("\"9007199254740993\""));
    roundtrip(&RecoveryFence::new(u64::MAX, true));
    let mut operation = Operation::admit("request".into(), "digest".into()).unwrap();
    operation.prepare(vec![1, 2, 3]).unwrap();
    operation.claim(u64::MAX).unwrap();
    operation.mark_unknown().unwrap();
    roundtrip(&operation);
    let time = 9_007_199_254_740_993;
    let window = SessionWindow {
        approved_at: DecimalU64(time),
        write_deadline: DecimalU64(time + 300),
        read_until: DecimalU64(time + 1800),
        cleanup_until: DecimalU64(time + 600),
    };
    let mut session = SessionLifecycle::new(window).unwrap();
    session.heartbeat("heartbeat", time + 30).unwrap();
    roundtrip(&session);
}

#[test]
fn storage_rejects_unknown_fields_and_noncanonical_integer_representations() {
    for invalid in [
        r#"{"epoch":1,"disabled":false}"#,
        r#"{"epoch":"01","disabled":false}"#,
        r#"{"epoch":"18446744073709551616","disabled":false}"#,
        r#"{"epoch":"1","disabled":false,"extra":true}"#,
    ] {
        assert!(serde_json::from_str::<RecoveryFence>(invalid).is_err());
    }
}

#[test]
fn rejected_mutations_preserve_persisted_records() {
    let mut budget = BudgetLedger::new(10);
    budget.reserve("request", 8).unwrap();
    let original = budget.clone();
    assert!(budget.reserve("request", 9).is_err());
    assert!(budget.charge(3).is_err());
    assert!(budget.settle("request", 9).is_err());
    assert_eq!(budget, original);

    let mut operation = Operation::admit("request".into(), "digest".into()).unwrap();
    operation.prepare(vec![1]).unwrap();
    operation.claim(1).unwrap();
    operation.mark_unknown().unwrap();
    let original = operation.clone();
    assert!(operation.prepare(vec![2]).is_err());
    assert!(operation.cancel_before_dispatch().is_err());
    assert_eq!(operation, original);

    let mut fence = RecoveryFence::new(3, true);
    let original = fence;
    assert!(fence.tighten(2, true).is_err());
    assert!(fence.tighten(4, false).is_err());
    assert_eq!(fence, original);
}

#[test]
fn restore_rejects_inconsistent_accounting_and_operation_states() {
    for input in [
        r#"{"limit":"10","spent":"11","reservations":{}}"#,
        r#"{"limit":"10","spent":"3","reservations":{"a":"8"}}"#,
        r#"{"limit":"18446744073709551615","spent":"0","reservations":{"a":"18446744073709551615","b":"1"}}"#,
    ] {
        assert!(serde_json::from_str::<BudgetLedger>(input).is_err());
    }
    let mut op = Operation::admit("request".into(), "digest".into()).unwrap();
    let admitted = serde_json::to_value(&op).unwrap();
    for state in ["prepared", "dispatching", "unknown", "confirmed"] {
        let mut invalid = admitted.clone();
        invalid["state"] = state.into();
        assert!(serde_json::from_value::<Operation>(invalid).is_err());
    }
    op.prepare(vec![1]).unwrap();
    op.claim(1).unwrap();
    let dispatching = serde_json::to_value(&op).unwrap();
    for (field, value) in [
        ("signed_bytes", serde_json::json!([])),
        ("claimed_epoch", serde_json::Value::Null),
        ("receipt_digest", serde_json::json!("unconfirmed")),
        ("request_id", serde_json::json!("")),
    ] {
        let mut invalid = dispatching.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<Operation>(invalid).is_err());
    }
}

#[test]
fn restore_rejects_invalid_session_windows_and_heartbeat_history() {
    let window = SessionWindow {
        approved_at: DecimalU64(100),
        write_deadline: DecimalU64(400),
        read_until: DecimalU64(700),
        cleanup_until: DecimalU64(500),
    };
    let mut session = SessionLifecycle::new(window).unwrap();
    roundtrip(&session);
    session.heartbeat("one", 130).unwrap();
    let original = serde_json::to_value(&session).unwrap();
    for (field, value) in [
        ("lease_until", serde_json::json!("401")),
        ("last_heartbeat_at", serde_json::Value::Null),
        ("heartbeats", serde_json::json!({"one":"200"})),
        ("heartbeats", serde_json::json!({"one":"401"})),
        ("heartbeats", serde_json::json!({"":"220"})),
    ] {
        let mut invalid = original.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<SessionLifecycle>(invalid).is_err());
    }
    let mut invalid = original;
    invalid["window"]["write_deadline"] = "2000".into();
    assert!(serde_json::from_value::<SessionLifecycle>(invalid).is_err());
    session.close();
    roundtrip(&session);
    let short = SessionWindow {
        write_deadline: DecimalU64(140),
        cleanup_until: DecimalU64(400),
        ..window
    };
    let mut clamped = SessionLifecycle::new(short).unwrap();
    clamped.heartbeat("one", 130).unwrap();
    roundtrip(&clamped);
}
