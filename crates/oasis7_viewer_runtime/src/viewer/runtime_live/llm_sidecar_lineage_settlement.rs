use super::lineage_generation_recovery::{
    validate_provider_lease_binding, validate_provider_lease_identity,
};
use super::lineage_persistence::committed_runtime_record_for_request;
use super::*;
use crate::runtime::World as RuntimeWorld;

impl RuntimeLlmSidecar {
    /// Settle a Runtime lease retained by commit-marker recovery before the
    /// sidecar discards its last request mirror. Runtime commits and economy
    /// settlement are separate durable transitions, so a crash between them
    /// must replay the exact settlement operation rather than release or
    /// redispatch the provider request.
    pub(in crate::viewer::runtime_live) fn settle_committed_provider_cognition_leases(
        &mut self,
        world: &mut RuntimeWorld,
    ) -> Result<(), String> {
        if self.provider_service_required {
            // Canonical receipt Lookup owns this recovery path. A projection
            // marker cannot authorize settlement after service restart.
            return Ok(());
        }
        let candidate_agents = self
            .provider_cognition_leases
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut settled_agents = Vec::new();
        for agent_id in candidate_agents {
            let Some(lease) = self.provider_cognition_leases.get(&agent_id).cloned() else {
                continue;
            };
            let context = self
                .provider_active_turns
                .get(agent_id.as_str())
                .or_else(|| self.provider_contexts.get(agent_id.as_str()))
                .or_else(|| self.provider_retry_contexts.get(agent_id.as_str()))
                .cloned()
                .or_else(|| {
                    self.pending_actions.values().find_map(|pending| {
                        (pending.agent_id == agent_id)
                            .then(|| {
                                pending
                                    .cognition
                                    .as_ref()
                                    .map(|cognition| cognition.request.clone())
                            })
                            .flatten()
                    })
                });
            let Some(context) = context else {
                continue;
            };
            let Some(_marker) =
                committed_runtime_record_for_request(world, &context.request_context)?
            else {
                continue;
            };
            validate_provider_lease_identity(agent_id.as_str(), &context.request_context, &lease)?;
            validate_provider_lease_binding(
                world,
                agent_id.as_str(),
                &context.request_context,
                &lease,
                "settle",
            )?;
            world
                .settle_cognition_lease(lease.lease_id.as_str(), lease.reserved_amount)
                .map_err(|error| {
                    format!(
                        "committed provider cognition lease settlement failed for {}: {error:?}",
                        lease.lease_id
                    )
                })?;
            settled_agents.push(agent_id);
        }
        if settled_agents.is_empty() {
            return Ok(());
        }
        for agent_id in settled_agents {
            self.provider_cognition_leases.remove(agent_id.as_str());
            let pending_receipt_recovery = self
                .pending_actions
                .values()
                .any(|pending| pending.agent_id == agent_id && pending.cognition.is_some());
            if pending_receipt_recovery {
                // A crash can leave a committed action in the reconstructed
                // pending map after Runtime settlement has succeeded. Keep
                // its request/response and memory intents until the receipt
                // recovery pass recreates feedback and consumes those intents.
                continue;
            }
            self.provider_active_turns.remove(agent_id.as_str());
            self.provider_contexts.remove(agent_id.as_str());
            self.provider_retry_contexts.remove(agent_id.as_str());
            self.provider_recovery_pending.remove(agent_id.as_str());
            self.provider_wait_until.remove(agent_id.as_str());
            self.pending_actions
                .retain(|_, pending| pending.agent_id != agent_id);
        }
        self.persist_provider_lineage()
    }
}
