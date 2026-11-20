//! Private per-call classification. Only actual VM traps/returned entrypoints
//! can produce business failure; any resource fault remains sticky and wins.
use super::*;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
pub(crate) struct WorkerExecutionContextV1 {
    resource: AtomicBool,
    declared_refusal: AtomicBool,
    stage: AtomicU8, // 0 preparation, 1 actual returned entrypoint, 2 VM trap, 3 host sink
}
impl WorkerExecutionContextV1 {
    pub(crate) fn new() -> Self {
        Self {
            resource: AtomicBool::new(false),
            declared_refusal: AtomicBool::new(false),
            stage: AtomicU8::new(0),
        }
    }
    pub(crate) fn returned(&self) {
        self.stage.store(1, Ordering::Release);
    }
    pub(crate) fn consuming(&self) {
        self.stage.store(3, Ordering::Release);
    }
    fn growth_failure(&self, error: &wasmtime::Error) {
        if typed_resource_failure(error) {
            self.resource.store(true, Ordering::Release);
        } else {
            self.declared_refusal.store(true, Ordering::Release);
        }
    }
    pub(crate) fn resource_failed(&self) -> bool {
        self.resource.load(Ordering::Acquire)
    }
    pub(crate) fn classify(
        &self,
        failure: ModuleCallFailure,
    ) -> bounded_worker::WorkerExecutionErrorV1 {
        let stage = self.stage.load(Ordering::Acquire);
        if !self.resource_failed()
            && stage != 3
            && (stage == 1 || stage == 2 || self.declared_refusal.load(Ordering::Acquire))
        {
            bounded_worker::WorkerExecutionErrorV1::Vm(failure)
        } else {
            bounded_worker::host_failure(failure)
        }
    }
}
// Closed real allocator causes, not diagnostics. In particular Wasmtime's
// Unix mmap/mprotect path preserves rustix::Errno directly, not std::io::Error.
fn typed_resource_failure(error: &wasmtime::Error) -> bool {
    if error.is::<wasmtime::OutOfMemory>()
        || error.is::<std::io::Error>()
        || error.is::<std::collections::TryReserveError>()
    {
        return true;
    }
    #[cfg(unix)]
    if error.is::<rustix::io::Errno>() {
        return true;
    }
    false
}
impl WasmExecutor {
    pub(crate) fn map_worker_execution_error(
        &self,
        request: &ModuleCallRequest,
        error: wasmtime::Error,
        context: Option<&Arc<WorkerExecutionContextV1>>,
    ) -> ModuleCallFailure {
        if let Some(context) = context {
            if typed_resource_failure(&error) {
                context.resource.store(true, Ordering::Release);
            } else if error.downcast_ref::<wasmtime::Trap>().is_some() {
                context.stage.store(2, Ordering::Release);
            }
        }
        self.map_wasmtime_error(request, error)
    }
}
impl wasmtime::ResourceLimiter for WasmStoreState {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let result =
            wasmtime::ResourceLimiter::memory_growing(&mut self.limits, current, desired, maximum);
        // A declared ModuleLimits/Wasm maximum keeps StoreLimits' original
        // deterministic trap/-1 policy. It is not a host allocator failure.
        if let (Err(error), Some(context)) = (&result, &self.worker) {
            context.growth_failure(error);
        }
        result
    }
    fn memory_grow_failed(&mut self, error: wasmtime::Error) -> wasmtime::Result<()> {
        if let Some(context) = &self.worker {
            context.growth_failure(&error);
        }
        let result = wasmtime::ResourceLimiter::memory_grow_failed(&mut self.limits, error);
        if let (Err(error), Some(context)) = (&result, &self.worker) {
            context.growth_failure(error);
        }
        result
    }
    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let result =
            wasmtime::ResourceLimiter::table_growing(&mut self.limits, current, desired, maximum);
        // A declared ModuleLimits/Wasm maximum keeps StoreLimits' original
        // deterministic trap/-1 policy. It is not a host allocator failure.
        if let (Err(error), Some(context)) = (&result, &self.worker) {
            context.growth_failure(error);
        }
        result
    }
    fn table_grow_failed(&mut self, error: wasmtime::Error) -> wasmtime::Result<()> {
        if let Some(context) = &self.worker {
            context.growth_failure(&error);
        }
        let result = wasmtime::ResourceLimiter::table_grow_failed(&mut self.limits, error);
        if let (Err(error), Some(context)) = (&result, &self.worker) {
            context.growth_failure(error);
        }
        result
    }
    fn instances(&self) -> usize {
        wasmtime::ResourceLimiter::instances(&self.limits)
    }
    fn tables(&self) -> usize {
        wasmtime::ResourceLimiter::tables(&self.limits)
    }
    fn memories(&self) -> usize {
        wasmtime::ResourceLimiter::memories(&self.limits)
    }
}
