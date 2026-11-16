//! Physical consumer of genuine Current V2 observation JSON originals.
//! Object publication/readback is distinct from registration and acceptance.
use oasis7_distfs::incremental_page_store::{
    PageStoreError,
    objects::{IncrementalObjectStore, ObjectIo, ObjectStreamWriteError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_runtime::{
    ObservationJsonOriginalSinkV2, ObservationJsonOriginalStreamV2,
    ObservationOriginalStreamFailedV2,
};

/// Uses the caller's continuous physical/logical allowances. The Runtime
/// producer additionally meters its own actual hash/copy and cancellation.
pub struct EmergencyObservationObjectSinkV2<'a, 'store, 'io> {
    pub objects: &'a IncrementalObjectStore<'store>,
    pub io: &'a mut ObjectIo<'io>,
}

impl ObservationJsonOriginalSinkV2 for EmergencyObservationObjectSinkV2<'_, '_, '_> {
    type Error = ObjectStreamWriteError<ObservationOriginalStreamFailedV2>;

    fn finish_json_original(
        &mut self,
        source: &mut dyn ObservationJsonOriginalStreamV2,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<[u8; 32], Self::Error> {
        if self.io.class != MemoryClass::Ordinary || !self.objects.uses_budget(source.memory()) {
            return Err(ObjectStreamWriteError::Store(PageStoreError::Corrupt(
                "Current observation original pool/class",
            )));
        }
        // This performs the actual value/page barriers, original locator
        // publication and verification of the winning physical original.
        self.objects.write_object_from_stream(
            source.original_bytes(),
            source.original_id(),
            self.io,
            cancelled,
            |output| source.write_to(output),
        )
    }
}
