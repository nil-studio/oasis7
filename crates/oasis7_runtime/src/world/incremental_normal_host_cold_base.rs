//! Same accepted-parent transaction bootstrap, before the projection cache.
//! Keeping the caller's pristine transaction breaks the cache/source cycle
//! without a detached guard, replacement budget or a manufactured acceptance.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_authenticated_cold_normal_host_base_v1<M, R, W, O, E, C>(
    parent: &super::super::incremental_normal_sources::BoundNormalTickParentV1<'_>,
    transaction: &mut ExecutionTransaction<'_, M>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    originals: &mut O,
    projection: ProvisionalAuthorizationProjectionV1,
    process: &ReservedValue<VerifiedNormalColdProcessConfigurationV1>,
    configuration: &ReservedValue<NormalHostColdRuntimeConfigurationV1>,
    io: &mut NormalHostIoWorkV1,
    startup: &mut NormalHostStartupWorkV1<'_>,
    take_state_error: E,
    cancelled: C,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<
    (
        super::super::incremental_normal_host_resident::PendingNormalHostResidentSourcesV1,
        u64,
    ),
    BodyMutationError,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    O: OriginalObjectStreamReader,
    O::Error: std::error::Error + Send + Sync + 'static,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
{
    configuration.validate_context(common).map_err(formal)?;
    let retention = super::super::incremental_world_event_counter::cold_configuration_retention_v1(
        configuration,
        common,
    )
    .map_err(formal)?;
    prepare_authenticated_normal_host_base_v1(
        parent,
        transaction,
        state,
        values,
        member_sink,
        originals,
        projection,
        NormalProcessStartupSourceV1::Cold(process),
        retention,
        io,
        startup,
        take_state_error,
        cancelled,
        common,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_authenticated_normal_host_base_v1<M, R, W, O, E, C>(
    parent: &super::super::incremental_normal_sources::BoundNormalTickParentV1<'_>,
    transaction: &mut ExecutionTransaction<'_, M>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    originals: &mut O,
    projection: ProvisionalAuthorizationProjectionV1,
    process: NormalProcessStartupSourceV1<'_>,
    retention: ReservedValue<
        super::super::incremental_world_event_counter::WorldJournalRetentionV1,
    >,
    io: &mut NormalHostIoWorkV1,
    startup: &mut NormalHostStartupWorkV1<'_>,
    take_state_error: E,
    cancelled: C,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<
    (
        super::super::incremental_normal_host_resident::PendingNormalHostResidentSourcesV1,
        u64,
    ),
    BodyMutationError,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    O: OriginalObjectStreamReader,
    O::Error: std::error::Error + Send + Sync + 'static,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
{
    parent.validate_context(common).map_err(formal)?;
    let memory = common.memory().clone();
    let fence = parent.fence();
    transaction.validate_bootstrap_read_only_v1(&fence)?;
    if transaction.parent_fence() != fence || !transaction.uses_budget(&memory) {
        return Err(BodyMutationError::Invalid(
            "bootstrap resident base exact parent/TX/pool",
        ));
    }
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction,
        values: ValueReadContext {
            source: values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io.overlay.io,
        },
        key_work: &mut io.overlay.keys,
        semantic_bytes: io.overlay.semantic_bytes,
        record_bytes: io.overlay.record_bytes,
        take_state_error,
        cancelled,
    };
    let mut read = io.current.field(
        parent
            .refs()
            .roots
            .state_root(StateNamespaceV1::Authorization),
        state,
        values,
        &memory,
        take_state_error,
        cancelled,
    );
    let mut members = io.members.members(member_sink, &memory, cancelled);
    // Fixed before==after begin performs its actual Current checks and creates
    // no writes. The exact original authority reader child meters both read
    // contexts and all typed/scan lanes; no nested Common child is active.
    let (revocation, clock) = with_authorization_source_read_work_v1(
        parent.refs().roots,
        &mut overlay,
        &mut read,
        startup.authorization,
        common,
        |overlay, read, work| {
            let guard = overlay.transaction.guard();
            let revocation = begin_runtime_revocation_current_v1(
                parent.refs().storage.current().runtime().revocation(),
                overlay,
                read,
                &mut work.scan,
                &mut work.typed,
                &mut members,
                &guard,
                &fence,
            )
            .map_err(crate::world::incremental_authorization_root_current::SourceError::from)?;
            let clock = oasis7_world_state::incremental_world_u64::read_world_u64_overlay_v1(
                oasis7_world_state::incremental_world_u64::WorldU64FieldV1::Time,
                overlay,
            )
            .map_err(crate::world::incremental_authorization_root_current::SourceError::from)?;
            Ok((revocation, clock))
        },
    )
    .map_err(original)?;
    super::super::incremental_normal_host_resident::prepare_normal_host_resident_base_v1(
        parent,
        projection,
        process,
        retention,
        &revocation,
        &mut overlay,
        &mut read,
        originals,
        startup,
        common,
        &fence,
    )
    .map(|base| (base, clock))
}
