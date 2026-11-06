//! Plain owned event admission, never a fact, occurrence or permission.
use crate::{
    WorldEvent,
    incremental_world_event_decode::{WorldEventDecodeWorkV1, admit_world_event_original_heap_v1},
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_wasm_abi::{
    CanonicalCborStreamContextV1, CanonicalCborStreamError, CanonicalCborWorkV1,
    stream_canonical_cbor_v1,
};
use std::io::Write;
struct Raw {
    bytes: Vec<u8>,
    max: usize,
}
impl Write for Raw {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(b.len())
            .is_none_or(|n| n > self.max)
        {
            return Err(std::io::Error::other("WorldEvent owned transport bytes"));
        }
        self.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Preserve event content in a newly decoded, admitted DTO. This is not a
/// zero-copy transfer: serialized shape cannot bound input spare capacity.
/// The caller covers every input allocation until this function returns,
/// including the overlap while the normalized output is decoded.
pub fn retain_world_event_owned_v1(
    event: WorldEvent,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut WorldEventDecodeWorkV1,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldEvent>, ValueTreeError> {
    let result = retain_serialized_world_event(&event, memory, class, work, cancelled);
    drop(event);
    result
}

fn retain_serialized_world_event<T: serde::Serialize + ?Sized>(
    event: &T,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut WorldEventDecodeWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldEvent>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if class != MemoryClass::Ordinary {
        return Err(ValueTreeError::Invalid("owned event class"));
    }
    let _raw_fee = memory
        .reserve(work.max_original_bytes, class)
        .map_err(ValueTreeError::Memory)?;
    let mut raw = Raw {
        bytes: Vec::new(),
        max: work.max_original_bytes,
    };
    raw.bytes
        .try_reserve_exact(raw.max)
        .map_err(|_| ValueTreeError::ResourceLimited("WorldEvent owned transport allocation"))?;
    if raw.bytes.capacity() > raw.max {
        return Err(ValueTreeError::ResourceLimited(
            "WorldEvent owned transport capacity",
        ));
    }
    let allocator = crate::incremental_original_decode::Allocator::new(memory, class);
    let empty = CanonicalCborWorkV1 {
        visits_remaining: 0,
        bytes_remaining: 0,
        max_depth: work.canonical.max_depth,
        max_map_keys: work.canonical.max_map_keys,
        max_key_bytes: work.canonical.max_key_bytes,
    };
    let (result, spent) = {
        let mut context = CanonicalCborStreamContextV1::new(
            &allocator,
            std::mem::replace(&mut work.canonical, empty),
            &mut cancelled,
        );
        let result = stream_canonical_cbor_v1(event, &mut context, &mut raw);
        (result, context.work)
    };
    work.canonical = spent;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = allocator.take_error() {
        return Err(ValueTreeError::Memory(e));
    }
    match result {
        Ok(summary) if summary.encoded_bytes == raw.bytes.len() as u64 => {}
        Err(CanonicalCborStreamError::ResourceLimited(s)) => {
            return Err(ValueTreeError::ResourceLimited(s));
        }
        Err(CanonicalCborStreamError::Cancelled) => return Err(ValueTreeError::Cancelled),
        _ => {
            return Err(ValueTreeError::ResourceLimited(
                "WorldEvent owned transport bytes",
            ));
        }
    }
    let fee = admit_world_event_original_heap_v1(&raw.bytes, memory, class, work, &mut cancelled)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let normalized: WorldEvent = serde_cbor::from_slice(&raw.bytes)
        .map_err(|_| ValueTreeError::Invalid("owned WorldEvent typed decode"))?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    crate::incremental_original_decode::check_decoded_canonical_original_v1(
        &normalized,
        &raw.bytes,
        memory,
        class,
        &mut work.canonical,
        &mut cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(fee.retain(normalized))
}
/// Admit the genuine borrowed override DTO fields without cloning either Action.
/// Metadata is plain event data; this function grants no occurrence or authority.
#[allow(clippy::too_many_arguments)]
pub fn retain_action_override_event_v1(
    id: crate::WorldEventId,
    time: crate::WorldTime,
    action_id: crate::ActionId,
    original_action: &crate::Action,
    override_action: &crate::Action,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut WorldEventDecodeWorkV1,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldEvent>, ValueTreeError> {
    #[derive(serde::Serialize)]
    struct Event<'a> {
        id: crate::WorldEventId,
        time: crate::WorldTime,
        caused_by: Option<crate::CausedBy>,
        body: Body<'a>,
    }
    #[derive(serde::Serialize)]
    #[serde(tag = "kind", content = "payload")]
    enum Body<'a> {
        ActionOverridden(Record<'a>),
    }
    #[derive(serde::Serialize)]
    struct Record<'a> {
        action_id: crate::ActionId,
        original_action: &'a crate::Action,
        override_action: &'a crate::Action,
    }
    // Exhaustive field guards, plus the existing source inventory and canonical
    // typed recheck, make this narrowly mirrored wire fail closed on drift.
    fn closed_fields(event: &WorldEvent, record: &crate::ActionOverrideRecord) {
        let WorldEvent {
            id: _,
            time: _,
            caused_by: _,
            body: _,
        } = event;
        let crate::ActionOverrideRecord {
            action_id: _,
            original_action: _,
            override_action: _,
        } = record;
    }
    let _closed_fields = closed_fields;
    let event = Event {
        id,
        time,
        caused_by: Some(crate::CausedBy::Action(action_id)),
        body: Body::ActionOverridden(Record {
            action_id,
            original_action,
            override_action,
        }),
    };
    retain_serialized_world_event(&event, memory, class, work, cancelled)
}

#[cfg(test)]
#[path = "incremental_world_event_owned_tests.rs"]
mod tests;
