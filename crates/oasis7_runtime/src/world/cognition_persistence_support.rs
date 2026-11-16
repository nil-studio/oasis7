//! Small typed helpers for World cognition projection recovery.
//!
//! Keeping repair and journal canonicalization separate from the transaction
//! methods makes the authority boundary easier to audit without changing the
//! persisted projection shape.

use super::super::cognition_persistence_validation::{
    cognition_error, cognition_event_payload_projection, cognition_validation,
};
use super::World;
use super::{
    CognitionJournalEvent, CognitionJournalProjection, CognitionProjection, CognitionReceiptViewV1,
    IdempotencyProjection, WORLD_COMMIT_SCHEMA, append_cognition_event,
};
use crate::cognition::{
    AgentDecisionEnvelopeV1, finality_binding_digest_v1, finality_binding_is_legal,
    world_state_binding_digest_v1,
};
use crate::cognition_recovery::{
    RuntimeCognitionCommitRequestV1, RuntimeCognitionResponseArtifactV1, RuntimeReceiptLineageV1,
    WorldCommitRecordV1, WorldRootViewV1, cognition_digest_v1,
    default_cognition_persistence_projection, response_artifact_digest,
};
use crate::cognition_scheduler::SchedulerWakeV1;
use crate::error::WorldError;
use serde_json::{Value as JsonValue, json};
use std::collections::BTreeMap;

pub(super) fn receipt_digest_for_marker(marker: &WorldCommitRecordV1) -> String {
    receipt_digest_for_marker_with_work_v1(
        marker,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .expect("original receipt identity is serializable")
    .as_str()
    .to_owned()
}
pub(in crate::world) fn receipt_digest_for_marker_with_work_v1<
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
>(
    marker: &WorldCommitRecordV1,
    source: &A,
) -> Result<
    oasis7_agent_api::world_cognition::PaidCognitionDigestDataV1<A::Charge>,
    oasis7_wasm_abi::CanonicalCborStreamError,
> {
    let mut n = 8192usize;
    for value in [
        &marker.receipt_id,
        &marker.envelope_digest,
        &marker.action_id,
        &marker.parent_world_hash,
        &marker.response_artifact_digest,
    ] {
        for part in value.as_bytes().chunks(8192) {
            source.charge_work(1, part.len() as u64)?;
        }
        n = n.checked_add(value.len()).ok_or(
            oasis7_wasm_abi::CanonicalCborStreamError::ResourceLimited("receipt preimage layout"),
        )?;
    }
    source.charge_work(32, n as u64)?;
    let _phase = source.reserve(n)?;
    let mut receipt = json!({
        "receipt_id": marker.receipt_id,
        "envelope_digest": marker.envelope_digest,
        "action_id": marker.action_id,
        "world_root": marker.parent_world_hash,
    });
    if !marker.response_artifact_digest.is_empty() {
        receipt["response_artifact_digest"] = json!(marker.response_artifact_digest);
    }
    oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(
        "oasis7.cognition.receipt.v1",
        &receipt,
        source,
    )
}

pub(super) fn validate_prepare_response_artifact(
    envelope: &AgentDecisionEnvelopeV1,
    artifact: &JsonValue,
) -> Result<RuntimeCognitionResponseArtifactV1, WorldError> {
    let identity: RuntimeCognitionResponseArtifactV1 = serde_json::from_value(artifact.clone())
        .map_err(|_| cognition_validation("response_artifact_identity_invalid"))?;
    identity
        .validate()
        .map_err(|_| cognition_validation("response_artifact_identity_invalid"))?;
    let outer = artifact
        .get("outer_lineage")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| cognition_validation("response_outer_lineage_missing"))?;
    let expected_base_world_hash = if envelope.base_world_hash.starts_with("blake3:") {
        envelope.base_world_hash.clone()
    } else {
        world_state_binding_digest_v1(
            &envelope.world_id,
            &envelope.branch_id,
            envelope.finality_epoch,
            envelope.finality_block_hash.as_deref(),
            &envelope.finality_status,
            envelope.base_tick,
            &envelope.base_world_hash,
            envelope.reorg_epoch,
            &envelope.runtime_manifest_hash,
        )
    };
    let expected_manifest_hash = if envelope.runtime_manifest_hash.starts_with("blake3:") {
        envelope.runtime_manifest_hash.clone()
    } else {
        cognition_digest_v1(
            "oasis7.runtime.manifest.v1",
            &envelope.runtime_manifest_hash,
        )
    };
    let outer_block_hash = outer
        .get("finality_block_hash")
        .and_then(JsonValue::as_str)
        .map(str::to_string);
    if outer.get("agent_id").and_then(JsonValue::as_str) != Some(envelope.agent_id.as_str())
        || outer.get("world_id").and_then(JsonValue::as_str) != Some(envelope.world_id.as_str())
        || outer.get("branch_id").and_then(JsonValue::as_str) != Some(envelope.branch_id.as_str())
        || outer.get("finality_epoch").and_then(JsonValue::as_u64) != Some(envelope.finality_epoch)
        || outer_block_hash != envelope.finality_block_hash
        || outer.get("finality_status").and_then(JsonValue::as_str)
            != Some(envelope.finality_status.as_str())
        || outer.get("base_tick").and_then(JsonValue::as_u64) != Some(envelope.base_tick)
        || outer.get("base_world_hash").and_then(JsonValue::as_str)
            != Some(expected_base_world_hash.as_str())
        || outer.get("reorg_epoch").and_then(JsonValue::as_u64) != Some(envelope.reorg_epoch)
        || outer
            .get("runtime_manifest_hash")
            .and_then(JsonValue::as_str)
            != Some(expected_manifest_hash.as_str())
    {
        return Err(cognition_validation("response_outer_lineage_mismatch"));
    }
    if identity.agent_session_id != envelope.agent_session_id
        || identity.agent_turn_id != envelope.agent_turn_id
        || identity.decision_request_id != envelope.decision_request_id
        || identity.request_digest != envelope.request_digest
    {
        return Err(cognition_validation("response_artifact_lineage_mismatch"));
    }
    Ok(identity)
}

