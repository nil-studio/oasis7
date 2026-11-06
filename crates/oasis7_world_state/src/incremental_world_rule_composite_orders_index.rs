//! Narrow directory view over one genuine Current order. No quote hydration.
//! Prepared as a child of orders_current; intentionally not hooked during builds.
use super::*;
use crate::incremental_record_columns_read::ColumnRowV1;
use crate::incremental_typed_record_diff::point_typed_column_v1;

/// Plain selected data, never a family or business qualification. Strings keep
/// their original per-row admission; there is no cloned or uncharged ID buffer.
pub struct CurrentAgentOrderIndexV1 {
    phase: crate::world_rules::AgentIndustrialPhaseV1,
    factory: ReservedValue<ColumnRowV1>,
    target: ReservedValue<ColumnRowV1>,
    delivered_at: Option<u64>,
}
impl CurrentAgentOrderIndexV1 {
    pub fn phase(&self) -> crate::world_rules::AgentIndustrialPhaseV1 {
        self.phase
    }
    pub fn factory_id(&self) -> &str {
        match &self.factory.cell {
            ColumnCellV1::Text(value) => value,
            _ => unreachable!("constructor checks original factory column"),
        }
    }
    pub fn target_agent_id(&self) -> &str {
        match &self.target.cell {
            ColumnCellV1::Text(value) => value,
            _ => unreachable!("constructor checks original target column"),
        }
    }
    pub fn delivered_at(&self) -> Option<u64> {
        self.delivered_at
    }
}

/// Actual absent map entry returns None. Missing required selected columns,
/// wrong cells, stale Current sources and resource failures remain errors.
#[allow(clippy::too_many_arguments)]
pub fn read_current_agent_order_index_v1<M, S, R, E, C>(
    source: &ReservedValue<CurrentAgentOrdersV1>,
    id: &str,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    max_text_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<CurrentAgentOrderIndexV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        validate(source, ctx, guard, live)?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let Some(root) = source.body.as_deref() else {
            return Ok(None);
        };
        let fee = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentAgentOrderIndexV1>() + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let p = profile();
        let order = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            let (key, _) = records.key(p.field, WorldFieldRecordKindV1::Entry, &("entry", id))?;
            let Some(value) = incremental_record_tree::lookup(root, &key, &mut records, p.labels)?
            else {
                return Ok(None);
            };
            // The fixed output metadata reservation above covers this short
            // authenticated wire key before the entry Vec is constructed.
            crate::incremental_record_columns_read::read_column_row_v1(
                ColumnReadProfileV1 {
                    field: p.field,
                    schema: p.schema,
                    groups: &[],
                    reference_schema: p.reference_schema,
                },
                &StateEntryV1 {
                    namespace: root.namespace,
                    key: key.as_bytes().to_vec(),
                    value,
                },
                &mut records,
                work.max_tree_heap,
            )?
        };
        let ColumnCellV1::Root(body) = &order.cell else {
            return Err(bad().into());
        };
        let mut read = WorldFieldReadContextV1 {
            root: body,
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
        let mut d = ChangeDecoder {
            ctx: &mut read,
            scan,
            work,
            profile: p,
            trees_remaining: trees,
            tree_depth: 0,
        };
        let max = d.work.max_tree_heap;
        let row = point_typed_column_v1(&p, body, "phase", &mut d, max)?;
        let ColumnCellV1::Root(phase_root) = &row.cell else {
            return Err(bad().into());
        };
        // Decode only the original tiny closed enum, not a handwritten tag map.
        let phase = recover_typed_record_body_v1::<
            crate::world_rules::AgentIndustrialPhaseV1,
            _,
            _,
            _,
            _,
        >(phase_root, &p, &mut *d.ctx, &mut *d.scan, &mut *d.work)?;
        let phase = *phase.value();
        let factory = point_typed_column_v1(&p, body, "factory_id", &mut d, max)?;
        let target = point_typed_column_v1(&p, body, "target_agent_id", &mut d, max)?;
        for row in [&factory, &target] {
            let ColumnCellV1::Text(value) = &row.cell else {
                return Err(bad().into());
            };
            if value.len() > max_text_bytes {
                return Err(ValueTreeError::ResourceLimited("order index identity text").into());
            }
        }
        let delivered = point_typed_column_v1(&p, body, "delivered_at", &mut d, max)?;
        let delivered_at = match &delivered.cell {
            ColumnCellV1::Unit => None,
            ColumnCellV1::U64(value) => Some(*value),
            _ => return Err(bad().into()),
        };
        d.check()?;
        Ok(Some(fee.retain(CurrentAgentOrderIndexV1 {
            phase,
            factory,
            target,
            delivered_at,
        })))
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
#[path = "incremental_world_rule_composite_orders_index_tests.rs"]
mod tests;
