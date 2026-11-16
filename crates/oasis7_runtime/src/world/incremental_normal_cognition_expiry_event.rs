//! Exact v1 ContinuationExpired details and Journal defaults. Hashes use the
//! original canonical domains; no Current commitment is put in v1 fields.
use super::*;
use serde::{Serialize, ser::SerializeMap};
pub(in crate::world) struct PaidExpiryEventV1 {
    pub(in crate::world) value: values::OwnedJson,
    _binding: Option<values::OwnedJson>,
    _metadata: MemoryReservation,
}
pub(in crate::world) fn head_text(
    value: Option<[u8; 32]>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    let n = if value.is_some() { 71 } else { 0 };
    common.charge(1, n as u64).map_err(formal)?;
    let fee = common
        .memory()
        .reserve(std::mem::size_of::<String>() + n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut text = String::with_capacity(n);
    if let Some(value) = value {
        text.push_str("blake3:");
        text.push_str(blake3::Hash::from(value).to_hex().as_str());
    }
    Ok(fee.retain(text))
}
struct Payload<'a>(&'a serde_json::Value);
impl Serialize for Payload<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let object = self
            .0
            .as_object()
            .ok_or_else(|| serde::ser::Error::custom("cognition_event_details_not_object"))?;
        let omit = [
            "journal_seq",
            "parent_event_digest",
            "payload_digest",
            "event_digest",
        ];
        let n = object
            .keys()
            .filter(|k| !omit.contains(&k.as_str()))
            .count();
        let mut map = s.serialize_map(Some(n))?;
        for (k, v) in object {
            if !omit.contains(&k.as_str()) {
                map.serialize_entry(k, v)?
            }
        }
        map.end()
    }
}
#[derive(Serialize)]
struct Details<'a> {
    continuation_id: &'a str,
    wake_id: &'a str,
    world_id: &'a str,
    agent_id: &'a str,
    agent_session_id: &'a str,
    agent_turn_id: &'a str,
    decision_request_id: &'a str,
    status: crate::cognition_wake::ContinuationStatusV1,
    continuation_status_digest: &'a Option<String>,
    scheduler_policy_digest: &'a str,
    cursor_seq: u64,
}
pub(super) fn prepare(
    continuation: &crate::cognition_wake::AgentContinuation,
    scheduler: &crate::cognition_scheduler::CognitionScheduler,
    wake: Option<&crate::cognition_scheduler::SchedulerWakeV1>,
    binding: Option<&serde_json::Value>,
    head_seq: u64,
    previous: Option<[u8; 32]>,
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidExpiryEventV1, BodyMutationError> {
    // Scheduler policy validation fixes both its strings to the original
    // constants and its six scalar fields. This finite original digest has a
    // fixed 16KiB phase, independent of the number of wakes or World size.
    common.charge(4096, 16384).map_err(formal)?;
    let _policy_phase = common
        .memory()
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let policy = scheduler.policy_config_digest();
    let mut value = values::json(
        &Details {
            continuation_id: &continuation.continuation_id,
            wake_id: &continuation.wake_id,
            world_id: &continuation.world_id,
            agent_id: &continuation.agent_id,
            agent_session_id: &continuation.agent_session_id,
            agent_turn_id: &continuation.agent_turn_id,
            decision_request_id: &continuation.decision_request_id,
            status: continuation.status,
            continuation_status_digest: &continuation.continuation_status_digest,
            scheduler_policy_digest: &policy,
            cursor_seq: scheduler.cursor_seq(),
        },
        work.max_selected_heap,
        common,
    )?;
    // Fixed inserted key/node inventory, including BTree split peak and the two
    // 71-byte digest strings. Variable binding payloads move from their separately
    // paid selected graph, and are never charged as a fixed scratch allowance.
    common.charge(40, 2048).map_err(formal)?;
    let metadata = common
        .memory()
        .reserve(64 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut binding = match binding.filter(|v| v.is_object()) {
        Some(value) => Some(values::json(value, work.max_selected_heap, common)?),
        None => None,
    };
    let object = value.value.as_object_mut().ok_or_else(bad)?;
    // A deactivated wake is part of the lifecycle details, exactly as before.
    // Build it as a borrowed serializer field rather than an unpaid DTO clone.
    if let Some(wake) = wake {
        let encoded = values::json(wake, work.max_selected_heap, common)?;
        return finish_with_wake(
            value, binding, metadata, encoded, head_seq, previous, work, common,
        );
    }
    fill_journal(
        object,
        binding.as_mut(),
        head_seq,
        previous,
        "ContinuationExpired",
    )?;
    finish(value, binding, metadata, work, common)
}
fn fill_journal(
    object: &mut serde_json::Map<String, serde_json::Value>,
    mut binding: Option<&mut values::OwnedJson>,
    head_seq: u64,
    previous: Option<[u8; 32]>,
    kind: &'static str,
) -> Result<(), BodyMutationError> {
    fn take(binding: &mut Option<&mut values::OwnedJson>, key: &str) -> Option<serde_json::Value> {
        binding
            .as_mut()
            .and_then(|v| v.value.as_object_mut())
            .and_then(|m| m.remove(key))
    }
    object.insert("schema_version".into(), "cognition-journal-event.v1".into());
    object.insert("journal_seq".into(), head_seq.saturating_add(1).into());
    let mut prev = String::with_capacity(if previous.is_some() { 71 } else { 0 });
    if let Some(digest) = previous {
        prev.push_str("blake3:");
        prev.push_str(blake3::Hash::from(digest).to_hex().as_str());
    }
    object.insert("parent_event_digest".into(), prev.into());
    object.insert("event_kind".into(), kind.into());
    object.insert("kind".into(), kind.into());
    for (key, default) in [
        ("world_id", "unbound"),
        ("branch_id", "main"),
        ("finality_status", "pending"),
    ] {
        let value = match take(&mut binding, key) {
            Some(serde_json::Value::String(v)) => serde_json::Value::String(v),
            _ => default.into(),
        };
        object.insert(key.into(), value);
    }
    for key in ["finality_epoch", "reorg_epoch"] {
        let n = take(&mut binding, key)
            .and_then(|v| v.as_u64())
            .unwrap_or_default();
        object.insert(key.into(), n.into());
    }
    object.insert(
        "finality_block_hash".into(),
        take(&mut binding, "finality_block_hash").unwrap_or(serde_json::Value::Null),
    );
    for key in ["logical_tick", "retry_seq", "transport_attempt"] {
        object.entry(key.to_owned()).or_insert(0.into());
    }
    for key in ["request_digest", "response_digest", "envelope_digest"] {
        object
            .entry(key.to_owned())
            .or_insert(serde_json::Value::Null);
    }
    object
        .entry("status".to_owned())
        .or_insert("pending".into());
    object
        .entry("causal_refs".to_owned())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    Ok(())
}
fn finish(
    mut value: values::OwnedJson,
    binding: Option<values::OwnedJson>,
    metadata: MemoryReservation,
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidExpiryEventV1, BodyMutationError> {
    let payload = values::digest(
        "oasis7.cognition.event-payload.v1",
        &Payload(&value.value),
        &mut work.hashes,
        common,
    )?;
    common.charge(1, payload.len() as u64).map_err(formal)?;
    value
        .value
        .as_object_mut()
        .ok_or_else(bad)?
        .insert("payload_digest".into(), payload.to_string().into());
    let digest = values::digest(
        "oasis7.cognition.event.v1",
        &value.value,
        &mut work.hashes,
        common,
    )?;
    common.charge(1, digest.len() as u64).map_err(formal)?;
    value
        .value
        .as_object_mut()
        .ok_or_else(bad)?
        .insert("event_digest".into(), digest.to_string().into());
    Ok(PaidExpiryEventV1 {
        value,
        _binding: binding,
        _metadata: metadata,
    })
}
fn finish_with_wake(
    mut value: values::OwnedJson,
    binding: Option<values::OwnedJson>,
    metadata: MemoryReservation,
    wake: values::OwnedJson,
    head_seq: u64,
    previous: Option<[u8; 32]>,
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidExpiryEventV1, BodyMutationError> {
    // Re-serialize the borrowed composed details once. The target owner holds
    // every moved wake field and its source stays paid until this finishes.
    #[derive(Serialize)]
    struct WithWake<'a> {
        #[serde(flatten)]
        base: &'a serde_json::Value,
        scheduler_action: &'static str,
        wake: &'a serde_json::Value,
    }
    value = values::json(
        &WithWake {
            base: &value.value,
            scheduler_action: "deactivated",
            wake: &wake.value,
        },
        work.max_selected_heap,
        common,
    )?;
    let mut binding = binding;
    fill_journal(
        value.value.as_object_mut().ok_or_else(bad)?,
        binding.as_mut(),
        head_seq,
        previous,
        "ContinuationExpired",
    )?;
    finish(value, binding, metadata, work, common)
}

/// Original scheduler details (policy/cursor plus flattened selected wake),
/// followed by the same original Journal defaults and digest domains.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_scheduler_v2(
    kind: &'static str,
    scheduler: &crate::cognition_scheduler::CognitionScheduler,
    wake: Option<&crate::cognition_scheduler::SchedulerWakeV1>,
    binding: Option<&serde_json::Value>,
    head_seq: u64,
    previous: Option<[u8; 32]>,
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidExpiryEventV1, BodyMutationError> {
    if !matches!(
        kind,
        "SchedulerWakeDeactivated" | "SchedulerWakeSelected" | "ContinuationWoken"
    ) {
        return Err(bad().into());
    }
    common.charge(4096, 16384).map_err(formal)?;
    let _policy = common
        .memory()
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let policy = scheduler.policy_config_digest();
    #[derive(Serialize)]
    struct SchedulerDetails<'a> {
        scheduler_policy_digest: &'a str,
        cursor_seq: u64,
        #[serde(flatten)]
        wake: Option<&'a crate::cognition_scheduler::SchedulerWakeV1>,
    }
    let mut value = values::json(
        &SchedulerDetails {
            scheduler_policy_digest: &policy,
            cursor_seq: scheduler.cursor_seq(),
            wake,
        },
        work.max_selected_heap,
        common,
    )?;
    common.charge(40, 2048).map_err(formal)?;
    let metadata = common
        .memory()
        .reserve(64 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut binding = match binding.filter(|v| v.is_object()) {
        Some(v) => Some(values::json(v, work.max_selected_heap, common)?),
        None => None,
    };
    fill_journal(
        value.value.as_object_mut().ok_or_else(bad)?,
        binding.as_mut(),
        head_seq,
        previous,
        kind,
    )?;
    finish(value, binding, metadata, work, common)
}

/// The original ContinuationScheduled details, with two explicit V2 source
/// commitments. The existing Journal envelope/default/hash kernel is shared.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_continuation_scheduled_v2(
    continuation: &crate::cognition_wake::AgentContinuation,
    scheduler: &crate::cognition_scheduler::CognitionScheduler,
    proposal_digest: &str,
    context_core_digest: &str,
    binding: Option<&serde_json::Value>,
    head_seq: u64,
    previous: Option<[u8; 32]>,
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidExpiryEventV1, BodyMutationError> {
    common.charge(4096, 16384).map_err(formal)?;
    let _policy = common
        .memory()
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let policy = scheduler.policy_config_digest();
    #[derive(Serialize)]
    struct Scheduled<'a> {
        #[serde(flatten)]
        base: Details<'a>,
        proposal_digest: &'a str,
        context_core_digest: &'a str,
    }
    let mut value = values::json(
        &Scheduled {
            base: Details {
                continuation_id: &continuation.continuation_id,
                wake_id: &continuation.wake_id,
                world_id: &continuation.world_id,
                agent_id: &continuation.agent_id,
                agent_session_id: &continuation.agent_session_id,
                agent_turn_id: &continuation.agent_turn_id,
                decision_request_id: &continuation.decision_request_id,
                status: continuation.status,
                continuation_status_digest: &continuation.continuation_status_digest,
                scheduler_policy_digest: &policy,
                cursor_seq: scheduler.cursor_seq(),
            },
            proposal_digest,
            context_core_digest,
        },
        work.max_selected_heap,
        common,
    )?;
    common.charge(40, 2048).map_err(formal)?;
    let metadata = common
        .memory()
        .reserve(64 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut binding = match binding.filter(|v| v.is_object()) {
        Some(v) => Some(values::json(v, work.max_selected_heap, common)?),
        None => None,
    };
    fill_journal(
        value.value.as_object_mut().ok_or_else(bad)?,
        binding.as_mut(),
        head_seq,
        previous,
        "ContinuationScheduled",
    )?;
    finish(value, binding, metadata, work, common)
}
