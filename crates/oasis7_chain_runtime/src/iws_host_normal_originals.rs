//! Private adapters over one real original-object IO loan. A callback cannot
//! manufacture custody: every write uses the store's finish/readback kernel.
use oasis7_distfs::incremental_page_store::{
    PageStoreError,
    objects::{IncrementalObjectStore, ObjectIo, ObjectStreamWriteError},
};
use oasis7_formal_protocol::{
    incremental_material::{
        OriginalObjectReader, OriginalObjectStreamReader, OriginalObjectStreamWriter,
    },
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_runtime::{
    BodyMutationError, ObservationJsonOriginalSinkV2, ObservationJsonOriginalStreamV2,
    ObservationOriginalStreamFailedV2,
};
use oasis7_world_state::incremental_original_producer::{
    AgentEventOriginalSinkV1, AgentEventOriginalStreamV1, AgentEventOriginalV1,
};
use std::{
    cell::RefCell,
    io::{Read, Write},
    sync::Arc,
};

pub(in crate::iws_host_runtime) struct HostNormalOriginalsV1<'a, 'store, 'quota> {
    store: &'a IncrementalObjectStore<'store>,
    io: RefCell<ObjectIo<'quota>>,
    replica: Option<(
        &'a IncrementalObjectStore<'store>,
        RefCell<ObjectIo<'quota>>,
    )>,
    memory: &'a MemoryBudget,
    _fee: MemoryReservation,
}
impl<'a, 'store, 'quota> HostNormalOriginalsV1<'a, 'store, 'quota> {
    pub(in crate::iws_host_runtime) fn new(
        store: &'a IncrementalObjectStore<'store>,
        io: ObjectIo<'quota>,
        memory: &'a MemoryBudget,
    ) -> Result<Self, PageStoreError> {
        if io.class != MemoryClass::Ordinary || !store.uses_budget(memory) {
            return Err(PageStoreError::Corrupt("normal original IO pool/class"));
        }
        let bytes = std::mem::size_of::<Self>()
            .checked_add(std::mem::size_of::<
                HostNormalOriginalReaderV1<'_, '_, '_, '_>,
            >())
            .and_then(|n| {
                n.checked_add(std::mem::size_of::<
                    HostNormalObservationSinkV1<'_, '_, '_, '_>,
                >())
            })
            .and_then(|n| {
                n.checked_add(std::mem::size_of::<HostNormalAgentSinkV1<'_, '_, '_, '_>>())
            })
            .ok_or(PageStoreError::ResourceLimited("normal original IO layout"))?;
        let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
        Ok(Self {
            store,
            io: RefCell::new(io),
            replica: None,
            memory,
            _fee: fee,
        })
    }
    pub(super) fn new_pair(
        primary: &'a IncrementalObjectStore<'store>,
        io: ObjectIo<'quota>,
        replica: &'a IncrementalObjectStore<'store>,
        replica_io: ObjectIo<'quota>,
        memory: &'a MemoryBudget,
    ) -> Result<Self, PageStoreError> {
        if replica_io.class != MemoryClass::Ordinary || !replica.uses_budget(memory) {
            return Err(PageStoreError::Corrupt(
                "normal replica original IO pool/class",
            ));
        }
        let mut value = Self::new(primary, io, memory)?;
        value.replica = Some((replica, RefCell::new(replica_io)));
        Ok(value)
    }
    /// Replay the verified immutable primary object rather than the producer.
    /// This also handles one-shot Read producers and charges both namespaces'
    /// real IO. Failure leaves provisional immutable objects, never a new head.
    fn replicate_original(
        &self,
        id: [u8; 32],
        length: u64,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), PageStoreError> {
        let Some((replica, replica_io)) = &self.replica else {
            return Ok(());
        };
        let mut source_io = self.io.try_borrow_mut().map_err(|_| reentrant())?;
        let mut target_io = replica_io.try_borrow_mut().map_err(|_| reentrant())?;
        let mut reader = self.store.provisional_read_write_session(&mut source_io)?;
        replica
            .write_object_from_stream(length, id, &mut target_io, cancelled, |output| {
                let actual = reader.stream_original(&id, length, self.memory, output)?;
                if actual != length {
                    return Err(PageStoreError::Corrupt("normal replica original length"));
                }
                Ok(())
            })
            .map(|_| ())
            .map_err(|error| match error {
                ObjectStreamWriteError::Producer(error) | ObjectStreamWriteError::Store(error) => {
                    error
                }
            })
    }
    /// Publication output persistence requires the actual two-store adapter.
    pub(in crate::iws_host_runtime) fn paired_reader(
        &self,
    ) -> Result<HostNormalOriginalReaderV1<'_, 'a, 'store, 'quota>, PageStoreError> {
        if self.replica.is_none() {
            return Err(PageStoreError::Corrupt(
                "normal package requires paired original stores",
            ));
        }
        Ok(self.reader())
    }
    pub(in crate::iws_host_runtime) fn reader(
        &self,
    ) -> HostNormalOriginalReaderV1<'_, 'a, 'store, 'quota> {
        HostNormalOriginalReaderV1(self)
    }
    pub(super) fn observation_sink(&self) -> HostNormalObservationSinkV1<'_, 'a, 'store, 'quota> {
        HostNormalObservationSinkV1(self)
    }
    pub(in crate::iws_host_runtime) fn persist_event(
        &self,
        source: &mut dyn AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError> {
        source.persist_to(&mut HostNormalAgentSinkV1(self))
    }
}
fn reentrant() -> PageStoreError {
    PageStoreError::Corrupt("normal original IO reentry")
}
fn body(error: PageStoreError) -> BodyMutationError {
    match error {
        PageStoreError::Memory(error) => ValueTreeError::Memory(error).into(),
        PageStoreError::Cancelled => ValueTreeError::Cancelled.into(),
        PageStoreError::ResourceLimited(label) => ValueTreeError::ResourceLimited(label).into(),
        error => BodyMutationError::Original(Box::new(error)),
    }
}

pub(in crate::iws_host_runtime) struct HostNormalOriginalReaderV1<'s, 'a, 'store, 'quota>(
    &'s HostNormalOriginalsV1<'a, 'store, 'quota>,
);
impl OriginalObjectReader for HostNormalOriginalReaderV1<'_, '_, '_, '_> {
    type Error = PageStoreError;
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn allocation_budget(&self) -> &MemoryBudget {
        self.0.memory
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        let mut io = self.0.io.try_borrow_mut().map_err(|_| reentrant())?;
        let mut session = self.0.store.provisional_read_write_session(&mut io)?;
        session.read_original(id, max_bytes, memory)
    }
}
impl OriginalObjectStreamReader for HostNormalOriginalReaderV1<'_, '_, '_, '_> {
    fn original_writer(
        &mut self,
    ) -> Option<&mut dyn OriginalObjectStreamWriter<Error = Self::Error>> {
        Some(self)
    }
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        let mut io = self.0.io.try_borrow_mut().map_err(|_| reentrant())?;
        let mut session = self.0.store.provisional_read_write_session(&mut io)?;
        session.stream_original(id, max_bytes, memory, output)
    }
}
impl OriginalObjectStreamWriter for HostNormalOriginalReaderV1<'_, '_, '_, '_> {
    type Error = PageStoreError;
    fn write_original(
        &mut self,
        input: &mut dyn Read,
        length: u64,
        expected_id: [u8; 32],
        memory: &MemoryBudget,
    ) -> Result<[u8; 32], Self::Error> {
        let id = {
            let mut io = self.0.io.try_borrow_mut().map_err(|_| reentrant())?;
            let mut session = self.0.store.provisional_read_write_session(&mut io)?;
            OriginalObjectStreamWriter::write_original(
                &mut session,
                input,
                length,
                expected_id,
                memory,
            )?
        };
        self.0.replicate_original(id, length, &mut || false)?;
        Ok(id)
    }
}

