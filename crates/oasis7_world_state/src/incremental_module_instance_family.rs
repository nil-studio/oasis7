//! Exact-root structural instance qualification, distinct from invocation authority.
use super::{
    incremental_agent_mutation::BodyMutationError,
    incremental_module_instance_recovery::ModuleInstanceRecoveryContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionTransaction, RootSetV1},
    incremental_scratch_counter::CounterScratchStore,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanContext},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use crate::{
    incremental_module_instance_fields::ModuleInstanceKeyV1,
    incremental_module_instance_index::{
        MODULE_INSTANCE_INDEX_KEY_V1, is_module_instance_count_key,
    },
};

const FEE: usize = 1024;
pub struct ModuleInstanceFamilyQualificationV1 {
    roots: [u8; 32],
    business: StateRootV1,
    instances: u64,
    modules: u64,
    class: MemoryClass,
}
impl ModuleInstanceFamilyQualificationV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn instance_count(&self) -> u64 {
        self.instances
    }
    pub fn module_count(&self) -> u64 {
        self.modules
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), BodyMutationError> {
        if roots.digest()? != self.roots
            || roots.state_root(StateNamespaceV1::Business) != &self.business
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(super) fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("module instance family binding")
}
pub(super) fn owns(key: &[u8]) -> bool {
    ModuleInstanceKeyV1::is_instance_key(key)
        || key.starts_with(MODULE_INSTANCE_INDEX_KEY_V1)
        || is_module_instance_count_key(key)
}
pub fn validate_module_instance_transaction_v1<R: MerkleNodeReader>(
    proof: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    txn: &ExecutionTransaction<'_, R>,
    memory: &MemoryBudget,
) -> Result<(), BodyMutationError> {
    if !txn.uses_budget(memory)
        || !proof.uses_budget(memory)
        || proof.reserved_bytes() < FEE
        || proof.class != MemoryClass::Ordinary
        || proof.roots != txn.parent_fence().parent_roots
    {
        return Err(invalid());
    }
    Ok(())
}
impl<R: MerkleNodeReader, V: ValueNodeReader, S: CounterScratchStore>
    ModuleInstanceRecoveryContext<'_, R, V, S>
{
    /// Full six-root scope, including rejection of misplaced dedicated keys.
    pub fn qualify_family(
        &mut self,
        roots: &RootSetV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<ModuleInstanceFamilyQualificationV1>, BodyMutationError> {
        roots.validate()?;
        if self.root != roots.state_root(StateNamespaceV1::Business) {
            return Err(invalid());
        }
        let charge = self
            .memory
            .reserve(FEE + 8192, self.class)
            .map_err(ValueTreeError::Memory)?;
        for root in &roots.state {
            if root.namespace == StateNamespaceV1::Business {
                continue;
            }
            StateScanContext {
                source: self.state,
                memory: self.memory,
                class: self.class,
                io: self.state_io,
            }
            .visit_entries(
                root,
                |entry| {
                    if owns(&entry.key) {
                        Err(invalid())
                    } else {
                        Ok(())
                    }
                },
                &mut cancelled,
            )
            .map_err(super::incremental_module_instance_recovery::scan_error)?;
        }
        let content = self.verify(&mut cancelled)?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        mint(
            roots,
            content.instances,
            content.modules,
            self.memory,
            self.class,
            charge,
        )
    }
}
pub(super) fn mint(
    roots: &RootSetV1,
    instances: u64,
    modules: u64,
    memory: &MemoryBudget,
    class: MemoryClass,
    mut scratch: oasis7_proto::memory_budget::MemoryReservation,
) -> Result<ReservedValue<ModuleInstanceFamilyQualificationV1>, BodyMutationError> {
    let _ = memory;
    let roots_digest = roots.digest()?;
    let business = roots.state_root(StateNamespaceV1::Business).clone();
    Ok(scratch
        .split(FEE)
        .map_err(BodyMutationError::Invalid)?
        .retain(ModuleInstanceFamilyQualificationV1 {
            roots: roots_digest,
            business,
            instances,
            modules,
            class,
        }))
}
/// Resource re-admission only: exact binding, fixed metadata, no source or history IO.
pub fn readmit_module_instance_family_ordinary_v1(
    source: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<ModuleInstanceFamilyQualificationV1>, BodyMutationError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < FEE
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let scratch = memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    source.validate_binding(roots)?;
    let output = mint(
        roots,
        source.instances,
        source.modules,
        memory,
        MemoryClass::Ordinary,
        scratch,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(output)
}
