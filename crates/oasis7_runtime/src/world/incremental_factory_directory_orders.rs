//! Complete original order-key selection without materializing any quote.
//! Parent construction is GEN/cold only; ordinary merge uses actual child diff.
use crate::world::{
    incremental_body::BodyMutationError, incremental_world_event_counter::WorldEventCounterV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_world_rule_composite::WorldRuleCompositeReadWorkV1,
    incremental_world_rule_composite_family::{
        AgentOrderDirectoryWorkV1, CurrentAgentOrderIndexV1, CurrentAgentOrdersV1,
        collect_changed_agent_order_ids_v1, collect_parent_agent_order_ids_v1,
        read_current_agent_order_index_v1, validate_current_agent_orders_v1,
    },
};
use std::sync::Arc;

struct CachedOrderIndexV1 {
    index: ReservedValue<CurrentAgentOrderIndexV1>,
    _allocation: MemoryReservation,
}
fn cache_index(
    index: ReservedValue<CurrentAgentOrderIndexV1>,
    memory: &MemoryBudget,
) -> Result<Arc<CachedOrderIndexV1>, ValueTreeError> {
    if !index.uses_budget(memory) {
        return Err(ValueTreeError::Invalid("order index pool"));
    }
    let allocation = memory
        .reserve(
            std::mem::size_of::<CachedOrderIndexV1>() + 2 * std::mem::size_of::<usize>() + 128,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(Arc::new(CachedOrderIndexV1 {
        index,
        _allocation: allocation,
    }))
}

#[derive(Debug)]
pub enum FactoryOrderDirectoryErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Body(BodyMutationError),
}
impl From<ExecutionError> for FactoryOrderDirectoryErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<ValueTreeError> for FactoryOrderDirectoryErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<WorldFieldOverlayErrorV1> for FactoryOrderDirectoryErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
impl From<BodyMutationError> for FactoryOrderDirectoryErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}

