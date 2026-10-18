use super::World;
use crate::cognition::{finality_binding_is_legal, world_state_binding_digest_v1};
use crate::cognition_recovery::{
    CognitionRecoveryReport, CognitionResponseRecordV1, RuntimeCognitionResponseArtifactV1,
    WorldCommitRecordV1, WorldRootViewV1, cognition_digest_v1, response_artifact_digest,
};
use crate::error::WorldError;
use serde_json::{Value as JsonValue, json};
use std::collections::BTreeMap;

pub(super) fn append_cognition_event(
    projection: &mut JsonValue,
    kind: &str,
    details: JsonValue,
) -> Result<u64, WorldError> {
    const EVENT_SCHEMA: &str = "cognition-journal-event.v1";
    let binding = projection
        .get("runtime_binding")
        .filter(|value| value.is_object())
        .cloned();
    let journal = projection
        .get_mut("cognition_journal")
        .ok_or_else(|| validation("cognition_journal_missing"))?
        .as_object_mut()
        .ok_or_else(|| validation("cognition_journal_not_object"))?;
    let head_seq = journal
        .get("head_seq")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    let next_seq;
    {
        let events = journal
            .entry("events")
            .or_insert_with(|| JsonValue::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| validation("cognition_events_not_array"))?;
        next_seq = head_seq
            .max(
                events
                    .iter()
                    .filter_map(|event| event.get("journal_seq").and_then(JsonValue::as_u64))
                    .max()
                    .unwrap_or_default(),
            )
            .saturating_add(1);
        let mut event = details
            .as_object()
            .cloned()
            .ok_or_else(|| validation("cognition_event_details_not_object"))?;
        let binding_string = |field: &str, default: &str| {
            binding
                .as_ref()
                .and_then(|value| value.get(field))
                .and_then(JsonValue::as_str)
                .unwrap_or(default)
                .to_string()
        };
        let binding_u64 = |field: &str| {
            binding
                .as_ref()
                .and_then(|value| value.get(field))
                .and_then(JsonValue::as_u64)
                .unwrap_or_default()
        };
        let binding_hash = binding
            .as_ref()
            .and_then(|value| value.get("finality_block_hash"))
            .cloned()
            .unwrap_or(JsonValue::Null);
        let previous_event_digest = events
            .last()
            .and_then(|event| event.get("event_digest"))
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        // Keep `kind` for read compatibility with pre-v1 consumers while
        // making the design's explicit `event_kind` part of the signed
        // canonical record.
        event.insert("schema_version".to_string(), json!(EVENT_SCHEMA));
        event.insert("journal_seq".to_string(), json!(next_seq));
        event.insert(
            "parent_event_digest".to_string(),
            json!(previous_event_digest),
        );
        event.insert("event_kind".to_string(), json!(kind));
        event.insert("kind".to_string(), json!(kind));
        event.insert(
            "world_id".to_string(),
            json!(binding_string("world_id", "unbound")),
        );
        event.insert(
            "branch_id".to_string(),
            json!(binding_string("branch_id", "main")),
        );
        event.insert(
            "finality_epoch".to_string(),
            json!(binding_u64("finality_epoch")),
        );
        event.insert("finality_block_hash".to_string(), binding_hash);
        event.insert(
            "finality_status".to_string(),
            json!(binding_string("finality_status", "pending")),
        );
        event.insert("reorg_epoch".to_string(), json!(binding_u64("reorg_epoch")));
        event
            .entry("logical_tick".to_string())
            .or_insert_with(|| json!(0));
        event
            .entry("request_digest".to_string())
            .or_insert(JsonValue::Null);
        event
            .entry("response_digest".to_string())
            .or_insert(JsonValue::Null);
        event
            .entry("envelope_digest".to_string())
            .or_insert(JsonValue::Null);
        event
            .entry("retry_seq".to_string())
            .or_insert_with(|| json!(0));
        event
            .entry("transport_attempt".to_string())
            .or_insert_with(|| json!(0));
        event
            .entry("status".to_string())
            .or_insert_with(|| json!("pending"));
        event
            .entry("causal_refs".to_string())
            .or_insert_with(|| json!([]));
        let payload_digest = cognition_digest_v1(
            "oasis7.cognition.event-payload.v1",
            &cognition_event_payload_projection(&JsonValue::Object(event.clone())),
        );
        event.insert("payload_digest".to_string(), json!(payload_digest));
        let event_digest = cognition_digest_v1(
            "oasis7.cognition.event.v1",
            &JsonValue::Object(event.clone()),
        );
        event.insert("event_digest".to_string(), json!(event_digest));
        events.push(JsonValue::Object(event));
    }
    journal.insert("head_seq".to_string(), json!(next_seq));
    let events = journal
        .get("events")
        .cloned()
        .ok_or_else(|| validation("cognition_events_missing"))?;
    journal.insert(
        "head_digest".to_string(),
        cognition_digest_v1(
            "oasis7.cognition.journal-head.v1",
            &json!({"head_seq": next_seq, "events": events}),
        )
        .into(),
    );
    Ok(next_seq)
}

