//! Genuine terminal Current WorldRules projection DATA. Only the fixed Root
//! source consumer mints this owner; no caller paths, values or qualification
//! flags enter the construction boundary.
use super::incremental_authorization_root_current::{
    AuthorizationRootSourceReadWorkV1, SourceError, with_business_source_read_work_v1,
};
use super::{
    incremental_body::BodyMutationError, incremental_module_invocation::*,
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
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_world_rule_projection_delta::{
        CurrentWorldRuleProjectionDeltaV1, read_current_world_rule_projection_delta_v1,
    },
};
pub struct NormalProjectionWorldRulesWorkV1 {
    pub source: AuthorizationRootSourceReadWorkV1,
    pub processing: BusinessDiffProcessingWorkV1,
}
pub struct PaidCurrentProjectionWorldRulesDeltaV1 {
    actual: CurrentWorldRuleProjectionDeltaV1,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    fee: MemoryReservation,
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("actual Current WorldRules projection source")
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
impl PaidCurrentProjectionWorldRulesDeltaV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        if self.core != core || !self.memory.same_pool(memory) || !self.fee.uses_budget(memory) {
            return Err(bad());
        }
        self.actual
            .validate(tx, memory, live)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))
    }
    /// Actual optional/skip-emission DATA, always derived from the same Current
    /// source. It is not a caller-controlled field-presence or admission flag.
    pub(in crate::world) fn rules_present(&self) -> bool {
        self.actual.rules_present()
    }
    pub(in crate::world) fn branch_emission(&self) -> [Option<bool>; 2] {
        self.actual.branch_emission()
    }
    pub(in crate::world) fn visit<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
        mut visitor: impl FnMut(
            &[String],
            &[u8],
            Option<&[u8]>,
            &mut FormalBoundedContextV1<'_>,
        ) -> Result<(), BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.validate(tx, common.memory(), common.work().identity(), live)?;
        let common = std::cell::RefCell::new(common);
        self.actual.visit_with_work_v1(
            &mut |bytes| common.borrow_mut().charge(1, bytes as u64).map_err(error),
            |path, semantic, json| visitor(path, semantic, json, &mut common.borrow_mut()),
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
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn capture_current_projection_world_rules_delta_v1<
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut NormalProjectionWorldRulesWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PaidCurrentProjectionWorldRulesDeltaV1, BodyMutationError> {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.validate_context(common).map_err(error)?;
            let refs = parent.refs();
            let guard = self.ctx.transaction.guard();
            let live = live_owner();
            let actual = with_business_source_read_work_v1(
                refs.roots,
                self.ctx,
                read,
                &mut work.source,
                &mut work.processing,
                common,
                |ctx, read, w, processing| {
                    read_current_world_rule_projection_delta_v1(
                        refs.storage.current().native().rules(),
                        refs.roots,
                        read.state,
                        ctx,
                        &mut w.scan,
                        &mut w.typed,
                        &mut w.pending_entries,
                        processing,
                        &guard,
                        &live,
                    )
                    .map_err(SourceError::from)
                },
            )
            .map_err(|e| error(ReadCause(e)))?;
            let bytes = std::mem::size_of::<PaidCurrentProjectionWorldRulesDeltaV1>();
            common.charge(1, bytes as u64).map_err(error)?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let result = PaidCurrentProjectionWorldRulesDeltaV1 {
                actual,
                core: common.work().identity(),
                memory: common.memory().clone(),
                fee,
            };
            result.validate(
                self.ctx.transaction,
                common.memory(),
                common.work().identity(),
                &live_owner(),
            )?;
            Ok(result)
        }));
        match attempt {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
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
