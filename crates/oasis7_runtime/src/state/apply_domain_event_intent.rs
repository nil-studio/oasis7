use super::*;

use crate::{AGENT_INTENT_V2_SCHEMA_VERSION, AgentIntentV2};
use oasis7_agent_api::intent_summary::canonical_agent_intent_summary;

const STATUS_PROPOSED: &str = "proposed";
const STATUS_SUBMITTED: &str = "submitted";
const STATUS_ACCEPTED: &str = "accepted";
const STATUS_BLOCKED: &str = "blocked";
const STATUS_COMPLETED: &str = "completed";
const STATUS_SUPERSEDED: &str = "superseded";
const SOURCE_PROVIDER_ADVISORY: &str = "provider_advisory";
const MAX_AGENT_INTENT_SUMMARY_CHARS: usize = 512;
const TERMINAL_STATUSES: &[&str] = &[
    "completed",
    "rejected",
    "expired",
    "cancelled",
    STATUS_SUPERSEDED,
];

struct IntentLedgerProjection<'a> {
    base: &'a BTreeMap<String, AgentIntentV2>,
    updates: BTreeMap<String, AgentIntentV2>,
}

impl IntentLedgerProjection<'_> {
    fn get(&self, intent_id: &str) -> Option<&AgentIntentV2> {
        self.updates
            .get(intent_id)
            .or_else(|| self.base.get(intent_id))
    }

    fn insert(&mut self, intent_id: String, intent: AgentIntentV2) {
        self.updates.insert(intent_id, intent);
    }
}

fn invalid_intent(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: format!("invalid AgentIntentV2: {}", reason.into()),
    }
}

fn validate_common_intent(intent: &AgentIntentV2) -> Result<(), WorldError> {
    if intent.schema_version != AGENT_INTENT_V2_SCHEMA_VERSION {
        return Err(invalid_intent(format!(
            "unsupported schema_version {}, expected {}",
            intent.schema_version, AGENT_INTENT_V2_SCHEMA_VERSION
        )));
    }
    if intent.agent_id.trim().is_empty() {
        return Err(invalid_intent("agent_id cannot be empty"));
    }
    if intent.intent_id.trim().is_empty() {
        return Err(invalid_intent("intent_id cannot be empty"));
    }
    if intent.kind.trim().is_empty() {
        return Err(invalid_intent("kind cannot be empty"));
    }
    if intent.summary.trim().is_empty() {
        return Err(invalid_intent("summary cannot be empty"));
    }
    if intent.summary.chars().count() > MAX_AGENT_INTENT_SUMMARY_CHARS {
        return Err(invalid_intent(format!(
            "summary exceeds {} characters",
            MAX_AGENT_INTENT_SUMMARY_CHARS
        )));
    }
    if intent
        .summary
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(invalid_intent("summary contains a control character"));
    }
    if intent.source.trim().is_empty() {
        return Err(invalid_intent("source cannot be empty"));
    }
    if intent
        .world_id
        .as_deref()
        .is_some_and(|world_id| world_id.trim().is_empty())
    {
        return Err(invalid_intent("world_id cannot be empty when supplied"));
    }
    if intent
        .authority_scope
        .as_deref()
        .is_some_and(|scope| scope.trim().is_empty())
    {
        return Err(invalid_intent(
            "authority_scope cannot be empty when supplied",
        ));
    }
    if intent.status != STATUS_SUPERSEDED && intent.replaced_by.is_some() {
        return Err(invalid_intent(
            "replaced_by is only valid for an explicit superseded intent",
        ));
    }
    if intent.status != STATUS_COMPLETED && intent.receipt_ref.is_some() {
        return Err(invalid_intent(
            "receipt_ref is only valid for a completed intent",
        ));
    }
    if intent.replaced_by.as_deref() == Some(intent.intent_id.as_str()) {
        return Err(invalid_intent("replaced_by cannot equal intent_id"));
    }
    Ok(())
}

fn validate_runtime_acceptance_source(intent: &AgentIntentV2) -> Result<(), WorldError> {
    if intent.status == STATUS_ACCEPTED && intent.source == SOURCE_PROVIDER_ADVISORY {
        return Err(invalid_intent(
            "provider_advisory cannot become a runtime-accepted intent",
        ));
    }
    Ok(())
}

