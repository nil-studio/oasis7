//! Borrow actual accepted holders while their endpoint, namespace and FD live.
//! This read view is data; it never grants a transaction or receipt permission.
use super::*;
use oasis7_formal_protocol::incremental_material::CheckedCommitCore;
use oasis7_world_state::{
    incremental_cognition_history::CompleteCognitionHistorySourceV3,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_world_storage_source::CheckedWorldStorageSourceV1,
};

pub struct HostAcceptedParentReadViewV1<'a> {
    core: &'a ReservedValue<CheckedCommitCore>,
    storage: &'a ReservedValue<CheckedWorldStorageSourceV1>,
    roots: &'a ReservedValue<oasis7_formal_protocol::incremental_execution::RootSetV1>,
    runtime: &'a ReservedValue<AcceptedRuntimeHistorySourceV1>,
    cognition: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
    head: &'a VerifiedFormalCommitV1,
    authority: &'a VerifiedAuthorityV1,
    pages: &'a crate::incremental_page_store::IncrementalPageStore,
}
impl HostAcceptedParentReadViewV1<'_> {
    pub fn core(&self) -> &ReservedValue<CheckedCommitCore> {
        self.core
    }
    pub fn storage(&self) -> &ReservedValue<CheckedWorldStorageSourceV1> {
        self.storage
    }
    pub fn roots(
        &self,
    ) -> &ReservedValue<oasis7_formal_protocol::incremental_execution::RootSetV1> {
        self.roots
    }
    pub fn runtime(&self) -> &ReservedValue<AcceptedRuntimeHistorySourceV1> {
        self.runtime
    }
    pub fn cognition(&self) -> &ReservedValue<CompleteCognitionHistorySourceV3> {
        self.cognition
    }
    pub fn head(&self) -> &VerifiedFormalCommitV1 {
        self.head
    }
    pub fn authority(&self) -> &VerifiedAuthorityV1 {
        self.authority
    }
    /// Content-addressed IO only; writing new pages does not publish a head.
    pub fn pages(&self) -> &crate::incremental_page_store::IncrementalPageStore {
        self.pages
    }
}
impl PaidAcceptedGenesisHostSourceV1 {
    pub fn with_parent_read_view_bounded<T, E>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(HostAcceptedParentReadViewV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        {
            let mut ctx = work.bounded_context(memory, stop);
            self.validate_context(&mut ctx)?;
        }
        let _phase = memory
            .reserve(
                std::mem::size_of::<HostAcceptedParentReadViewV1<'_>>()
                    + std::mem::size_of::<
                        std::cell::Ref<'_, Option<provisional::AcceptedCandidateCache>>,
                    >(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("Host parent view admission"))?;
        let capability = self
            .endpoint
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("Host accepted page source missing"))?;
        let slot = capability.jobs.parent()?;
        let cache = slot
            .as_ref()
            .ok_or_else(|| invalid("Host accepted parent missing"))?;
        let provisional = match &cache.current_owner {
            provisional::AcceptedCurrentOwner::Candidate(value) => value,
            provisional::AcceptedCurrentOwner::Cold { .. } => {
                return Err(invalid("Host live parent source required").into());
            }
        };
        let head = self.commit()?;
        let data = cache
            .complete_data()
            .ok_or_else(|| invalid("Host parent complete data missing"))?;
        work.charge(1, 32 * 10, stop)?;
        if !provisional.core.uses_budget(memory)
            || !provisional.source.uses_budget(memory)
            || !cache.roots.uses_budget(memory)
            || !cache.runtime.uses_budget(memory)
            || !cache.cognition.uses_budget(memory)
            || !data.uses_budget(memory)
            || data.page_namespace_identity() != self.page_namespace
            || data.native_namespace_identity() != self.native_namespace
            || data.body_id() != self.fence.parent_commit
            || data.roots_id() != self.fence.parent_roots
            || cache.commit_record != self.record
            || head.verified_record_id() != self.record
            || provisional.core.candidate_record_id() != self.record
            || !self.lease.uses_budget(memory)
        {
            return Err(invalid("Host accepted parent binding changed").into());
        }
        consume(HostAcceptedParentReadViewV1 {
            core: &provisional.core,
            storage: &provisional.source,
            roots: &cache.roots,
            runtime: &cache.runtime,
            cognition: &cache.cognition,
            head,
            authority: &self.endpoint.authority,
            pages: &capability.pages,
        })
    }
}
