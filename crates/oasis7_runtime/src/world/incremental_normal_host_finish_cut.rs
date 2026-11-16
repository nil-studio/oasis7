//! Streaming actual net-write/fact cut. Read-set revisions are not source
//! refreshes: only unchanged original net writes and ordered facts can rejoin.
use super::{BodyMutationError, NormalHostFinishCutWorkV1};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionFence, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::MemoryClass;
use std::cell::RefCell;
#[derive(PartialEq, Eq)]
pub(in crate::world) struct NormalFinishWriteFactCutV1 {
    digest: [u8; 32],
    facts: usize,
}
impl NormalFinishWriteFactCutV1 {
    pub(super) fn fact_count(&self) -> usize {
        self.facts
    }
}
fn put(
    hash: &mut blake3::Hasher,
    bytes: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    for chunk in bytes.chunks(4096) {
        common
            .charge(1, chunk.len() as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        hash.update(chunk);
    }
    Ok(())
}
fn option(
    hash: &mut blake3::Hasher,
    value: Option<[u8; 32]>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    match value {
        None => put(hash, &[0], common),
        Some(value) => {
            put(hash, &[1], common)?;
            put(hash, &value, common)
        }
    }
}
pub(in crate::world) fn capture<M: MerkleNodeReader>(
    tx: &ExecutionTransaction<'_, M>,
    work: &mut NormalHostFinishCutWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
) -> Result<NormalFinishWriteFactCutV1, BodyMutationError> {
    tx.validate_guard(&tx.guard(), &live_owner())?;
    common
        .charge(1, std::mem::size_of::<blake3::Hasher>() as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let _scratch = common
        .memory()
        .reserve(
            std::mem::size_of::<blake3::Hasher>() + 256,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut hash = blake3::Hasher::new();
    put(
        &mut hash,
        b"oasis7.normal-host.directory-freeze.write-fact-cut.v1\0",
        common,
    )?;
    let common = RefCell::new(common);
    let live = RefCell::new(live_owner);
    for namespace in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Allocator,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Effects,
        StateNamespaceV1::Artifacts,
    ] {
        put(&mut hash, &[namespace as u8], &mut common.borrow_mut())?;
        tx.visit_pending_writes::<BodyMutationError>(
            namespace,
            &mut work.pending_entries_remaining,
            || {
                // The visitor invokes this for every inspected read entry too. The explicit
                // quota and Common debit therefore both count unchanged-read traversal.
                common
                    .borrow_mut()
                    .consume_steps(1)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                tx.validate_guard(&tx.guard(), &(live.borrow_mut())())?;
                Ok(())
            },
            |key, before, after| {
                let mut common = common.borrow_mut();
                put(&mut hash, &[1], &mut common)?;
                let len = u64::try_from(key.len())
                    .map_err(|_| ValueTreeError::ResourceLimited("normal cut key bytes"))?;
                put(&mut hash, &len.to_be_bytes(), &mut common)?;
                put(&mut hash, key, &mut common)?;
                option(&mut hash, before, &mut common)?;
                option(&mut hash, after, &mut common)
            },
        )?;
        put(&mut hash, &[0], &mut common.borrow_mut())?;
    }
    let facts = tx.ordered_fact_count();
    let count =
        u64::try_from(facts).map_err(|_| ValueTreeError::ResourceLimited("normal cut facts"))?;
    put(&mut hash, &count.to_be_bytes(), &mut common.borrow_mut())?;
    for position in 0..facts {
        tx.validate_guard(&tx.guard(), &(live.borrow_mut())())?;
        let fact = tx
            .ordered_fact(position)
            .ok_or(BodyMutationError::Invalid("normal cut fact position"))?;
        let mut common = common.borrow_mut();
        put(&mut hash, &[fact.kind as u8], &mut common)?;
        put(&mut hash, &fact.id, &mut common)?;
    }
    common
        .borrow_mut()
        .charge(1, 32)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    tx.validate_guard(&tx.guard(), &(live.borrow_mut())())?;
    Ok(NormalFinishWriteFactCutV1 {
        digest: *hash.finalize().as_bytes(),
        facts,
    })
}
