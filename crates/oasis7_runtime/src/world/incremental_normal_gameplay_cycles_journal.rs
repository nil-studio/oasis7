//! Actual retained Journal cut: accepted parent positions plus this transaction's
//! typed counted notes. Numeric event IDs intentionally retain the old rollover
//! behavior; an era is never substituted for the original numeric predicate.
use super::*;
use crate::world::{
    incremental_module_invocation::*,
    incremental_normal_industry_due::delivery::{
        read_gameplay_journal_position_v1, read_gameplay_world_event_original_v1,
    },
    incremental_world_event_counter::CountedWorldEventSourceV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::ExecutionFactKindV1,
    incremental_material::OriginalObjectStreamReader,
    incremental_schedule::SchedulePageWriter,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_manifest::RuntimeHistoryLaneV1,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_world_event_decode::DecodedWorldEventV1,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("gameplay retained Journal source")
}
/// Source owners remain alive until every selected emit has been decoded and
/// applied. No caller ModuleEmit/JSON can construct this private cut.
pub(super) struct GameplayEmitCutV1 {
    rows: std::vec::IntoIter<ReservedValue<DecodedWorldEventV1>>,
    _heap: ReservedValue<()>,
}
impl Iterator for GameplayEmitCutV1 {
    type Item = ReservedValue<DecodedWorldEventV1>;
    fn next(&mut self) -> Option<Self::Item> {
        self.rows.next()
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter + SchedulePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn validate_gameplay_journal_v1(
        &self,
        history: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        sources: &NormalGameplayCycleSourcesV1<'_>,
        common: &mut FormalBoundedContextV1,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        common.charge(1024, 4096).map_err(|e| {
            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
        })?;
        let occurrence = history.occurrence();
        let window = &history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];
        let index = history.history_index(RuntimeHistoryLaneV1::Journal);
        if !history.uses_budget(common.memory())
            || history.allocation_class() != MemoryClass::Ordinary
            || occurrence.kind() != ExecutionFactKindV1::Result
            || occurrence.head_record_id() != self.families.head.verified_record_id()
            || occurrence.head_body_id() != occurrence.fact_body_id()
            || occurrence.roots_id() != history.manifest().roots
            || sources.roots.world_id.len() > 256
            || sources.roots.state.len() != 6
            || sources.roots.digest()? != history.manifest().roots
            || self.ctx.transaction.parent_fence().parent_roots != history.manifest().roots
            || index.window() != window
            || index.lane() != RuntimeHistoryLaneV1::Journal
            || !index.uses_budget(common.memory())
            || index.allocation_class() != MemoryClass::Ordinary
            || !sources.retention.uses_budget(common.memory())
            || sources.retention.allocation_class() != MemoryClass::Ordinary
            || !self.ctx.values.memory.same_pool(common.memory())
        {
            return Err(bad().into());
        }
        self.events
            .world_events
            .bind_journal_retention_v1(self.ctx.transaction, sources.retention)
            .map_err(RootModuleExecutionErrorV1::Body)?;
        Ok(())
    }
    pub(in crate::world) fn gameplay_journal_notes_v1(
        &self,
        parent_count: u64,
        common: &mut FormalBoundedContextV1,
    ) -> Result<ReservedValue<Vec<CountedWorldEventSourceV1>>, RootModuleExecutionErrorV1<O::Error>>
    {
        let appended = self
            .events
            .world_events
            .journal_append_height_v1(self.ctx.transaction)
            .map_err(RootModuleExecutionErrorV1::Body)?
            .checked_sub(parent_count)
            .ok_or_else(bad)?;
        let count = usize::try_from(appended).map_err(|_| bad())?;
        let bytes = count
            .checked_mul(std::mem::size_of::<CountedWorldEventSourceV1>())
            .and_then(|n| n.checked_add(std::mem::size_of::<Vec<CountedWorldEventSourceV1>>()))
            .ok_or_else(bad)?;
        common.charge(count as u64, bytes as u64).map_err(|e| {
            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
        })?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut notes = Vec::new();
        notes.try_reserve_exact(count).map_err(|_| bad())?;
        if notes.capacity() > count {
            return Err(bad().into());
        }
        self.events
            .world_events
            .visit_counted_sources_since(self.ctx.transaction, 0, |source| {
                common
                    .consume_steps(1)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                source.validate(self.ctx.transaction)?;
                if notes.len() >= count {
                    return Err(BodyMutationError::Invalid("gameplay counted note closure"));
                }
                notes.push(source);
                Ok(())
            })
            .map_err(RootModuleExecutionErrorV1::Body)?;
        if notes.len() != count {
            return Err(bad().into());
        }
        Ok(fee.retain(notes))
    }
    pub(super) fn gameplay_journal_start_id_v1<
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        history: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        sources: &NormalGameplayCycleSourcesV1<'_>,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut NormalGameplayCycleWorkV1,
        common: &mut FormalBoundedContextV1,
    ) -> Result<u64, RootModuleExecutionErrorV1<O::Error>> {
        self.validate_gameplay_journal_v1(history, sources, common)?;
        let window = &history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];
        let notes = self.gameplay_journal_notes_v1(window.count, common)?;
        let (id, encoded) = if let Some(last) = notes.first() {
            (*last.original(), None)
        } else if window.count > 0 {
            let row = read_gameplay_journal_position_v1(
                history,
                window.end()?.checked_sub(1).ok_or_else(bad)?,
                read,
                &mut work.journal,
                common,
            )
            .map_err(|e| RootModuleExecutionErrorV1::ProductDelivery(e.into()))?;
            (row.location.original, Some(row.encoded_bytes))
        } else {
            return Ok(0);
        };
        let (event, _) = read_gameplay_world_event_original_v1(
            &id,
            encoded,
            self.originals,
            &mut work.journal,
            common,
            &mut self.ctx.cancelled,
        )
        .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
        Ok(event.event().id)
    }
    pub(super) fn collect_gameplay_emits_v1<
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        history: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        sources: &NormalGameplayCycleSourcesV1<'_>,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut NormalGameplayCycleWorkV1,
        start_id: u64,
        common: &mut FormalBoundedContextV1,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<GameplayEmitCutV1, RootModuleExecutionErrorV1<O::Error>> {
        self.validate_gameplay_journal_v1(history, sources, common)?;
        let window = &history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];
        let notes = self.gameplay_journal_notes_v1(window.count, common)?;
        let total = window
            .count
            .checked_add(notes.len() as u64)
            .ok_or_else(bad)?;
        let retained = total.min(sources.retention.max_events() as u64);
        let floor = total - retained;
        let capacity = usize::try_from(retained).map_err(|_| bad())?;
        let bytes = capacity
            .checked_mul(std::mem::size_of::<ReservedValue<DecodedWorldEventV1>>())
            .ok_or_else(bad)?;
        let heap = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let mut rows = Vec::new();
        rows.try_reserve_exact(capacity).map_err(|_| bad())?;
        if rows.capacity() > capacity {
            return Err(bad().into());
        }
        for position in floor..total {
            common.consume_steps(1).map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            let (id, encoded) = if position < window.count {
                let row = read_gameplay_journal_position_v1(
                    history,
                    window.start.checked_add(position).ok_or_else(bad)?,
                    read,
                    &mut work.journal,
                    common,
                )
                .map_err(|e| RootModuleExecutionErrorV1::ProductDelivery(e.into()))?;
                (row.location.original, Some(row.encoded_bytes))
            } else {
                let ordinal = usize::try_from(position - window.count).map_err(|_| bad())?;
                let source = notes
                    .get(notes.len().checked_sub(ordinal + 1).ok_or_else(bad)?)
                    .ok_or_else(bad)?;
                source
                    .validate(self.ctx.transaction)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                (*source.original(), None)
            };
            let (event, _) = read_gameplay_world_event_original_v1(
                &id,
                encoded,
                self.originals,
                &mut work.journal,
                common,
                &mut self.ctx.cancelled,
            )
            .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
            if event.event().id <= start_id {
                continue;
            }
            let crate::WorldEventBody::ModuleEmitted(emit) = &event.event().body else {
                continue;
            };
            common
                .charge(3, (emit.trace_id.len() + emit.kind.len()) as u64)
                .map_err(|e| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
                })?;
            if !(emit.trace_id.starts_with("tick-") || emit.trace_id.starts_with("infra-tick-"))
                || emit.kind != kernel::GAMEPLAY_LIFECYCLE_EMIT_KIND
            {
                continue;
            }
            // Registry-active role is resolved at the post-route cut, before the first
            // directive mutation, just like collect_gameplay_tick_emits' owned list.
            let selected =
                self.gameplay_manifest_selection_v1(&emit.module_id, true, common, live)?;
            if selected
                .as_ref()
                .is_some_and(|s| s.manifest().role == oasis7_wasm_abi::ModuleRole::Gameplay)
            {
                rows.push(event);
            }
        }
        Ok(GameplayEmitCutV1 {
            rows: rows.into_iter(),
            _heap: heap,
        })
    }
}