/// Return the canonical event payload independently of journal linkage. The
/// sequence, parent pointer and event digest are transport metadata; all
/// other dense/defaulted fields are part of the signed payload and therefore
/// must be covered identically on append and restore.
pub(super) fn cognition_event_payload_projection(event: &JsonValue) -> JsonValue {
    let mut payload = event.clone();
    if let Some(object) = payload.as_object_mut() {
        object.remove("journal_seq");
        object.remove("parent_event_digest");
        object.remove("payload_digest");
        object.remove("event_digest");
    }
    payload
}

pub(super) fn validate_marker_current_world(
    world: &World,
    marker: &WorldCommitRecordV1,
) -> Result<(), WorldError> {
    if marker.agent_id.trim().is_empty() {
        return Ok(());
    }
    let root = world.current_state_root_hash()?;
    let manifest = world.current_manifest_hash()?;
    let head_matches = if matches!(marker.status.as_str(), "prepared" | "aborted") {
        marker.parent_tick == world.state().time
            && marker.parent_world_hash == root
            && marker.staged_state_root == root
    } else {
        marker.status == "committed"
    };
    if !head_matches || marker.runtime_manifest_hash != manifest {
        return Err(validation("cognition_world_head_mismatch"));
    }
    if let Some(binding) = world.cognition().get("runtime_binding") {
        let block_hash = strict_optional_finality_hash(binding.get("finality_block_hash"))?;
        let identity_matches = binding["world_id"].as_str() == Some(marker.world_id.as_str())
            && binding["branch_id"].as_str() == Some(marker.branch_id.as_str())
            && binding["runtime_manifest_hash"].as_str()
                == Some(marker.runtime_manifest_hash.as_str());
        // Committed receipts are immutable historical evidence. A later
        // canonical reorg/rebind may legitimately change finality fields and
        // reorg epoch, but it may not move the receipt to another world,
        // branch, or runtime manifest. Prepared/aborted records still require
        // the exact current binding because they have not become history.
        let binding_matches = if marker.status == "committed" {
            identity_matches
        } else {
            identity_matches
                && binding["finality_epoch"].as_u64() == Some(marker.finality_epoch)
                && block_hash.as_deref() == marker.finality_block_hash.as_deref()
                && binding["finality_status"].as_str() == Some(marker.finality_status.as_str())
                && binding["reorg_epoch"].as_u64() == Some(marker.reorg_epoch)
        };
        if !binding_matches {
            return Err(validation("cognition_finality_binding_mismatch"));
        }
    }
    Ok(())
}

