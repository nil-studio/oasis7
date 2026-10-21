pub use oasis7_world_state::world::agent_intent_publication::*;
// Atomic projection for one AgentIntent lifecycle event.

use super::super::{DomainEvent, WorldError, WorldEventId};
use super::World;

impl World {
    pub(super) fn prepare_raw_agent_intent_event(
        &self,
        event: &DomainEvent,
    ) -> Result<PreparedAgentIntent, WorldError> {
        let envelope_event_seq: WorldEventId = self.next_event_id.max(1);
        let committed_receipt_event_id =
            self.validate_agent_intent_receipt_reference(event, Some(envelope_event_seq))?;
        self.state.prepare_agent_intent_event(
            event,
            self.state.time,
            Some(envelope_event_seq),
            committed_receipt_event_id,
        )
    }
}