fn response_artifact_with_outer_lineage(
    artifact: &RuntimeCognitionResponseArtifactV1,
    request: &RuntimeCognitionCommitRequestV1,
) -> Result<JsonValue, WorldError> {
    response_artifact_with_outer_lineage_with_work_v1(
        artifact,
        request,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .expect("legacy response lineage work is infallible")
    .map(|(value, _)| value)
    .map_err(|(error, _)| WorldError::from(error))
}
pub(in crate::world) fn response_artifact_with_outer_lineage_with_work_v1<
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
>(
    artifact: &RuntimeCognitionResponseArtifactV1,
    request: &RuntimeCognitionCommitRequestV1,
    source: &A,
) -> Result<
    Result<(JsonValue, A::Charge), (serde_json::Error, A::Charge)>,
    oasis7_wasm_abi::CanonicalCborStreamError,
> {
    let mut layout = 65536usize;
    for text in [
        &artifact.context_discriminator,
        &artifact.agent_session_id,
        &artifact.agent_turn_id,
        &artifact.decision_request_id,
        &artifact.request_digest,
        &artifact.response_digest,
        &artifact.artifact_digest,
        &request.agent_id,
        &request.captured_base_binding.world_id,
        &request.captured_base_binding.branch_id,
        &request.captured_base_binding.finality_status,
        &request.captured_base_binding.base_world_hash,
        &request.captured_base_binding.runtime_manifest_hash,
    ]
    .into_iter()
    .map(Some)
    .chain([request.captured_base_binding.finality_block_hash.as_ref()])
    {
        source.charge_work(1, 0)?;
        if let Some(text) = text {
            for part in text.as_bytes().chunks(8192) {
                source.charge_work(1, part.len() as u64)?;
            }
            layout = layout
                .checked_add(text.len().checked_mul(2).ok_or(
                    oasis7_wasm_abi::CanonicalCborStreamError::ResourceLimited(
                        "response lineage layout",
                    ),
                )?)
                .ok_or(oasis7_wasm_abi::CanonicalCborStreamError::ResourceLimited(
                    "response lineage layout",
                ))?;
        }
    }
    source.charge_work(128, layout as u64)?;
    let charge = source.reserve(layout)?;
    let mut value = match serde_json::to_value(artifact) {
        Ok(value) => value,
        Err(error) => return Ok(Err((error, charge))),
    };
    value["outer_lineage"] = json!({
        "agent_id": request.agent_id,
        "world_id": request.captured_base_binding.world_id,
        "branch_id": request.captured_base_binding.branch_id,
        "finality_epoch": request.captured_base_binding.finality_epoch,
        "finality_block_hash": request.captured_base_binding.finality_block_hash,
        "finality_status": request.captured_base_binding.finality_status,
        "base_tick": request.captured_base_binding.base_tick,
        "base_world_hash": request.captured_base_binding.base_world_hash,
        "reorg_epoch": request.captured_base_binding.reorg_epoch,
        "runtime_manifest_hash": request.captured_base_binding.runtime_manifest_hash,
    });
    Ok(Ok((value, charge)))
}

impl World {
    fn bind_cognition_response_artifact(
        &mut self,
        commit_id: &str,
        response_artifact: &RuntimeCognitionResponseArtifactV1,
    ) -> Result<(), WorldError> {
        let mut projection = self.cognition.clone();
        let envelope_digest = projection
            .get("commit_records")
            .and_then(JsonValue::as_array)
            .and_then(|records| {
                records.iter().find(|record| {
                    record.get("commit_id").and_then(JsonValue::as_str) == Some(commit_id)
                })
            })
            .and_then(|record| record.get("envelope_digest"))
            .and_then(JsonValue::as_str)
            .ok_or_else(|| cognition_validation("envelope_digest_missing"))?
            .to_string();
        let response_digest = projection
            .get("responses")
            .and_then(JsonValue::as_array)
            .and_then(|responses| {
                responses.iter().find(|response| {
                    response.get("envelope_digest").and_then(JsonValue::as_str)
                        == Some(envelope_digest.as_str())
                })
            })
            .and_then(|response| response.get("response_artifact"))
            .map(response_artifact_digest)
            .unwrap_or_else(|| {
                response_artifact_digest(
                    &serde_json::to_value(response_artifact).unwrap_or(JsonValue::Null),
                )
            });
        let record = projection
            .get_mut("commit_records")
            .and_then(JsonValue::as_array_mut)
            .and_then(|records| {
                records.iter_mut().find(|record| {
                    record.get("commit_id").and_then(JsonValue::as_str) == Some(commit_id)
                })
            })
            .ok_or_else(|| cognition_validation("commit_record_missing"))?;
        record["response_context_discriminator"] = json!(response_artifact.context_discriminator);
        record["response_context_version"] = json!(response_artifact.context_version);
        record["response_retry_seq"] = json!(response_artifact.retry_seq);
        record["transport_attempt"] = json!(response_artifact.transport_attempt);
        record["response_artifact_digest"] = json!(response_digest);
        let receipt_id = record["receipt_id"]
            .as_str()
            .ok_or_else(|| cognition_validation("receipt_id_missing"))?
            .to_string();
        let envelope_digest = record["envelope_digest"]
            .as_str()
            .ok_or_else(|| cognition_validation("envelope_digest_missing"))?
            .to_string();
        let action_id = record["action_id"]
            .as_str()
            .ok_or_else(|| cognition_validation("action_id_missing"))?
            .to_string();
        let world_root = record["parent_world_hash"]
            .as_str()
            .ok_or_else(|| cognition_validation("parent_world_hash_missing"))?
            .to_string();
        record["receipt_digest"] = json!(cognition_digest_v1(
            "oasis7.cognition.receipt.v1",
            &json!({
                "receipt_id": receipt_id,
                "envelope_digest": envelope_digest,
                "action_id": action_id,
                "world_root": world_root,
                "response_artifact_digest": response_digest,
            })
        ));
        self.cognition = projection;
        Ok(())
    }
}

#[path = "cognition_persistence_world_commit.rs"]
mod world_commit;

pub(super) fn reconcile_committed_projection(
    projection: &mut JsonValue,
) -> Result<(Vec<String>, u64), WorldError> {
    let mut parsed: CognitionProjection = serde_json::from_value(projection.clone())
        .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
    let markers: Vec<WorldCommitRecordV1> = parsed
        .commit_records
        .iter()
        .filter(|marker| marker.status == "committed")
        .cloned()
        .collect();
    let mut repaired_ids = Vec::new();
    let mut repairs = 0u64;
    let mut journal_repaired = false;

    for marker in markers {
        let receipt_present = parsed.receipt_registry.iter().any(|receipt| {
            receipt.receipt_id == marker.receipt_id
                && receipt.receipt_digest == marker.receipt_digest
        });
        if !receipt_present {
            parsed.receipt_registry.push(CognitionReceiptViewV1 {
                receipt_id: marker.receipt_id.clone(),
                receipt_digest: marker.receipt_digest.clone(),
            });
            record_repair(&mut repaired_ids, "receipt_registry");
            repairs = repairs.saturating_add(1);
        }

        let idempotency_present = parsed
            .idempotency_index
            .get(&marker.envelope_idempotency_key)
            .is_some_and(|entry| {
                entry.envelope_digest == marker.envelope_digest
                    && entry.disposition == "committed"
                    && entry.receipt_id == marker.receipt_id
            });
        if !idempotency_present {
            parsed.idempotency_index.insert(
                marker.envelope_idempotency_key.clone(),
                IdempotencyProjection {
                    envelope_digest: marker.envelope_digest.clone(),
                    disposition: "committed".to_string(),
                    receipt_id: marker.receipt_id.clone(),
                },
            );
            record_repair(&mut repaired_ids, "idempotency_index");
            repairs = repairs.saturating_add(1);
        }

        let linked_present = parsed.cognition_journal.events.iter().any(|event| {
            event.kind == "WorldReceiptLinked"
                && event.envelope_idempotency_key == marker.envelope_idempotency_key
                && (event.receipt_id.as_deref() == Some(marker.receipt_id.as_str())
                    // Sparse markers are the backwards-compatible projection
                    // shape used by pre-lineage snapshots. Their link event
                    // predates receipt identity being persisted, and the
                    // validator intentionally treats that absent field as
                    // compatible. Do not synthesize a dense event during a
                    // read/repair merely because the legacy event is sparse.
                    || (event.receipt_id.is_none() && marker.agent_id.is_empty()))
        });
        if !linked_present {
            let next_seq = parsed
                .cognition_journal
                .events
                .iter()
                .map(|event| event.journal_seq)
                .max()
                .unwrap_or_default()
                .saturating_add(1);
            parsed.cognition_journal.events.push(CognitionJournalEvent {
                journal_seq: next_seq,
                kind: "WorldReceiptLinked".to_string(),
                envelope_idempotency_key: marker.envelope_idempotency_key.clone(),
                receipt_id: Some(marker.receipt_id.clone()),
                agent_id: (!marker.agent_id.is_empty()).then(|| marker.agent_id.clone()),
                agent_session_id: (!marker.agent_session_id.is_empty())
                    .then(|| marker.agent_session_id.clone()),
                agent_turn_id: (!marker.agent_turn_id.is_empty())
                    .then(|| marker.agent_turn_id.clone()),
                decision_request_id: (!marker.decision_request_id.is_empty())
                    .then(|| marker.decision_request_id.clone()),
                request_digest: (!marker.request_digest.is_empty())
                    .then(|| marker.request_digest.clone()),
                feedback_id: (!marker.feedback_id.is_empty()).then(|| marker.feedback_id.clone()),
                ..CognitionJournalEvent::default()
            });
            parsed.cognition_journal.head_seq = next_seq;
            record_repair(&mut repaired_ids, "world_receipt_link");
            repairs = repairs.saturating_add(1);
            journal_repaired = true;
        }

        let completed_present = parsed.cognition_journal.events.iter().any(|event| {
            event.kind == "CognitionTurnCompleted"
                && event.envelope_idempotency_key == marker.envelope_idempotency_key
                && (event.receipt_id.as_deref() == Some(marker.receipt_id.as_str())
                    || (event.receipt_id.is_none() && marker.agent_id.is_empty()))
                && event.status == "committed"
        });
        if !completed_present {
            let next_seq = parsed
                .cognition_journal
                .events
                .iter()
                .map(|event| event.journal_seq)
                .max()
                .unwrap_or_default()
                .saturating_add(1);
            parsed.cognition_journal.events.push(CognitionJournalEvent {
                journal_seq: next_seq,
                kind: "CognitionTurnCompleted".to_string(),
                envelope_idempotency_key: marker.envelope_idempotency_key.clone(),
                receipt_id: Some(marker.receipt_id.clone()),
                agent_id: (!marker.agent_id.is_empty()).then(|| marker.agent_id.clone()),
                agent_session_id: (!marker.agent_session_id.is_empty())
                    .then(|| marker.agent_session_id.clone()),
                agent_turn_id: (!marker.agent_turn_id.is_empty())
                    .then(|| marker.agent_turn_id.clone()),
                decision_request_id: (!marker.decision_request_id.is_empty())
                    .then(|| marker.decision_request_id.clone()),
                request_digest: (!marker.request_digest.is_empty())
                    .then(|| marker.request_digest.clone()),
                feedback_id: (!marker.feedback_id.is_empty()).then(|| marker.feedback_id.clone()),
                status: "committed".to_string(),
                ..CognitionJournalEvent::default()
            });
            parsed.cognition_journal.head_seq = next_seq;
            record_repair(&mut repaired_ids, "cognition_turn_completed");
            repairs = repairs.saturating_add(1);
            journal_repaired = true;
        }

        if let Some(derived) = RuntimeReceiptLineageV1::from_durable_commit_record(&marker) {
            if let Some(existing) = parsed
                .receipt_lineage_registry
                .iter()
                .find(|lineage| lineage.receipt_id == marker.receipt_id)
            {
                if existing != &derived {
                    return Err(cognition_validation("receipt_lineage_binding_mismatch"));
                }
            } else {
                parsed.receipt_lineage_registry.push(derived);
                record_repair(&mut repaired_ids, "receipt_lineage_registry");
                repairs = repairs.saturating_add(1);
            }
        }
    }

    if repairs > 0 {
        // Registry/idempotency repairs are projection-only recovery. The
        // journal is the durable event source and must remain byte-for-byte
        // stable unless recovery actually appends a missing lifecycle event.
        if journal_repaired {
            refresh_cognition_journal(&mut parsed.cognition_journal);
            *projection = serde_json::to_value(parsed).map_err(WorldError::from)?;
        } else {
            let object = projection
                .as_object_mut()
                .ok_or_else(|| cognition_validation("cognition_projection_not_object"))?;
            object.insert(
                "receipt_registry".to_string(),
                serde_json::to_value(parsed.receipt_registry).map_err(WorldError::from)?,
            );
            object.insert(
                "idempotency_index".to_string(),
                serde_json::to_value(parsed.idempotency_index).map_err(WorldError::from)?,
            );
            object.insert(
                "receipt_lineage_registry".to_string(),
                serde_json::to_value(parsed.receipt_lineage_registry).map_err(WorldError::from)?,
            );
        }
    }
    Ok((repaired_ids, repairs))
}

pub(super) fn stale_base_error() -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: "stale_base".to_string(),
    }
}

