//! Provisional physical composition of the current Runtime GEN factory.
//! Acceptance, complete originals/history and active-head publication stay with
//! the caller; this function does not select a head or mint WorldReady.
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageIoBudget, genesis::GenesisIndexWorkV1,
};
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::RootSetV1,
    incremental_history::GenesisConfigV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use oasis7_runtime::{
    BodyMutationError, World,
    incremental_genesis::{RuntimeGenesisErrorV1, RuntimeGenesisSummaryV1, RuntimeGenesisWorkV1},
};
use oasis7_world_state::{
    ActionEnvelope, DomainEvent,
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_semantic_key::SemanticKeyBudget,
};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct RuntimeGenesisPhysicalBudgetsV1 {
    pub producer_pages: PageIoBudget,
    pub index_reads: PageIoBudget,
    pub index_writes: PageIoBudget,
    pub index: GenesisIndexWorkV1,
}
pub(crate) struct RuntimeGenesisPhysicalContextV1<'a> {
    pub memory: &'a MemoryBudget,
    pub values: &'a mut ValueIoBudget,
    pub keys: &'a mut SemanticKeyBudget,
    pub limits: MemberRecordLimitsV1,
    pub cancelled: &'a AtomicBool,
}
pub(crate) struct ProvisionalRuntimeGenesisIndexV1 {
    pub roots: ReservedValue<RootSetV1>,
    pub summary: RuntimeGenesisSummaryV1,
}
#[derive(Debug)]
pub(crate) enum RuntimeGenesisPhysicalErrorV1 {
    Value(ValueTreeError),
    Runtime(RuntimeGenesisErrorV1),
    Business(oasis7::world_service::formal_result::PaidFormalBusinessErrorV1),
    Projected(oasis7_runtime::incremental_genesis_projection::RuntimeProjectedGenesisErrorV1),
}
impl From<ValueTreeError> for RuntimeGenesisPhysicalErrorV1 {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
impl From<RuntimeGenesisErrorV1> for RuntimeGenesisPhysicalErrorV1 {
    fn from(error: RuntimeGenesisErrorV1) -> Self {
        Self::Runtime(error)
    }
}
impl std::fmt::Display for RuntimeGenesisPhysicalErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(error) => error.fmt(f),
            Self::Runtime(error) => error.fmt(f),
            Self::Business(error) => error.fmt(f),
            Self::Projected(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for RuntimeGenesisPhysicalErrorV1 {}

/// One actual PageStore and cancellation signal back both producer nodes and
/// the bounded six-namespace index builder. Physical sessions consume the
/// caller's supplied allowances; logical and typed work are never reset.
pub(crate) fn prepare_runtime_genesis_index_v1<
    P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>,
    A: FnMut(u64, &ActionEnvelope) -> Result<OriginalArtifactRefV1, ValueTreeError>,
    L: FnMut(&str, u64, &DomainEvent) -> Result<[u8; 32], BodyMutationError>,
>(
    world: &World,
    store: &IncrementalPageStore,
    config: &GenesisConfigV1,
    context: &mut RuntimeGenesisPhysicalContextV1<'_>,
    work: &mut RuntimeGenesisWorkV1,
    physical: RuntimeGenesisPhysicalBudgetsV1,
    persist_artifact: &mut P,
    persist_action: &mut A,
    mailbox_original: &mut L,
) -> Result<ProvisionalRuntimeGenesisIndexV1, RuntimeGenesisPhysicalErrorV1> {
    let RuntimeGenesisPhysicalBudgetsV1 {
        producer_pages,
        index_reads,
        index_writes,
        index,
    } = physical;
    let mut pages = store.value_session_with_cancelled(producer_pages, context.cancelled);
    prepare_runtime_genesis_index_with_writer_v1(
        world,
        store,
        config,
        context,
        work,
        RuntimeGenesisIndexBudgetsV1 {
            index_reads,
            index_writes,
            index,
        },
        &mut pages,
        persist_artifact,
        persist_action,
        mailbox_original,
    )
}
/// The same actual GEN kernel with a caller-owned charged page writer. This
/// allows the Host to stream every nested record/value page to both replicas.
pub(crate) struct RuntimeGenesisIndexBudgetsV1 {
    pub index_reads: PageIoBudget,
    pub index_writes: PageIoBudget,
    pub index: GenesisIndexWorkV1,
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_runtime_genesis_index_with_writer_v1<
    W: StatePageWriter
        + ValueNodeReader
        + oasis7_formal_protocol::incremental_schedule::SchedulePageWriter,
    P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>,
    A: FnMut(u64, &ActionEnvelope) -> Result<OriginalArtifactRefV1, ValueTreeError>,
    L: FnMut(&str, u64, &DomainEvent) -> Result<[u8; 32], BodyMutationError>,
>(
    world: &World,
    store: &IncrementalPageStore,
    config: &GenesisConfigV1,
    context: &mut RuntimeGenesisPhysicalContextV1<'_>,
    work: &mut RuntimeGenesisWorkV1,
    physical: RuntimeGenesisIndexBudgetsV1,
    pages: &mut W,
    persist_artifact: &mut P,
    persist_action: &mut A,
    mailbox_original: &mut L,
) -> Result<ProvisionalRuntimeGenesisIndexV1, RuntimeGenesisPhysicalErrorV1> {
    if context.cancelled.load(Ordering::Acquire) {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !context.memory.same_pool(store.memory_budget()) {
        return Err(ValueTreeError::Invalid("runtime GEN physical context pool").into());
    }
    let cancelled = context.cancelled;
    let (roots, summary) = with_genesis_index_v1(store, config, physical, cancelled, |sink| {
        Ok(world.prepare_iws_genesis_v1(
            &mut MemberRecordWriteContext {
                sink: &mut *pages,
                memory: context.memory,
                io: &mut *context.values,
                key_work: &mut *context.keys,
                limits: context.limits,
                cancelled: || cancelled.load(Ordering::Acquire),
            },
            work,
            persist_artifact,
            persist_action,
            mailbox_original,
            &mut |row| sink(row),
        )?)
    })?;
    Ok(ProvisionalRuntimeGenesisIndexV1 { roots, summary })
}

/// The fixed physical producer retains the columns and original business
/// metadata from the very same immutable World that emitted these index rows.
/// Neither field is caller-mintable or accepted-head authority.
pub(crate) struct ProvisionalProjectedRuntimeGenesisIndexV1 {
    outer: oasis7_proto::memory_budget::MemoryReservation,
    roots: ReservedValue<RootSetV1>,
    projected: oasis7_runtime::incremental_genesis_projection::RuntimeProjectedGenesisV1,
    retention:
        ReservedValue<oasis7_formal_protocol::incremental_retention::RetentionLedgerManifestV1>,
    journal_originals: ReservedValue<Vec<OriginalArtifactRefV1>>,
    snapshot_originals: ReservedValue<Vec<OriginalArtifactRefV1>>,
    hot_tick_originals: ReservedValue<Vec<OriginalArtifactRefV1>>,
    hot_tick_archived_count: u64,
}
impl ProvisionalProjectedRuntimeGenesisIndexV1 {
    /// Initialize the original business index from this very physical GEN
    /// output. Existing nonempty legacy results still require real migration.
    pub(crate) fn prepare_business_output_bounded_v1<'a>(
        &'a self,
        nodes: &impl oasis7_formal_protocol::controlled_authority::formal::business_index::PaidBusinessIndexNodeReaderV1,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<
        oasis7::world_service::formal_result::PaidGenesisFormalBusinessResultUpdateV1<'a>,
        RuntimeGenesisPhysicalErrorV1,
    > {
        let source = self.business_source_bounded_v1(common).map_err(|error|
            RuntimeGenesisPhysicalErrorV1::Projected(
                oasis7_runtime::incremental_genesis_projection::RuntimeProjectedGenesisErrorV1::Source(error)))?;
        oasis7::world_service::formal_result::build_genesis_formal_business_result_bounded_v1(
            source, nodes, common, canonical,
        )
        .map_err(RuntimeGenesisPhysicalErrorV1::Business)
    }

    /// Fixed consumer borrows only the roots returned by this producer's
    /// successful index finish, never a caller's replacement root set.
    pub(crate) fn business_source_bounded_v1<'a>(
        &'a self,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<oasis7_runtime::PaidProjectedGenesisBusinessSourceV1<'a>, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.outer.uses_budget(common.memory()) || !self.roots.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "physical projected GEN owner pool",
            ));
        }
        self.projected
            .business_source_bounded_v1(&self.roots, common)
    }

