//! Borrow actual accepted holders while their endpoint, namespace and FD live.
//! This read view is data; it never grants a transaction or receipt permission.
use super::*;
use oasis7_formal_protocol::incremental_material::CheckedCommitCore;
use oasis7_world_state::{
    incremental_cognition_history::CompleteCognitionHistorySourceV3,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_world_storage_source_cold::AuthenticatedWorldStorageParentSourceV1,
};

pub struct HostAcceptedParentReadViewV1<'a> {
    store: &'a Store,
    qualified_proof:
        &'a oasis7_formal_protocol::controlled_authority::formal::FormalQualifiedCommitProofV1,
    qualified_proof_core:
        oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    core: &'a ReservedValue<CheckedCommitCore>,
    storage: &'a dyn AuthenticatedWorldStorageParentSourceV1,
    roots: &'a ReservedValue<oasis7_formal_protocol::incremental_execution::RootSetV1>,
    runtime: &'a ReservedValue<AcceptedRuntimeHistorySourceV1>,
    cognition: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
    ledger: Option<
        &'a ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1,
        >,
    >,
    retention: Option<&'a ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>>,
    head: &'a VerifiedFormalCommitV1,
    authority: &'a VerifiedAuthorityV1,
    pages: &'a crate::incremental_page_store::IncrementalPageStore,
    page_namespace: (u64, u64),
    native_namespace: (u64, u64),
    fence: ExecutionFence,
    role: EndpointRole,
}
impl HostAcceptedParentReadViewV1<'_> {
    pub fn endpoint_role(&self) -> EndpointRole {
        self.role
    }
    pub fn fence(&self) -> ExecutionFence {
        self.fence
    }
    pub fn namespace_identities(&self) -> ((u64, u64), (u64, u64)) {
        (self.page_namespace, self.native_namespace)
    }
    pub fn core(&self) -> &ReservedValue<CheckedCommitCore> {
        self.core
    }
    pub fn storage(&self) -> &dyn AuthenticatedWorldStorageParentSourceV1 {
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
    /// Selected by the complete canonical routing owner in storage().results().
    /// A detached None is not a ledger-absence qualification.
    pub fn proof_ledger(
        &self,
    ) -> Option<
        &ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1,
        >,
    > {
        self.ledger
    }
    /// None is a legacy missing commitment, never a qualified empty store.
    pub fn retention_ledger(&self) -> Option<&ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>>{
        self.retention
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
        with_parent_source_bounded(
            HostParentSourceRefV1 {
                endpoint: &self.endpoint,
                lease: &self.lease,
                fence: self.fence,
                record: self.record,
                page_namespace: self.page_namespace,
                native_namespace: self.native_namespace,
            },
            memory,
            work,
            stop,
            consume,
        )
    }
}