/// Validate the durable event prefix before recovery exposes a committed
/// projection. Recovery may repair a crash-truncated lifecycle suffix, but it
/// must never accept an event with a missing or forged canonical digest.
pub(super) fn validate_cognition_journal_integrity(journal: &JsonValue) -> Result<(), WorldError> {
    let schema = journal
        .get("schema_version")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    if !schema.is_empty() && schema != "cognition-journal.v1" {
        return Err(validation("cognition_journal_schema_mismatch"));
    }
    let events = journal
        .get("events")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| validation("cognition_events_missing"))?;
    let mut previous_seq = 0u64;
    let mut previous_event_digest = String::new();
    let mut lifecycle: BTreeMap<(String, String, String, String, String), LifecycleState> =
        BTreeMap::new();
    let mut dispatch_attempts: BTreeMap<(String, String, String, String, String), u64> =
        BTreeMap::new();
    for event in events {
        let object = event
            .as_object()
            .ok_or_else(|| validation("cognition_event_not_object"))?;
        let sequence = object
            .get("journal_seq")
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| validation("cognition_journal_sequence_missing"))?;
        // The journal is an append-only prefix. A gap is not a recoverable
        // crash suffix: it would make an unverified middle event appear
        // committed and must fail closed before any projection is exposed.
        if sequence != previous_seq.saturating_add(1) {
            return Err(validation("cognition_journal_sequence_mismatch"));
        }
        let required_string = |field: &str| {
            object
                .get(field)
                .and_then(JsonValue::as_str)
                .filter(|value| !value.trim().is_empty() && value.len() <= 256)
        };
        if required_string("schema_version") != Some("cognition-journal-event.v1")
            || required_string("event_kind").is_none()
            || required_string("kind") != required_string("event_kind")
            || required_string("world_id").is_none()
            || required_string("branch_id").is_none()
            || required_string("finality_status").is_none()
            || object
                .get("finality_epoch")
                .and_then(JsonValue::as_u64)
                .is_none()
            || object
                .get("reorg_epoch")
                .and_then(JsonValue::as_u64)
                .is_none()
            || object
                .get("logical_tick")
                .and_then(JsonValue::as_u64)
                .is_none()
            || object
                .get("retry_seq")
                .and_then(JsonValue::as_u64)
                .is_none()
            || object
                .get("transport_attempt")
                .and_then(JsonValue::as_u64)
                .is_none()
            || required_string("status").is_none()
            || !object.contains_key("finality_block_hash")
            || !object.contains_key("request_digest")
            || !object.contains_key("response_digest")
            || !object.contains_key("envelope_digest")
            || object
                .get("causal_refs")
                .and_then(JsonValue::as_array)
                .is_none()
        {
            return Err(validation("cognition_event_dense_fields_missing"));
        }
        let parent = object
            .get("parent_event_digest")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| validation("cognition_event_parent_missing"))?;
        if parent != previous_event_digest {
            return Err(validation("cognition_event_parent_mismatch"));
        }
        let finality_status = object
            .get("finality_status")
            .and_then(JsonValue::as_str)
            .expect("required finality status");
        let finality_hash = match object.get("finality_block_hash") {
            Some(JsonValue::Null) => None,
            Some(JsonValue::String(hash)) => Some(hash.as_str()),
            _ => return Err(validation("cognition_event_finality_invalid")),
        };
        if !finality_binding_is_legal(finality_status, finality_hash) {
            return Err(validation("cognition_event_finality_invalid"));
        }
        let payload_digest = object
            .get("payload_digest")
            .and_then(JsonValue::as_str)
            .filter(|value| is_canonical_digest(value));
        if payload_digest.is_none() {
            return Err(validation("cognition_event_payload_digest_missing"));
        }
        let expected_payload_digest = cognition_digest_v1(
            "oasis7.cognition.event-payload.v1",
            &cognition_event_payload_projection(event),
        );
        if payload_digest != Some(expected_payload_digest.as_str()) {
            return Err(validation("cognition_event_payload_digest_mismatch"));
        }
        if object
            .get("causal_refs")
            .and_then(JsonValue::as_array)
            .is_some_and(|refs| {
                refs.iter().any(|reference| {
                    reference
                        .as_str()
                        .is_none_or(|value| value.trim().is_empty() || value.len() > 256)
                })
            })
        {
            return Err(validation("cognition_event_causal_refs_invalid"));
        }
        let event_digest = object
            .get("event_digest")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| validation("cognition_event_digest_missing"))?;
        let mut unsigned = event.clone();
        unsigned
            .as_object_mut()
            .ok_or_else(|| validation("cognition_event_not_object"))?
            .remove("event_digest");
        let expected = cognition_digest_v1("oasis7.cognition.event.v1", &unsigned);
        if expected != event_digest {
            return Err(validation("cognition_event_digest_mismatch"));
        }
        validate_lifecycle_event(object, &mut lifecycle, &mut dispatch_attempts)?;
        previous_event_digest = event_digest.to_string();
        previous_seq = sequence;
    }
    let head_seq = journal
        .get("head_seq")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    // A head one sequence past the durable prefix represents the single
    // terminal event that recovery may reconstruct from a committed marker.
    // Larger divergence would conceal an unverified suffix and is rejected.
    if head_seq < previous_seq || head_seq > previous_seq.saturating_add(1) {
        return Err(validation("cognition_journal_head_sequence_mismatch"));
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LifecycleState {
    Started,
    ContextCaptured,
    Dispatched,
    ResponseRecorded,
    Submitted,
    Validated,
    Linked,
    Failed,
    Cancelled,
    Completed,
}

fn validate_lifecycle_event(
    event: &serde_json::Map<String, JsonValue>,
    states: &mut BTreeMap<(String, String, String, String, String), LifecycleState>,
    attempts: &mut BTreeMap<(String, String, String, String, String), u64>,
) -> Result<(), WorldError> {
    let kind = event
        .get("event_kind")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    // Scheduler/continuation/feedback records are not turn lifecycle events;
    // they may carry an agent identity but have their own state machine.
    if !matches!(
        kind,
        "TurnStarted"
            | "ContextCaptured"
            | "RequestDispatched"
            | "ResponseRecorded"
            | "DecisionEnvelopeSubmitted"
            | "DecisionValidated"
            | "WorldReceiptLinked"
            | "DecisionRejected"
            | "CognitionTurnFailed"
            | "CognitionTurnCancelled"
            | "CognitionTurnCompleted"
            | "LateResponseRejected"
    ) {
        return Ok(());
    }
    let key = (
        event
            .get("agent_id")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        event
            .get("agent_session_id")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        event
            .get("agent_turn_id")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        event
            .get("decision_request_id")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        event
            .get("request_digest")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
    );
    // Legacy sparse events do not have a complete identity and are validated
    // by their marker/recovery compatibility path instead.
    if key.0.is_empty()
        || key.1.is_empty()
        || key.2.is_empty()
        || key.3.is_empty()
        || key.4.is_empty()
    {
        return Ok(());
    }
    let state = states.get(&key).copied();
    let next = match kind {
        "TurnStarted" if state.is_none() => LifecycleState::Started,
        // A stale-base rejection can be persisted after provider dispatch (or
        // response validation) when the captured World head moved. It is a
        // terminal branch before any receipt is linked, not a new turn.
        "DecisionRejected"
            if matches!(
                state,
                None | Some(
                    LifecycleState::Started
                        | LifecycleState::ContextCaptured
                        | LifecycleState::Dispatched
                        | LifecycleState::ResponseRecorded
                        | LifecycleState::Submitted
                        | LifecycleState::Validated
                )
            ) =>
        {
            LifecycleState::Cancelled
        }
        "ContextCaptured" if matches!(state, Some(LifecycleState::Started)) => {
            LifecycleState::ContextCaptured
        }
        "RequestDispatched"
            if matches!(
                state,
                Some(LifecycleState::ContextCaptured | LifecycleState::Dispatched)
            ) =>
        {
            let attempt = event
                .get("transport_attempt")
                .and_then(JsonValue::as_u64)
                .ok_or_else(|| validation("cognition_transport_attempt_invalid"))?;
            let previous = attempts.get(&key).copied().unwrap_or_default();
            if attempt != previous.saturating_add(1) {
                return Err(validation("cognition_transport_attempt_gap"));
            }
            attempts.insert(key.clone(), attempt);
            LifecycleState::Dispatched
        }
        "ResponseRecorded" if matches!(state, Some(LifecycleState::Dispatched)) => {
            LifecycleState::ResponseRecorded
        }
        "DecisionEnvelopeSubmitted" if matches!(state, Some(LifecycleState::ResponseRecorded)) => {
            LifecycleState::Submitted
        }
        "DecisionValidated"
            if matches!(
                state,
                Some(
                    LifecycleState::ContextCaptured
                        | LifecycleState::Dispatched
                        | LifecycleState::ResponseRecorded
                        | LifecycleState::Submitted
                )
            ) =>
        {
            LifecycleState::Validated
        }
        "WorldReceiptLinked" if matches!(state, Some(LifecycleState::Validated)) => {
            LifecycleState::Linked
        }
        "CognitionTurnFailed"
            if matches!(
                state,
                Some(LifecycleState::ContextCaptured | LifecycleState::Dispatched)
            ) =>
        {
            LifecycleState::Failed
        }
        "CognitionTurnCancelled" if !matches!(state, None | Some(LifecycleState::Completed)) => {
            LifecycleState::Cancelled
        }
        "CognitionTurnCompleted"
            if matches!(
                state,
                Some(
                    LifecycleState::Failed
                        | LifecycleState::Cancelled
                        | LifecycleState::Validated
                        | LifecycleState::Linked
                )
            ) =>
        {
            LifecycleState::Completed
        }
        "LateResponseRejected"
            if matches!(
                state,
                Some(LifecycleState::Cancelled | LifecycleState::Completed)
            ) =>
        {
            state.unwrap()
        }
        _ => return Err(validation("cognition_lifecycle_order_invalid")),
    };
    states.insert(key, next);
    Ok(())
}

pub(super) fn validate_cognition_journal_head(journal: &JsonValue) -> Result<(), WorldError> {
    let events = journal
        .get("events")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| validation("cognition_events_missing"))?;
    let head_seq = journal
        .get("head_seq")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    let max_seq = events
        .iter()
        .filter_map(|event| event.get("journal_seq").and_then(JsonValue::as_u64))
        .max()
        .unwrap_or_default();
    if head_seq != max_seq {
        return Err(validation("cognition_journal_head_sequence_mismatch"));
    }
    let expected_head = cognition_digest_v1(
        "oasis7.cognition.journal-head.v1",
        &json!({"head_seq": head_seq, "events": events}),
    );
    if journal.get("head_digest").and_then(JsonValue::as_str) != Some(expected_head.as_str()) {
        return Err(validation("cognition_journal_head_digest_mismatch"));
    }
    Ok(())
}

