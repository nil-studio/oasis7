//! Read-only preaccept access to the exact staged candidate. This is not an
//! accepted parent, a sandbox loan or publication permission.
use super::*;
pub struct NormalReadyCandidateReadViewV1<'a> {
    core: &'a ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore>,
    storage: &'a ReservedValue<
        oasis7_world_state::incremental_world_storage_source::CheckedWorldStorageSourceV1,
    >,
    roots: &'a ReservedValue<oasis7_formal_protocol::incremental_execution::RootSetV1>,
    proof_ledger: Option<
        &'a ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::CheckedCognitionProofLedgerV1,
        >,
    >,
    quorum: &'a PaidVerifiedNormalCommitV1,
    parent_record: [u8; 32],
    namespace: ((u64, u64), (u64, u64)),
    pages: &'a crate::incremental_page_store::IncrementalPageStore,
}
impl NormalReadyCandidateReadViewV1<'_> {
    pub fn parent_record_id(&self) -> [u8; 32] {
        self.parent_record
    }
    pub fn namespace_identities(&self) -> ((u64, u64), (u64, u64)) {
        self.namespace
    }
    pub fn core(
        &self,
    ) -> &ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore> {
        self.core
    }
    pub fn storage(
        &self,
    ) -> &ReservedValue<
        oasis7_world_state::incremental_world_storage_source::CheckedWorldStorageSourceV1,
    > {
        self.storage
    }
    pub fn roots(
        &self,
    ) -> &ReservedValue<oasis7_formal_protocol::incremental_execution::RootSetV1> {
        self.roots
    }
    pub fn quorum(&self) -> &PaidVerifiedNormalCommitV1 {
        self.quorum
    }
    /// The exact checked candidate source, retained with its original charge.
    /// This borrow does not confer accepted-source or publication authority.
    pub fn proof_ledger(
        &self,
    ) -> Option<
        &ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::CheckedCognitionProofLedgerV1,
        >,
    > {
        self.proof_ledger
    }
    pub fn pages(&self) -> &crate::incremental_page_store::IncrementalPageStore {
        self.pages
    }
}
impl PaidNormalReadyPairV1 {
    pub fn with_primary_candidate_read_view_bounded<T, E>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(NormalReadyCandidateReadViewV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        self.with_candidate_read_view_bounded(EndpointRole::Primary, memory, work, stop, consume)
    }
    pub fn with_replica_candidate_read_view_bounded<T, E>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(NormalReadyCandidateReadViewV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        self.with_candidate_read_view_bounded(EndpointRole::Replica, memory, work, stop, consume)
    }
    fn with_candidate_read_view_bounded<T, E>(
        &self,
        role: EndpointRole,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(NormalReadyCandidateReadViewV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        self.owned
            .value
            .quorum
            .validate_context(&mut work.bounded_context(memory, stop))?;
        work.charge(32, 8192, stop)?;
        if !self.uses_budget(memory) {
            return Err(invalid("normal candidate view pool").into());
        }
        let source = match role {
            EndpointRole::Primary => &self.owned.value.pair.owned.value.primary.owned.value.source,
            EndpointRole::Replica => &self.owned.value.pair.owned.value.replica.owned.value.source,
        };
        if source.endpoint.role != role {
            return Err(invalid("normal candidate actual endpoint role").into());
        }
        let lease = source
            .lease
            .as_ref()
            .ok_or_else(|| invalid("normal candidate actual retained lease missing"))?;
        if source.identity != work.bounded_context(memory, stop).work().identity()
            || !lease.uses_budget(memory)
        {
            return Err(invalid("normal candidate retained source Core/pool").into());
        }
        let capability = source
            .endpoint
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("normal candidate actual pages missing"))?;
        let parent = source
            .endpoint
            .parent
            .as_ref()
            .ok_or_else(|| invalid("normal candidate actual parent missing"))?;
        if !capability
            .jobs
            .matches_bounded(self.candidate()?, memory, work, stop)?
        {
            return Err(invalid("normal candidate exact Ready stage").into());
        }
        let job = capability.jobs.normal_job(parent, memory)?;
        let (core, storage, roots, proof_ledger) = job.normal_candidate_read_sources_v1(
            self.owned.value.quorum.verified(),
            parent,
            memory,
        )?;
        if !core.uses_budget(memory)
            || !storage.uses_budget(memory)
            || !roots.uses_budget(memory)
            || core.candidate_record_id() != self.owned.value.quorum.verified().verified_record_id()
            || roots
                .digest()
                .map_err(|_| invalid("normal candidate roots"))?
                != core.body().roots
        {
            return Err(invalid("normal candidate source conjunction").into());
        }
        let _phase = memory
            .reserve(
                std::mem::size_of::<NormalReadyCandidateReadViewV1<'_>>() + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("normal candidate view admission"))?;
        consume(NormalReadyCandidateReadViewV1 {
            core,
            storage,
            roots,
            proof_ledger,
            quorum: &self.owned.value.quorum,
            parent_record: parent.verified_record_id(),
            namespace: (source.page_namespace, source.native_namespace),
            pages: &capability.pages,
        })
    }
}
