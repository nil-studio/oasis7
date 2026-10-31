//! Charged page observation shared by closed typed-family validators.
//! This tracker has no schema/authority knowledge. Only the owning validator
//! can decide that its complete typed closure has been traversed successfully.
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cell::RefCell, collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy)]
pub(crate) struct TrackingLabels {
    pub(crate) allowance: &'static str,
    pub(crate) hash: &'static str,
    pub(crate) page_limit: &'static str,
    pub(crate) state_bound: &'static str,
    pub(crate) state_digest: &'static str,
    pub(crate) state_tracking: &'static str,
    pub(crate) value_admission: &'static str,
}

pub(crate) struct Tracking<'a, W> {
    pub(crate) pages: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    max: usize,
    sink: Option<&'a mut W>,
    pub(crate) error: Option<ValueTreeError>,
    labels: TrackingLabels,
}
impl<'a, W: ValueNodeWriter> Tracking<'a, W> {
    pub(crate) fn new(
        memory: &MemoryBudget,
        class: MemoryClass,
        max: usize,
        sink: Option<&'a mut W>,
        labels: TrackingLabels,
    ) -> Result<Self, ValueTreeError> {
        let bytes = max
            .checked_mul(128)
            .and_then(|n| n.checked_add(4096))
            .ok_or(ValueTreeError::ResourceLimited(labels.allowance))?;
        let retained = memory.reserve(bytes, class)?;
        Ok(Self {
            pages: retained.retain(RefCell::new(BTreeSet::new())),
            max,
            sink,
            error: None,
            labels,
        })
    }

    fn note(
        &mut self,
        id: [u8; 32],
        bytes: &[u8],
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if *blake3::hash(bytes).as_bytes() != id {
            return Err(ValueTreeError::Invalid(self.labels.hash));
        }
        if self.pages.borrow().contains(&id) {
            return Ok(());
        }
        if self.pages.borrow().len() >= self.max {
            return Err(ValueTreeError::ResourceLimited(self.labels.page_limit));
        }
        if let Some(sink) = self.sink.as_mut() {
            let charge = memory.reserve(bytes.len(), MemoryClass::Ordinary)?;
            sink.put_node(id, charge.retain(bytes.to_vec()))?;
        }
        self.pages.borrow_mut().insert(id);
        Ok(())
    }
}
pub(crate) struct State<'a, 's, M, W> {
    pub(crate) source: &'a M,
    pub(crate) tracking: &'a RefCell<Tracking<'s, W>>,
    pub(crate) memory: &'a MemoryBudget,
}
impl<M: MerkleNodeReader, W: ValueNodeWriter> MerkleNodeReader for State<'_, '_, M, W> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let page = self.source.read_node(digest, max)?;
        if let Some(page) = page.as_ref() {
            let result = if page.as_ref().len() > max {
                Err(ValueTreeError::Invalid(
                    self.tracking.borrow().labels.state_bound,
                ))
            } else {
                let hash = blake3::hash(page.as_ref());
                if hash.to_hex().as_str() != digest {
                    Err(ValueTreeError::Invalid(
                        self.tracking.borrow().labels.state_digest,
                    ))
                } else {
                    self.tracking
                        .borrow_mut()
                        .note(*hash.as_bytes(), page.as_ref(), self.memory)
                }
            };
            if let Err(error) = result {
                self.tracking.borrow_mut().error = Some(error);
                return Err(self.tracking.borrow().labels.state_tracking.into());
            }
        }
        Ok(page)
    }
}
pub(crate) struct Values<'a, 's, R, W> {
    pub(crate) source: &'a R,
    pub(crate) tracking: &'a RefCell<Tracking<'s, W>>,
}
impl<R: ValueNodeReader, W: ValueNodeWriter> ValueNodeReader for Values<'_, '_, R, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let page = self.source.read_node(id, max, memory, class)?;
        if page.len() > max || page.capacity() > page.reserved_bytes() || !page.uses_budget(memory)
        {
            return Err(ValueTreeError::Invalid(
                self.tracking.borrow().labels.value_admission,
            ));
        }
        self.tracking.borrow_mut().note(*id, &page, memory)?;
        Ok(page)
    }
}
pub(crate) struct NoSink;
impl ValueNodeWriter for NoSink {
    fn put_node(&mut self, _: [u8; 32], _: ReservedValue<Vec<u8>>) -> Result<(), ValueTreeError> {
        unreachable!("mark does not write")
    }
}