    /// Same immutable World and actual finished physical roots as this owner.
    /// Borrowed DATA; the signed role and formal acceptance remain mandatory.
    pub(crate) fn retention_manifest(
        &self,
    ) -> &ReservedValue<oasis7_formal_protocol::incremental_retention::RetentionLedgerManifestV1>
    {
        &self.retention
    }
    pub(crate) fn roots(&self) -> &RootSetV1 {
        &self.roots
    }
    pub(crate) fn projected(
        &self,
    ) -> &oasis7_runtime::incremental_genesis_projection::RuntimeProjectedGenesisV1 {
        &self.projected
    }

    /// Actual originals in complete Journal order from the same immutable GEN
    /// borrow. This is paid provisional DATA, not History location or acceptance.
    pub(crate) fn journal_originals_bounded_v1(
        &self,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<&ReservedValue<Vec<OriginalArtifactRefV1>>, BodyMutationError> {
        self.projected.validate_context_bounded_v1(common)?;
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.outer.uses_budget(common.memory())
            || !self.roots.uses_budget(common.memory())
            || !self.journal_originals.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "physical GEN journal owner pool",
            ));
        }
        Ok(&self.journal_originals)
    }

    /// Same actual catalog order as the immutable World borrowed by this GEN.
    /// Original references carry no History index ordinal or accepted source.
    pub(crate) fn snapshot_originals_bounded_v1(
        &self,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<&ReservedValue<Vec<OriginalArtifactRefV1>>, BodyMutationError> {
        self.projected.validate_context_bounded_v1(common)?;
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.outer.uses_budget(common.memory())
            || !self.roots.uses_budget(common.memory())
            || !self.snapshot_originals.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "physical GEN snapshot owner pool",
            ));
        }
        Ok(&self.snapshot_originals)
    }

    /// Same-World legacy hot suffix and actual archived count, both DATA only.
    /// The count proves no archived original and carries no IWS Tick authority.
    pub(crate) fn hot_tick_originals_bounded_v1(
        &self,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<(&ReservedValue<Vec<OriginalArtifactRefV1>>, u64), BodyMutationError> {
        self.projected.validate_context_bounded_v1(common)?;
        common
            .charge(1, std::mem::size_of::<u64>() as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.outer.uses_budget(common.memory())
            || !self.roots.uses_budget(common.memory())
            || !self.hot_tick_originals.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "physical GEN hot Tick owner pool",
            ));
        }
        Ok((&self.hot_tick_originals, self.hot_tick_archived_count))
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_projected_runtime_genesis_index_with_writer_v1<
    W: StatePageWriter
        + ValueNodeReader
        + oasis7_formal_protocol::incremental_schedule::SchedulePageWriter,
    P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>,
    A: FnMut(u64, &ActionEnvelope) -> Result<OriginalArtifactRefV1, ValueTreeError>,
    L: FnMut(&str, u64, &DomainEvent) -> Result<[u8; 32], BodyMutationError>,
