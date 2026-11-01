use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardValueRefV1<'a> {
    U64(u64),
    Text(&'a str),
}
impl<'a> RewardValueRefV1<'a> {
    fn text(self) -> &'a str {
        match self {
            Self::Text(v) => v,
            _ => "",
        }
    }
    fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::U64(v) => MemberPayloadRefV1::U64(v),
            Self::Text(v) => MemberPayloadRefV1::Text(v),
        }
    }
    fn validate(self, cell: RewardCellV1<'_>) -> Result<(), ValueTreeError> {
        let actual = match self {
            Self::U64(_) => MemberPayloadCodecV1::U64,
            Self::Text(_) => MemberPayloadCodecV1::Text,
        };
        if actual != cell.codec() {
            Err(read::invalid())
        } else {
            Ok(())
        }
    }
    fn matches(self, value: RewardValueV1) -> bool {
        match (self, value) {
            (Self::U64(a), RewardValueV1::Unsigned(b)) => a == b,
            (Self::Text(v), RewardValueV1::Text { utf8_bytes }) => v.len() as u64 == utf8_bytes,
            _ => false,
        }
    }
}
pub(super) fn metadata<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: RewardRecordKindV1,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let meta = WorldFieldKeyV1::new(kind.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(tx, &meta, records, error)?.ok_or_else(read::invalid)?;
    records.presence(&root, &meta, 0)?;
    let format = read::format_key(kind, records)?;
    let root = lookup(tx, &format.0, records, error)?.ok_or_else(read::invalid)?;
    read::primitive(
        MemberPayloadCodecV1::Unit,
        &root,
        &format,
        records,
        0,
        &mut std::io::sink(),
    )?;
    Ok(())
}
pub(super) struct Length {
    pub key: (WorldFieldKeyV1, u64),
    pub value: u64,
}
pub(super) fn length<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Length, WorldFieldOverlayErrorV1> {
    let key = read::length_key(records)?;
    let root = lookup(tx, &key.0, records, error)?.ok_or_else(read::invalid)?;
    let RewardValueV1::Unsigned(value) = read::primitive(
        MemberPayloadCodecV1::U64,
        &root,
        &key,
        records,
        8,
        &mut std::io::sink(),
    )?
    else {
        return Err(read::invalid().into());
    };
    Ok(Length { key, value })
}
pub(super) fn current<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: RewardRecordKeyV1,
    proof: &ReservedValue<RewardRecordQualificationV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(WorldFieldKeyV1, StateRootV1), WorldFieldOverlayErrorV1> {
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !proof.uses_budget(records.values.memory)
    {
        return Err(read::invalid().into());
    }
    metadata(key.kind, tx, records, error)?;
    if key.kind == RewardRecordKindV1::Mint && key.index >= length(tx, records, error)?.value {
        return Err(read::invalid().into());
    }
    let outer = read::outer_key(key, records)?;
    let root = lookup(tx, &outer.0, records, error)?.ok_or_else(read::invalid)?;
    let (index, child) = read::outer(key.kind, &outer.0, &root, records)?;
    if index != key.index {
        return Err(read::invalid().into());
    }
    proof.validate_binding(key, &child)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((outer.0, child))
}
struct Resolved {
    outer: WorldFieldKeyV1,
    child: StateRootV1,
    key: (WorldFieldKeyV1, u64),
    before: Option<[u8; 32]>,
    value: Option<RewardValueV1>,
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RewardCellRequestV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<Resolved, WorldFieldOverlayErrorV1> {
    if !request.cell.accepts(request.key.kind) {
        return Err(read::invalid().into());
    }
    let (outer, child) = current(request.key, request.qualification, tx, records, error)?;
    if let RewardCellV1::Member(c, _) = request.cell {
        let marker = read::cell_key(request.key.kind, RewardCellV1::Collection(c), records)?;
        let root =
            crate::incremental_record_tree::lookup(&child, &marker.0, records, genesis::LABELS)?
                .ok_or_else(read::invalid)?;
        read::primitive(
            MemberPayloadCodecV1::Unit,
            &root,
            &marker,
            records,
            0,
            &mut std::io::sink(),
        )?;
    }
    let key = read::cell_key(request.key.kind, request.cell, records)?;
    let before = crate::incremental_record_tree::lookup(&child, &key.0, records, genesis::LABELS)?;
    let value = match before {
        Some(root) => Some(read::primitive(
            request.cell.codec(),
            &root,
            &key,
            records,
            max,
            output,
        )?),
        None if matches!(request.cell, RewardCellV1::Member(..)) => None,
        None => return Err(read::invalid().into()),
    };
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Resolved {
        outer,
        child,
        key,
        before,
        value,
    })
}
pub fn read_qualified_reward_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RewardCellRequestV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: u64,
    output: &mut impl std::io::Write,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<RewardValueV1>, WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !request.qualification.uses_budget(ctx.values.memory)
        {
            return Err(read::invalid().into());
        }
        let _workspace = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        Ok(resolve(
            request,
            tx,
            &mut records,
            &mut ctx.take_state_error,
            max,
            output,
        )?
        .value)
    })
}
pub struct RewardCellChangeV1<'a> {
    pub key: RewardRecordKeyV1,
    pub cell: RewardCellV1<'a>,
    pub qualification: &'a ReservedValue<RewardRecordQualificationV1>,
    pub expected: Option<RewardValueRefV1<'a>>,
    pub after: Option<RewardValueRefV1<'a>>,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
