//! True appended WorldEvents and actual retention to immutable projection DATA.
//! Reuses the already decoded event originals retained by final capture; old
//! journal branches are shared and only the actual removed prefix is dropped.
use super::owner::AcceptedNormalProjectionCacheV1;
use super::*;
use crate::world::{
    incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
    incremental_normal_event_commitment::NormalEventCommitmentV1,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_world_projection::current_commitment::{
    ProjectionColumnDataV1, ProjectionColumnErrorV1,
};
use std::mem::size_of;
fn graph_error(
    e: ProjectionColumnErrorV1<oasis7_formal_protocol::controlled_authority::formal::FormalError>,
) -> BodyMutationError {
    match e {
        ProjectionColumnErrorV1::Work(e) => paid(e),
        ProjectionColumnErrorV1::Memory(e) => ValueTreeError::Memory(e).into(),
        ProjectionColumnErrorV1::Invalid(e) => invalid(e),
    }
}
pub(in crate::world) struct ActualCurrentJournalProjectionV1 {
    events: ProjectionColumnDataV1,
    next_event_id: u64,
    next_event_id_era: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    parent_record: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl ActualCurrentJournalProjectionV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(paid)?;
        common.charge(1, 32).map_err(paid)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.parent_record != parent.refs().head.verified_record_id()
            || self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
            || !self.events.uses_budget(common.memory())
            || tx.parent_fence() != parent.fence()
        {
            return Err(invalid("current journal actual parent/context"));
        }
        Ok(())
    }
    /// These are the authenticated allocator scalars captured with the same
    /// terminal source, never inferred from a potentially empty retained suffix.
    pub(in crate::world) fn next_event_id_v1(&self) -> u64 { self.next_event_id }
    pub(in crate::world) fn next_event_id_era_v1(&self) -> u64 { self.next_event_id_era }
    pub(in crate::world) fn event_columns_v1(&self) -> &ProjectionColumnDataV1 {
        &self.events
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_current_journal_projection_v1<M: MerkleNodeReader>(
    parent: &BoundNormalTickParentV1<'_>,
    cache: &AcceptedNormalProjectionCacheV1,
    metadata: &NormalTerminalReadMetadataV1,
    appended: &NormalEventCommitmentV1,
    tx: &ExecutionTransaction<'_, M>,
    max_event_json_bytes: usize,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ActualCurrentJournalProjectionV1, BodyMutationError> {
    cache.validate_parent_v1(parent, common)?;
    metadata.validate_provenance(tx, common, live)?;
    if tx.parent_fence() != parent.fence() {
        return Err(invalid("journal actual transaction parent"));
    }
    let memory = common.memory().clone();
    let bytes = size_of::<ActualCurrentJournalProjectionV1>();
    common.charge(1, bytes as u64).map_err(paid)?;
    let fee = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut events = cache.world_journal_events_v1(parent, common)?;
    let before = events
        .array_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
        .map_err(graph_error)?;
    let expected = parent.refs().runtime.manifest().lanes
        [oasis7_world_state::incremental_runtime_history_manifest::RuntimeHistoryLaneV1::Journal
            .index()]
    .count;
    if u64::try_from(before).map_err(|_| invalid("journal parent count"))? != expected {
        return Err(invalid("journal cache differs from actual parent window"));
    }
    let end = before
        .checked_add(appended.count())
        .ok_or_else(|| invalid("journal append count"))?;
    let retained = usize::try_from(metadata.journal_retained_event_count_v1())
        .map_err(|_| invalid("journal retained count"))?;
    let removed = end
        .checked_sub(retained)
        .ok_or_else(|| invalid("journal retention adds nonexistent events"))?;
    appended.visit_appended_events_v1(tx, common, live, |event, common| {
        // The unique original typed JSON writer counts first and refuses its actual
        // output bound before allocating. Only this new event is parsed into DATA.
        let json = super::initial::capture_bounded_v1(common, max_event_json_bytes, |output| {
            serde_json::to_writer(output, event).map_err(paid)
        })?;
        let graph = json
            .len()
            .checked_mul(128)
            .and_then(|n| n.checked_add(32768))
            .ok_or_else(|| invalid("journal event JSON graph layout"))?;
        common
            .charge(
                u64::try_from(json.len()).map_err(|_| invalid("journal JSON length"))?,
                graph as u64,
            )
            .map_err(paid)?;
        let _phase = memory
            .reserve(graph, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let value = serde_json::from_slice(&json).map_err(paid)?;
        let value =
            ProjectionColumnDataV1::from_json_data(value, &memory, &mut |s, b| common.charge(s, b))
                .map_err(graph_error)?;
        events = events
            .with_array_append_data_v1(&value, &memory, &mut |s, b| common.charge(s, b))
            .map_err(graph_error)?;
        Ok(())
    })?;
    if events
        .array_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
        .map_err(graph_error)?
        != end
    {
        return Err(invalid("journal appended source count"));
    }
    events = events
        .without_array_prefix_data_v1(removed, &memory, &mut |s, b| common.charge(s, b))
        .map_err(graph_error)?;
    if events
        .array_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
        .map_err(graph_error)?
        != retained
    {
        return Err(invalid("journal actual retained source count"));
    }
    let source = ActualCurrentJournalProjectionV1 {
        events,
        next_event_id: metadata.next_event_id_v1(),
        next_event_id_era: metadata.next_event_id_era_v1(),
        guard: tx.guard(),
        core: common.work().identity(),
        parent_record: parent.refs().head.verified_record_id(),
        memory,
        _fee: fee,
    };
    source.validate(parent, tx, common, live)?;
    Ok(source)
}

#[path="commitment_journal_cold.rs"]mod cold;
pub(in crate::world) use self::cold::prepare_cold_journal_projection_v1;
