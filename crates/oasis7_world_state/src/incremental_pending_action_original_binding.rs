//! Bind a charged typed Action projection to actual canonical original bytes.
//! A byte/type proof is separate from authority, retention and durable replicas.
use crate::incremental_runtime_pending_action_record::RecoveredPendingActionRecordV1;
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1, incremental_material::OriginalObjectStreamReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_wasm_abi::{CanonicalCborStreamError, CanonicalCborWorkV1};

pub struct PendingActionOriginalWorkV1 {
    pub objects_remaining: u64,
    pub original_bytes_remaining: u64,
    pub max_original_bytes: u64,
    pub canonical: CanonicalCborWorkV1,
}
#[derive(Debug)]
pub enum PendingActionOriginalErrorV1<E> {
    Value(ValueTreeError),
    Encoding(CanonicalCborStreamError),
    Source(E),
    Invalid(&'static str),
}
impl<E> From<ValueTreeError> for PendingActionOriginalErrorV1<E> {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
impl<E: std::fmt::Display> std::fmt::Display for PendingActionOriginalErrorV1<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(e) => e.fmt(f),
            Self::Encoding(e) => e.fmt(f),
            Self::Source(e) => e.fmt(f),
            Self::Invalid(e) => f.write_str(e),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for PendingActionOriginalErrorV1<E> {}
pub struct CheckedPendingActionOriginalV1 {
    reference: OriginalArtifactRefV1,
    class: MemoryClass,
}
impl CheckedPendingActionOriginalV1 {
    pub fn original(&self) -> OriginalArtifactRefV1 {
        self.reference
    }
    pub fn class(&self) -> MemoryClass {
        self.class
    }
}
#[path = "incremental_typed_original_binding.rs"]
mod binding;
pub(crate) use binding::verify_typed_original_v1;
/// No untrusted original DTO is deserialized. The complete typed projection is
/// canonically encoded to a bounded hash sink, then the actual original source
/// is independently streamed and compared by exact length and content ID.
pub fn verify_pending_action_original_v1<O: OriginalObjectStreamReader>(
    record: &RecoveredPendingActionRecordV1,
    objects: &mut O,
    memory: &MemoryBudget,
    work: &mut PendingActionOriginalWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedPendingActionOriginalV1>, PendingActionOriginalErrorV1<O::Error>> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let class = record.class();
    if !record.uses_budget(memory)
        || !objects.allocation_budget().same_pool(memory)
        || objects.allocation_class() != class
    {
        return Err(PendingActionOriginalErrorV1::Invalid(
            "pending original pool/class",
        ));
    }
    let reference = record.value().original()?;
    binding::verify_typed_original_v1(
        record.value().envelope(),
        reference.id,
        Some(reference.byte_len),
        objects,
        memory,
        class,
        work,
        cancelled,
        |reference, class| CheckedPendingActionOriginalV1 { reference, class },
    )
}

#[cfg(test)]
#[path = "incremental_pending_action_original_binding_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
use binding::HashSink;
#[cfg(test)]
use std::io::Write;
