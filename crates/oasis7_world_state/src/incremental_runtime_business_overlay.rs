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
    field: RuntimeBusinessFieldV1,
    key: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(WorldFieldKeyV1, Option<RuntimeBusinessNumberV1>), WorldFieldOverlayErrorV1> {
    field.check(
        key,
        match field {
            RuntimeBusinessFieldV1::LogisticsSlaMetrics => RuntimeBusinessNumberV1::Unsigned(0),
            RuntimeBusinessFieldV1::ThreatHeatmap => RuntimeBusinessNumberV1::Signed(0),
        },
    )?;
    let signed = field == RuntimeBusinessFieldV1::ThreatHeatmap;
    let required = !signed;
    let field = field.descriptor();
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(transaction, &metadata, records, error)?
        .ok_or(ValueTreeError::Invalid("Runtime business missing metadata"))?;
    records.presence(&root, &metadata, 0)?;
    let (key, bytes) = records.key(field, WorldFieldRecordKindV1::Entry, key)?;
    let root = lookup(transaction, &key, records, error)?;
    if required && root.is_none() {
        return Err(invalid().into());
    }
    let value = root
        .map(|root| {
            if signed {
                records
                    .quantity(&root, &key, bytes)
                    .map(RuntimeBusinessNumberV1::Signed)
            } else {
                records
                    .unsigned(&root, &key, bytes)
                    .map(RuntimeBusinessNumberV1::Unsigned)
            }
        })
        .transpose()?;
    Ok((key, value))
}
pub fn read_runtime_business_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: RuntimeBusinessFieldV1,
    key: &str,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<RuntimeBusinessNumberV1>, WorldFieldOverlayErrorV1> {
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
            return Err(ValueTreeError::Invalid("Runtime business overlay pool/class").into());
        }
        resolve(field, key, transaction, &mut records, error).map(|(_, member)| member)
    })
}
#[derive(Debug, Clone, Copy)]
pub struct RuntimeBusinessChangeV1<'a> {
    pub field: RuntimeBusinessFieldV1,
    pub key: &'a str,
    pub expected: Option<RuntimeBusinessNumberV1>,
    pub after: Option<RuntimeBusinessNumberV1>,
}
/// Saves the owning reducer's final value; no settlement or ID authorization.
pub fn change_runtime_business_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: RuntimeBusinessChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let RuntimeBusinessChangeV1 {
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
            return Err(ValueTreeError::Invalid("Runtime business overlay pool/class").into());
        }
        for value in [expected, after].into_iter().flatten() {
            field.check(key, value)?;
        }
        if field == RuntimeBusinessFieldV1::LogisticsSlaMetrics
            && (expected.is_none() || after.is_none())
        {
            return Err(invalid().into());
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
            return Err(ValueTreeError::Invalid("Runtime business expected value").into());
        }
        if current == after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let root = if let Some(after) = after {
            let field = field.descriptor();
            let payload = match after {
                RuntimeBusinessNumberV1::Unsigned(v) => MemberPayloadRefV1::U64(v),
                RuntimeBusinessNumberV1::Signed(v) => MemberPayloadRefV1::I64(v),
            };
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            let output = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                key,
                payload,
                &mut combined,
            )?;
            if output.field_key != encoded {
                return Err(ValueTreeError::Invalid("Runtime business writer key").into());
            }
            Some(output.value_root)
        } else {
            None
        };
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(StateNamespaceV1::Business, encoded.as_bytes(), root)?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
