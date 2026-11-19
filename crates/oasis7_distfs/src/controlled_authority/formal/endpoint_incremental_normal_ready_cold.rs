//! Reauthenticate the original parent under the retained Ready leases. This
//! never reconstructs Ready, changes its candidate, or opens another namespace.
use super::*;
use crate::controlled_authority::formal::endpoint::incremental::cold_install::{
    AuthenticatedColdHostErrorV1, ColdParentReadRefsV1,
    recover_authenticated_cold_parent_bounded_v1,
};
use crate::controlled_authority::formal::endpoint::{
    FormalEndpointPairPublicationRefsV1, HostAcceptedParentReadViewV1, IwsCandidateSourceWorkV1,
    IwsColdParentHintsV1, IwsColdParentWorkV1,
};
use oasis7_formal_protocol::incremental_material::FactLocationV1;

impl PaidNormalReadyPairV1 {
    pub fn recover_parent_sources_bounded_v1(
        &mut self,
        work: [(&mut IwsColdParentWorkV1, &mut IwsCandidateSourceWorkV1); 2],
        hints: [IwsColdParentHintsV1; 2],
        publication: FormalEndpointPairPublicationRefsV1<'_>,
        mailbox_hint: &mut dyn FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), AuthenticatedColdHostErrorV1> {
        let FormalEndpointPairPublicationRefsV1 {
            primary_provider: _,
            replica_provider: _,
            primary_policy,
            replica_policy,
            memory,
            work: archive,
        } = publication;
        self.owned
            .value
            .quorum
            .validate_context(&mut archive.bounded_context(memory, stop))?;
        archive.charge(32, 1024, stop)?;
        if !self.uses_budget(memory) {
            return Err(invalid("Ready cold parent actual pool").into());
        }
        let [primary_work, replica_work] = work;
        let [primary_hints, replica_hints] = hints;
        let state = &mut *self.owned.value;
        let candidate = match state.original.state() {
            FormalRequestStateV1::Ready { candidate, .. } => candidate,
            _ => return Err(invalid("Ready cold parent original state").into()),
        };
        let pair = &mut *state.pair.owned.value;
        for (source, role, pinned, (cold, source_work), hints) in [
            (
                &mut pair.primary.owned.value.source,
                EndpointRole::Primary,
                primary_policy,
                primary_work,
                primary_hints,
            ),
            (
                &mut pair.replica.owned.value.source,
                EndpointRole::Replica,
                replica_policy,
                replica_work,
                replica_hints,
            ),
        ] {
            if source.identity != archive.bounded_context(memory, stop).work().identity()
                || source.endpoint.role != role
                || !source.endpoint_fee.uses_budget(memory)
            {
                return Err(invalid("Ready cold parent exact source/Core/role").into());
            }
            let lease = source
                .lease
                .as_ref()
                .ok_or_else(|| invalid("Ready cold parent original lease required"))?;
            let endpoint = &mut *source.endpoint;
            let capability = endpoint
                .iws_page_store
                .as_ref()
                .ok_or_else(|| invalid("Ready cold parent actual pages"))?;
            if !capability
                .jobs
                .matches_bounded(candidate, memory, archive, stop)?
            {
                return Err(invalid("Ready cold parent exact staged material").into());
            }
            let (cache, head) = recover_authenticated_cold_parent_bounded_v1(
                ColdParentReadRefsV1 {
                    endpoint,
                    lease,
                    pinned,
                    memory,
                    archive,
                },
                cold,
                source_work,
                hints,
                &mut *mailbox_hint,
                stop,
            )?;
            let complete = cache
                .complete_data()
                .ok_or_else(|| invalid("Ready cold parent complete data missing"))?;
            if head.verified_record_id() != source.record
                || complete.body_id() != source.fence.parent_commit
                || complete.roots_id() != source.fence.parent_roots
                || complete.page_namespace_identity() != source.page_namespace
                || complete.native_namespace_identity() != source.native_namespace
                || !lease.uses_budget(memory)
            {
                return Err(invalid("Ready cold parent changed original binding").into());
            }
            // This private installation additionally proves the exact staged
            // normal job against this Ready's actual quorum. It never takes it.
            capability
                .jobs
                .restore_ready_parent(cache, &head, &state.quorum, memory)?;
            endpoint.parent = Some(head);
        }
        Ok(())
    }

    pub fn with_primary_parent_read_view_bounded<T, E>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(HostAcceptedParentReadViewV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        self.owned
            .value
            .pair
            .with_primary_parent_read_view_bounded(memory, work, stop, consume)
    }

    pub fn with_replica_parent_read_view_bounded<T, E>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(HostAcceptedParentReadViewV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        self.owned
            .value
            .pair
            .with_replica_parent_read_view_bounded(memory, work, stop, consume)
    }
}