fn refresh_cognition_journal(journal: &mut CognitionJournalProjection) {
    let mut previous_event_digest = String::new();
    for event in &mut journal.events {
        if event.schema_version.is_empty() {
            event.schema_version = "cognition-journal-event.v1".to_string();
        }
        if event.event_kind.is_empty() {
            event.event_kind = event.kind.clone();
        }
        if event.kind.is_empty() {
            event.kind = event.event_kind.clone();
        }
        if event.world_id.is_empty() {
            event.world_id = "unbound".to_string();
        }
        if event.branch_id.is_empty() {
            event.branch_id = "main".to_string();
        }
        if event.finality_status.is_empty() {
            event.finality_status = "pending".to_string();
        }
        if event.status.is_empty() {
            event.status = event
                .extra
                .get("status")
                .and_then(JsonValue::as_str)
                .unwrap_or("pending")
                .to_string();
        }
        if event.payload_digest.is_empty() {
            let payload = cognition_event_payload_projection(
                &serde_json::to_value(&*event).unwrap_or(JsonValue::Null),
            );
            event.payload_digest =
                cognition_digest_v1("oasis7.cognition.event-payload.v1", &payload);
        }
        if event.parent_event_digest.is_empty() && event.journal_seq > 1 {
            event.parent_event_digest = previous_event_digest.clone();
        }
        event.event_digest = None;
        let unsigned = serde_json::to_value(&*event).unwrap_or(JsonValue::Null);
        let digest = cognition_digest_v1("oasis7.cognition.event.v1", &unsigned);
        event.event_digest = Some(digest.clone());
        previous_event_digest = digest;
    }
    journal.head_seq = journal.head_seq.max(
        journal
            .events
            .iter()
            .map(|event| event.journal_seq)
            .max()
            .unwrap_or_default(),
    );
    let events = serde_json::to_value(&journal.events).unwrap_or(JsonValue::Array(Vec::new()));
    journal.head_digest = cognition_digest_v1(
        "oasis7.cognition.journal-head.v1",
        &json!({"head_seq": journal.head_seq, "events": events}),
    );
}