pub(super) struct HostNormalObservationSinkV1<'s, 'a, 'store, 'quota>(
    &'s HostNormalOriginalsV1<'a, 'store, 'quota>,
);
impl ObservationJsonOriginalSinkV2 for HostNormalObservationSinkV1<'_, '_, '_, '_> {
    type Error = ObjectStreamWriteError<ObservationOriginalStreamFailedV2>;
    fn finish_json_original(
        &mut self,
        source: &mut dyn ObservationJsonOriginalStreamV2,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<[u8; 32], Self::Error> {
        if !self.0.store.uses_budget(source.memory()) {
            return Err(ObjectStreamWriteError::Store(PageStoreError::Corrupt(
                "normal observation pool",
            )));
        }
        let mut io = self
            .0
            .io
            .try_borrow_mut()
            .map_err(|_| ObjectStreamWriteError::Store(reentrant()))?;
        let id = self.0.store.write_object_from_stream(
            source.original_bytes(),
            source.original_id(),
            &mut io,
            cancelled,
            |output| source.write_to(output),
        )?;
        drop(io);
        self.0
            .replicate_original(id, source.original_bytes(), cancelled)
            .map_err(ObjectStreamWriteError::Store)?;
        Ok(id)
    }
}
struct HostNormalAgentSinkV1<'s, 'a, 'store, 'quota>(&'s HostNormalOriginalsV1<'a, 'store, 'quota>);
impl AgentEventOriginalSinkV1 for HostNormalAgentSinkV1<'_, '_, '_, '_> {
    fn finish_original(
        &mut self,
        source: &mut dyn AgentEventOriginalStreamV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), BodyMutationError> {
        let mut io = self.0.io.try_borrow_mut().map_err(|_| body(reentrant()))?;
        let id = self
            .0
            .store
            .write_object_from_stream(
                source.original_bytes(),
                source.original_id(),
                &mut io,
                cancelled,
                |output| source.write_to(output),
            )
            .map_err(|error| match error {
                ObjectStreamWriteError::Producer(error) => error,
                ObjectStreamWriteError::Store(error) => body(error),
            })?;
        drop(io);
        self.0
            .replicate_original(id, source.original_bytes(), cancelled)
            .map_err(body)
    }
}

#[cfg(all(test, unix))]
#[path = "iws_host_normal_originals_tests.rs"]
mod tests;

#[path = "iws_host_normal_package_meter.rs"]
mod package_meter;
pub(in crate::iws_host_runtime) use package_meter::PairedNormalOriginalMeterErrorV1;

// A retained-history consumer still authenticates its root/path/schema. This
// adapter supplies only original bytes through the real paired IO account.
impl oasis7_runtime::PaidProjectionRetainedOriginalReaderV1
    for HostNormalOriginalReaderV1<'_, '_, '_, '_>
{
    type RetainedError = BodyMutationError;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.0.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_retained_original_bounded_v1(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::RetainedError> {
        common
            .charge(1, 32)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.0.memory.same_pool(common.memory()) || !self.0._fee.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "retained original actual Host pool",
            ));
        }
        let raw = self
            .0
            .with_metered_actual_reader(common, |reader| {
                OriginalObjectReader::read_original(reader, id, max_bytes, self.0.memory)
            })
            .map_err(|error| match error {
                PairedNormalOriginalMeterErrorV1::Context(error) => {
                    BodyMutationError::Original(Box::new(error))
                }
                PairedNormalOriginalMeterErrorV1::Store(error) => body(error),
            })?;
        if !raw.uses_budget(common.memory()) || raw.len() > max_bytes {
            return Err(BodyMutationError::Invalid(
                "retained original actual allocation/length",
            ));
        }
        let mut hash = blake3::Hasher::new();
        for chunk in raw.chunks(8192) {
            common
                .charge(1, chunk.len() as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            hash.update(chunk);
        }
        common
            .charge(1, 32)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if hash.finalize().as_bytes() != id {
            return Err(BodyMutationError::Invalid("retained original content hash"));
        }
        Ok(raw)
    }
}
