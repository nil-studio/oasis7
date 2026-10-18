use super::{WorldEvent, WorldEventKind, WorldKernel};
use crate::{AgentId, LlmEffectIntentTrace, LlmEffectReceiptTrace};

impl WorldKernel {
    /// Record an already selected host effect without executing a model or provider.
    pub fn record_effect_intent(
        &mut self,
        agent_id: AgentId,
        intent: LlmEffectIntentTrace,
    ) -> WorldEvent {
        self.record_event(WorldEventKind::LlmEffectQueued { agent_id, intent })
    }
    /// Record the original host receipt value; the kernel assigns the event identity.
    pub fn record_effect_receipt(
        &mut self,
        agent_id: AgentId,
        receipt: LlmEffectReceiptTrace,
    ) -> WorldEvent {
        self.record_event(WorldEventKind::LlmReceiptAppended { agent_id, receipt })
    }
}