pub struct FactoryOrderDirectoryWorkV1 {
    pub ids: AgentOrderDirectoryWorkV1,
    pub pending_entries_remaining: usize,
    pub merge_comparison_bytes_remaining: u64,
    pub max_index_text_bytes: usize,
}
/// No constructor, Clone or mutable ID access. The actual accepted parent is
/// established before its complete original order keys can be cached.
pub struct AcceptedFactoryOrderDirectoryV1 {
    commit: [u8; 32],
    roots: [u8; 32],
    ids: ReservedValue<Vec<String>>,
    indices: Vec<Arc<CachedOrderIndexV1>>,
}
/// Holds the genuine validated source alive. The snapshot covers ALL orders,
/// including delivered/terminal entries needed by original identity selection.
pub struct CurrentFactoryOrderDirectoryV1<'source> {
    commit: [u8; 32],
    roots: [u8; 32],
    identity: PreparedGuard,
    memory: MemoryBudget,
    source: &'source ReservedValue<CurrentAgentOrdersV1>,
    ids: Vec<String>,
    indices: Vec<Arc<CachedOrderIndexV1>>,
}
impl<'source> CurrentFactoryOrderDirectoryV1<'source> {
    pub fn iter_order_ids(&self) -> impl ExactSizeIterator<Item = &str> {
        self.ids.iter().map(String::as_str)
    }
    pub fn iter_producing_order_ids(&self) -> impl Iterator<Item = &str> {
        self.ids
            .iter()
            .zip(&self.indices)
            .filter_map(|(id, cached)| {
                (cached.index.phase()
                    == oasis7_world_state::world_rules::AgentIndustrialPhaseV1::Producing)
                    .then_some(id.as_str())
            })
    }
    pub fn iter_order_indices(
        &self,
    ) -> impl ExactSizeIterator<Item = (&str, &CurrentAgentOrderIndexV1)> {
        self.ids
            .iter()
            .zip(&self.indices)
            .map(|(id, cached)| (id.as_str(), &*cached.index))
    }
    pub fn source(&self) -> &'source ReservedValue<CurrentAgentOrdersV1> {
        self.source
    }
    /// Must precede use after unrelated contributions. Rechecks the actual
    /// Current branch, not merely the original parent or a revision boolean.
    pub fn validate_current<M, R, E, C>(
        &self,
        ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), FactoryOrderDirectoryErrorV1>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        let result = (|| {
            ctx.transaction.validate_guard(guard, live)?;
            ctx.transaction
                .validate_transaction_identity(&self.identity)?;
            let fence = ctx.transaction.parent_fence();
            if !self.memory.same_pool(ctx.values.memory)
                || !ctx.transaction.uses_budget(ctx.values.memory)
                || ctx.values.class != MemoryClass::Ordinary
                || self.commit != fence.parent_commit
                || self.roots != fence.parent_roots
                || !self.source.uses_budget(ctx.values.memory)
            {
                return Err(ValueTreeError::Invalid("Current order directory binding").into());
            }
            validate_current_agent_orders_v1(self.source, ctx, guard, live)?;
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
#[allow(clippy::too_many_arguments)]
pub fn build_accepted_factory_order_directory_v1<M, S, R, E, C>(
    parent: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    head: &VerifiedFormalCommitV1,
    source: &ReservedValue<CurrentAgentOrdersV1>,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldRuleCompositeReadWorkV1,
    work: &mut FactoryOrderDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<AcceptedFactoryOrderDirectoryV1>, FactoryOrderDirectoryErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
        {
            return Err(ValueTreeError::Invalid("accepted order directory pool/class").into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                work.ids
                    .max_ids
                    .checked_mul(std::mem::size_of::<Arc<CachedOrderIndexV1>>())
                    .and_then(|n| n.checked_add(4096))
                    .ok_or(ValueTreeError::ResourceLimited("order index vector"))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let _accepted =
            WorldEventCounterV1::begin(parent, head, ctx.transaction, ctx.values.memory)?;
        let mut changed = false;
        for namespace in [
            StateNamespaceV1::Business,
            StateNamespaceV1::Allocator,
            StateNamespaceV1::Scheduler,
            StateNamespaceV1::Authorization,
            StateNamespaceV1::Effects,
            StateNamespaceV1::Artifacts,
        ] {
            ctx.transaction.visit_pending_writes(
                namespace,
                &mut work.pending_entries_remaining,
                || {
                    if (ctx.cancelled)() {
                        Err(ExecutionError::Aborted)
                    } else {
                        Ok(())
                    }
                },
                |_, _, _| {
                    changed = true;
                    Ok::<(), ExecutionError>(())
                },
            )?;
        }
        if changed {
            return Err(ValueTreeError::Invalid(
                "accepted order directory requires pristine parent",
            )
            .into());
        }
        let guard = ctx.transaction.guard();
        let ids = collect_parent_agent_order_ids_v1(
            source,
            state,
            ctx,
            scan,
            typed,
            &mut work.ids,
            &guard,
            live,
        )?;
        let mut indices = Vec::with_capacity(work.ids.max_ids);
        if indices.capacity() > work.ids.max_ids {
            return Err(ValueTreeError::ResourceLimited("order index vector capacity").into());
        }
        for id in ids.iter() {
            let guard = ctx.transaction.guard();
            let index = read_current_agent_order_index_v1(
                source,
                id,
                state,
                ctx,
                scan,
                typed,
                work.max_index_text_bytes,
                &guard,
                live,
            )?
            .ok_or(ValueTreeError::Invalid("accepted order index absent"))?;
            indices.push(cache_index(index, ctx.values.memory)?);
        }
        let fence = ctx.transaction.parent_fence();
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(fee.retain(AcceptedFactoryOrderDirectoryV1 {
            commit: fence.parent_commit,
            roots: fence.parent_roots,
            ids,
            indices,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
/// Parent candidates plus the complete actual nested changed-key set. The
/// narrow index decides real deletion/phase without decoding any order quote.
#[allow(clippy::too_many_arguments)]
pub fn open_current_factory_order_directory_v1<'source, M, S, R, E, C>(
    parent: &ReservedValue<AcceptedFactoryOrderDirectoryV1>,
    source: &'source ReservedValue<CurrentAgentOrdersV1>,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldRuleCompositeReadWorkV1,
    work: &mut FactoryOrderDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentFactoryOrderDirectoryV1<'source>>, FactoryOrderDirectoryErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let fence = ctx.transaction.parent_fence();
        if !parent.uses_budget(ctx.values.memory)
            || parent.reserved_bytes() < 4096
            || ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || parent.commit != fence.parent_commit
            || parent.roots != fence.parent_roots
            || !source.uses_budget(ctx.values.memory)
        {
            return Err(ValueTreeError::Invalid("order directory parent binding").into());
        }
        let capacity = work
            .ids
            .max_ids
            .checked_mul(2)
            .ok_or(ValueTreeError::ResourceLimited("order directory capacity"))?;
        let bytes = capacity
            .checked_mul(
                std::mem::size_of::<String>()
                    .checked_add(work.ids.max_id_bytes)
                    .ok_or(ValueTreeError::ResourceLimited("order directory capacity"))?,
            )
            .and_then(|n| {
                work.ids
                    .max_ids
                    .checked_mul(std::mem::size_of::<Arc<CachedOrderIndexV1>>())
                    .and_then(|v| n.checked_add(v))
            })
            .and_then(|n| n.checked_add(4096))
            .ok_or(ValueTreeError::ResourceLimited("order directory capacity"))?;
        let comparisons = capacity
            .checked_mul(capacity)
            .and_then(|n| n.checked_mul(work.ids.max_id_bytes.max(1)))
            .and_then(|n| n.checked_mul(8))
            .ok_or(ValueTreeError::ResourceLimited(
                "order directory comparison bound",
            ))?;
        work.merge_comparison_bytes_remaining =
            work.merge_comparison_bytes_remaining
                .checked_sub(u64::try_from(comparisons).map_err(|_| {
                    ValueTreeError::ResourceLimited("order directory comparison bound")
                })?)
                .ok_or(ValueTreeError::ResourceLimited(
                    "order directory comparison work",
                ))?;
        let fee = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let changes = collect_changed_agent_order_ids_v1(
            source,
            state,
            ctx,
            scan,
            typed,
            &mut work.ids,
            guard,
            live,
        )?;
        let mut ids = Vec::with_capacity(capacity);
        let mut indices = Vec::with_capacity(work.ids.max_ids);
        if ids.capacity() > capacity || indices.capacity() > work.ids.max_ids {
            return Err(ValueTreeError::ResourceLimited("order directory actual capacity").into());
        }
        for id in parent.ids.iter().chain(changes.iter()) {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if id.len() > work.ids.max_id_bytes || ids.len() == capacity {
                return Err(
                    ValueTreeError::ResourceLimited("order directory candidate IDs").into(),
                );
            }
            let copy = id.to_owned();
            if copy.capacity() > work.ids.max_id_bytes {
                return Err(
                    ValueTreeError::ResourceLimited("order directory identity capacity").into(),
                );
            }
            ids.push(copy);
        }
        ids.sort_unstable();
        ids.dedup();
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut retained = 0;
        for index in 0..ids.len() {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let cached = if changes.binary_search(&ids[index]).is_ok() {
                let guard = ctx.transaction.guard();
                read_current_agent_order_index_v1(
                    source,
                    &ids[index],
                    state,
                    ctx,
                    scan,
                    typed,
                    work.max_index_text_bytes,
                    &guard,
                    live,
                )?
                .map(|value| cache_index(value, ctx.values.memory))
                .transpose()?
            } else {
                let position = parent
                    .ids
                    .binary_search(&ids[index])
                    .map_err(|_| ValueTreeError::Invalid("uncertified order identity"))?;
                Some(Arc::clone(parent.indices.get(position).ok_or(
                    ValueTreeError::Invalid("accepted order index missing"),
                )?))
            };
            if let Some(cached) = cached {
                if retained == work.ids.max_ids {
                    return Err(
                        ValueTreeError::ResourceLimited("order directory current IDs").into(),
                    );
                }
                indices.push(cached);
                ids.swap(retained, index);
                retained += 1;
            }
        }
        ids.truncate(retained);
        // Stable compaction keeps the sorted candidate order; no phase-only
        // subset is substituted for the full target/identity lookup universe.
        let directory = fee.retain(CurrentFactoryOrderDirectoryV1 {
            commit: fence.parent_commit,
            roots: fence.parent_roots,
            identity: ctx.transaction.guard(),
            memory: ctx.values.memory.clone(),
            source,
            ids,
            indices,
        });
        let guard = ctx.transaction.guard();
        directory.validate_current(ctx, &guard, live)?;
        Ok(directory)
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