pub fn change_qualified_reward_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: RewardCellChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<RewardRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !change.cell.accepts(change.key.kind)
            || matches!(
                change.cell,
                RewardCellV1::Record | RewardCellV1::Collection(_)
            )
            || ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !tx.uses_budget(write.memory)
            || !change.qualification.uses_budget(ctx.values.memory)
        {
            return Err(read::invalid().into());
        }
        if !matches!(change.cell, RewardCellV1::Member(..))
            && (change.expected.is_none() || change.after.is_none())
        {
            return Err(read::invalid().into());
        }
        for value in [change.expected, change.after].into_iter().flatten() {
            value.validate(change.cell)?;
        }
        let _workspace = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            let mut compare = Compare::new(
                change
                    .expected
                    .map_or("", RewardValueRefV1::text)
                    .as_bytes(),
            );
            let resolved = resolve(
                RewardCellRequestV1 {
                    key: change.key,
                    cell: change.cell,
                    qualification: change.qualification,
                },
                tx,
                &mut records,
                error,
                change.max_current_payload_bytes,
                &mut compare,
            )?;
            let matched = match (change.expected, resolved.value) {
                (None, None) => true,
                (Some(expected), Some(value)) => expected.matches(value),
                _ => false,
            };
            if !matched || !compare.is_equal() {
                return Err(read::invalid().into());
            }
            resolved
        };
        let unchanged = change.expected == change.after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(None);
        }
        let after = if let Some(after) = change.after {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let row = write_cell(change.key, change.cell, after.payload(), &mut combined)?;
            if row.field_key != resolved.key.0 {
                return Err(read::invalid().into());
            }
            Some(row.value_root)
        } else {
            None
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let child = crate::incremental_record_tree::edit(
            resolved.child,
            &resolved.key.0,
            (resolved.before, after),
            &mut ctx.values,
            write,
            cancel,
            crate::incremental_record_tree::RecordTreeWritePolicyV1 {
                max_pending_node_bytes: change.max_pending_node_bytes,
                labels: genesis::LABELS,
            },
        )?;
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            genesis::root(change.key, child.clone(), &mut combined)?
        };
        if row.field_key != resolved.outer {
            return Err(read::invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            change.key.kind.namespace(),
            resolved.outer.as_bytes(),
            Some(row.value_root),
        )?;
        let proof = closure::mint(change.key, child, write.memory, MemoryClass::Ordinary)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
