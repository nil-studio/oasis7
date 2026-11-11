//! Concrete action/event originals reuse the bounded canonical one-shot engine.
//! Completion is provisional: the owning ObjectStore must finish its barriers,
//! and acceptance still independently verifies the actual original source.
use super::incremental_body::BodyMutationError;
pub use super::incremental_body_event::{
    AgentEventOriginalSinkV1 as ActionOriginalSinkV1,
    AgentEventOriginalStreamV1 as ActionOriginalStreamV1,
};
use super::incremental_body_event::{AgentEventOriginalV1, original};
use oasis7_proto::memory_budget::MemoryBudget;
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::ActionEnvelope;
/// A consumed sink report, not a durable-original or authority qualification.
#[derive(Debug)]
pub struct ActionOriginalReceiptV1 {
    id: [u8; 32],
    bytes: u64,
}
pub type WorldEventOriginalReceiptV1 = ActionOriginalReceiptV1;
impl ActionOriginalReceiptV1 {
    pub fn original_id(&self) -> [u8; 32] {
        self.id
    }
    pub fn original_bytes(&self) -> u64 {
        self.bytes
    }
}
fn persist<T: serde::Serialize>(
    value: &T,
    memory: &MemoryBudget,
    work: &mut CanonicalCborWorkV1,
    cancelled: &mut impl FnMut() -> bool,
    max_bytes: usize,
    sink: &mut dyn ActionOriginalSinkV1,
) -> Result<ActionOriginalReceiptV1, BodyMutationError> {
    let measured = original::measure_serializable(value, memory, work, cancelled, max_bytes)?;
    let mut producer = original::Producer::new(measured, memory, work, cancelled);
    let id = producer.original_id();
    let bytes = producer.original_bytes();
    producer.persist_to(sink)?;
    producer.check_complete()?;
    Ok(ActionOriginalReceiptV1 { id, bytes })
}
/// Both measurement and actual canonical output debit the same caller-owned
/// work. Input admission is owned by the caller; no action/encoded Vec is cloned.
pub fn persist_action_original_v1(
    action: &ActionEnvelope,
    memory: &MemoryBudget,
    work: &mut CanonicalCborWorkV1,
    cancelled: &mut impl FnMut() -> bool,
    max_action_bytes: usize,
    sink: &mut dyn ActionOriginalSinkV1,
) -> Result<ActionOriginalReceiptV1, BodyMutationError> {
    persist(action, memory, work, cancelled, max_action_bytes, sink)
}
/// The existing WorldEvent measure/producer APIs remain compatible; this
/// concrete wrapper supports the same actual physical-original consumer.
pub fn persist_world_event_original_v1(
    event: &crate::WorldEvent,
    memory: &MemoryBudget,
    work: &mut CanonicalCborWorkV1,
    cancelled: &mut impl FnMut() -> bool,
    max_event_bytes: usize,
    sink: &mut dyn ActionOriginalSinkV1,
) -> Result<WorldEventOriginalReceiptV1, BodyMutationError> {
    persist(event, memory, work, cancelled, max_event_bytes, sink)
}
#[cfg(test)]
#[path = "incremental_action_original_tests.rs"]
mod tests;
