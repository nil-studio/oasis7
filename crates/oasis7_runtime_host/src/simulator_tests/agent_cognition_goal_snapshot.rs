//! Harness-policy RED fixtures for the bounded GoalSnapshot projection.
//!
//! GoalSnapshot is only the P0 compatibility projection.  GoalGraph, belief,
//! preference correction, player commitments and goal completion authority are
//! intentionally out of scope.

use crate::simulator_tests::{GoalSnapshotInputV1, GoalSnapshotProjector, GoalSnapshotV1};
use serde_json::{Value, json};

fn host_goal() -> GoalSnapshotInputV1 {
    serde_json::from_value(json!({
        "revision": 7,
        "short_term_summary": "  host short  ",
        "long_term_summary": "host long",
        "blocked_reason": "host block",
        "provenance": "harness_projection"
    }))
    .expect("decode host GoalSnapshot input")
}

fn legacy_goal() -> GoalSnapshotInputV1 {
    serde_json::from_value(json!({
        "revision": 1,
        "short_term_summary": "legacy short",
        "long_term_summary": "legacy long",
        "blocked_reason": "legacy block",
        "provenance": "legacy_provider"
    }))
    .expect("decode legacy GoalSnapshot input")
}

#[test]
fn empty_goal_snapshot_is_explicit_revision_zero_and_digest_stable() {
    let first = GoalSnapshotV1::empty();
    let second = GoalSnapshotV1::empty();
    assert_eq!(first.revision, 0);
    assert_eq!(first.short_term_summary, "");
    assert_eq!(first.long_term_summary, "");
    assert_eq!(first.blocked_reason, None);
    assert_eq!(first.provenance, "harness_projection");
    assert!(!first.digest.is_empty());
    assert_eq!(
        first.digest,
        "blake3:31c93293570c36946869bc941435981ad5063734e65342a5eda6b79ec6fc9fe9"
    );
    assert_eq!(first.digest, second.digest);

    let encoded = serde_json::to_value(&first).expect("encode empty GoalSnapshot");
    assert!(encoded.get("revision").is_some());
    assert!(encoded.get("short_term_summary").is_some());
    assert!(encoded.get("long_term_summary").is_some());
    assert!(encoded.get("provenance").is_some());
    assert!(encoded.get("digest").is_some());
    assert!(
        GoalSnapshotV1::from_value(json!({
            "revision": 0,
            "short_term_summary": null,
            "long_term_summary": "",
            "provenance": "harness_projection",
            "digest": first.digest
        }))
        .is_err(),
        "null must not replace an explicit empty summary"
    );
}

#[test]
fn trusted_host_goal_values_take_precedence_over_legacy_provider_projection() {
    let projected = GoalSnapshotProjector::project(Some(host_goal()), Some(legacy_goal()))
        .expect("project host and legacy goals");
    assert_eq!(projected.revision, 7);
    assert_eq!(projected.short_term_summary, "host short");
    assert_eq!(projected.long_term_summary, "host long");
    assert_eq!(projected.blocked_reason.as_deref(), Some("host block"));
    assert_eq!(projected.provenance, "harness_projection");

    let legacy_only = GoalSnapshotProjector::project(None, Some(legacy_goal()))
        .expect("project explicit legacy compatibility goal");
    assert_eq!(legacy_only.short_term_summary, "legacy short");
    assert_eq!(legacy_only.provenance, "legacy_provider");
}

#[test]
fn goal_snapshot_normalizes_nfc_trim_and_rejects_summary_bounds_or_control_bytes() {
    let normalized = GoalSnapshotProjector::project(
        Some(
            serde_json::from_value(json!({
                "revision": 2,
                "short_term_summary": " e\u{301} ",
                "long_term_summary": " long ",
                "blocked_reason": " blocked ",
                "provenance": "harness_projection"
            }))
            .expect("decode normalized host goal"),
        ),
        None,
    )
    .expect("normalize GoalSnapshot strings");
    assert_eq!(normalized.short_term_summary, "é");
    assert_eq!(normalized.long_term_summary, "long");
    assert_eq!(normalized.blocked_reason.as_deref(), Some("blocked"));

    for field in ["short_term_summary", "long_term_summary"] {
        let mut value = json!({
            "revision": 2,
            "short_term_summary": "ok",
            "long_term_summary": "ok",
            "blocked_reason": null,
            "provenance": "harness_projection"
        });
        value[field] = Value::String("x".repeat(513));
        let error = GoalSnapshotProjector::project(
            Some(serde_json::from_value(value).expect("decode oversized goal")),
            None,
        )
        .expect_err("summary bound");
        assert_eq!(error.code(), "goal_snapshot_too_large");
    }

    let mut blocked = json!({
        "revision": 2,
        "short_term_summary": "ok",
        "long_term_summary": "ok",
        "blocked_reason": "ok",
        "provenance": "harness_projection"
    });
    blocked["blocked_reason"] = Value::String("x".repeat(257));
    assert_eq!(
        GoalSnapshotProjector::project(
            Some(serde_json::from_value(blocked).expect("decode oversized blocked reason")),
            None,
        )
        .expect_err("blocked reason bound")
        .code(),
        "goal_snapshot_too_large"
    );

    let mut control = json!({
        "revision": 2,
        "short_term_summary": "bad\u{0000}",
        "long_term_summary": "ok",
        "blocked_reason": null,
        "provenance": "harness_projection"
    });
    control["short_term_summary"] = json!("bad\u{0000}");
    assert_eq!(
        GoalSnapshotProjector::project(
            Some(serde_json::from_value(control).expect("decode control goal")),
            None,
        )
        .expect_err("control character")
        .code(),
        "goal_snapshot_invalid"
    );
}

#[test]
fn goal_digest_changes_with_trusted_projection_and_is_not_provider_supplied() {
    let first = GoalSnapshotProjector::project(Some(host_goal()), None).expect("first projection");
    let mut changed = host_goal();
    changed.long_term_summary = "different host goal".to_string();
    let second = GoalSnapshotProjector::project(Some(changed), None).expect("second projection");
    assert_ne!(first.digest, second.digest);
    assert!(first.digest.starts_with("blake3:"));
    assert!(second.digest.starts_with("blake3:"));
}
