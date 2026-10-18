use super::*;
use sha2::{Digest, Sha256};

/// A provider world-event notification is durable until the actor accepts it.
/// The Runtime chain link may advance its watermark after this adapter call,
/// so dropping a full/unavailable mailbox would otherwise lose the event.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimePendingProviderWorldEvent {
    pub(super) agent_id: String,
    pub(super) event: WorldEvent,
    /// The Runtime head at which this event was observed.  Old checkpoints
    /// may omit it; those entries retain the prior compatibility behavior.
    #[serde(default)]
    pub(super) runtime_binding: Option<RuntimeBindingV1>,
}

fn provider_world_event_key(agent_id: &str, event: &WorldEvent) -> String {
    let mut digest = Sha256::new();
    digest.update(b"oasis7.viewer.provider-world-event.v1");
    digest.update(agent_id.as_bytes());
    digest.update([0]);
    // Runtime event identity is the durable dedup key.  The mapped simulator
    // projection may gain presentation fields without causing a second
    // provider notification.
    if let Some(runtime_event) = event.runtime_event.as_ref() {
        digest.update(
            serde_json::to_vec(runtime_event).expect("RuntimeWorldEvent must remain serializable"),
        );
    } else {
        digest.update(serde_json::to_vec(event).expect("WorldEvent must remain serializable"));
    }
    format!("provider-world-event-{}", hex::encode(digest.finalize()))
}

fn provider_event_actor_missing(error: &str, agent_id: &str) -> bool {
    // Keep mailbox/backpressure errors retryable.  Only an explicit actor
    // absence is terminal for this event; this prevents a deleted Agent from
    // creating a durable retry loop while preserving transient delivery.
    error.contains("agent is not registered")
        || error.contains("provider actor unavailable")
        || error.contains("builtin actor unavailable")
        || (error.contains("actor is unavailable") && error.contains(agent_id))
}

impl RuntimeLlmSidecar {
    /// Deliver an authoritative production completion to its owning Agent.
    /// Runtime-live receives the full runtime receipt through `mapped_event`,
    /// while the simulator behavior owns coverage state and replay idempotence.
    #[cfg(test)]
    pub(in crate::viewer::runtime_live) fn notify_recipe_completion_if_needed(
        &mut self,
        runtime_event: &RuntimeWorldEvent,
        mapped_event: WorldEvent,
    ) {
        self.notify_recipe_completion_with_binding(runtime_event, mapped_event, None);
    }

    /// Queue a completion before the provider runner is available (notably
    /// during chain-sync startup), retaining the Runtime binding so a later
    /// reorg/replacement cannot deliver an event from the wrong world head.
    pub(in crate::viewer::runtime_live) fn notify_recipe_completion_with_binding(
        &mut self,
        runtime_event: &RuntimeWorldEvent,
        mapped_event: WorldEvent,
        runtime_binding: Option<RuntimeBindingV1>,
    ) {
        if self.provider_lineage_binding.is_none() {
            self.provider_lineage_binding = runtime_binding.clone();
        }
        let RuntimeWorldEventBody::Domain(RuntimeDomainEvent::RecipeCompleted {
            requester_agent_id,
            ..
        }) = &runtime_event.body
        else {
            return;
        };

        #[cfg(not(target_arch = "wasm32"))]
        let provider_runner = self.runner.as_ref().is_some_and(|runner| {
            matches!(
                runner,
                RuntimeDecisionRunner::Builtin(_) | RuntimeDecisionRunner::ProviderBacked(_)
            )
        });
        #[cfg(target_arch = "wasm32")]
        let provider_runner = self
            .runner
            .as_ref()
            .is_some_and(|runner| matches!(runner, RuntimeDecisionRunner::ProviderBacked(_)));
        // A missing runner is a startup phase, not proof that the event has no
        // owner.  Keep it durable and flush after runner registration.
        if provider_runner || self.runner.is_none() {
            self.enqueue_provider_world_event(requester_agent_id, mapped_event, runtime_binding);
        }
        #[cfg(target_arch = "wasm32")]
        if !provider_runner {
            if let Some(RuntimeDecisionRunner::Builtin(runner)) = self.runner.as_mut() {
                if let Some(agent) = runner.get_mut(requester_agent_id.as_str()) {
                    agent.behavior.on_event(&mapped_event);
                }
            }
        }
    }

