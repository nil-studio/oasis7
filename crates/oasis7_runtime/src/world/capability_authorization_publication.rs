//! Atomic publication for trusted capability-authorization transitions.
//!
//! Capability installation has coupled durable authorization projections,
//! including the grant registry, system identity, invocation context, and
//! budget account. This module prepares their event envelopes, the retained
//! journal, the final authorization root, and one tick-consensus candidate
//! before installing any of those effects.

use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use super::super::capability_authorization::{
    CapabilityBudgetAccount, CapabilityInvocationContext, CapabilityRevocationState,
};
use super::super::{
    CapabilityAuthorizationEvent, TickConsensusRecord, WorldError, WorldEvent, WorldEventBody,
    WorldEventId,
};
use super::World;
use super::capability_authorization_events::validate_authority_record_transition;

struct PreparedCapabilityAuthorizationBatch {
    capability_grants_v2: BTreeMap<String, JsonValue>,
    capability_revocation_state: CapabilityRevocationState,
    capability_invocation_contexts: BTreeMap<String, CapabilityInvocationContext>,
    capability_budget_accounts: BTreeMap<String, CapabilityBudgetAccount>,
    capability_authorization_root: String,
    events: Vec<WorldEvent>,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
    consensus_record: TickConsensusRecord,
}

pub(super) struct PreparedCapabilityAuthorizationEvent {
    event: CapabilityAuthorizationEvent,
    capability_grants_v2: BTreeMap<String, JsonValue>,
    capability_revocation_state: CapabilityRevocationState,
    capability_invocation_contexts: BTreeMap<String, CapabilityInvocationContext>,
    capability_budget_accounts: BTreeMap<String, CapabilityBudgetAccount>,
    capability_authorization_root: String,
}

impl PreparedCapabilityAuthorizationEvent {
    pub(super) fn matches_event(&self, event: &CapabilityAuthorizationEvent) -> bool {
        &self.event == event
    }

    pub(super) fn install(self, world: &mut World) {
        world.capability_grants_v2 = self.capability_grants_v2;
        world.capability_revocation_state = self.capability_revocation_state;
        world.capability_invocation_contexts = self.capability_invocation_contexts;
        world.capability_budget_accounts = self.capability_budget_accounts;
        world.capability_authorization_root = self.capability_authorization_root;
    }
}

impl World {
    pub(super) fn prepare_raw_capability_authorization_event(
        &self,
        event: &CapabilityAuthorizationEvent,
    ) -> Result<PreparedCapabilityAuthorizationEvent, WorldError> {
        let mut capability_grants_v2 = self.capability_grants_v2.clone();
        let mut capability_revocation_state = self.capability_revocation_state.clone();
        let mut capability_invocation_contexts = self.capability_invocation_contexts.clone();
        let mut capability_budget_accounts = self.capability_budget_accounts.clone();
        self.validate_and_project_capability_authorization_event(
            event,
            &mut capability_grants_v2,
            &mut capability_revocation_state,
            &mut capability_invocation_contexts,
            &mut capability_budget_accounts,
        )?;
        let capability_authorization_root = self
            .compute_capability_authorization_root_with_projection(
                &capability_grants_v2,
                &capability_revocation_state,
                &capability_invocation_contexts,
                &capability_budget_accounts,
            )?;
        Ok(PreparedCapabilityAuthorizationEvent {
            event: event.clone(),
            capability_grants_v2,
            capability_revocation_state,
            capability_invocation_contexts,
            capability_budget_accounts,
            capability_authorization_root,
        })
    }

    pub(super) fn append_capability_authorization_event_batch(
        &mut self,
        events: Vec<CapabilityAuthorizationEvent>,
    ) -> Result<WorldEventId, WorldError> {
        let prepared = self.prepare_capability_authorization_event_batch(events)?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }

        let PreparedCapabilityAuthorizationBatch {
            capability_grants_v2,
            capability_revocation_state,
            capability_invocation_contexts,
            capability_budget_accounts,
            capability_authorization_root,
            events,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted,
            consensus_record,
        } = prepared;
        let event_id = events
            .last()
            .map(|event| event.id)
            .expect("capability authorization publication contains an event");

        self.install_prepared_tick_consensus_record(consensus_record)?;
        self.capability_grants_v2 = capability_grants_v2;
        self.capability_revocation_state = capability_revocation_state;
        self.capability_invocation_contexts = capability_invocation_contexts;
        self.capability_budget_accounts = capability_budget_accounts;
        self.capability_authorization_root = capability_authorization_root;
        self.next_event_id = next_event_id;
        self.next_event_id_era = next_event_id_era;
        self.journal.events = journal_events;
        self.runtime_backpressure_stats.journal_events_evicted = self
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(journal_events_evicted);

