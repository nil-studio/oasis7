//! Shared optional world_rules parent; never a complete children qualification.
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayErrorV1, required_member},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::WORLD_STATE_FIELDS,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use std::io::Cursor;
pub(crate) fn descriptor() -> Result<FieldDescriptor, ValueTreeError> {
    WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|field| {
            field.name == "world_rules"
                && field.owner == FieldOwner::WorldState
                && field.shape == FieldShape::Value
                && field.destination == FieldDestination::State(StateNamespaceV1::Authorization)
        })
        .ok_or(ValueTreeError::Invalid("world rules registration"))
}
fn presence<R: ValueNodeReader>(
    root: &[u8; 32],
    key: &WorldFieldKeyV1,
    semantic_bytes: u64,
    values: &mut ValueReadContext<'_, R>,
    record_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<bool, ValueTreeError> {
    let mut bytes = [0u8; 1];
    read_member_record_profile_v1(
        root,
        key,
        values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Bool,
            semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes,
                payload_bytes: 1,
                record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    match bytes[0] {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ValueTreeError::Invalid("world rules presence bool")),
    }
}

pub(crate) fn read_parent_fixed<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(FieldDescriptor, bool), ValueTreeError> {
    context.check()?;
    let field = descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context
        .lookup(&metadata)?
        .ok_or(ValueTreeError::Invalid("world rules missing metadata"))?;
    context.records().presence(&root, &metadata, 0)?;
    let (parent, bytes) = context.key(field, WorldFieldRecordKindV1::Entry, &())?;
    let root = context
        .lookup(&parent)?
        .ok_or(ValueTreeError::Invalid("world rules missing parent"))?;
    let present = presence(
        &root,
        &parent,
        bytes,
        &mut context.values,
        context.record_bytes,
        &mut context.cancelled,
    )?;
    Ok((field, present))
}

pub(crate) fn read_parent_overlay<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(FieldDescriptor, bool), WorldFieldOverlayErrorV1> {
    let field = descriptor()?;
    let (parent, root, bytes) = required_member(field, transaction, records, error)?;
    let present = presence(
        &root,
        &parent,
        bytes,
        records.values,
        records.record_bytes,
        records.cancelled,
    )?;
    Ok((field, present))
}
