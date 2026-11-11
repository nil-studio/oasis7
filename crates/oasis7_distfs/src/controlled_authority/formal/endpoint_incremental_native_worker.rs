//! Borrow the exact accepted endpoint native baseline throughout a Host call.
use super::*;
impl FileFormalEndpoint {
    pub fn with_iws_native_worker_v1<T, E>(
        &self,
        commit: &VerifiedFormalCommitV1,
        memory: &MemoryBudget,
        cancelled: &mut impl FnMut() -> bool,
        consume: impl FnOnce(closure::NativeWorkerLeaseV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("native worker IWS capability missing"))?;
        let accepted = capability.jobs.parent()?;
        let accepted = accepted
            .as_ref()
            .ok_or_else(|| invalid("native worker accepted baseline missing"))?;
        let data = accepted
            .complete_data()
            .ok_or_else(|| invalid("native worker complete data missing"))?;
        if !data.uses_budget(memory) || data.candidate_record_id() != commit.verified_record_id() {
            return Err(invalid("native worker exact accepted commit/pool").into());
        }
        // Keep the RefCell borrow and complete-data owner live until the real
        // bounded Host invocation (including child wait) has returned.
        let lease = data.material().native_worker_lease_v1(memory, cancelled)?;
        consume(lease)
    }
}
