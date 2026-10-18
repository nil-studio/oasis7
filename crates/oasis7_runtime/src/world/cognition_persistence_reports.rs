//! Recovery report constructors for cognition projection outcomes.
//!
//! These constructors keep durable recovery decisions separate from projection
//! validation and repair helpers without changing the persisted report shape.

use crate::cognition_recovery::{
    CognitionReceiptViewV1, CognitionRecoveryReport, WorldCommitRecordV1, WorldRootViewV1,
};

pub(super) fn pending_report(
    marker: &WorldCommitRecordV1,
    root: WorldRootViewV1,
) -> CognitionRecoveryReport {
    let mut root = root;
    root.head_status = "recovery_pending".to_string();
    root.commit_id = None;
    root.quarantine_id = None;
    CognitionRecoveryReport {
        world_root: Some(root),
        receipt: None,
        disposition: "recovery_pending".to_string(),
        reject_reason: None,
        auto_submitted: false,
        idempotency_key: Some(marker.envelope_idempotency_key.clone()),
        quarantine_id: None,
        candidate_root: None,
        candidate_receipt: None,
        journal_head: String::new(),
        retry_count: 0,
        revalidation_count: 1,
        projection_repairs: 0,
        provider_invocation_count: 0,
        kernel_invocation_count: 0,
        effect_count: 0,
        debit_count: 0,
        world_receipt_linked_count: 0,
        event_count: 0,
        response_replayed: false,
    }
}

pub(super) fn conflict_report(
    marker: &WorldCommitRecordV1,
    root: WorldRootViewV1,
) -> CognitionRecoveryReport {
    let quarantine_id = format!("quarantine:{}", marker.commit_id);
    let mut root = root;
    root.state_root = marker.parent_world_hash.clone();
    root.logical_tick = marker.parent_tick;
    root.head_status = "recovery_pending".to_string();
    root.commit_id = None;
    root.quarantine_id = Some(quarantine_id.clone());
    CognitionRecoveryReport {
        world_root: Some(root),
        receipt: None,
        disposition: "recovery_pending".to_string(),
        reject_reason: Some("commit_conflict".to_string()),
        auto_submitted: false,
        idempotency_key: Some(marker.envelope_idempotency_key.clone()),
        quarantine_id: Some(quarantine_id),
        candidate_root: Some(marker.staged_state_root.clone()),
        candidate_receipt: Some(CognitionReceiptViewV1 {
            receipt_id: marker.receipt_id.clone(),
            receipt_digest: marker.receipt_digest.clone(),
        }),
        journal_head: String::new(),
        retry_count: 0,
        revalidation_count: 0,
        projection_repairs: 0,
        provider_invocation_count: 0,
        kernel_invocation_count: 0,
        effect_count: 0,
        debit_count: 0,
        world_receipt_linked_count: 0,
        event_count: 0,
        response_replayed: false,
    }
}

pub(super) fn visible_root_conflict_report(
    marker: &WorldCommitRecordV1,
    mut canonical_root: WorldRootViewV1,
) -> CognitionRecoveryReport {
    let quarantine_id = format!("quarantine:{}", marker.commit_id);
    canonical_root.head_status = "canonical".to_string();
    canonical_root.commit_id = (marker.status == "committed").then(|| marker.commit_id.clone());
    canonical_root.quarantine_id = None;
    CognitionRecoveryReport {
        world_root: Some(canonical_root),
        receipt: None,
        disposition: "recovery_pending".to_string(),
        reject_reason: Some("world_root_mismatch".to_string()),
        auto_submitted: false,
        idempotency_key: Some(marker.envelope_idempotency_key.clone()),
        quarantine_id: Some(quarantine_id),
        candidate_root: Some(marker.staged_state_root.clone()),
        candidate_receipt: Some(CognitionReceiptViewV1 {
            receipt_id: marker.receipt_id.clone(),
            receipt_digest: marker.receipt_digest.clone(),
        }),
        journal_head: String::new(),
        retry_count: 0,
        revalidation_count: 0,
        projection_repairs: 0,
        provider_invocation_count: 0,
        kernel_invocation_count: 0,
        effect_count: 0,
        debit_count: 0,
        world_receipt_linked_count: 0,
        event_count: 0,
        response_replayed: false,
    }
}
