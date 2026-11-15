//! A continuing scalar budget shared by storage and future bounded providers.
//! Neither a budget, an identity nor a context grants execution or signing authority.
use super::FormalError;
use oasis7_proto::memory_budget::MemoryBudget;
use std::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct WorkIdentityV1(u64);
impl std::fmt::Debug for WorkIdentityV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkIdentityV1").finish_non_exhaustive()
    }
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn invalid(message: &str) -> FormalError {
    FormalError::Invalid(message.to_owned())
}
fn next_identity(counter: &AtomicU64) -> Result<WorkIdentityV1, FormalError> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
        .map(WorkIdentityV1)
        .map_err(|_| invalid("bounded archive meter identity exhausted"))
}
/// No Clone, setter, addition or alternate counter store. Cell makes this !Sync.
pub struct FormalBoundedWorkV1 {
    remaining_steps: Cell<u64>,
    remaining_read_bytes: Cell<u64>,
    identity: WorkIdentityV1,
    child_active: Cell<bool>,
}
impl FormalBoundedWorkV1 {
    pub fn new(steps: u64, read_bytes: u64) -> Result<Self, FormalError> {
        Ok(Self {
            remaining_steps: Cell::new(steps),
            remaining_read_bytes: Cell::new(read_bytes),
            identity: next_identity(&NEXT_ID)?,
            child_active: Cell::new(false),
        })
    }
    pub fn identity(&self) -> WorkIdentityV1 {
        self.identity
    }
    pub fn is_child_active(&self) -> bool {
        self.child_active.get()
    }
    pub fn remaining_steps(&self) -> u64 {
        self.remaining_steps.get()
    }
    pub fn remaining_read_bytes(&self) -> u64 {
        self.remaining_read_bytes.get()
    }
    /// Cancellation may itself charge this meter. Read CURRENT balances only
    /// after the callback and reject parent reentry on both sides of it.
    pub fn charge(
        &self,
        steps: u64,
        bytes: u64,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        if self.is_child_active() {
            return Err(invalid("bounded archive child read active"));
        }
        if stop() {
            return Err(invalid("bounded archive read cancelled"));
        }
        if self.is_child_active() {
            return Err(invalid("bounded archive child read active"));
        }
        self.deduct(steps, bytes)
    }
    pub fn consume_steps(&self, n: u64, stop: &mut dyn FnMut() -> bool) -> Result<(), FormalError> {
        self.charge(n, 0, stop)
    }
    pub fn consume_read_bytes(
        &self,
        n: u64,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        self.charge(1, n, stop)
    }
    fn deduct(&self, steps: u64, bytes: u64) -> Result<(), FormalError> {
        let previous_steps = self.remaining_steps.get();
        let previous_bytes = self.remaining_read_bytes.get();
        self.remaining_steps
            .set(previous_steps.saturating_sub(steps));
        self.remaining_read_bytes
            .set(previous_bytes.saturating_sub(bytes));
        if previous_steps < steps {
            return Err(invalid("bounded archive read steps"));
        }
        if previous_bytes < bytes {
            return Err(invalid("bounded archive read bytes"));
        }
        Ok(())
    }
    /// Pure Cell gate. Existing adapters keep their original charge placement.
    /// No cancellation callback or extra step is hidden in this operation.
    pub fn begin_child(&self) -> Result<ChildLeaseV1<'_>, FormalError> {
        if self.child_active.replace(true) {
            return Err(invalid("bounded archive child read active"));
        }
        Ok(ChildLeaseV1 {
            work: self,
            finished: false,
        })
    }
}
/// A private-field, non-Clone exclusive borrow. Its only scalar operation is a
/// one-way debit. Trusted legacy adapters keep before/after arithmetic private;
/// they must not expose this lease to user cancellation callbacks.
pub struct ChildLeaseV1<'a> {
    work: &'a FormalBoundedWorkV1,
    finished: bool,
}
impl ChildLeaseV1<'_> {
    /// Identity of the actual borrowed meter, without minting another balance.
    pub fn identity(&self) -> WorkIdentityV1 {
        self.work.identity()
    }
    pub fn remaining_steps(&self) -> u64 {
        self.work.remaining_steps()
    }
    pub fn remaining_read_bytes(&self) -> u64 {
        self.work.remaining_read_bytes()
    }
    /// No callback here: reconciliation preserves an existing child first error.
    /// A failing debit still saturates BOTH balances, with steps error first.
    pub fn consume(&self, steps: u64, bytes: u64) -> Result<(), FormalError> {
        self.work.deduct(steps, bytes)
    }
    /// Unlock without refunding. Call even after a normally returned child error
    /// once its attempted work has been debited; abandonment/panic must not finish.
    pub fn finish(mut self) {
        self.finished = true;
        self.work.child_active.set(false);
    }
}
impl Drop for ChildLeaseV1<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.work.remaining_steps.set(0);
            self.work.remaining_read_bytes.set(0);
            self.work.child_active.set(false);
        }
    }
}
/// A stack-only borrow of the caller's existing pool, meter and cancellation.
/// Construction creates neither a budget nor a permit and has no hidden debit.
pub struct FormalBoundedContextV1<'a> {
    memory: &'a MemoryBudget,
    work: &'a FormalBoundedWorkV1,
    stop: &'a mut dyn FnMut() -> bool,
}
impl<'a> FormalBoundedContextV1<'a> {
    pub fn new(
        memory: &'a MemoryBudget,
        work: &'a FormalBoundedWorkV1,
        stop: &'a mut dyn FnMut() -> bool,
    ) -> Self {
        Self { memory, work, stop }
    }
    pub fn memory(&self) -> &MemoryBudget {
        self.memory
    }
    pub fn work(&self) -> &FormalBoundedWorkV1 {
        self.work
    }
    pub fn charge(&mut self, steps: u64, bytes: u64) -> Result<(), FormalError> {
        self.work.charge(steps, bytes, self.stop)
    }
    pub fn consume_steps(&mut self, n: u64) -> Result<(), FormalError> {
        self.work.consume_steps(n, self.stop)
    }
    pub fn consume_read_bytes(&mut self, n: u64) -> Result<(), FormalError> {
        self.work.consume_read_bytes(n, self.stop)
    }
    pub fn parts(
        &mut self,
    ) -> (
        &MemoryBudget,
        &FormalBoundedWorkV1,
        &mut dyn FnMut() -> bool,
    ) {
        (self.memory, self.work, self.stop)
    }
}
#[cfg(test)]
#[path = "bounded_context_tests.rs"]
mod tests;
