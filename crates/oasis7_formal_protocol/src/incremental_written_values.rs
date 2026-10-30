//! Candidate delta values. Full genesis/current-state closure is a separate
//! obligation: absence of a transition cannot produce this delta-only marker.
use super::*;
use crate::incremental_value::{
    ValueClosureSummary, ValueNodeReader, ValueReadContext, ValueTreeError,
};

pub struct WrittenValueBudget {
    pub values_remaining: u64,
    pub logical_bytes_remaining: u64,
    pub max_value_bytes: u64,
}
#[derive(Debug)]
pub struct CheckedWrittenValues {
    body: [u8; 32],
    values: u64,
    logical_bytes: u64,
    node_occurrences: u64,
}
impl CheckedWrittenValues {
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub fn values(&self) -> u64 {
        self.values
    }
    pub fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }
    pub fn node_occurrences(&self) -> u64 {
        self.node_occurrences
    }
}
/// Every final non-deleted write is checked, including repeated references.
/// Success binds only these value bytes to this candidate. It grants neither
/// parent/genesis closure nor business legality, retention or acceptance.
pub fn verify_written_values<R: ValueNodeReader>(
    core: &CheckedCommitCore,
    context: &mut ValueReadContext<'_, R>,
    budget: &mut WrittenValueBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedWrittenValues>, ValueTreeError> {
    let charge = context.memory.reserve(16 * 1024, MemoryClass::Ordinary)?;
    let transition = core
        .transition
        .as_ref()
        .ok_or(ValueTreeError::Invalid("delta values require a transition"))?;
    if context.class != MemoryClass::Ordinary
        || !transition.uses_budget(context.memory)
        || !core.catalog.uses_budget(context.memory)
    {
        return Err(ValueTreeError::Invalid("written value memory pool/class"));
    }
    let mut result = CheckedWrittenValues {
        body: core.catalog.entry.commit,
        values: 0,
        logical_bytes: 0,
        node_occurrences: 0,
    };
    for write in &transition.writes {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let Some(id) = write.mutation.after else {
            continue;
        };
        let summary = check_value_reference(context, &id, budget, &mut cancelled)?;
        let length = summary.root().0.logical_len;
        result.values += 1;
        result.logical_bytes =
            result
                .logical_bytes
                .checked_add(length)
                .ok_or(ValueTreeError::ResourceLimited(
                    "written logical byte total",
                ))?;
        result.node_occurrences = result
            .node_occurrences
            .checked_add(summary.node_occurrences())
            .ok_or(ValueTreeError::ResourceLimited("written value node total"))?;
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}

fn check_value_reference<R: ValueNodeReader>(
    context: &mut ValueReadContext<'_, R>,
    id: &[u8; 32],
    budget: &mut WrittenValueBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ValueClosureSummary, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if budget.values_remaining == 0 {
        return Err(ValueTreeError::ResourceLimited("value occurrences"));
    }
    let admitted = budget.max_value_bytes.min(budget.logical_bytes_remaining);
    budget.values_remaining -= 1;
    budget.logical_bytes_remaining -= admitted;
    let summary = context.verify_closure(id, admitted, &mut cancelled)?;
    budget.logical_bytes_remaining += admitted - summary.root().0.logical_len;
    Ok(summary)
}

#[path = "incremental_full_state_values.rs"]
mod full_state;
pub use full_state::*;
