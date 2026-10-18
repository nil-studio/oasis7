use super::super::util::sha256_hex;
use super::super::{
    AGENT_INTENT_V2_SCHEMA_VERSION, AgentIntentAuthorityContext, AgentIntentReplayDisposition,
    AgentIntentV2, DomainEvent, WorldError, WorldEventBody, WorldEventId,
};
use super::World;
use oasis7_agent_api::intent_summary::canonical_agent_intent_summary;

const AGENT_CHAT_INTENT_KIND: &str = "agent_chat";
const AGENT_CHAT_INTENT_SOURCE: &str = "player";
const AGENT_INTENT_STATUS_ACCEPTED: &str = "accepted";
const AGENT_INTENT_STATUS_BLOCKED: &str = "blocked";
const AGENT_INTENT_STATUS_REJECTED: &str = "rejected";
const AGENT_INTENT_STATUS_SUPERSEDED: &str = "superseded";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentIntentProviderFailureDisposition {
    Blocked,
    Rejected,
}

impl AgentIntentProviderFailureDisposition {
    fn status(self) -> &'static str {
        match self {
            Self::Blocked => AGENT_INTENT_STATUS_BLOCKED,
            Self::Rejected => AGENT_INTENT_STATUS_REJECTED,
        }
    }

    fn reason_code(self) -> &'static str {
        match self {
            Self::Blocked => "provider_unavailable",
            Self::Rejected => "provider_rejected",
        }
    }

    fn reason_summary(self) -> &'static str {
        match self {
            Self::Blocked => "Agent service is temporarily unavailable",
            Self::Rejected => "Agent service rejected this instruction",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentIntentRecordOutcome {
    Accepted { event_seq: WorldEventId },
    Replayed { event_seq: WorldEventId },
}

impl AgentIntentRecordOutcome {
    pub fn event_seq(self) -> WorldEventId {
        match self {
            Self::Accepted { event_seq } | Self::Replayed { event_seq } => event_seq,
        }
    }

    pub fn is_replay(self) -> bool {
        matches!(self, Self::Replayed { .. })
    }
}

fn invalid_agent_intent(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: format!("invalid AgentIntentV2 handoff: {}", reason.into()),
    }
}

fn append_canonical_bytes(payload: &mut Vec<u8>, value: &[u8]) {
    let length = u64::try_from(value.len()).expect("canonical field length must fit in u64");
    payload.extend_from_slice(&length.to_be_bytes());
    payload.extend_from_slice(value);
}

fn append_canonical_str(payload: &mut Vec<u8>, value: &str) {
    append_canonical_bytes(payload, value.as_bytes());
}

fn append_canonical_u64(payload: &mut Vec<u8>, value: u64) {
    payload.extend_from_slice(&value.to_be_bytes());
}

fn append_canonical_optional_str(payload: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            payload.push(1);
            append_canonical_str(payload, value);
        }
        None => payload.push(0),
    }
}

fn append_canonical_optional_u64(payload: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            payload.push(1);
            append_canonical_u64(payload, value);
        }
        None => payload.push(0),
    }
}

fn derive_agent_chat_intent_id(player_id: &str, agent_id: &str, intent_seq: u64) -> String {
    let mut payload = b"agent-chat-intent-v2\0".to_vec();
    append_canonical_str(&mut payload, player_id);
    append_canonical_str(&mut payload, agent_id);
    append_canonical_u64(&mut payload, intent_seq);
    format!("agent-intent-v2:{}", sha256_hex(&payload))
}

