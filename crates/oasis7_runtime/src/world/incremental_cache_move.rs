//! An admitted payload move, never Formal acceptance or execution authority.
use oasis7_formal_protocol::incremental_execution::ExecutionError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::mem::size_of;

/// The payload's owner decides its meaning. This container cannot mint a fence.
pub struct RuntimeCacheSlotV1<T> {
    current: Option<ReservedValue<Box<T>>>,
    generation: u64,
}
impl<T> Default for RuntimeCacheSlotV1<T> {
    fn default() -> Self {
        Self {
            current: None,
            generation: 0,
        }
    }
}
impl<T> RuntimeCacheSlotV1<T> {
    pub fn current(&self) -> Option<&T> {
        self.current.as_ref().map(|v| &***v)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn is_empty(&self) -> bool {
        self.current.is_none()
    }
    /// The exclusive borrow locks the destination until finish or cancellation.
    /// All allocation and checked generation arithmetic precede publication.
    pub fn prepare_empty_bounded<'a>(
        &'a mut self,
        payload: T,
        memory: &MemoryBudget,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<PreparedCacheMoveV1<'a, T>, ExecutionError> {
        if stop() {
            return Err(ExecutionError::Aborted);
        }
        if self.current.is_some() {
            return Err(ExecutionError::ParentConflict);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(ExecutionError::ResourceLimited("cache generation"))?;
        // Includes the allocated T and both the retained value and slot headers.
        let bytes = size_of::<T>()
            .checked_add(size_of::<ReservedValue<Box<T>>>())
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .and_then(|n| n.checked_add(size_of::<PreparedCacheMoveV1<'a, T>>()))
            .ok_or(ExecutionError::ResourceLimited("cache move layout"))?;
        let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
        let next = fee.retain(Box::new(payload));
        if stop() {
            return Err(ExecutionError::Aborted);
        }
        Ok(PreparedCacheMoveV1 {
            slot: self,
            next,
            generation,
        })
    }
}
/// Dropping a preparation releases only the new payload and leaves the slot.
pub struct PreparedCacheMoveV1<'a, T> {
    slot: &'a mut RuntimeCacheSlotV1<T>,
    next: ReservedValue<Box<T>>,
    generation: u64,
}
impl<T> PreparedCacheMoveV1<'_, T> {
    /// No IO, allocation, hash, assertion, cancellation or fallible work.
    pub fn finish(self) -> Option<ReservedValue<Box<T>>> {
        let old = self.slot.current.replace(self.next);
        self.slot.generation = self.generation;
        old
    }
}
#[cfg(test)]
#[path = "incremental_cache_move_tests.rs"]
mod tests;
