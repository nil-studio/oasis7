//! Chain-private movable ownership. The reservation never detaches from a live
//! boxed source, including while the source is consumed by Frozen publication.
use super::*;
use oasis7_proto::memory_budget::MemoryReservation;

struct PaidHostCurrentV1 {
    value: Box<HostCurrentV1>,
    fee: MemoryReservation,
}
#[derive(Default)]
pub(super) struct HostCurrentSlotV1 {
    current: Option<PaidHostCurrentV1>,
    generation: u64,
}
impl HostCurrentSlotV1 {
    pub(super) fn current(&self) -> Option<&HostCurrentV1> {
        self.current.as_ref().map(|owner| &*owner.value)
    }
    pub(super) fn is_empty(&self) -> bool {
        self.current.is_none()
    }
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
    pub(super) fn prepare_empty<'a>(
        &'a mut self,
        value: HostCurrentV1,
        memory: &MemoryBudget,
        cancelled: &AtomicBool,
    ) -> Result<PreparedHostCurrentV1<'a>, IwsHostRuntimeErrorV1> {
        if cancelled.load(Ordering::Acquire) {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::Aborted));
        }
        if self.current.is_some() {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(IwsHostRuntimeErrorV1::Move(
                ExecutionError::ResourceLimited("Host generation"),
            ))?;
        let bytes = size_of::<HostCurrentV1>()
            .checked_add(size_of::<PaidHostCurrentV1>())
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .and_then(|n| n.checked_add(size_of::<PreparedHostCurrentV1<'a>>()))
            .ok_or_else(|| FormalError::Invalid("Host current layout".into()))?;
        let fee = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|e| IwsHostRuntimeErrorV1::Move(e.into()))?;
        let next = PaidHostCurrentV1 {
            value: Box::new(value),
            fee,
        };
        if cancelled.load(Ordering::Acquire) {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::Aborted));
        }
        Ok(PreparedHostCurrentV1 {
            slot: self,
            next,
            generation,
        })
    }
    /// An exclusive take removes execution permission before any IO. Checked
    /// arithmetic happens first; a later error cannot expose the old live FD.
    pub(super) fn consume<T>(
        &mut self,
        consume: impl FnOnce(HostCurrentV1) -> Result<T, IwsHostRuntimeErrorV1>,
    ) -> Result<T, IwsHostRuntimeErrorV1> {
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(IwsHostRuntimeErrorV1::Move(
                ExecutionError::ResourceLimited("Host generation"),
            ))?;
        let owner = self
            .current
            .take()
            .ok_or_else(|| FormalError::Invalid("Host current owner missing".into()))?;
        self.generation = generation;
        let PaidHostCurrentV1 { value, fee } = owner;
        // Keep the old shell paid across Err and unwinding as well as success.
        let _fee = fee;
        consume(*value)
    }
}
pub(super) struct PreparedHostCurrentV1<'a> {
    slot: &'a mut HostCurrentSlotV1,
    next: PaidHostCurrentV1,
    generation: u64,
}
impl PreparedHostCurrentV1<'_> {
    /// Only moves already admitted owners. No allocation, cancellation or IO.
    pub(super) fn finish(self) {
        self.slot.current = Some(self.next);
        self.slot.generation = self.generation;
    }
}
