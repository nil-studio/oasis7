//! Dense ordered-list edits touch only added/removed slots and the count.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub enum FactoryAuthorityAllowlistEditV1<'a> {
    Append(&'a [String]),
    Truncate(u64),
}
pub struct FactoryAuthorityAllowlistChangeV1<'a> {
    pub key: FactoryAuthorityRecordKeyV1<'a>,
    pub qualification: &'a ReservedValue<FactoryAuthorityQualificationV1>,
    pub expected_len: u64,
    pub edit: FactoryAuthorityAllowlistEditV1<'a>,
}
/// The complete current child qualification proves original density. Intermediate
/// child roots remain provisional until one final outer reference is written.
pub fn change_qualified_factory_allowlist_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: FactoryAuthorityAllowlistChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryAuthorityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<FactoryAuthorityQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if change.key.kind != FactoryAuthorityRecordKindV1::Site
            || ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !tx.uses_budget(write.memory)
            || !change.qualification.uses_budget(write.memory)
        {
            return Err(read::invalid().into());
        }
        let _workspace = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        work.spend()?;
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            overlay_read::resolve(
                FactoryAuthorityCellRequestV1 {
                    key: change.key,
                    cell: FactoryAuthorityCellV1::AuthorizedCount,
                    qualification: change.qualification,
                },
                tx,
                &mut records,
                error,
                8,
                &mut std::io::sink(),
            )?
        };
        let overlay_read::Resolved {
            outer,
            mut child,
            mut paths,
            roots,
            value,
        } = resolved;
        let Some(FactoryAuthorityValueV1::Unsigned(old)) = value else {
            return Err(read::invalid().into());
        };
        let count_key = paths[0].take().ok_or_else(read::invalid)?.1;
        let count_root = roots[0].ok_or_else(read::invalid)?;
        if old != change.expected_len {
            return Err(read::invalid().into());
        }
        let new = match &change.edit {
            FactoryAuthorityAllowlistEditV1::Append(items) => old
                .checked_add(u64::try_from(items.len()).map_err(|_| read::invalid())?)
                .ok_or_else(read::invalid)?,
            FactoryAuthorityAllowlistEditV1::Truncate(new) if *new <= old => *new,
            _ => return Err(read::invalid().into()),
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if old == new {
            return Ok(None);
        }
        let policy = RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: genesis::LABELS,
        };
        for index in old.min(new)..old.max(new) {
            work.spend()?;
            let cell = FactoryAuthorityCellV1::AuthorizedAgent(index);
            let (key, before) = {
                let mut combined = || cancel() || (write.cancelled)();
                let mut records = InventoryValueReader {
                    values: &mut ctx.values,
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    cancelled: &mut combined,
                };
                let key = read::cell_key(change.key.kind, cell, &mut records)?;
                let before =
                    incremental_record_tree::lookup(&child, &key.0, &mut records, genesis::LABELS)?;
                if before.is_some() != (new < old) {
                    return Err(read::invalid().into());
                }
                (key, before)
            };
            let after = match &change.edit {
                FactoryAuthorityAllowlistEditV1::Append(items) => {
                    let offset = usize::try_from(index - old).map_err(|_| read::invalid())?;
                    let item = items.get(offset).ok_or_else(read::invalid)?;
                    let mut combined = MemberRecordWriteContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        key_work: &mut *write.key_work,
                        limits: write.limits,
                        cancelled: || cancel() || (write.cancelled)(),
                    };
                    let row = write_cell(
                        change.key.kind,
                        cell,
                        MemberPayloadRefV1::Text(item),
                        &mut combined,
                    )?;
                    if row.field_key != key.0 {
                        return Err(read::invalid().into());
                    }
                    Some(row.value_root)
                }
                FactoryAuthorityAllowlistEditV1::Truncate(_) => None,
            };
            child = incremental_record_tree::edit(
                child,
                &key.0,
                (before, after),
                &mut ctx.values,
                write,
                cancel,
                policy,
            )?;
        }
        work.spend()?;
        let new_count = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            write_cell(
                change.key.kind,
                FactoryAuthorityCellV1::AuthorizedCount,
                MemberPayloadRefV1::U64(new),
                &mut combined,
            )?
        };
        if new_count.field_key != count_key.0 {
            return Err(read::invalid().into());
        }
        child = incremental_record_tree::edit(
            child,
            &count_key.0,
            (Some(count_root), Some(new_count.value_root)),
            &mut ctx.values,
            write,
            cancel,
            policy,
        )?;
        work.spend()?;
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            genesis::write_root(change.key, child.clone(), &mut combined)?
        };
        if row.field_key != outer {
            return Err(read::invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            change.key.kind.namespace(),
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let owner = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(read::invalid)?;
        let proof = closure::mint(
            change.key.kind,
            owner,
            child,
            write.memory,
            MemoryClass::Ordinary,
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
