//! Full current state/value integrity for genesis and explicit bootstrap or
//! recovery. Ordinary commit consumers must use delta checks instead.
use super::*;
use crate::incremental_state::{StateScanBudget, StateScanContext, StateScanError};
use std::cell::RefCell;

#[derive(Debug)]
pub struct CheckedStateValues {
    body: [u8; 32],
    leaves: [u64; 6],
    logical_bytes: u64,
    value_nodes: u64,
}
impl CheckedStateValues {
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
    /// Fixed RootSet namespace order; counts authenticated leaves, not uniques.
    pub fn leaf_counts(&self) -> [u64; 6] {
        self.leaves
    }
    pub fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }
    pub fn value_node_occurrences(&self) -> u64 {
        self.value_nodes
    }
}
/// Verify every namespace, leaf path/count and complete value. This attests
/// content integrity only; typed fields, artifacts, retention and authority
/// readiness remain full endpoint obligations. No receipt consumes this marker.
pub fn verify_state_values<S: MerkleNodeReader, V: ValueNodeReader>(
    core: &CheckedCommitCore,
    state: &S,
    values: &mut ValueReadContext<'_, V>,
    state_io: &mut StateScanBudget,
    value_budget: &mut WrittenValueBudget,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedStateValues>, StateScanError<ValueTreeError>> {
    let charge = values
        .memory
        .reserve(16 * 1024, values.class)
        .map_err(StateScanError::Memory)?;
    if !core.catalog.uses_budget(values.memory) {
        return Err(StateScanError::Invalid("full state core memory pool"));
    }
    let cancelled = RefCell::new(cancelled);
    let mut result = CheckedStateValues {
        body: core.catalog.entry.commit,
        leaves: [0; 6],
        logical_bytes: 0,
        value_nodes: 0,
    };
    for (index, root) in core.roots.state.iter().enumerate() {
        let mut scan = StateScanContext {
            source: state,
            memory: values.memory,
            class: values.class,
            io: state_io,
        };
        result.leaves[index] = scan.visit_entries(
            root,
            |entry| {
                let summary = check_value_reference(values, &entry.value, value_budget, || {
                    (cancelled.borrow_mut())()
                })?;
                result.logical_bytes = result
                    .logical_bytes
                    .checked_add(summary.root().0.logical_len)
                    .ok_or(ValueTreeError::ResourceLimited("full state logical bytes"))?;
                result.value_nodes = result
                    .value_nodes
                    .checked_add(summary.node_occurrences())
                    .ok_or(ValueTreeError::ResourceLimited("full state value nodes"))?;
                Ok(())
            },
            || (cancelled.borrow_mut())(),
        )?;
    }
    Ok(charge.retain(result))
}
