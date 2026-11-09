//! Typed delivery cursor storage; business advancement remains with its owner.
use crate::{
    incremental_field_member::*, incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1, incremental_projection::WORLD_STATE_FIELDS,
    state::ProductValidationDeliveryCursor,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
fn descriptor() -> Result<FieldDescriptor, MemberRecordError> {
    WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|field| {
            field.name == "product_validation_delivery_cursor"
                && field.owner == FieldOwner::WorldState
                && field.shape == FieldShape::Value
                && field.destination == FieldDestination::State(StateNamespaceV1::Scheduler)
        })
        .ok_or(MemberRecordError::Invalid("delivery cursor registration"))
}
/// Both outputs remain provisional until the complete caller transaction succeeds.
pub fn prepare_delivery_cursor_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    cursor: &ProductValidationDeliveryCursor,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records_remaining: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    let field = descriptor()?;
    for metadata in [true, false] {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if *records_remaining == 0 {
            return Err(ValueTreeError::ResourceLimited("delivery cursor records").into());
        }
        *records_remaining -= 1;
        let (key, root) = if metadata {
            let record = write_field_metadata_v1(
                field,
                &mut PresenceRecordWriteContext {
                    sink: &mut *context.sink,
                    memory: context.memory,
                    io: &mut *context.io,
                    key_work: &mut *context.key_work,
                    limits: PresenceRecordLimitsV1 {
                        semantic_bytes: context.limits.semantic_bytes,
                        record_bytes: context.limits.record_bytes,
                    },
                    cancelled: &mut context.cancelled,
                },
            )?;
            (record.field_key, record.value_root)
        } else {
            let record = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &(),
                MemberPayloadRefV1::DeliveryCursor(cursor),
                context,
            )?;
            (record.field_key, record.value_root)
        };
        crate::incremental_field_write::emit(key, root, context, sink)?;
    }
    Ok(())
}
pub fn read_delivery_cursor_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<ProductValidationDeliveryCursor, ValueTreeError> {
    context.check()?;
    let field =
        descriptor().map_err(|_| ValueTreeError::Invalid("delivery cursor registration"))?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context
        .lookup(&metadata)?
        .ok_or(ValueTreeError::Invalid("delivery cursor missing metadata"))?;
    context.records().presence(&root, &metadata, 0)?;
    let (key, bytes) = context.key(field, WorldFieldRecordKindV1::Entry, &())?;
    let root = context
        .lookup(&key)?
        .ok_or(ValueTreeError::Invalid("delivery cursor missing entry"))?;
    let payload: [u8; 16] =
        context
            .records()
            .fixed(&root, &key, bytes, MemberPayloadCodecV1::DeliveryCursor)?;
    Ok(decode(payload))
}

fn decode(payload: [u8; 16]) -> ProductValidationDeliveryCursor {
    ProductValidationDeliveryCursor {
        event_id_era: u64::from_be_bytes(payload[..8].try_into().unwrap()),
        routed_through_event_id: u64::from_be_bytes(payload[8..].try_into().unwrap()),
    }
}
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_proto::memory_budget::MemoryClass;
fn resolve_overlay<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    transaction: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(WorldFieldKeyV1, ProductValidationDeliveryCursor), WorldFieldOverlayErrorV1> {
    let field = descriptor()?;
    let (key, root, bytes) =
        crate::incremental_field_overlay::required_member(field, transaction, records, error)?;
    let payload = records.fixed(&root, &key, bytes, MemberPayloadCodecV1::DeliveryCursor)?;
    Ok((key, decode(payload)))
}
pub fn read_delivery_cursor_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<ProductValidationDeliveryCursor, WorldFieldOverlayErrorV1> {
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        if records.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(records.values.memory)
        {
            return Err(ValueTreeError::Invalid("delivery cursor overlay pool/class").into());
        }
        resolve_overlay(transaction, &mut records, error).map(|(_, value)| value)
    })
}
/// Saves the original owner's final cursor; does not advance or dispatch work.
pub fn change_delivery_cursor_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    expected: &ProductValidationDeliveryCursor,
    after: &ProductValidationDeliveryCursor,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &oasis7_formal_protocol::incremental_execution::PreparedGuard,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let cancelled = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("delivery cursor overlay pool/class").into());
        }
        let (key, current) = {
            let mut cancel = || cancelled() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancel,
            };
            resolve_overlay(transaction, &mut records, error)?
        };
        if &current != expected {
            return Err(ValueTreeError::Invalid("delivery cursor expected value").into());
        }
        if &current == after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let output = write_member_record_v1(
            descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::DeliveryCursor(after),
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            },
        )?;
        if output.field_key != key {
            return Err(ValueTreeError::Invalid("delivery cursor writer key").into());
        }
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(
            StateNamespaceV1::Scheduler,
            key.as_bytes(),
            Some(output.value_root),
        )?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