/// Derive the canonical authenticated Agent Chat request binding.
///
/// Runtime cognition treats this value as an Agent-owned opaque digest; this
/// helper is exposed within the crate so runtime fixtures and adjacent
/// boundaries can construct the same durable intent binding without dummy
/// values or a second hash formula.
pub(crate) fn derive_agent_chat_request_digest(
    player_id: &str,
    agent_id: &str,
    intent_seq: u64,
    message: &str,
    authority: &AgentIntentAuthorityContext,
) -> String {
    let mut payload = b"agent-chat-request-v2\0".to_vec();
    append_canonical_str(&mut payload, player_id);
    append_canonical_str(&mut payload, agent_id);
    append_canonical_u64(&mut payload, intent_seq);
    append_canonical_optional_u64(&mut payload, authority.intent_tick);
    append_canonical_optional_str(&mut payload, authority.world_id.as_deref());
    append_canonical_optional_u64(&mut payload, authority.reorg_epoch);
    append_canonical_optional_str(&mut payload, authority.authority_scope.as_deref());
    append_canonical_optional_str(&mut payload, authority.replaces_intent_id.as_deref());
    append_canonical_str(&mut payload, message);
    sha256_hex(&payload)
}

fn normalize_authority_context(
    mut authority: AgentIntentAuthorityContext,
) -> Result<AgentIntentAuthorityContext, WorldError> {
    if authority.intent_tick == Some(0) {
        return Err(invalid_agent_intent(
            "intent_tick must be greater than zero when supplied",
        ));
    }
    let has_authority_tuple_field = authority.world_id.is_some()
        || authority.reorg_epoch.is_some()
        || authority.authority_scope.is_some();
    let has_complete_authority_tuple = authority.world_id.is_some()
        && authority.reorg_epoch.is_some()
        && authority.authority_scope.is_some();
    if has_authority_tuple_field && !has_complete_authority_tuple {
        return Err(invalid_agent_intent(
            "authority context must include world_id, reorg_epoch, and authority_scope together",
        ));
    }
    authority.world_id = authority
        .world_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    authority.authority_scope = authority
        .authority_scope
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if authority.world_id.is_some() != has_authority_tuple_field
        || authority.authority_scope.is_some() != has_authority_tuple_field
    {
        return Err(invalid_agent_intent(
            "authority context fields cannot be empty",
        ));
    }
    authority.replaces_intent_id = authority
        .replaces_intent_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    Ok(authority)
}

impl World {
    /// Resolve the durable disposition for an exact authenticated chat retry.
    ///
    /// A sidecar ACK is only a transport cache. This lookup binds the retry to
    /// the same canonical digest and authority position before a caller
    /// decides whether it can replay an ACK.
    ///
    /// Direct Agent Intent mutations are intentionally kept inside the trusted
    /// runtime/viewer boundary. External clients must use the authenticated
    /// Viewer Agent Chat path instead of mutating runtime state directly.
    /// ```compile_fail
    /// fn external_bypass(
    ///     world: &mut oasis7_runtime::World,
    ///     authority: oasis7_runtime::AgentIntentAuthorityContext,
    /// ) {
    ///     let _ = world.transition_agent_chat_provider_failure_exact(
    ///         "agent", "intent", "digest", oasis7_runtime::AgentIntentProviderFailureDisposition::Blocked,
    ///     );
    ///     let _ = world.reject_agent_intent_exact("agent", "intent", "digest");
    ///     let _ = world.expire_agent_intent_exact("agent", "intent", "digest");
    ///     let _ = world.cancel_agent_intent_exact("agent", "intent", "digest");
    ///     let _ = world.record_provider_advisory_proposed(
    ///         "agent", "advisory", "kind", None,
    ///     );
    ///     let _ = world.complete_agent_intent_with_receipt_exact(
    ///         "agent", "intent", "digest", 1,
    ///     );
    ///     let _ = authority;
    /// }
    /// ```
    pub fn agent_chat_intent_replay_disposition(
        &self,
        player_id: &str,
        agent_id: &str,
        intent_seq: u64,
        message: &str,
        authority: AgentIntentAuthorityContext,
    ) -> Result<Option<AgentIntentReplayDisposition>, WorldError> {
        let authority = normalize_authority_context(authority)?;
        let player_id = player_id.trim();
        let agent_id = agent_id.trim();
        let intent_id = derive_agent_chat_intent_id(player_id, agent_id, intent_seq);
        let request_digest = derive_agent_chat_request_digest(
            player_id,
            agent_id,
            intent_seq,
            message.trim(),
            &authority,
        );
        let Some(recorded) = self.agent_intent_record_for_replay(&intent_id)? else {
            return Ok(None);
        };
        if recorded.actor_id == player_id
            && recorded.agent_id == agent_id
            && recorded.kind == AGENT_CHAT_INTENT_KIND
            && recorded.source == AGENT_CHAT_INTENT_SOURCE
            && recorded.request_digest == request_digest
            && recorded.intent_tick == authority.intent_tick
            && recorded.world_id == authority.world_id
            && recorded.reorg_epoch == authority.reorg_epoch
            && recorded.authority_scope == authority.authority_scope
        {
            return Ok(Some((&recorded).into()));
        }
        Err(invalid_agent_intent(format!(
            "intent_id {} conflicts with its durable request digest",
            intent_id
        )))
    }

