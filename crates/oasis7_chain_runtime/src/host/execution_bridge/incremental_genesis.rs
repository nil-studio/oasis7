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
    Ok(ProvisionalProjectedRuntimeGenesisIndexV1 {
        outer,
        roots,
        projected,
        retention,
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

#[cfg(all(test, unix))]
#[path = "incremental_genesis_tests.rs"]
mod tests;
