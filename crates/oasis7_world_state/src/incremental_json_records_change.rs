//! Changed-path JSON qualification. The scalar API preserves every container;
//! the structural API validates member/count deltas and array tail boundaries.
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
    advance_json_scalar_subtree_records_v1(profile, parent, after, &mut context.records(), work)
}

pub(crate) fn advance_json_scalar_subtree_records_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    parent: &ReservedValue<JsonSubtreeQualificationV1>,
    after: &StateRootV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonScalarChangeWorkV1,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    advance_json_scalar_subtree_records_constrained_v1(profile, parent, after, records, work, None)
}
pub(crate) fn advance_json_scalar_subtree_records_constrained_v1<
    R: ValueNodeReader,
    C: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    parent: &ReservedValue<JsonSubtreeQualificationV1>,
    after: &StateRootV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonScalarChangeWorkV1,
    preserve_kind_depth: Option<usize>,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    advance_records(
        profile,
        parent,
        after,
        records,
        work,
        Mode::Scalar(preserve_kind_depth),
    )
}
#[derive(Clone, Copy)]
enum Mode {
    Scalar(Option<usize>),
    Structural(Option<JsonImmediateShapeV1>),
}
/// Same continuous changed-path/node budgets as the scalar operation.
pub type JsonChangeWorkV1 = JsonScalarChangeWorkV1;
pub fn advance_json_subtree_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    parent: &ReservedValue<JsonSubtreeQualificationV1>,
    after: &StateRootV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonChangeWorkV1,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    context.check()?;
    advance_json_subtree_records_v1(profile, parent, after, &mut context.records(), work, None)
}
pub(crate) fn advance_json_subtree_records_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    parent: &ReservedValue<JsonSubtreeQualificationV1>,
    after: &StateRootV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonChangeWorkV1,
    immediate: Option<JsonImmediateShapeV1>,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    advance_records(
        profile,
        parent,
        after,
        records,
        work,
        Mode::Structural(immediate),
    )
}
fn needed_kind(shape: JsonImmediateShapeV1, kind: read::Kind) -> bool {
    matches!(
        (shape, kind),
        (JsonImmediateShapeV1::Object, read::Kind::Object)
            | (JsonImmediateShapeV1::Unsigned, read::Kind::U64)
    )
}
fn full_at<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    root: &StateRootV1,
    depth: usize,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    let max = work.max_depth;
    work.max_depth = max
        .checked_sub(depth)
        .ok_or(ValueTreeError::ResourceLimited("JSON changed depth"))?;
    let result = read::qualify_records(profile, root, records, work);
    work.max_depth = max;
    result
}
fn advance_records<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    parent: &ReservedValue<JsonSubtreeQualificationV1>,
    after: &StateRootV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonScalarChangeWorkV1,
    mode: Mode,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if records.values.class != MemoryClass::Ordinary || !parent.uses_budget(records.values.memory) {
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
    let _queue = records.values.memory.reserve(bytes, records.values.class)?;
    let retained = records.values.memory.reserve(1024, records.values.class)?;
    let mut pending = Vec::with_capacity(work.max_pending_pairs);
    pending.push(Pending {
        before: parent.root().clone(),
        after: after.clone(),
        depth: 0,
    });
    let mut nodes = parent.node_count_upper_bound();
    let mut exact = parent.exact;
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
        if let Mode::Structural(immediate) = mode {
            work.json.cells_remaining = work
                .json
                .cells_remaining
                .checked_sub(2)
                .ok_or(ValueTreeError::ResourceLimited("JSON changed kind cells"))?;
            let old_kind = read::kind(profile, &pair.before, records)?;
            let new_kind = read::kind(profile, &pair.after, records)?;
            if pair.depth == 1 && immediate.is_some_and(|shape| !needed_kind(shape, new_kind)) {
                return Err(invalid());
            }
            if !old_kind.container() || old_kind != new_kind {
                // A qualified old scalar has one node. A type switch replaces
                // this complete node; no old large scalar payload is read.
                work.json.json_nodes_remaining = work
                    .json
                    .json_nodes_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("JSON changed old node"))?;
                let new = full_at(profile, &pair.after, pair.depth, records, &mut work.json)?;
                if pair.depth == 0 {
                    nodes = new.node_count_upper_bound();
                    exact = new.exact;
                } else if old_kind.container() || new_kind.container() {
                    nodes = nodes
                        .checked_add(new.node_count_upper_bound())
                        .ok_or(ValueTreeError::ResourceLimited("JSON changed node bound"))?;
                    exact = false;
                }
                continue;
            }
        }
        work.json.cells_remaining = work
            .json
            .cells_remaining
            .checked_sub(10)
            .ok_or(ValueTreeError::ResourceLimited("JSON scalar fixed cells"))?;
        let old = read::frame(profile, &pair.before, records, &mut work.json)?;
        let new = read::frame(profile, &pair.after, records, &mut work.json)?;
        if matches!(mode,Mode::Scalar(Some(depth)) if depth==pair.depth) && old.kind != new.kind {
            return Err(invalid());
        }
        let container = old.kind.container();
        if container != new.kind.container()
            || (matches!(mode, Mode::Scalar(_)) && old.length != new.length)
            || (container && old.kind != new.kind)
            || pair.before.leaf_count != 5u64.checked_add(old.length).ok_or_else(invalid)?
            || pair.after.leaf_count != 5u64.checked_add(new.length).ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        let mut added = 0u64;
        let mut removed = 0u64;
        incremental_record_diff::visit(
            &pair.before,
            &pair.after,
            records,
            &mut work.paths_remaining,
            RecordDiffLabelsV1 {
                invalid: "JSON scalar diff",
                cells: "JSON scalar changed paths",
                stack: "JSON scalar diff stack",
                tree: LABELS,
            },
            |before, after, records| {
                let entry = before.or(after).ok_or_else(invalid)?;
                if let Some(index) = old.keys.iter().position(|key| key.as_bytes() == entry.key) {
                    let (Some(before), Some(after)) = (before, after) else {
                        return Err(invalid());
                    };
                    let mutable = match mode {
                        Mode::Scalar(_) => matches!(index, 2 | 3) && !container,
                        Mode::Structural(_) => index == 4 && container,
                    };
                    if !mutable
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
                if matches!(mode, Mode::Scalar(_)) && (before.is_none() || after.is_none()) {
                    return Err(invalid());
                }
                let references = u64::from(before.is_some()) + u64::from(after.is_some());
                work.json.cells_remaining =
                    work.json.cells_remaining.checked_sub(references).ok_or(
                        ValueTreeError::ResourceLimited("JSON scalar child references"),
                    )?;
                let index = old.kind == read::Kind::Array;
                let old_child = before
                    .map(|entry| {
                        read::child_with_ordinal(profile, entry, index, old.length, records)
                    })
                    .transpose()?;
                let new_child = after
                    .map(|entry| {
                        read::child_with_ordinal(profile, entry, index, new.length, records)
                    })
                    .transpose()?;
                let depth = pair.depth.checked_add(1).ok_or_else(invalid)?;
                if depth > work.json.max_depth {
                    return Err(ValueTreeError::ResourceLimited("JSON scalar changed depth"));
                }
                match (old_child, new_child) {
                    (Some((before, old_index)), Some((after, new_index))) => {
                        if old_index != new_index {
                            return Err(invalid());
                        }
                        if before != after {
                            if pending.len() == work.max_pending_pairs {
                                return Err(ValueTreeError::ResourceLimited(
                                    "JSON scalar pending roots",
                                ));
                            }
                            pending.push(Pending {
                                before,
                                after,
                                depth,
                            });
                        }
                    }
                    (None, Some((after, ordinal))) => {
                        if index && ordinal.is_none_or(|ordinal| ordinal < old.length) {
                            return Err(invalid());
                        }
                        if let Mode::Structural(Some(shape)) = mode
                            && depth == 1
                            && !needed_kind(shape, read::kind(profile, &after, records)?)
                        {
                            return Err(invalid());
                        }
                        let child = full_at(profile, &after, depth, records, &mut work.json)?;
                        nodes = nodes
                            .checked_add(child.node_count_upper_bound())
                            .ok_or(ValueTreeError::ResourceLimited("JSON changed node bound"))?;
                        exact = false;
                        added = added.checked_add(1).ok_or_else(invalid)?;
                    }
                    (Some((_, ordinal)), None) => {
                        if index && ordinal.is_none_or(|ordinal| ordinal < new.length) {
                            return Err(invalid());
                        }
                        exact = false;
                        removed = removed.checked_add(1).ok_or_else(invalid)?;
                    }
                    _ => return Err(invalid()),
                }
                Ok(())
            },
        )?;
        if old
            .length
            .checked_add(added)
            .and_then(|length| length.checked_sub(removed))
            != Some(new.length)
        {
            return Err(invalid());
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let output = retained.retain(JsonSubtreeQualificationV1 {
        root: after.clone(),
        profile,
        nodes,
        exact,
    });
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}

/// A concrete owner's root-container constraint, checked on a fully qualified
/// JSON graph. This does not infer or validate that owner's business schema.
pub(crate) fn require_json_container_records_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    proof: &ReservedValue<JsonSubtreeQualificationV1>,
    array: bool,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<(), ValueTreeError> {
    if !proof.uses_budget(records.values.memory) {
        return Err(ValueTreeError::Invalid("JSON root-container pool"));
    }
    proof.validate_binding(profile, proof.root())?;
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(5)
        .ok_or(ValueTreeError::ResourceLimited("JSON root-container cells"))?;
    let frame = read::frame(profile, proof.root(), records, work)?;
    let expected = if array {
        read::Kind::Array
    } else {
        read::Kind::Object
    };
    if frame.kind != expected {
        return Err(ValueTreeError::Invalid("JSON root-container kind"));
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum JsonImmediateShapeV1 {
    Object,
    Unsigned,
}
/// Validate a concrete owner's homogeneous immediate rows after full JSON
/// closure. The private proof excludes unknown nodes; no business rule is added.
pub(crate) fn require_json_immediate_records_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    proof: &ReservedValue<JsonSubtreeQualificationV1>,
    shape: JsonImmediateShapeV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<(), ValueTreeError> {
    if !proof.uses_budget(records.values.memory) {
        return Err(ValueTreeError::Invalid("JSON immediate rows pool"));
    }
    proof.validate_binding(profile, proof.root())?;
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(5)
        .ok_or(ValueTreeError::ResourceLimited("JSON immediate root cells"))?;
    let mut root = read::frame(profile, proof.root(), records, work)?;
    if root.kind != read::Kind::Object {
        return Err(ValueTreeError::Invalid("JSON immediate root object"));
    }
    while let Some(entry) = root.cursor.next(records)? {
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("JSON immediate rows"))?;
        if root.keys.iter().any(|key| key.as_bytes() == entry.key) {
            continue;
        }
        let child = read::child(profile, &entry, false, root.length, records)?;
        work.cells_remaining =
            work.cells_remaining
                .checked_sub(5)
                .ok_or(ValueTreeError::ResourceLimited(
                    "JSON immediate child cells",
                ))?;
        let child = read::frame(profile, &child, records, work)?;
        let expected = match shape {
            JsonImmediateShapeV1::Object => read::Kind::Object,
            JsonImmediateShapeV1::Unsigned => read::Kind::U64,
        };
        if child.kind != expected {
            return Err(ValueTreeError::Invalid("JSON immediate child kind"));
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