fn validate_player_safe_copy(intent: &AgentIntentV2) -> Result<(), WorldError> {
    let expected = if intent.source == SOURCE_PROVIDER_ADVISORY {
        if intent.status != STATUS_PROPOSED {
            return Err(invalid_intent(
                "provider_advisory may remain proposed but cannot enter another lifecycle state",
            ));
        }
        "Agent guidance is proposed and not yet accepted.".to_string()
    } else {
        canonical_agent_intent_summary(&intent.kind, &intent.source, intent.status.as_str())
            .map_err(|error| invalid_intent(error.to_string()))?
            .text
    };
    if intent.summary != expected {
        return Err(invalid_intent(
            "intent summary must use the canonical player-safe template for its lifecycle state",
        ));
    }
    if let Some(reason_code) = intent.reason_code.as_deref() {
        let expected_reason = match reason_code {
            "insufficient_power" => "Restore power to continue",
            "policy_denied" => "This instruction is not permitted",
            "provider_unavailable" => "Agent service is temporarily unavailable",
            "provider_rejected" => "Agent service rejected this instruction",
            "agency_action_id_invalid_replan_required"
            | "agency_action_unsupported_replan_required"
            | "agency_agent_dissent_replan_required"
            | "agency_agent_missing_replan_required"
            | "agency_asset_source_unsupported_replan_required"
            | "agency_budget_corrupt_replan_required"
            | "agency_budget_exhausted_replan_required"
            | "agency_budget_overflow_replan_required"
            | "agency_cognition_invalid_replan_required"
            | "agency_decision_action_changed_replan_required"
            | "agency_decision_binding_required_replan_required"
            | "agency_decision_idempotency_conflict_replan_required"
            | "agency_decision_identity_mismatch_replan_required"
            | "agency_domain_projection_conflict_replan_required"
            | "agency_domain_quote_invalid_replan_required"
            | "agency_domain_quote_unavailable_replan_required"
            | "agency_domain_quote_unsupported_replan_required"
            | "agency_domain_reference_action_mismatch_replan_required"
            | "agency_domain_reference_invalid_replan_required"
            | "agency_domain_reference_unavailable_replan_required"
            | "agency_grant_binding_required_replan_required"
            | "agency_grant_invalid_replan_required"
            | "agency_grant_missing_replan_required"
            | "agency_grant_or_quote_expired_replan_required"
            | "agency_grant_revision_conflict_replan_required"
            | "agency_grant_revision_stale_replan_required"
            | "agency_grant_revoked_replan_required"
            | "agency_grant_scope_mismatch_replan_required"
            | "agency_grant_selection_ambiguous_replan_required"
            | "agency_intent_authority_changed_replan_required"
            | "agency_intent_missing_replan_required"
            | "agency_intent_replan_required_replan_required"
            | "agency_organization_source_unsupported_replan_required"
            | "agency_override_already_pending_replan_required"
            | "agency_override_authority_stale_replan_required"
            | "agency_override_context_untrusted_replan_required"
            | "agency_override_grant_unavailable_replan_required"
            | "agency_override_owner_mismatch_replan_required"
            | "agency_override_requires_grant_replan_required"
            | "agency_owner_control_denied_replan_required"
            | "agency_owner_control_grant_tuple_invalid_replan_required"
            | "agency_owner_control_idempotency_conflict_replan_required"
            | "agency_owner_control_intent_stale_replan_required"
            | "agency_owner_source_changed_replan_required"
            | "agency_owner_source_unavailable_replan_required"
            | "agency_quote_units_mismatch_replan_required"
            | "agency_receipt_lineage_conflict_replan_required"
            | "agency_trusted_quote_required_replan_required"
            | "owner_interrupted_replan_required" => {
                "Pending instruction requires replanning before execution."
            }
            _ => return Err(invalid_intent("unsupported player-visible reason_code")),
        };
        if intent.reason_summary.as_deref() != Some(expected_reason) {
            return Err(invalid_intent(
                "reason_summary must match the canonical reason_code template",
            ));
        }
    } else if intent.reason_summary.is_some() {
        return Err(invalid_intent(
            "reason_summary requires a canonical reason_code",
        ));
    }
    Ok(())
}

fn validate_transition_time(intent: &AgentIntentV2, now: WorldTime) -> Result<(), WorldError> {
    if intent.updated_at != now {
        return Err(invalid_intent(format!(
            "updated_at {} does not match event time {}",
            intent.updated_at, now
        )));
    }
    if intent.logical_time > intent.updated_at {
        return Err(invalid_intent(
            "logical_time cannot be later than updated_at",
        ));
    }
    Ok(())
}

