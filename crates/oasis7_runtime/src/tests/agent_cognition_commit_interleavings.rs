//! P0.3 RED fixtures for the durable cognition journal/commit protocol.
//!
//! The fixture is intentionally expressed through the wished-for typed
//! recovery seam.  It is the executable contract for issue #3602: a prepared
//! marker is not an effect, a committed marker is the sole recovery anchor,
//! and a contradictory marker can only produce a quarantined pending head.

use crate::{
    CognitionCrashPrefix, CognitionRecovery, CognitionRecoveryFixture, CognitionRecoveryProbe,
    WorldCommitRecordV1, WorldRootViewV1,
};
use serde_json::{Value, json};

const WORLD_ID: &str = "world-cognition-fixture";
const BRANCH_ID: &str = "main";
const R_PARENT: &str = "R_parent";
const R_NEXT: &str = "R_next";
const RECEIPT_ID: &str = "receipt-cognition-7";
const RECEIPT_DIGEST: &str = "digest:receipt-cognition-7";
const IDEMPOTENCY_KEY: &str = "key:cognition-7";
const ENVELOPE_DIGEST: &str = "digest:envelope-cognition-7";

fn commit_record(status: &str) -> WorldCommitRecordV1 {
    serde_json::from_value(json!({
        "schema_version": "world-commit-record.v1",
        "commit_id": "commit-cognition-7",
        "envelope_idempotency_key": IDEMPOTENCY_KEY,
        "envelope_digest": ENVELOPE_DIGEST,
        "world_id": WORLD_ID,
        "branch_id": BRANCH_ID,
        "finality_epoch": 7,
        "finality_block_hash": "hash:finality-7",
        "finality_status": "verified",
        "finality_binding_digest": "digest:finality-binding-7",
        "runtime_manifest_hash": "hash:runtime-manifest-7",
        "action_id": "action:cognition-7",
        "parent_tick": 41,
        "parent_world_hash": R_PARENT,
        "staged_event_root": "events:R_next",
        "staged_state_root": R_NEXT,
        "receipt_id": RECEIPT_ID,
        "receipt_digest": RECEIPT_DIGEST,
        "reorg_epoch": 3,
        "cognition_journal_seq": 12,
        "status": status,
        "abort_reason": null
    }))
    .expect("decode typed WorldCommitRecordV1 fixture")
}

fn root_view(root: &str, head_status: &str) -> WorldRootViewV1 {
    serde_json::from_value(json!({
        "schema_version": "world-root-view.v1",
        "world_id": WORLD_ID,
        "branch_id": BRANCH_ID,
        "logical_tick": if root == R_PARENT { 41 } else { 42 },
        "state_root": root,
        "head_status": head_status,
        "commit_id": null,
        "quarantine_id": null
    }))
    .expect("decode typed WorldRootViewV1 fixture")
}

fn fixture(prefix: CognitionCrashPrefix) -> CognitionRecoveryFixture {
    let status = match &prefix {
        CognitionCrashPrefix::BeforePrepared
        | CognitionCrashPrefix::PreparedOnly
        | CognitionCrashPrefix::Conflict => "prepared",
        CognitionCrashPrefix::Committed | CognitionCrashPrefix::CommittedMissingProjection => {
            "committed"
        }
    };
    CognitionRecoveryFixture::new(
        prefix,
        root_view(R_PARENT, "canonical"),
        commit_record(status),
    )
}

fn recover(mut fixture: CognitionRecoveryFixture) -> Value {
    let mut probe = CognitionRecoveryProbe::default();
    let report = CognitionRecovery::recover(&mut fixture, &mut probe)
        .expect("recovery fixture must produce a structured report");
    serde_json::to_value(report).expect("encode recovery report")
}

fn assert_pending_parent_without_receipt(report: &Value) {
    assert_eq!(report["world_root"]["state_root"], R_PARENT);
    assert_eq!(report["world_root"]["head_status"], "recovery_pending");
    assert!(
        report["receipt"].is_null(),
        "pending prefix exposed a receipt"
    );
    assert_eq!(report["provider_invocation_count"], 0);
    assert_eq!(report["kernel_invocation_count"], 0);
    assert_eq!(report["effect_count"], 0);
    assert_eq!(report["debit_count"], 0);
}

