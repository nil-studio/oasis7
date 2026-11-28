//! Original minimum owners and their paid response copies share the caller's Core.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1;
use oasis7_proto::memory_budget::MemoryBudget;
use std::mem::size_of;

/// A short loan of the server's existing pool, continuing meter and cancellation.
pub(crate) struct RpcReadBudget<'a> {
    memory: &'a MemoryBudget,
    work: &'a FormalArchiveReadWorkV1,
    cancelled: &'a mut dyn FnMut() -> bool,
}
impl<'a> RpcReadBudget<'a> {
    pub(crate) fn new(
        memory: &'a MemoryBudget,
        work: &'a FormalArchiveReadWorkV1,
        cancelled: &'a mut dyn FnMut() -> bool,
    ) -> Self {
        Self {
            memory,
            work,
            cancelled,
        }
    }
    pub(super) fn context(&mut self) -> FormalBoundedContextV1<'_> {
        self.work.bounded_context(self.memory, self.cancelled)
    }
}

pub(super) enum MinimumSource {
    Legacy(Vec<Vec<u8>>),
    Paid(store::PaidMinimumWitness, WorkIdentityV1),
}
pub(super) struct RpcAncestry {
    // The copied DATA must drop before either original or copy reservations.
    pub(super) wire: Vec<Vec<u8>>,
    pub(super) retention: Option<AncestryRetention>,
}
pub(super) struct AncestryRetention {
    _original: store::PaidMinimumWitness,
    _copy_fee: MemoryReservation,
}

impl MinimumSource {
    /// Only the original walk belongs inside the guarded poison wrapper.
    pub(super) fn load(
        store: &Store,
        minimum: &FormalHeadRefV1,
        budget: Option<&mut RpcReadBudget<'_>>,
    ) -> Result<Self, FormalError> {
        match budget {
            Some(budget) => {
                let identity = budget.context().work().identity();
                store
                    .minimum_witness_bounded(minimum, budget.memory, budget.work, budget.cancelled)
                    .map(|original| Self::Paid(original, identity))
            }
            None => store.minimum_witness(minimum).map(Self::Legacy),
        }
    }

    /// Copy after the original command result, outside that poison wrapper.
    pub(super) fn into_wire(
        self,
        budget: Option<&mut RpcReadBudget<'_>>,
    ) -> Result<RpcAncestry, FormalError> {
        let (original, identity) = match self {
            Self::Legacy(wire) => {
                return Ok(RpcAncestry {
                    wire,
                    retention: None,
                });
            }
            Self::Paid(original, identity) => (original, identity),
        };
        let budget = budget.ok_or_else(|| invalid("formal minimum response budget"))?;
        let mut ctx = budget.context();
        ctx.consume_steps(0)?;
        if !original.uses_budget(ctx.memory()) || identity != ctx.work().identity() {
            return Err(invalid("formal minimum response budget"));
        }
        let layout_error = || invalid("formal minimum response layout");
        let headers = original
            .len()
            .checked_mul(size_of::<Vec<u8>>())
            .and_then(|n| n.checked_add(size_of::<Vec<Vec<u8>>>()))
            .ok_or_else(layout_error)?;
        ctx.charge(1, u64::try_from(headers).map_err(|_| layout_error())?)?;
        let mut allocation = headers;
        for raw in original.iter() {
            ctx.consume_steps(1)?;
            allocation = allocation
                .checked_add(raw.as_ref().len())
                .ok_or_else(layout_error)?;
        }
        ctx.consume_steps(1)?;
        let copy_fee = ctx
            .memory()
            .reserve(allocation, MemoryClass::Ordinary)
            .map_err(|_| invalid("formal minimum response admission"))?;
        // The complete wire graph is paid while every original is still live.
        ctx.consume_steps(1)?;
        let mut wire = Vec::with_capacity(original.len());
        if wire.capacity() != original.len() {
            return Err(invalid("formal minimum response capacity"));
        }
        for raw in original.iter() {
            ctx.charge(1, size_of::<Vec<u8>>() as u64)?;
            let bytes = raw.as_ref();
            let mut copy = Vec::with_capacity(bytes.len());
            if copy.capacity() != bytes.len() {
                return Err(invalid("formal minimum response capacity"));
            }
            for chunk in bytes.chunks(8192) {
                ctx.charge(1, chunk.len() as u64)?;
                copy.extend_from_slice(chunk);
            }
            wire.push(copy);
        }
        ctx.consume_steps(1)?;
        Ok(RpcAncestry {
            wire,
            retention: Some(AncestryRetention {
                _original: original,
                _copy_fee: copy_fee,
            }),
        })
    }
}

#[cfg(test)]
#[path = "endpoint_rpc_minimum_paid_tests.rs"]
mod tests;
