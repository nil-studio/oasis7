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
use crate::world::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition, SubsystemDelta,
    SynchronousEventTransition, TransitionContext, TransitionReject, publish_synchronous_event,
};

/// The original authorization publication's sparse staged projection. No
/// canonical authorization, journal, or queue write occurs in preparation.
#[derive(Clone)]
pub(crate) struct CapabilityAuthorizationBatchDelta {
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
}

impl CapabilityAuthorizationBatchDelta {
    fn last_event_id(&self) -> WorldEventId {
        self.events
            .last()
            .map(|event| event.id)
            .expect("capability authorization publication contains an event")
    }
}

impl SubsystemDelta for CapabilityAuthorizationBatchDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        let world = ctx.world_mut();
        world.capability_grants_v2 = self.capability_grants_v2;
        let previous_alternatives = world.capability_revocation_state.safe_alternatives.clone();
        world.capability_revocation_state = self.capability_revocation_state;
        let records = world.capability_revocation_state.safe_alternatives.clone();
        for (key, record) in &records {
            world.install_safe_alternative_queue(previous_alternatives.get(key), record);
        }
        world.capability_invocation_contexts = self.capability_invocation_contexts;
        world.capability_budget_accounts = self.capability_budget_accounts;
        world.capability_authorization_root = self.capability_authorization_root;
        ctx.install_journal_tail(
            self.next_event_id,
            self.next_event_id_era,
            self.journal_events,
            self.journal_events_evicted,
        );
    }
}

pub(crate) struct StagedCapabilityAuthorizationBatch<'a> {
    pub(crate) events: &'a [CapabilityAuthorizationEvent],
}

impl StagedTransition for StagedCapabilityAuthorizationBatch<'_> {
    type Delta = CapabilityAuthorizationBatchDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        _ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let (delta, consensus_record) = view
            .world()
            .prepare_capability_authorization_event_batch(self.events)?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            Some(consensus_record),
            delta,
        ))
    }
}

// An ordinary capability-authority publication is prepared and installed
// under one exclusive &mut World borrow. It does not need to hash every
// unrelated module artifact or journal page merely to ignore that hash.
// Delayed/external commits still use the full StagedTransition base-head proof.
impl SynchronousEventTransition for StagedCapabilityAuthorizationBatch<'_> {
    fn prepare_event(
        &self,
        world: &World,
        _ctx: &TransitionContext,
    ) -> Result<(WorldEventId, TickConsensusRecord, Self::Delta), TransitionReject> {
        let (delta, consensus_record) =
            world.prepare_capability_authorization_event_batch(self.events)?;
        Ok((delta.last_event_id(), consensus_record, delta))
    }
}

#[allow(dead_code)] // adapter: #6142
pub(crate) struct StagedCapabilityGrantV2 {
    pub(crate) grant: oasis7_wasm_abi::CapabilityGrantV2,
}
impl StagedTransition for StagedCapabilityGrantV2 {
    type Delta = Option<CapabilityAuthorizationBatchDelta>;
    fn prepare(&self, view: &BorrowedWorldView<'_>, _ctx: &TransitionContext) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let (world, grant) = (view.world(), &self.grant);
        let deny = |s: String| super::capability_authorization::deny(s);
        world.verify_capability_authorization_root()?;
        grant.validate().map_err(|e| deny(format!("grant validation: {e}")))?;
        if !grant.body_hash_matches().map_err(|e| deny(format!("grant body hash: {e}")))?
            || grant.expected_grant_id().map_err(|e| deny(format!("grant id hash: {e}")))? != grant.grant_id
        {
            return Err(super::capability_authorization::deny("grant canonical body hash or id mismatch").into());
        }
        if grant.expires_at_tick.is_none() || grant.status != "verified" || grant.issued_at_tick > world.state.time {
            return Err(super::capability_authorization::deny("grant is not finalized, verified, and live").into());
        }
        world.verify_issuer(grant)?;
        world.verify_live_revocation(grant)?;
        world.verify_parent_chain(grant)?;
        if let oasis7_wasm_abi::CapabilitySubject::System { system_id, .. } = &grant.subject {
            let manifest = world.active_module_manifest(&grant.scope.module_id).map_err(|e| deny(format!("system subject module is not live: {e:?}")))?;
            if grant.scope.module_id.strip_prefix("module.").map(|s| format!("system-{s}")).as_deref() != Some(system_id.as_str()) || manifest.version != grant.scope.module_version {
                return Err(super::capability_authorization::deny("system subject is not bound to a live module system").into());
            }
        }
        let encoded = serde_json::to_value(grant).map_err(WorldError::from)?;
        if let Some(existing) = world.capability_grants_v2.get(&grant.grant_id) {
            if existing != &encoded { return Err(super::capability_authorization::deny("immutable grant body changed").into()); }
            return Ok(PreparedStagedCommit::new(view.base_head().clone(), None, None));
        }
        let (delta, consensus) = world.prepare_capability_authorization_event_batch(&[CapabilityAuthorizationEvent::GrantRegistered { grant: grant.clone() }])?;
        Ok(PreparedStagedCommit::new(view.base_head().clone(), Some(consensus), Some(delta)))
    }
}