pub(super) fn validate_response_record(
    projection: &JsonValue,
    response: &CognitionResponseRecordV1,
) -> Result<(), WorldError> {
    if !is_canonical_digest(&response.response_digest) {
        return Err(validation("response_digest_noncanonical"));
    }
    let artifact = response
        .response_artifact
        .as_ref()
        .ok_or_else(|| validation("response_artifact_missing"))?;
    if artifact.get("context_discriminator").is_some() {
        let identity: RuntimeCognitionResponseArtifactV1 = serde_json::from_value(artifact.clone())
            .map_err(|_| validation("response_artifact_identity_invalid"))?;
        identity
            .validate()
            .map_err(|_| validation("response_artifact_identity_invalid"))?;
    }
    if response_artifact_digest(artifact) != response.response_digest {
        return Err(validation("response_artifact_digest_mismatch"));
    }
    let journal = projection
        .get("cognition_journal")
        .ok_or_else(|| validation("cognition_journal_missing"))?;
    let head_seq = journal
        .get("head_seq")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    let events = journal
        .get("events")
        .cloned()
        .unwrap_or_else(|| JsonValue::Array(Vec::new()));
    let expected = cognition_digest_v1(
        "oasis7.cognition.journal-head.v1",
        &json!({"head_seq": head_seq, "events": events}),
    );
    if response.journal_head.trim().is_empty()
        || (!journal_head_history_contains(projection, &response.journal_head)
            && expected != response.journal_head)
    {
        return Err(validation("response_journal_head_mismatch"));
    }
    Ok(())
}

