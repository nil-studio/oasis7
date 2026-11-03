//! Original contents in one transaction; no transfer/mint authorization.
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
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
    field: WorldBalanceFieldV1,
    key: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max_identity_bytes: u64,
    output: &mut impl Write,
) -> Result<(WorldFieldKeyV1, Option<WorldBalanceReadV1>), WorldFieldOverlayErrorV1> {
    let descriptor = field.descriptor()?;
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(transaction, &metadata, records, error)?
        .ok_or(ValueTreeError::Invalid("world balance missing metadata"))?;
    records.presence(&root, &metadata, 0)?;
    let (key, bytes) = records.key(descriptor, WorldFieldRecordKindV1::Entry, key)?;
    let root = lookup(transaction, &key, records, error)?;
    let balance = root
        .map(|root| {
            let payload = max_identity_bytes
                .checked_add(32)
                .ok_or(ValueTreeError::ResourceLimited("balance identity limit"))?;
            read_identity_amounts(
                &root,
                &key,
                records.values,
                MemberReadProfileV1 {
                    codec: MemberPayloadCodecV1::IdentityU64Triple,
                    semantic_bytes: bytes,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: records.semantic_bytes,
                        payload_bytes: payload,
                        record_bytes: records.record_bytes,
                    },
                },
                output,
                records.cancelled,
            )
            .map(|(id, amounts)| summary(field, id, amounts))
        })
        .transpose()?;
    Ok((key, balance))
}
/// Retained identity output is caller-admitted and provisional until success.
pub fn read_world_balance_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldBalanceFieldV1,
    key: &str,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_identity_bytes: u64,
    output: &mut impl Write,
) -> Result<Option<WorldBalanceReadV1>, WorldFieldOverlayErrorV1> {
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
            return Err(ValueTreeError::Invalid("world balance overlay pool/class").into());
        }
        resolve(
            field,
            key,
            transaction,
            &mut records,
            error,
            max_identity_bytes,
            output,
        )
        .map(|(_, balance)| balance)
    })
}
#[derive(Debug, Clone, Copy)]
pub struct WorldBalanceChangeV1<'a> {
    pub field: WorldBalanceFieldV1,
    pub key: &'a str,
    pub expected: Option<WorldBalanceRefV1<'a>>,
    pub after: Option<WorldBalanceRefV1<'a>>,
    pub max_current_identity_bytes: u64,
}
/// Store only the original owner's final named contents/tombstone. The caller
/// admits borrowed requests and supplies all required policy/authority checks.
pub fn change_world_balance_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: WorldBalanceChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let WorldBalanceChangeV1 {
        field,
        key,
        expected,
        after,
        max_current_identity_bytes,
    } = change;
    let cancelled = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("world balance overlay pool/class").into());
        }
        if [expected, after]
            .into_iter()
            .flatten()
            .any(|v| v.field() != field)
        {
            return Err(ValueTreeError::Invalid("world balance value type").into());
        }
        let expected_bytes = expected
            .as_ref()
            .map_or(&[][..], |v| v.identity().as_bytes());
        let mut compare = Compare::new(expected_bytes);
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
                max_current_identity_bytes,
                &mut compare,
            )?
        };
        if current != expected.map(WorldBalanceRefV1::summary) || !compare.is_equal() {
            return Err(ValueTreeError::Invalid("world balance expected value").into());
        }
        if expected == after {
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
            let row = write_member_record_v1(
                field.descriptor()?,
                WorldFieldRecordKindV1::Entry,
                key,
                MemberPayloadRefV1::IdentityU64Triple {
                    identity: after.identity(),
                    amounts: after.amounts(),
                },
                &mut combined,
            )?;
            if row.field_key != encoded {
                return Err(ValueTreeError::Invalid("world balance writer key").into());
            }
            Some(row.value_root)
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
