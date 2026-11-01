//! Changed-path JSON qualification for scalar edits. Every container keeps its
//! kind, length and exact member key set. Structural CRUD is a separate scope.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_record_diff::{self, RecordDiffLabelsV1},
};
use oasis7_formal_protocol::incremental_state::MerkleNodeReader;
use oasis7_proto::memory_budget::ReservedValue;
pub struct JsonScalarChangeWorkV1 {
    pub json: JsonReadWorkV1,
    pub paths_remaining: u64,
    /// Explicit pending changed-root queue admission; never sized by full S.
    pub max_pending_pairs: usize,
}
struct Pending {
    before: StateRootV1,
    after: StateRootV1,
    depth: usize,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("JSON scalar changed closure")
}
/// Proves one JSON graph's integrity from a private, exact qualified parent and
/// authenticated changed paths. Caller still verifies its generic outer
/// transition and original field/RootSet relationships in the owning receiver.
pub fn advance_json_scalar_subtree_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    parent: &ReservedValue<JsonSubtreeQualificationV1>,
    after: &StateRootV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonScalarChangeWorkV1,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    context.check()?;
    if context.values.class != MemoryClass::Ordinary || !parent.uses_budget(context.values.memory) {
        return Err(ValueTreeError::Invalid("JSON scalar parent pool/class"));
    }
    parent.validate_binding(profile, parent.root())?;
    if after.namespace != profile.namespace()? {
        return Err(invalid());
    }
    after.validate().map_err(|_| invalid())?;
    if work.max_pending_pairs == 0 {
        return Err(ValueTreeError::ResourceLimited("JSON scalar pending roots"));
    }
    let bytes = work
        .max_pending_pairs
        .checked_mul(std::mem::size_of::<Pending>() + 256)
        .ok_or(ValueTreeError::ResourceLimited(
            "JSON scalar queue admission",
        ))?;
    let _queue = context.values.memory.reserve(bytes, context.values.class)?;
    let retained = context.values.memory.reserve(1024, context.values.class)?;
    let mut pending = Vec::with_capacity(work.max_pending_pairs);
    pending.push(Pending {
        before: parent.root().clone(),
        after: after.clone(),
        depth: 0,
    });
    let mut records = context.records();
    while let Some(pair) = pending.pop() {
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        if pair.before == pair.after {
            continue;
        }
        if pair.depth > work.json.max_depth {
            return Err(ValueTreeError::ResourceLimited("JSON scalar changed depth"));
        }
        let old = read::frame(profile, &pair.before, &mut records, &mut work.json)?;
        let new = read::frame(profile, &pair.after, &mut records, &mut work.json)?;
        let container = old.kind.container();
        if container != new.kind.container()
            || old.length != new.length
            || (container && old.kind != new.kind)
            || pair.before.leaf_count != 5u64.checked_add(old.length).ok_or_else(invalid)?
            || pair.after.leaf_count != 5u64.checked_add(new.length).ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        incremental_record_diff::visit(
            &pair.before,
            &pair.after,
            &mut records,
            &mut work.paths_remaining,
            RecordDiffLabelsV1 {
                invalid: "JSON scalar diff",
                cells: "JSON scalar changed paths",
                stack: "JSON scalar diff stack",
                tree: LABELS,
            },
            |before, after, records| {
                let (before, after) = match (before, after) {
                    (Some(before), Some(after)) => (before, after),
                    _ => return Err(invalid()),
                };
                if let Some(index) = old.keys.iter().position(|key| key.as_bytes() == before.key) {
                    // Metadata, marker and container length are immutable here.
                    if !matches!(index, 2 | 3)
                        || container
                        || new.keys[index] != old.keys[index]
                        || before.value != old.ids[index]
                        || after.value != new.ids[index]
                    {
                        return Err(invalid());
                    }
                    return Ok(());
                }
                if !container {
                    return Err(invalid());
                }
                work.json.cells_remaining = work.json.cells_remaining.checked_sub(2).ok_or(
                    ValueTreeError::ResourceLimited("JSON scalar child references"),
                )?;
                let index = old.kind == read::Kind::Array;
                let child_before = read::child(profile, before, index, old.length, records)?;
                let child_after = read::child(profile, after, index, new.length, records)?;
                if child_before != child_after {
                    if pending.len() == work.max_pending_pairs {
                        return Err(ValueTreeError::ResourceLimited("JSON scalar pending roots"));
                    }
                    let depth = pair.depth.checked_add(1).ok_or_else(invalid)?;
                    if depth > work.json.max_depth {
                        return Err(ValueTreeError::ResourceLimited("JSON scalar changed depth"));
                    }
                    pending.push(Pending {
                        before: child_before,
                        after: child_after,
                        depth,
                    });
                }
                Ok(())
            },
        )?;
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let output = retained.retain(JsonSubtreeQualificationV1 {
        root: after.clone(),
        profile,
        nodes: parent.node_count(),
    });
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}