/// Only private-mint endpoint holders can construct this borrowed binding.
/// It carries no copied balance or detached accepted qualification.
pub(super) struct HostParentSourceRefV1<'a> {
    pub(super) endpoint: &'a FileFormalEndpoint,
    pub(super) lease: &'a PaidRemoteLeaseV1,
    pub(super) fence: ExecutionFence,
    pub(super) record: [u8; 32],
    pub(super) page_namespace: (u64, u64),
    pub(super) native_namespace: (u64, u64),
}
pub(super) fn with_parent_source_bounded<T, E>(
    source: HostParentSourceRefV1<'_>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut impl FnMut() -> bool,
    consume: impl FnOnce(HostAcceptedParentReadViewV1<'_>) -> Result<T, E>,
) -> Result<T, E>
where
    E: From<FormalError>,
{
    let _phase = memory
        .reserve(
            std::mem::size_of::<HostAcceptedParentReadViewV1<'_>>()
                + std::mem::size_of::<
                    std::cell::Ref<'_, Option<provisional::AcceptedCandidateCache>>,
                >(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("Host parent view admission"))?;
    let capability = source
        .endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("Host accepted page source missing"))?;
    let slot = capability.jobs.parent()?;
    let cache = slot
        .as_ref()
        .ok_or_else(|| invalid("Host accepted parent missing"))?;
    let (core, storage): (_, &dyn AuthenticatedWorldStorageParentSourceV1) =
        match &cache.current_owner {
            provisional::AcceptedCurrentOwner::Candidate(value) => (&value.core, &value.source),
            provisional::AcceptedCurrentOwner::AuthenticatedCold(value) => {
                let mut common = work.bounded_context(memory, stop);
                value.validate_context(cache, &mut common)?;
                (value.core(), value.source())
            }
            provisional::AcceptedCurrentOwner::Cold { .. } => {
                return Err(invalid("Host authenticated complete parent source required").into());
            }
        };
    let head = source
        .endpoint
        .parent
        .as_ref()
        .ok_or_else(|| invalid("Host accepted parent missing"))?;
    // Borrow only an original genuinely verified quorum/checkpoint. A bare
    // accepted record or a caller DTO cannot substitute for full receipts.
    let qualified_proof = {
        let mut common = work.bounded_context(memory, stop);
        common.charge(1, 32)?;
        match (
            &cache.genesis_quorum,
            &cache.normal_quorum,
            &cache.current_owner,
        ) {
            (Some(quorum), None, provisional::AcceptedCurrentOwner::Candidate(_)) => {
                quorum.validate_context(&mut common)?;
                if quorum.verified().verified_record_id() != head.verified_record_id() {
                    return Err(invalid("Host GEN quorum actual head mismatch").into());
                }
                quorum.proof()
            }
            (None, Some(quorum), provisional::AcceptedCurrentOwner::Candidate(_)) => {
                quorum.validate_context(&mut common)?;
                if quorum.verified().verified_record_id() != head.verified_record_id() {
                    return Err(invalid("Host Normal quorum actual head mismatch").into());
                }
                quorum.proof()
            }
            (None, None, provisional::AcceptedCurrentOwner::AuthenticatedCold(actual)) => {
                let checkpoint = actual.checkpoint();
                checkpoint.validate_context(&mut common)?;
                if checkpoint.head().verified_record_id() != head.verified_record_id() {
                    return Err(invalid("Host cold quorum actual head mismatch").into());
                }
                checkpoint.native().proof()
            }
            _ => {
                return Err(invalid("Host actual verified Qualified proof source required").into());
            }
        }
    };
    let data = cache
        .complete_data()
        .ok_or_else(|| invalid("Host parent complete data missing"))?;
    work.charge(1, 32 * 10, stop)?;
    if !core.uses_budget(memory)
        || !storage.uses_budget(memory)
        || !cache.roots.uses_budget(memory)
        || !cache.runtime.uses_budget(memory)
        || !cache.cognition.uses_budget(memory)
        || !data.uses_budget(memory)
        || data.page_namespace_identity() != source.page_namespace
        || data.native_namespace_identity() != source.native_namespace
        || data.body_id() != source.fence.parent_commit
        || data.roots_id() != source.fence.parent_roots
        || cache.commit_record != source.record
        || head.verified_record_id() != source.record
        || core.candidate_record_id() != source.record
        || !source.lease.uses_budget(memory)
    {
        return Err(invalid("Host accepted parent binding changed").into());
    }
    if storage.results().proof_ledger().is_some() != cache.ledger.is_some() {
        return Err(invalid("Host complete ProofLedger role binding").into());
    }
    if let Some(ledger) = cache.ledger.as_ref() {
        let occurrence = ledger.occurrence();
        if !ledger.uses_budget(memory)
            || occurrence.head_record_id() != source.record
            || occurrence.head_body_id() != source.fence.parent_commit
            || occurrence.fact_body_id() != source.fence.parent_commit
            || occurrence.roots_id() != source.fence.parent_roots
            || storage
                .results()
                .proof_ledger()
                .is_none_or(|row| row.occurrence().location() != occurrence.location())
        {
            return Err(invalid("Host selected ProofLedger source binding").into());
        }
    }
    if core.retention_ledger_artifact_id().is_some() != cache.retention.is_some() {
        return Err(invalid("Host signed retention role binding").into());
    }
    if let Some(retention) = cache.retention.as_ref() {
        retention
            .validate_binding_v1(head, &cache.roots, &mut work.bounded_context(memory, stop))
            .map_err(|e| invalid(format!("Host accepted retention source: {e:?}")))?;
    }
    consume(HostAcceptedParentReadViewV1 {
        store: &source.endpoint.store,
        qualified_proof,
        qualified_proof_core: work.bounded_context(memory, stop).work().identity(),
        core,
        storage,
        roots: &cache.roots,
        runtime: &cache.runtime,
        cognition: &cache.cognition,
        ledger: cache.ledger.as_ref(),
        retention: cache.retention.as_ref(),
        head,
        authority: &source.endpoint.authority,
        pages: &capability.pages,
        page_namespace: source.page_namespace,
        native_namespace: source.native_namespace,
        fence: source.fence,
        role: source.endpoint.role,
    })
}

#[path = "endpoint_incremental_host_parent_proof.rs"]
mod qualified_proof;
pub use qualified_proof::PaidHostQualifiedParentProofV1;
