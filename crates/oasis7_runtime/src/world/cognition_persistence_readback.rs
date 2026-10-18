use super::super::cognition_persistence_validation::{
    cognition_error, cognition_validation, validate_cognition_journal_head,
    validate_cognition_journal_integrity, validate_response_lineage_binding,
    validate_response_record,
};
use super::cognition_persistence_support::{validate_lineage_binding, validate_marker};
use super::{CognitionProjection, World};
use crate::cognition_recovery::{
    CognitionResponseRecordV1, RuntimeReceiptLineageV1, WorldCommitRecordV1, cognition_digest_v1,
    default_cognition_persistence_projection,
};
use crate::error::WorldError;
use serde_json::Value as JsonValue;

impl World {
    /// Runtime readback verifier for the opaque receipt projection consumed
    /// by simulator adapters.
    pub fn verify_runtime_receipt_lineage(
        &self,
        lineage: &RuntimeReceiptLineageV1,
    ) -> Result<(), WorldError> {
        let readback = self.read_runtime_receipt_lineage(&lineage.receipt_id)?;
        if &readback == lineage {
            Ok(())
        } else {
            Err(cognition_validation("receipt_lineage_readback_mismatch"))
        }
    }

    /// Read and validate a canonical response artifact without re-invoking a
    /// provider. Missing artifacts, digest mismatches, and stale journal
    /// heads are rejected.
    pub fn replay_cognition_response(
        &self,
        envelope_digest: &str,
    ) -> Result<CognitionResponseRecordV1, WorldError> {
        let projection: CognitionProjection = serde_json::from_value(self.cognition.clone())
            .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
        // Validate journal digests against the stored canonical JSON.  A typed
        // round-trip is not digest-preserving here: optional dense fields are
        // represented as `null` in the append projection, while the typed
        // compatibility DTO skips absent Option values during serialization.
        // Re-serializing it would therefore change the signed payload without
        // any durable mutation.
        let projection_value = self.cognition.clone();
        let journal = projection_value
            .get("cognition_journal")
            .ok_or_else(|| cognition_validation("cognition_journal_missing"))?;
        validate_cognition_journal_integrity(journal)?;
        validate_cognition_journal_head(journal)?;
        let response = projection
            .responses
            .iter()
            .find(|response| response.envelope_digest == envelope_digest)
            .ok_or_else(|| cognition_validation("response_missing"))?;
        validate_response_record(&projection_value, response)?;
        let marker = projection
            .commit_records
            .iter()
            .find(|marker| {
                marker.status == "committed" && marker.envelope_digest == envelope_digest
            })
            .ok_or_else(|| cognition_validation("response_commit_marker_missing"))?;
        validate_marker(marker)?;
        validate_response_lineage_binding(response, marker)?;
        Ok(response.clone())
    }

    pub fn record_cognition_commit(
        &mut self,
        marker: WorldCommitRecordV1,
    ) -> Result<(), WorldError> {
        let _ = marker;
        Err(cognition_validation("legacy_commit_projection_fenced"))
    }

    pub fn project_runtime_receipt_lineage(
        &mut self,
        lineage: RuntimeReceiptLineageV1,
    ) -> Result<(), WorldError> {
        lineage
            .validate()
            .map_err(|error| cognition_validation(error.code()))?;
        let projection = if self.cognition.is_null() {
            default_cognition_persistence_projection()
        } else {
            self.cognition.clone()
        };
        let mut parsed: CognitionProjection = serde_json::from_value(projection.clone())
            .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
        let marker = parsed
            .commit_records
            .iter()
            .find(|marker| marker.status == "committed" && marker.receipt_id == lineage.receipt_id)
            .ok_or_else(|| cognition_validation("receipt_commit_marker_missing"))?;
        if RuntimeReceiptLineageV1::from_durable_commit_record(marker).is_none() {
            return Err(cognition_validation(
                "receipt_lineage_dense_marker_required",
            ));
        }
        validate_lineage_binding(&parsed, marker, &lineage)?;
        let derived = RuntimeReceiptLineageV1::from_durable_commit_record(marker)
            .expect("dense marker checked above");
        if derived != lineage {
            return Err(cognition_validation("receipt_lineage_caller_override"));
        }
        if let Some(existing) = parsed
            .receipt_lineage_registry
            .iter()
            .find(|existing| existing.receipt_id == lineage.receipt_id)
        {
            if existing == &lineage {
                return Ok(());
            }
            return Err(cognition_validation("receipt_lineage_conflict"));
        }
        parsed.receipt_lineage_registry.push(lineage);
        let mut projection = projection;
        projection
            .as_object_mut()
            .ok_or_else(|| cognition_validation("cognition_projection_not_object"))?
            .insert(
                "receipt_lineage_registry".to_string(),
                serde_json::to_value(parsed.receipt_lineage_registry).map_err(WorldError::from)?,
            );
        self.cognition = projection;
        Ok(())
    }

