use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub struct RewardPoolCreateV1<'a> {
    pub epoch_key: u64,
    pub source: &'a SystemOrderPoolBudget,
    pub family: &'a ReservedValue<RewardFamilyQualificationV1>,
}
pub struct RewardMintAppendV1<'a> {
    pub expected_length: u64,
    pub source: &'a NodeRewardMintRecord,
    pub family: &'a ReservedValue<RewardFamilyQualificationV1>,
}
pub fn create_qualified_reward_pool_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    request: RewardPoolCreateV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RewardRecordsWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<RewardRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let key = RewardRecordKeyV1 {
        kind: RewardRecordKindV1::Pool,
        index: request.epoch_key,
    };
    create(
        Create {
            key,
            source: RewardRecordRefV1::Pool(request.source),
            family: request.family,
            expected_length: None,
        },
        ctx,
        write,
        work,
        guard,
        live,
    )
}
/// Append at the exact current length; no old ordinal or unchanged record is rewritten.
pub fn append_qualified_reward_mint_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    request: RewardMintAppendV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RewardRecordsWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<RewardRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let key = RewardRecordKeyV1 {
        kind: RewardRecordKindV1::Mint,
        index: request.expected_length,
    };
    create(
        Create {
            key,
            source: RewardRecordRefV1::Mint(request.source),
            family: request.family,
            expected_length: Some(request.expected_length),
        },
        ctx,
        write,
        work,
        guard,
        live,
    )
}
struct Create<'a> {
    key: RewardRecordKeyV1,
    source: RewardRecordRefV1<'a>,
    family: &'a ReservedValue<RewardFamilyQualificationV1>,
    expected_length: Option<u64>,
}
fn create<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    request: Create<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RewardRecordsWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<RewardRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let Create {
        key,
        source,
        family,
        expected_length,
    } = request;
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !tx.uses_budget(write.memory)
            || !family.uses_budget(ctx.values.memory)
            || source.kind() != key.kind
        {
            return Err(read::invalid().into());
        }
        family.validate_binding(&tx.parent_fence().parent_roots)?;
        let _workspace = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (outer, length) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            point::metadata(key.kind, tx, &mut records, error)?;
            let outer = read::outer_key(key, &mut records)?;
            if lookup(tx, &outer.0, &mut records, error)?.is_some() {
                return Err(read::invalid().into());
            }
            let length = if let Some(expected) = expected_length {
                let point::Length {
                    key: path,
                    value: actual,
                } = point::length(tx, &mut records, error)?;
                if actual != expected || actual != key.index {
                    return Err(read::invalid().into());
                }
                Some((path, actual.checked_add(1).ok_or_else(read::invalid)?))
            } else {
                None
            };
            (outer.0, length)
        };
        let (child, row) = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let child = genesis::body(key, source, &mut combined, work)?;
            work.cell()?;
            let row = genesis::root(key, child.clone(), &mut combined)?;
            (child, row)
        };
        if row.field_key != outer {
            return Err(read::invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(key.kind.namespace(), outer.as_bytes(), Some(row.value_root))?;
        if let Some((path, length)) = length {
            work.cell()?;
            let row = {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                write_member_record_v1(
                    RewardRecordKindV1::Mint.descriptor()?,
                    WorldFieldRecordKindV1::Entry,
                    &(FORMAT, "length"),
                    MemberPayloadRefV1::U64(length),
                    &mut combined,
                )?
            };
            if row.field_key != path.0 {
                return Err(read::invalid().into());
            }
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            tx.write(
                RewardRecordKindV1::Mint.namespace(),
                row.field_key.as_bytes(),
                Some(row.value_root),
            )?;
        }
        let proof = closure::mint(key, child, write.memory, MemoryClass::Ordinary)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(proof)
    })
}
pub struct RewardPoolDeleteV1<'a> {
    pub epoch_key: u64,
    pub qualification: &'a ReservedValue<RewardRecordQualificationV1>,
}
pub fn delete_qualified_reward_pool_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RewardPoolDeleteV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let _workspace = ctx
            .values
            .memory
            .reserve(16 << 10, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let key = RewardRecordKeyV1 {
            kind: RewardRecordKindV1::Pool,
            index: request.epoch_key,
        };
        let (outer, _) = point::current(
            key,
            request.qualification,
            tx,
            &mut records,
            &mut ctx.take_state_error,
        )?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(key.kind.namespace(), outer.as_bytes(), None)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