pub(super) fn validate_response_lineage_binding(
    response: &CognitionResponseRecordV1,
    marker: &WorldCommitRecordV1,
) -> Result<(), WorldError> {
    let Some(artifact) = response.response_artifact.as_ref() else {
        return Err(validation("response_artifact_missing"));
    };
    let Some(_) = artifact.get("context_discriminator") else {
        return if marker.agent_id.is_empty() {
            Ok(())
        } else {
            Err(validation("response_artifact_identity_required"))
        };
    };
    let identity: RuntimeCognitionResponseArtifactV1 = serde_json::from_value(artifact.clone())
        .map_err(|_| validation("response_artifact_identity_invalid"))?;
    identity
        .validate()
        .map_err(|_| validation("response_artifact_identity_invalid"))?;
    if marker.response_artifact_digest.is_empty() {
        return Err(validation("response_artifact_marker_missing"));
    }
    let outer = artifact
        .get("outer_lineage")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| validation("response_outer_lineage_missing"))?;
    let outer_block_hash = outer
        .get("finality_block_hash")
        .and_then(JsonValue::as_str)
        .map(str::to_string);
    let expected_base_world_hash = if marker.parent_world_hash.starts_with("blake3:") {
        marker.parent_world_hash.clone()
    } else {
        world_state_binding_digest_v1(
            &marker.world_id,
            &marker.branch_id,
            marker.finality_epoch,
            marker.finality_block_hash.as_deref(),
            &marker.finality_status,
            marker.parent_tick,
            &marker.parent_world_hash,
            marker.reorg_epoch,
            &marker.runtime_manifest_hash,
        )
    };
    let expected_manifest_hash = if marker.runtime_manifest_hash.starts_with("blake3:") {
        marker.runtime_manifest_hash.clone()
    } else {
        cognition_digest_v1("oasis7.runtime.manifest.v1", &marker.runtime_manifest_hash)
    };
    if outer.get("agent_id").and_then(JsonValue::as_str) != Some(marker.agent_id.as_str())
        || outer.get("world_id").and_then(JsonValue::as_str) != Some(marker.world_id.as_str())
        || outer.get("branch_id").and_then(JsonValue::as_str) != Some(marker.branch_id.as_str())
        || outer.get("finality_epoch").and_then(JsonValue::as_u64) != Some(marker.finality_epoch)
        || outer_block_hash != marker.finality_block_hash
        || outer.get("finality_status").and_then(JsonValue::as_str)
            != Some(marker.finality_status.as_str())
        || outer.get("base_tick").and_then(JsonValue::as_u64) != Some(marker.parent_tick)
        || outer.get("base_world_hash").and_then(JsonValue::as_str)
            != Some(expected_base_world_hash.as_str())
        || outer.get("reorg_epoch").and_then(JsonValue::as_u64) != Some(marker.reorg_epoch)
        || outer
            .get("runtime_manifest_hash")
            .and_then(JsonValue::as_str)
            != Some(expected_manifest_hash.as_str())
    {
        return Err(validation("response_outer_lineage_mismatch"));
    }
    if identity.agent_session_id != marker.agent_session_id
        || identity.agent_turn_id != marker.agent_turn_id
        || identity.decision_request_id != marker.decision_request_id
        || identity.context_discriminator != marker.response_context_discriminator
        || identity.context_version != marker.response_context_version
        || (marker.response_retry_seq > 0 && identity.retry_seq != marker.response_retry_seq)
        || (marker.transport_attempt > 0 && identity.transport_attempt != marker.transport_attempt)
        || identity.request_digest != marker.request_digest
        || (!marker.response_artifact_digest.is_empty()
            && response.response_digest != marker.response_artifact_digest)
    {
        return Err(validation("response_artifact_lineage_mismatch"));
    }
    Ok(())
}

