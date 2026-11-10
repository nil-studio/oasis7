//! Narrow accepted-Host entry around the original target, with no new execution authority.
//! Mounted as a child of target::entry so no duplicate DTO or target kernel is needed.
use super::*;
use oasis7_proto::memory_budget::MemoryBudget;
use oasis7_wasm_executor::bounded_worker::{WorkerExecutionErrorV1, WorkerHostFailureV1};

/// Called at the original target's actual request boundary, before typed VM IO.
/// Existing component fixtures keep their original path; every NativeBound call
/// must execute this hook, including calls reached through immediate routing.
/// The actual native environment is still verified inside the typed child.
pub(super) fn verify_scoped_native_protocol_v1(
    worker: &BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
    request: &ModuleCallRequest,
    memory: &MemoryBudget,
    work: &RootModuleOutputWorkV1,
) -> Result<(), RootModuleWorkerErrorV1> {
    if !matches!(
        worker.executable,
        WorkerExecutableV1::NativeBoundSealedFd { .. }
    ) {
        return Ok(());
    }
    if work.calls_remaining == 0
        || request.module_id.len() > work.max_identity_bytes
        || request.trace_id.len() > work.max_identity_bytes
    {
        return Err(RootModuleWorkerErrorV1::Value(
            ValueTreeError::ResourceLimited("native protocol invocation admission"),
        ));
    }
    let bytes = request
        .module_id
        .len()
        .checked_add(request.trace_id.len())
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| {
            n.checked_add(
                oasis7_wasm_abi::module_raw_output::RAW_SANDBOX_FAILURE_DETAIL_MAX_BYTES_V1 * 2,
            )
        })
        .and_then(|n| n.checked_add(std::mem::size_of::<WorkerHostFailureV1>() + 8192))
        .ok_or(RootModuleWorkerErrorV1::Value(
            ValueTreeError::ResourceLimited("native protocol diagnostic admission"),
        ))?;
    let fee = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|error| RootModuleWorkerErrorV1::Value(ValueTreeError::Memory(error)))?;
    match worker.verify_native_protocol_v1(request) {
        Ok(()) => Ok(()),
        Err(WorkerExecutionErrorV1::Host(error)) => {
            Err(RootModuleWorkerErrorV1::Host(fee.retain(error)))
        }
        // A protocol probe can never authorize a business frame. Do not turn
        // an unexpected probe VM error into a committable module rejection.
        Err(WorkerExecutionErrorV1::Vm(_)) => Err(RootModuleWorkerErrorV1::Value(
            ValueTreeError::Invalid("native protocol returned a VM business frame"),
        )),
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
    /// The private Host scope supplies the actual accepted head and native-bound
    /// worker. All qualifications, Current readers, originals, counters and
    /// continuous work remain the original context's owners. This is component
    /// execution, not a full tick permit, candidate seal or durable acceptance.
    pub fn execute_scoped_root_tick_invocation_v1(
        &mut self,
        accepted: &VerifiedFormalCommitV1,
        invocation_id: &str,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        let result = (|| {
            if !matches!(
                self.worker.executable,
                WorkerExecutableV1::NativeBoundSealedFd { .. }
            ) || self.families.head.verified_record_id() != accepted.verified_record_id()
                || self.ctx.values.class != MemoryClass::Ordinary
                || !self.ctx.transaction.uses_budget(self.ctx.values.memory)
                || !self.manifest_cache.uses_budget(self.ctx.values.memory)
                || (self.ctx.cancelled)()
            {
                return Err(
                    ValueTreeError::Invalid("accepted native scoped target binding").into(),
                );
            }
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
            let output = self.execute_tick_invocation_v1(invocation_id, live_owner)?;
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
            Ok(output)
        })();
        if result.is_err() {
            // Preserve the target's first concrete error. Business behavior is
            // decided by its original consumer; this mirrors its sticky abort.
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}

#[cfg(test)]
#[path = "incremental_module_execution_scoped_tests.rs"]
mod tests;