#[test]
fn before_prepared_keeps_r_parent_and_allows_no_unrecorded_effect() {
    let report = recover(fixture(CognitionCrashPrefix::BeforePrepared));
    assert_pending_parent_without_receipt(&report);
    assert_eq!(report["disposition"], "recovery_pending");
    assert_eq!(report["revalidation_count"], 1);
}

#[test]
fn prepared_only_is_not_guessable_and_keeps_r_parent() {
    let report = recover(fixture(CognitionCrashPrefix::PreparedOnly));
    assert_pending_parent_without_receipt(&report);
    assert_eq!(report["disposition"], "recovery_pending");
    assert_eq!(report["world_receipt_linked_count"], 0);
}

#[test]
fn aborted_marker_is_a_terminal_no_effect_recovery_disposition() {
    let mut fixture = fixture(CognitionCrashPrefix::PreparedOnly);
    fixture.commit_record.status = "aborted".to_string();
    fixture.commit_record.abort_reason = Some("cancelled".to_string());

    let report = recover(fixture);
    assert_eq!(report["disposition"], "aborted");
    assert_eq!(report["reject_reason"], "cancelled");
    assert_eq!(report["world_root"]["state_root"], R_PARENT);
    assert_eq!(report["world_root"]["head_status"], "canonical");
    assert!(report["receipt"].is_null());
    assert_eq!(report["effect_count"], 0);
    assert_eq!(report["debit_count"], 0);
    assert_eq!(report["provider_invocation_count"], 0);
    assert_eq!(report["kernel_invocation_count"], 0);
}

#[test]
fn committed_marker_publishes_r_next_and_exact_recorded_receipt_once() {
    let report = recover(fixture(CognitionCrashPrefix::Committed));
    assert_eq!(report["world_root"]["state_root"], R_NEXT);
    assert_eq!(report["world_root"]["head_status"], "canonical");
    assert_eq!(report["receipt"]["receipt_id"], RECEIPT_ID);
    assert_eq!(report["receipt"]["receipt_digest"], RECEIPT_DIGEST);
    assert_eq!(report["disposition"], "committed");
    assert_eq!(report["provider_invocation_count"], 0);
    assert_eq!(report["kernel_invocation_count"], 0);
    assert_eq!(report["effect_count"], 1);
    assert_eq!(report["debit_count"], 1);
    assert_eq!(report["world_receipt_linked_count"], 1);
}

#[test]
fn committed_missing_receipt_key_or_link_repairs_projection_only() {
    let mut fixture = fixture(CognitionCrashPrefix::CommittedMissingProjection);
    fixture.receipt_projection_present = false;
    fixture.idempotency_projection_present = false;
    fixture.world_receipt_linked = false;

    let report = recover(fixture);
    assert_eq!(report["world_root"]["state_root"], R_NEXT);
    assert_eq!(report["world_root"]["head_status"], "canonical");
    assert_eq!(report["receipt"]["receipt_id"], RECEIPT_ID);
    assert_eq!(report["receipt"]["receipt_digest"], RECEIPT_DIGEST);
    assert_eq!(report["idempotency_key"], IDEMPOTENCY_KEY);
    assert_eq!(report["disposition"], "committed");
    assert_eq!(report["projection_repairs"], 3);
    assert_eq!(report["provider_invocation_count"], 0);
    assert_eq!(report["kernel_invocation_count"], 0);
    assert_eq!(report["effect_count"], 0);
    assert_eq!(report["debit_count"], 0);
}

#[test]
fn marker_root_key_or_receipt_conflict_exposes_parent_and_quarantines_candidate() {
    for conflict in [
        "marker_mismatch",
        "root_mismatch",
        "idempotency_key_mismatch",
        "receipt_digest_mismatch",
    ] {
        let mut fixture = fixture(CognitionCrashPrefix::Conflict);
        fixture.conflict = Some(conflict.to_string());

        let report = recover(fixture);
        assert_pending_parent_without_receipt(&report);
        assert_eq!(report["disposition"], "recovery_pending");
        assert!(report["quarantine_id"].as_str().is_some());
        assert_eq!(report["candidate_root"], R_NEXT);
        assert_eq!(report["candidate_receipt"]["receipt_id"], RECEIPT_ID);
        assert_eq!(report["retry_count"], 0);
    }
}
