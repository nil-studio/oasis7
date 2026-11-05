//! One canonical producer, three actual streaming object stores. A failure
//! propagates before the Host can publish any active root or Formal candidate.
use oasis7_distfs::incremental_page_store::{
    PageIoBudget, PageStoreError,
    objects::{IncrementalObjectStore, ObjectIo, ObjectStreamWriteError},
};
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_runtime::{BodyMutationError, incremental_action_original::ActionOriginalStreamV1};
use std::{
    cell::RefCell,
    io::{self, Write},
    sync::atomic::AtomicBool,
};
pub(super) struct Originals<'a, 'store, 'work> {
    pub(super) stores: &'a [IncrementalObjectStore<'store>; 3],
    pub(super) physical: &'work mut [PageIoBudget; 3],
    pub(super) logical: &'work mut [ValueIoBudget; 3],
    pub(super) occurrences: &'work mut u64,
    pub(super) max_bytes: usize,
    pub(super) cancelled: &'a AtomicBool,
    pub(super) error: &'a RefCell<Option<PageStoreError>>,
}
fn store_error(error: PageStoreError, slot: &RefCell<Option<PageStoreError>>) -> BodyMutationError {
    if slot.borrow().is_none() {
        *slot.borrow_mut() = Some(error);
    }
    BodyMutationError::Invalid("IWS original replica storage failed")
}
fn stream_error(
    error: ObjectStreamWriteError<BodyMutationError>,
    slot: &RefCell<Option<PageStoreError>>,
) -> BodyMutationError {
    match error {
        ObjectStreamWriteError::Producer(e) => e,
        ObjectStreamWriteError::Store(e) => store_error(e, slot),
    }
}
struct Tee<'a> {
    writers: [&'a mut dyn Write; 3],
    failed: bool,
}
impl Write for Tee<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("IWS original fanout poisoned"));
        }
        self.failed = true;
        for writer in &mut self.writers {
            writer.write_all(bytes)?;
        }
        self.failed = false;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("IWS original fanout poisoned"));
        }
        self.failed = true;
        for writer in &mut self.writers {
            writer.flush()?;
        }
        self.failed = false;
        Ok(())
    }
}
impl Originals<'_, '_, '_> {
    fn begin(&mut self, length: u64) -> Result<(), BodyMutationError> {
        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ValueTreeError::Cancelled.into());
        }
        if self.error.borrow().is_some() {
            return Err(BodyMutationError::Invalid("IWS original transfer poisoned"));
        }
        if length
            > u64::try_from(self.max_bytes)
                .map_err(|_| BodyMutationError::Invalid("IWS original maximum"))?
        {
            return Err(BodyMutationError::Invalid(
                "IWS original transfer byte bound",
            ));
        }
        *self.occurrences = self
            .occurrences
            .checked_sub(1)
            .ok_or(BodyMutationError::Invalid(
                "IWS original transfer occurrences",
            ))?;
        Ok(())
    }
    pub(super) fn raw(&mut self, id: [u8; 32], bytes: &[u8]) -> Result<(), BodyMutationError> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| BodyMutationError::Invalid("IWS original byte length"))?;
        self.begin(length)?;
        for (index, store) in self.stores.iter().enumerate() {
            let mut io = ObjectIo {
                physical: &mut self.physical[index],
                logical: &mut self.logical[index],
                class: MemoryClass::Ordinary,
                cancelled: self.cancelled,
            };
            store
                .write_object(&mut &bytes[..], length, Some(id), &mut io)
                .map_err(|e| store_error(e, self.error))?;
        }
        Ok(())
    }
    pub(super) fn stream(
        &mut self,
        source: &mut dyn ActionOriginalStreamV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), BodyMutationError> {
        let length = source.original_bytes();
        let id = source.original_id();
        self.begin(length)?;
        let [sp, pp, rp] = &mut *self.physical;
        let [sl, pl, rl] = &mut *self.logical;
        let mut source_io = ObjectIo {
            physical: sp,
            logical: sl,
            class: MemoryClass::Ordinary,
            cancelled: self.cancelled,
        };
        let mut primary_io = ObjectIo {
            physical: pp,
            logical: pl,
            class: MemoryClass::Ordinary,
            cancelled: self.cancelled,
        };
        let mut replica_io = ObjectIo {
            physical: rp,
            logical: rl,
            class: MemoryClass::Ordinary,
            cancelled: self.cancelled,
        };
        let stop = RefCell::new(cancelled);
        self.stores[0]
            .write_object_from_stream(
                length,
                id,
                &mut source_io,
                &mut || (*stop.borrow_mut())(),
                |source_out| {
                    self.stores[1]
                        .write_object_from_stream(
                            length,
                            id,
                            &mut primary_io,
                            &mut || (*stop.borrow_mut())(),
                            |primary_out| {
                                self.stores[2]
                                    .write_object_from_stream(
                                        length,
                                        id,
                                        &mut replica_io,
                                        &mut || (*stop.borrow_mut())(),
                                        |replica_out| {
                                            source.write_to(&mut Tee {
                                                writers: [source_out, primary_out, replica_out],
                                                failed: false,
                                            })
                                        },
                                    )
                                    .map_err(|e| stream_error(e, self.error))?;
                                Ok(())
                            },
                        )
                        .map_err(|e| stream_error(e, self.error))?;
                    Ok(())
                },
            )
            .map_err(|e| stream_error(e, self.error))?;
        Ok(())
    }
}
