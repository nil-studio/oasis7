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
    /// Reserve before a signed RPC Query may clone a recorded RequestState or
    /// serialize its response. A data-only Qualified state grants no permit.
    pub fn response_copy_workspace(&self) -> Result<usize, FormalError> {
        let data = match &self.state {
            Some(value) => value.response_copy_workspace()?,
            None => 0,
        };
        // An independent conservative envelope for the original signed
        // checkpoint, witness, response CBOR and temporary hash/sign buffers.
        data.checked_mul(2)
            .and_then(|n| n.checked_add(2 * 1024 * 1024))
            .ok_or_else(|| invalid("formal RPC Query response workspace overflow"))
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.state
            .as_ref()
            .is_none_or(|state| state.uses_budget(memory))
    }
}
impl FileFormalEndpoint {
    /// The private typed data loader is also directly exercised by the real
    /// five-state Store fixture; every error poisons the original endpoint.
    pub(super) fn load_bounded(
        &mut self,
        request: &str,
        memory: &MemoryBudget,
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
        self.load_bounded(request, memory, work, cancelled)
            .map(|state| PaidFormalRequestQueryV1 { state })
    }
}
