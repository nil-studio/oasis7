//! All fallible resident-cache refinement happens before durable publication.
//! Finish only compares the actual verified record and moves prepaid owners.
use super::*;
use crate::world::{
    incremental_commit::{PreparedRuntimeDirectoryCacheV1, RuntimeDirectoryCacheErrorV1},
    incremental_normal_host_driver::PreparedNormalHostRuntimeV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{PaidVerifiedNormalCommitV1, VerifiedFormalCommitV1},
    incremental_material::CheckedCommitCore,
};
use oasis7_world_state::incremental_runtime_history_source::CheckedRuntimeHistorySourceV1;
pub struct PreparedNormalHostResidentV1 {
    projection_cache: ReservedValue<
        crate::world::incremental_normal_projection_commitment::AcceptedNormalProjectionCacheV1,
    >,
    business: ReservedValue<
        crate::world::incremental_normal_business_commitment::NormalBusinessMetadataParentSourceV1,
    >,
    process: std::sync::Arc<
        ReservedValue<crate::world::incremental_normal_process_source::NormalProcessSourceV1>,
    >,
    directories: PreparedRuntimeDirectoryCacheV1,
    manifest: ReservedValue<AcceptedWorldManifestHashV1>,
    delivery: ReservedValue<ProductValidationDeliveryIndexV1>,
    retention: ReservedValue<WorldJournalRetentionV1>,
    authorization: AcceptedAuthorizationProjectionV1,
    capture: std::sync::Arc<
        ReservedValue<crate::world::incremental_normal_final_sources::CapturedNormalFinalSourcesV1>,
    >,
    record: [u8; 32],
    roots: [u8; 32],
    core: WorkIdentityV1,
    fee: ReservedValue<()>,
    outer: oasis7_proto::memory_budget::MemoryReservation,
}
#[allow(clippy::too_many_arguments)]
pub fn prepare_normal_host_resident_promotion_v1<M, R, E, C>(
    prepared: PreparedNormalHostRuntimeV1,
    previous: &NormalHostResidentSourcesV1,
    core: &ReservedValue<CheckedCommitCore>,
    history: &ReservedValue<CheckedRuntimeHistorySourceV1>,
    quorum: &PaidVerifiedNormalCommitV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ProductValidationDeliveryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PreparedNormalHostResidentV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    quorum.validate_context(common).map_err(formal)?;
    common.charge(32, 8192).map_err(formal)?;
    let prepared = prepared.into_inner();
    let (candidate, tick, mut inherited, terminal) = prepared.into_parts();
    let after = &candidate.transition().record.after;
    if previous.core != common.work().identity()
        || !previous.fee.uses_budget(common.memory())
        || candidate.commit_id() != core.catalog().entry.commit
        || core.candidate_record_id() != quorum.verified().verified_record_id()
        || previous.record != tick.tick.parent_record_for_resident_v1()
    {
        return Err(bad());
    }
    let capture = std::sync::Arc::clone(&tick.final_sources);
    let manifest = capture
        .manifest_capture()
        .prepare_normal_candidate_source_v1(
            &terminal,
            core,
            history,
            quorum.verified(),
            after,
            common,
        )?;
    let delivery = capture
        .delivery_capture()
        .prepare_checked_candidate_v1(
            &terminal,
            core,
            history,
            quorum.verified(),
            after,
            read,
            work,
            common,
        )
        .map_err(original)?;
    let authorization = capture
        .authorization_capture()
        .prepare_checked_candidate_v1(&terminal, core, quorum.verified(), history, after, common)
        .map_err(original)?;
    let business = capture
        .terminal_metadata_capture()
        .prepare_normal_candidate_source_v1(
            &terminal,
            core,
            history,
            quorum.verified(),
            after,
            common,
        )?;
    let projection_cache = capture
        .projection_capture()
        .prepare_normal_candidate_source_v1(
            &terminal,
            core,
            history,
            quorum.verified(),
            after,
            common,
        )?;
    let retention = previous.retention.retain_normal_successor_v1(common)?;
    let roots = terminal.after_roots();
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<NormalHostResidentSourcesV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?
        .retain(());
    let mut outer = common
        .memory()
        .reserve(
            std::mem::size_of::<PreparedNormalHostResidentV1>()
                .max(std::mem::size_of::<NormalHostResidentSourcesV1>()),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    outer
        .merge(&mut inherited)
        .map_err(BodyMutationError::Invalid)?;
    let directories = candidate
        .prepare_directory_cache_bounded_v1(core, common)
        .map_err(original)?;
    // All temporary execution evidence remains alive through the fallible source
    // conjunction above. The captured frontier/Q/graph owners remain in capture.
    drop(tick);
    Ok(PreparedNormalHostResidentV1 {
        projection_cache,
        business,
        process: std::sync::Arc::clone(&previous.process),
        directories,
        manifest,
        delivery,
        retention,
        authorization,
        capture,
        record: quorum.verified().verified_record_id(),
        roots,
        core: common.work().identity(),
        fee,
        outer,
    })
}
impl PreparedNormalHostResidentV1 {
    pub fn finish(
        self,
        accepted: &VerifiedFormalCommitV1,
    ) -> Result<ReservedValue<NormalHostResidentSourcesV1>, ExecutionError> {
        if accepted.verified_record_id() != self.record {
            return Err(ExecutionError::Invalid(
                "normal resident durable accepted record",
            ));
        }
        let directories = self.directories.finish(accepted)?;
        Ok(self.outer.retain(NormalHostResidentSourcesV1 {
            projection_cache: self.projection_cache,
            business: self.business,
            process: self.process,
            manifest: self.manifest,
            delivery: self.delivery,
            retention: self.retention,
            storage: ResidentStorage::Warm {
                directories,
                capture: self.capture,
            },
            authorization: self.authorization,
            record: self.record,
            roots: self.roots,
            core: self.core,
            fee: self.fee,
        }))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_normal_host_resident_promotion_from_io_v1<M, R, E, C>(
    prepared: PreparedNormalHostRuntimeV1,
    previous: &NormalHostResidentSourcesV1,
    core: &ReservedValue<CheckedCommitCore>,
    history: &ReservedValue<CheckedRuntimeHistorySourceV1>,
    quorum: &PaidVerifiedNormalCommitV1,
    roots: &ReservedValue<oasis7_formal_protocol::incremental_execution::RootSetV1>,
    state: &M,
    values: &R,
    work: &mut crate::world::incremental_normal_host_work::NormalHostWorkV1,
    take_state_error: E,
    cancelled: C,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PreparedNormalHostResidentV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    common.charge(16, 8192).map_err(formal)?;
    if !roots.uses_budget(common.memory()) || roots.digest()? != core.body().roots {
        return Err(bad());
    }
    let memory = common.memory().clone();
    let mut read = work.io.parent.field(
        roots.state_root(StateNamespaceV1::Artifacts),
        state,
        values,
        &memory,
        take_state_error,
        cancelled,
    );
    prepare_normal_host_resident_promotion_v1(
        prepared,
        previous,
        core,
        history,
        quorum,
        &mut read,
        &mut work.driver.final_sources.originals,
        common,
    )
}

impl PreparedNormalHostResidentV1 {
    /// Second endpoint refinement is based on its independently checked full
    /// candidate and the actual complete quorum, not a clone permission.
    pub fn prepare_secondary_candidate_v1(
        &self,
        previous: &NormalHostResidentSourcesV1,
        core: &ReservedValue<CheckedCommitCore>,
        history: &ReservedValue<CheckedRuntimeHistorySourceV1>,
        quorum: &PaidVerifiedNormalCommitV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, BodyMutationError> {
        quorum.validate_context(common).map_err(formal)?;
        common.charge(32, 8192).map_err(formal)?;
        if !core.uses_budget(common.memory())
            || !history.uses_budget(common.memory())
            || !self.fee.uses_budget(common.memory())
            || self.core != common.work().identity()
            || core.candidate_record_id() != self.record
            || quorum.verified().verified_record_id() != self.record
            || core.body().roots != self.roots
            || history.manifest().roots != self.roots
            || history.occurrence().candidate_record_id() != self.record
            || history.occurrence().body_id() != core.catalog().entry.commit
        {
            return Err(bad());
        }
        self.process
            .same_configuration_v1(&previous.process, common)?;
        let directories = self
            .directories
            .share_normal_candidate_v1(core, common)
            .map_err(original)?;
        let business = self.business.share_normal_ordinary_v1(common)?;
        let projection_cache = self.projection_cache.share_ordinary_v1(common)?;
        let manifest = self.manifest.share_normal_ordinary_v1(common)?;
        let delivery = self
            .delivery
            .share_normal_ordinary_v1(common)
            .map_err(original)?;
        let authorization = self
            .authorization
            .share_normal_ordinary_v1(common)
            .map_err(original)?;
        let retention = self.retention.retain_normal_successor_v1(common)?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<NormalHostResidentSourcesV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let outer = common
            .memory()
            .reserve(
                std::mem::size_of::<Self>().max(std::mem::size_of::<NormalHostResidentSourcesV1>()),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            projection_cache,
            business,
            process: std::sync::Arc::clone(&previous.process),
            directories,
            manifest,
            delivery,
            authorization,
            retention,
            capture: std::sync::Arc::clone(&self.capture),
            record: self.record,
            roots: self.roots,
            core: self.core,
            fee,
            outer,
        })
    }
}
