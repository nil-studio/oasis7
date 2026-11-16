//! Exact economy-action input through the existing selected sealed worker.
//! This is a component data result, not a whole-tick or publication permit.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;

enum FactoryResultV1 {
    Output(ReservedValue<ModuleOutput>),
    VmFailure(ReservedValue<WorldError>),
}
pub(in crate::world) struct PaidFactoryModuleExecutionV1 {
    result: FactoryResultV1,
    transaction: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    now: u64,
    action_id: u64,
    staged_disposition:
        Option<oasis7_formal_protocol::incremental_execution::StagedBranchDispositionV1>,
    _phase: oasis7_proto::memory_budget::MemoryReservation,
}
impl PaidFactoryModuleExecutionV1 {
    pub(in crate::world) fn output(&self) -> Option<&ModuleOutput> {
        match &self.result {
            FactoryResultV1::Output(value) => Some(value),
            _ => None,
        }
    }
    pub(in crate::world) fn business_failure(&self) -> Option<&WorldError> {
        match &self.result {
            FactoryResultV1::VmFailure(value) => Some(value),
            _ => None,
        }
    }
    pub(in crate::world) fn vm_failure(&self) -> Option<&WorldError> {
        match &self.result {
            FactoryResultV1::VmFailure(value) => Some(value),
            _ => None,
        }
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn execute_factory_module_request_v1(
        &mut self,
        call: &crate::world::incremental_normal_action_factory_schedule::factory_module::PreparedFactoryModuleCallV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PaidFactoryModuleExecutionV1, RootModuleExecutionErrorV1<O::Error>> {
        let transaction = self.ctx.transaction.guard();
        let identity = common.work().identity();
        let now = self.now;
        let result = (|| {
            if call.kind() == crate::world::incremental_normal_action_factory_schedule::factory_module::FactoryModuleCallKindV1::Build && !self.ctx.transaction.is_staged_branch_v1()? {
                return Err(ValueTreeError::Invalid("factory build requires staged module transaction").into());
            }
            let outer_bytes = std::mem::size_of::<PaidFactoryModuleExecutionV1>();
            common.charge(1, outer_bytes as u64).map_err(|error| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(error)))
            })?;
            let outer_phase = common
                .memory()
                .reserve(
                    outer_bytes,
                    oasis7_proto::memory_budget::MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            // The selected request is immutable parent DATA in the actual
            // staged child. Mutable participants still require exact child
            // identity; this check grants no write or execution completion.
            call.validate_evidence_identity(self.ctx.transaction, common, self.now, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            if !self.ctx.values.memory.same_pool(common.memory()) {
                return Err(ValueTreeError::Invalid("due product Common pool").into());
            }
            let failure = RefCell::new(None);
            let input = EconomyInputV1::Factory {
                call,
                failure: &failure,
            };
            let output =
                match super::due_product::io::paid(self, common, live_owner, |root, live| {
                    root.execute_input_invocation_inner(
                        call.module_id(),
                        None,
                        None,
                        None,
                        Some(&input),
                        live,
                    )
                }) {
                    Ok(output) => output,
                    Err(error) => return Err(error),
                };
            common.consume_steps(0).map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
            match (output, failure.into_inner()) {
                (Some(output), None) => Ok(PaidFactoryModuleExecutionV1 {
                    result: FactoryResultV1::Output(output),
                    transaction,
                    identity,
                    now,
                    action_id: call.action_id(),
                    staged_disposition: None,
                    _phase: outer_phase,
                }),
                (None, Some(failure)) => Ok(PaidFactoryModuleExecutionV1 {
                    result: FactoryResultV1::VmFailure(failure),
                    transaction,
                    identity,
                    now,
                    action_id: call.action_id(),
                    staged_disposition: None,
                    _phase: outer_phase,
                }),
                _ => Err(ValueTreeError::Invalid("due product execution result source").into()),
            }
        })();
        // Genuine VM and still-live original business failures retain their
        // provenance. Host, budget, cancellation and codec failures are fatal;
        // an application failure that already aborted the transaction is fatal too.
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}

impl PaidFactoryModuleExecutionV1 {
    pub(in crate::world) fn validate_evidence_identity<M: MerkleNodeReader>(
        &self,
        call: &crate::world::incremental_normal_action_factory_schedule::factory_module::PreparedFactoryModuleCallV1<'_>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        call.validate_evidence_identity(tx, common, now, live)?;
        tx.validate_transaction_identity(&self.transaction)?;
        tx.validate_guard(&tx.guard(), live)?;
        common
            .charge(
                1,
                call.module_id().len() as u64 + call.trace_id().len() as u64,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let paid = match &self.result {
            FactoryResultV1::Output(value) => value.uses_budget(common.memory()),
            FactoryResultV1::VmFailure(value) => {
                value.uses_budget(common.memory())
                    && matches!(
                &**value, WorldError::ModuleCallFailed { module_id, trace_id, .. }
                    if module_id == call.module_id() && trace_id == call.trace_id())
            }
        };
        if self.action_id != call.action_id()
            || self.now != now
            || self.identity != common.work().identity()
            || !self._phase.uses_budget(common.memory())
            || !paid
        {
            return Err(BodyMutationError::Invalid(
                "actual factory module execution identity",
            ));
        }
        Ok(())
    }
}

impl PaidFactoryModuleExecutionV1 {
    pub(in crate::world) fn validate_live_resolution(&self) -> Result<(), BodyMutationError> {
        if self.staged_disposition
            == Some(
                oasis7_formal_protocol::incremental_execution::StagedBranchDispositionV1::Discarded,
            )
        {
            return Err(BodyMutationError::Invalid(
                "discarded factory execution is historical DATA",
            ));
        }
        Ok(())
    }
    /// The concrete resolution owner separately enforces its allowed disposition.
    /// This only moves actual VM-result provenance via the exact Protocol receipt.
    pub(in crate::world) fn rejoin_build_execution_v1<M: MerkleNodeReader>(
        &mut self,
        call:&crate::world::incremental_normal_action_factory_schedule::factory_module::PreparedFactoryModuleCallV1<'_>,
        evidence: &oasis7_formal_protocol::incremental_execution::StagedBranchRejoinEvidenceV1,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        call.validate_evidence_identity(tx, common, now, live)?;
        if call.kind()!=crate::world::incremental_normal_action_factory_schedule::factory_module::FactoryModuleCallKindV1::Build||self.staged_disposition.is_some()||self.action_id!=call.action_id()||self.now!=now||self.identity!=common.work().identity()||!self._phase.uses_budget(common.memory()) {return Err(BodyMutationError::Invalid("factory execution rejoin binding"))}
        let paid = match &self.result {
            FactoryResultV1::Output(value) => value.uses_budget(common.memory()),
            FactoryResultV1::VmFailure(value) => {
                value.uses_budget(common.memory())
                    && matches!(&**value,WorldError::ModuleCallFailed{module_id,trace_id,..}if module_id==call.module_id()&&trace_id==call.trace_id())
            }
        };
        if !paid {
            return Err(BodyMutationError::Invalid(
                "factory execution rejoin result",
            ));
        }
        common
            .charge(
                1,
                call.module_id().len() as u64 + call.trace_id().len() as u64,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        self.transaction = evidence
            .rebind_source_guard(&self.transaction, tx, common, live)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        self.staged_disposition = Some(evidence.disposition());
        self.validate_evidence_identity(call, tx, common, now, live)
    }
}
