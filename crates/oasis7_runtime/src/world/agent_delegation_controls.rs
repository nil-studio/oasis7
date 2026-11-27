//! Current-owner commands and exact pending-intent authority dispositions.
use super::super::transition::{BorrowedWorldView, DurableCommitConfirmation, PreparedStagedCommit, RootInstallContext, StagedTransition, SubsystemDelta, TransitionContext, TransitionReject};
use super::{AgentDecisionCausalContextV1, AgentDelegationGrantV1, Ledger, World, denied};
use crate::{AgentIntentReplayDisposition, WorldError};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOwnerControlKindV1 {
    Override,
    Interrupt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentOwnerControlV1 {
    pub control_id: String,
    pub agent_id: String,
    pub intent_id: String,
    pub request_digest: String,
    pub grant_id: Option<String>,
    pub expected_grant_revision: Option<u64>,
    pub kind: AgentOwnerControlKindV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct OwnerControlRecord {
    pub owner_id: String,
    pub control: AgentOwnerControlV1,
    pub disposition: AgentIntentReplayDisposition,
    pub consumed_receipt_id: Option<String>,
}

type PreparedIntentTransition = (crate::world::agent_intent_publication::PreparedAgentIntent, crate::WorldEventId, u64, Vec<crate::WorldEvent>, u64);
#[derive(Default)]
pub(crate) struct AgentOwnerControlDelta {
    encoded_ledger: Option<serde_json::Value>,
    intent_transition: Option<PreparedIntentTransition>,
}
impl SubsystemDelta for AgentOwnerControlDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        if let Some(encoded) = self.encoded_ledger {
            let world = ctx.world_mut();
            if world.cognition.is_null() { world.cognition = crate::cognition_recovery::default_cognition_persistence_projection(); }
            world.cognition["agent_delegation"] = encoded;
        }
        if let Some((prepared, next_id, next_era, journal, evicted)) = self.intent_transition {
            prepared.install_infallible(ctx.state_mut());
            ctx.install_journal_tail(next_id, next_era, journal, evicted);
        }
    }
}
pub(crate) struct StagedAgentOwnerControl {
    pub(crate) player_id: String,
    pub(crate) control: AgentOwnerControlV1,
    pub(crate) disposition: RefCell<Option<AgentIntentReplayDisposition>>,
}
impl StagedAgentOwnerControl {
    pub(crate) fn new(player_id: impl Into<String>, control: AgentOwnerControlV1) -> Self { Self { player_id: player_id.into(), control, disposition: RefCell::new(None) } }
    pub(crate) fn take_disposition(&self) -> Option<AgentIntentReplayDisposition> { self.disposition.borrow_mut().take() }
}
impl StagedTransition for StagedAgentOwnerControl {
    type Delta = AgentOwnerControlDelta;
    fn prepare(&self, view: &BorrowedWorldView<'_>, _ctx: &TransitionContext) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let (delta, consensus, disposition) = view.world().prepare_agent_owner_control_delta(&self.player_id, &self.control)?;
        *self.disposition.borrow_mut() = Some(disposition);
        Ok(PreparedStagedCommit::new(view.base_head().clone(), consensus, delta))
    }
}
pub(crate) struct StagedAgentDelegationDenial<'a> {
    pub(crate) agent_id: String,
    pub(crate) context: AgentDecisionCausalContextV1,
    pub(crate) error: &'a WorldError,
}
impl StagedTransition for StagedAgentDelegationDenial<'_> {
    type Delta = AgentOwnerControlDelta;
    fn prepare(&self, view: &BorrowedWorldView<'_>, _ctx: &TransitionContext) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let (delta, consensus) = view.world().prepare_agent_delegation_denial_delta(&self.agent_id, &self.context, self.error)?;
        Ok(PreparedStagedCommit::new(view.base_head().clone(), consensus, delta))
    }
}

