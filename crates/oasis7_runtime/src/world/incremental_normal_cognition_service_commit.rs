//! Actual evaluation/scheduler state CAS before the fixed Journal batch.
use super::*;
use crate::world::incremental_normal_cognition_expiry::with_current_cognition_io_v1;
use oasis7_world_state::{
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_normal_cognition_current::{
        create_current_cognition_service_map_v2, replace_current_cognition_service_batch_v1,
    },
};
pub(super) struct PreparedCurrentCognitionServiceStateV2 {
    old_sequence: u64,
    old_digest: ReservedValue<String>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    _fee: ReservedValue<()>,
}
impl PreparedCurrentCognitionServiceStateV2 {
    pub(super) fn old_sequence(&self) -> u64 {
        self.old_sequence
    }
    pub(super) fn old_digest(&self) -> &str {
        &self.old_digest
    }
    pub(super) fn validate_before_append<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        self.validate_evidence_identity(tx, common, now, live)
    }
    pub(super) fn validate_evidence_identity<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.charge(1, 128).map_err(formal)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.identity != common.work().identity()
            || self.now != now
            || !self.old_digest.uses_budget(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(bad().into());
        }
        Ok(())
    }
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => formal(e),
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
    pub(super) fn commit_current_cognition_service_state_v2(
        &mut self,
        sources: &CurrentCognitionServiceSourcesV2<'_, '_>,
        plan: &selection::CurrentCognitionSelectionPlanV2,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PreparedCurrentCognitionServiceStateV2, RootModuleExecutionErrorV1<O::Error>> {
        plan.validate_before_write(self.ctx.transaction, sources, common, &live_owner())
            .map_err(RootModuleExecutionErrorV1::Body)?;
        let scheduler = plan.state.scheduler.as_ref().ok_or_else(bad)?;
        let old = plan.inputs.old_digest.as_str();
        common
            .charge(1, old.len() as u64)
            .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
        let digest_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<String>() + old.len(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut old_digest = String::new();
        old_digest
            .try_reserve_exact(old.len())
            .map_err(|_| ValueTreeError::ResourceLimited("service prior head allocation"))?;
        if old_digest.capacity() > old.len() {
            return Err(ValueTreeError::ResourceLimited("service prior head capacity").into());
        }
        old_digest.push_str(old);
        let old_digest = digest_fee.retain(old_digest);
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<PreparedCurrentCognitionServiceStateV2>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        with_current_cognition_io_v1(
            self.ctx,
            self.members,
            work.expiry,
            common,
            |ctx, write, work| {
                let live = live_owner();
                match (&plan.evaluations_after, &plan.state.evaluations) {
                    (Some(after), Some(before)) => {
                        let guard = ctx.transaction.guard();
                        replace_current_cognition_service_batch_v1(
                            &[
                                (before, &after.value),
                                (scheduler, &plan.scheduler_after.value),
                            ],
                            ctx,
                            write,
                            &mut work.write,
                            &guard,
                            &live,
                        )
                        .map_err(overlay)?;
                    }
                    (Some(after), None) => {
                        // Genuine optional-map absence is re-opened inside the fixed creator.
                        // It never manufactures a fake CurrentRecord/expected digest.
                        let guard = ctx.transaction.guard();
                        create_current_cognition_service_map_v2(
                            CognitionJsonFieldV1::ContinuationEvaluations,
                            sources.current,
                            sources.parent.refs().roots,
                            &after.value,
                            ctx,
                            write,
                            &mut work.write,
                            &guard,
                            &live,
                        )
                        .map_err(overlay)?;
                        let guard = ctx.transaction.guard();
                        replace_current_cognition_service_batch_v1(
                            &[(scheduler, &plan.scheduler_after.value)],
                            ctx,
                            write,
                            &mut work.write,
                            &guard,
                            &live,
                        )
                        .map_err(overlay)?;
                    }
                    (None, _) => {
                        let guard = ctx.transaction.guard();
                        replace_current_cognition_service_batch_v1(
                            &[(scheduler, &plan.scheduler_after.value)],
                            ctx,
                            write,
                            &mut work.write,
                            &guard,
                            &live,
                        )
                        .map_err(overlay)?;
                    }
                }
                Ok(())
            },
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        Ok(PreparedCurrentCognitionServiceStateV2 {
            old_sequence: plan.inputs.old_sequence,
            old_digest,
            guard: self.ctx.transaction.guard(),
            identity: common.work().identity(),
            now: self.now,
            _fee: fee,
        })
    }
}
