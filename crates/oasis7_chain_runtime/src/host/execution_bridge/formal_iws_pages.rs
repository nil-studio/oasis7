//! Bounded GEN page transfer. Every destination owns its admitted byte copy;
//! successful copying never selects a head or qualifies an execution.
use oasis7_distfs::incremental_page_store::{
    BufferedPageWriter, IncrementalPageStore, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget, StateScanContext},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cell::{Cell, RefCell},
    sync::{Arc, atomic::AtomicBool},
};

pub(super) struct Targets<'a, 'quota> {
    pub(super) stores: [&'a IncrementalPageStore; 2],
    writers: RefCell<Option<[BufferedPageWriter<'quota>; 2]>>,
    pub(super) cancelled: &'a AtomicBool,
    failed: Cell<bool>,
    error: RefCell<Option<PageStoreError>>,
}
impl<'a, 'quota: 'a> Targets<'a, 'quota> {
    pub(super) fn new(
        stores: [&'quota IncrementalPageStore; 2],
        io: &'quota mut [PageIoBudget; 2],
        cancelled: &'quota AtomicBool,
        checks: [&'quota mut dyn FnMut() -> bool; 2],
    ) -> Result<Self, PageStoreError> {
        let [first, second] = io;
        let [check_first, check_second] = checks;
        let writers = [
            stores[0].buffered_page_writer(first, cancelled, check_first)?,
            stores[1].buffered_page_writer(second, cancelled, check_second)?,
        ];
        Ok(Self {
            stores,
            writers: RefCell::new(Some(writers)),
            cancelled,
            failed: Cell::new(false),
            error: RefCell::new(None),
        })
    }
    pub(super) fn finish(&self) -> Result<(), PageStoreError> {
        if self.failed.replace(true) {
            return Err(PageStoreError::Corrupt(
                "IWS replica page transfer poisoned",
            ));
        }
        let writers = self
            .writers
            .borrow_mut()
            .take()
            .ok_or(PageStoreError::Corrupt("IWS replica already finished"))?;
        for writer in writers {
            writer.finish()?;
        }
        self.failed.set(false);
        Ok(())
    }
    pub(super) fn send(&self, id: [u8; 32], bytes: &[u8]) -> Result<(), ValueTreeError> {
        if self.failed.replace(true) {
            return Err(ValueTreeError::Invalid(
                "IWS replica page transfer poisoned",
            ));
        }
        let result = (|| {
            for (index, store) in self.stores.iter().enumerate() {
                if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(ValueTreeError::Cancelled);
                }
                let charge = store.memory_budget().reserve(
                    bytes
                        .len()
                        .checked_add(128)
                        .ok_or(ValueTreeError::ResourceLimited("IWS page transfer charge"))?,
                    MemoryClass::Ordinary,
                )?;
                let copy = charge.retain(bytes.to_vec());
                if let Err(error) = self
                    .writers
                    .borrow_mut()
                    .as_mut()
                    .ok_or(ValueTreeError::Invalid("IWS replica writer finished"))?[index]
                    .put_node_with_store_error_v1(id, copy)
                {
                    match error {
                        oasis7_distfs::incremental_page_store::BufferedPageWriteErrorV1::Store(
                            error,
                        ) => {
                            *self.error.borrow_mut() = Some(error);
                        }
                        oasis7_distfs::incremental_page_store::BufferedPageWriteErrorV1::Value(
                            error,
                        ) => return Err(error),
                    }
                    return Err(ValueTreeError::Invalid("IWS destination page write failed"));
                }
            }
            Ok(())
        })();
        if result.is_ok() {
            self.failed.set(false);
        }
        result
    }
    pub(super) fn check_healthy(&self) -> Result<(), ValueTreeError> {
        if self.failed.get() {
            Err(ValueTreeError::Invalid(
                "IWS page transfer did not complete",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn check_complete(&self) -> Result<(), ValueTreeError> {
        self.check_healthy()?;
        if self.writers.borrow().is_some() {
            return Err(ValueTreeError::Invalid("IWS page transfer not finished"));
        }
        Ok(())
    }
    pub(super) fn take_error(&self) -> Option<PageStoreError> {
        self.error.borrow_mut().take()
    }
}
pub(super) struct Fanout<'a, 'store, 'quota, W> {
    pub(super) primary: &'a mut W,
    pub(super) targets: &'a Targets<'store, 'quota>,
    pub(super) memory: &'a MemoryBudget,
}
impl<W: ValueNodeWriter> ValueNodeWriter for Fanout<'_, '_, '_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if !bytes.uses_budget(self.memory) {
            return Err(ValueTreeError::Invalid("IWS GEN page source pool"));
        }
        self.targets.send(id, &bytes)?;
        self.targets.failed.set(true);
        let result = self.primary.put_node(id, bytes);
        if result.is_ok() {
            self.targets.failed.set(false);
        }
        result
    }
}
impl<W: StatePageWriter> StatePageWriter for Fanout<'_, '_, '_, W> {}
impl<W: StatePageWriter> oasis7_formal_protocol::incremental_schedule::SchedulePageWriter
    for Fanout<'_, '_, '_, W>
{
}
impl<W: ValueNodeReader> ValueNodeReader for Fanout<'_, '_, '_, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.primary.read_node(id, max, memory, class)
    }
}
struct IndexReader<'a, 'b, 'quota, R> {
    source: &'a R,
    targets: &'a Targets<'b, 'quota>,
}
impl<R: MerkleNodeReader> MerkleNodeReader for IndexReader<'_, '_, '_, R> {
    type Node = R::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let node = self.source.read_node(digest, max)?;
        if let Some(node) = &node {
            let mut id = [0; 32];
            hex::decode_to_slice(digest, &mut id).map_err(|e| e.to_string())?;
            self.targets
                .send(id, node.as_ref())
                .map_err(|e| e.to_string())?;
        }
        Ok(node)
    }
}
/// Only the six outer SMP indexes need a final walk: all nested record/state
/// and value pages have already traversed the producer's Fanout writer.
pub(super) fn send_outer_indexes<R: MerkleNodeReader>(
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    source: &R,
    targets: &Targets<'_, '_>,
    memory: &MemoryBudget,
    work: &mut StateScanBudget,
) -> Result<(), super::IwsGenesisTransferErrorV1> {
    let reader = IndexReader { source, targets };
    let mut scan = StateScanContext {
        source: &reader,
        memory,
        class: MemoryClass::Ordinary,
        io: work,
    };
    for root in &roots.state {
        let result = scan.visit_entries::<std::convert::Infallible>(
            root,
            |_| Ok(()),
            || targets.cancelled.load(std::sync::atomic::Ordering::Acquire),
        );
        if let Some(error) = targets.take_error() {
            return Err(super::IwsGenesisTransferErrorV1::Store(error));
        }
        result.map_err(super::IwsGenesisTransferErrorV1::Scan)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "formal_iws_pages_tests.rs"]
mod tests;
