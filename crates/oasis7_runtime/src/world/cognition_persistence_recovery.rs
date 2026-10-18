//! Crash-safe cognition projection recovery.
//!
//! Recovery is kept separate from the write path so the marker, journal and
//! root reconciliation rules can be audited independently of transaction
//! construction. It never invokes provider, kernel, effect, or debit code.

use super::super::cognition_persistence_validation::{
    cognition_error, cognition_validation, legacy_recovery_report, parent_root,
    persist_recovery_report, strict_optional_finality_hash, validate_cognition_journal_head,
    validate_cognition_journal_integrity, validate_marker_current_world,
    validate_response_lineage_binding, validate_response_record,
};
use super::World;
use super::cognition_persistence_reports::{
    conflict_report, pending_report, visible_root_conflict_report,
};
use super::cognition_persistence_support::{
    committed_marker_history_conflict, has_idempotency_conflict, has_receipt_conflict,
    marker_root_conflict, reconcile_committed_projection, record_repair, select_commit_record,
    validate_marker, visible_root_conflict,
};
use super::{CognitionProjection, JOURNAL_SCHEMA, PROJECTION_SCHEMA};
use crate::cognition_recovery::{
    CognitionReceiptViewV1, CognitionRecoveryReport, WorldRootViewV1,
    default_cognition_persistence_projection,
};
use crate::cognition_scheduler::CognitionScheduler;
use crate::cognition_wake::AgentContinuation;
use crate::error::WorldError;
use serde_json::{Value as JsonValue, json};

impl World {
    /// Reconcile a persisted cognition commit without invoking provider,
    /// kernel, effect, or debit code.
    pub fn recover_cognition(&mut self) -> Result<CognitionRecoveryReport, WorldError> {
        self.recover_cognition_mode(true)
    }

    /// Validate the durable projection without applying repair or cloning the
    /// state/journal/artifacts of an observer world.
    pub(in crate::world) fn validate_observer_cognition(
        &mut self,
    ) -> Result<(), WorldError> {
        self.recover_cognition_mode(false).map(|_| ())
    }

