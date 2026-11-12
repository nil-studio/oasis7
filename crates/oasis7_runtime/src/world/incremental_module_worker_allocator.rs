//! Concrete same-pool allocator shared by actual sealed worker consumers.
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use oasis7_wasm_abi::{CanonicalCborAllocator, CanonicalCborStreamError};

/// A concrete adapter; an arbitrary allocator implementation cannot stand in
/// for the same process-wide ordinary pool on the production worker path.
pub struct RootModuleWorkerAllocatorV1 {
    pub(super) memory: MemoryBudget,
}
impl RootModuleWorkerAllocatorV1 {
    pub(in crate::world) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.memory.same_pool(memory)
    }
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
