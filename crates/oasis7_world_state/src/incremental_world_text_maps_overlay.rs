//! Transactional text contents; identity and account policy stay with the owner.
use super::*;
use crate::incremental_expected_bytes::Compare;
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
    field: WorldTextMapFieldV1,
    key: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<(WorldFieldKeyV1, Option<u64>), WorldFieldOverlayErrorV1> {
    let descriptor = field.descriptor()?;
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(transaction, &metadata, records, error)?
        .ok_or(ValueTreeError::Invalid("world text map missing metadata"))?;
    records.presence(&root, &metadata, 0)?;
    let (key, bytes) = records.key(descriptor, WorldFieldRecordKindV1::Entry, key)?;
    let value = lookup(transaction, &key, records, error)?;
    let length = value
        .map(|root| {
            read_text_value(
                &root,
                &key,
                records.values,
                MemberReadProfileV1 {
                    codec: MemberPayloadCodecV1::Text,
                    semantic_bytes: bytes,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: records.semantic_bytes,
                        payload_bytes: max_payload_bytes,
                        record_bytes: records.record_bytes,
                    },
                },
                output,
                "world text map UTF-8",
                records.cancelled,
            )
        })
        .transpose()?;
    Ok((key, length))
}
/// Output remains provisional until the whole call succeeds. Retained bytes
/// and the request key require caller admission, including on an absent read.
pub fn read_world_text_map_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldTextMapFieldV1,
    key: &str,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<Option<u64>, WorldFieldOverlayErrorV1> {
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
            return Err(ValueTreeError::Invalid("world text map overlay pool/class").into());
        }
        resolve(
            field,
            key,
            transaction,
            &mut records,
            error,
            max_payload_bytes,
            output,
        )
        .map(|(_, length)| length)
    })
}
#[derive(Debug, Clone, Copy)]
pub struct WorldTextMapChangeV1<'a> {
    pub field: WorldTextMapFieldV1,
    pub key: &'a str,
    pub expected: Option<&'a str>,
    pub after: Option<&'a str>,
    pub max_current_payload_bytes: u64,
}
/// Borrow expected/after without allocating the existing value. Caller admits
/// all request strings; the setter grants no owner identity or account rights.
pub fn change_world_text_map_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: WorldTextMapChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let WorldTextMapChangeV1 {
        field,
        key,
        expected,
        after,
        max_current_payload_bytes,
    } = change;
    let cancelled = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("world text map overlay pool/class").into());
        }
        let mut compare = Compare::new(expected.unwrap_or("").as_bytes());
        let (encoded, current) = {
            let mut cancel = || cancelled() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancel,
            };
            resolve(
                field,
                key,
                transaction,
                &mut records,
                error,
                max_current_payload_bytes,
                &mut compare,
            )?
        };
        if current.is_some() != expected.is_some() || !compare.is_equal() {
            return Err(ValueTreeError::Invalid("world text map expected value").into());
        }
        if expected == after {
            // Comparing two long borrowed strings can outlast the read's last
            // cancellation check. A no-op must not leave prior work sealable.
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let root = if let Some(after) = after {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            let output = write_member_record_v1(
                field.descriptor()?,
                WorldFieldRecordKindV1::Entry,
                key,
                MemberPayloadRefV1::Text(after),
                &mut combined,
            )?;
            if output.field_key != encoded {
                return Err(ValueTreeError::Invalid("world text map writer key").into());
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
