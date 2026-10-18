use super::super::{
    AgentIntentReplayDisposition, AgentIntentV2, CausedBy, DomainEvent, WorldError, WorldEventBody,
    WorldEventId,
};
use super::World;
use oasis7_agent_api::intent_summary::canonical_agent_intent_summary;

const STATUS_PROPOSED: &str = "proposed";
const STATUS_SUBMITTED: &str = "submitted";
const STATUS_ACCEPTED: &str = "accepted";
const STATUS_BLOCKED: &str = "blocked";
const STATUS_COMPLETED: &str = "completed";
const STATUS_REJECTED: &str = "rejected";
const STATUS_EXPIRED: &str = "expired";
const STATUS_CANCELLED: &str = "cancelled";
#[cfg(test)]
const SOURCE_PROVIDER_ADVISORY: &str = "provider_advisory";

fn invalid_intent(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: format!(
            "invalid AgentIntentV2 terminal transition: {}",
            reason.into()
        ),
    }
}

fn terminal_summary(intent: &AgentIntentV2, status: &str) -> Result<String, WorldError> {
    canonical_agent_intent_summary(&intent.kind, &intent.source, status)
        .map(|summary| summary.text)
        .map_err(|error| invalid_intent(error.to_string()))
}

fn validate_exact_identity(
    world: &World,
    agent_id: &str,
    intent_id: &str,
    request_digest: &str,
) -> Result<AgentIntentV2, WorldError> {
    let agent_id = agent_id.trim();
    let intent_id = intent_id.trim();
    let request_digest = request_digest.trim();
    if agent_id.is_empty() || intent_id.is_empty() || request_digest.is_empty() {
        return Err(invalid_intent(
            "terminal transition requires agent_id, intent_id, and request_digest",
        ));
    }
    let Some(recorded) = world.state.agent_intent_ledger.get(intent_id) else {
        return Err(invalid_intent(format!(
            "terminal transition target {} is not in the durable ledger",
            intent_id
        )));
    };
    if recorded.agent_id != agent_id || recorded.request_digest != request_digest {
        return Err(invalid_intent(format!(
            "terminal transition target {} conflicts with its durable identity",
            intent_id
        )));
    }
    let Some(current) = world
        .state
        .agents
        .get(agent_id)
        .and_then(|cell| cell.intent.clone())
    else {
        return Err(invalid_intent(format!(
            "terminal transition target {} has no current Agent Intent",
            intent_id
        )));
    };
    if current.intent_id != intent_id {
        return Err(invalid_intent(format!(
            "terminal transition target {} is not the current Agent Intent",
            intent_id
        )));
    }
    Ok(current)
}

impl World {
    /// Persist provider output as a non-authoritative proposed suggestion.
    ///
    /// This path deliberately creates only `AgentIntentProposed`; provider
    /// output cannot submit, accept, or complete an intent on its own.
    #[cfg(test)]
    pub(crate) fn record_provider_advisory_proposed(
        &mut self,
        agent_id: &str,
        intent_id: &str,
        kind: &str,
        target_id: Option<String>,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        let agent_id = agent_id.trim();
        let intent_id = intent_id.trim();
        let kind = kind.trim();
        if agent_id.is_empty() || intent_id.is_empty() || kind.is_empty() {
            return Err(invalid_intent(
                "provider advisory requires agent_id, intent_id, and kind",
            ));
        }
        if !self.state.agents.contains_key(agent_id) {
            return Err(WorldError::AgentNotFound {
                agent_id: agent_id.to_string(),
            });
        }
        if let Some(recorded) = self.state.agent_intent_ledger.get(intent_id) {
            if recorded.agent_id == agent_id
                && recorded.kind == kind
                && recorded.source == SOURCE_PROVIDER_ADVISORY
                && recorded.status == STATUS_PROPOSED
                && recorded.target_id == target_id
            {
                return Ok(recorded.into());
            }
            return Err(invalid_intent(format!(
                "provider advisory {} conflicts with its durable identity",
                intent_id
            )));
        }
        let event_seq = self.next_event_id.max(1);
        let proposal = AgentIntentV2 {
            schema_version: super::super::AGENT_INTENT_V2_SCHEMA_VERSION,
            agent_id: agent_id.to_string(),
            intent_id: intent_id.to_string(),
            kind: kind.to_string(),
            summary: "Agent guidance is proposed and not yet accepted.".to_string(),
            target_id,
            effect_intent_id: None,
            intent_tick: None,
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            status: STATUS_PROPOSED.to_string(),
            source: SOURCE_PROVIDER_ADVISORY.to_string(),
            logical_time: self.state.time,
            event_seq,
            updated_at: self.state.time,
            receipt_ref: None,
            reason_code: None,
            reason_summary: None,
            replaced_by: None,
            actor_id: String::new(),
            request_digest: String::new(),
        };
        let actual = self.append_event(
            WorldEventBody::Domain(DomainEvent::AgentIntentProposed { intent: proposal }),
            None,
        )?;
        if actual != event_seq {
            return Err(invalid_intent(format!(
                "provider advisory event sequence drifted: expected {}, got {}",
                event_seq, actual
            )));
        }
        self.state
            .agent_intent_ledger
            .get(intent_id)
            .map(AgentIntentReplayDisposition::from)
            .ok_or_else(|| invalid_intent("provider advisory was not persisted"))
    }

