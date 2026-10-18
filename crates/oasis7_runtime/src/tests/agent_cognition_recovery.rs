//! P0.3 RED fixtures for response replay, duplicate commit projection, and
//! additive legacy snapshot compatibility.

use crate::{
    CognitionRecovery, CognitionRecoveryFixture, CognitionRecoveryProbe, CognitionSnapshotV1,
    WorldCommitRecordV1, WorldRootViewV1,
};
use serde_json::{Value, json};

const WORLD_ID: &str = "world-cognition-fixture";
const BRANCH_ID: &str = "main";
const R_NEXT: &str = "R_next";
const RECEIPT_ID: &str = "receipt-cognition-7";
const RECEIPT_DIGEST: &str = "digest:receipt-cognition-7";
const IDEMPOTENCY_KEY: &str = "key:cognition-7";
const ENVELOPE_DIGEST: &str = "digest:envelope-cognition-7";

fn committed_marker() -> WorldCommitRecordV1 {
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
        "parent_world_hash": "R_parent",
        "staged_event_root": "events:R_next",
        "staged_state_root": R_NEXT,
        "receipt_id": RECEIPT_ID,
        "receipt_digest": RECEIPT_DIGEST,
        "reorg_epoch": 3,
        "cognition_journal_seq": 12,
        "status": "committed",
        "abort_reason": null
    }))
    .expect("decode committed marker")
}

fn committed_root() -> WorldRootViewV1 {
    serde_json::from_value(json!({
        "schema_version": "world-root-view.v1",
        "world_id": WORLD_ID,
        "branch_id": BRANCH_ID,
        "logical_tick": 42,
        "state_root": R_NEXT,
        "head_status": "canonical",
        "commit_id": "commit-cognition-7",
        "quarantine_id": null
    }))
    .expect("decode committed root")
}

fn committed_fixture() -> CognitionRecoveryFixture {
    CognitionRecoveryFixture::committed(
        committed_root(),
        committed_marker(),
        json!({
            "response_digest": "digest:response-cognition-7",
            "response_artifact": {"decision": {"type": "move_agent"}},
            "envelope_digest": ENVELOPE_DIGEST,
            "journal_head": "digest:journal-head-12"
        }),
    )
}

fn report(fixture: &mut CognitionRecoveryFixture, probe: &mut CognitionRecoveryProbe) -> Value {
    serde_json::to_value(CognitionRecovery::recover(fixture, probe).expect("recovery report"))
        .expect("encode recovery report")
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn durable_response_replay_does_not_invoke_provider_again() {
    let mut probe = CognitionRecoveryProbe::default();
    probe.provider_invocation_count = 1;
    let before = probe.provider_invocation_count;

    let mut fixture = committed_fixture();
    let recovered = report(&mut fixture, &mut probe);
    assert_eq!(probe.provider_invocation_count, before);
    assert_eq!(recovered["provider_invocation_count"], 0);
    assert_eq!(recovered["response_replayed"], true);
    assert_eq!(recovered["disposition"], "committed");
    assert_eq!(recovered["world_root"]["state_root"], R_NEXT);
    assert_eq!(recovered["receipt"]["receipt_id"], RECEIPT_ID);
}

#[test]
fn duplicate_replay_is_projection_idempotent_and_has_one_effect_receipt_event() {
    let mut fixture = committed_fixture();
    let mut probe = CognitionRecoveryProbe::default();
    let first = report(&mut fixture, &mut probe);
    let second = report(&mut fixture, &mut probe);

    for field in [
        "journal_head",
        "world_root",
        "receipt",
        "disposition",
        "event_count",
        "world_receipt_linked_count",
        "effect_count",
        "debit_count",
    ] {
        assert_eq!(first[field], second[field], "replay changed {field}");
    }
    assert_eq!(second["world_receipt_linked_count"], 1);
    assert_eq!(second["effect_count"], 1);
    assert_eq!(second["debit_count"], 1);
}

#[test]
fn legacy_snapshot_is_read_only_and_maps_to_legacy_no_cognition_proof() {
    let legacy = json!({
        "schema_version": "snapshot.v0",
        "world_id": WORLD_ID,
        "queued_action": {
            "action_id": 7,
            "action": {"type": "move_agent"},
            "success": true,
            "summary": "legacy success"
        }
    });
    let snapshot: CognitionSnapshotV1 =
        CognitionSnapshotV1::from_legacy_json(legacy).expect("legacy compatibility decode");
    let report = CognitionRecovery::restore_snapshot(snapshot)
        .expect("legacy snapshot must produce explicit compatibility result");
    let report = serde_json::to_value(report).expect("encode compatibility result");

    assert_eq!(report["disposition"], "rejected");
    assert_eq!(report["reject_reason"], "legacy_no_cognition_proof");
    assert_eq!(report["auto_submitted"], false);
    assert_eq!(report["provider_invocation_count"], 0);
    assert_eq!(report["effect_count"], 0);
    assert_eq!(report["receipt"], Value::Null);
}