pub(super) fn record_repair(repaired_ids: &mut Vec<String>, id: &str) {
    if !repaired_ids.iter().any(|existing| existing == id) {
        repaired_ids.push(id.to_string());
    }
}

pub(super) fn validate_lineage_binding(
    projection: &CognitionProjection,
    marker: &WorldCommitRecordV1,
    lineage: &RuntimeReceiptLineageV1,
) -> Result<(), WorldError> {
    match validate_lineage_binding_with_work_v1(
        projection,
        marker,
        lineage,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    ) {
        Ok(()) => Ok(()),
        Err(super::cognition_persistence_readback::feedback::ReceiptReadErrorV1::World(e)) => {
            Err(e)
        }
        Err(super::cognition_persistence_readback::feedback::ReceiptReadErrorV1::Work(_)) => {
            unreachable!("legacy lineage binding work cannot fail")
        }
    }
}
pub(in crate::world) fn validate_lineage_binding_with_work_v1<
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
>(
    projection: &CognitionProjection,
    marker: &WorldCommitRecordV1,
    lineage: &RuntimeReceiptLineageV1,
    source: &A,
) -> Result<(), super::cognition_persistence_readback::feedback::ReceiptReadErrorV1> {
    fn eq<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        a: &str,
        b: &str,
        source: &A,
    ) -> Result<bool, oasis7_wasm_abi::CanonicalCborStreamError> {
        source.charge_work(1, (a.len() + b.len()) as u64)?;
        Ok(a == b)
    }
    fn optional<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        a: Option<&str>,
        b: &str,
        permissive: bool,
        source: &A,
    ) -> Result<bool, oasis7_wasm_abi::CanonicalCborStreamError> {
        match a {
            None => Ok(permissive),
            Some(a) => eq(a, b, source),
        }
    }
    let first_bad = !eq(&marker.receipt_digest, &lineage.receipt_digest, source)?
        || !eq(&marker.envelope_digest, &lineage.envelope_digest, source)?
        || !eq(&marker.action_id, &lineage.action_id, source)?;
    let fail = || cognition_validation("receipt_lineage_binding_mismatch");
    if first_bad {
        return Err(fail().into());
    }
    let mut receipt_found = false;
    for receipt in &projection.receipt_registry {
        if eq(&receipt.receipt_id, &marker.receipt_id, source)?
            && eq(&receipt.receipt_digest, &marker.receipt_digest, source)?
        {
            receipt_found = true;
            break;
        }
    }
    if !receipt_found {
        return Err(fail().into());
    }
    source.charge_work(1, marker.envelope_idempotency_key.len() as u64)?;
    let Some(entry) = projection
        .idempotency_index
        .get(&marker.envelope_idempotency_key)
    else {
        return Err(fail().into());
    };
    if !eq(&entry.envelope_digest, &marker.envelope_digest, source)?
        || !eq(&entry.disposition, "committed", source)?
        || !eq(&entry.receipt_id, &marker.receipt_id, source)?
    {
        return Err(fail().into());
    }
    let mut event_found = false;
    for event in &projection.cognition_journal.events {
        if eq(&event.kind, "WorldReceiptLinked", source)?
            && eq(
                &event.envelope_idempotency_key,
                &marker.envelope_idempotency_key,
                source,
            )?
            && optional(
                event.receipt_id.as_deref(),
                &marker.receipt_id,
                true,
                source,
            )?
            && optional(event.agent_id.as_deref(), &lineage.agent_id, true, source)?
            && optional(
                event.agent_session_id.as_deref(),
                &lineage.agent_session_id,
                true,
                source,
            )?
            && optional(
                event.agent_turn_id.as_deref(),
                &lineage.agent_turn_id,
                true,
                source,
            )?
            && optional(
                event.decision_request_id.as_deref(),
                &lineage.decision_request_id,
                true,
                source,
            )?
            && optional(
                event.request_digest.as_deref(),
                &lineage.request_digest,
                true,
                source,
            )?
            && optional(
                event.feedback_id.as_deref(),
                &lineage.feedback_id,
                true,
                source,
            )?
        {
            event_found = true;
            break;
        }
    }
    if !event_found {
        return Err(fail().into());
    }
    if !marker.agent_id.is_empty() {
        if !eq(&marker.agent_id, &lineage.agent_id, source)?
            || !eq(&marker.agent_session_id, &lineage.agent_session_id, source)?
            || !eq(&marker.agent_turn_id, &lineage.agent_turn_id, source)?
            || !eq(
                &marker.decision_request_id,
                &lineage.decision_request_id,
                source,
            )?
            || !eq(&marker.request_digest, &lineage.request_digest, source)?
            || !eq(&marker.feedback_id, &lineage.feedback_id, source)?
        {
            return Err(fail().into());
        }
    }
    if !marker.agent_id.is_empty() {
        let mut dense_found = false;
        for event in &projection.cognition_journal.events {
            if eq(&event.kind, "WorldReceiptLinked", source)?
                && eq(
                    &event.envelope_idempotency_key,
                    &marker.envelope_idempotency_key,
                    source,
                )?
                && optional(
                    event.receipt_id.as_deref(),
                    &marker.receipt_id,
                    false,
                    source,
                )?
                && optional(event.agent_id.as_deref(), &marker.agent_id, false, source)?
                && optional(
                    event.agent_session_id.as_deref(),
                    &marker.agent_session_id,
                    false,
                    source,
                )?
                && optional(
                    event.agent_turn_id.as_deref(),
                    &marker.agent_turn_id,
                    false,
                    source,
                )?
                && optional(
                    event.decision_request_id.as_deref(),
                    &marker.decision_request_id,
                    false,
                    source,
                )?
                && optional(
                    event.request_digest.as_deref(),
                    &marker.request_digest,
                    false,
                    source,
                )?
                && optional(
                    event.feedback_id.as_deref(),
                    &marker.feedback_id,
                    false,
                    source,
                )?
            {
                dense_found = true;
                break;
            }
        }
        if !dense_found {
            return Err(fail().into());
        }
    }
    Ok(())
}

