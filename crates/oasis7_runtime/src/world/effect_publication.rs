use super::super::{
    EffectIntent, PolicyDecisionRecord, TickConsensusRecord, WorldError, WorldEvent,
    WorldEventBody, WorldEventId,
};
use super::World;
use std::collections::{BTreeSet, VecDeque};

struct PreparedNoStateEventBatch {
    events: Vec<WorldEvent>,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
    consensus_record: TickConsensusRecord,
}

impl World {
    /// Reconcile the intent allocator while replaying an effect tail.  Both
    /// the policy audit and queue event carry the same intent id, so the
    /// numeric sequence is de-duplicated before advancing the rolling
    /// allocator.  When the max value is replayed, the shared preview helper
    /// advances the era exactly as live allocation does.
    pub(super) fn reconcile_replayed_intent_allocator(
        &mut self,
        intent_id: &str,
        seen_sequences: &mut BTreeSet<u64>,
    ) {
        let Some(raw_sequence) = intent_id.strip_prefix("intent-") else {
            return;
        };
        let Ok(sequence) = raw_sequence.parse::<u64>() else {
            return;
        };
        if !seen_sequences.insert(sequence) {
            return;
        }
        let (allocated, next_intent_id, next_intent_id_era) =
            Self::preview_next_intent_seq(self.next_intent_id, self.next_intent_id_era);
        if sequence != allocated {
            return;
        }
        self.next_intent_id = next_intent_id;
        self.next_intent_id_era = next_intent_id_era;
    }

