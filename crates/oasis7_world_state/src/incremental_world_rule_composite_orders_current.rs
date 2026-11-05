//! Authenticated Current AgentOrders branch and selected-record readers.
//! Data qualification only; never business execution or World activation.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_typed_record_diff::{TypedCellRefV1, visit_entries},
};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
use oasis7_formal_protocol::incremental_state::StateEntryV1;
use oasis7_formal_protocol::incremental_value::ValueReadContext;
use std::collections::BTreeMap;
type Orders = BTreeMap<String, crate::world_rules::AgentIndustrialOrderV1>;

/// Private construction binds the real outer pointer and its typed changed closure.
pub struct CurrentAgentOrdersV1 {
    parent: [u8; 32],
    before: Option<StateRootV1>,
    body: Option<ReservedValue<StateRootV1>>,
    identity: PreparedGuard,
}
pub struct AgentOrderDirectoryWorkV1 {
    pub max_ids: usize,
    pub max_id_bytes: usize,
    pub comparison_bytes_remaining: u64,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current AgentOrders source")
}
fn profile() -> TypedRecordProfileV1 {
    WorldRuleCompositeBranchV1::AgentOrders.profile()
}

fn current_body<M, R, E, C>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let (field, present) = crate::incremental_world_rule_parent::read_parent_overlay(
        ctx.transaction,
        &mut records,
        &mut ctx.take_state_error,
    )?;
    let (key, len) = records.key(field, WorldFieldRecordKindV1::Entry, &("agent_orders", ()))?;
    let value = lookup(
        ctx.transaction,
        &key,
        &mut records,
        &mut ctx.take_state_error,
    )?;
    match (present, value) {
        (false, None) => Ok(None),
        (true, Some(id)) => Ok(Some(read_body::<Orders, _, _>(
            &key,
            len,
            &id,
            &mut records,
        )?)),
        _ => Err(bad().into()),
    }
}
fn validate<M, R, E, C>(
    source: &ReservedValue<CurrentAgentOrdersV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.validate_guard(guard, live)?;
    ctx.transaction
        .validate_transaction_identity(&source.identity)?;
    if !source.uses_budget(ctx.values.memory)
        || source.reserved_bytes() < 4096
        || ctx.values.class != MemoryClass::Ordinary
        || !ctx.transaction.uses_budget(ctx.values.memory)
        || source.parent != ctx.transaction.parent_fence().parent_roots
    {
        return Err(bad().into());
    }
    let body = current_body(ctx)?;
    if body.as_deref() != source.body.as_deref() {
        return Err(bad().into());
    }
    Ok(())
}
/// Read the actual Current outer pointer, then validate only its changed typed
/// closure against the complete parent. No whole-map DTO is retained.
#[allow(clippy::too_many_arguments)]
pub fn bind_current_agent_orders_v1<M, S, R, E, C>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Orders>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentAgentOrdersV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        parent.check_budget(ctx.values.memory, MemoryClass::Ordinary)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
        {
            return Err(bad().into());
        }
        // Keep this exact eventual source fee live across both digest calls,
        // empty-root creation and parent-root cloning; no temporary proof's
        // already-dropped phase is borrowed to cover these allocations.
        let fee = ctx
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(roots)?;
        if ctx.transaction.parent_fence().parent_roots != roots.digest().map_err(|_| bad())? {
            return Err(bad().into());
        }
        let body = current_body(ctx)?;
        let empty = StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| bad())?;
        let anchor = body.as_deref().or_else(|| parent.body()).unwrap_or(&empty);
        let mut read = WorldFieldReadContextV1 {
            root: anchor,
            state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        let trees = work.max_retained_trees;
        accept_body::<Orders, _, _, _, _>(
            &profile(),
            parent.body(),
            body.as_deref(),
            &mut ChangeDecoder {
                ctx: &mut read,
                scan,
                work,
                profile: profile(),
                trees_remaining: trees,
                tree_depth: 0,
            },
        )?;
        read.check()?;
        Ok(fee.retain(CurrentAgentOrdersV1 {
            parent: ctx.transaction.parent_fence().parent_roots,
            before: parent.body().cloned(),
            body,
            identity: ctx.transaction.guard(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

/// Decode exactly one real map entry using the original closed Order codec.
#[allow(clippy::too_many_arguments)]
pub fn read_current_agent_order_v1<M, S, R, E, C>(
    source: &ReservedValue<CurrentAgentOrdersV1>,
    id: &str,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    Option<RecoveredTypedRecordV1<crate::world_rules::AgentIndustrialOrderV1>>,
    WorldFieldOverlayErrorV1,
>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        validate(source, ctx, guard, live)?;
        let Some(body) = source.body.as_deref() else {
            return Ok(None);
        };
        let p = profile();
        let row = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            let (key, _) = records.key(p.field, WorldFieldRecordKindV1::Entry, &("entry", id))?;
            let Some(value) = incremental_record_tree::lookup(body, &key, &mut records, p.labels)?
            else {
                return Ok(None);
            };
            let _entry_fee = records
                .values
                .memory
                .reserve(
                    key.as_bytes().len() + std::mem::size_of::<StateEntryV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            crate::incremental_record_columns_read::read_column_row_v1(
                ColumnReadProfileV1 {
                    field: p.field,
                    schema: p.schema,
                    groups: &[],
                    reference_schema: p.reference_schema,
                },
                &StateEntryV1 {
                    namespace: body.namespace,
                    key: key.as_bytes().to_vec(),
                    value,
                },
                &mut records,
                work.max_tree_heap,
            )?
        };
        let ColumnCellV1::Root(child) = &row.cell else {
            return Err(bad().into());
        };
        let mut read = WorldFieldReadContextV1 {
            root: child,
            state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        Ok(Some(recover_typed_record_body_v1(
            child, &p, &mut read, scan, work,
        )?))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

fn entry_id<R: ValueNodeReader, C: FnMut() -> bool>(
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: usize,
) -> Result<Option<ReservedValue<ColumnKeyV1>>, ValueTreeError> {
    let p = profile();
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(None);
    }
    let key = crate::incremental_record_columns_read::read_column_key_v1(
        ColumnReadProfileV1 {
            field: p.field,
            schema: p.schema,
            groups: &[],
            reference_schema: p.reference_schema,
        },
        entry,
        records,
        max,
    )?;
    match &*key {
        ColumnKeyV1::Text(name) if name == "_length" => Ok(None),
        ColumnKeyV1::Pair(kind, id)
            if matches!(&**kind,ColumnKeyV1::Text(s) if s=="entry")
                && matches!(&**id, ColumnKeyV1::Text(_)) =>
        {
            Ok(Some(key))
        }
        _ => Err(bad()),
    }
}

/// GEN/cold enumerates the complete authenticated parent; ordinary callers use
/// the changed-ID API below and point-read candidates against Current.
#[allow(clippy::too_many_arguments)]
pub fn collect_parent_agent_order_ids_v1<M, S, R, E, C>(
    source: &ReservedValue<CurrentAgentOrdersV1>,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut TypedRecordReadWorkV1,
    work: &mut AgentOrderDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Vec<String>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    collect_ids(source, state, ctx, scan, typed, work, guard, live, true)
}
#[allow(clippy::too_many_arguments)]
pub fn collect_changed_agent_order_ids_v1<M, S, R, E, C>(
    source: &ReservedValue<CurrentAgentOrdersV1>,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut TypedRecordReadWorkV1,
    work: &mut AgentOrderDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Vec<String>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    collect_ids(source, state, ctx, scan, typed, work, guard, live, false)
}
#[allow(clippy::too_many_arguments)]
fn collect_ids<M, S, R, E, C>(
    source: &ReservedValue<CurrentAgentOrdersV1>,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut TypedRecordReadWorkV1,
    work: &mut AgentOrderDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    parent_only: bool,
) -> Result<ReservedValue<Vec<String>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        validate(source, ctx, guard, live)?;
        let bytes = work
            .max_ids
            .checked_mul(
                work.max_id_bytes
                    .checked_add(std::mem::size_of::<String>())
                    .ok_or_else(bad)?,
            )
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(bad)?;
        let comparisons = work
            .max_ids
            .checked_mul(work.max_ids)
            .and_then(|n| n.checked_mul(work.max_id_bytes))
            .ok_or_else(bad)?;
        work.comparison_bytes_remaining = work
            .comparison_bytes_remaining
            .checked_sub(u64::try_from(comparisons).map_err(|_| bad())?)
            .ok_or(ValueTreeError::ResourceLimited(
                "order directory comparisons",
            ))?;
        let fee = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut ids = Vec::with_capacity(work.max_ids);
        if ids.capacity() > work.max_ids {
            return Err(bad().into());
        }
        let empty = StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| bad())?;
        let before = if parent_only {
            &empty
        } else {
            source.before.as_ref().unwrap_or(&empty)
        };
        let after = if parent_only {
            source.before.as_ref().unwrap_or(&empty)
        } else {
            source.body.as_deref().unwrap_or(&empty)
        };
        let mut read = WorldFieldReadContextV1 {
            root: after,
            state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        let trees = typed.max_retained_trees;
        visit_entries(
            before,
            after,
            &mut ChangeDecoder {
                ctx: &mut read,
                scan,
                work: typed,
                profile: profile(),
                trees_remaining: trees,
                tree_depth: 0,
            },
            |old, new, d| {
                // The original map leaf identity is stable across replacements.
                let Some(TypedCellRefV1::Entry(entry)) = new.or(old) else {
                    return Err(bad());
                };
                let mut records = InventoryValueReader {
                    values: &mut d.ctx.values,
                    key_work: &mut *d.ctx.key_work,
                    semantic_bytes: d.ctx.semantic_bytes,
                    record_bytes: d.ctx.record_bytes,
                    cancelled: &mut d.ctx.cancelled,
                };
                if let Some(key) = entry_id(entry, &mut records, d.work.max_tree_heap)? {
                    let ColumnKeyV1::Pair(_, id) = &*key else {
                        return Err(bad());
                    };
                    let ColumnKeyV1::Text(id) = &**id else {
                        return Err(bad());
                    };
                    if id.len() > work.max_id_bytes || ids.len() == work.max_ids {
                        return Err(ValueTreeError::ResourceLimited("order directory IDs"));
                    }
                    let copy = id.to_owned();
                    if copy.capacity() > work.max_id_bytes {
                        return Err(bad());
                    }
                    ids.push(copy);
                }
                Ok(())
            },
        )?;
        if (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        ids.sort_unstable();
        ids.dedup();
        if (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(fee.retain(ids))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

#[cfg(test)]
#[path = "incremental_world_rule_composite_orders_current_tests.rs"]
mod tests;

#[path = "incremental_world_rule_composite_orders_index.rs"]
mod index;
pub use index::{CurrentAgentOrderIndexV1, read_current_agent_order_index_v1};
