//! Closed adapter over the original marketplace decoders and invariants.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_diff::{self as record_diff, RecordDiffLabelsV1},
    incremental_record_tree::RecordTreeLabelsV1,
};
use oasis7_proto::memory_budget::ReservedValue;
const LABELS: RecordDiffLabelsV1 = RecordDiffLabelsV1 {
    invalid: "marketplace child delta",
    cells: "marketplace changed cells",
    stack: "marketplace diff stack",
    tree: RecordTreeLabelsV1 {
        read: "marketplace diff read",
        pending: "marketplace diff pending",
        tree: "marketplace diff tree",
        digest: "marketplace diff digest",
        read_allowance: "marketplace diff read allowance",
        admission: "marketplace diff admission",
        bound: "marketplace diff bound",
        producer_pool: "marketplace diff pool",
        page: "marketplace diff page",
        write_allowance: "marketplace diff writes",
    },
};
fn bad() -> WorldStateOwnerError {
    WorldStateOwnerError::Invalid("market2 family bridge")
}
pub(crate) fn child_root<R: ValueNodeReader>(
    field: FieldDescriptor,
    entry: &StateEntryV1,
    values: &mut ValueReadContext<'_, R>,
    semantic_limit: u64,
    max: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<StateRootV1>, WorldStateOwnerError> {
    let owner = field_from_descriptor(field).ok_or_else(bad)?;
    let _phase = values
        .memory
        .reserve(4096, values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let charge = values
        .memory
        .reserve(512, values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(WorldStateOwnerError::Invalid)?;
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    crate::incremental_field_member::read_member_record_header_v1(
        &entry.value,
        &key,
        values,
        MemberRecordLimitsV1 {
            semantic_bytes: semantic_limit,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: max,
        },
        &mut *cancelled,
    )
    .map_err(WorldStateOwnerError::Value)?;
    let root = read_outer_child(owner, field, entry, parsed, values, max, cancelled)?;
    if cancelled() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    Ok(charge.retain(root))
}
pub(crate) fn full_child<M: MerkleNodeReader, R: ValueNodeReader, C: FnMut() -> bool>(
    field: FieldDescriptor,
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    values: &mut ValueReadContext<'_, R>,
    keys: &mut SemanticKeyBudget,
    max: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    if !reader.source_matches(values.memory, values.class) {
        return Err(bad());
    }
    match field_from_descriptor(field).ok_or_else(bad)? {
        ModuleMarketplaceFieldV1::ArtifactListings => {
            validate_listing(child, reader, field, values, max, cancelled)
        }
        ModuleMarketplaceFieldV1::ArtifactBids => {
            bids::validate_bids(child, reader, field, values, keys, max, cancelled)
        }
        ModuleMarketplaceFieldV1::VisualEntities => {
            validate_visual(child, reader, field, values, max, cancelled)
        }
    }
}
fn listing_row<R: ValueNodeReader>(
    entry: &StateEntryV1,
    field: FieldDescriptor,
    values: &mut ValueReadContext<'_, R>,
    max: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    let (semantic, value) = read_child_member(entry, field, values, max, cancelled)?;
    let valid = matches!(
        (semantic, value),
        (
            ChildSemantic::Field("order_id" | "listed_at"),
            ChildValue::U64(_)
        ) | (ChildSemantic::Field("seller_agent_id"), ChildValue::Text)
            | (ChildSemantic::Field("price_amount"), ChildValue::I64(_))
    ) || matches!(
        (semantic, value),
        (ChildSemantic::Field("price_kind"), ChildValue::U64(0 | 1))
    );
    if !valid {
        return Err(bad());
    }
    Ok(())
}
pub(crate) fn listing_delta<R: ValueNodeReader, C: FnMut() -> bool>(
    field: FieldDescriptor,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    max: u64,
) -> Result<(), WorldStateOwnerError> {
    if field_from_descriptor(field) != Some(ModuleMarketplaceFieldV1::ArtifactListings)
        || before.leaf_count != 5
        || after.leaf_count != 5
    {
        return Err(bad());
    }
    let mut original = None;
    let result = record_diff::visit(before, after, records, cells, LABELS, |_, new, records| {
        if let Some(new) = new {
            if let Err(error) = listing_row(new, field, records.values, max, records.cancelled) {
                original = Some(error);
                return Err(ValueTreeError::Invalid("marketplace listing changed row"));
            }
        }
        Ok(())
    });
    if (records.cancelled)() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    if let Some(error) = original {
        return Err(error);
    }
    result.map_err(WorldStateOwnerError::Value)
}

#[path = "incremental_owner_marketplace_delta_bids.rs"]
mod delta_bids;
pub(crate) use delta_bids::bids_delta;

#[path = "incremental_owner_marketplace_delta_visual.rs"]
mod delta_visual;
pub(crate) use delta_visual::visual_delta;