    pub fn read_runtime_receipt_lineage(
        &self,
        receipt_id: &str,
    ) -> Result<RuntimeReceiptLineageV1, WorldError> {
        let projection = if self.cognition.is_null() {
            default_cognition_persistence_projection()
        } else {
            self.cognition.clone()
        };
        let parsed: CognitionProjection = serde_json::from_value(projection)
            .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
        let lineage = parsed
            .receipt_lineage_registry
            .iter()
            .find(|lineage| lineage.receipt_id == receipt_id)
            .ok_or_else(|| cognition_validation("receipt_lineage_missing"))?;
        lineage
            .validate()
            .map_err(|error| cognition_validation(error.code()))?;
        let marker = parsed
            .commit_records
            .iter()
            .find(|marker| marker.status == "committed" && marker.receipt_id == receipt_id)
            .ok_or_else(|| cognition_validation("receipt_commit_marker_missing"))?;
        validate_marker(marker)?;
        if let Some(response) = parsed
            .responses
            .iter()
            .find(|response| response.envelope_digest == marker.envelope_digest)
        {
            validate_response_lineage_binding(response, marker)?;
        }
        validate_lineage_binding(&parsed, marker, lineage)?;
        let derived = RuntimeReceiptLineageV1::from_durable_commit_record(marker)
            .ok_or_else(|| cognition_validation("receipt_lineage_dense_marker_required"))?;
        if derived != *lineage {
            return Err(cognition_validation("receipt_lineage_binding_mismatch"));
        }
        Ok(lineage.clone())
    }

    pub(in crate::world) fn cognition_committed_evidence(
        &self,
    ) -> Result<
        (
            std::collections::BTreeSet<String>,
            std::collections::BTreeSet<String>,
        ),
        WorldError,
    > {
        let projection = if self.cognition.is_null() {
            default_cognition_persistence_projection()
        } else {
            self.cognition.clone()
        };
        let parsed: CognitionProjection = serde_json::from_value(projection.clone())
            .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
        let mut committed_receipts = std::collections::BTreeSet::new();
        for marker in parsed
            .commit_records
            .iter()
            .filter(|marker| marker.status == "committed")
        {
            validate_marker(marker)?;
            if parsed.receipt_registry.iter().any(|receipt| {
                receipt.receipt_id == marker.receipt_id
                    && receipt.receipt_digest == marker.receipt_digest
            }) {
                committed_receipts.insert(marker.receipt_id.clone());
            }
        }

        let mut committed_events = std::collections::BTreeSet::new();
        let events = projection
            .get("cognition_journal")
            .and_then(|journal| journal.get("events"))
            .and_then(JsonValue::as_array)
            .ok_or_else(|| cognition_validation("cognition_events_missing"))?;
        if !events.is_empty() {
            let journal = projection
                .get("cognition_journal")
                .ok_or_else(|| cognition_validation("cognition_journal_missing"))?;
            // Wake evidence is authoritative only after the same dense
            // journal and head checks used by recovery/replay. A forged
            // event that happens to carry a self-consistent digest must not
            // satisfy a live wake by bypassing those checks.
            validate_cognition_journal_integrity(journal)?;
            validate_cognition_journal_head(journal)?;
        }
        for event in events {
            let Some(event_digest) = event.get("event_digest").and_then(JsonValue::as_str) else {
                continue;
            };
            let mut unsigned = event.clone();
            let Some(object) = unsigned.as_object_mut() else {
                return Err(cognition_validation("cognition_event_not_object"));
            };
            object.remove("event_digest");
            let expected = cognition_digest_v1("oasis7.cognition.event.v1", &unsigned);
            if expected != event_digest {
                return Err(cognition_validation("cognition_event_digest_mismatch"));
            }
            committed_events.insert(event_digest.to_string());
        }
        Ok((committed_events, committed_receipts))
    }
}
