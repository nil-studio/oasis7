//! Data-only paid formal request query. This is not accepted GEN/Frozen, a
//! qualification permit, or an implicit budget for the legacy RPC query.
use super::*;
use oasis7_proto::memory_budget::MemoryBudget;

/// An opaque, budget-retaining local query result. A caller must keep this
/// owner alive while it consumes the original typed state. No detach/clone.
#[derive(Debug)]
pub struct PaidFormalRequestQueryV1 {
    state: Option<store::PaidRequestStateV1>,
}
impl PaidFormalRequestQueryV1 {
    pub fn state(&self) -> Option<&FormalRequestStateV1> {
        self.state.as_ref().map(store::PaidRequestStateV1::state)
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.state
            .as_ref()
            .is_none_or(|state| state.uses_budget(memory))
    }
}
impl FileFormalEndpoint {
    /// A real local endpoint consumer for the same caller-owned Ordinary pool,
    /// continuing archive meter and cancellation. Unlike legacy query(), this
    /// never creates a work allowance, and retains typed decode memory fees.
    /// A remote caller must supply its own explicitly budgeted protocol; this
    /// API does not silently weaken or replace that transport contract.
    pub fn query_data_bounded(
        &mut self,
        request: &str,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<PaidFormalRequestQueryV1, FormalError> {
        let _guard = self.guard()?;
        let result = self.store.read_request_bounded(request, memory, work, cancelled);
        if result.is_err() {
            self.poisoned = true;
        }
        result.map(|state| PaidFormalRequestQueryV1 { state })
    }
}
