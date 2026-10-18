//! Trusted host reports enter the exact durable Intent state machine here.
use super::*;
use crate::{AgentIntentReplayDisposition, WorldEventBody};
/// Exact accepted provider-turn identity. This reference alone is not a receipt proof.
#[derive(Debug, Clone, Copy)]
pub struct AgentIntentReference<'a> {
    pub agent_id: &'a str,
    pub intent_id: &'a str,
    pub request_digest: &'a str,
}
impl World {
    /// Record one of the defined host provider failures; the Runtime rechecks
    /// the durable ledger, live intent, exact digest, and allowed transition.
    pub fn record_agent_provider_failure(
        &mut self, identity: AgentIntentReference<'_>,
        disposition: AgentIntentProviderFailureDisposition,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        self.transition_agent_chat_provider_failure_exact(identity.agent_id, identity.intent_id, identity.request_digest, disposition)
    }
    /// Reconcile against this World's actual ReceiptAppended event for the
    /// effect already bound to the accepted Intent. A provider cannot choose a receipt.
    pub fn reconcile_agent_intent_receipt(
        &mut self, identity: AgentIntentReference<'_>,
    ) -> Result<Option<AgentIntentReplayDisposition>, WorldError> {
        let effect = self.state.agents.get(identity.agent_id)
            .and_then(|cell| cell.intent.as_ref())
            .filter(|intent| intent.intent_id == identity.intent_id
                && intent.request_digest == identity.request_digest && intent.status == "accepted")
            .and_then(|intent| intent.effect_intent_id.as_deref());
        let Some(effect) = effect else { return Ok(None); };
        let receipt = self.journal.events.iter().find_map(|event| {
            matches!(&event.body, WorldEventBody::ReceiptAppended(receipt) if receipt.identity_key() == effect)
                .then_some(event.id)
        });
        let Some(receipt) = receipt else { return Ok(None); };
        self.complete_agent_intent_with_receipt_exact(identity.agent_id, identity.intent_id, identity.request_digest, receipt).map(Some)
    }
}
