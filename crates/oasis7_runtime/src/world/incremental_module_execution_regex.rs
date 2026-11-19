//! The production router can only use the same concrete sealed worker as VM execution.
use super::RootModuleWorkerAllocatorV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};
use oasis7_wasm_abi::CanonicalCborStreamError;
use oasis7_wasm_executor::bounded_worker::{
    BoundedWorkerSandboxV1, RegexProbeV1, RegexResultsV1, RegexWorkerErrorV1,
};
use oasis7_wasm_router::bounded::{RegexBatchBackendV1, RegexBatchOutputV1};

pub(in crate::world) struct RootRegexBackendV1<'a, 'v> {
    worker: &'a mut BoundedWorkerSandboxV1<'v, RootModuleWorkerAllocatorV1>,
    memory: &'a MemoryBudget,
}
impl<'a, 'v> RootRegexBackendV1<'a, 'v> {
    pub(in crate::world) fn new(
        worker: &'a mut BoundedWorkerSandboxV1<'v, RootModuleWorkerAllocatorV1>,
        memory: &'a MemoryBudget,
    ) -> Self {
        Self { worker, memory }
    }
}
pub(in crate::world) struct RootRegexResultsV1(RegexResultsV1<MemoryReservation>);
impl RegexBatchOutputV1 for RootRegexResultsV1 {
    fn values(&self) -> &[bool] {
        self.0.values()
    }
}
impl RegexBatchBackendV1 for RootRegexBackendV1<'_, '_> {
    type Error = RegexWorkerErrorV1;
    type Output = RootRegexResultsV1;
    fn match_batch<C: FnMut() -> bool>(
        &mut self,
        probes: &[oasis7_wasm_router::bounded::RegexProbeV1<'_>],
        cancelled: &mut C,
    ) -> Result<Self::Output, Self::Error> {
        if !self.worker.allocator.memory.same_pool(self.memory) {
            return Err(RegexWorkerErrorV1::Admission(
                CanonicalCborStreamError::Invalid("ordinary regex worker pool"),
            ));
        }
        self.worker
            .match_regex_batch_iter_v1(
                probes.iter().map(|probe| RegexProbeV1 {
                    pattern: probe.pattern,
                    text: probe.text,
                }),
                cancelled,
            )
            .map(RootRegexResultsV1)
    }
}