pub(super) fn select_commit_record(
    records: &[WorldCommitRecordV1],
) -> Option<&WorldCommitRecordV1> {
    records
        .iter()
        .max_by_key(|record| (record.cognition_journal_seq, record.commit_id.as_str()))
}

pub(in crate::world) fn validate_marker(marker: &WorldCommitRecordV1) -> Result<(), WorldError> {
    match validate_marker_with_work_v1(marker, &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1) {
        Ok(()) => Ok(()),
        Err(super::cognition_persistence_readback::feedback::ReceiptReadErrorV1::World(e)) => {
            Err(e)
        }
        Err(super::cognition_persistence_readback::feedback::ReceiptReadErrorV1::Work(_)) => {
            unreachable!("legacy marker work cannot fail")
        }
    }
}
pub(in crate::world) fn validate_marker_with_work_v1<
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
>(
    marker: &WorldCommitRecordV1,
    source: &A,
) -> Result<(), super::cognition_persistence_readback::feedback::ReceiptReadErrorV1> {
    let _phase =
        oasis7_world_state::cognition_feedback_contract::readonly_work::phase(marker, source)?;
    let valid_abort_reason = marker.abort_reason.as_deref().is_some_and(|reason| {
        matches!(
            reason,
            "stale_base"
                | "cancelled"
                | "late_response_after_cancel"
                | "recovery_operator_abort"
                | "reorg_invalidated"
        )
    });
    let abort_shape_valid = match marker.status.as_str() {
        "aborted" => valid_abort_reason,
        "prepared" | "committed" => marker.abort_reason.is_none(),
        _ => false,
    };
    if marker.schema_version != WORLD_COMMIT_SCHEMA
        || marker.commit_id.trim().is_empty()
        || marker.envelope_idempotency_key.trim().is_empty()
        || marker.envelope_digest.trim().is_empty()
        || marker.world_id.trim().is_empty()
        || marker.branch_id.trim().is_empty()
        || marker
            .finality_block_hash
            .as_deref()
            .is_some_and(|hash| hash.trim().is_empty())
        || marker.finality_status.trim().is_empty()
        || marker.finality_binding_digest.trim().is_empty()
        || marker.runtime_manifest_hash.trim().is_empty()
        || marker.action_id.trim().is_empty()
        || marker.parent_world_hash.trim().is_empty()
        || marker.staged_event_root.trim().is_empty()
        || marker.staged_state_root.trim().is_empty()
        || marker.receipt_id.trim().is_empty()
        || marker.receipt_digest.trim().is_empty()
        || !abort_shape_valid
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: "invalid_commit_record".to_string(),
        }
        .into());
    }
    if !finality_binding_is_legal(
        &marker.finality_status,
        marker.finality_block_hash.as_deref(),
    ) {
        return Err(WorldError::DistributedValidationFailed {
            reason: "invalid_commit_record_finality".to_string(),
        }
        .into());
    }
    let dense = [
        &marker.agent_id,
        &marker.agent_session_id,
        &marker.agent_turn_id,
        &marker.decision_request_id,
        &marker.request_digest,
        &marker.feedback_id,
    ];
    if marker.agent_id.is_empty() != dense.iter().all(|value| value.is_empty())
        || (!marker.agent_id.is_empty() && dense.iter().any(|value| value.trim().is_empty()))
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: "invalid_commit_record_identity".to_string(),
        }
        .into());
    }
    if !marker.agent_id.is_empty() {
        let expected_commit_id =
            oasis7_world_state::cognition_feedback_contract::readonly_work::digest(
                "oasis7.cognition.commit-id.v1",
                &json!({
                    "envelope_digest": marker.envelope_digest,
                    "envelope_idempotency_key": marker.envelope_idempotency_key,
                }),
                source,
            )?;
        let expected_receipt_id =
            oasis7_world_state::cognition_feedback_contract::readonly_work::digest(
                "oasis7.cognition.receipt-id.v1",
                &json!({"commit_id": expected_commit_id}),
                source,
            )?;
        let expected_receipt_digest = if marker.response_artifact_digest.is_empty() {
            oasis7_world_state::cognition_feedback_contract::readonly_work::digest(
                "oasis7.cognition.receipt.v1",
                &json!({
                    "receipt_id": expected_receipt_id,
                    "envelope_digest": marker.envelope_digest,
                    "action_id": marker.action_id,
                    "world_root": marker.parent_world_hash,
                }),
                source,
            )?
        } else {
            oasis7_world_state::cognition_feedback_contract::readonly_work::digest(
                "oasis7.cognition.receipt.v1",
                &json!({
                    "receipt_id": expected_receipt_id,
                    "envelope_digest": marker.envelope_digest,
                    "action_id": marker.action_id,
                    "world_root": marker.parent_world_hash,
                    "response_artifact_digest": marker.response_artifact_digest,
                }),
                source,
            )?
        };
        let expected_staged_event_root =
            oasis7_world_state::cognition_feedback_contract::readonly_work::digest(
                "oasis7.cognition.staged-event-root.v1",
                &json!({
                    "world_root": marker.parent_world_hash,
                    "envelope_digest": marker.envelope_digest,
                }),
                source,
            )?;
        let expected_finality_binding = crate::cognition::finality_binding_digest_with_work_v1(
            &marker.branch_id,
            marker.finality_epoch,
            marker.finality_block_hash.as_deref(),
            &marker.finality_status,
            marker.reorg_epoch,
            source,
        )?;
        let expected_feedback_id =
            oasis7_world_state::cognition_feedback_contract::readonly_work::digest(
                "oasis7.cognition.feedback-id.v1",
                &marker.envelope_digest,
                source,
            )?;
        if marker.commit_id != expected_commit_id
            || marker.receipt_id != expected_receipt_id
            || marker.receipt_digest != expected_receipt_digest
            || marker.staged_event_root != expected_staged_event_root
            || marker.finality_binding_digest != expected_finality_binding.as_str()
            || marker.feedback_id != expected_feedback_id
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "invalid_commit_record_digest".to_string(),
            }
            .into());
        }
        if !marker.response_artifact_digest.is_empty()
            && (!canonical_cognition_digest(&marker.response_artifact_digest)
                || marker.response_context_discriminator
                    != RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR
                || marker.response_context_version
                    != RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION
                || marker.response_retry_seq == 0
                || marker.transport_attempt == 0)
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "invalid_response_artifact_binding".to_string(),
            }
            .into());
        }
    }
    Ok(())
}

