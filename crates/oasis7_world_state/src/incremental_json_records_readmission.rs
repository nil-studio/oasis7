//! Resource-only private metadata copy. No traversal, new profile or new
//! qualification is inferred from public getters or caller-reported class.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};

pub(crate) fn copy_json_subtree_metadata_ordinary_v1(
    source: &ReservedValue<JsonSubtreeQualificationV1>,
    profile: JsonRecordProfileV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let base = std::mem::size_of::<JsonSubtreeQualificationV1>();
    let footprint = base
        .checked_add(source.root.node_root.capacity())
        .ok_or(ValueTreeError::Invalid("JSON metadata footprint"))?;
    if !source.uses_budget(memory) || source.reserved_bytes() < footprint {
        return Err(ValueTreeError::Invalid("JSON metadata source pool/charge"));
    }
    source.validate_binding(profile, &source.root)?;
    if source.root.namespace != profile.namespace()? {
        return Err(ValueTreeError::Invalid("JSON metadata namespace"));
    }
    let bound = footprint
        .checked_mul(2)
        .and_then(|n| n.checked_add(256))
        .ok_or(ValueTreeError::Invalid("JSON metadata admission bound"))?;
    let charge = memory.reserve(bound, MemoryClass::Ordinary)?;
    let root = source.root.clone();
    if base
        .checked_add(root.node_root.capacity())
        .is_none_or(|n| n > charge.bytes())
    {
        return Err(ValueTreeError::Invalid("JSON metadata copied capacity"));
    }
    let output = JsonSubtreeQualificationV1 {
        root,
        profile: source.profile,
        nodes: source.nodes,
        exact: source.exact,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(output))
}
