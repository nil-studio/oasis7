//! A fresh accepted GEN owner installed only after real native admission.
//! This local slot is neither WorldReady nor global parent-lease authority.
use oasis7_distfs::controlled_authority::{
    fencing::{BoundedAcquireErrorV1, RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1},
    formal::endpoint::{
        FileFormalEndpoint, FormalArchiveReadWorkV1, PaidAcceptedGenesisHostSourceV1,
    },
};
use oasis7_formal_protocol::{
    controlled_authority::formal::FormalError,
    incremental_execution::{ExecutionError, ExecutionFence},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    incremental_cache_move::RuntimeCacheSlotV1,
    incremental_module_execution_worker::RootModuleWorkerAllocatorV1,
};
use oasis7_wasm_abi::ModuleCallRequest;
use oasis7_wasm_executor::bounded_worker::{
    BoundedWorkerSandboxV1, RegexWorkerErrorV1, WorkerExecutableV1, WorkerExecutionErrorV1,
    WorkerLimitsV1,
};
use std::{
    mem::size_of,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug)]
pub enum IwsHostRuntimeErrorV1 {
    Source(BoundedAcquireErrorV1),
    Formal(FormalError),
    Move(ExecutionError),
    Protocol(WorkerExecutionErrorV1),
    Environment(RegexWorkerErrorV1),
}
impl From<FormalError> for IwsHostRuntimeErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Formal(e)
    }
}
impl std::fmt::Display for IwsHostRuntimeErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IWS Host admission: {self:?}")
    }
}
impl std::error::Error for IwsHostRuntimeErrorV1 {}
struct HostCurrentV1 {
    source: PaidAcceptedGenesisHostSourceV1,
    limits: WorkerLimitsV1,
}
/// The slot is private. A public generic Runtime payload cannot enter it.
#[derive(Default)]
pub struct IwsHostRuntimeV1 {
    current: RuntimeCacheSlotV1<HostCurrentV1>,
}
impl IwsHostRuntimeV1 {
    pub fn is_installed(&self) -> bool {
        !self.current.is_empty()
    }
    /// Read from the actual installed owner, never from a caller callback.
    /// Possession of these bytes does not create execution permission.
    pub fn current_fence(&self) -> Option<ExecutionFence> {
        self.current.current().map(|v| v.source.fence())
    }
    pub fn current_record_id(&self) -> Option<[u8; 32]> {
        self.current.current().map(|v| v.source.record_id())
    }
    pub fn generation(&self) -> u64 {
        self.current.generation()
    }
    pub fn current_namespace_identities(&self) -> Option<((u64, u64), (u64, u64))> {
        self.current.current().map(|v| {
            (
                v.source.page_namespace_identity(),
                v.source.native_namespace_identity(),
            )
        })
    }
    /// Entire real endpoint ownership is consumed. Error drops its FD leases;
    /// the old slot is untouched. Historical/cold/staged data cannot enter.
    pub fn install_genesis_bounded(
        &mut self,
        endpoint: FileFormalEndpoint,
        provider: &RemoteMonotonicEndpointProvider,
        pinned: &TrustedFencingPolicyV1,
        limits: WorkerLimitsV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        work.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
        if !self.current.is_empty() {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
        }
        let source = endpoint
            .into_accepted_genesis_host_source_bounded(provider, pinned, memory, work, &mut || {
                cancelled.load(Ordering::Acquire)
            })
            .map_err(IwsHostRuntimeErrorV1::Source)?;
        self.install_accepted_genesis_source_bounded(source, limits, memory, work, cancelled)
    }
    /// Consumes only the private-mint live source. Generic cache payloads cannot
    /// call this constructor with a fabricated accepted namespace or FD.
    pub fn install_accepted_genesis_source_bounded(
        &mut self,
        source: PaidAcceptedGenesisHostSourceV1,
        limits: WorkerLimitsV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        work.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
        if !self.current.is_empty() {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
        }
        let current = HostCurrentV1 { source, limits };
        admit_worker(&current, memory, work, cancelled)?;
        // All fallible protocol/environment checks precede preparing the slot.
        let prepared = self
            .current
            .prepare_empty_bounded(current, memory, &mut || cancelled.load(Ordering::Acquire))
            .map_err(IwsHostRuntimeErrorV1::Move)?;
        drop(prepared.finish());
        Ok(())
    }
    /// Re-admit the exact current owner. No raw sandbox, executable path,
    /// caller fence or arbitrary module request is exposed to the consumer.
    pub fn verify_current_worker_bounded(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        let current = self
            .current
            .current()
            .ok_or_else(|| FormalError::Invalid("Host current owner missing".into()))?;
        admit_worker(current, memory, work, cancelled)
    }
}
fn admit_worker(
    current: &HostCurrentV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<(), IwsHostRuntimeErrorV1> {
    with_admitted_worker(current, memory, work, cancelled, |_| Ok(()))
}
fn with_admitted_worker<T>(
    current: &HostCurrentV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
    run: impl FnOnce(
        &mut BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
    ) -> Result<T, IwsHostRuntimeErrorV1>,
) -> Result<T, IwsHostRuntimeErrorV1> {
    {
        let mut stop = || cancelled.load(Ordering::Acquire);
        let mut ctx = work.bounded_context(memory, &mut stop);
        current.source.validate_context(&mut ctx)?;
    }
    // Explicit stack/header fee, including the request's empty Arc allocation.
    let bytes = size_of::<ModuleCallRequest>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|n| {
            n.checked_add(size_of::<
                BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
            >())
        })
        .and_then(|n| n.checked_add(size_of::<RootModuleWorkerAllocatorV1>()))
        .ok_or_else(|| FormalError::Invalid("Host native admission layout".into()))?;
    let _fee = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| FormalError::Invalid("Host native admission memory".into()))?;
    let allocator = RootModuleWorkerAllocatorV1::new(memory);
    work.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
    current.source.with_native_worker(
        memory,
        work,
        &mut || cancelled.load(Ordering::Acquire),
        |lease| {
            work.charge(1, lease.descriptor_bytes().len() as u64, &mut || {
                cancelled.load(Ordering::Acquire)
            })?;
            if *blake3::hash(lease.descriptor_bytes()).as_bytes()
                != current.source.execution_environment()
            {
                return Err(FormalError::Invalid(
                    "Host same accepted native descriptor/environment".into(),
                )
                .into());
            }
            let mut worker = BoundedWorkerSandboxV1 {
                executable: WorkerExecutableV1::NativeBoundSealedFd {
                    file: lease.file(),
                    descriptor: lease.descriptor_bytes(),
                },
                allocator: &allocator,
                limits: WorkerLimitsV1 {
                    address_space_bytes: current.limits.address_space_bytes,
                    max_request_bytes: current.limits.max_request_bytes,
                    max_output_bytes: current.limits.max_output_bytes,
                    wall_time_ms: current.limits.wall_time_ms,
                },
                cancelled,
            };
            let request = ModuleCallRequest {
                module_id: String::new(),
                wasm_hash: String::new(),
                trace_id: String::new(),
                entrypoint: String::new(),
                input: Vec::new(),
                wasm_bytes: Default::default(),
                limits: Default::default(),
            };
            work.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
            worker
                .verify_native_protocol_v1(&request)
                .map_err(IwsHostRuntimeErrorV1::Protocol)?;
            work.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
            // This enters the actual native-bound environment-aware child route.
            // The handshake alone does not prove descriptor/environment equivalence.
            let results = worker
                .match_regex_batch_v1(&[], &mut || cancelled.load(Ordering::Acquire))
                .map_err(IwsHostRuntimeErrorV1::Environment)?;
            if !results.values().is_empty() {
                return Err(
                    FormalError::Invalid("Host native empty admission result".into()).into(),
                );
            }
            work.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
            run(&mut worker)
        },
    )
}
#[cfg(test)]
#[path = "iws_host_runtime_tests.rs"]
mod tests;

#[path = "iws_host_normal.rs"]
mod normal;
