//! One complete FIFO phase over the actual PendingAction Current owner.
//! A fresh work-bank view borrows the same remaining counters each iteration.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_factory_phase::FactoryPhaseOutcomeV1,
    incremental_module_invocation::*,
    incremental_normal_action_work::{NormalActionSourcesV1, NormalActionWorkSetV1},
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_pending_action_family::PendingActionFamilyWorkV1,
    incremental_pending_action_overlay::PendingActionCurrentV1,
    incremental_runtime_revocation::{
        RevocationReadWorkV1, RuntimeRevocationCurrentV1, RuntimeRevocationQualificationV1,
    },
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
#[derive(Debug)]
pub(in crate::world) enum NormalQueueErrorV1<OE> {
    Module(RootModuleExecutionErrorV1<OE>),
    Action(NonmovementSelectedErrorV1<OE>),
}
impl<OE> From<RootModuleExecutionErrorV1<OE>> for NormalQueueErrorV1<OE> {
    fn from(e: RootModuleExecutionErrorV1<OE>) -> Self {
        Self::Module(e)
    }
}
/// Proves the real queue was exhausted at this phase boundary. Later system
/// modules may enqueue work for the next tick; this is not a final-empty claim
/// or a completed-tick, acceptance or resume capability.
pub(in crate::world) struct DrainedNormalQueueV1 {
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    count: usize,
    parent_record: [u8; 32],
    fee: ReservedValue<()>,
}
impl DrainedNormalQueueV1 {
    pub(in crate::world) fn processed(&self) -> usize {
        self.count
    }
    pub(in crate::world) fn validate_before_tail<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_guard(&self.guard, live)?;
        if self.now != now
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || self.parent_record != parent.refs().core.candidate_record_id()
            || tx.parent_fence() != parent.fence()
        {
            return Err(BodyMutationError::Invalid("drained normal queue boundary"));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_evidence<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid("normal queue historical source"));
        }
        Ok(())
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn drain_normal_current_queue_v1<P, CE, CC, PE, PC, AP, AE, AC, ME>(
        &mut self,
        parent_source: &ReservedValue<BoundNormalTickParentV1<'_>>,
        prologue: &FactoryPhaseOutcomeV1,
        pending: &mut ReservedValue<PendingActionCurrentV1>,
        pending_work: &mut PendingActionFamilyWorkV1,
        qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
        revocation: &RuntimeRevocationCurrentV1,
        admission_read: &mut WorldFieldReadContextV1<'_, AP, R, AE, AC>,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut RevocationReadWorkV1,
        sources: &NormalActionSourcesV1<'_>,
        actions: &mut NormalActionWorkSetV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        audit: &mut WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rules: &mut RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<DrainedNormalQueueV1, NormalQueueErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent_source.validate_context(common).map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            common
                .charge(1, std::mem::size_of::<DrainedNormalQueueV1>() as u64)
                .map_err(|e| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
                })?;
            if self.ctx.transaction.parent_fence() != parent_source.fence()
                || actions.common_identity() != common.work().identity()
            {
                return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "normal queue parent/work binding",
                ))
                .into());
            }
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<DrainedNormalQueueV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Value(ValueTreeError::Memory(e)))?
                .retain(());
            let mut count = 0usize;
            loop {
                common.consume_steps(1).map_err(|e| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
                })?;
                let mut bank = actions
                    .bank(sources, common.work())
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                let next = self
                    .execute_next_nonmovement_with_modules_v1(
                        prologue,
                        pending,
                        pending_work,
                        qualification,
                        revocation,
                        admission_read,
                        policy_scan,
                        policy_work,
                        materials,
                        current,
                        parent,
                        NonmovementSelectedWorkV1::DispatchBank(&mut bank),
                        common,
                        audit,
                        directory,
                        rules,
                        enumeration,
                        tail,
                        live_owner,
                    )
                    .map_err(NormalQueueErrorV1::Action)?;
                if next.is_none() {
                    break;
                }
                count = count.checked_add(1).ok_or_else(|| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "normal queue action count overflow",
                    ))
                })?;
                // Original facts and their Current mutations live in the sole transaction;
                // the per-action immutable receipts may drop after its complete post route.
                drop(next);
            }
            let guard = self.ctx.transaction.guard();
            self.ctx
                .transaction
                .validate_guard(&guard, &live_owner())
                .map_err(RootModuleExecutionErrorV1::from)?;
            Ok(DrainedNormalQueueV1 {
                guard,
                core: common.work().identity(),
                now: self.now,
                count,
                parent_record: parent_source.refs().core.candidate_record_id(),
                fee,
            })
        }));
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
}
