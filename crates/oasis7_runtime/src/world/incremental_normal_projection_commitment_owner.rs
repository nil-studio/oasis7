//! Opaque resident and terminal readonly projection carriers. No constructor
//! accepts a root/column tuple; the fixed producers retain their real sources.
use super::*;
use super::{
    raw_hash::PaidOriginalStateHashDataV1, state_columns::OriginalStateColumnsDataV1,
    template::StaticProjectionSchemaDataV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1, SealedExecutionBindingV1,
    },
    incremental_state::MerkleNodeReader,
};
use oasis7_world_projection::current_commitment::ProjectionColumnDataV1;
use std::{mem::size_of, sync::Arc};

/// All allocations are original readonly columns, not an executable World.
struct ProjectionReadonlyColumnsV1 {
    state: OriginalStateColumnsDataV1,
    agent_electricity: super::original_rows::OriginalProjectionRowsDataV1,
    agent_intents: super::original_rows::OriginalProjectionRowsDataV1,
    observation_state: ProjectionColumnDataV1,
    auxiliary: ProjectionColumnDataV1,
    template: StaticProjectionSchemaDataV1,
}
/// Accepted DATA cache, bound to the genuine selected parent. This type grants
/// neither a new commit nor permission to execute its retained observations.
pub struct AcceptedNormalProjectionCacheV1 {
    columns: Arc<ReservedValue<ProjectionReadonlyColumnsV1>>,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    memory: MemoryBudget,
}
impl AcceptedNormalProjectionCacheV1 {
    pub(in crate::world) fn validate_parent_v1(
        &self,
        parent: &crate::world::incremental_normal_sources::BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(paid)?;
        common.charge(4, 96).map_err(paid)?;
        let refs = parent.refs();
        if !self.memory.same_pool(common.memory())
            || !self.columns.uses_budget(common.memory())
            || self.record != refs.head.verified_record_id()
            || self.body != refs.runtime.occurrence().head_body_id()
            || self.roots != refs.core.body().roots
        {
            return Err(invalid("projection accepted parent binding"));
        }
        Ok(())
    }
    pub(in crate::world) fn auxiliary_columns_v1(
        &self,
        parent: &crate::world::incremental_normal_sources::BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ProjectionColumnDataV1, BodyMutationError> {
        self.validate_parent_v1(parent, common)?;
        let memory = common.memory().clone();
        self.columns
            .auxiliary
            .share_data(&memory, &mut |s, b| common.charge(s, b))
            .map_err(projection_data_error)
    }
    pub(in crate::world) fn world_journal_events_v1(
        &self,
        parent: &crate::world::incremental_normal_sources::BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ProjectionColumnDataV1, BodyMutationError> {
        self.validate_parent_v1(parent, common)?;
        let memory = common.memory().clone();
        let value = self
            .columns
            .auxiliary
            .object_member_data("world_journal", &memory, &mut |s, b| common.charge(s, b))
            .map_err(projection_data_error)?
            .ok_or_else(|| invalid("actual cached WorldJournal missing"))?;
        let result = value
            .share_data(&memory, &mut |s, b| common.charge(s, b))
            .map_err(projection_data_error)?;
        result
            .array_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
            .map_err(projection_data_error)?;
        Ok(result)
    }
    pub(in crate::world) fn share_ordinary_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Self>, BodyMutationError> {
        common.charge(1, size_of::<Self>() as u64).map_err(paid)?;
        if !self.memory.same_pool(common.memory()) || !self.columns.uses_budget(common.memory()) {
            return Err(invalid("projection cache share pool"));
        }
        let fee = common
            .memory()
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(Self {
            columns: Arc::clone(&self.columns),
            record: self.record,
            body: self.body,
            roots: self.roots,
            memory: self.memory.clone(),
        }))
    }
}
/// Minted only after the fixed Current source pipeline covers its real net
/// changes and computes both original commitments. It is still provisional.
pub(in crate::world) struct CurrentNormalProjectionCommitmentV1 {
    columns: Arc<ReservedValue<ProjectionReadonlyColumnsV1>>,
    raw_state: PaidOriginalStateHashDataV1,
    projection_root: ReservedValue<String>,
    parent_record: [u8; 32],
    parent_body: [u8; 32],
    parent_roots: [u8; 32],
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl CurrentNormalProjectionCommitmentV1 {
    pub(in crate::world) fn raw_state_hash(&self) -> &str {
        self.raw_state.as_str()
    }
    pub(in crate::world) fn captured_guard_v1(&self) -> &PreparedGuard {
        &self.guard
    }
    pub(in crate::world) fn parent_record_v1(&self) -> [u8; 32] {
        self.parent_record
    }
    pub(in crate::world) fn parent_body_v1(&self) -> [u8; 32] {
        self.parent_body
    }
    pub(in crate::world) fn projection_root(&self) -> &str {
        &self.projection_root
    }
    pub(in crate::world) fn retained_history_root_json_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        common.consume_steps(1).map_err(paid)?;
        self.raw_state
            .validate_binding(common.memory(), common.work().identity())?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || !self.columns.uses_budget(common.memory())
            || !self.memory.same_pool(common.memory())
        {
            return Err(invalid("projection retained root source context"));
        }
        let bytes = self
            .columns
            .state
            .scalar_original_json_data_v1("retained_history_root", common)?;
        let n = bytes
            .len()
            .checked_add(size_of::<Vec<u8>>())
            .ok_or_else(|| invalid("projection retained root output layout"))?;
        common.charge(1, n as u64).map_err(paid)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut output = Vec::with_capacity(bytes.len());
        if output.capacity() != bytes.len() {
            return Err(invalid("projection retained root output capacity"));
        }
        for part in bytes.chunks(8192) {
            common.charge(1, part.len() as u64).map_err(paid)?;
            output.extend_from_slice(part)
        }
        common.consume_steps(0).map_err(paid)?;
        Ok(fee.retain(output))
    }

    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_read_source_guard(&self.guard, live)?;
        self.raw_state.validate_binding(memory, core)?;
        if self.core != core
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !self.columns.uses_budget(memory)
            || !self.projection_root.uses_budget(memory)
            || self.parent_roots != tx.parent_fence().parent_roots
        {
            return Err(invalid("Current projection context"));
        }
        Ok(())
    }
    /// Capture the actual final strict cut after all source reads. The original
    /// source guard remains retained and is validated through read stability.
    pub(in crate::world) fn freeze_v1<M: MerkleNodeReader>(
        self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<FrozenNormalProjectionCommitmentV1, BodyMutationError> {
        common
            .charge(1, size_of::<FrozenNormalProjectionCommitmentV1>() as u64)
            .map_err(paid)?;
        self.validate(tx, common.memory(), common.work().identity(), live)?;
        let fee = common
            .memory()
            .reserve(
                size_of::<FrozenNormalProjectionCommitmentV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(FrozenNormalProjectionCommitmentV1 {
            source: self,
            guard,
            _fee: fee,
        })
    }
}
/// A strict terminal cut is data evidence for the actual seal, never durable
/// acceptance. Host admission still requires the actual qualified source.
pub(in crate::world) struct FrozenNormalProjectionCommitmentV1 {
    source: CurrentNormalProjectionCommitmentV1,
    guard: PreparedGuard,
    _fee: MemoryReservation,
}
impl FrozenNormalProjectionCommitmentV1 {
    pub(in crate::world) fn raw_state_hash(&self) -> &str {
        self.source.raw_state_hash()
    }
    pub(in crate::world) fn final_guard_v1(&self) -> &PreparedGuard {
        &self.guard
    }
    pub(in crate::world) fn projection_root(&self) -> &str {
        self.source.projection_root()
    }
    pub(in crate::world) fn retained_history_root_json_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        common.consume_steps(1).map_err(paid)?;
        if !self._fee.uses_budget(common.memory()) {
            return Err(invalid("frozen retained root outer pool"));
        }
        self.source.retained_history_root_json_bounded_v1(common)
    }

    pub(in crate::world) fn validate_current_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(paid)?;
        self.source
            .validate(tx, common.memory(), common.work().identity(), live)?;
        tx.validate_guard(&self.guard, live)?;
        if !self._fee.uses_budget(common.memory()) {
            return Err(invalid("frozen projection outer pool"));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_sealed_v1(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.charge(16, 8192).map_err(paid)?;
        if self.source.core != common.work().identity()
            || !self._fee.uses_budget(memory)
            || !self.source.memory.same_pool(memory)
            || !memory.same_pool(common.memory())
            || !self.source.columns.uses_budget(memory)
            || !self.source.projection_root.uses_budget(memory)
        {
            return Err(invalid("frozen projection context"));
        }
        self.source
            .raw_state
            .validate_binding(memory, common.work().identity())?;
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        Ok(())
    }
}

impl FrozenNormalProjectionCommitmentV1 {
    /// Prepares only resident DATA. Host admits the returned parent only from its
    /// real post-publication accepted source; a seal alone never reaches this API.
    pub(in crate::world) fn prepare_normal_candidate_source_v1(
        &self,
        terminal: &crate::world::incremental_normal_host_finish::SealedNormalResidentSourceV1,
        core: &ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore>,
        history: &ReservedValue<
            oasis7_world_state::incremental_runtime_history_source::CheckedRuntimeHistorySourceV1,
        >,
        head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        roots: &RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<AcceptedNormalProjectionCacheV1>, BodyMutationError> {
        common.charge(32, 8192).map_err(paid)?;
        terminal.validate_projection_source_v1(self, common)?;
        if self.source.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || !self.source.columns.uses_budget(common.memory())
            || !self.source.projection_root.uses_budget(common.memory())
            || !core.uses_budget(common.memory())
            || !history.uses_budget(common.memory())
        {
            return Err(invalid("projection warm source pool"));
        }
        self.source
            .raw_state
            .validate_binding(common.memory(), common.work().identity())?;
        let selected = history.occurrence();
        if core.candidate_record_id() != head.verified_record_id()
            || selected.candidate_record_id() != core.candidate_record_id()
            || selected.body_id() != core.catalog().entry.commit
            || selected.roots_id() != terminal.after_roots()
            || core.body().roots != terminal.after_roots()
            || history.manifest().roots != terminal.after_roots()
            || roots.digest()? != terminal.after_roots()
        {
            return Err(invalid("projection warm exact candidate"));
        }
        common.charge(64, 128).map_err(paid)?;
        let mut parent = [0u8; 64];
        hex::encode_to_slice(self.source.parent_record, &mut parent).map_err(paid)?;
        if head.record().parent_record_id.as_bytes() != parent
            || head.record().projection_root.as_str() != self.projection_root()
        {
            return Err(invalid("projection warm record commitments"));
        }
        let fee = common
            .memory()
            .reserve(
                size_of::<AcceptedNormalProjectionCacheV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(AcceptedNormalProjectionCacheV1 {
            columns: Arc::clone(&self.source.columns),
            record: head.verified_record_id(),
            body: selected.body_id(),
            roots: terminal.after_roots(),
            memory: common.memory().clone(),
        }))
    }
}

fn projection_data_error<E: std::error::Error + Send + Sync + 'static>(
    error: oasis7_world_projection::current_commitment::ProjectionColumnErrorV1<E>,
) -> BodyMutationError {
    match error {
        oasis7_world_projection::current_commitment::ProjectionColumnErrorV1::Work(error) => {
            paid(error)
        }
        oasis7_world_projection::current_commitment::ProjectionColumnErrorV1::Memory(error) => {
            ValueTreeError::Memory(error).into()
        }
        oasis7_world_projection::current_commitment::ProjectionColumnErrorV1::Invalid(note) => {
            invalid(note)
        }
    }
}

#[path = "incremental_normal_projection_commitment_owner_bootstrap.rs"]
mod bootstrap;
#[path = "incremental_normal_projection_commitment_producer.rs"]
mod producer;
pub(in crate::world) use producer::ActualCurrentStateHashSnapshotV1;
