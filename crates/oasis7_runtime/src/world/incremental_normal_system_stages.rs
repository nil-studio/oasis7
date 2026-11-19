//! The original World.step system stages through threat refresh. This actual
//! Current cut precedes the choice of local or committed consensus source.
//! It is stage DATA, not a completed World.step or seal qualification.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionTransaction, PreparedGuard},
};
pub(in crate::world) struct NormalSystemStagesBeforeConsensusV1 {
    guard: PreparedGuard,
    parent_record: [u8; 32],
    input_material: [u8; 32],
    now: u64,
    core: WorkIdentityV1,
    _fee: ReservedValue<()>,
}
impl NormalSystemStagesBeforeConsensusV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        input
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        common
            .charge(4, 64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_exact_transaction_identity(&self.guard)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
            || self.parent_record != parent.refs().head.verified_record_id()
            || self.input_material != input.checked().material_id()
            || self.now != now
        {
            return Err(BodyMutationError::Invalid(
                "actual system stages Current cut",
            ));
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
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter + SchedulePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_system_stages_v1<'p, P, CE, CC, PE, PC, IE, IC, ME, S>(
        &mut self,
        parent_source: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        sources: &NormalSystemTailSourcesV1<'_>,
        input: &PaidBoundNormalInputDataV1<'_>,
        work: &mut NormalSystemTailWorkV1<'_>,
        product_delivery: &mut CurrentProductValidationDeliveryIndexV1<'_>,
        product_work: &mut ProductValidationDeliveryWorkV1,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        sink: &mut S,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        normal: &mut NormalTailWorkV1<'_, '_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<NormalSystemStagesBeforeConsensusV1, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
        S: ObservationJsonOriginalSinkV2,
        S::Error: std::fmt::Debug + Send + Sync + 'static,
    {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            normal
                .validate_parent(parent_source)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            normal
                .charge(std::mem::size_of_val(sources) + std::mem::size_of_val(work))
                .map_err(RootModuleExecutionErrorV1::Body)?;
            input
                .validate_context(
                    normal
                        .common_mut()
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                )
                .map_err(|error| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(error)))
                })?;
            let live = live_owner();
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live)?;
            if self.ctx.transaction.parent_fence() != parent_source.fence()
                || !parent_source.uses_budget(normal.memory())
            {
                return Err(RootModuleExecutionErrorV1::Body(
                    BodyMutationError::Invalid("fixed normal system-tail parent"),
                ));
            }
            let result_bytes = std::mem::size_of::<NormalSystemStagesBeforeConsensusV1>();
            normal
                .charge(result_bytes)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let outcome_fee = normal
                .reserve(result_bytes, MemoryClass::Ordinary)
                .map_err(|error| RootModuleExecutionErrorV1::Body(error.into()))?
                .retain(());
            let bound = NormalTailSourcesV1::from_bound_parent(parent_source);
            let state = self.state;
            self.execute_normal_economic_tail_v1(
                state,
                parent_source,
                sources.economic,
                sources.recipes,
                &mut work.economic,
                product_delivery,
                product_work,
                industry_read,
                sink,
                materials,
                current,
                parent,
                normal,
                directory,
                enumeration,
                live_owner,
            )?;
            let gameplay = NormalGameplayCycleSourcesV1 {
                roots: bound.roots(),
                directory: sources.gameplay,
                diplomacy: bound.native().diplomacy(),
                governance: bound.native().governance(),
                native: bound.native().native(),
                agents: bound.raw().agents(),
                retention: sources.economic.retention,
            };
            self.execute_normal_gameplay_cycles_v1(
                bound.runtime_history(),
                &gameplay,
                work.gameplay,
                materials,
                current,
                parent,
                normal,
                directory,
                enumeration,
                live_owner,
            )?;
            self.execute_normal_organization_tail_v1(
                parent_source,
                &bound,
                sources.economic,
                work.organization,
                materials,
                current,
                parent,
                normal,
                directory,
                enumeration,
                live_owner,
            )?;
            let claims = NormalClaimEpochSourcesV1 {
                state: ClaimEpochStateSourcesV1 {
                    roots: bound.roots(),
                    agents: bound.raw().agents(),
                    native: bound.native().native(),
                    fixed: bound.native().fixed(),
                },
                policy: bound.native().main_policy(),
                governance: bound.runtime_state().governance(),
                grants: sources.grants,
                claims: sources.economic.claims,
                retention: sources.economic.retention,
            };
            self.execute_normal_claim_epochs_v1(
                parent_source,
                &claims,
                &mut *work.economic.world_delivery,
                materials,
                state,
                current,
                parent,
                normal,
                directory,
                enumeration,
                live_owner,
            )?;
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
            self.refresh_normal_current_threat_heatmap_v1(
                parent_source,
                sources.gameplay,
                sources.threats,
                sources.economic.retention,
                work.gameplay,
                work.threat_keys,
                materials,
                current,
                parent,
                normal,
                live_owner,
            )?;
            let common = normal
                .common_mut()
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let value = NormalSystemStagesBeforeConsensusV1 {
                guard: self.ctx.transaction.guard(),
                parent_record: parent_source.refs().head.verified_record_id(),
                input_material: input.checked().material_id(),
                now: self.now,
                core: common.work().identity(),
                _fee: outcome_fee,
            };
            value
                .validate(
                    self.ctx.transaction,
                    parent_source,
                    input,
                    self.now,
                    common,
                    &live_owner(),
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(value)
        }));
        let result = match outcome {
            Ok(result) => result,
            Err(payload) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(payload)
            }
        };
        if result.is_err() {
            let _: Result<(), ExecutionError> = self
                .ctx
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
