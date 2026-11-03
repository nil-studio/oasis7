//! Charged state-page access for organization inner point updates.
use super::*;
use oasis7_formal_protocol::incremental_value::ValueIoBudget;
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::{cell::RefCell, sync::Arc};
pub(crate) struct Page(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Page {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
pub(crate) struct Source<'a, R, C> {
    pub source: &'a R,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub io: RefCell<&'a mut ValueIoBudget>,
    pub cancelled: RefCell<&'a mut C>,
    pub error: RefCell<Option<ValueTreeError>>,
}
impl<R: ValueNodeReader, C: FnMut() -> bool> Source<'_, R, C> {
    pub fn finish<T>(&self, result: Result<T, String>) -> Result<T, ValueTreeError> {
        if (self.cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = self.error.borrow_mut().take() {
            return Err(error);
        }
        result.map_err(|message| {
            if message.starts_with("ResourceLimited:") {
                ValueTreeError::ResourceLimited("organization inner tree workspace")
            } else {
                ValueTreeError::Invalid("organization inner tree")
            }
        })
    }
    fn read(&self, id: &str, max: usize) -> Result<Page, ValueTreeError> {
        if (self.cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if id.len() != 64 {
            return Err(ValueTreeError::Invalid("organization state digest"));
        }
        let mut digest = [0; 32];
        for (i, pair) in id.as_bytes().chunks_exact(2).enumerate() {
            let digit = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                _ => None,
            };
            digest[i] =
                (digit(pair[0]).ok_or(ValueTreeError::Invalid("organization state digest"))? << 4)
                    | digit(pair[1]).ok_or(ValueTreeError::Invalid("organization state digest"))?;
        }
        let mut io = self.io.borrow_mut();
        if io.nodes_remaining == 0 || io.read_bytes_remaining < max as u64 {
            return Err(ValueTreeError::ResourceLimited(
                "organization inner tree read allowance",
            ));
        }
        io.nodes_remaining -= 1;
        io.read_bytes_remaining -= max as u64;
        let page = self
            .source
            .read_node(&digest, max, self.memory, self.class)?;
        if !page.uses_budget(self.memory) || page.reserved_bytes() < page.capacity() {
            return Err(ValueTreeError::Invalid(
                "organization state page allocation",
            ));
        }
        if page.len() > max {
            return Err(ValueTreeError::Invalid("organization state page bound"));
        }
        io.read_bytes_remaining += (max - page.len()) as u64;
        if (self.cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(Page(page))
    }
}
impl<R: ValueNodeReader, C: FnMut() -> bool> MerkleNodeReader for Source<'_, R, C> {
    type Node = Page;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Page>, String> {
        match self.read(id, max) {
            Ok(page) => Ok(Some(page)),
            Err(error) => {
                let mut first = self.error.borrow_mut();
                if first.is_none() {
                    *first = Some(error);
                }
                Err("organization state source failed".into())
            }
        }
    }
}
pub(crate) fn persist<
    W: oasis7_formal_protocol::incremental_state::StatePageWriter,
    C: FnMut() -> bool,
>(
    delta: &ReservedValue<oasis7_formal_protocol::incremental_state::StateDeltaV1>,
    write: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<(), ValueTreeError> {
    if !delta.uses_budget(write.memory) {
        return Err(ValueTreeError::Invalid("organization inner producer pool"));
    }
    // Logical work is still charged once per canonical page, independently of
    // physical packing. Exhaustion/cancellation never publishes an outer ref.
    for page in &delta.new_nodes {
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        if write.io.nodes_remaining == 0 || page.bytes.len() as u64 > write.io.write_bytes_remaining
        {
            return Err(ValueTreeError::ResourceLimited(
                "organization inner page allowance",
            ));
        }
        write.io.nodes_remaining -= 1;
        write.io.write_bytes_remaining -= page.bytes.len() as u64;
    }
    write
        .sink
        .put_state_pages_cancelled(delta, write.memory, &mut write.cancelled)?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}

/// Match the formal transition producer's conservative per-write admission:
/// encoded pending bytes alone omit Vec capacity, digest strings, pending/final
/// maps and tracked parent paths. The separate pending-byte work quota is not a
/// heap bound and remains unchanged at StateDeltaV1::build.
pub(crate) fn workspace(
    memory: &MemoryBudget,
    writes: usize,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, ValueTreeError> {
    let bytes = writes
        .checked_mul(512 << 10)
        .and_then(|n| n.checked_add(128 << 10))
        .ok_or(ValueTreeError::ResourceLimited(
            "organization inner workspace overflow",
        ))?;
    memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
}

pub(crate) fn lookup<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    key: &WorldFieldKeyV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
) -> Result<Option<[u8; 32]>, ValueTreeError> {
    let _point = records
        .values
        .memory
        .reserve(8 << 10, records.values.class)?;
    let source = Source {
        source: records.values.source,
        memory: records.values.memory,
        class: records.values.class,
        io: RefCell::new(&mut *records.values.io),
        cancelled: RefCell::new(&mut *records.cancelled),
        error: RefCell::new(None),
    };
    source.finish(root.get(key.as_bytes(), &source))
}
pub(crate) fn build<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    writes: Vec<oasis7_formal_protocol::incremental_state::StateWriteV1>,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    max_pending: usize,
) -> Result<ReservedValue<oasis7_formal_protocol::incremental_state::StateDeltaV1>, ValueTreeError>
{
    if records.values.class != MemoryClass::Ordinary {
        return Err(ValueTreeError::Invalid("organization inner mutation class"));
    }
    let admission = workspace(records.values.memory, writes.len())?;
    let source = Source {
        source: records.values.source,
        memory: records.values.memory,
        class: records.values.class,
        io: RefCell::new(&mut *records.values.io),
        cancelled: RefCell::new(&mut *records.cancelled),
        error: RefCell::new(None),
    };
    let delta = source.finish(
        oasis7_formal_protocol::incremental_state::StateDeltaV1::build(
            root.clone(),
            writes,
            &source,
            max_pending,
        ),
    )?;
    Ok(admission.retain(delta))
}
