use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::MemoryClass;
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldNumberMapFieldV1,
    key: WorldNumberMapKeyRefV1<'_>,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(WorldFieldKeyV1, Option<WorldNumberV1>), WorldFieldOverlayErrorV1> {
    field.check_key(key)?;
    let signed = field.signed();
    let field = field.descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(transaction, &metadata, records, error)?
        .ok_or(ValueTreeError::Invalid("world number map missing metadata"))?;
    records.presence(&root, &metadata, 0)?;
    let (key, bytes) = match key {
        WorldNumberMapKeyRefV1::Text(key) => {
            records.key(field, WorldFieldRecordKindV1::Entry, key)?
        }
        WorldNumberMapKeyRefV1::Action(key) => {
            records.key(field, WorldFieldRecordKindV1::Entry, &key)?
        }
    };
    let root = lookup(transaction, &key, records, error)?;
    let value = root
        .map(|root| {
            if signed {
                records
                    .quantity(&root, &key, bytes)
                    .map(WorldNumberV1::Signed)
            } else {
                records
                    .unsigned(&root, &key, bytes)
                    .map(WorldNumberV1::Unsigned)
            }
        })
        .transpose()?;
    Ok((key, value))
}
pub fn read_world_number_map_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldNumberMapFieldV1,
    key: WorldNumberMapKeyRefV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<WorldNumberV1>, WorldFieldOverlayErrorV1> {
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
            return Err(ValueTreeError::Invalid("world number map overlay pool/class").into());
        }
        resolve(field, key, transaction, &mut records, error).map(|(_, member)| member)
    })
}
#[derive(Debug, Clone, Copy)]
pub struct WorldNumberMapChangeV1<'a> {
    pub field: WorldNumberMapFieldV1,
    pub key: WorldNumberMapKeyRefV1<'a>,
    pub expected: Option<WorldNumberV1>,
    pub after: Option<WorldNumberV1>,
}
/// Saves the owning reducer's final value; no settlement or ID authorization.
pub fn change_world_number_map_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: WorldNumberMapChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let WorldNumberMapChangeV1 {
        field,
        key,
        expected,
        after,
    } = change;
    let cancelled = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("world number map overlay pool/class").into());
        }
        for value in [expected, after].into_iter().flatten() {
            field.check_value(value)?;
        }
        let (encoded, current) = {
            let mut cancel = || cancelled() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancel,
            };
            resolve(field, key, transaction, &mut records, error)?
        };
        if current != expected {
            return Err(ValueTreeError::Invalid("world number map expected value").into());
        }
        if current == after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let root = if let Some(after) = after {
            let field = field.descriptor()?;
            let payload = match after {
                WorldNumberV1::Unsigned(v) => MemberPayloadRefV1::U64(v),
                WorldNumberV1::Signed(v) => MemberPayloadRefV1::I64(v),
            };
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            let output = match key {
                WorldNumberMapKeyRefV1::Text(key) => write_member_record_v1(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    key,
                    payload,
                    &mut combined,
                )?,
                WorldNumberMapKeyRefV1::Action(key) => write_member_record_v1(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    &key,
                    payload,
                    &mut combined,
                )?,
            };
            if output.field_key != encoded {
                return Err(ValueTreeError::Invalid("world number map writer key").into());
            }
            Some(output.value_root)
        } else {
            None
        };
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(field.namespace(), encoded.as_bytes(), root)?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
