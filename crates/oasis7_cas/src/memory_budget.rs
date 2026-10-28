//! Shared admission accounting for incremental storage allocations.
//!
//! A reservation must be acquired before allocating. Moving it with a buffer
//! transfers ownership without releasing capacity; an Arc must own both the
//! buffer and its reservation so eviction cannot uncharge a live reader.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct Pool {
    limit: usize,
    recovery_reserve: usize,
    used: AtomicUsize,
}

/// Clone this handle across load, commit, cache, query and transfer consumers.
#[derive(Debug, Clone)]
pub struct MemoryBudget(Arc<Pool>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryClass {
    Ordinary,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryLimitExceeded {
    pub requested: usize,
    pub used: usize,
    pub admission_limit: usize,
}

impl std::fmt::Display for MemoryLimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "memory reservation of {} bytes exceeds admission limit {} ({} used)",
            self.requested, self.admission_limit, self.used
        )
    }
}

impl std::error::Error for MemoryLimitExceeded {}

impl MemoryBudget {
    /// Recovery reserve is part of the total limit, never extra capacity.
    pub fn new(limit: usize, recovery_reserve: usize) -> Result<Self, &'static str> {
        if recovery_reserve > limit {
            return Err("recovery reserve exceeds total memory limit");
        }
        Ok(Self(Arc::new(Pool {
            limit,
            recovery_reserve,
            used: AtomicUsize::new(0),
        })))
    }

    pub fn used(&self) -> usize {
        self.0.used.load(Ordering::Acquire)
    }

    /// Nonblocking admission avoids deadlock while an operation holds pages.
    /// Callers reserve their complete next allocation before expanding work.
    pub fn reserve(
        &self,
        bytes: usize,
        class: MemoryClass,
    ) -> Result<MemoryReservation, MemoryLimitExceeded> {
        let limit = match class {
            MemoryClass::Ordinary => self.0.limit - self.0.recovery_reserve,
            MemoryClass::Recovery => self.0.limit,
        };
        self.0
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= limit)
            })
            .map_err(|used| MemoryLimitExceeded {
                requested: bytes,
                used,
                admission_limit: limit,
            })?;
        Ok(MemoryReservation {
            pool: self.0.clone(),
            bytes,
        })
    }
}

/// Unique ownership prevents duplicate releases. Share an Arc of the complete
/// charged allocation, never clone the reservation separately from its data.
#[derive(Debug)]
pub struct MemoryReservation {
    pool: Arc<Pool>,
    bytes: usize,
}

impl MemoryReservation {
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Bind an allocation made under this reservation to its charge. Keep this
    /// complete value in caches and reader Arcs, so data cannot outlive charge.
    /// The caller must include the allocation's capacity and owned children.
    pub fn retain<T>(self, value: T) -> ReservedValue<T> {
        ReservedValue {
            value,
            reservation: self,
        }
    }

    /// Partition a charge for independently retained output and scratch data.
    pub fn split(&mut self, bytes: usize) -> Result<Self, &'static str> {
        if bytes > self.bytes {
            return Err("reservation split exceeds owned bytes");
        }
        self.bytes -= bytes;
        Ok(Self {
            pool: self.pool.clone(),
            bytes,
        })
    }
}

/// Immutable access deliberately provides no way to detach data or grow its
/// allocation. Field order drops data before returning its reserved capacity.
#[derive(Debug)]
pub struct ReservedValue<T> {
    value: T,
    reservation: MemoryReservation,
}

impl<T> ReservedValue<T> {
    pub fn reserved_bytes(&self) -> usize {
        self.reservation.bytes()
    }
}

impl<T> std::ops::Deref for ReservedValue<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.pool.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

#[cfg(test)]
#[path = "memory_budget_tests.rs"]
mod tests;