    pub fn verify_agent_chat_intent_replay(
        &self,
        player_id: &str,
        agent_id: &str,
        intent_seq: u64,
        message: &str,
        authority: AgentIntentAuthorityContext,
    ) -> Result<Option<WorldEventId>, WorldError> {
        Ok(self
            .agent_chat_intent_replay_disposition(
                player_id, agent_id, intent_seq, message, authority,
            )?
            .map(|disposition| disposition.event_seq))
    }

    /// Persist a provider failure against the exact accepted Intent that
    /// produced the provider request. Repeated delivery is idempotent.
    pub(crate) fn transition_agent_chat_provider_failure_exact(
        &mut self,
        agent_id: &str,
        intent_id: &str,
        request_digest: &str,
        disposition: AgentIntentProviderFailureDisposition,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        let agent_id = agent_id.trim();
        let intent_id = intent_id.trim();
        let request_digest = request_digest.trim();
        if agent_id.is_empty() || intent_id.is_empty() || request_digest.is_empty() {
            return Err(invalid_agent_intent(
                "provider failure disposition requires agent_id, intent_id, and request_digest",
            ));
        }
        let Some(recorded) = self.state.agent_intent_ledger.get(intent_id).cloned() else {
            return Err(invalid_agent_intent(format!(
                "provider failure target {} is not in the durable ledger",
                intent_id
            )));
        };
        if recorded.agent_id != agent_id || recorded.request_digest != request_digest {
            return Err(invalid_agent_intent(format!(
                "provider failure target {} conflicts with its durable identity",
                intent_id
            )));
        }
        if recorded.status == disposition.status() {
            return Ok((&recorded).into());
        }
        if recorded.status != AGENT_INTENT_STATUS_ACCEPTED {
            return Err(invalid_agent_intent(format!(
                "provider failure target {} is already {}",
                intent_id, recorded.status
            )));
        }
        let Some(current) = self
            .state
            .agents
            .get(agent_id)
            .and_then(|cell| cell.intent.clone())
        else {
            return Err(invalid_agent_intent(format!(
                "provider failure target {} has no current Agent Intent",
                intent_id
            )));
        };
        if current.intent_id != intent_id {
            return Err(invalid_agent_intent(format!(
                "provider failure target {} is not the current Agent Intent",
                intent_id
            )));
        }
        let event_seq = self.next_event_id.max(1);
        let mut transitioned = current;
        transitioned.status = disposition.status().to_string();
        transitioned.summary = canonical_agent_intent_summary(
            AGENT_CHAT_INTENT_KIND,
            AGENT_CHAT_INTENT_SOURCE,
            disposition.status(),
        )
        .map(|summary| summary.text)
        .map_err(|error| invalid_agent_intent(error.to_string()))?;
        transitioned.event_seq = event_seq;
        transitioned.updated_at = self.state.time;
        transitioned.reason_code = Some(disposition.reason_code().to_string());
        transitioned.reason_summary = Some(disposition.reason_summary().to_string());
        let actual = self.append_event(
            WorldEventBody::Domain(DomainEvent::AgentIntentTransitioned {
                intent: transitioned,
            }),
            None,
        )?;
        if actual != event_seq {
            return Err(invalid_agent_intent(format!(
                "provider failure disposition event sequence drifted: expected {}, got {}",
                event_seq, actual
            )));
        }
        self.state
            .agent_intent_ledger
            .get(intent_id)
            .cloned()
            .map(|intent| (&intent).into())
            .ok_or_else(|| invalid_agent_intent("provider failure disposition was not persisted"))
    }

