//! A private proof binds the original DomainEvent type to actual source bytes.
//! Mailbox agent/ordinal identity stays in its authenticated existing state key.
use crate::{
    DomainEvent,
    incremental_pending_action_original_binding::{
        PendingActionOriginalErrorV1, PendingActionOriginalWorkV1, verify_typed_original_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1, incremental_material::OriginalObjectStreamReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub type DomainEventOriginalWorkV1 = PendingActionOriginalWorkV1;
pub type DomainEventOriginalErrorV1<E> = PendingActionOriginalErrorV1<E>;
pub struct CheckedDomainEventOriginalV1 {
    reference: OriginalArtifactRefV1,
    class: MemoryClass,
}
impl CheckedDomainEventOriginalV1 {
    pub fn original(&self) -> OriginalArtifactRefV1 {
        self.reference
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub(crate) fn validate(
        carrier: &ReservedValue<Self>,
        id: &[u8; 32],
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if carrier.reference.id != *id
            || carrier.class != class
            || !carrier.uses_budget(memory)
            || carrier.reserved_bytes() < 512
        {
            return Err(ValueTreeError::Invalid(
                "mailbox DomainEvent original binding",
            ));
        }
        Ok(())
    }
}
/// A caller cannot mint from an ID/length claim or opaque success flag. The
/// original source is streamed independently after canonical borrowed encoding;
/// every output byte, occurrence, serializer visit and heap copy is bounded.
#[allow(clippy::too_many_arguments)]
pub fn verify_domain_event_original_v1<O: OriginalObjectStreamReader>(
    event: &DomainEvent,
    expected_id: [u8; 32],
    objects: &mut O,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut DomainEventOriginalWorkV1,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedDomainEventOriginalV1>, DomainEventOriginalErrorV1<O::Error>> {
    verify_typed_original_v1(
        event,
        expected_id,
        None,
        objects,
        memory,
        class,
        work,
        cancelled,
        |reference, class| CheckedDomainEventOriginalV1 { reference, class },
    )
}

#[cfg(test)]
#[path = "incremental_world_raw_complete_original_tests.rs"]
mod tests;
