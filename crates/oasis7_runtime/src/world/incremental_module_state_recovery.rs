//! Bounded recovery qualification for the dedicated opaque module-state key
//! family. It returns counts only and never hydrates the module map.
use super::{
    incremental_body::BodyMutationError, incremental_module_state::ModuleStateContentBudgetV1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError,
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::incremental_module_fields::ModuleStateKeyV1;
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedModuleStateContentV1 {
    pub states: u64,
    pub logical_bytes: u64,
}

pub struct ModuleStateRecoveryContextV1<'a, R, V> {
    pub root: &'a StateRootV1,
    pub state: &'a R,
    pub values: &'a V,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub state_io: &'a mut StateScanBudget,
    pub value_io: &'a mut ValueIoBudget,
    pub content: &'a mut ModuleStateContentBudgetV1,
}

impl<R: MerkleNodeReader, V: ValueNodeReader> ModuleStateRecoveryContextV1<'_, R, V> {
    pub fn verify(
        &mut self,
        cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<VerifiedModuleStateContentV1>, BodyMutationError> {
        let mut phase = self
            .memory
            .reserve(64 << 10, self.class)
            .map_err(ValueTreeError::Memory)?;
        if self.root.namespace != StateNamespaceV1::Business {
            return Err(BodyMutationError::Invalid(
                "module state recovery namespace",
            ));
        }
        let cancelled = RefCell::new(cancelled);
        let mut result = VerifiedModuleStateContentV1 {
            states: 0,
            logical_bytes: 0,
        };
        StateScanContext {
            source: self.state,
            memory: self.memory,
            class: self.class,
            io: self.state_io,
        }
        .visit_entries(
            self.root,
            |entry| {
                if !ModuleStateKeyV1::is_state_key(&entry.key) {
                    return Ok(());
                }
                ModuleStateKeyV1::state_key_from_bytes(&entry.key)
                    .map_err(BodyMutationError::Invalid)?;
                if self.content.states_remaining == 0 {
                    return Err(
                        ValueTreeError::ResourceLimited("module state recovery count").into(),
                    );
                }
                let mut reader = ValueReadContext {
                    source: self.values,
                    memory: self.memory,
                    class: self.class,
                    io: self.value_io,
                };
                let root = reader.load_root(&entry.value)?;
                let length = root.0.logical_len;
                if length > self.content.max_state_bytes
                    || length > self.content.logical_bytes_remaining
                {
                    return Err(
                        ValueTreeError::ResourceLimited("module state recovery bytes").into(),
                    );
                }
                self.content.states_remaining -= 1;
                self.content.logical_bytes_remaining -= length;
                // Includes the typed empty leaf for a present empty state.
                reader.verify_closure(&entry.value, self.content.max_state_bytes, || {
                    (cancelled.borrow_mut())()
                })?;
                result.states = result
                    .states
                    .checked_add(1)
                    .ok_or(BodyMutationError::Invalid(
                        "module state recovery count overflow",
                    ))?;
                result.logical_bytes =
                    result
                        .logical_bytes
                        .checked_add(length)
                        .ok_or(BodyMutationError::Invalid(
                            "module state recovery bytes overflow",
                        ))?;
                Ok(())
            },
            || (cancelled.borrow_mut())(),
        )
        .map_err(scan_error)?;
        if (cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(phase
            .split(std::mem::size_of::<VerifiedModuleStateContentV1>())
            .map_err(BodyMutationError::Invalid)?
            .retain(result))
    }
}

pub(super) fn scan_error(error: StateScanError<BodyMutationError>) -> BodyMutationError {
    match error {
        StateScanError::Visitor(error) => error,
        StateScanError::Memory(error) => ValueTreeError::Memory(error).into(),
        StateScanError::ResourceLimited => {
            ValueTreeError::ResourceLimited("module state recovery scan").into()
        }
        StateScanError::Cancelled => ValueTreeError::Cancelled.into(),
        StateScanError::Tree(error) => ExecutionError::Tree(error).into(),
        StateScanError::Invalid(reason) => BodyMutationError::Invalid(reason),
    }
}