#[allow(dead_code)] // adapter: #6142
pub(crate) struct StagedCapabilityRevokeV2 {
    pub(crate) grant_id: String,
    pub(crate) record: super::super::capability_authorization::CapabilityAuthorityRecord,
    pub(crate) proof: super::super::capability_authorization::CapabilityAuthorityFinalityProof,
}
impl StagedTransition for StagedCapabilityRevokeV2 {
    type Delta = CapabilityAuthorizationBatchDelta;
    fn prepare(&self, view: &BorrowedWorldView<'_>, _ctx: &TransitionContext) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let world = view.world();
        if self.grant_id.trim().is_empty() {
            return Err(super::capability_authorization::deny("capability revocation grant id is required").into());
        }
        let mut record = self.record.clone();
        record.revoked_grant_ids.insert(self.grant_id.clone());
        world.verify_capability_authorization_root()?;
        super::capability_authorization::validate_authority_record(&record)?;
        world.verify_capability_authority_finality_proof(&record, &self.proof)?;
        if world.chain_resource_manifest.world_id != "unbound" && world.chain_resource_manifest.world_id != record.world_id {
            return Err(super::capability_authorization::deny("authority record world does not match live world").into());
        }
        let (delta, consensus) = world.prepare_capability_authorization_event_batch(&[CapabilityAuthorizationEvent::AuthorityInstalledWithProof { record, proof: self.proof.clone() }])?;
        Ok(PreparedStagedCommit::new(view.base_head().clone(), Some(consensus), delta))
    }
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
        let previous_alternatives = world.capability_revocation_state.safe_alternatives.clone();
        world.capability_revocation_state = self.capability_revocation_state;
        let records = world.capability_revocation_state.safe_alternatives.clone();
        for (key, record) in &records {
            world.install_safe_alternative_queue(previous_alternatives.get(key), record);
        }
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
        // Ordinary synchronous publication is one exclusive World borrow;
        // no stale-head window exists. The shared helper preserves the original
        // post-prepare failure gate, consensus validation, and root installation.
        // This is NOT a delayed or IWS durable-confirmation path.
        let context = TransitionContext::new(self.state.time, None);
        let transition = StagedCapabilityAuthorizationBatch { events: &events };
        publish_synchronous_event(self, &transition, &context)
    }

    fn prepare_capability_authorization_event_batch(
        &self,
        events: &[CapabilityAuthorizationEvent],
    ) -> Result<(CapabilityAuthorizationBatchDelta, TickConsensusRecord), WorldError> {
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

        for authorization_event in events.iter().cloned() {
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

        Ok((
            CapabilityAuthorizationBatchDelta {
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
            },
            consensus_record,
        ))
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
            CapabilityAuthorizationEvent::DevLocalParticipantPolicyInstalled { .. }
            | CapabilityAuthorizationEvent::ParticipantPolicyInstalled { .. }
            | CapabilityAuthorizationEvent::ParticipantRegistered { .. } => {
                super::participant::project_event(self, capability_revocation_state, event)
            }
            CapabilityAuthorizationEvent::SafeAlternativeCommandRecorded { signed } => {
                let (key, record) = self
                    .project_safe_alternative_with_revocation_state(
                        capability_revocation_state,
                        signed,
                    )
                    .map_err(|reason| super::capability_authorization::deny(&reason))?;
                capability_revocation_state
                    .safe_alternatives
                    .insert(key, record);
                Ok(())
            }
            CapabilityAuthorizationEvent::SafeAlternativeOutcomeRecorded {
                key,
                action_id,
                event_id,
            } => {
                let record = self.safe_alternative_outcome_projection_with_revocation_state(
                    capability_revocation_state,
                    key,
                    *action_id,
                    *event_id,
                )?;
                capability_revocation_state
                    .safe_alternatives
                    .insert(key.clone(), record);
                Ok(())
            }
            CapabilityAuthorizationEvent::TaskTerminalRecordedV2 { record } => {
                super::task_terminal_v2::project(self, capability_revocation_state, record)
            }
            CapabilityAuthorizationEvent::WorldServiceIntentRecorded { key, record } => {
                crate::service_admission::correlation::validate_result(key, record)
                    .map_err(|reason| super::capability_authorization::deny(&reason))?;
                super::participant::validate_service_receipt(capability_revocation_state, record)?;
                crate::service_admission::starter::validate_result(self, record)
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
                crate::service_admission::agent_authority::validate_delegation_with_revocation_state(
                    self,
                    capability_revocation_state,
                    signed,
                )
                .map_err(|reason| super::capability_authorization::deny(&reason))?;
                capability_revocation_state
                    .agent_signer_delegations
                    .insert(signed.request.agent_id.clone(), signed.request.clone());
                Ok(())
            }
            CapabilityAuthorizationEvent::AuthorityInstalledWithProof { record, proof } => {
                if self.devlocal_participant_authority.is_some() {
                    return Err(super::capability_authorization::deny("finality authority conflicts with explicit DevLocal participant authority"));
                }

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
                super::capability_authorization_events::project_agent_identity(
                    &self.state,
                    capability_revocation_state,
                    agent_id,
                    identity,
                )
            }
            CapabilityAuthorizationEvent::GrantRegistered { grant } => {
                super::capability_authorization_events::validate_and_project_registered_grant_with_revocation_state(
                    self,
                    capability_revocation_state,
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

#[cfg(test)]
impl World {
    pub(crate) fn append_capability_identity_batch_for_test(
        &mut self,
        events: Vec<CapabilityAuthorizationEvent>,
    ) -> Result<(), WorldError> {
        self.append_capability_authorization_event_batch(events)
            .map(|_| ())
    }
}