        Ok(event_id)
    }

    fn prepare_capability_authorization_event_batch(
        &self,
        events: Vec<CapabilityAuthorizationEvent>,
    ) -> Result<PreparedCapabilityAuthorizationBatch, WorldError> {
        if events.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "prepared capability authorization event batch cannot be empty".to_string(),
            });
        }

        // Stage only the authorization projections touched by this batch.
        // In particular, do not clone World or WorldState: those contain
        // process-local caches and the full simulation state.
        let mut capability_grants_v2 = self.capability_grants_v2.clone();
        let mut capability_revocation_state = self.capability_revocation_state.clone();
        let mut capability_invocation_contexts = self.capability_invocation_contexts.clone();
        let mut capability_budget_accounts = self.capability_budget_accounts.clone();
        let mut prepared_events = Vec::with_capacity(events.len());
        let mut next_event_id = self.next_event_id;
        let mut next_event_id_era = self.next_event_id_era;

        for authorization_event in events {
            self.validate_and_project_capability_authorization_event(
                &authorization_event,
                &mut capability_grants_v2,
                &mut capability_revocation_state,
                &mut capability_invocation_contexts,
                &mut capability_budget_accounts,
            )?;
            let (event_id, next_id, next_era) =
                Self::preview_next_event_id(next_event_id, next_event_id_era);
            prepared_events.push(WorldEvent {
                id: event_id,
                time: self.state.time,
                caused_by: None,
                body: WorldEventBody::CapabilityAuthorization(authorization_event),
            });
            next_event_id = next_id;
            next_event_id_era = next_era;
        }

        let mut journal_events = self.journal.events.clone();
        journal_events.extend(prepared_events.iter().cloned());
        let max_len = self.runtime_memory_limits.max_journal_events.max(1);
        let overflow = journal_events.len().saturating_sub(max_len);
        if overflow > 0 {
            journal_events.drain(0..overflow);
        }
        let tick_events: Vec<WorldEvent> = journal_events
            .iter()
            .filter(|event| event.time == self.state.time)
            .cloned()
            .collect();
        let state_root = self.current_state_root_hash()?;
        let consensus_record = self.build_tick_consensus_record_for_prepared_events(
            self.state.time,
            tick_events.as_slice(),
            state_root.clone(),
        )?;
        self.validate_tick_consensus_candidate_for_prepared_publication(
            &consensus_record,
            tick_events.as_slice(),
            state_root.as_str(),
        )?;
        let capability_authorization_root = self
            .compute_capability_authorization_root_with_projection(
                &capability_grants_v2,
                &capability_revocation_state,
                &capability_invocation_contexts,
                &capability_budget_accounts,
            )?;

        Ok(PreparedCapabilityAuthorizationBatch {
            capability_grants_v2,
            capability_revocation_state,
            capability_invocation_contexts,
            capability_budget_accounts,
            capability_authorization_root,
            events: prepared_events,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted: overflow as u64,
            consensus_record,
        })
    }

    fn validate_and_project_capability_authorization_event(
        &self,
        event: &CapabilityAuthorizationEvent,
        capability_grants_v2: &mut BTreeMap<String, JsonValue>,
        capability_revocation_state: &mut CapabilityRevocationState,
        capability_invocation_contexts: &mut BTreeMap<String, CapabilityInvocationContext>,
        capability_budget_accounts: &mut BTreeMap<String, CapabilityBudgetAccount>,
    ) -> Result<(), WorldError> {
        match event {
            CapabilityAuthorizationEvent::WorldServiceIntentRecorded { key, record } => {
                crate::service_admission::correlation::validate_result(key, record)
                    .map_err(|reason| super::capability_authorization::deny(&reason))?;
                if let Some(existing) = capability_revocation_state.world_service_results.get(key)
                    && existing != record
                {
                    return Err(super::capability_authorization::deny(
                        "canonical intent result conflict",
                    ));
                }
                capability_revocation_state
                    .world_service_results
                    .insert(key.clone(), record.clone());
                Ok(())
            }
            CapabilityAuthorizationEvent::AgentSignerDelegationInstalled { signed } => {
                crate::service_admission::agent_authority::validate_delegation(
                    self, signed,
                )
                .map_err(|reason| super::capability_authorization::deny(&reason))?;
                capability_revocation_state
                    .agent_signer_delegations
                    .insert(signed.request.agent_id.clone(), signed.request.clone());
                Ok(())
            }
            CapabilityAuthorizationEvent::AuthorityInstalledWithProof { record, proof } => {
                super::capability_authorization::validate_authority_record(record)?;
                self.verify_capability_authority_finality_proof(record, proof)?;
                if self.chain_resource_manifest.world_id != "unbound"
                    && self.chain_resource_manifest.world_id != record.world_id
                {
                    return Err(super::capability_authorization::deny(
                        "authority record world does not match live world",
                    ));
                }
                if let Some(existing) = capability_revocation_state
                    .authority_records
                    .get(&record.issuer_id)
                    && existing != record
                {
                    validate_authority_record_transition(existing, record)?;
                }
                if let Some(existing) = capability_revocation_state
                    .authority_finality_proofs
                    .get(&record.issuer_id)
                    && existing != proof
                    && capability_revocation_state
                        .authority_records
                        .get(&record.issuer_id)
                        == Some(record)
                {
                    return Err(super::capability_authorization::deny(
                        "authority finality proof is immutable",
                    ));
                }
                capability_revocation_state.epoch = capability_revocation_state
                    .epoch
                    .max(record.revocation_epoch);
                capability_revocation_state
                    .revoked_grant_ids
                    .extend(record.revoked_grant_ids.iter().cloned());
                capability_revocation_state
                    .superseded_by
                    .extend(record.superseded_by.clone());
                capability_revocation_state.finalized_receipt_id =
                    Some(record.finalized_receipt_id.clone());
                capability_revocation_state
                    .authority_records
                    .insert(record.issuer_id.clone(), record.clone());
                capability_revocation_state
                    .authority_finality_proofs
                    .insert(record.issuer_id.clone(), proof.clone());
                Ok(())
            }
            CapabilityAuthorizationEvent::AgentIdentityInstalled { agent_id, identity } => {
                super::capability_authorization::validate_agent_identity(agent_id, identity)?;
                let Some(agent) = self.state.agents.get(agent_id) else {
                    return Err(super::capability_authorization::deny(
                        "capability agent identity requires a live agent",
                    ));
                };
                if agent.state.agent_id != agent_id.as_str() {
                    return Err(super::capability_authorization::deny(
                        "live agent state id does not match its registry key",
                    ));
                }
                if let Some(existing) = capability_revocation_state.agent_identities.get(agent_id) {
                    if identity.generation < existing.generation {
                        return Err(super::capability_authorization::deny(
                            "capability agent identity generation regressed",
                        ));
                    }
                    if identity.generation == existing.generation && existing != identity {
                        return Err(super::capability_authorization::deny(
                            "capability agent identity changed without a new generation",
                        ));
                    }
                }
                capability_revocation_state
                    .agent_identities
                    .insert(agent_id.clone(), identity.clone());
                Ok(())
            }
            CapabilityAuthorizationEvent::GrantRegistered { grant } => {
                super::capability_authorization_events::validate_and_project_registered_grant(
                    self,
                    capability_grants_v2,
                    grant,
                    self.state.time,
                )
            }
            CapabilityAuthorizationEvent::SystemIdentityInstalled { system_id, epoch } => {
                if system_id.trim().is_empty() || *epoch > self.state.time {
                    return Err(super::capability_authorization::deny(
                        "capability system identity is not live",
                    ));
                }
                if let Some(existing) = capability_revocation_state.system_identities.get(system_id)
                {
                    if existing != epoch {
                        return Err(super::capability_authorization::deny(
                            "capability system identity changed without a new epoch",
                        ));
                    }
                    return Ok(());
                }
                capability_revocation_state
                    .system_identities
                    .insert(system_id.clone(), *epoch);
                Ok(())
            }
            CapabilityAuthorizationEvent::InvocationContextInstalled { key, context } => {
                super::capability_authorization::validate_invocation_context(context)?;
                let expected_key =
                    super::capability_authorization_events::capability_invocation_context_key(
                        context,
                    )?;
                if key != &expected_key {
                    return Err(super::capability_authorization::deny(
                        "invocation context journal key does not match context",
                    ));
                }
                if let Some(existing) = capability_invocation_contexts.get(key)
                    && existing != context
                {
                    return Err(super::capability_authorization::deny(
                        "invocation context is immutable",
                    ));
                }
                capability_invocation_contexts.insert(key.clone(), context.clone());
                Ok(())
            }
            CapabilityAuthorizationEvent::BudgetAccountInstalled { key, account } => {
                super::capability_authorization_state::validate_budget_account(account)?;
                let expected_key = super::capability_authorization_state::capability_budget_key(
                    &account.subject,
                    &account.grant_id,
                )?;
                if key != &expected_key {
                    return Err(super::capability_authorization::deny(
                        "capability budget journal key does not match account",
                    ));
                }
                if let Some(existing) = capability_budget_accounts.get(key)
                    && existing != account
                {
                    return Err(super::capability_authorization::deny(
                        "capability budget account is immutable",
                    ));
                }
                capability_budget_accounts.insert(key.clone(), account.clone());
                Ok(())
            }
            _ => Err(super::capability_authorization::deny(
                "prepared capability authorization event batch contains an unsupported body",
            )),
        }
    }
}