fn validate_transition_time_after_current(
    intent: &AgentIntentV2,
    now: WorldTime,
    current: &AgentIntentV2,
) -> Result<(), WorldError> {
    validate_transition_time(intent, now)?;
    if intent.updated_at < current.updated_at {
        return Err(invalid_intent(format!(
            "updated_at {} cannot precede current intent timestamp {}",
            intent.updated_at, current.updated_at
        )));
    }
    Ok(())
}

fn validate_receipt_reference(
    intent: &AgentIntentV2,
    committed_receipt_event_id: Option<u64>,
) -> Result<(), WorldError> {
    if intent.status != STATUS_COMPLETED {
        return Ok(());
    }
    let Some(receipt_ref) = intent.receipt_ref.as_deref() else {
        return Err(invalid_intent(
            "completed transition requires a committed receipt_ref",
        ));
    };
    if intent
        .world_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
        || intent.reorg_epoch.is_none()
        || intent
            .authority_scope
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(invalid_intent(
            "completed transition requires the full world_id, reorg_epoch, and authority_scope tuple",
        ));
    }
    if intent
        .effect_intent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err(invalid_intent(
            "completed transition requires a bound effect_intent_id",
        ));
    }
    let Some(event_id) = receipt_ref
        .strip_prefix("world-event:")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|event_id| *event_id > 0)
    else {
        return Err(invalid_intent(
            "completed transition requires a non-zero world-event receipt_ref",
        ));
    };
    let Some(committed_receipt_event_id) = committed_receipt_event_id else {
        return Err(invalid_intent(
            "completed transition requires a canonical committed receipt witness",
        ));
    };
    if committed_receipt_event_id != event_id {
        return Err(invalid_intent(format!(
            "receipt witness {} does not match receipt_ref {}",
            committed_receipt_event_id, event_id
        )));
    }
    Ok(())
}

fn immutable_payload_matches(current: &AgentIntentV2, incoming: &AgentIntentV2) -> bool {
    current.schema_version == incoming.schema_version
        && current.agent_id == incoming.agent_id
        && current.intent_id == incoming.intent_id
        && current.kind == incoming.kind
        && current.target_id == incoming.target_id
        && current.effect_intent_id == incoming.effect_intent_id
        && current.intent_tick == incoming.intent_tick
        && current.world_id == incoming.world_id
        && current.reorg_epoch == incoming.reorg_epoch
        && current.authority_scope == incoming.authority_scope
        && current.source == incoming.source
        && current.logical_time == incoming.logical_time
        && current.actor_id == incoming.actor_id
        && current.request_digest == incoming.request_digest
}