    fn enqueue_provider_world_event(
        &mut self,
        agent_id: &str,
        event: WorldEvent,
        runtime_binding: Option<RuntimeBindingV1>,
    ) {
        let key = provider_world_event_key(agent_id, &event);
        self.pending_provider_world_events
            .entry(key)
            .or_insert_with(|| RuntimePendingProviderWorldEvent {
                agent_id: agent_id.to_string(),
                event,
                runtime_binding,
            });
        // Persist before trying the actor. If the process stops after the
        // actor accepts the command but before this checkpoint is cleared,
        // the duplicate is intentional: this boundary is at-least-once.
        self.persist_provider_lineage_best_effort();
        self.flush_pending_provider_world_events();
    }

    fn dispatch_provider_world_event(
        &mut self,
        agent_id: &str,
        event: WorldEvent,
    ) -> Result<(), String> {
        match self.runner.as_mut() {
            #[cfg(not(target_arch = "wasm32"))]
            Some(
                RuntimeDecisionRunner::Builtin(runner)
                | RuntimeDecisionRunner::ProviderBacked(runner),
            ) => runner
                .notify_world_event(agent_id, event)
                .map_err(|error| error.to_string()),
            #[cfg(target_arch = "wasm32")]
            Some(RuntimeDecisionRunner::ProviderBacked(runner)) => runner
                .get_mut(agent_id)
                .map(|agent| agent.behavior.on_event(&event))
                .ok_or_else(|| format!("provider actor unavailable: {agent_id}")),
            #[cfg(target_arch = "wasm32")]
            Some(RuntimeDecisionRunner::Builtin(runner)) => {
                let Some(agent) = runner.get_mut(agent_id) else {
                    return Err(format!("builtin actor unavailable: {agent_id}"));
                };
                agent.behavior.on_event(&event);
                Ok(())
            }
            None => Err("provider runner not initialized".to_string()),
        }
    }

    pub(super) fn flush_pending_provider_world_events(&mut self) {
        let pending = self
            .pending_provider_world_events
            .iter()
            .map(|(key, pending)| (key.clone(), pending.clone()))
            .collect::<Vec<_>>();
        let mut delivered = false;
        for (key, pending) in pending {
            if let (Some(saved), Some(current)) = (
                pending.runtime_binding.as_ref(),
                self.provider_lineage_binding.as_ref(),
            ) && saved != current
            {
                self.pending_provider_world_events.remove(key.as_str());
                self.provider_world_event_quarantine.insert(
                    key,
                    "runtime_binding_replaced_before_provider_delivery".to_string(),
                );
                delivered = true;
                continue;
            }
            let agent_id = pending.agent_id;
            let event = pending.event;
            match self.dispatch_provider_world_event(agent_id.as_str(), event) {
                Ok(()) => {
                    self.pending_provider_world_events.remove(key.as_str());
                    delivered = true;
                }
                Err(error) => {
                    if provider_event_actor_missing(error.as_str(), agent_id.as_str()) {
                        self.pending_provider_world_events.remove(key.as_str());
                        self.provider_world_event_quarantine
                            .insert(key, format!("provider_actor_missing:{agent_id}"));
                        delivered = true;
                        tracing::warn!(
                            agent_id,
                            error,
                            "provider world event quarantined because its actor is missing"
                        );
                    } else {
                        tracing::warn!(
                            agent_id,
                            error,
                            "provider world event delivery deferred for retry"
                        );
                    }
                }
            }
        }
        if delivered {
            self.persist_provider_lineage_best_effort();
        }
    }
}
