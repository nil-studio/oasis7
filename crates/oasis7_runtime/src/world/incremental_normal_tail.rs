//! Parent-bound sources and shared work for the normal tick tail.
//!
//! Concrete reducers are integrated here as they are implemented. No function
//! in this module mints whole-tick completion; the remaining original tail
//! phases must be implemented before the fixed driver may do so.

use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, RootSetV1},
    incremental_material::CheckedCommitCore,
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::CompleteCognitionHistorySourceV3,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_runtime_typed_complete::RuntimeTypedCompleteQualificationV1,
    incremental_world_current_complete::WorldCurrentStateQualificationV1,
    incremental_world_native_complete::WorldNativeCompleteQualificationV1,
    incremental_world_raw_complete::WorldRawCompleteQualificationV1,
};

use super::incremental_body::BodyMutationError;
use super::incremental_normal_sources::{BoundNormalTickParentV1, NormalTickParentRefsV1};

#[path = "incremental_normal_due_economy.rs"]
mod due_economy;
#[path = "incremental_normal_factory_built.rs"]
pub(in crate::world) mod factory_built;

/// Every reducer receives the same remaining allowance. Constructing this
/// value requires the caller's explicit limits; no phase can reset it.
pub(in crate::world) struct NormalTailWorkV1<'borrow, 'ctx> {
    common: &'borrow mut FormalBoundedContextV1<'ctx>,
}

impl<'borrow, 'ctx> NormalTailWorkV1<'borrow, 'ctx> {
    pub(in crate::world) fn new(common: &'borrow mut FormalBoundedContextV1<'ctx>) -> Self {
        Self { common }
    }

    pub(in crate::world) fn memory(&self) -> &MemoryBudget {
        self.common.memory()
    }

    pub(in crate::world) fn reserve(
        &self,
        bytes: usize,
        class: MemoryClass,
    ) -> Result<MemoryReservation, ValueTreeError> {
        self.common
            .memory()
            .reserve(bytes, class)
            .map_err(ValueTreeError::Memory)
    }

    pub(in crate::world) fn charge(&mut self, bytes: usize) -> Result<(), BodyMutationError> {
        let bytes = u64::try_from(bytes)
            .map_err(|_| ValueTreeError::ResourceLimited("normal tick tail bytes"))?;
        self.common
            .charge(1, bytes)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))
    }

    pub(in crate::world) fn charge_record(&mut self) -> Result<(), BodyMutationError> {
        self.common
            .consume_steps(1)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))
    }

    pub(in crate::world) fn identity(
        &self,
    ) -> oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1 {
        self.common.work().identity()
    }

    pub(in crate::world) fn validate_parent(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
    ) -> Result<(), BodyMutationError> {
        parent
            .validate_context(self.common)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))
    }
}

/// Only references reached through the exact accepted source owner are kept.
/// This view is not a second qualification and has no constructor from IDs,
/// raw roots, DTOs, or caller-provided family tokens.
pub(in crate::world) struct NormalTailSourcesV1<'p, 'a> {
    parent: &'p BoundNormalTickParentV1<'a>,
    core: &'p ReservedValue<CheckedCommitCore>,
    roots: &'p ReservedValue<RootSetV1>,
    current: &'p ReservedValue<WorldCurrentStateQualificationV1>,
    native: &'p ReservedValue<WorldNativeCompleteQualificationV1>,
    raw: &'p ReservedValue<WorldRawCompleteQualificationV1>,
    runtime_state: &'p ReservedValue<RuntimeTypedCompleteQualificationV1>,
    cognition_state: &'p ReservedValue<
        oasis7_world_state::incremental_cognition_family::CompleteCognitionCurrentQualificationV3,
    >,
    runtime_history: &'p ReservedValue<AcceptedRuntimeHistorySourceV1>,
    cognition_history: &'p ReservedValue<CompleteCognitionHistorySourceV3>,
}

