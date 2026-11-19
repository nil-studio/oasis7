//! Actual qualified Current agent and Meta record reads.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn agent<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    id: &str,
    sources: &MetaProgressSourcesV1<'_>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut MetaProgressWorkV1,
    live: &ExecutionFence,
) -> Result<bool, BodyMutationError> {
    work.spend(id.len())?;
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = ctx.transaction.guard();
    Ok(read_current_agent_presence_v1(
        id,
        sources.agents,
        roots,
        &mut ctx,
        parent_read,
        &mut work.pending_entries_remaining,
        &guard,
        live,
    )
    .map_err(overlay_error)?
        == CurrentAgentPresenceV1::Existing)
}
