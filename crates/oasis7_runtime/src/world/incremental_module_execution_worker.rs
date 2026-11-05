//! The concrete sealed-worker consumer preserves host/VM failure provenance.
//! A generic raw sandbox is never sufficient to enter this execution path.
use super::incremental_module_execution_output::{
    RootModuleOutputErrorV1, RootModuleOutputWorkV1, call_admitted_root_module_output_v1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError, ModuleCallFailure, ModuleCallRequest,
    ModuleOutput, ModuleSandbox,
    module_raw_output::{ModuleRawOutputSinkV1, ModuleSandboxRawOutputV1},
};
use oasis7_wasm_executor::bounded_worker::{
    BoundedWorkerSandboxV1, WorkerExecutionErrorV1, WorkerHostFailureV1,
};

/// A concrete adapter; an arbitrary allocator implementation cannot stand in
/// for the same process-wide ordinary pool on the production worker path.
pub struct RootModuleWorkerAllocatorV1 {
    memory: MemoryBudget,
}
impl RootModuleWorkerAllocatorV1 {
    pub fn new(memory: &MemoryBudget) -> Self {
        Self {
            memory: memory.clone(),
        }
    }
}
impl CanonicalCborAllocator for RootModuleWorkerAllocatorV1 {
    type Charge = MemoryReservation;
    fn reserve(&self, bytes: usize) -> Result<Self::Charge, CanonicalCborStreamError> {
        self.memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|_| CanonicalCborStreamError::ResourceLimited("ordinary worker process pool"))
    }
}

#[derive(Debug)]
pub enum RootModuleWorkerErrorV1 {
    Output(RootModuleOutputErrorV1),
    Host(ReservedValue<WorkerHostFailureV1>),
    Value(ValueTreeError),
    Transaction(ExecutionError),
}

/// Adapt only the real typed worker into the existing admitted output decoder.
/// Host provenance is retained separately; the legacy failure merely lets that
/// decoder unwind its real sink/VM reservations through its normal error path.
struct TypedWorker<'a, 'worker> {
    worker: &'a mut BoundedWorkerSandboxV1<'worker, RootModuleWorkerAllocatorV1>,
    host: Option<WorkerHostFailureV1>,
}
impl ModuleSandbox for TypedWorker<'_, '_> {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.worker.call(request)
    }
}
impl ModuleSandboxRawOutputV1 for TypedWorker<'_, '_> {
    fn call_to(
        &mut self,
        request: &ModuleCallRequest,
        sink: &mut dyn ModuleRawOutputSinkV1,
    ) -> Result<(), ModuleCallFailure> {
        match self.worker.call_to_typed_v1(request, sink) {
            Ok(()) => Ok(()),
            Err(WorkerExecutionErrorV1::Vm(failure)) => Err(failure),
            Err(WorkerExecutionErrorV1::Host(host)) => {
                let legacy = host.legacy_failure().clone();
                self.host = Some(host);
                Err(legacy)
            }
        }
    }
}

/// Host failure poisons the caller's sole transaction. Only a genuine worker
/// VM frame can retain the established business ModuleCallFailure semantics.
/// The worker itself rejects FixturePath on this typed entry; the final driver
/// must additionally bind its SealedFd to the authenticated Native baseline.
pub fn execute_root_module_worker_v1<M, C>(
    transaction: &mut ExecutionTransaction<'_, M>,
    worker: &mut BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
    request: &ModuleCallRequest,
    memory: &MemoryBudget,
    work: &mut RootModuleOutputWorkV1,
    mut cancelled: C,
) -> Result<ReservedValue<ModuleOutput>, RootModuleWorkerErrorV1>
where
    M: MerkleNodeReader,
    C: FnMut() -> bool,
{
    let stopped = cancelled();
    if stopped || !transaction.uses_budget(memory) || !worker.allocator.memory.same_pool(memory) {
        return transaction.contribute(|_| {
            Err(RootModuleWorkerErrorV1::Value(if stopped {
                ValueTreeError::Cancelled
            } else {
                ValueTreeError::Invalid("module worker transaction pool")
            }))
        });
    }
    // Worker identities are at most 256 bytes each and diagnostics at most
    // 4096 bytes. The phase
    // remains live while its legacy unwind copy and typed host owner coexist.
    let fee = match memory.reserve(
        std::mem::size_of::<WorkerHostFailureV1>() + (16 << 10),
        MemoryClass::Ordinary,
    ) {
        Ok(fee) => fee,
        Err(error) => {
            return transaction.contribute(|_| {
                Err(RootModuleWorkerErrorV1::Value(ValueTreeError::Memory(
                    error,
                )))
            });
        }
    };
    let mut backend = TypedWorker { worker, host: None };
    let output =
        call_admitted_root_module_output_v1(&mut backend, request, memory, work, &mut cancelled);
    if let Some(host) = backend.host.take() {
        let error = RootModuleWorkerErrorV1::Host(fee.retain(host));
        return transaction.contribute(|_| Err(error));
    }
    if cancelled() {
        return transaction
            .contribute(|_| Err(RootModuleWorkerErrorV1::Value(ValueTreeError::Cancelled)));
    }
    match output {
        Ok(output) => Ok(output),
        Err(error @ RootModuleOutputErrorV1::Value(_)) => {
            transaction.contribute(|_| Err(RootModuleWorkerErrorV1::Output(error)))
        }
        Err(error) => Err(RootModuleWorkerErrorV1::Output(error)),
    }
}
impl From<ExecutionError> for RootModuleWorkerErrorV1 {
    fn from(error: ExecutionError) -> Self {
        Self::Transaction(error)
    }
}

#[path = "incremental_module_execution_regex.rs"]
mod regex;
pub(super) use regex::RootRegexBackendV1;