impl World {
    pub(crate) fn agent_owner_control_replay(
        &self,
        player_id: &str,
        control_id: &str,
    ) -> Result<Option<(AgentOwnerControlV1, AgentIntentReplayDisposition)>, WorldError> {
        let ledger = self.delegation_ledger()?;
        let Some(record) = ledger.controls.get(control_id) else {
            return Ok(None);
        };
        if record.owner_id != player_id
            || self
                .current_agent_owner(&record.control.agent_id)
                .is_none_or(|owner| owner.player_id != player_id)
        {
            return Err(denied("owner_control_denied"));
        }
        Ok(Some((record.control.clone(), record.disposition.clone())))
    }
    pub(super) fn validate_recorded_owner_override(
        &self,
        ledger: &Ledger,
        grant: &AgentDelegationGrantV1,
        context: &AgentDecisionCausalContextV1,
    ) -> Result<(), WorldError> {
        if context.owner_control_refs.len() != 1 {
            return Err(denied("override_context_untrusted"));
        }
        let record = ledger
            .controls
            .get(&context.owner_control_refs[0])
            .ok_or_else(|| denied("override_context_untrusted"))?;
        if record.control.kind != AgentOwnerControlKindV1::Override
            || record.consumed_receipt_id.is_some()
            || context.intent_id.as_ref() != Some(&record.control.intent_id)
            || context.override_actor.as_ref() != Some(&record.owner_id)
            || record.control.grant_id.as_ref() != Some(&grant.grant_id)
            || record.control.expected_grant_revision != Some(grant.revision)
        {
            return Err(denied("override_authority_stale"));
        }
        Ok(())
    }
    /// Called only after the transport verifies the principal/signature.
    /// The runtime independently rechecks the live owner and exact intent.
    pub(crate) fn bind_agent_owner_control(
        &mut self,
        player_id: &str,
        control: AgentOwnerControlV1,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        let staged = StagedAgentOwnerControl::new(player_id, control);
        let commit = staged.prepare(&BorrowedWorldView::new(self)?, &TransitionContext::default()).map_err(TransitionReject::into_world_error)?;
        self.install_owner_control_commit(commit)?;
        Ok(staged.take_disposition().expect("staged owner control disposition"))
    }

    pub(super) fn apply_current_owner_control(
        &self,
        ledger: &Ledger,
        grant: Option<&AgentDelegationGrantV1>,
        context: &mut AgentDecisionCausalContextV1,
    ) -> Result<(), WorldError> {
        // Only this persisted authenticated record can author an override.
        if context.override_actor.is_some() || !context.owner_control_refs.is_empty() {
            return Err(denied("override_context_untrusted"));
        }
        let Some(intent_id) = &context.intent_id else {
            return Ok(());
        };
        let Some(record) = ledger.controls.values().find(|record| {
            &record.control.intent_id == intent_id
                && record.control.kind == AgentOwnerControlKindV1::Override
                && record.consumed_receipt_id.is_none()
        }) else {
            return Ok(());
        };
        let grant = grant.ok_or_else(|| denied("override_requires_grant"))?;
        let intent = self
            .state
            .agent_intent_ledger
            .get(intent_id)
            .ok_or_else(|| denied("intent_missing"))?;
        if record.control.request_digest != intent.request_digest
            || record.control.grant_id.as_ref() != Some(&grant.grant_id)
            || record.control.expected_grant_revision != Some(grant.revision)
            || record.owner_id != grant.owner_id
        {
            return Err(denied("override_authority_stale"));
        }
        self.validate_grant_source(grant)?;
        context.override_actor = Some(record.owner_id.clone());
        context
            .owner_control_refs
            .push(record.control.control_id.clone());
        Ok(())
    }

    /// Persist the metadata-only disposition while returning the original
    /// execution error. Prior effects and settlement receipts are immutable.
    pub(super) fn persist_agent_delegation_denial(
        &mut self,
        agent_id: &str,
        context: &AgentDecisionCausalContextV1,
        error: &WorldError,
    ) -> Result<(), WorldError> {
        let staged = StagedAgentDelegationDenial { agent_id: agent_id.to_string(), context: context.clone(), error };
        let commit = staged.prepare(&BorrowedWorldView::new(self)?, &TransitionContext::default()).map_err(TransitionReject::into_world_error)?;
        self.install_owner_control_commit(commit)
    }