fn journal_head_history_contains(projection: &JsonValue, candidate: &str) -> bool {
    let Some(journal) = projection.get("cognition_journal") else {
        return false;
    };
    let Some(events) = journal.get("events").and_then(JsonValue::as_array) else {
        return false;
    };
    let head_seq = journal
        .get("head_seq")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    for prefix_len in 0..=events.len() {
        let prefix = &events[..prefix_len];
        let prefix_seq = prefix
            .iter()
            .filter_map(|event| event.get("journal_seq").and_then(JsonValue::as_u64))
            .max()
            .unwrap_or_default();
        let sequence = if prefix_len == events.len() {
            head_seq.max(prefix_seq)
        } else {
            prefix_seq
        };
        if cognition_digest_v1(
            "oasis7.cognition.journal-head.v1",
            &json!({"head_seq": sequence, "events": prefix}),
        ) == candidate
        {
            return true;
        }
    }
    false
}

pub(super) fn persist_recovery_report(
    projection: &mut JsonValue,
    report: &CognitionRecoveryReport,
) -> Result<(), WorldError> {
    let object = projection
        .as_object_mut()
        .ok_or_else(|| validation("cognition_projection_not_object"))?;
    let recovery = object
        .entry("recovery")
        .or_insert_with(|| JsonValue::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or_else(|| validation("recovery_projection_not_object"))?;
    recovery.insert("disposition".to_string(), json!(&report.disposition));
    recovery.insert("reject_reason".to_string(), json!(&report.reject_reason));
    recovery.insert(
        "idempotency_key".to_string(),
        json!(&report.idempotency_key),
    );
    recovery.insert("quarantine_id".to_string(), json!(&report.quarantine_id));
    recovery.insert("candidate_root".to_string(), json!(&report.candidate_root));
    recovery.insert(
        "candidate_receipt".to_string(),
        json!(&report.candidate_receipt),
    );
    recovery.insert("receipt".to_string(), json!(&report.receipt));
    recovery.insert("world_root".to_string(), json!(&report.world_root));
    recovery.insert("journal_head".to_string(), json!(&report.journal_head));
    recovery.insert("retry_count".to_string(), json!(report.retry_count));
    recovery.insert(
        "revalidation_count".to_string(),
        json!(report.revalidation_count),
    );
    recovery.insert(
        "projection_repairs".to_string(),
        json!(report.projection_repairs),
    );
    recovery.insert(
        "provider_invocation_count".to_string(),
        json!(report.provider_invocation_count),
    );
    recovery.insert(
        "kernel_invocation_count".to_string(),
        json!(report.kernel_invocation_count),
    );
    recovery.insert("effect_count".to_string(), json!(report.effect_count));
    recovery.insert("debit_count".to_string(), json!(report.debit_count));
    recovery.insert(
        "receipt_count".to_string(),
        json!(u64::from(report.receipt.is_some())),
    );
    recovery.insert(
        "world_receipt_linked_count".to_string(),
        json!(report.world_receipt_linked_count),
    );
    recovery.insert(
        "response_replayed".to_string(),
        json!(report.response_replayed),
    );
    Ok(())
}

pub(super) fn cognition_error<T: std::fmt::Display>(code: &'static str, error: T) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: format!("{code}: {error}"),
    }
}

