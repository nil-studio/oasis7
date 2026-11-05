//! Bounded owned decoding of one WorldEvent original, not a History collection.
//! Returned content has no occurrence, mailbox, execution or authority proof.
#[path = "incremental_world_event_decode_inventory.rs"]
mod inventory;
#[path = "incremental_world_event_decode_layout.rs"]
mod layout;
#[path = "incremental_world_event_module_state_budget.rs"]
mod module_state_budget;
use crate::{WorldEvent, incremental_original_decode::check_decoded_canonical_original_v1};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::{
    cbor_decode_preflight::{
        CborPreflightErrorV1, CborPreflightWorkV1, preflight_definite_cbor_v1,
    },
    memory_budget::{MemoryBudget, MemoryClass, ReservedValue},
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
pub struct WorldEventDecodeWorkV1 {
    pub structure: CborPreflightWorkV1,
    pub canonical: CanonicalCborWorkV1,
    pub max_original_bytes: usize,
    pub max_heap_bytes: usize,
}
pub struct DecodedWorldEventV1 {
    event: WorldEvent,
    class: MemoryClass,
}
impl DecodedWorldEventV1 {
    pub fn event(&self) -> &WorldEvent {
        &self.event
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("WorldEvent type/canonical original")
}
/// Verify the shared closed source inventory before admitting a ModuleOutput.
/// Its DTOs, serde defaults and implementations live in the pinned ABI source.
/// This checks implementation layout only; it grants no execution authority.
pub fn verify_module_output_layout_inventory_v1(
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<(), ValueTreeError> {
    let _phase = memory.reserve(8192, class)?;
    crate::incremental_original_decode::assert_json_owned_layout_features_v1();
    if !inventory::world_event_layout_inventory_matches_v1() {
        return Err(ValueTreeError::Invalid(
            "ModuleOutput closed type inventory",
        ));
    }
    Ok(())
}
/// The closed layout inventory includes all reachable Vec/Map/Set/Box elements,
/// serde's pinned actual Content type and arbitrary serde_json::Value. Every
/// map/sequence/token is counted before typed serde or a size hint is honored.
/// BTree sets are serialized as sequences and receive node, not Vec-head cost.
/// ContentRefDeserializer can clone a buffered subtree before decoding a nested
/// enum. At most one extra subtree per nesting level can be live on its call
/// stack, bounded by the observed depth plus the two boundary stages and the
/// audited single transparent ModuleEvent flatten layer. Its buffered
/// Vec<Option<(Content,Content)>> is additionally charged by map entry count. The shared
/// per-stage allowance covers both content and target containers, owned strings
/// and Debug-escaped error capacity. No reachable missing-field default in the
/// audited graph allocates nonempty data. This admission is conservative, not a
/// heap measurement; limits are never raised and refusal preserves raw input.
pub fn decode_world_event_original_v1<C: FnMut() -> bool>(
    raw: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut WorldEventDecodeWorkV1,
    mut cancelled: C,
) -> Result<ReservedValue<DecodedWorldEventV1>, ValueTreeError> {
    crate::incremental_original_decode::assert_json_owned_layout_features_v1();
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    // A changed transitive DTO/default/Deserialize graph invalidates this
    // implementation inventory until its layout proof is re-audited.
    {
        let _inventory_phase = memory.reserve(8192, class)?;
        if !inventory::world_event_layout_inventory_matches_v1() {
            return Err(ValueTreeError::Invalid(
                "WorldEvent decode layout inventory changed",
            ));
        }
    }
    if raw.len() > work.max_original_bytes {
        return Err(ValueTreeError::ResourceLimited("WorldEvent original bytes"));
    }
    let shape = preflight_definite_cbor_v1(raw, &mut work.structure, &mut cancelled).map_err(
        |e| match e {
            CborPreflightErrorV1::Invalid => invalid(),
            CborPreflightErrorV1::Cancelled => ValueTreeError::Cancelled,
            CborPreflightErrorV1::ResourceLimited => {
                ValueTreeError::ResourceLimited("WorldEvent structure")
            }
        },
    )?;
    work.structure.values_remaining = work
        .structure
        .values_remaining
        .checked_sub(shape.values)
        .ok_or(ValueTreeError::ResourceLimited("WorldEvent typed visits"))?;
    work.structure.bytes_remaining = work
        .structure
        .bytes_remaining
        .checked_sub(raw.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited("WorldEvent typed bytes"))?;
    let bytes = match module_state_budget::module_state_world_event_heap_bound_v1(
        raw,
        &mut work.structure,
        &mut cancelled,
    )? {
        Some(bytes) => bytes,
        None => {
            let l = layout::world_event_owned_layouts_v1();
            let limit = || ValueTreeError::ResourceLimited("WorldEvent heap bound");
            let seq = l
                .seq_element
                .max(l.content)
                .checked_mul(4)
                .ok_or_else(limit)?
                .checked_add(
                    l.set_element
                        .checked_mul(16)
                        .and_then(|n| n.checked_add(256))
                        .ok_or_else(limit)?,
                )
                .ok_or_else(limit)?;
            let pair = l
                .map_pair
                .max(l.map_key.checked_add(l.map_value).ok_or_else(limit)?);
            let map = pair
                .checked_mul(16)
                .and_then(|n| n.checked_add(256))
                .and_then(|n| l.content_pair.checked_mul(4).and_then(|c| n.checked_add(c)))
                .ok_or_else(limit)?;
            let flatten_cost = l.flatten_element.checked_mul(4).ok_or_else(limit)?;
            let map = map.checked_add(flatten_cost).ok_or_else(limit)?;
            let token = l
                .box_pointee
                .checked_add(l.content)
                .and_then(|n| n.checked_add(l.json_value))
                .and_then(|n| n.checked_add(128))
                .ok_or_else(limit)?;
            let phase = shape
                .sequence_entries
                .checked_mul(seq as u64)
                .and_then(|n| {
                    shape
                        .map_entries
                        .checked_mul(map as u64)
                        .and_then(|m| n.checked_add(m))
                })
                .and_then(|n| {
                    shape
                        .values
                        .checked_mul(token as u64)
                        .and_then(|v| n.checked_add(v))
                })
                .and_then(|n| {
                    shape
                        .string_bytes
                        .checked_mul(16)
                        .and_then(|s| n.checked_add(s))
                })
                .ok_or_else(limit)?;
            let stages = shape.max_depth.checked_add(3).ok_or_else(limit)?;
            let bytes = phase
                .checked_mul(stages as u64)
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| n.checked_add(l.root))
                .and_then(|n| n.checked_add(std::mem::size_of::<DecodedWorldEventV1>()))
                .and_then(|n| n.checked_add(8192))
                .ok_or_else(limit)?;
            bytes
        }
    };
    if bytes > work.max_heap_bytes {
        return Err(ValueTreeError::ResourceLimited("WorldEvent heap allowance"));
    }
    let charge = memory.reserve(bytes, class)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let event: WorldEvent = serde_cbor::from_slice(raw).map_err(|_| invalid())?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    check_decoded_canonical_original_v1(
        &event,
        raw,
        memory,
        class,
        &mut work.canonical,
        &mut cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(DecodedWorldEventV1 { event, class }))
}

#[cfg(test)]
#[path = "incremental_world_event_decode_tests.rs"]
mod tests;