    fn install_owner_control_commit(&mut self, commit: PreparedStagedCommit<AgentOwnerControlDelta>) -> Result<(), WorldError> {
        if commit.delta().encoded_ledger.is_none() && commit.delta().intent_transition.is_none() && commit.consensus_record().is_none() { return Ok(()); }
        let prev = self.persistence_dir.borrow().is_some().then(|| (self.cognition.clone(), self.state.clone(), self.journal.clone(), self.next_event_id, self.next_event_id_era, self.runtime_backpressure_stats.journal_events_evicted, self.tick_consensus_records.clone()));
        commit.install_confirmed_unchecked_head(self, DurableCommitConfirmation::Confirmed).map_err(TransitionReject::into_world_error)?;
        if let Err(err) = self.persist_runtime_transaction_if_configured() {
            if let Some((cognition, state, journal, next_id, next_era, evicted, consensus)) = prev {
                (self.cognition, self.state, self.journal, self.next_event_id, self.next_event_id_era, self.runtime_backpressure_stats.journal_events_evicted, self.tick_consensus_records) = (cognition, state, journal, next_id, next_era, evicted, consensus);
            }
            return Err(err);
        }
        Ok(())
    }

    pub(in crate::world) fn persist_agent_delegation_commit_denial(
        &mut self,
        commit_id: &str,
        error: &WorldError,
    ) -> Result<(), WorldError> {
        let request_id = self
            .cognition
            .get("commit_records")
            .and_then(serde_json::Value::as_array)
            .and_then(|records| {
                records.iter().find(|r| {
                    r.get("commit_id").and_then(serde_json::Value::as_str) == Some(commit_id)
                })
            })
            .and_then(|r| r.get("decision_request_id"))
            .and_then(serde_json::Value::as_str);
        let decision = request_id.and_then(|id| {
            self.delegation_ledger()
                .ok()?
                .decisions
                .get(id)
                .map(|d| (d.agent_id.clone(), d.context.clone()))
        });
        if let Some((agent_id, context)) = decision {
            self.persist_agent_delegation_denial(&agent_id, &context, error)?;
        }
        Ok(())
    }

    pub(super) fn prepare_agent_owner_control_delta(&self, player_id: &str, control: &AgentOwnerControlV1) -> Result<(AgentOwnerControlDelta, Option<crate::TickConsensusRecord>, AgentIntentReplayDisposition), WorldError> {
        let owner = self.current_agent_owner(&control.agent_id).ok_or_else(|| denied("owner_source_unavailable"))?;
        if player_id != owner.player_id || control.control_id.trim().is_empty() { return Err(denied("owner_control_denied")); }
        let mut ledger = self.delegation_ledger()?;
        if let Some(old) = ledger.controls.get(&control.control_id) {
            if &old.control == control && old.owner_id == player_id { return Ok((AgentOwnerControlDelta::default(), None, old.disposition.clone())); }
            return Err(denied("owner_control_idempotency_conflict"));
        }
        let intent = self.state.agents.get(&control.agent_id).and_then(|c| c.intent.as_ref()).ok_or_else(|| denied("intent_missing"))?;
        if intent.intent_id != control.intent_id || intent.request_digest != control.request_digest || !matches!(intent.status.as_str(), "accepted" | "blocked") { return Err(denied("owner_control_intent_stale")); }
        if control.grant_id.is_some() != control.expected_grant_revision.is_some() { return Err(denied("owner_control_grant_tuple_invalid")); }
        if let Some(id) = &control.grant_id {
            let g = ledger.grants.get(id).ok_or_else(|| denied("grant_missing"))?;
            if g.agent_id != control.agent_id || g.owner_id != player_id || control.expected_grant_revision != Some(g.revision) { return Err(denied("grant_revision_stale")); }
            self.validate_grant_source(g)?;
            if control.kind == AgentOwnerControlKindV1::Override && (g.revoked || self.state.time < g.valid_from_tick || self.state.time > g.valid_until_tick) { return Err(denied("override_grant_unavailable")); }
        } else if control.kind == AgentOwnerControlKindV1::Override { return Err(denied("override_requires_grant")); }
        if control.kind == AgentOwnerControlKindV1::Override && ledger.controls.values().any(|r| r.control.intent_id == control.intent_id && r.control.kind == AgentOwnerControlKindV1::Override && r.consumed_receipt_id.is_none()) {
            return Err(denied("override_already_pending"));
        }
        let (intent_transition, consensus, disposition) = if control.kind == AgentOwnerControlKindV1::Interrupt {
            match self.prepare_agent_intent_terminal_transition(&control.agent_id, &control.intent_id, &control.request_digest, "cancelled", "owner_interrupted_replan_required")? {
                Some((t, c, d)) => (Some(t), Some(c), d),
                None => (None, None, AgentIntentReplayDisposition::from(intent)),
            }
        } else { (None, None, AgentIntentReplayDisposition::from(intent)) };
        ledger.controls.insert(control.control_id.clone(), OwnerControlRecord { owner_id: player_id.into(), control: control.clone(), disposition: disposition.clone(), consumed_receipt_id: None });
        Ok((AgentOwnerControlDelta { encoded_ledger: Some(serde_json::to_value(&ledger)?), intent_transition }, consensus, disposition))
    }

