//! Fixed source binding for the actual terminal Runtime revocation delta.
//! A source observation alone is never authorization, accepted cache or status.
use crate::world::{
    incremental_authorization_root_current::{SourceError, with_business_source_read_work_v1},
    incremental_body::BodyMutationError,
    incremental_module_invocation::*,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::{
        CurrentRuntimeRevocationProjectionDeltaV1, RevocationProjectionPatchRefV1,
        RuntimeRevocationCurrentV1, read_current_runtime_revocation_projection_delta_v1,
    },
};
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("terminal Runtime revocation projection source")
}
fn error(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
#[derive(Debug)]
struct ReadCause(SourceError);
impl std::fmt::Display for ReadCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for ReadCause {}
pub(in crate::world) struct ActualCurrentRevocationProjectionSourceV1 {
    actual: CurrentRuntimeRevocationProjectionDeltaV1,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    fee: MemoryReservation,
}
impl ActualCurrentRevocationProjectionSourceV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(error)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        self.actual
            .validate(tx, common.memory(), live)
            .map_err(error)
    }
    pub(in crate::world) fn visit<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
        mut emit: impl FnMut(
            RevocationProjectionPatchRefV1<'_>,
            &mut FormalBoundedContextV1<'_>,
        ) -> Result<(), BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.validate(tx, common, live)?;
        let common = std::cell::RefCell::new(common);
        self.actual.visit_with_work_v1(
            |steps, bytes| common.borrow_mut().charge(steps, bytes).map_err(error),
            |patch| emit(patch, &mut common.borrow_mut()),
        )
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn capture_current_revocation_projection_v1<
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        current: &RuntimeRevocationCurrentV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut crate::world::NormalProjectionCurrentWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ActualCurrentRevocationProjectionSourceV1, BodyMutationError> {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.validate_context(common).map_err(error)?;
            let refs = parent.refs();
            let guard = self.ctx.transaction.guard();
            let live = live_owner();
            let actual = with_business_source_read_work_v1(
                refs.roots,
                self.ctx,
                read,
                work.aux_authorization,
                work.aux_processing,
                common,
                |ctx, read, w, processing| {
                    read_current_runtime_revocation_projection_delta_v1(
                        refs.storage.current().runtime().revocation(),
                        current,
                        ctx,
                        read,
                        &mut w.scan,
                        &mut w.typed,
                        processing,
                        &guard,
                        &live,
                    )
                    .map_err(SourceError::from)
                },
            )
            .map_err(|e| error(ReadCause(e)))?;
            let n = std::mem::size_of::<ActualCurrentRevocationProjectionSourceV1>();
            common.charge(1, n as u64).map_err(error)?;
            let fee = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let source = ActualCurrentRevocationProjectionSourceV1 {
                actual,
                core: common.work().identity(),
                memory: common.memory().clone(),
                fee,
            };
            source.validate(self.ctx.transaction, common, &live_owner())?;
            Ok(source)
        }));
        match attempt {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(e)
            }
            Err(payload) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        }
    }
}

/// Complete provisional auxiliary projection DATA. Only this module's fixed
/// source assembler can construct it; an arbitrary graph cannot be admitted by
/// callers. It grants no execution, accepted-parent or retained-read authority.
pub(in crate::world) struct ActualCurrentNormalProjectionAuxV1 {
    columns: oasis7_world_projection::current_commitment::ProjectionColumnDataV1,
    parent_record: [u8; 32],
    guard: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    fee: MemoryReservation,
}
impl ActualCurrentNormalProjectionAuxV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(error)?;
        common.charge(1, 32).map_err(error)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.parent_record != parent.refs().head.verified_record_id()
            || tx.parent_fence() != parent.fence()
            || self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fee.uses_budget(common.memory())
            || !self.columns.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "actual auxiliary projection source binding",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn columns_v1(
        &self,
    ) -> &oasis7_world_projection::current_commitment::ProjectionColumnDataV1 {
        &self.columns
    }
}

#[path = "incremental_normal_projection_runtime_fold.rs"]
mod fold;
pub(in crate::world) use fold::{
    fold_current_revocation_projection_v1, initialize_revocation_set_projection_v1,
};

#[path = "incremental_normal_projection_runtime_cognition.rs"]
mod cognition;
pub(in crate::world) use cognition::{
    apply as retention_path_apply_v1, check as retention_path_check_v1,
    presence as retention_path_presence_v1,
};
pub(in crate::world) use cognition::{
    fold_current_cognition_proof_projection_v1, fold_current_cognition_state_projection_v1,
};

#[path = "incremental_normal_projection_runtime_metadata.rs"]
mod metadata;

#[path = "incremental_normal_projection_runtime_history.rs"]
mod history;

#[path = "incremental_normal_projection_runtime_capture.rs"]
mod capture;

#[path = "incremental_normal_projection_runtime_fields.rs"]
mod fields;

pub(in crate::world) fn bootstrap_projection_value_v1(
    bytes: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<oasis7_world_projection::current_commitment::ProjectionColumnDataV1, BodyMutationError>
{
    fold::value(bytes, common)
}

#[path = "incremental_normal_projection_runtime_cold.rs"]
mod cold;
#[path = "incremental_normal_projection_runtime_cold_cognition_profile.rs"]
mod cold_cognition_profile;
