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
    incremental_value::{ValueIoBudget, ValueTreeError},
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
    if context.cancelled.load(Ordering::Acquire) {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !context.memory.same_pool(store.memory_budget()) {
        return Err(ValueTreeError::Invalid("runtime GEN physical context pool").into());
    }
    let cancelled = context.cancelled;
    let mut index = store.genesis_state_index_builder(
        config,
        physical.index,
        physical.index_reads,
        physical.index_writes,
        cancelled,
    )?;
    let mut pages = store.value_session_with_cancelled(physical.producer_pages, cancelled);
    let mut summary = None;
    index.contribute::<RuntimeGenesisPhysicalErrorV1>(|sink| {
        let result = world.prepare_iws_genesis_v1(
            &mut MemberRecordWriteContext {
                sink: &mut pages,
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
        )?;
        summary = Some(result);
        Ok(())
    })?;
    let roots = index.finish()?;
    if cancelled.load(Ordering::Acquire) {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(ProvisionalRuntimeGenesisIndexV1 {
        roots,
        summary: summary.ok_or(ValueTreeError::Invalid("runtime GEN missing summary"))?,
    })
}

#[cfg(all(test, unix))]
#[path = "incremental_genesis_tests.rs"]
mod tests;