    fn transition_agent_intent_terminal_exact(
        &mut self,
        agent_id: &str,
        intent_id: &str,
        request_digest: &str,
        status: &str,
        reason: Option<(&str, &str)>,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        let current = validate_exact_identity(self, agent_id, intent_id, request_digest)?;
        let allowed = matches!(
            (current.status.as_str(), status),
            (STATUS_PROPOSED, STATUS_REJECTED | STATUS_EXPIRED)
                | (
                    STATUS_SUBMITTED,
                    STATUS_REJECTED | STATUS_EXPIRED | STATUS_CANCELLED
                )
                | (
                    STATUS_ACCEPTED,
                    STATUS_REJECTED | STATUS_EXPIRED | STATUS_CANCELLED | STATUS_BLOCKED
                )
                | (
                    STATUS_BLOCKED,
                    STATUS_REJECTED | STATUS_EXPIRED | STATUS_CANCELLED
                )
        );
        if !allowed {
            if current.status == status {
                return Ok((&current).into());
            }
            return Err(invalid_intent(format!(
                "unsupported terminal transition {} -> {}",
                current.status, status
            )));
        }

        let event_seq = self.next_event_id.max(1);
        let mut transitioned = current;
        transitioned.status = status.to_string();
        transitioned.summary = terminal_summary(&transitioned, status)?;
        transitioned.event_seq = event_seq;
        transitioned.updated_at = self.state.time;
        transitioned.reason_code = reason.map(|(code, _)| code.to_string());
        transitioned.reason_summary = reason.map(|(_, summary)| summary.to_string());
        let actual = self.append_event(
            WorldEventBody::Domain(DomainEvent::AgentIntentTransitioned {
                intent: transitioned,
            }),
            None,
        )?;
        if actual != event_seq {
            return Err(invalid_intent(format!(
                "terminal transition event sequence drifted: expected {}, got {}",
                event_seq, actual
            )));
        }
        self.state
            .agent_intent_ledger
            .get(intent_id)
            .map(AgentIntentReplayDisposition::from)
            .ok_or_else(|| invalid_intent("terminal disposition was not persisted"))
    }

    /// Persist expiry for an exact intent before execution or completion.
    #[cfg(test)]
    pub(crate) fn expire_agent_intent_exact(
        &mut self,
        agent_id: &str,
        intent_id: &str,
        request_digest: &str,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        self.transition_agent_intent_terminal_exact(
            agent_id,
            intent_id,
            request_digest,
            STATUS_EXPIRED,
            None,
        )
    }

