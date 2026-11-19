//! Resident complete key DATA retained by the original module-state Q.
use super::*;
use crate::incremental_module_state_recovery::ModuleStateContentBudgetV1;
use oasis7_proto::memory_budget::MemoryReservation;
use std::{collections::BTreeMap, sync::Arc};

pub(crate) struct ModuleStateKeyDirectoryV1 {
    keys: BTreeMap<String, MemoryReservation>,
    _layout: MemoryReservation,
    bytes: usize,
    class: MemoryClass,
}
impl ModuleStateKeyDirectoryV1 {
    pub(crate) fn empty(
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, BodyMutationError> {
        let fee = memory
            .reserve(std::mem::size_of::<Self>() + 128, class)
            .map_err(ValueTreeError::Memory)?;
        let bytes = fee.bytes();
        Ok(Self {
            keys: BTreeMap::new(),
            _layout: fee,
            bytes,
            class,
        })
    }
    pub(crate) fn insert(
        &mut self,
        key: &str,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), BodyMutationError> {
        if self.keys.contains_key(key) {
            return Err(invalid());
        }
        // A BTreeMap allocates a full node even for its first key. Include a
        // conservative leaf/internal split envelope per inserted key, rather
        // than averaging node capacity over a large resident directory.
        let bytes = key.len().checked_add(4096).ok_or_else(invalid)?;
        let fee = memory
            .reserve(bytes, class)
            .map_err(ValueTreeError::Memory)?;
        self.bytes = self.bytes.checked_add(fee.bytes()).ok_or_else(invalid)?;
        self.keys.insert(key.to_owned(), fee);
        Ok(())
    }
    pub(crate) fn keys(&self) -> impl ExactSizeIterator<Item = &str> {
        self.keys.keys().map(String::as_str)
    }
    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }
    pub(crate) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        // All node charges were minted with the same pool as this layout.
        self._layout.uses_budget(memory)
    }
    pub(crate) fn ordinary_coverage(
        &self,
        memory: &MemoryBudget,
    ) -> Result<Option<MemoryReservation>, BodyMutationError> {
        if !self.uses_budget(memory) {
            return Err(invalid());
        }
        if self.class == MemoryClass::Ordinary {
            return Ok(None);
        }
        Ok(Some(
            memory
                .reserve(self.bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
        ))
    }
    pub(crate) fn covered_ordinary(
        &self,
        fee: Option<&MemoryReservation>,
        memory: &MemoryBudget,
    ) -> bool {
        self.uses_budget(memory)
            && (self.class == MemoryClass::Ordinary
                || fee.is_some_and(|f| f.uses_budget(memory) && f.bytes() >= self.bytes))
    }
    pub(crate) fn copy_changed(
        &self,
        memory: &MemoryBudget,
        content: &mut ModuleStateContentBudgetV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, BodyMutationError> {
        let mut next = Self::empty(memory, MemoryClass::Ordinary)?;
        for key in self.keys() {
            pay_key(key, self.len(), content, &mut cancelled)?;
            next.insert(key, memory, MemoryClass::Ordinary)?;
        }
        Ok(next)
    }
    pub(crate) fn change(
        &mut self,
        key: &str,
        before: bool,
        after: bool,
        memory: &MemoryBudget,
    ) -> Result<(), BodyMutationError> {
        if self.keys.contains_key(key) != before {
            return Err(invalid());
        }
        match (before, after) {
            (false, true) => self.insert(key, memory, MemoryClass::Ordinary)?,
            (true, false) => {
                if let Some(fee) = self.keys.remove(key) {
                    self.bytes = self.bytes.checked_sub(fee.bytes()).ok_or_else(invalid)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
pub(crate) fn pay_key(
    key: &str,
    count: usize,
    content: &mut ModuleStateContentBudgetV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), BodyMutationError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let comparisons = usize::BITS - count.max(1).leading_zeros();
    let bytes = (key.len() as u64)
        .checked_add(1)
        .and_then(|n| n.checked_mul((u64::from(comparisons) + 2) * 32))
        .ok_or_else(invalid)?;
    content.logical_bytes_remaining = content.logical_bytes_remaining.checked_sub(bytes).ok_or(
        ValueTreeError::ResourceLimited("module state directory key work"),
    )?;
    Ok(())
}

impl ModuleStateFamilyQualificationV1 {
    /// Complete accepted key DATA. Current callers must additionally validate
    /// this Q against their actual transaction and fold its pending mutations.
    pub fn state_keys(&self) -> impl ExactSizeIterator<Item = &str> {
        self.directory.keys()
    }
}
pub(crate) fn retained(directory: ModuleStateKeyDirectoryV1) -> Arc<ModuleStateKeyDirectoryV1> {
    Arc::new(directory)
}