    /// Atomically publish the public effect audit and queue transition.
    ///
    /// Effect emission has one additional allocator and one bounded queue
    /// beside the normal event publication state. Preparing those values
    /// together with both events ensures a queue-full or late publication
    /// failure cannot leave a policy audit, intent id, journal, consensus
    /// record, or eviction counter partially committed.
    pub(super) fn append_effect_publication_atomically(
        &mut self,
        policy_record: PolicyDecisionRecord,
        intent: EffectIntent,
    ) -> Result<WorldEventId, WorldError> {
        if !policy_record.decision.is_allowed() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "effect publication requires an allowed policy decision".to_string(),
            });
        }
        let expected_record =
            PolicyDecisionRecord::from_intent(&intent, policy_record.decision.clone());
        if policy_record != expected_record {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "effect policy record does not match intent".to_string(),
            });
        }
        let (allocated_intent_seq, next_intent_id, next_intent_id_era) =
            Self::preview_next_intent_seq(self.next_intent_id, self.next_intent_id_era);
        let expected_intent_id = format!("intent-{allocated_intent_seq}");
        if intent.intent_id != expected_intent_id
            || policy_record.intent_id != expected_intent_id
            || intent.birth_era != self.effect_birth_era_for(self.next_intent_id_era)
        {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "effect intent sequence does not match the world allocator".to_string(),
            });
        }

        let (pending_effects, pending_effects_evicted) =
            self.prepare_pending_effect_queue(intent.clone())?;
        let prepared = self.prepare_no_state_event_batch(vec![
            WorldEventBody::PolicyDecisionRecorded(policy_record),
            WorldEventBody::EffectQueued(intent),
        ])?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }

        self.next_intent_id = next_intent_id;
        self.next_intent_id_era = next_intent_id_era;
        self.pending_effects = pending_effects;
        self.runtime_backpressure_stats.pending_effects_evicted = self
            .runtime_backpressure_stats
            .pending_effects_evicted
            .saturating_add(pending_effects_evicted);
        let effect_event_id = prepared
            .events
            .last()
            .map(|event| event.id)
            .expect("effect publication batch contains policy and effect events");
        self.install_prepared_no_state_event_batch(prepared)?;
        Ok(effect_event_id)
    }

    /// Commit the sole policy disposition for a deterministic policy deny.
    /// The intent allocator advances with the audit record, while no queue
    /// entry is installed. Capability preflight never reaches this seam.
    pub(super) fn append_effect_policy_decision_atomically(
        &mut self,
        policy_record: PolicyDecisionRecord,
    ) -> Result<WorldEventId, WorldError> {
        if policy_record.decision.is_allowed() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "policy disposition publication requires a deny decision".to_string(),
            });
        }
        let (allocated_intent_seq, next_intent_id, next_intent_id_era) =
            Self::preview_next_intent_seq(self.next_intent_id, self.next_intent_id_era);
        let expected_intent_id = format!("intent-{allocated_intent_seq}");
        if policy_record.intent_id != expected_intent_id {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "policy decision sequence does not match the world allocator".to_string(),
            });
        }
        let prepared =
            self.prepare_no_state_event_batch(vec![WorldEventBody::PolicyDecisionRecorded(
                policy_record,
            )])?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }

        self.next_intent_id = next_intent_id;
        self.next_intent_id_era = next_intent_id_era;
        let event_id = prepared
            .events
            .first()
            .map(|event| event.id)
            .expect("policy disposition batch contains one event");
        self.install_prepared_no_state_event_batch(prepared)?;
        Ok(event_id)
    }

    pub(super) fn prepare_pending_effect_queue(
        &self,
        intent: EffectIntent,
    ) -> Result<(VecDeque<EffectIntent>, u64), WorldError> {
        let max_len = self.runtime_memory_limits.max_pending_effects.max(1);
        if self.pending_effects.len() >= max_len
            && !self.pending_effects.iter().any(|pending| {
                !self
                    .capability_effect_receipt_links
                    .contains_key(&pending.identity_key())
            })
        {
            return Err(WorldError::CapabilityAuthorizationDenied {
                reason: "effect queue is full of authorization-linked intents".to_string(),
            });
        }

        let mut pending_effects = self.pending_effects.clone();
        pending_effects.push_back(intent);
        let mut evicted = 0_u64;
        while pending_effects.len() > max_len {
            let Some(eviction_index) = pending_effects.iter().position(|pending| {
                !self
                    .capability_effect_receipt_links
                    .contains_key(&pending.identity_key())
            }) else {
                break;
            };
            let _ = pending_effects.remove(eviction_index);
            evicted = evicted.saturating_add(1);
        }
        Ok((pending_effects, evicted))
    }

    fn prepare_no_state_event_batch(
        &self,
        bodies: Vec<WorldEventBody>,
    ) -> Result<PreparedNoStateEventBatch, WorldError> {
        if bodies.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "prepared event batch cannot be empty".to_string(),
            });
        }
        let mut events = Vec::with_capacity(bodies.len());
        let mut next_event_id = self.next_event_id;
        let mut next_event_id_era = self.next_event_id_era;
        for body in bodies {
            if !matches!(
                body,
                WorldEventBody::PolicyDecisionRecorded(_) | WorldEventBody::EffectQueued(_)
            ) {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: "prepared effect event batch contains an unsupported body".to_string(),
                });
            }
            let (event_id, next_id, next_era) =
                Self::preview_next_event_id(next_event_id, next_event_id_era);
            events.push(WorldEvent {
                id: event_id,
                time: self.state.time,
                caused_by: None,
                body,
            });
            next_event_id = next_id;
            next_event_id_era = next_era;
        }

        let mut journal_events = self.journal.events.clone();
        journal_events.extend(events.iter().cloned());
        let max_len = self.runtime_memory_limits.max_journal_events.max(1);
        let overflow = journal_events.len().saturating_sub(max_len);
        if overflow > 0 {
            journal_events.drain(0..overflow);
        }
        let tick_events: Vec<WorldEvent> = journal_events
            .iter()
            .filter(|journal_event| journal_event.time == self.state.time)
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

        Ok(PreparedNoStateEventBatch {
            events,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted: overflow as u64,
            consensus_record,
        })
    }

    fn install_prepared_no_state_event_batch(
        &mut self,
        prepared: PreparedNoStateEventBatch,
    ) -> Result<(), WorldError> {
        self.install_prepared_tick_consensus_record(prepared.consensus_record)?;
        self.next_event_id = prepared.next_event_id;
        self.next_event_id_era = prepared.next_event_id_era;
        self.journal.events = prepared.journal_events;
        self.runtime_backpressure_stats.journal_events_evicted = self
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(prepared.journal_events_evicted);
        Ok(())
    }
}