pub(super) fn marker_root_conflict(marker: &WorldCommitRecordV1, root: &WorldRootViewV1) -> bool {
    if root.world_id != marker.world_id || root.branch_id != marker.branch_id {
        return true;
    }
    // A committed marker is historical evidence after a later ordinary World
    // tick. The current root is intentionally different from the marker's
    // staged root in that case; `committed_marker_history_conflict` verifies
    // the durable receipt-link boundary for this historical branch.
    if marker.status == "committed" && root.logical_tick > marker.parent_tick {
        return false;
    }
    let parent = root.state_root == marker.parent_world_hash
        && root.logical_tick == marker.parent_tick
        && root.commit_id.is_none();
    let next = root.state_root == marker.staged_state_root
        && root.logical_tick == marker.parent_tick.saturating_add(1)
        && root.commit_id.as_deref() == Some(marker.commit_id.as_str());
    let stable = marker.status == "committed"
        && root.state_root == marker.staged_state_root
        && root.logical_tick == marker.parent_tick
        && root.commit_id.as_deref() == Some(marker.commit_id.as_str());
    !parent && !next && !stable
}

/// Verify that a committed marker has a durable receipt-link boundary. Once a
/// later ordinary tick advances the World root, the marker remains valid as
/// historical evidence only when this canonical link still proves its parent
/// and committed state roots. Older projections lack the optional boundary
/// fields; their receipt/link identity remains the compatibility proof.
pub(super) fn committed_marker_history_conflict(
    projection: &JsonValue,
    marker: &WorldCommitRecordV1,
    current_tick: u64,
) -> bool {
    if marker.status != "committed" || current_tick <= marker.parent_tick {
        return false;
    }
    let Some(events) = projection
        .get("cognition_journal")
        .and_then(|journal| journal.get("events"))
        .and_then(JsonValue::as_array)
    else {
        return true;
    };
    let Some(link) = events.iter().find(|event| {
        event.get("event_kind").and_then(JsonValue::as_str) == Some("WorldReceiptLinked")
            && event
                .get("envelope_idempotency_key")
                .and_then(JsonValue::as_str)
                == Some(marker.envelope_idempotency_key.as_str())
            && event.get("receipt_id").and_then(JsonValue::as_str)
                == Some(marker.receipt_id.as_str())
    }) else {
        return true;
    };
    let optional_matches =
        |field: &str, expected: &JsonValue| link.get(field).is_none_or(|value| value == expected);
    !optional_matches("commit_id", &json!(marker.commit_id))
        || !optional_matches("parent_tick", &json!(marker.parent_tick))
        || !optional_matches("parent_world_hash", &json!(marker.parent_world_hash))
        || !optional_matches("staged_state_root", &json!(marker.staged_state_root))
}

