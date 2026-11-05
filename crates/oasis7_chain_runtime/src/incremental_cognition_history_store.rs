//! Concrete History-original consumer. Success includes the object store's
//! locator publication and winning-original readback, not formal acceptance.
use oasis7_distfs::incremental_page_store::{
    PageStoreError,
    objects::{IncrementalObjectStore, ObjectIo, ObjectStreamWriteError},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1, RootSetV1},
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_world_state::incremental_cognition_history::{
    CognitionHistoryOriginalProducerV1, CognitionHistoryOriginalSinkV1, CognitionHistoryViewV1,
};

/// The caller owns continuous physical/logical IO and the store cancellation
/// callback. The producer additionally checks its own cancellation channel.
pub struct CognitionHistoryObjectSinkV1<'a, 'store, 'io, C> {
    pub objects: &'a IncrementalObjectStore<'store>,
    pub io: &'a mut ObjectIo<'io>,
    pub cancelled: C,
}

/// Intended for ExecutionTransaction::seal_with_final_fact. Compare the actual
/// after RootSet before physical publication. This emits one ordinary Result
/// reference, never an accepted or History-closure capability.
pub fn persist_cognition_history_view_result_v1(
    after: &RootSetV1,
    view: &CognitionHistoryViewV1,
    memory: &MemoryBudget,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ExecutionFactRefV1, ValueTreeError> {
    let _scratch = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if io.class != MemoryClass::Ordinary || !objects.uses_budget(memory) {
        return Err(ValueTreeError::Invalid(
            "Cognition manifest store pool/class",
        ));
    }
    if after
        .digest()
        .map_err(|_| ValueTreeError::Invalid("Cognition manifest after RootSet"))?
        != view.roots
    {
        return Err(ValueTreeError::Invalid(
            "Cognition manifest after-root binding",
        ));
    }
    let bytes = view.canonical_bytes(memory, &mut cancelled)?;
    publish_manifest_bytes(bytes.as_slice(), objects, io, &mut cancelled)
}
/// Explicit v2 Result publication. Dependencies are integrity references;
/// their actual availability and typed closure remain receiver obligations.
pub fn persist_cognition_history_view_result_v2(
    after: &RootSetV1,
    view: &CognitionHistoryViewV1,
    derived: &oasis7_world_state::incremental_cognition_history::CognitionHistoryDerivedRefsV2,
    memory: &MemoryBudget,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ExecutionFactRefV1, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if io.class != MemoryClass::Ordinary
        || !objects.uses_budget(memory)
        || after
            .digest()
            .map_err(|_| ValueTreeError::Invalid("Cognition manifest after RootSet"))?
            != view.roots
    {
        return Err(ValueTreeError::Invalid(
            "Cognition v2 manifest pool/class/roots",
        ));
    }
    let bytes = oasis7_world_state::incremental_cognition_history::cognition_history_view_v2_bytes(
        view,
        derived,
        memory,
        &mut cancelled,
    )?;
    publish_manifest_bytes(bytes.as_slice(), objects, io, &mut cancelled)
}
fn publish_manifest_bytes(
    bytes: &[u8],
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ExecutionFactRefV1, ValueTreeError> {
    let id = *blake3::hash(bytes).as_bytes();
    objects
        .write_object_from_stream(bytes.len() as u64, id, io, &mut cancelled, |output| {
            output.write_all(bytes)
        })
        .map_err(|error| match error {
            ObjectStreamWriteError::Producer(error) => ValueTreeError::Io(error),
            ObjectStreamWriteError::Store(error) => store_error(error),
        })?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Result,
        id,
    })
}
fn store_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(error) => ValueTreeError::Memory(error),
        PageStoreError::ResourceLimited(label) => ValueTreeError::ResourceLimited(label),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(error) => ValueTreeError::Io(error),
        PageStoreError::Missing(_) => ValueTreeError::Io(std::io::ErrorKind::NotFound.into()),
        PageStoreError::Corrupt(label) => ValueTreeError::Invalid(label),
        PageStoreError::UnsupportedPlatform => {
            ValueTreeError::Invalid("Cognition original store platform")
        }
    }
}
impl<C: FnMut() -> bool> CognitionHistoryOriginalSinkV1
    for CognitionHistoryObjectSinkV1<'_, '_, '_, C>
{
    fn store<P: FnMut() -> bool>(
        &mut self,
        original: &mut CognitionHistoryOriginalProducerV1<'_, '_, '_, P>,
    ) -> Result<(), ValueTreeError> {
        if self.io.class != MemoryClass::Ordinary || !self.objects.uses_budget(original.memory()) {
            return Err(ValueTreeError::Invalid(
                "Cognition original store pool/class",
            ));
        }
        let expected = *original.id();
        let length = original.encoded_bytes();
        self.objects
            .write_object_from_stream(length, expected, self.io, &mut self.cancelled, |output| {
                original.write_to(output)
            })
            .map(|_| ())
            .map_err(|error| match error {
                ObjectStreamWriteError::Producer(error) => error,
                ObjectStreamWriteError::Store(error) => store_error(error),
            })
    }
}
