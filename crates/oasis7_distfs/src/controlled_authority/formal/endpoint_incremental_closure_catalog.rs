//! Synchronize only the true Core's already verified catalog append pages.
//! This is not a catalog full scan, selected head, or complete root receipt.
use super::*;
use crate::incremental_page_store::{IncrementalPageStore, PageIoBudget};
use std::sync::atomic::AtomicBool;
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_checked_catalog_delta_v1(
    core: &ReservedValue<CheckedCommitCore>,
    pages: &IncrementalPageStore,
    read_io: &mut PageIoBudget,
    publish_io: &mut PageIoBudget,
    occurrences: &mut u64,
    physical_cancelled: &AtomicBool,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormalError> {
    if cancelled() {
        return Err(invalid("catalog synchronization cancelled"));
    }
    let memory = pages.memory_budget();
    if !core.uses_budget(memory) || core.reserved_bytes() < std::mem::size_of::<CheckedCommitCore>()
    {
        return Err(invalid("catalog synchronization Core pool"));
    }
    let _phase = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(|_| invalid("catalog synchronization admission"))?;
    for node in &core.catalog().new_nodes {
        if cancelled() {
            return Err(invalid("catalog synchronization cancelled"));
        }
        *occurrences = occurrences
            .checked_sub(1)
            .ok_or_else(|| invalid("catalog synchronization occurrences"))?;
        let mut id = [0; 32];
        hex::decode_to_slice(&node.digest, &mut id)
            .map_err(|_| invalid("catalog synchronization actual node ID"))?;
        let actual = pages
            .read_page(
                &id,
                node.bytes.len(),
                MemoryClass::Ordinary,
                read_io,
                physical_cancelled,
            )
            .map_err(page_store_formal_error)?;
        if actual.as_slice() != node.bytes.as_slice() {
            return Err(invalid("catalog synchronization verified bytes"));
        }
        pages
            .put_page(id, &actual, publish_io, physical_cancelled)
            .map_err(page_store_formal_error)?;
        if cancelled() {
            return Err(invalid("catalog synchronization final node cancellation"));
        }
    }
    if cancelled() {
        return Err(invalid("catalog synchronization final cancellation"));
    }
    Ok(())
}

/// Every reachable new after-state page lies on a final written-key path.
/// Walking deletion paths as well covers absence proofs and collapsed branches.
/// The genuine Core has already verified the complete transition rebuild;
/// this adds actual StoreSessions read/sync provenance without scanning S.
pub(crate) fn sync_checked_transition_paths_v1<R: MerkleNodeReader>(
    core: &ReservedValue<CheckedCommitCore>,
    reader: &R,
    memory: &MemoryBudget,
    scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
    occurrences: &mut u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormalError> {
    if cancelled() {
        return Err(invalid("transition path synchronization cancelled"));
    }
    if !core.uses_budget(memory) || core.reserved_bytes() < std::mem::size_of::<CheckedCommitCore>()
    {
        return Err(invalid("transition path synchronization Core pool"));
    }
    let Some(transition) = core.transition() else {
        if core.parent_record_id().is_some() {
            return Err(invalid("transition path missing ordinary record"));
        }
        return Ok(());
    };
    let mut ctx = oasis7_formal_protocol::incremental_state::StateScanContext {
        source: reader,
        memory,
        class: MemoryClass::Ordinary,
        io: scan,
    };
    ctx.with_reader(&mut *cancelled, |guard| {
        for write in &transition.writes {
            *occurrences = occurrences
                .checked_sub(1)
                .ok_or_else(|| invalid("transition path occurrences"))?;
            let root = core.roots().state_root(write.namespace);
            let actual = guard
                .get(root, &write.mutation.key)
                .map_err(|_| invalid("transition actual after path"))?;
            if actual != write.mutation.after {
                return Err(invalid("transition actual after value"));
            }
        }
        Ok::<_, FormalError>(())
    })
    .map_err(|_| invalid("transition bounded after path synchronization"))?;
    if cancelled() {
        return Err(invalid(
            "transition path synchronization final cancellation",
        ));
    }
    Ok(())
}