    fn recover_cognition_mode(
        &mut self,
        allow_recovery: bool,
    ) -> Result<CognitionRecoveryReport, WorldError> {
        let mut projection = if self.cognition.is_null() {
            default_cognition_persistence_projection()
        } else {
            self.cognition.clone()
        };
        let feedback_projection_changed =
            Self::recover_feedback_outbox_projection(&mut projection)?;
        let mut parsed: CognitionProjection = serde_json::from_value(projection.clone())
            .map_err(|error| cognition_error("invalid_cognition_projection", error))?;

        if parsed.schema_version.is_empty() {
            if feedback_projection_changed {
                if !allow_recovery && self.cognition != projection {
                    return Err(observer_recovery_required());
                }
                self.cognition = projection;
            }
            return Ok(legacy_recovery_report());
        }
        if parsed.schema_version != PROJECTION_SCHEMA {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "cognition projection schema mismatch expected={PROJECTION_SCHEMA} actual={}",
                    parsed.schema_version
                ),
            });
        }
        if !parsed.cognition_journal.schema_version.is_empty()
            && parsed.cognition_journal.schema_version != JOURNAL_SCHEMA
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "cognition journal schema mismatch expected={JOURNAL_SCHEMA} actual={}",
                    parsed.cognition_journal.schema_version
                ),
            });
        }
        // Validate the persisted optional finality shape even for a
        // marker-less projection. Otherwise a malformed runtime binding would
        // be treated as an unproven legacy snapshot and silently normalized to
        // the same state as an explicitly absent hash.
        if let Some(binding) = projection.get("runtime_binding") {
            if !binding.is_null() && !binding.is_object() {
                return Err(cognition_validation("recovery_pending"));
            }
            if binding.is_object() {
                strict_optional_finality_hash(binding.get("finality_block_hash"))?;
            }
        }
        // A terminal failure turn has no commit marker, but its dense event
        // chain is still durable authority. Validate that chain on restore
        // instead of treating a marker-less malformed journal as legacy.
        let journal = &projection["cognition_journal"];
        let dense_journal = journal
            .get("events")
            .and_then(JsonValue::as_array)
            .is_some_and(|events| {
                events.iter().any(|event| {
                    event.get("schema_version").is_some() || event.get("event_kind").is_some()
                })
            });
        if dense_journal {
            validate_cognition_journal_integrity(journal)?;
            // A committed marker may legitimately leave one terminal event
            // absent for the recovery repair path below. Marker-less dense
            // terminal turns have no repair oracle and must have an exact
            // journal head on restore.
            if !parsed
                .commit_records
                .iter()
                .any(|marker| !marker.agent_id.is_empty())
            {
                validate_cognition_journal_head(journal)?;
            }
        }
        if projection
            .get("scheduler_state")
            .is_some_and(|state| !state.is_null())
        {
            // Scheduler restore validates policy, bucket disjointness,
            // capacity, and every durable in-flight wake identity before any
            // cognition projection is exposed to a caller.
            let scheduler = CognitionScheduler::from_snapshot_json(
                projection
                    .get("scheduler_state")
                    .cloned()
                    .unwrap_or(JsonValue::Null),
            )
            .map_err(|error| cognition_validation(error.code()))?;
            let evaluations = projection
                .get("continuation_evaluations")
                .and_then(JsonValue::as_object);
            for wake in scheduler.in_flight_wakes() {
                if evaluations.is_none_or(|values| !values.contains_key(&wake.continuation_id)) {
                    return Err(cognition_validation("cognition_wake_evaluation_missing"));
                }
            }
        }

        // A continuation that advertises an authoritative status digest must
        // be validated before restore exposes it to scheduling. Snapshots
        // from before the digest field existed remain read-only legacy data;
        // they are not silently promoted to an authoritative projection.
        if let Some(value) = projection.get("continuations") {
            let continuations: Vec<AgentContinuation> = serde_json::from_value(value.clone())
                .map_err(|error| cognition_error("invalid_continuation_projection", error))?;
            for continuation in continuations {
                if continuation.continuation_status_digest.is_some() {
                    continuation
                        .validate_authoritative()
                        .map_err(|error| cognition_validation(error.code()))?;
                }
            }
        }
        // Validity windows are Runtime authority, not a scheduler hint. Run
        // the same durable expiry transition against the restored
        // projection before validating wake references, so a persisted wake
        // cannot consume a lease after its deadline merely because the
        // process was offline.
        let mut expiry_projection_changed = false;
        if allow_recovery {
            let mut expiry_world = self.clone();
            *expiry_world.persistence_dir.borrow_mut() = None;
            expiry_world.cognition = projection.clone();
            expiry_world.expire_cognition_continuations_at_tick(self.state.time)?;
            expiry_projection_changed = expiry_world.cognition != projection;
            if expiry_projection_changed {
                projection = expiry_world.cognition.clone();
                parsed = serde_json::from_value(projection.clone())
                    .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
            }
            if let Some(scheduler_state) = projection
                .get("scheduler_state")
                .filter(|state| !state.is_null())
            {
                expiry_world.cognition = projection.clone();
                expiry_world.validate_persisted_cognition_wakes(scheduler_state)?;
            }
        } else {
            if projection != self.cognition {
                return Err(observer_recovery_required());
            }
            if let Some(value) = projection.get("continuations") {
                let continuations: Vec<AgentContinuation> = serde_json::from_value(value.clone())
                    .map_err(|error| {
                    cognition_error("invalid_continuation_projection", error)
                })?;
                if continuations.iter().any(|continuation| {
                    !Self::is_terminal_continuation_status(continuation.status)
                        && continuation
                            .valid_until_tick
                            .is_some_and(|until| self.state.time > until)
                }) {
                    return Err(observer_recovery_required());
                }
            }
            if let Some(scheduler_state) = projection
                .get("scheduler_state")
                .filter(|state| !state.is_null())
            {
                self.validate_persisted_cognition_wakes(scheduler_state)?;
            }
        }

        for marker in &parsed.commit_records {
            validate_marker(marker)?;
        }
        let has_dense_marker = parsed
            .commit_records
            .iter()
            .any(|marker| !marker.agent_id.is_empty());
        if has_dense_marker {
            // Dense v1 markers use the dense journal contract for every
            // lifecycle status, including prepared/aborted records. Legacy
            // sparse fixtures have no dense marker and remain read-only.
            validate_cognition_journal_integrity(&projection["cognition_journal"])?;
            if !parsed
                .commit_records
                .iter()
                .any(|marker| marker.status == "committed")
            {
                validate_cognition_journal_head(&projection["cognition_journal"])?;
            }
        }
        let Some(marker) = select_commit_record(&parsed.commit_records).cloned() else {
            if feedback_projection_changed || expiry_projection_changed {
                if !allow_recovery && self.cognition != projection {
                    return Err(observer_recovery_required());
                }
                self.cognition = projection;
            }
            return Ok(legacy_recovery_report());
        };
        let dense_marker = !marker.agent_id.is_empty();
        if dense_marker {
            validate_marker_current_world(self, &marker)?;
        }

        let recovery = parsed.recovery.clone().unwrap_or_default();
        let trusted_state_root = self.current_state_root_hash()?;
        let trusted_root = WorldRootViewV1 {
            schema_version: "world-root-view.v1".to_string(),
            world_id: marker.world_id.clone(),
            branch_id: marker.branch_id.clone(),
            logical_tick: self.state.time,
            state_root: trusted_state_root.clone(),
            head_status: "canonical".to_string(),
            commit_id: (marker.status == "committed").then(|| marker.commit_id.clone()),
            quarantine_id: None,
        };
        let root = if dense_marker {
            trusted_root.clone()
        } else {
            recovery
                .world_root
                .clone()
                .unwrap_or_else(|| parent_root(&marker))
        };
        if dense_marker
            && recovery.world_root.as_ref().is_some_and(|visible| {
                visible_root_conflict(&marker, visible, &trusted_state_root, self.state.time)
            })
        {
            let report = visible_root_conflict_report(&marker, trusted_root);
            persist_recovery_report(&mut projection, &report)?;
            if !allow_recovery && self.cognition != projection {
                return Err(observer_recovery_required());
            }
            self.cognition = projection;
            return Ok(report);
        }
        let response = parsed
            .responses
            .iter()
            .find(|response| response.envelope_digest == marker.envelope_digest);
        if dense_marker && marker.status == "committed" {
            // Validate the response after committed-prefix reconciliation
            // below. A crash can leave an earlier committed journal event
            // absent while retaining the response for the selected marker;
            // strict validation against that transient head would reject a
            // repairable snapshot before reconciliation gets a chance to
            // restore the missing prefix.
            response.ok_or_else(|| cognition_validation("response_missing"))?;
        }
        let mut response_journal_head = response
            .map(|response| response.journal_head.clone())
            .filter(|head| !head.is_empty());
        let response_replayed = response.is_some();

        if recovery.crash_prefix == "conflict"
            || marker_root_conflict(&marker, &root)
            || committed_marker_history_conflict(&projection, &marker, self.state.time)
            || parsed
                .commit_records
                .iter()
                .filter(|record| record.status == "committed")
                .any(|record| {
                    has_receipt_conflict(&parsed.receipt_registry, record)
                        || has_idempotency_conflict(&parsed.idempotency_index, record)
                })
            || response.is_some_and(|response| response.envelope_digest != marker.envelope_digest)
        {
            let report = conflict_report(&marker, root);
            persist_recovery_report(&mut projection, &report)?;
            if !allow_recovery && self.cognition != projection {
                return Err(observer_recovery_required());
            }
            self.cognition = projection;
            return Ok(report);
        }

        if marker.status == "prepared" {
            let report = pending_report(&marker, root);
            persist_recovery_report(&mut projection, &report)?;
            if !allow_recovery && self.cognition != projection {
                return Err(observer_recovery_required());
            }
            self.cognition = projection;
            return Ok(report);
        }
        if marker.status == "aborted" {
            // An aborted marker is terminal evidence that the staged action
            // was discarded. It never exposes a receipt, advances the world
            // root, or enters committed projection reconciliation.
            let mut report = pending_report(&marker, trusted_root);
            report.disposition = "aborted".to_string();
            report.reject_reason = marker.abort_reason.clone();
            report.revalidation_count = 0;
            persist_recovery_report(&mut projection, &report)?;
            if !allow_recovery && self.cognition != projection {
                return Err(observer_recovery_required());
            }
            self.cognition = projection;
            return Ok(report);
        }

        // A committed marker is the authority. If the root still points at
        // the parent (the crash window between commit and root projection),
        // repair that projection in the same in-memory transaction.
        let (new_repaired_ids, mut projection_repairs) =
            reconcile_committed_projection(&mut projection)?;
        let journal_repaired = new_repaired_ids.iter().any(|id| {
            matches!(
                id.as_str(),
                "world_receipt_link" | "cognition_turn_completed"
            )
        });
        parsed = serde_json::from_value(projection.clone())
            .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
        if dense_marker {
            validate_cognition_journal_head(&projection["cognition_journal"])?;
        }
        if journal_repaired {
            // Repairing a missing committed journal prefix necessarily changes
            // the durable head. Rebind each response's journal cursor to the
            // repaired head before strict replay validation below.
            let repaired_head = parsed.cognition_journal.head_digest.clone();
            for response in &mut parsed.responses {
                response.journal_head = repaired_head.clone();
            }
            response_journal_head = Some(repaired_head);
            projection = serde_json::to_value(&parsed).map_err(WorldError::from)?;
        }
        for committed_marker in parsed
            .commit_records
            .iter()
            .filter(|record| record.status == "committed" && !record.agent_id.is_empty())
        {
            let response = parsed
                .responses
                .iter()
                .find(|response| response.envelope_digest == committed_marker.envelope_digest)
                .ok_or_else(|| cognition_validation("response_missing"))?;
            validate_response_record(&projection, response)?;
            validate_response_lineage_binding(response, committed_marker)?;
        }
        let receipt = CognitionReceiptViewV1 {
            receipt_id: marker.receipt_id.clone(),
            receipt_digest: marker.receipt_digest.clone(),
        };
        let mut repaired_ids = recovery.repaired_projection_ids;
        for repaired_id in new_repaired_ids {
            record_repair(&mut repaired_ids, &repaired_id);
        }
        let mut canonical_root = root;
        if !dense_marker
            && (canonical_root.state_root == marker.parent_world_hash
                || canonical_root.commit_id.is_none()
                || canonical_root.head_status != "canonical"
                || canonical_root.quarantine_id.is_some())
        {
            canonical_root.logical_tick = marker.parent_tick.saturating_add(1);
            canonical_root.state_root = marker.staged_state_root.clone();
            canonical_root.head_status = "canonical".to_string();
            canonical_root.commit_id = Some(marker.commit_id.clone());
            canonical_root.quarantine_id = None;
            record_repair(&mut repaired_ids, "world_root");
        }
        let repaired_count = repaired_ids
            .iter()
            .filter(|id| {
                matches!(
                    id.as_str(),
                    "receipt_registry" | "idempotency_index" | "world_receipt_link"
                )
            })
            .count() as u64;
        if projection_repairs == 0 && repaired_count > 0 {
            // The durable repair markers make a second recovery exactly
            // idempotent while still requiring the projections above to be
            // structurally present and marker-bound.
            projection_repairs = repaired_count;
        }

        let replacing_legacy_report = projection
            .get("recovery")
            .and_then(JsonValue::as_object)
            .is_some_and(|recovery| {
                recovery.get("disposition").and_then(JsonValue::as_str) == Some("rejected")
                    && recovery.get("reject_reason").and_then(JsonValue::as_str)
                        == Some("legacy_no_cognition_proof")
            });
        let journal_head =
            response_journal_head.unwrap_or_else(|| parsed.cognition_journal.head_digest.clone());
        let report = CognitionRecoveryReport {
            world_root: Some(canonical_root.clone()),
            receipt: Some(receipt.clone()),
            disposition: "committed".to_string(),
            reject_reason: None,
            auto_submitted: false,
            idempotency_key: Some(marker.envelope_idempotency_key.clone()),
            quarantine_id: None,
            candidate_root: None,
            candidate_receipt: None,
            journal_head,
            retry_count: 0,
            revalidation_count: 0,
            projection_repairs,
            provider_invocation_count: 0,
            kernel_invocation_count: 0,
            effect_count: 0,
            debit_count: 0,
            world_receipt_linked_count: 1,
            event_count: 1,
            response_replayed,
        };
        // A committed marker can be restored from a snapshot whose additive
        // recovery field still carries the legacy default. Persist the
        // verified outcome even when no projection repair was required, while
        // leaving an existing crash-prefix report stable on replay.
        if replacing_legacy_report {
            persist_recovery_report(&mut projection, &report)?;
        }
        if !repaired_ids.is_empty() {
            let object = projection.as_object_mut().ok_or_else(|| {
                WorldError::DistributedValidationFailed {
                    reason: "cognition projection must be an object".to_string(),
                }
            })?;
            object
                .entry("recovery")
                .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));
            if let Some(recovery) = object
                .get_mut("recovery")
                .and_then(JsonValue::as_object_mut)
            {
                if repaired_ids.iter().any(|id| id == "world_root") {
                    recovery.insert("world_root".to_string(), json!(&canonical_root));
                }
                recovery.insert("repaired_projection_ids".to_string(), json!(repaired_ids));
            }
            if !allow_recovery && self.cognition != projection {
                return Err(observer_recovery_required());
            }
            self.cognition = projection;
        } else if feedback_projection_changed || replacing_legacy_report {
            if !allow_recovery && self.cognition != projection {
                return Err(observer_recovery_required());
            }
            self.cognition = projection;
        }

        Ok(report)
    }
}

fn observer_recovery_required() -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: "RecoveryRequired: cognition projection needs writer repair".into(),
    }
}
