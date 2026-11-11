//! The actual Wasmtime path shares execution with the legacy call; only the
//! final host consumer differs. No typed output decode precedes the raw sink.
use super::*;
use oasis7_wasm_abi::module_raw_output::{
    ModuleRawOutputSinkV1, ModuleSandboxRawOutputV1, RAW_SANDBOX_FAILURE_DETAIL_MAX_BYTES_V1,
};
fn bounded_failure(mut failure: ModuleCallFailure) -> ModuleCallFailure {
    let mut end = failure
        .detail
        .len()
        .min(RAW_SANDBOX_FAILURE_DETAIL_MAX_BYTES_V1);
    while !failure.detail.is_char_boundary(end) {
        end -= 1;
    }
    // A new bounded allocation is necessary: truncate alone retains the
    // potentially large native diagnostic's original String capacity.
    let detail = failure.detail[..end].to_owned();
    failure.detail = detail;
    failure
}
impl ModuleSandboxRawOutputV1 for WasmExecutor {
    fn call_to(
        &mut self,
        request: &ModuleCallRequest,
        sink: &mut dyn ModuleRawOutputSinkV1,
    ) -> Result<(), ModuleCallFailure> {
        self.call_with_raw_output(request, |bytes| sink.consume_output(bytes))
            .map_err(bounded_failure)
    }
}
