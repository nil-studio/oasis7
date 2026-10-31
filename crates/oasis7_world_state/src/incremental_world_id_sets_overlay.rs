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
    field: WorldIdSetFieldV1,
    key: WorldIdSetKeyRefV1<'_>,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(WorldFieldKeyV1, bool), WorldFieldOverlayErrorV1> {
    field.check_key(key)?;
    let field = field.descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(transaction, &metadata, records, error)?
        .ok_or(ValueTreeError::Invalid("world ID set missing metadata"))?;
    records.presence(&root, &metadata, 0)?;
    let (key, bytes) = match key {
        WorldIdSetKeyRefV1::Text(key) => records.key(field, WorldFieldRecordKindV1::Entry, key)?,
        WorldIdSetKeyRefV1::Action(key) => {
            records.key(field, WorldFieldRecordKindV1::Entry, &key)?
        }
    };
    let root = lookup(transaction, &key, records, error)?;
    if let Some(root) = root {
        let _: [u8; 0] = records.fixed(&root, &key, bytes, MemberPayloadCodecV1::Unit)?;
    }
    Ok((key, root.is_some()))
}
pub fn world_id_set_contains_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldIdSetFieldV1,
    key: WorldIdSetKeyRefV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<bool, WorldFieldOverlayErrorV1> {
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
            return Err(ValueTreeError::Invalid("world ID set overlay pool/class").into());
        }
        resolve(field, key, transaction, &mut records, error).map(|(_, member)| member)
    })
}
#[derive(Debug, Clone, Copy)]
pub struct WorldIdSetChangeV1<'a> {
    pub field: WorldIdSetFieldV1,
    pub key: WorldIdSetKeyRefV1<'a>,
    pub expected: bool,
    pub after: bool,
}
/// Saves the owning reducer's final membership; no settlement authorization.
pub fn change_world_id_set_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: WorldIdSetChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let WorldIdSetChangeV1 {
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
            return Err(ValueTreeError::Invalid("world ID set overlay pool/class").into());
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
            return Err(ValueTreeError::Invalid("world ID set expected value").into());
        }
        if current == after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let root = if after {
            let field = field.descriptor()?;
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            let output = match key {
                WorldIdSetKeyRefV1::Text(key) => write_member_record_v1(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    key,
                    MemberPayloadRefV1::Unit,
                    &mut combined,
                )?,
                WorldIdSetKeyRefV1::Action(key) => write_member_record_v1(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    &key,
                    MemberPayloadRefV1::Unit,
                    &mut combined,
                )?,
            };
            if output.field_key != encoded {
                return Err(ValueTreeError::Invalid("world ID set writer key").into());
            }
            Some(output.value_root)
        } else {
            None
        };
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(StateNamespaceV1::Authorization, encoded.as_bytes(), root)?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
