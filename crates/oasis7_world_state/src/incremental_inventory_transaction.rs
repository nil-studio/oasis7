//! Guarded contribution of authenticated inventory points to the shared overlay.
use crate::incremental_inventory_change::{
    InventoryOutputV1, PreparedInventoryChangeV1, write_digest,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateWriteV1},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

#[derive(Debug)]
pub enum InventoryContributionError {
    Execution(ExecutionError),
    Invalid(&'static str),
    DependencyConflict,
    Cancelled,
}
impl From<ExecutionError> for InventoryContributionError {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}
impl std::fmt::Display for InventoryContributionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(error) => error.fmt(f),
            Self::Invalid(reason) => f.write_str(reason),
            Self::DependencyConflict => f.write_str("inventory transaction dependency conflict"),
            Self::Cancelled => f.write_str("inventory transaction contribution cancelled"),
        }
    }
}
impl std::error::Error for InventoryContributionError {}

/// Contribute owner-decided final roots, not sequential business commands.
/// Every error poisons the complete transaction, including invalid input/guard.
/// The caller retains the provisional pages and their closure obligations.
pub fn contribute_inventory_change_v1<R: MerkleNodeReader>(
    prepared: &ReservedValue<PreparedInventoryChangeV1>,
    writes: &[ReservedValue<StateWriteV1>],
    transaction: &mut ExecutionTransaction<'_, R>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(), InventoryContributionError> {
    transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if !transaction.uses_budget(memory) || !prepared.uses_budget(memory) {
            return Err(InventoryContributionError::Invalid(
                "inventory contribution pool",
            ));
        }
        if cancelled() {
            return Err(InventoryContributionError::Cancelled);
        }
        let _workspace = memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::from)?;
        if transaction
            .parent_state_root(StateNamespaceV1::Business)
            .canonical_bytes()
            .map_err(|_| InventoryContributionError::Invalid("inventory transaction parent"))?
            .as_slice()
            != prepared.before_root
        {
            return Err(InventoryContributionError::DependencyConflict);
        }
        drop(_workspace);
        apply_inventory_output_v1(
            &prepared.output,
            writes.iter(),
            transaction,
            memory,
            &mut cancelled,
        )
    })
}

pub(crate) fn apply_inventory_output_v1<
    'w,
    R: MerkleNodeReader,
    I: Iterator<Item = &'w ReservedValue<StateWriteV1>> + Clone,
>(
    output: &InventoryOutputV1,
    writes: I,
    transaction: &mut ExecutionTransaction<'_, R>,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), InventoryContributionError> {
    if !transaction.uses_budget(memory) {
        return Err(InventoryContributionError::Invalid(
            "inventory contribution pool",
        ));
    }
    let _workspace = memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ExecutionError::from)?;
    let count = writes.clone().take(3).count();
    if count != usize::from(output.records) || count > 2 {
        return Err(InventoryContributionError::Invalid(
            "inventory contribution write count",
        ));
    }
    // Validate the whole tiny batch before mutating any overlay entry.
    for (index, write) in writes.clone().enumerate() {
        let required =
            write
                .key
                .capacity()
                .checked_add(1024)
                .ok_or(InventoryContributionError::Invalid(
                    "inventory contribution write charge",
                ))?;
        if write.reserved_bytes() < required {
            return Err(InventoryContributionError::Invalid(
                "inventory contribution write charge",
            ));
        }
        let digest = write_digest(write)
            .map_err(|_| InventoryContributionError::Invalid("inventory contribution write key"))?;
        if output.write_digests[index] != Some(digest) {
            return Err(InventoryContributionError::Invalid(
                "inventory contribution prepared write",
            ));
        }
        if !write.uses_budget(memory) || writes.clone().take(index).any(|old| old.key == write.key)
        {
            return Err(InventoryContributionError::Invalid(
                "inventory contribution write ownership/duplicate",
            ));
        }
        let dependency = output
            .dependencies
            .iter()
            .flatten()
            .find(|dependency| dependency.key.as_bytes() == write.key)
            .ok_or(InventoryContributionError::Invalid(
                "inventory contribution unrelated key",
            ))?;
        if !dependency.writable || dependency.root != write.before {
            return Err(InventoryContributionError::Invalid(
                "inventory contribution precondition",
            ));
        }
    }
    for dependency in output.dependencies.iter().flatten() {
        if cancelled() {
            return Err(InventoryContributionError::Cancelled);
        }
        if transaction.read(StateNamespaceV1::Business, dependency.key.as_bytes())?
            != dependency.root
        {
            return Err(InventoryContributionError::DependencyConflict);
        }
    }
    for write in writes {
        if cancelled() {
            return Err(InventoryContributionError::Cancelled);
        }
        transaction.write(StateNamespaceV1::Business, &write.key, write.after)?;
    }
    if cancelled() {
        return Err(InventoryContributionError::Cancelled);
    }
    Ok(())
}