    /// Append a player-authenticated agent-chat intent to the canonical runtime journal.
    ///
    /// Authentication, authorization, nonce consumption, and provider enqueueing remain
    /// outside this kernel method.  The caller supplies the already verified player identity
    /// and intent sequence; the runtime owns logical time and journal event sequencing.
    #[cfg(test)]
    pub(crate) fn record_agent_chat_intent(
        &mut self,
        player_id: &str,
        agent_id: &str,
        intent_seq: u64,
        message: &str,
    ) -> Result<WorldEventId, WorldError> {
        Ok(self
            .record_agent_chat_intent_with_authority(
                player_id,
                agent_id,
                intent_seq,
                message,
                AgentIntentAuthorityContext::default(),
            )?
            .event_seq())
    }

    /// Append a player-authenticated agent-chat intent with the authority
    /// identity that must survive durable retry/replay.
    #[expect(
        clippy::collapsible_if,
        reason = "Retry idempotency first matches the intent key, then validates the complete durable payload."
    )]
    pub(crate) fn record_agent_chat_intent_with_authority(
        &mut self,
        player_id: &str,
        agent_id: &str,
        intent_seq: u64,
        message: &str,
        authority: AgentIntentAuthorityContext,
    ) -> Result<AgentIntentRecordOutcome, WorldError> {
        let player_id = player_id.trim();
        let agent_id = agent_id.trim();
        let message = message.trim();
        let authority = normalize_authority_context(authority)?;
        if player_id.is_empty() {
            return Err(invalid_agent_intent("player_id cannot be empty"));
        }
        if agent_id.is_empty() {
            return Err(invalid_agent_intent("agent_id cannot be empty"));
        }
        if intent_seq == 0 {
            return Err(invalid_agent_intent("intent_seq must be greater than zero"));
        }
        if message.is_empty() {
            return Err(invalid_agent_intent("summary cannot be empty"));
        }
        if message.chars().count() > 512 {
            return Err(invalid_agent_intent("summary exceeds 512 characters"));
        }
        if message
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(invalid_agent_intent("summary contains a control character"));
        }
        let lifecycle_summary = |status| {
            canonical_agent_intent_summary(AGENT_CHAT_INTENT_KIND, AGENT_CHAT_INTENT_SOURCE, status)
                .map(|summary| summary.text)
                .map_err(|error| invalid_agent_intent(error.to_string()))
        };
        let player_summary = lifecycle_summary(AGENT_INTENT_STATUS_ACCEPTED)?;
        if !self.state.agents.contains_key(agent_id) {
            return Err(WorldError::AgentNotFound {
                agent_id: agent_id.to_string(),
            });
        }

        let intent_id = derive_agent_chat_intent_id(player_id, agent_id, intent_seq);
        let request_digest =
            derive_agent_chat_request_digest(player_id, agent_id, intent_seq, message, &authority);
        if let Some(recorded) = self.agent_intent_record_for_replay(&intent_id)? {
            if recorded.actor_id == player_id
                && recorded.request_digest == request_digest
                && recorded.agent_id == agent_id
                && recorded.kind == AGENT_CHAT_INTENT_KIND
                && recorded.source == AGENT_CHAT_INTENT_SOURCE
                && recorded.intent_tick == authority.intent_tick
                && recorded.world_id == authority.world_id
                && recorded.reorg_epoch == authority.reorg_epoch
                && recorded.authority_scope == authority.authority_scope
            {
                return Ok(AgentIntentRecordOutcome::Replayed {
                    event_seq: recorded.event_seq,
                });
            }
            return Err(invalid_agent_intent(format!(
                "intent_id {} conflicts with its durable request digest",
                intent_id
            )));
        }
        let now = self.state.time;
        let current = self
            .state
            .agents
            .get(agent_id)
            .and_then(|cell| cell.intent.clone());

        if let Some(existing) = current.as_ref() {
            if existing.intent_id == intent_id {
                if existing.schema_version == AGENT_INTENT_V2_SCHEMA_VERSION
                    && existing.agent_id == agent_id
                    && existing.kind == AGENT_CHAT_INTENT_KIND
                    && existing.summary == player_summary
                    && existing.target_id.is_none()
                    && existing.status == AGENT_INTENT_STATUS_ACCEPTED
                    && existing.source == AGENT_CHAT_INTENT_SOURCE
                    && existing.intent_tick == authority.intent_tick
                    && existing.world_id == authority.world_id
                    && existing.reorg_epoch == authority.reorg_epoch
                    && existing.authority_scope == authority.authority_scope
                    && existing.receipt_ref.is_none()
                    && existing.reason_code.is_none()
                    && existing.reason_summary.is_none()
                    && existing.replaced_by.is_none()
                    && (existing.actor_id.is_empty() || existing.actor_id == player_id)
                    && (existing.request_digest.is_empty()
                        || existing.request_digest == request_digest)
                {
                    // The canonical event is already present.  Returning its recorded journal
                    // position keeps a retry idempotent even when the sidecar ack was lost.
                    return Ok(AgentIntentRecordOutcome::Replayed {
                        event_seq: existing.event_seq,
                    });
                }
                return Err(invalid_agent_intent(format!(
                    "intent_id {} conflicts with its recorded payload",
                    intent_id
                )));
            }
        }

        // An active prior intent is explicitly superseded before the new accepted intent is
        // appended. Terminal dispositions are already closed and can be replaced directly.
        if let Some(previous) = current {
            let terminal = matches!(
                previous.status.as_str(),
                "completed" | "rejected" | "expired" | "cancelled" | "superseded"
            );
            if !terminal {
                if authority.replaces_intent_id.as_deref() != Some(previous.intent_id.as_str()) {
                    return Err(invalid_agent_intent(format!(
                        "active intent {} requires an explicit matching replaces_intent_id",
                        previous.intent_id
                    )));
                }
                if matches!(previous.status.as_str(), "proposed" | "submitted") {
                    return Err(invalid_agent_intent(format!(
                        "intent {} cannot be superseded before acceptance",
                        previous.intent_id
                    )));
                }
                let replacement_event_seq = self.next_event_id.max(1);
                let replacement = AgentIntentV2 {
                    schema_version: AGENT_INTENT_V2_SCHEMA_VERSION,
                    agent_id: previous.agent_id,
                    intent_id: previous.intent_id,
                    kind: previous.kind,
                    summary: lifecycle_summary(AGENT_INTENT_STATUS_SUPERSEDED)?,
                    target_id: previous.target_id,
                    effect_intent_id: previous.effect_intent_id,
                    intent_tick: previous.intent_tick,
                    world_id: previous.world_id,
                    reorg_epoch: previous.reorg_epoch,
                    authority_scope: previous.authority_scope,
                    status: AGENT_INTENT_STATUS_SUPERSEDED.to_string(),
                    source: previous.source,
                    logical_time: previous.logical_time,
                    event_seq: replacement_event_seq,
                    updated_at: now,
                    receipt_ref: previous.receipt_ref,
                    reason_code: previous.reason_code,
                    reason_summary: previous.reason_summary,
                    replaced_by: Some(intent_id.clone()),
                    actor_id: previous.actor_id,
                    request_digest: previous.request_digest,
                };
                let actual_event_seq = self.append_event(
                    WorldEventBody::Domain(DomainEvent::AgentIntentReplaced {
                        intent: replacement,
                    }),
                    None,
                )?;
                if actual_event_seq != replacement_event_seq {
                    return Err(invalid_agent_intent(format!(
                        "replacement event sequence drifted: expected {}, got {}",
                        replacement_event_seq, actual_event_seq
                    )));
                }
            } else if authority.replaces_intent_id.is_some() {
                return Err(invalid_agent_intent(
                    "replaces_intent_id cannot target a terminal intent",
                ));
            }
        } else if authority.replaces_intent_id.is_some() {
            return Err(invalid_agent_intent(
                "replaces_intent_id requires a current active intent",
            ));
        }

        let proposed_event_seq = self.next_event_id.max(1);
        let proposed = AgentIntentV2 {
            schema_version: AGENT_INTENT_V2_SCHEMA_VERSION,
            agent_id: agent_id.to_string(),
            intent_id: intent_id.clone(),
            kind: AGENT_CHAT_INTENT_KIND.to_string(),
            summary: lifecycle_summary("proposed")?,
            target_id: None,
            effect_intent_id: None,
            intent_tick: authority.intent_tick,
            world_id: authority.world_id,
            reorg_epoch: authority.reorg_epoch,
            authority_scope: authority.authority_scope,
            status: "proposed".to_string(),
            source: AGENT_CHAT_INTENT_SOURCE.to_string(),
            logical_time: now,
            event_seq: proposed_event_seq,
            updated_at: now,
            receipt_ref: None,
            reason_code: None,
            reason_summary: None,
            replaced_by: None,
            actor_id: player_id.to_string(),
            request_digest: request_digest.clone(),
        };
        let actual_event_seq = self.append_event(
            WorldEventBody::Domain(DomainEvent::AgentIntentProposed {
                intent: proposed.clone(),
            }),
            None,
        )?;
        if actual_event_seq != proposed_event_seq {
            return Err(invalid_agent_intent(format!(
                "proposed event sequence drifted: expected {}, got {}",
                proposed_event_seq, actual_event_seq
            )));
        }
        let submitted_event_seq = self.next_event_id.max(1);
        let mut submitted = proposed;
        submitted.status = "submitted".to_string();
        submitted.summary = lifecycle_summary("submitted")?;
        submitted.event_seq = submitted_event_seq;
        submitted.updated_at = now;
        let actual_event_seq = self.append_event(
            WorldEventBody::Domain(DomainEvent::AgentIntentSubmitted {
                intent: submitted.clone(),
            }),
            None,
        )?;
        if actual_event_seq != submitted_event_seq {
            return Err(invalid_agent_intent(format!(
                "submitted event sequence drifted: expected {}, got {}",
                submitted_event_seq, actual_event_seq
            )));
        }
        let accepted_event_seq = self.next_event_id.max(1);
        let mut accepted = submitted;
        accepted.status = AGENT_INTENT_STATUS_ACCEPTED.to_string();
        accepted.summary = player_summary;
        accepted.event_seq = accepted_event_seq;
        accepted.updated_at = now;
        let actual_event_seq = self.append_event(
            WorldEventBody::Domain(DomainEvent::AgentIntentAccepted { intent: accepted }),
            None,
        )?;
        if actual_event_seq != accepted_event_seq {
            return Err(invalid_agent_intent(format!(
                "accepted event sequence drifted: expected {}, got {}",
                accepted_event_seq, actual_event_seq
            )));
        }
        Ok(AgentIntentRecordOutcome::Accepted {
            event_seq: actual_event_seq,
        })
    }

    fn agent_intent_record_for_replay(
        &self,
        intent_id: &str,
    ) -> Result<Option<AgentIntentV2>, WorldError> {
        if let Some(recorded) = self.state.agent_intent_ledger.get(intent_id) {
            return Ok(Some(recorded.clone()));
        }
        if self.state.retained_history_root.is_some() {
            return Ok(self
                .read_retained_agent_intent(intent_id)?
                .map(|record| record.intent));
        }
        Ok(None)
    }
}
