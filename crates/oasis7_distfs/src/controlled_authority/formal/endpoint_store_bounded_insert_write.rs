//! Complete retained Index rewrite under one real storage guard; never Hot publication.
use super::super::FormalArchiveReadWorkV1;
use super::bounded_index::copy_paid;
use super::bounded_insert_plan::{IndexInsertPlanV1, InsertTerminalRef, consume_for_write};
use super::*;
use crate::controlled_authority::formal::physical;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub(in crate::controlled_authority::formal::endpoint) struct RetainedIndexRewriteV1 {
    root: ReservedValue<String>,
    before: Option<ReservedValue<String>>,
    _directory: ReservedValue<PathBuf>,
    _guard: ReservedValue<physical::PublicationGuard>,
}
impl std::fmt::Debug for RetainedIndexRewriteV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetainedIndexRewriteV1")
            .field("root", &self.root.as_str())
            .finish_non_exhaustive()
    }
}
impl RetainedIndexRewriteV1 {
    pub(in crate::controlled_authority::formal::endpoint) fn id(&self) -> &str {
        &self.root
    }
    pub(in crate::controlled_authority::formal::endpoint) fn before_index(&self) -> Option<&str> {
        self.before.as_ref().map(|s| s.as_str())
    }
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.root.uses_budget(memory)
            && self.before.as_ref().is_none_or(|s| s.uses_budget(memory))
            && self._directory.uses_budget(memory)
            && self._guard.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.root.reserved_bytes()
            + self.before.as_ref().map_or(0, |s| s.reserved_bytes())
            + self._directory.reserved_bytes()
            + self._guard.reserved_bytes()
    }
}
fn copied_text(text: &str) -> Result<String, FormalError> {
    let mut result = String::with_capacity(text.len());
    if result.capacity() > text.len() {
        return Err(invalid("bounded Index rewrite text capacity"));
    }
    result.push_str(text);
    Ok(result)
}
fn paid_node(
    leaf: bool,
    bit: u16,
    left: &str,
    right: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<Index>, FormalError> {
    let n = left
        .len()
        .checked_add(right.len())
        .and_then(|n| n.checked_add(std::mem::size_of::<Index>()))
        .ok_or_else(|| invalid("bounded Index rewrite node layout"))?;
    work.consume_read_bytes(
        u64::try_from(left.len()).map_err(|_| invalid("bounded Index rewrite length"))?,
        stop,
    )?;
    work.consume_read_bytes(
        u64::try_from(right.len()).map_err(|_| invalid("bounded Index rewrite length"))?,
        stop,
    )?;
    let fee = memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded Index rewrite node admission"))?;
    let left = copied_text(left)?;
    let right = copied_text(right)?;
    Ok(fee.retain(if leaf {
        Index::Leaf {
            request: left,
            entry: right,
        }
    } else {
        Index::Fork { bit, left, right }
    }))
}
fn stage_and_retain(
    guard: &ReservedValue<physical::PublicationGuard>,
    node: &Index,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<String>, FormalError> {
    let staged = super::stage_io::stage_index_node_bounded_v1(guard, node, memory, work, stop)?;
    let receipt = physical::roots::append_staged_node_bounded_v1(&staged, memory, work, stop)?;
    // Only a genuinely durable and retained node may become the next child.
    // Both source owners remain alive while the new digest copy is prepaid.
    let next = copy_paid(staged.id(), memory, work, stop)?;
    drop(receipt);
    drop(staged);
    Ok(next)
}
pub(super) fn write_insert_plan_bounded_v1<'a>(
    store: &'a Store,
    plan: IndexInsertPlanV1<'a>,
    memory: &MemoryBudget,
    work: &'a FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<RetainedIndexRewriteV1, FormalError> {
    let parts = consume_for_write(plan, store, memory, work, stop)?;
    let before = parts
        .before_index()
        .map(|s| copy_paid(s, memory, work, stop))
        .transpose()?;
    work.consume_steps(1, stop)?;
    let cap = store
        .path
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(1 + "formal-objects-v1".len())
        .ok_or_else(|| invalid("bounded Index rewrite directory layout"))?;
    let fee = memory
        .reserve(
            cap.checked_add(std::mem::size_of::<PathBuf>())
                .ok_or_else(|| invalid("bounded Index rewrite directory layout"))?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("bounded Index rewrite directory admission"))?;
    let mut directory = PathBuf::with_capacity(cap);
    if directory.capacity() > cap {
        return Err(invalid("bounded Index rewrite directory capacity"));
    }
    directory.push(&store.path);
    directory.push("formal-objects-v1");
    if directory.capacity() > cap {
        return Err(invalid("bounded Index rewrite directory growth"));
    }
    let directory = fee.retain(directory);
    let guard = super::stage_io::guard_initialized(&directory, memory, work, stop)?;
    let leaf = paid_node(true, 0, parts.request(), parts.entry(), memory, work, stop)?;
    let mut current = stage_and_retain(&guard, &leaf, memory, work, stop)?;
    drop(leaf);
    if let InsertTerminalRef::Splice { bit, old_subtree } = parts.terminal() {
        if bit >= 256 {
            return Err(invalid("bounded Index rewrite splice bit"));
        }
        work.consume_read_bytes(parts.request().len() as u64, stop)?;
        let mut request = [0; 32];
        hex::decode_to_slice(parts.request(), &mut request).map_err(io)?;
        let right = request[usize::from(bit / 8)] & (0x80 >> (bit % 8)) != 0;
        let (left, right) = if right {
            (old_subtree, current.as_str())
        } else {
            (current.as_str(), old_subtree)
        };
        let fork = paid_node(false, bit, left, right, memory, work, stop)?;
        current = stage_and_retain(&guard, &fork, memory, work, stop)?;
        drop(fork);
    }
    for index in (0..parts.ancestor_count()).rev() {
        work.consume_steps(1, stop)?;
        let (ancestor, rightward) = parts.ancestor_at(index)?;
        let Index::Fork { bit, left, right } = ancestor else {
            return Err(invalid("bounded Index rewrite ancestor type"));
        };
        let (left, right) = if rightward {
            (left.as_str(), current.as_str())
        } else {
            (current.as_str(), right.as_str())
        };
        let fork = paid_node(false, *bit, left, right, memory, work, stop)?;
        current = stage_and_retain(&guard, &fork, memory, work, stop)?;
        drop(fork);
    }
    parts.validate_current(store, memory, work, stop)?;
    // Returning owned data releases the Plan's Store/Work borrow. The sole real
    // guard stays live; a future Hot publisher must independently validate it.
    drop(parts);
    Ok(RetainedIndexRewriteV1 {
        root: current,
        before,
        _directory: directory,
        _guard: guard,
    })
}
#[cfg(test)]
#[path = "endpoint_store_bounded_insert_write_tests.rs"]
mod tests;