pub(super) fn visible_root_conflict(
    marker: &WorldCommitRecordV1,
    visible: &WorldRootViewV1,
    trusted_state_root: &str,
    trusted_tick: u64,
) -> bool {
    visible.world_id != marker.world_id
        || visible.branch_id != marker.branch_id
        || visible.state_root != trusted_state_root
        || visible.logical_tick != trusted_tick
        || visible.head_status != "canonical"
        || (marker.status == "committed"
            && visible.commit_id.as_deref() != Some(marker.commit_id.as_str()))
}

pub(super) fn has_receipt_conflict(
    receipts: &[CognitionReceiptViewV1],
    marker: &WorldCommitRecordV1,
) -> bool {
    receipts.iter().any(|receipt| {
        receipt.receipt_id == marker.receipt_id && receipt.receipt_digest != marker.receipt_digest
    })
}

pub(super) fn has_idempotency_conflict(
    index: &BTreeMap<String, IdempotencyProjection>,
    marker: &WorldCommitRecordV1,
) -> bool {
    index
        .get(&marker.envelope_idempotency_key)
        .is_some_and(|entry| {
            entry.envelope_digest != marker.envelope_digest
                || (entry.disposition == "committed" && entry.receipt_id != marker.receipt_id)
        })
}

pub(super) fn canonical_cognition_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Original retry refusal order. Return borrowed reason DATA so a metered
/// consumer can admit the exact business error graph before allocating it.
pub(in crate::world) fn validate_existing_retry_action_with_work_v1<
    'a,
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
>(
    marker: &'a WorldCommitRecordV1,
    stored_action_digest: Option<&str>,
    action_digest: &str,
    source: &A,
) -> Result<Result<(), &'a str>, oasis7_wasm_abi::CanonicalCborStreamError> {
    source.charge_work(1, marker.status.len() as u64)?;
    if marker.status == "prepared" && stored_action_digest.is_none() {
        return Ok(Err("cognition_staged_action_missing"));
    }
    if let Some(stored) = stored_action_digest {
        source.charge_work(1, (stored.len() + action_digest.len()) as u64)?;
        if stored != action_digest {
            return Ok(Err("envelope_idempotency_conflict"));
        }
    }
    source.charge_work(1, marker.status.len() as u64)?;
    if marker.status == "aborted" {
        return Ok(Err(marker
            .abort_reason
            .as_deref()
            .unwrap_or("cognition_commit_aborted")));
    }
    Ok(Ok(()))
}
fn response_value_comparison_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
    value: &JsonValue,
    source: &A,
) -> Result<(), oasis7_wasm_abi::CanonicalCborStreamError> {
    source.charge_work(1, 0)?;
    match value {
        JsonValue::String(text) => {
            for part in text.as_bytes().chunks(8192) {
                source.charge_work(1, part.len() as u64)?;
            }
        }
        JsonValue::Array(values) => {
            for value in values {
                response_value_comparison_work_v1(value, source)?;
            }
        }
        JsonValue::Object(values) => {
            for (key, value) in values {
                for part in key.as_bytes().chunks(8192) {
                    source.charge_work(1, part.len() as u64)?;
                }
                response_value_comparison_work_v1(value, source)?;
            }
        }
        _ => {}
    }
    Ok(())
}
/// Same response ANY predicate as the original projection Vec, now with work
/// over the real retained graph before its original serde_json equality.
pub(in crate::world) fn response_artifact_matches_with_work_v1<
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
>(
    response: &crate::cognition_recovery::CognitionResponseRecordV1,
    envelope_digest: &str,
    artifact: &JsonValue,
    source: &A,
) -> Result<bool, oasis7_wasm_abi::CanonicalCborStreamError> {
    source.charge_work(
        1,
        (response.envelope_digest.len() + envelope_digest.len()) as u64,
    )?;
    if response.envelope_digest != envelope_digest {
        return Ok(false);
    }
    let Some(actual) = response.response_artifact.as_ref() else {
        return Ok(false);
    };
    if source.is_metered() {
        response_value_comparison_work_v1(actual, source)?;
        response_value_comparison_work_v1(artifact, source)?;
    }
    Ok(actual == artifact)
}
