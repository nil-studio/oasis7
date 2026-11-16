//! Actual cold startup: original reconciliation, one retained execution FD,
//! full source reconstruction, then the same accepted Host constructor.
use super::super::cold_install::{
    AuthenticatedColdHostErrorV1, authenticate_and_install_cold_parent_bounded_v1,
};
use super::*;
impl FileFormalEndpoint {
    #[allow(clippy::too_many_arguments)]
    pub fn into_authenticated_cold_host_source_bounded<L>(
        mut self,
        provider: &RemoteMonotonicEndpointProvider,
        pinned: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        work: &mut super::super::IwsColdParentWorkV1,
        source_work: &mut super::super::work::IwsCandidateSourceWorkV1,
        hints: super::super::IwsColdParentHintsV1,
        mailbox_hint: L,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidAcceptedGenesisHostSourceV1, AuthenticatedColdHostErrorV1>
    where
        L: FnMut(
            &[u8; 32],
        )
            -> Result<oasis7_formal_protocol::incremental_material::FactLocationV1, String>,
    {
        // A fresh startup may need to reconcile an actual previously persisted
        // intent. That original recovery call completes and releases its FD
        // before this one execution lease is acquired. No live Host is dropped
        // and reacquired; this function consumes the endpoint by value.
        self.store.reconcile_authenticated_cold_scope_bounded_v1(
            &self.authority,
            self.role,
            provider,
            pinned,
            memory,
            archive,
            stop,
        )?;
        let lease = {
            let instance = match self.role {
                EndpointRole::Primary => &self.authority.definition().primary.instance,
                EndpointRole::Replica => &self.authority.definition().replica.instance,
            };
            let mut common = archive.bounded_context(memory, stop);
            acquire_provider_lease_bounded_v1(
                provider,
                instance,
                &self.store.checkpoint,
                pinned,
                &mut common,
            )?
        };
        authenticate_and_install_cold_parent_bounded_v1(
            &mut self,
            &lease,
            pinned,
            memory,
            archive,
            work,
            source_work,
            hints,
            mailbox_hint,
            stop,
        )?;
        // The same actual FD survives restore, physical validation and the
        // final Host constructor. Any failure drops it instead of restoring an
        // earlier live source or inventing an accepted cache.
        self.into_authenticated_cold_host_source_with_lease_bounded(
            lease, provider, pinned, memory, archive, stop,
        )
        .map_err(AuthenticatedColdHostErrorV1::from)
    }
}