impl<'p, 'a> NormalTailSourcesV1<'p, 'a> {
    pub(in crate::world) fn from_bound_parent(parent: &'p BoundNormalTickParentV1<'a>) -> Self {
        let refs = parent.refs();
        let current = refs.storage.current();
        Self {
            parent,
            core: refs.core,
            roots: refs.roots,
            current,
            native: current.native(),
            raw: current.raw(),
            runtime_state: current.runtime(),
            cognition_state: current.cognition(),
            runtime_history: refs.runtime,
            cognition_history: refs.cognition,
        }
    }

    pub(in crate::world) fn current(&self) -> &'p ReservedValue<WorldCurrentStateQualificationV1> {
        self.current
    }

    pub(in crate::world) fn native(&self) -> &'p ReservedValue<WorldNativeCompleteQualificationV1> {
        self.native
    }

    pub(in crate::world) fn factory(
        &self,
    ) -> &'p ReservedValue<
        oasis7_world_state::incremental_factory_records::FactoryFamilyQualificationV1,
    > {
        self.native.factory()
    }

    pub(in crate::world) fn industry(
        &self,
    ) -> &'p ReservedValue<
        oasis7_world_state::incremental_industry_jobs::IndustryFamilyQualificationV1,
    > {
        self.native.industry()
    }

    pub(in crate::world) fn raw(&self) -> &'p ReservedValue<WorldRawCompleteQualificationV1> {
        self.raw
    }

    pub(in crate::world) fn runtime_state(
        &self,
    ) -> &'p ReservedValue<RuntimeTypedCompleteQualificationV1> {
        self.runtime_state
    }

    pub(in crate::world) fn cognition_state(
        &self,
    ) -> &'p ReservedValue<
        oasis7_world_state::incremental_cognition_family::CompleteCognitionCurrentQualificationV3,
    > {
        self.cognition_state
    }

    pub(in crate::world) fn runtime_history(
        &self,
    ) -> &'p ReservedValue<AcceptedRuntimeHistorySourceV1> {
        self.runtime_history
    }

    pub(in crate::world) fn cognition_history(
        &self,
    ) -> &'p ReservedValue<CompleteCognitionHistorySourceV3> {
        self.cognition_history
    }

    pub(in crate::world) fn roots(&self) -> &'p ReservedValue<RootSetV1> {
        self.roots
    }

    pub(in crate::world) fn core(&self) -> &'p ReservedValue<CheckedCommitCore> {
        self.core
    }

    pub(in crate::world) fn parent(&self) -> &'p BoundNormalTickParentV1<'a> {
        self.parent
    }

    /// Check the already-derived references against the exact active pool and
    /// parent root. This check does not requalify, readmit, or reset any work.
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        transaction: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        guard: &oasis7_formal_protocol::incremental_execution::PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), ExecutionError> {
        transaction.validate_guard(guard, live)?;
        if transaction.parent_fence() != self.parent.fence() {
            return Err(ExecutionError::ParentConflict);
        }
        if !transaction.uses_budget(memory)
            || !self.core.uses_budget(memory)
            || !self.roots.uses_budget(memory)
            || !self.current.uses_budget(memory)
            || !self.native.uses_budget(memory)
            || !self.raw.uses_budget(memory)
            || !self.runtime_state.uses_budget(memory)
            || !self.cognition_state.uses_budget(memory)
            || !self.runtime_history.uses_budget(memory)
            || !self.cognition_history.uses_budget(memory)
        {
            return Err(ExecutionError::Invalid("normal tail source pool"));
        }
        let _metadata = memory.reserve(4096, MemoryClass::Ordinary)?;
        self.current
            .validate_binding(self.roots)
            .map_err(|_| ExecutionError::Invalid("normal tail current binding"))?;
        self.native
            .validate_binding(self.roots)
            .map_err(|_| ExecutionError::Invalid("normal tail native binding"))?;
        self.raw
            .validate_binding(self.roots)
            .map_err(|_| ExecutionError::Invalid("normal tail raw binding"))?;
        self.runtime_state
            .validate_binding(self.roots)
            .map_err(|_| ExecutionError::Invalid("normal tail Runtime binding"))?;
        if self.runtime_history.occurrence().roots_id() != self.core.body().roots
            || self.cognition_history.current().roots_id() != self.core.body().roots
        {
            return Err(ExecutionError::Invalid("normal tail history binding"));
        }
        Ok(())
    }
}
