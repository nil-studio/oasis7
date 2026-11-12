//! Exact economy-action input through the existing selected sealed worker.
//! This is a component data result, not a whole-tick or publication permit.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;

enum ProductResultV1 {
    Output(ReservedValue<ModuleOutput>),
    VmFailure(ReservedValue<WorldError>),
    BusinessFailure(ReservedValue<WorldError>),
}
pub(in crate::world) struct PaidDueProductExecutionV1 {
    result: ProductResultV1,
    transaction: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    now: u64,
    _phase: oasis7_proto::memory_budget::MemoryReservation,
}
impl PaidDueProductExecutionV1 {
    pub(in crate::world) fn output(&self) -> Option<&ModuleOutput> {
        match &self.result {
            ProductResultV1::Output(value) => Some(value),
            _ => None,
        }
    }
    pub(in crate::world) fn business_failure(&self) -> Option<&WorldError> {
        match &self.result {
            ProductResultV1::VmFailure(value) | ProductResultV1::BusinessFailure(value) => {
                Some(value)
            }
            _ => None,
        }
    }
    pub(in crate::world) fn vm_failure(&self) -> Option<&WorldError> {
        match &self.result {
            ProductResultV1::VmFailure(value) => Some(value),
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
    pub(in crate::world) fn execute_due_product_request_v1(
        &mut self,
        call: &crate::world::incremental_normal_industry_due::PreparedDueProductCallV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PaidDueProductExecutionV1, RootModuleExecutionErrorV1<O::Error>> {
        let transaction = self.ctx.transaction.guard();
        let identity = common.work().identity();
        let now = self.now;
        let result = (|| {
            let outer_bytes = std::mem::size_of::<PaidDueProductExecutionV1>();
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
            call.validate(self.ctx.transaction, common, self.now, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            if !self.ctx.values.memory.same_pool(common.memory()) {
                return Err(ValueTreeError::Invalid("due product Common pool").into());
            }
            let failure = RefCell::new(None);
            let input = DueProductInputV1 {
                call,
                failure: &failure,
            };
            let output = match io::paid(self, common, live_owner, |root, live| {
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
                Err(RootModuleExecutionErrorV1::Business(error)) => {
                    // The original economy consumer converts a normal business
                    // rejection to a persisted product rejection. An already
                    // poisoned transaction or changed fence is never resumed.
                    self.ctx
                        .transaction
                        .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
                    common.consume_steps(0).map_err(|e| {
                        RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
                    })?;
                    return Ok(PaidDueProductExecutionV1 {
                        result: ProductResultV1::BusinessFailure(error),
                        transaction,
                        identity,
                        now,
                        _phase: outer_phase,
                    });
                }
                Err(error) => return Err(error),
            };
            common.consume_steps(0).map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
            match (output, failure.into_inner()) {
                (Some(output), None) => Ok(PaidDueProductExecutionV1 {
                    result: ProductResultV1::Output(output),
                    transaction,
                    identity,
                    now,
                    _phase: outer_phase,
                }),
                (None, Some(failure)) => Ok(PaidDueProductExecutionV1 {
                    result: ProductResultV1::VmFailure(failure),
                    transaction,
                    identity,
                    now,
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

#[path = "incremental_module_execution_due_product_io.rs"]
mod io;

/// Genuine sealed-worker failure; the private execution owner retains its fee.
/// This borrowed data source grants neither a replay nor a business permit.
pub(in crate::world) struct PaidDueProductVmFailureV1<'a> {
    failure: &'a ReservedValue<WorldError>,
    transaction: &'a oasis7_formal_protocol::incremental_execution::PreparedGuard,
    identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    now: u64,
    _phase: oasis7_proto::memory_budget::MemoryReservation,
}
impl PaidDueProductVmFailureV1<'_> {
    pub(in crate::world) fn failure(&self) -> &WorldError {
        self.failure
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
        identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.transaction)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.identity != identity
            || self.now != now
            || !tx.uses_budget(memory)
            || !self.failure.uses_budget(memory)
            || !matches!(&**self.failure, WorldError::ModuleCallFailed { .. })
        {
            return Err(BodyMutationError::Invalid("due actual VM failure identity"));
        }
        Ok(())
    }
}
struct TraceCompare<'a> {
    value: &'a [u8],
    offset: usize,
}
impl std::fmt::Write for TraceCompare<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let end = self.offset.checked_add(text.len()).ok_or(std::fmt::Error)?;
        if self.value.get(self.offset..end) != Some(text.as_bytes()) {
            return Err(std::fmt::Error);
        }
        self.offset = end;
        Ok(())
    }
}
impl PaidDueProductExecutionV1 {
    pub(in crate::world) fn vm_failure_source_v1<'a, M: MerkleNodeReader>(
        &'a self,
        call: &crate::world::incremental_normal_industry_due::PreparedDueProductCallV1<'_>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<Option<PaidDueProductVmFailureV1<'a>>, BodyMutationError> {
        use std::fmt::Write as _;
        let ProductResultV1::VmFailure(failure) = &self.result else {
            return Ok(None);
        };
        call.validate_evidence_identity(tx, common, now, live)?;
        let shell_bytes = std::mem::size_of::<PaidDueProductVmFailureV1<'_>>();
        common
            .charge(1, shell_bytes as u64)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        let shell = common
            .memory()
            .reserve(
                shell_bytes,
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let source = PaidDueProductVmFailureV1 {
            failure,
            transaction: &self.transaction,
            identity: self.identity,
            now: self.now,
            _phase: shell,
        };
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        let WorldError::ModuleCallFailed {
            module_id,
            trace_id,
            ..
        } = &**failure
        else {
            return Err(BodyMutationError::Invalid("due VM failure kind"));
        };
        common
            .charge(1, (module_id.len() + trace_id.len()) as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let mut trace = TraceCompare {
            value: trace_id.as_bytes(),
            offset: 0,
        };
        if module_id != call.module_id()
            || write!(
                &mut trace,
                "action-{}-{}-product-{}",
                call.job().job_id,
                call.module_id(),
                call.output_index()
            )
            .is_err()
            || trace.offset != trace.value.len()
        {
            return Err(BodyMutationError::Invalid(
                "due VM failure actual call binding",
            ));
        }
        Ok(Some(source))
    }
}