    pub(super) fn prepare_agent_delegation_denial_delta(&self, agent_id: &str, context: &AgentDecisionCausalContextV1, error: &WorldError) -> Result<(AgentOwnerControlDelta, Option<crate::TickConsensusRecord>), WorldError> {
        let empty = (AgentOwnerControlDelta::default(), None);
        let WorldError::ResourceBalanceInvalid { reason } = error else { return Ok(empty); };
        let Some(code) = reason.strip_prefix("agent_delegation:") else { return Ok(empty); };
        let Some(intent_id) = &context.intent_id else { return Ok(empty); };
        let Some(intent) = self.state.agent_intent_ledger.get(intent_id).filter(|i| i.agent_id == agent_id && matches!(i.status.as_str(), "accepted" | "blocked")) else { return Ok(empty); };
        let status = if code == "agent_dissent" { "blocked" } else { "rejected" };
        Ok(match self.prepare_agent_intent_terminal_transition(&intent.agent_id, intent_id, &intent.request_digest, status, &format!("agency_{code}_replan_required"))? {
            Some((t, c, _)) => (AgentOwnerControlDelta { encoded_ledger: None, intent_transition: Some(t) }, Some(c)),
            None => empty,
        })
    }

    fn prepare_agent_intent_terminal_transition(&self, agent_id: &str, intent_id: &str, request_digest: &str, status: &str, reason_code: &str) -> Result<Option<(PreparedIntentTransition, crate::TickConsensusRecord, AgentIntentReplayDisposition)>, WorldError> {
        let current = self.state.agents.get(agent_id).and_then(|c| c.intent.as_ref()).filter(|i| i.intent_id == intent_id).ok_or_else(|| denied("intent_missing"))?;
        if current.agent_id != agent_id || current.request_digest != request_digest { return Err(denied("owner_control_intent_stale")); }
        if current.status == status { return Ok(None); }
        let (event_id, next_id, next_era) = Self::preview_next_event_id(self.next_event_id, self.next_event_id_era);
        let mut transitioned = current.clone();
        transitioned.status = status.to_string();
        transitioned.summary = oasis7_agent_api::intent_summary::canonical_agent_intent_summary(&transitioned.kind, &transitioned.source, status).map(|s| s.text).map_err(|e| denied(&e.to_string()))?;
        transitioned.event_seq = event_id;
        transitioned.updated_at = self.state.time;
        transitioned.reason_code = Some(reason_code.to_string());
        transitioned.reason_summary = Some("Pending instruction requires replanning before execution.".to_string());
        let disposition = AgentIntentReplayDisposition::from(&transitioned);
        let domain_event = crate::DomainEvent::AgentIntentTransitioned { intent: transitioned };
        let prepared = self.prepare_raw_agent_intent_event(&domain_event)?;
        let state_root = self.state_root_hash_with_agent_intent_overlay(&prepared)?;
        let mut journal = self.journal.events.clone();
        journal.push(crate::WorldEvent { id: event_id, time: self.state.time, caused_by: None, body: crate::WorldEventBody::Domain(domain_event) });
        let overflow = journal.len().saturating_sub(self.runtime_memory_limits.max_journal_events.max(1));
        if overflow > 0 { journal.drain(0..overflow); }
        let tick_events: Vec<_> = journal.iter().filter(|e| e.time == self.state.time).cloned().collect();
        let consensus = self.build_tick_consensus_record_for_prepared_events(self.state.time, &tick_events, state_root.clone())?;
        self.validate_tick_consensus_candidate_for_prepared_publication(&consensus, &tick_events, &state_root)?;
        Ok(Some(((prepared, next_id, next_era, journal, overflow as u64), consensus, disposition)))
    }
}