>(
    world: &World,
    store: &IncrementalPageStore,
    config: &GenesisConfigV1,
    context: &mut RuntimeGenesisPhysicalContextV1<'_>,
    work: &mut RuntimeGenesisWorkV1,
    physical: RuntimeGenesisIndexBudgetsV1,
    pages: &mut W,
    persist_artifact: &mut P,
    persist_action: &mut A,
    mailbox_original: &mut L,
    retention_work: &mut oasis7_world_state::incremental_cognition_retention_ledger::CognitionRetentionWorkV1<'_>,
    journal_original: &mut dyn FnMut(
        &oasis7_world_state::WorldEvent,
        &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<OriginalArtifactRefV1, BodyMutationError>,
    snapshot_original: &mut dyn FnMut(
        &oasis7_world_state::snapshot::SnapshotRecord,
        &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<OriginalArtifactRefV1, BodyMutationError>,
    hot_tick_original: &mut dyn FnMut(
        &oasis7_world_state::consensus::TickConsensusRecord,
        &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<OriginalArtifactRefV1, BodyMutationError>,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<ProvisionalProjectedRuntimeGenesisIndexV1, RuntimeGenesisPhysicalErrorV1> {
    if context.cancelled.load(Ordering::Acquire) {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !context.memory.same_pool(store.memory_budget())
        || !context.memory.same_pool(common.memory())
    {
        return Err(ValueTreeError::Invalid("projected GEN physical context pool").into());
    }
    let outer_bytes = std::mem::size_of::<ProvisionalProjectedRuntimeGenesisIndexV1>();
    common.charge(1, outer_bytes as u64).map_err(|e| {
        RuntimeGenesisPhysicalErrorV1::Projected(
            oasis7_runtime::incremental_genesis_projection::RuntimeProjectedGenesisErrorV1::Source(
                BodyMutationError::Original(Box::new(e)),
            ),
        )
    })?;
    let outer = common
        .memory()
        .reserve(
            outer_bytes,
            oasis7_proto::memory_budget::MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let cancelled = context.cancelled;
    let (roots, projected) = with_genesis_index_v1(store, config, physical, cancelled, |sink| {
        world
            .prepare_iws_genesis_with_projection_v1(
                &mut MemberRecordWriteContext {
                    sink: &mut *pages,
                    memory: context.memory,
                    io: &mut *context.values,
                    key_work: &mut *context.keys,
                    limits: context.limits,
                    cancelled: || cancelled.load(Ordering::Acquire),
                },
                work,
                persist_artifact,
                persist_action,
                mailbox_original,
                &mut |row| sink(row),
                common,
            )
            .map_err(RuntimeGenesisPhysicalErrorV1::Projected)
    })?;
    let retention = world.prepare_iws_cognition_retention_ledger_genesis_v1(
        &roots,
        &mut MemberRecordWriteContext {
            sink: &mut *pages,
            memory: context.memory,
            io: &mut *context.values,
            key_work: &mut *context.keys,
            limits: context.limits,
            cancelled: || cancelled.load(Ordering::Acquire),
        },
        retention_work,
        common,
    )?;
    let source_error = |error| {
        RuntimeGenesisPhysicalErrorV1::Projected(
            oasis7_runtime::incremental_genesis_projection::RuntimeProjectedGenesisErrorV1::Source(
                error,
            ),
        )
    };
    let (_, count) = projected
        .formal_event_commitment_bounded_v1(common)
        .map_err(source_error)?;
    if count != world.journal().events.len() {
        return Err(source_error(BodyMutationError::Invalid(
            "GEN journal capture count",
        )));
    }
    let bytes = count
        .checked_mul(std::mem::size_of::<OriginalArtifactRefV1>())
        .ok_or_else(|| source_error(BodyMutationError::Invalid("GEN journal refs length")))?;
    let byte_work = u64::try_from(bytes)
        .map_err(|_| source_error(BodyMutationError::Invalid("GEN journal refs width")))?;
    common
        .charge(1, byte_work)
        .map_err(|error| source_error(BodyMutationError::Original(Box::new(error))))?;
    // The reservation precedes the raw Vec, so every Err/unwind drops the
    // allocation before its fee. The outer reservation already covers metadata.
    let refs_fee = common
        .memory()
        .reserve(bytes, oasis7_proto::memory_budget::MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut refs = Vec::new();
    refs.try_reserve_exact(count)
        .map_err(|_| source_error(BodyMutationError::Invalid("GEN journal refs allocation")))?;
    if refs.capacity() != count {
        return Err(source_error(BodyMutationError::Invalid(
            "GEN journal refs capacity",
        )));
    }
    for event in &world.journal().events {
        common
            .charge(1, std::mem::size_of::<OriginalArtifactRefV1>() as u64)
            .map_err(|error| source_error(BodyMutationError::Original(Box::new(error))))?;
        refs.push(journal_original(event, common).map_err(source_error)?);
    }
    let snapshots = &world.snapshot_catalog().records;
    let snapshot_bytes = snapshots
        .len()
        .checked_mul(std::mem::size_of::<OriginalArtifactRefV1>())
        .ok_or_else(|| source_error(BodyMutationError::Invalid("GEN snapshot refs length")))?;
    let snapshot_work = u64::try_from(snapshot_bytes)
        .map_err(|_| source_error(BodyMutationError::Invalid("GEN snapshot refs width")))?;
    common
        .charge(1, snapshot_work)
        .map_err(|error| source_error(BodyMutationError::Original(Box::new(error))))?;
    // The complete actual catalog is borrowed, never captured or copied again.
    // Its independent buffer fee precedes allocation and survives with the refs.
    let snapshot_fee = common
        .memory()
        .reserve(
            snapshot_bytes,
            oasis7_proto::memory_budget::MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut snapshot_refs = Vec::new();
    snapshot_refs
        .try_reserve_exact(snapshots.len())
        .map_err(|_| source_error(BodyMutationError::Invalid("GEN snapshot refs allocation")))?;
    if snapshot_refs.capacity() != snapshots.len() {
        return Err(source_error(BodyMutationError::Invalid(
            "GEN snapshot refs capacity",
        )));
    }
    for snapshot in snapshots {
        common
            .charge(1, std::mem::size_of::<OriginalArtifactRefV1>() as u64)
            .map_err(|error| source_error(BodyMutationError::Original(Box::new(error))))?;
        snapshot_refs.push(snapshot_original(snapshot, common).map_err(source_error)?);
    }
    // This new borrowed view has its own admission before construction. Its
    // phase remains live through the hot-suffix loop; it is not child metadata.
    let history_view_bytes = std::mem::size_of::<
        oasis7_runtime::incremental_runtime_history_original::RuntimeHistoryBorrowedV1<'_>,
    >();
    common
        .charge(1, history_view_bytes as u64)
        .map_err(|error| source_error(BodyMutationError::Original(Box::new(error))))?;
    let _history_view_fee = common
        .memory()
        .reserve(
            history_view_bytes,
            oasis7_proto::memory_budget::MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let history = world.borrow_iws_runtime_history_v1();
    let hot_tick_bytes = history
        .ticks
        .len()
        .checked_mul(std::mem::size_of::<OriginalArtifactRefV1>())
        .ok_or_else(|| source_error(BodyMutationError::Invalid("GEN hot Tick refs length")))?;
    let hot_tick_work = u64::try_from(hot_tick_bytes)
        .map_err(|_| source_error(BodyMutationError::Invalid("GEN hot Tick refs width")))?;
    common
        .charge(1, hot_tick_work)
        .map_err(|error| source_error(BodyMutationError::Original(Box::new(error))))?;
    let hot_tick_fee = common
        .memory()
        .reserve(
            hot_tick_bytes,
            oasis7_proto::memory_budget::MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut hot_tick_refs = Vec::new();
    hot_tick_refs
        .try_reserve_exact(history.ticks.len())
        .map_err(|_| source_error(BodyMutationError::Invalid("GEN hot Tick refs allocation")))?;
    if hot_tick_refs.capacity() != history.ticks.len() {
        return Err(source_error(BodyMutationError::Invalid(
            "GEN hot Tick refs capacity",
        )));
    }
    for tick in history.ticks {
        common
            .charge(1, std::mem::size_of::<OriginalArtifactRefV1>() as u64)
            .map_err(|error| source_error(BodyMutationError::Original(Box::new(error))))?;
        hot_tick_refs.push(hot_tick_original(tick, common).map_err(source_error)?);
    }
    // Preserve the actual boundary; it is not a locator or archived-original proof.
    common
        .charge(1, std::mem::size_of::<u64>() as u64)
        .map_err(|error| source_error(BodyMutationError::Original(Box::new(error))))?;
    let hot_tick_archived_count = history.archived_tick_count;
    projected
        .validate_context_bounded_v1(common)
        .map_err(source_error)?;
    if cancelled.load(Ordering::Acquire) {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(ProvisionalProjectedRuntimeGenesisIndexV1 {
        outer,
        roots,
        projected,
        retention,
        journal_originals: refs_fee.retain(refs),
        snapshot_originals: snapshot_fee.retain(snapshot_refs),
        hot_tick_originals: hot_tick_fee.retain(hot_tick_refs),
        hot_tick_archived_count,
    })
}

/// Both old and projected producers use this original index contribution and
/// finish path. An error never returns a root or a partially constructed source.
fn with_genesis_index_v1<T>(
    store: &IncrementalPageStore,
    config: &GenesisConfigV1,
    physical: RuntimeGenesisIndexBudgetsV1,
    cancelled: &AtomicBool,
    produce: impl FnOnce(
        &mut dyn FnMut(
            ReservedValue<oasis7_formal_protocol::incremental_execution::ExecutionWriteV1>,
        ) -> Result<(), ValueTreeError>,
    ) -> Result<T, RuntimeGenesisPhysicalErrorV1>,
) -> Result<(ReservedValue<RootSetV1>, T), RuntimeGenesisPhysicalErrorV1> {
    let mut index = store.genesis_state_index_builder(
        config,
        physical.index,
        physical.index_reads,
        physical.index_writes,
        cancelled,
    )?;
    let mut result = None;
    index.contribute::<RuntimeGenesisPhysicalErrorV1>(|sink| {
        result = Some(produce(sink)?);
        Ok(())
    })?;
    let roots = index.finish()?;
    if cancelled.load(Ordering::Acquire) {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((
        roots,
        result.ok_or(ValueTreeError::Invalid("runtime GEN missing source"))?,
    ))
}
