//! Bounded owned decoding of one bare DomainEvent original, not WorldEvent.
//! Returned content has no occurrence, mailbox, execution or authority proof.
#[path = "incremental_domain_event_decode_inventory.rs"]
mod inventory;
#[path = "incremental_domain_event_decode_layout.rs"]
mod layout;
use crate::{DomainEvent, incremental_original_decode::check_decoded_canonical_original_v1};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::{
    cbor_decode_preflight::{
        CborPreflightErrorV1, CborPreflightWorkV1, preflight_definite_cbor_v1,
    },
    memory_budget::{MemoryBudget, MemoryClass, ReservedValue},
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
pub struct DomainEventDecodeWorkV1 {
    pub structure: CborPreflightWorkV1,
    pub canonical: CanonicalCborWorkV1,
    pub max_original_bytes: usize,
    pub max_heap_bytes: usize,
}
pub struct DecodedDomainEventV1 {
    event: DomainEvent,
    class: MemoryClass,
}
impl DecodedDomainEventV1 {
    pub fn event(&self) -> &DomainEvent {
        &self.event
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("bare DomainEvent type/canonical original")
}
/// The closed layout inventory includes all reachable Vec/Map/Set/Box elements,
/// serde's pinned actual Content type and arbitrary serde_json::Value. Every
/// map/sequence/token is counted before typed serde or a size hint is honored.
/// BTree sets are serialized as sequences and receive node, not Vec-head cost.
/// ContentRefDeserializer can clone a buffered subtree before decoding a nested
/// enum. At most one extra subtree per nesting level can be live on its call
/// stack, bounded by the observed depth (+ two boundary stages). The shared
/// per-stage allowance covers both content and target containers, owned strings
/// and Debug-escaped error capacity. No reachable missing-field default in the
/// audited graph allocates nonempty data. This admission is conservative, not a
/// heap measurement; limits are never raised and refusal preserves raw input.
pub fn decode_domain_event_original_v1<C: FnMut() -> bool>(
    raw: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut DomainEventDecodeWorkV1,
    mut cancelled: C,
) -> Result<ReservedValue<DecodedDomainEventV1>, ValueTreeError> {
    assert_json_owned_layout_features_v1();
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    // A changed transitive DTO/default/Deserialize graph invalidates this
    // implementation inventory until its layout proof is re-audited.
    {
        let _inventory_phase = memory.reserve(8192, class)?;
        if !inventory::domain_event_layout_inventory_matches_v1() {
            return Err(ValueTreeError::Invalid(
                "DomainEvent decode layout inventory changed",
            ));
        }
    }
    if raw.len() > work.max_original_bytes {
        return Err(ValueTreeError::ResourceLimited(
            "DomainEvent original bytes",
        ));
    }
    let shape = preflight_definite_cbor_v1(raw, &mut work.structure, &mut cancelled).map_err(
        |e| match e {
            CborPreflightErrorV1::Invalid => invalid(),
            CborPreflightErrorV1::Cancelled => ValueTreeError::Cancelled,
            CborPreflightErrorV1::ResourceLimited => {
                ValueTreeError::ResourceLimited("DomainEvent structure")
            }
        },
    )?;
    work.structure.values_remaining = work
        .structure
        .values_remaining
        .checked_sub(shape.values)
        .ok_or(ValueTreeError::ResourceLimited("DomainEvent typed visits"))?;
    work.structure.bytes_remaining = work
        .structure
        .bytes_remaining
        .checked_sub(raw.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited("DomainEvent typed bytes"))?;
    let l = layout::domain_event_owned_layouts_v1();
    let limit = || ValueTreeError::ResourceLimited("DomainEvent heap bound");
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
    let stages = shape.max_depth.checked_add(2).ok_or_else(limit)?;
    let bytes = phase
        .checked_mul(stages as u64)
        .and_then(|n| usize::try_from(n).ok())
        .and_then(|n| n.checked_add(l.root))
        .and_then(|n| n.checked_add(std::mem::size_of::<DecodedDomainEventV1>()))
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(limit)?;
    if bytes > work.max_heap_bytes {
        return Err(ValueTreeError::ResourceLimited(
            "DomainEvent heap allowance",
        ));
    }
    let charge = memory.reserve(bytes, class)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let event: DomainEvent = serde_cbor::from_slice(raw).map_err(|_| invalid())?;
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
    Ok(charge.retain(DecodedDomainEventV1 { event, class }))
}

#[cfg(test)]
#[path = "incremental_domain_event_decode_tests.rs"]
mod tests;

// serde_json's public Iter is an opaque wrapper in both feature modes, so it
// cannot be equated to btree_map::Iter. Instead use a compile-time API marker:
// shift_remove is an inherent Map method ONLY with preserve_order enabled in
// the pinned serde_json release. Inherent lookup then wins and returns Option,
// making this assignment fail rather than silently switching allocation model.
struct AuditedBtreeMapOnly;
trait NoPreserveOrderFeature {
    fn shift_remove(&mut self, key: &str) -> AuditedBtreeMapOnly;
}
impl NoPreserveOrderFeature for serde_json::Map<String, serde_json::Value> {
    fn shift_remove(&mut self, _: &str) -> AuditedBtreeMapOnly {
        AuditedBtreeMapOnly
    }
}
pub(crate) fn assert_json_owned_layout_features_v1() {
    let mut map = serde_json::Map::<String, serde_json::Value>::new();
    let _: AuditedBtreeMapOnly = map.shift_remove("");
}
// arbitrary_precision gives Number a String allocation even for a scalar.
// Feature unification anywhere in the dependency graph must stop this build.
const _: () = assert!(
    !std::mem::needs_drop::<serde_json::Number>(),
    "owned original decode requires audited scalar serde_json Number"
);
