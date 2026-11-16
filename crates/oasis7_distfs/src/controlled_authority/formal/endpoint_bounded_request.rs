//! Bounded request read with the endpoint's existing failure poison semantics.
use super::*;

impl FileFormalEndpoint {
    /// Paid data loading preserves the existing failure poison; it neither
    /// issues a serving permit nor replaces the formal freeze admission path.
    pub(super) fn load_bounded(
        &mut self,
        request: &str,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Option<store::PaidRequestStateV1>, FormalError> {
        let result = self
            .store
            .read_request_bounded(request, memory, work, cancelled);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}
