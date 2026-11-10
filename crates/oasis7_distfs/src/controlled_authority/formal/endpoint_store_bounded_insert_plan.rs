//! Paid current-Index insertion recipe; no writer, retention or publication capability.
use super::super::FormalArchiveReadWorkV1;
use super::bounded_index::{copy_paid, decode_index_bounded_v1};
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

struct Ancestor {
    node: ReservedValue<Index>,
    right: bool,
}
enum Terminal {
    Empty,
    Replace,
    Splice {
        bit: u16,
        old_subtree: ReservedValue<String>,
    },
}
pub(in crate::controlled_authority::formal::endpoint) struct IndexInsertPlanV1<'a> {
    _store: &'a Store,
    _work: &'a FormalArchiveReadWorkV1,
    before: Option<ReservedValue<String>>,
    request: ReservedValue<String>,
    entry: ReservedValue<String>,
    ancestors: ReservedValue<Vec<Ancestor>>,
    terminal: Terminal,
}
impl std::fmt::Debug for IndexInsertPlanV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexInsertPlanV1")
            .field("ancestors", &self.ancestors.len())
            .finish_non_exhaustive()
    }
}
impl IndexInsertPlanV1<'_> {
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.before.as_ref().is_none_or(|s| s.uses_budget(memory))
            && self.request.uses_budget(memory)
            && self.entry.uses_budget(memory)
            && self.ancestors.uses_budget(memory)
            && self.ancestors.iter().all(|a| a.node.uses_budget(memory))
            && match &self.terminal {
                Terminal::Splice { old_subtree, .. } => old_subtree.uses_budget(memory),
                _ => true,
            }
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.before.as_ref().map_or(0, |s| s.reserved_bytes())
            + self.request.reserved_bytes()
            + self.entry.reserved_bytes()
            + self.ancestors.reserved_bytes()
            + self
                .ancestors
                .iter()
                .map(|a| a.node.reserved_bytes())
                .sum::<usize>()
            + match &self.terminal {
                Terminal::Splice { old_subtree, .. } => old_subtree.reserved_bytes(),
                _ => 0,
            }
    }
}
fn read_node(
    store: &Store,
    id: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<Index>, FormalError> {
    work.consume_steps(1, stop)?;
    let raw = store
        .raw_object_bounded(id, LIMIT, memory, work, stop)?
        .ok_or_else(|| invalid("referenced formal archive object missing"))?;
    decode_index_bounded_v1(&raw, memory, work, stop)
}
fn key_bit(
    key: &str,
    bit: u16,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<bool, FormalError> {
    work.consume_read_bytes(key.len() as u64, stop)?;
    if bit >= 256 {
        return Err(invalid("formal index bit out of range"));
    }
    // Only the admitted request reaches this helper: exactly 64 hexadecimal
    // bytes. Stack decoding avoids the old unchecked heap-vector index.
    let mut bytes = [0; 32];
    hex::decode_to_slice(key, &mut bytes).map_err(io)?;
    Ok(bytes[usize::from(bit / 8)] & (0x80 >> (bit % 8)) != 0)
}
fn differing(
    request: &str,
    existing: &str,
    _memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<Option<u16>, FormalError> {
    work.consume_read_bytes(existing.len() as u64, stop)?;
    if existing.len() % 2 != 0 {
        return Ok(Some(0));
    }
    let mut decoded = [0u8; 32];
    // Reuse the original hex codec on every pair, including the entire tail.
    // No Vec collect/minimum/geometric capacity is allocated before admission.
    for (index, pair) in existing.as_bytes().chunks_exact(2).enumerate() {
        work.consume_steps(1, stop)?;
        let mut byte = [0u8; 1];
        if hex::decode_to_slice(pair, &mut byte).is_err() {
            return Ok(Some(0));
        }
        if index < 32 {
            decoded[index] = byte[0];
        }
    }
    if existing.len() / 2 < 32 {
        return Err(invalid("formal index existing key shorter than 256 bits"));
    }
    let mut request_bytes = [0; 32];
    hex::decode_to_slice(request, &mut request_bytes).map_err(io)?;
    let mut result = None;
    for bit in 0..256u16 {
        work.consume_steps(1, stop)?;
        let mask = 0x80 >> (bit % 8);
        let i = usize::from(bit / 8);
        if request_bytes[i] & mask != decoded[i] & mask {
            result = Some(bit);
            break;
        }
    }
    Ok(result)
}
pub(super) fn prepare_insert_plan_bounded_v1<'a>(
    store: &'a Store,
    request: &str,
    entry: &str,
    memory: &MemoryBudget,
    work: &'a FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<IndexInsertPlanV1<'a>, FormalError> {
    work.consume_steps(1, stop)?;
    hash_hex(request)?;
    hash_hex(entry)?;
    let request = copy_paid(request, memory, work, stop)?;
    let entry = copy_paid(entry, memory, work, stop)?;
    let before = store
        .body
        .index
        .as_deref()
        .map(|s| copy_paid(s, memory, work, stop))
        .transpose()?;
    let slots: usize = if before.is_some() { 256 } else { 0 };
    let layout = slots
        .checked_mul(std::mem::size_of::<Ancestor>())
        .and_then(|n| n.checked_add(std::mem::size_of::<Vec<Ancestor>>()))
        .ok_or_else(|| invalid("bounded insertion path layout"))?;
    let fee = memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded insertion path admission"))?;
    let mut frames = Vec::with_capacity(slots);
    if frames.capacity() > slots {
        return Err(invalid("bounded insertion path capacity"));
    }
    let terminal = if let Some(root) = &before {
        let mut cursor = copy_paid(root, memory, work, stop)?;
        let mut previous = None;
        let existing = loop {
            let node = read_node(store, &cursor, memory, work, stop)?;
            match &*node {
                Index::Leaf { request, .. } => break copy_paid(request, memory, work, stop)?,
                Index::Fork { bit, left, right } => {
                    if *bit >= 256 || previous.is_some_and(|p| *bit <= p) {
                        return Err(invalid("formal index bit order"));
                    }
                    previous = Some(*bit);
                    let next = if key_bit(&request, *bit, work, stop)? {
                        right
                    } else {
                        left
                    };
                    cursor = copy_paid(next, memory, work, stop)?;
                }
            }
        };
        let differing = differing(&request, &existing, memory, work, stop)?;
        drop(existing);
        drop(cursor);
        let mut cursor = copy_paid(root, memory, work, stop)?;
        loop {
            if frames.len() > 256 {
                return Err(invalid("formal index depth"));
            }
            let node = read_node(store, &cursor, memory, work, stop)?;
            if let Some(bit) = differing {
                let splice = match &*node {
                    Index::Leaf { .. } => true,
                    Index::Fork { bit: next, .. } => bit < *next,
                };
                if splice {
                    break Terminal::Splice {
                        bit,
                        old_subtree: cursor,
                    };
                }
            }
            match &*node {
                Index::Leaf { request: key, .. } => {
                    work.consume_read_bytes(key.len() as u64, stop)?;
                    if key == &*request {
                        break Terminal::Replace;
                    } else {
                        return Err(invalid("formal index insertion"));
                    }
                }
                Index::Fork { bit, left, right } => {
                    if frames.len() == 256 {
                        return Err(invalid("formal index depth"));
                    }
                    let rightward = key_bit(&request, *bit, work, stop)?;
                    let next = copy_paid(if rightward { right } else { left }, memory, work, stop)?;
                    frames.push(Ancestor {
                        node,
                        right: rightward,
                    });
                    cursor = next;
                }
            }
        }
    } else {
        Terminal::Empty
    };
    work.consume_steps(1, stop)?;
    Ok(IndexInsertPlanV1 {
        _store: store,
        _work: work,
        before,
        request,
        entry,
        ancestors: fee.retain(frames),
        terminal,
    })
}
#[cfg(test)]
#[path = "endpoint_store_bounded_insert_plan_tests.rs"]
mod tests;

// A consuming data bridge only. Its private constructor does not mint a
// storage guard or native publication authority.
pub(super) struct ValidatedInsertPlanV1<'a> {
    plan: IndexInsertPlanV1<'a>,
}
pub(super) enum InsertTerminalRef<'a> {
    Empty,
    Replace,
    Splice { bit: u16, old_subtree: &'a str },
}
pub(super) fn consume_for_write<'a>(
    plan: IndexInsertPlanV1<'a>,
    store: &'a Store,
    memory: &MemoryBudget,
    work: &'a FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ValidatedInsertPlanV1<'a>, FormalError> {
    let validated = ValidatedInsertPlanV1 { plan };
    validated.validate_current(store, memory, work, stop)?;
    Ok(validated)
}
impl ValidatedInsertPlanV1<'_> {
    pub(super) fn request(&self) -> &str {
        &self.plan.request
    }
    pub(super) fn entry(&self) -> &str {
        &self.plan.entry
    }
    pub(super) fn before_index(&self) -> Option<&str> {
        self.plan.before.as_ref().map(|s| s.as_str())
    }
    pub(super) fn ancestor_count(&self) -> usize {
        self.plan.ancestors.len()
    }
    pub(super) fn ancestor_at(&self, index: usize) -> Result<(&Index, bool), FormalError> {
        self.plan
            .ancestors
            .get(index)
            .map(|a| (&*a.node, a.right))
            .ok_or_else(|| invalid("bounded insertion ancestor index"))
    }
    pub(super) fn terminal(&self) -> InsertTerminalRef<'_> {
        match &self.plan.terminal {
            Terminal::Empty => InsertTerminalRef::Empty,
            Terminal::Replace => InsertTerminalRef::Replace,
            Terminal::Splice { bit, old_subtree } => InsertTerminalRef::Splice {
                bit: *bit,
                old_subtree,
            },
        }
    }
    pub(super) fn validate_current(
        &self,
        store: &Store,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        work.consume_steps(1, stop)?;
        if !std::ptr::eq(self.plan._store, store) {
            return Err(invalid("bounded insertion foreign Store"));
        }
        if !std::ptr::eq(self.plan._work, work) {
            return Err(invalid("bounded insertion foreign Work"));
        }
        // All observations are finite, and even the empty path pays one step.
        for _ in 0..self.plan.ancestors.len() {
            work.consume_steps(1, stop)?;
        }
        if !self.plan.uses_budget(memory) {
            return Err(invalid("bounded insertion foreign pool"));
        }
        let current = store.body.index.as_deref();
        let before = self.before_index();
        let n = current
            .map_or(0, str::len)
            .checked_add(before.map_or(0, str::len))
            .ok_or_else(|| invalid("bounded insertion root comparison layout"))?;
        work.consume_read_bytes(n as u64, stop)?;
        if current != before {
            return Err(invalid("bounded insertion current root changed"));
        }
        Ok(())
    }
}
