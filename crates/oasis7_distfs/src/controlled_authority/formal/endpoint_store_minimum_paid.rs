//! Paid DATA operations for the unchanged minimum predicates. Raw owners never detach.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::{
    self as cbor, BoundedCborDataErrorV1, CborDataLayoutV1, CborDataLimitsV1, PaidCborDataV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::mem::size_of;

pub(super) trait MinimumOps {
    type Raw: AsRef<[u8]>;
    type Node: Borrow<HistoryNode>;
    fn charge(&mut self, _: usize) -> Result<(), FormalError> {
        Ok(())
    }
    fn begin(&mut self, _: &HotBody) -> Result<(), FormalError> {
        Ok(())
    }
    fn advance(&mut self, _: &HistoryLink) -> Result<(), FormalError> {
        Ok(())
    }
    fn load(&mut self, id: &str) -> Result<Self::Raw, FormalError>;
    fn decode(&mut self, raw: &Self::Raw) -> Result<Self::Node, FormalError>;
    fn push(&mut self, out: &mut Vec<Self::Raw>, raw: Self::Raw) -> Result<(), FormalError>;
}
impl<F: FnMut(&str) -> Result<Vec<u8>, FormalError>> MinimumOps for F {
    type Raw = Vec<u8>;
    type Node = HistoryNode;
    fn load(&mut self, id: &str) -> Result<Self::Raw, FormalError> {
        self(id)
    }
    fn decode(&mut self, raw: &Self::Raw) -> Result<Self::Node, FormalError> {
        decode(raw)
    }
    fn push(&mut self, out: &mut Vec<Self::Raw>, raw: Self::Raw) -> Result<(), FormalError> {
        out.push(raw);
        Ok(())
    }
}
pub(in super::super) struct PaidRaw(super::super::FormalArchiveReadOwnerV1);
impl AsRef<[u8]> for PaidRaw {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
#[cfg(test)]
thread_local! { static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(in super::super) fn paid_minimum_read_count() -> usize {
    READS.with(std::cell::Cell::get)
}

fn layout() -> CborDataLayoutV1 {
    // Only Strings and Vec<HistoryLink>; no Box, map, set or allocating default.
    CborDataLayoutV1 {
        root: size_of::<HistoryNode>(),
        sequence_element: size_of::<HistoryLink>(),
        map_pair: 0,
        set_element: 0,
        box_pointee: 0,
        default_heap_per_value: 0,
        symbols_bytes: 0,
        visits: 1,
        missing: None,
    }
}
struct PaidOps<'a> {
    store: &'a Store,
    memory: &'a MemoryBudget,
    work: &'a super::super::FormalArchiveReadWorkV1,
    cancelled: &'a mut dyn FnMut() -> bool,
    clones: MemoryReservation,
    outer: MemoryReservation,
}
impl<'a> PaidOps<'a> {
    fn new(
        store: &'a Store,
        memory: &'a MemoryBudget,
        work: &'a super::super::FormalArchiveReadWorkV1,
        cancelled: &'a mut dyn FnMut() -> bool,
    ) -> Result<Self, FormalError> {
        work.consume_steps(0, cancelled)?;
        let zero = || {
            memory
                .reserve(0, MemoryClass::Ordinary)
                .map_err(|_| invalid("formal minimum ledger admission"))
        };
        Ok(Self {
            store,
            memory,
            work,
            cancelled,
            clones: zero()?,
            outer: zero()?,
        })
    }
    fn reserve(&mut self, bytes: usize, label: &str) -> Result<MemoryReservation, FormalError> {
        self.charge(bytes)?;
        self.memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|_| invalid(label))
    }
    fn copies(&mut self, values: [&str; 3]) -> Result<(), FormalError> {
        let headers = size_of::<FormalHeadRefV1>() + 2 * size_of::<Option<String>>();
        let bytes = values
            .into_iter()
            .try_fold(headers, |n, value| n.checked_add(value.len()))
            .ok_or_else(|| invalid("formal minimum layout"))?;
        let mut fee = self.reserve(bytes, "formal minimum clone admission")?;
        self.clones.merge(&mut fee).map_err(invalid)
    }
    fn finish(mut self, value: Vec<PaidRaw>) -> Result<ReservedValue<Vec<PaidRaw>>, FormalError> {
        if self.outer.bytes() == 0 {
            self.outer =
                self.reserve(size_of::<Vec<PaidRaw>>(), "formal minimum outer admission")?;
        }
        Ok(self.outer.retain(value))
    }
}
impl MinimumOps for PaidOps<'_> {
    type Raw = PaidRaw;
    type Node = PaidCborDataV1<HistoryNode>;
    fn charge(&mut self, bytes: usize) -> Result<(), FormalError> {
        self.work.charge(
            1,
            u64::try_from(bytes).map_err(|_| invalid("formal minimum layout"))?,
            self.cancelled,
        )
    }
    fn begin(&mut self, body: &HotBody) -> Result<(), FormalError> {
        // Both Option clones are eager, including the one discarded by `or`.
        self.copies([
            &body.head.record_id,
            body.head_node.as_deref().unwrap_or(""),
            body.imported_head_node.as_deref().unwrap_or(""),
        ])
    }
    fn advance(&mut self, link: &HistoryLink) -> Result<(), FormalError> {
        self.copies([&link.head.record_id, &link.node, ""])
    }
    fn load(&mut self, id: &str) -> Result<PaidRaw, FormalError> {
        self.charge(0)?;
        let raw = self
            .store
            .raw_object_bounded(id, 128 * 1024, self.memory, self.work, self.cancelled)?
            .ok_or_else(|| invalid("formal ancestry object missing"))?;
        #[cfg(test)]
        READS.with(|n| n.set(n.get().saturating_add(1)));
        Ok(PaidRaw(raw))
    }
    fn decode(&mut self, raw: &PaidRaw) -> Result<Self::Node, FormalError> {
        let limits = CborDataLimitsV1 {
            max_wire_bytes: 128 * 1024,
            max_graph_bytes: usize::MAX,
            max_depth: 128,
        };
        let decoded = cbor::decode_closed_cbor_data_v1(
            raw.as_ref(),
            layout(),
            &limits,
            &mut self.work.bounded_context(self.memory, self.cancelled),
        )
        .map_err(|e| match e {
            BoundedCborDataErrorV1::Common(e) => e,
            BoundedCborDataErrorV1::Memory(_) => invalid("formal minimum decoder admission"),
            other => invalid(other.to_string()),
        })?;
        match decoded {
            Ok(node) => Ok(node),
            Err(error) => {
                let bytes = error
                    .message()
                    .len()
                    .checked_add(size_of::<String>())
                    .ok_or_else(|| invalid("formal minimum layout"))?;
                let _copy = self.reserve(bytes, "formal minimum diagnostic admission")?;
                // Preserve the legacy diagnostic. Its escaping String has no retained fee.
                Err(invalid(error.message().to_owned()))
            }
        }
    }
    fn push(&mut self, out: &mut Vec<PaidRaw>, raw: PaidRaw) -> Result<(), FormalError> {
        self.charge(size_of::<PaidRaw>())?;
        if out.len() == out.capacity() {
            let next = out
                .capacity()
                .checked_mul(2)
                .map(|n| n.clamp(1, 64))
                .filter(|n| *n > out.len())
                .ok_or_else(|| invalid("formal minimum layout"))?;
            let old = out
                .capacity()
                .checked_mul(size_of::<PaidRaw>())
                .ok_or_else(|| invalid("formal minimum layout"))?;
            let bytes = next
                .checked_mul(size_of::<PaidRaw>())
                .and_then(|n| {
                    n.checked_add(if self.outer.bytes() == 0 {
                        size_of::<Vec<PaidRaw>>()
                    } else {
                        0
                    })
                })
                .ok_or_else(|| invalid("formal minimum layout"))?;
            let mut fee = self.reserve(bytes, "formal minimum outer admission")?;
            self.charge(old)?;
            let mut grown = Vec::with_capacity(next);
            if grown.capacity() != next {
                return Err(invalid("formal minimum outer capacity"));
            }
            grown.append(out);
            *out = grown;
            // Both buffers were paid through their overlap; only now release the old slots.
            self.outer.merge(&mut fee).map_err(invalid)?;
            drop(self.outer.split(old).map_err(invalid)?);
        }
        out.push(raw);
        Ok(())
    }
}
impl Store {
    pub(in super::super) fn minimum_witness_bounded(
        &self,
        minimum: &FormalHeadRefV1,
        memory: &MemoryBudget,
        work: &super::super::FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<ReservedValue<Vec<PaidRaw>>, FormalError> {
        // Fees precede the kernel's value: every early return drops DATA first.
        let mut ops = PaidOps::new(self, memory, work, cancelled)?;
        let value = Self::walk_minimum(&self.body, minimum, &mut ops)?;
        ops.finish(value)
    }
}
#[cfg(test)]
#[path = "endpoint_store_minimum_paid_tests.rs"]
mod tests;