impl WorldState {
    pub(crate) fn prepare_agent_intent_event(
        &self,
        event: &DomainEvent,
        now: WorldTime,
        envelope_event_seq: Option<u64>,
        committed_receipt_event_id: Option<u64>,
    ) -> Result<crate::world::agent_intent_publication::PreparedAgentIntent, WorldError>
    {
        let intent = match event {
            DomainEvent::AgentIntentProposed { intent }
            | DomainEvent::AgentIntentSubmitted { intent }
            | DomainEvent::AgentIntentAccepted { intent }
            | DomainEvent::AgentIntentReplaced { intent }
            | DomainEvent::AgentIntentTransitioned { intent } => intent,
            _ => unreachable!("apply_domain_event_intent received unsupported event"),
        };
        validate_common_intent(intent)?;
        if let Some(envelope_event_seq) = envelope_event_seq
            && intent.event_seq != envelope_event_seq
        {
            return Err(invalid_intent(format!(
                "event_seq {} does not match world event envelope {}",
                intent.event_seq, envelope_event_seq
            )));
        }
        validate_receipt_reference(intent, committed_receipt_event_id)?;
        validate_runtime_acceptance_source(intent)?;
        validate_player_safe_copy(intent)?;

        if !self.agents.contains_key(&intent.agent_id) {
            return Err(WorldError::AgentNotFound {
                agent_id: intent.agent_id.clone(),
            });
        }

        let mut cell = self
            .agents
            .get(&intent.agent_id)
            .expect("agent existence checked immediately above")
            .clone();
        let existing = cell.intent.clone();
        let mut ledger = IntentLedgerProjection {
            base: &self.agent_intent_ledger,
            updates: BTreeMap::new(),
        };
        macro_rules! finish {
            () => {
                crate::world::agent_intent_publication::PreparedAgentIntent {
                    event: event.clone(),
                    agent_id: intent.agent_id.clone(),
                    agent: cell,
                    ledger_updates: ledger.updates,
                }
            };
        }

        match event {
            DomainEvent::AgentIntentProposed { .. } => {
                if intent.status != STATUS_PROPOSED {
                    return Err(invalid_intent(format!(
                        "proposed event must carry status {STATUS_PROPOSED}, got {}",
                        intent.status
                    )));
                }

                // Provider guidance is durable audit data, not the current
                // authoritative intent. Keeping it out of the single active
                // slot lets a later authenticated player request proceed
                // independently while preserving the proposal in the ledger.
                if intent.source == SOURCE_PROVIDER_ADVISORY {
                    validate_transition_time(intent, now)?;
                    if let Some(recorded) = ledger.get(&intent.intent_id) {
                        if recorded == intent {
                            return Ok(finish!());
                        }
                        return Err(invalid_intent(format!(
                            "intent_id {} conflicts with its durable provider advisory payload",
                            intent.intent_id
                        )));
                    }
                    ledger.insert(intent.intent_id.clone(), intent.clone());
                    return Ok(finish!());
                }

                if let Some(recorded) = ledger.get(&intent.intent_id) {
                    if recorded == intent {
                        return Ok(finish!());
                    }
                    if !immutable_payload_matches(recorded, intent)
                        || !matches!(recorded.status.as_str(), STATUS_SUBMITTED | STATUS_ACCEPTED)
                    {
                        return Err(invalid_intent(format!(
                            "intent_id {} conflicts with its durable ledger payload during proposal",
                            intent.intent_id
                        )));
                    }
                    // A replay of the original proposal after submission or
                    // acceptance is a historical no-op.
                    return Ok(finish!());
                }

                match existing {
                    None => {
                        validate_transition_time(intent, now)?;
                        cell.intent = Some(intent.clone());
                    }
                    Some(current) if current == *intent => {}
                    Some(current) if current.intent_id == intent.intent_id => {
                        return Err(invalid_intent(format!(
                            "intent_id {} conflicts with its recorded payload",
                            intent.intent_id
                        )));
                    }
                    Some(current) if TERMINAL_STATUSES.contains(&current.status.as_str()) => {
                        validate_transition_time(intent, now)?;
                        cell.intent = Some(intent.clone());
                    }
                    Some(current) => {
                        return Err(invalid_intent(format!(
                            "active intent {} must be explicitly replaced before proposing {}",
                            current.intent_id, intent.intent_id
                        )));
                    }
                }
                ledger.insert(intent.intent_id.clone(), intent.clone());
            }
            DomainEvent::AgentIntentSubmitted { .. } => {
                if intent.status != STATUS_SUBMITTED {
                    return Err(invalid_intent(format!(
                        "submitted event must carry status {STATUS_SUBMITTED}, got {}",
                        intent.status
                    )));
                }
                if let Some(recorded) = ledger.get(&intent.intent_id) {
                    if recorded == intent {
                        return Ok(finish!());
                    }
                    if !immutable_payload_matches(recorded, intent) {
                        return Err(invalid_intent(format!(
                            "intent_id {} conflicts with its durable ledger payload during submission",
                            intent.intent_id
                        )));
                    }
                    if matches!(
                        recorded.status.as_str(),
                        STATUS_ACCEPTED | STATUS_BLOCKED | STATUS_SUPERSEDED
                    ) || TERMINAL_STATUSES.contains(&recorded.status.as_str())
                    {
                        // A replay of the original submission after a later
                        // lifecycle disposition is a historical no-op.
                        return Ok(finish!());
                    }
                }

                let Some(current) = existing else {
                    return Err(invalid_intent(format!(
                        "submission target {} is not present",
                        intent.intent_id
                    )));
                };
                if current.intent_id != intent.intent_id {
                    return Err(invalid_intent(format!(
                        "submission targets {}, but current intent is {}",
                        intent.intent_id, current.intent_id
                    )));
                }
                if current.status != STATUS_PROPOSED {
                    return Err(invalid_intent(format!(
                        "only proposed intent can be submitted, got {}",
                        current.status
                    )));
                }
                if !immutable_payload_matches(&current, intent) {
                    return Err(invalid_intent(
                        "submission cannot mutate immutable intent payload",
                    ));
                }
                if intent.event_seq <= current.event_seq {
                    return Err(invalid_intent(
                        "submission event_seq must advance the proposed intent",
                    ));
                }
                validate_transition_time_after_current(intent, now, &current)?;
                cell.intent = Some(intent.clone());
                ledger.insert(intent.intent_id.clone(), intent.clone());
            }
            DomainEvent::AgentIntentAccepted { .. } => {
                if intent.status != STATUS_ACCEPTED {
                    return Err(invalid_intent(format!(
                        "accepted event must carry status {STATUS_ACCEPTED}, got {}",
                        intent.status
                    )));
                }

                if let Some(recorded) = ledger.get(&intent.intent_id) {
                    if recorded == intent {
                        return Ok(finish!());
                    }
                    let promoting_pending_intent = existing.as_ref().is_some_and(|current| {
                        current.intent_id == intent.intent_id
                            && matches!(current.status.as_str(), STATUS_PROPOSED | STATUS_SUBMITTED)
                            && immutable_payload_matches(current, intent)
                    });
                    if !immutable_payload_matches(recorded, intent) {
                        return Err(invalid_intent(format!(
                            "intent_id {} conflicts with its durable ledger payload during acceptance",
                            intent.intent_id
                        )));
                    }
                    if recorded.status != STATUS_ACCEPTED && !promoting_pending_intent {
                        // A replay of the original accepted event after a later
                        // disposition is a historical no-op.
                        return Ok(finish!());
                    }
                    if !promoting_pending_intent {
                        return Err(invalid_intent(format!(
                            "intent_id {} conflicts with its durable ledger payload during acceptance",
                            intent.intent_id
                        )));
                    }
                }

                match existing {
                    None => {
                        return Err(invalid_intent(format!(
                            "accepted intent {} requires a submitted predecessor",
                            intent.intent_id
                        )));
                    }
                    Some(current) if current == *intent => {
                        // Exact replay is intentionally a no-op: do not refresh
                        // timestamps or mutate Activity/Receipt state.
                    }
                    Some(current) if current.intent_id == intent.intent_id => {
                        if current.status != STATUS_SUBMITTED
                            || !immutable_payload_matches(&current, intent)
                        {
                            return Err(invalid_intent(format!(
                                "intent_id {} conflicts with its recorded payload",
                                intent.intent_id
                            )));
                        }
                        if intent.event_seq <= current.event_seq {
                            return Err(invalid_intent(
                                "accepted event_seq must advance the pending intent",
                            ));
                        }
                        validate_transition_time_after_current(intent, now, &current)?;
                        cell.intent = Some(intent.clone());
                    }
                    Some(current) => {
                        return Err(invalid_intent(format!(
                            "only submitted intent can be accepted; current {} is {}",
                            current.intent_id, intent.intent_id
                        )));
                    }
                }
                ledger.insert(intent.intent_id.clone(), intent.clone());
            }
            DomainEvent::AgentIntentReplaced { .. } => {
                if intent.status != STATUS_SUPERSEDED {
                    return Err(invalid_intent(format!(
                        "replacement event must carry status {STATUS_SUPERSEDED}, got {}",
                        intent.status
                    )));
                }
                let Some(replaced_by) = intent.replaced_by.as_deref() else {
                    return Err(invalid_intent(
                        "replacement event requires a non-empty replaced_by",
                    ));
                };
                if replaced_by.trim().is_empty() {
                    return Err(invalid_intent(
                        "replacement event requires a non-empty replaced_by",
                    ));
                }

                if let Some(recorded) = ledger.get(&intent.intent_id) {
                    if recorded == intent {
                        return Ok(finish!());
                    }
                    if !immutable_payload_matches(recorded, intent) {
                        return Err(invalid_intent(format!(
                            "intent_id {} conflicts with its durable ledger payload",
                            intent.intent_id
                        )));
                    }
                }

                match existing {
                    Some(current) if current == *intent => {
                        // Exact replay of the replacement is a no-op.
                    }
                    Some(current) if current.intent_id == intent.intent_id => {
                        if !matches!(current.status.as_str(), STATUS_ACCEPTED | STATUS_BLOCKED) {
                            return Err(invalid_intent(format!(
                                "only accepted or blocked intent can be replaced, got {}",
                                current.status
                            )));
                        }
                        if !immutable_payload_matches(&current, intent) {
                            return Err(invalid_intent(
                                "replacement cannot mutate immutable intent payload",
                            ));
                        }
                        if intent.event_seq <= current.event_seq {
                            return Err(invalid_intent(
                                "replacement event_seq must advance the current intent",
                            ));
                        }
                        validate_transition_time_after_current(intent, now, &current)?;
                        cell.intent = Some(intent.clone());
                    }
                    Some(current) => {
                        return Err(invalid_intent(format!(
                            "replacement targets {}, but current intent is {}",
                            intent.intent_id, current.intent_id
                        )));
                    }
                    None => {
                        return Err(invalid_intent(format!(
                            "replacement target {} is not present",
                            intent.intent_id
                        )));
                    }
                }
                ledger.insert(intent.intent_id.clone(), intent.clone());
            }
            DomainEvent::AgentIntentTransitioned { .. } => {
                if intent.status == STATUS_SUPERSEDED {
                    let Some(replaced_by) = intent.replaced_by.as_deref() else {
                        return Err(invalid_intent(
                            "superseded transition requires a non-empty replaced_by",
                        ));
                    };
                    if replaced_by.trim().is_empty() {
                        return Err(invalid_intent(
                            "superseded transition requires a non-empty replaced_by",
                        ));
                    }
                }
                if let Some(recorded) = ledger.get(&intent.intent_id) {
                    if recorded == intent {
                        return Ok(finish!());
                    }
                    if !immutable_payload_matches(recorded, intent) {
                        return Err(invalid_intent(
                            "transition cannot mutate immutable intent payload",
                        ));
                    }
                }
                let Some(current) = existing else {
                    return Err(invalid_intent(format!(
                        "transition target {} is not present",
                        intent.intent_id
                    )));
                };
                if current == *intent {
                    return Ok(finish!());
                }
                if current.intent_id != intent.intent_id {
                    return Err(invalid_intent(format!(
                        "transition targets {}, but current intent is {}",
                        intent.intent_id, current.intent_id
                    )));
                }
                if !immutable_payload_matches(&current, intent) {
                    return Err(invalid_intent(
                        "transition cannot mutate immutable intent payload",
                    ));
                }
                if intent.event_seq <= current.event_seq {
                    return Err(invalid_intent(
                        "transition event_seq must advance the current intent",
                    ));
                }
                if TERMINAL_STATUSES.contains(&current.status.as_str()) {
                    return Err(invalid_intent(format!(
                        "terminal intent {} cannot transition again",
                        current.intent_id
                    )));
                }

                let allowed = matches!(
                    (current.status.as_str(), intent.status.as_str()),
                    (STATUS_PROPOSED, "rejected")
                        | (STATUS_PROPOSED, "expired")
                        | (STATUS_SUBMITTED, "rejected")
                        | (STATUS_SUBMITTED, "expired")
                        | (STATUS_SUBMITTED, "cancelled")
                        | (STATUS_ACCEPTED, STATUS_BLOCKED)
                        | (STATUS_ACCEPTED, "completed")
                        | (STATUS_ACCEPTED, "rejected")
                        | (STATUS_ACCEPTED, "expired")
                        | (STATUS_ACCEPTED, "cancelled")
                        | (STATUS_ACCEPTED, "superseded")
                        | (STATUS_BLOCKED, "rejected")
                        | (STATUS_BLOCKED, "expired")
                        | (STATUS_BLOCKED, "cancelled")
                        | (STATUS_BLOCKED, "superseded")
                );
                if !allowed {
                    return Err(invalid_intent(format!(
                        "unsupported transition {} -> {}",
                        current.status, intent.status
                    )));
                }
                if intent.status == STATUS_BLOCKED
                    && intent
                        .reason_code
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                {
                    return Err(invalid_intent("blocked transition requires a reason_code"));
                }
                validate_transition_time_after_current(intent, now, &current)?;
                cell.intent = Some(intent.clone());
                ledger.insert(intent.intent_id.clone(), intent.clone());
            }
            _ => unreachable!("apply_domain_event_intent received unsupported event"),
        }

        Ok(finish!())
    }

    pub(super) fn apply_domain_event_intent(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
        envelope_event_seq: Option<u64>,
        committed_receipt_event_id: Option<u64>,
    ) -> Result<(), WorldError> {
        self.prepare_agent_intent_event(
            event,
            now,
            envelope_event_seq,
            committed_receipt_event_id,
        )?
        .install_infallible(self);
        Ok(())
    }
}