pub(super) fn cognition_validation(code: impl Into<String>) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: format!("cognition validation failed: {}", code.into()),
    }
}

/// Decode persisted optional finality hashes without normalizing malformed
/// JSON into `None`. Missing or explicit `null` values preserve optional
/// absence; a supplied hash must remain a string so recovery cannot silently
/// weaken its binding.
pub(super) fn strict_optional_finality_hash(
    value: Option<&JsonValue>,
) -> Result<Option<String>, WorldError> {
    match value {
        None => Ok(None),
        Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::String(hash)) => Ok(Some(hash.clone())),
        _ => Err(validation("recovery_pending")),
    }
}

pub(super) fn legacy_recovery_report() -> CognitionRecoveryReport {
    CognitionRecoveryReport {
        world_root: None,
        receipt: None,
        disposition: "rejected".to_string(),
        reject_reason: Some("legacy_no_cognition_proof".to_string()),
        auto_submitted: false,
        idempotency_key: None,
        quarantine_id: None,
        candidate_root: None,
        candidate_receipt: None,
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

pub(super) fn parent_root(marker: &WorldCommitRecordV1) -> WorldRootViewV1 {
    WorldRootViewV1 {
        schema_version: "world-root-view.v1".to_string(),
        world_id: marker.world_id.clone(),
        branch_id: marker.branch_id.clone(),
        logical_tick: marker.parent_tick,
        state_root: marker.parent_world_hash.clone(),
        head_status: "recovery_pending".to_string(),
        commit_id: None,
        quarantine_id: None,
    }
}

fn is_canonical_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validation(code: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: format!("cognition validation failed: {code}"),
    }
}

#[cfg(test)]
mod lifecycle_validation_tests {
    use super::*;

    fn event(kind: &str, attempt: Option<u64>) -> serde_json::Map<String, JsonValue> {
        let mut event = json!({
            "event_kind": kind,
            "agent_id": "agent-a",
            "agent_session_id": "session-a",
            "agent_turn_id": "turn-a",
            "decision_request_id": "request-a",
            "request_digest": "blake3:request",
        })
        .as_object()
        .unwrap()
        .clone();
        if let Some(attempt) = attempt {
            event.insert("transport_attempt".to_string(), json!(attempt));
        }
        event
    }

    fn apply(events: &[(&str, Option<u64>)]) -> Result<(), WorldError> {
        let mut states = BTreeMap::new();
        let mut attempts = BTreeMap::new();
        for (kind, attempt) in events {
            validate_lifecycle_event(&event(kind, *attempt), &mut states, &mut attempts)?;
        }
        Ok(())
    }

    #[test]
    fn stale_rejection_closes_dispatched_turn_but_not_receipt_linked_turn() {
        apply(&[
            ("TurnStarted", None),
            ("ContextCaptured", None),
            ("RequestDispatched", Some(1)),
            ("DecisionRejected", None),
            ("CognitionTurnCompleted", None),
        ])
        .expect("a stale-base rejection after dispatch is a valid terminal branch");

        let error = apply(&[
            ("TurnStarted", None),
            ("ContextCaptured", None),
            ("RequestDispatched", Some(1)),
            ("ResponseRecorded", None),
            ("DecisionEnvelopeSubmitted", None),
            ("DecisionValidated", None),
            ("WorldReceiptLinked", None),
            ("DecisionRejected", None),
        ])
        .expect_err("a linked World receipt cannot later become a stale rejection");
        assert!(matches!(
            error,
            WorldError::DistributedValidationFailed { ref reason }
                if reason == "cognition validation failed: cognition_lifecycle_order_invalid"
        ));
    }
}
