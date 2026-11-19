//! Exact verifier inputs: historical body ID, current candidate authority.
use super::provisional::AcceptedCandidateCache;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionParent, incremental_material::CheckedCommitCore,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub(super) fn transition_parent(
    cache: &AcceptedCandidateCache,
    next: &ReservedValue<CheckedCommitCore>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<ExecutionParent>, ValueTreeError> {
    cache.check_parent(next, memory, &mut cancelled)?;
    let roots = &*cache.roots;
    let bytes = roots.state.iter().try_fold(
        std::mem::size_of::<ExecutionParent>()
            .checked_add(512)
            .and_then(|n| n.checked_add(roots.world_id.len()))
            .ok_or(ValueTreeError::Invalid("IWS transition root accounting"))?,
        |n, r| {
            n.checked_add(r.node_root.len())
                .and_then(|n| n.checked_add(128))
                .ok_or(ValueTreeError::Invalid("IWS transition root accounting"))
        },
    )?;
    let charge = memory.reserve(bytes, MemoryClass::Ordinary)?;
    let parent = ExecutionParent {
        commit: cache.runtime.occurrence().head_body_id(),
        roots: roots.clone(),
        authority: next.body().authority,
    };
    let record = next
        .transition()
        .ok_or(ValueTreeError::Invalid("IWS ordinary transition missing"))?;
    if record.parent_commit != parent.commit
        || record.authority != parent.authority
        || record.before != parent.roots
    {
        return Err(ValueTreeError::Invalid(
            "IWS actual verified transition parent mismatch",
        ));
    }
    Ok(charge.retain(parent))
}
