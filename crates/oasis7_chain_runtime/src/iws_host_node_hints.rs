//! Retain untrusted cold-read locators from the actual accepted fact set.
//! A location never replaces the cold reader's original-history verification.
use super::*;
use oasis7_formal_protocol::incremental_execution::ExecutionFactKindV1;
use oasis7_formal_protocol::incremental_material::FactLocationV1;
use oasis7_proto::memory_budget::MemoryReservation;
use std::collections::BTreeMap;

// The pinned Rust 1.96 BTree has B=6, eleven key/value slots and twelve
// internal edges. As in formal/proof_normal_bounded.rs, reserve a whole
// internal node per entry. Three extra nodes cover root/split scratch; their
// reservation remains with the map even when the map is initially empty.
const HINT_NODE_BYTES: usize =
    11 * size_of::<([u8; 32], FactLocationV1)>() + 14 * size_of::<usize>() + 128;

pub(crate) fn mailbox_hint_heap_bytes_v1(entries: usize) -> Result<usize, FormalError> {
    entries
        .checked_add(3)
        .and_then(|count| count.checked_mul(HINT_NODE_BYTES))
        .ok_or_else(|| FormalError::Invalid("live Node mailbox hint layout overflow".into()))
}

/// Update only keys present in this accepted commit and retain all older keys.
/// The original FactSet ordinal includes Input and Result entries; filtering
/// Event facts never renumbers those positions. Retry after a partial DATA
/// update is idempotent and retains both the map and its original reservation.
pub(crate) fn retain_current_mailbox_hints_bounded_v1(
    primary: &IwsHostRuntimeV1,
    hints: &mut BTreeMap<[u8; 32], FactLocationV1>,
    fee: &mut MemoryReservation,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<(), IwsHostRuntimeErrorV1> {
    let mut stop = || cancelled.load(Ordering::Acquire);
    archive.consume_steps(1, &mut stop)?;
    if !fee.uses_budget(memory) || fee.bytes() < mailbox_hint_heap_bytes_v1(hints.len())? {
        return Err(FormalError::Invalid("live Node mailbox hint pool/fee".into()).into());
    }
    let current = primary
        .current
        .current()
        .ok_or_else(|| FormalError::Invalid("live Node accepted mailbox source missing".into()))?;
    current
        .source
        .with_parent_read_view_bounded(memory, archive, &mut stop, |parent| {
            let height = parent.core().body().height;
            for (ordinal, fact) in parent.core().facts().facts.iter().enumerate() {
                archive.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
                if fact.kind != ExecutionFactKindV1::Event {
                    continue;
                }
                let ordinal = u64::try_from(ordinal).map_err(|_| {
                    FormalError::Invalid("live Node mailbox fact ordinal overflow".into())
                })?;
                let location = FactLocationV1 {
                    height,
                    ordinal,
                    original: fact.id,
                };
                // At most twelve comparisons per BTree level; its depth is
                // bounded by the machine-sized entry count. Pay before lookup
                // and insertion, independently of the allocation reservation.
                let steps = 2 * 12 * usize::BITS as u64 + 1;
                archive.charge(steps, steps * 96, &mut || cancelled.load(Ordering::Acquire))?;
                match hints.get(&fact.id) {
                    Some(previous) if previous.original != fact.id => {
                        return Err(FormalError::Invalid(
                            "live Node mailbox hint key differs from original".into(),
                        )
                        .into());
                    }
                    Some(previous) if *previous == location => continue,
                    Some(_) => {
                        // Identical original content may have another proved
                        // occurrence. Replace this DATA locator with the actual
                        // current occurrence; all other historical keys remain.
                    }
                    None => {
                        hints.len().checked_add(1).ok_or_else(|| {
                            FormalError::Invalid("live Node mailbox hint count overflow".into())
                        })?;
                        let mut next = memory
                            .reserve(HINT_NODE_BYTES, MemoryClass::Ordinary)
                            .map_err(|error| IwsHostRuntimeErrorV1::Move(error.into()))?;
                        fee.merge(&mut next)
                            .map_err(|error| FormalError::Invalid(error.into()))?;
                    }
                }
                hints.insert(fact.id, location);
            }
            archive.consume_steps(0, &mut || cancelled.load(Ordering::Acquire))?;
            Ok(())
        })
}