    /// Persist cancellation for an exact submitted/accepted/blocked intent.
    #[cfg(test)]
    pub(crate) fn cancel_agent_intent_exact(
        &mut self,
        agent_id: &str,
        intent_id: &str,
        request_digest: &str,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        self.transition_agent_intent_terminal_exact(
            agent_id,
            intent_id,
            request_digest,
            STATUS_CANCELLED,
            None,
        )
    }

    /// Runtime-owned authority/control terminal boundary. Does not publish an
    /// effect receipt or modify a prior committed result.
    pub(super) fn terminate_agent_authority_intent_exact(
        &mut self,
        agent_id: &str,
        intent_id: &str,
        request_digest: &str,
        status: &str,
        reason: &str,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        if !matches!(status, STATUS_CANCELLED | STATUS_REJECTED | STATUS_BLOCKED) {
            return Err(invalid_intent("authority disposition is invalid"));
        }
        self.transition_agent_intent_terminal_exact(
            agent_id,
            intent_id,
            request_digest,
            status,
            Some((
                reason,
                "Pending instruction requires replanning before execution.",
            )),
        )
    }

    /// Complete an accepted intent only after a committed receipt for its exact effect.
    ///
    /// The receipt event remains in the canonical journal and is bound to the
    /// intent's effect identity before the completion event is appended. The
    /// intent's world/reorg fields form the remaining authority tuple carried
    /// by the completion projection.
    pub(crate) fn complete_agent_intent_with_receipt_exact(
        &mut self,
        agent_id: &str,
        intent_id: &str,
        request_digest: &str,
        receipt_event_id: WorldEventId,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        let current = validate_exact_identity(self, agent_id, intent_id, request_digest)?;
        if current.status == STATUS_COMPLETED {
            return Ok((&current).into());
        }
        if current.status != STATUS_ACCEPTED {
            return Err(invalid_intent(format!(
                "only accepted intent can complete, got {}",
                current.status
            )));
        }
        if current
            .world_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
            || current.reorg_epoch.is_none()
            || current
                .authority_scope
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
        {
            return Err(invalid_intent(
                "completed intent requires the full world_id, reorg_epoch, and authority_scope tuple",
            ));
        }
        let expected_effect_intent_id = current
            .effect_intent_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| invalid_intent("completed intent requires a bound effect_intent_id"))?;
        if receipt_event_id == 0 || receipt_event_id >= self.next_event_id {
            return Err(invalid_intent(
                "receipt event must be a prior committed world event",
            ));
        }
        let Some(receipt_event) = self.journal.events.iter().find(|event| {
            event.id == receipt_event_id
                && matches!(
                        &event.body,
                    WorldEventBody::ReceiptAppended(receipt)
                        if receipt.identity_key() == expected_effect_intent_id
                )
        }) else {
            return Err(invalid_intent(format!(
                "receipt event {} is not committed for effect intent {}",
                receipt_event_id, expected_effect_intent_id
            )));
        };
        if receipt_event.time > self.state.time {
            return Err(invalid_intent(
                "receipt event cannot be later than current world time",
            ));
        }

        let event_seq = self.next_event_id.max(1);
        let mut completed = current;
        completed.status = STATUS_COMPLETED.to_string();
        completed.summary = terminal_summary(&completed, STATUS_COMPLETED)?;
        completed.event_seq = event_seq;
        completed.updated_at = self.state.time;
        completed.receipt_ref = Some(format!("world-event:{receipt_event_id}"));
        completed.reason_code = None;
        completed.reason_summary = None;
        let actual = self.append_event(
            WorldEventBody::Domain(DomainEvent::AgentIntentTransitioned { intent: completed }),
            Some(CausedBy::Effect {
                intent_id: expected_effect_intent_id,
            }),
        )?;
        if actual != event_seq {
            return Err(invalid_intent(format!(
                "completion event sequence drifted: expected {}, got {}",
                event_seq, actual
            )));
        }
        self.state
            .agent_intent_ledger
            .get(intent_id)
            .map(AgentIntentReplayDisposition::from)
            .ok_or_else(|| invalid_intent("completed disposition was not persisted"))
    }
}
