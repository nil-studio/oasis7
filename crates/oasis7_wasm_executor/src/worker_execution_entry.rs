//! Private shared wrappers; legacy passes no worker classification context.
use super::*;
impl WasmExecutor {
    pub(super) fn call_with_raw_output<T>(
        &self,
        request: &ModuleCallRequest,
        consume: impl FnOnce(&[u8]) -> Result<T, ModuleCallFailure>,
    ) -> Result<T, ModuleCallFailure> {
        self.call_with_raw_output_context(
            request,
            consume,
            #[cfg(feature = "wasmtime")]
            None,
        )
    }
    pub(crate) fn call_for_bounded_worker_v1<T>(
        &self,
        request: &ModuleCallRequest,
        consume: impl FnOnce(&[u8]) -> Result<T, ModuleCallFailure>,
    ) -> Result<T, bounded_worker::WorkerExecutionErrorV1> {
        #[cfg(feature = "wasmtime")]
        {
            let context = Arc::new(WorkerExecutionContextV1::new());
            self.call_with_raw_output_context(request, consume, Some(&context))
                .map_err(|failure| context.classify(failure))
        }
        #[cfg(not(feature = "wasmtime"))]
        self.call_with_raw_output_context(request, consume)
            .map_err(bounded_worker::host_failure)
    }
}
