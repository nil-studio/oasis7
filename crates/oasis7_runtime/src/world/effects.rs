use super::super::util::sha256_hex;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, VecDeque};

use super::super::capability_authorization::CapabilityAuthorizationAuditReceipt;
use super::super::{
    CapabilityAuthorizationEvent, CausedBy, EffectIntent, EffectOrigin, EffectReceipt,
    PolicyDecisionRecord, WorldError, WorldEvent, WorldEventBody, WorldEventId,
};
use super::World;

struct PreparedReceiptIngestion {
    authorization_receipt_update: Option<CapabilityAuthorizationAuditReceipt>,
    authorization_link_removed: Option<String>,
    capability_authorization_root: Option<String>,
    pending_effects: VecDeque<EffectIntent>,
    inflight_effects: BTreeMap<String, EffectIntent>,
    events: Vec<WorldEvent>,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
    consensus_record: super::super::TickConsensusRecord,
}

impl World {
    // ---------------------------------------------------------------------
    // Effect handling
    // ---------------------------------------------------------------------

    pub fn take_next_effect(&mut self) -> Option<EffectIntent> {
        if self.inflight_effect_capacity_reached() {
            self.record_inflight_effect_dispatch_blocked();
            return None;
        }
        let key = self.pending_effects.front()?.identity_key();
        if self.inflight_effects.contains_key(&key) {
            return None;
        }
        let intent = self.pending_effects.pop_front()?;
        self.inflight_effects.insert(key, intent.clone());
        Some(intent)
    }

    pub fn emit_effect(
        &mut self,
        kind: impl Into<String>,
        params: JsonValue,
        cap_ref: impl Into<String>,
        origin: EffectOrigin,
    ) -> Result<String, WorldError> {
        // Public effect emission is one transition.  Capability admission is
        // deliberately performed before previewing the rolling intent
        // sequence, and the policy audit plus queue event are installed by a
        // single prepared batch below.  This keeps a rejected preflight from
        // consuming an id and prevents a late publication failure from
        // leaving an audit event or sequence gap behind.
        let (intent, record) = self.prepare_effect_intent(kind, params, cap_ref, origin)?;
        if !record.decision.is_allowed() {
            let intent_id = intent.intent_id.clone();
            let reason = record
                .decision
                .reason()
                .unwrap_or_else(|| "policy_deny".to_string());
            self.append_effect_policy_decision_atomically(record)?;
            return Err(WorldError::PolicyDenied { intent_id, reason });
        }
        let intent_id = intent.intent_id.clone();
        self.append_effect_publication_atomically(record, intent)?;
        Ok(intent_id)
    }

    fn prepare_effect_intent(
        &self,
        kind: impl Into<String>,
        params: JsonValue,
        cap_ref: impl Into<String>,
        origin: EffectOrigin,
    ) -> Result<(EffectIntent, PolicyDecisionRecord), WorldError> {
        let kind = kind.into();
        let cap_ref = cap_ref.into();
        let grant =
            self.capabilities
                .get(&cap_ref)
                .ok_or_else(|| WorldError::CapabilityMissing {
                    cap_ref: cap_ref.clone(),
                })?;

        if grant.is_expired(self.state.time) {
            return Err(WorldError::CapabilityExpired { cap_ref });
        }

        if !grant.allows(&kind) {
            return Err(WorldError::CapabilityNotAllowed { cap_ref, kind });
        }

        let (allocated_intent_seq, _, _) =
            Self::preview_next_intent_seq(self.next_intent_id, self.next_intent_id_era);
        let intent_id = format!("intent-{allocated_intent_seq}");
        let intent = EffectIntent {
            birth_era: self.effect_birth_era_for(self.next_intent_id_era),
            intent_id,
            kind,
            params,
            cap_ref,
            origin,
        };
        let decision = self.policies.decide(&intent);
        let record = PolicyDecisionRecord::from_intent(&intent, decision);
        Ok((intent, record))
    }

    pub(super) fn build_effect_intent(
        &mut self,
        kind: impl Into<String>,
        params: JsonValue,
        cap_ref: impl Into<String>,
        origin: EffectOrigin,
    ) -> Result<EffectIntent, WorldError> {
        let kind = kind.into();
        let cap_ref = cap_ref.into();
        let birth_era = self.effect_birth_era_for(self.next_intent_id_era);
        let intent_id = format!("intent-{}", self.allocate_next_intent_seq());

        let intent = EffectIntent {
            birth_era,
            intent_id: intent_id.clone(),
            kind: kind.clone(),
            params,
            cap_ref: cap_ref.clone(),
            origin,
        };

        let grant =
            self.capabilities
                .get(&cap_ref)
                .ok_or_else(|| WorldError::CapabilityMissing {
                    cap_ref: cap_ref.clone(),
                })?;

        if grant.is_expired(self.state.time) {
            return Err(WorldError::CapabilityExpired { cap_ref });
        }

        if !grant.allows(&kind) {
            return Err(WorldError::CapabilityNotAllowed { cap_ref, kind });
        }

        let decision = self.policies.decide(&intent);
        let record = PolicyDecisionRecord::from_intent(&intent, decision.clone());
        self.append_event(WorldEventBody::PolicyDecisionRecorded(record), None)?;

        if !decision.is_allowed() {
            return Err(WorldError::PolicyDenied {
                intent_id,
                reason: decision
                    .reason()
                    .unwrap_or_else(|| "policy_deny".to_string()),
            });
        }

        Ok(intent)
    }

    pub fn ingest_receipt(&mut self, receipt: EffectReceipt) -> Result<WorldEventId, WorldError> {
        let prepared = self.prepare_receipt_ingestion(receipt)?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }
        self.install_prepared_receipt_ingestion(prepared)
    }

    fn prepare_receipt_ingestion(
        &self,
        mut receipt: EffectReceipt,
    ) -> Result<PreparedReceiptIngestion, WorldError> {
        let identity_key = self.effect_receipt_key(&receipt)?;

        let mut authorization_receipt_update = None;
        let mut authorization_link_removed = None;
        let mut capability_authorization_root = None;
        let closure_body = if let Some(link) =
            self.capability_effect_receipt_links.get(&identity_key)
        {
            let authorization_receipt_id = link.authorization_receipt_id.clone();
            if authorization_receipt_id.trim().is_empty() {
                return Err(WorldError::CapabilityAuthorizationDenied {
                    reason: "effect receipt authorization link is missing its receipt id"
                        .to_string(),
                });
            }
            let audit = self
                .capability_authorization_receipts
                .get(&authorization_receipt_id)
                .ok_or_else(|| WorldError::CapabilityAuthorizationDenied {
                    reason: "effect receipt authorization link has no audit receipt".to_string(),
                })?;
            let mut audit_after = audit.clone();
            if audit_after.committed_effect_receipt_id.is_none() {
                audit_after.committed_effect_receipt_id = Some(identity_key.clone());
            }
            audit_after
                .committed_effect_receipt_ids
                .insert(identity_key.clone());
            capability_authorization_root = Some(
                self.compute_capability_authorization_root_with_effect_receipt_commit(
                    &audit_after,
                    identity_key.as_str(),
                )?,
            );
            authorization_receipt_update = Some(audit_after);
            authorization_link_removed = Some(identity_key.clone());
            Some(WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::EffectReceiptCommitted {
                    intent_id: identity_key.clone(),
                    authorization_receipt_id,
                    effect_receipt_id: identity_key.clone(),
                },
            ))
        } else {
            None
        };

        let mut pending_effects = self.pending_effects.clone();
        let mut inflight_effects = self.inflight_effects.clone();
        let _ = inflight_effects.remove(&identity_key);
        pending_effects.retain(|intent| intent.identity_key() != identity_key);

        let mut events = Vec::with_capacity(2);
        let mut next_event_id = self.next_event_id;
        let mut next_event_id_era = self.next_event_id_era;
        let mut journal_events = self.journal.events.clone();
        let max_len = self.runtime_memory_limits.max_journal_events.max(1);
        let mut journal_events_evicted = 0_u64;
        if let Some(body) = closure_body {
            let (event_id, next_id, next_era) =
                Self::preview_next_event_id(next_event_id, next_event_id_era);
            let event = WorldEvent {
                id: event_id,
                time: self.state.time,
                caused_by: Some(CausedBy::Effect {
                    intent_id: identity_key.clone(),
                }),
                body,
            };
            journal_events.push(event.clone());
            let overflow = journal_events.len().saturating_sub(max_len);
            if overflow > 0 {
                journal_events.drain(0..overflow);
                journal_events_evicted = journal_events_evicted.saturating_add(overflow as u64);
            }
            events.push(event);
            next_event_id = next_id;
            next_event_id_era = next_era;
        }

        // Signature verification/signing deliberately observes the journal
        // after the authorization closure and before ReceiptAppended.  This
        // preserves the historical linked-receipt anchor order without
        // installing either event or the queue removal early.
        self.finalize_receipt_signature_with_journal(&mut receipt, journal_events.as_slice())?;
        let (event_id, next_id, next_era) =
            Self::preview_next_event_id(next_event_id, next_event_id_era);
        let receipt_event = WorldEvent {
            id: event_id,
            time: self.state.time,
            caused_by: Some(CausedBy::Effect {
                intent_id: identity_key.clone(),
            }),
            body: WorldEventBody::ReceiptAppended(receipt.clone()),
        };
        journal_events.push(receipt_event.clone());
        events.push(receipt_event);
        next_event_id = next_id;
        next_event_id_era = next_era;

        let overflow = journal_events.len().saturating_sub(max_len);
        if overflow > 0 {
            journal_events.drain(0..overflow);
            journal_events_evicted = journal_events_evicted.saturating_add(overflow as u64);
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

        Ok(PreparedReceiptIngestion {
            authorization_receipt_update,
            authorization_link_removed,
            capability_authorization_root,
            pending_effects,
            inflight_effects,
            events,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted,
            consensus_record,
        })
    }

    fn finalize_receipt_signature_with_journal(
        &self,
        receipt: &mut EffectReceipt,
        journal_events: &[WorldEvent],
    ) -> Result<(), WorldError> {
        let Some(signer) = &self.receipt_signer else {
            return Ok(());
        };
        let consensus_height = journal_events.len() as u64 + 1;
        let receipts_root =
            self.compute_next_receipts_root_for_events(receipt, consensus_height, journal_events)?;

        if let Some(signature) = &receipt.signature {
            signer.verify(
                receipt,
                signature,
                &self.state.node_identity_bindings,
                consensus_height,
                receipts_root.as_str(),
            )?;
        } else {
            let signature = signer.sign(receipt, consensus_height, receipts_root.as_str())?;
            receipt.signature = Some(signature);
        }
        Ok(())
    }

    fn compute_next_receipts_root_for_events(
        &self,
        receipt: &EffectReceipt,
        consensus_height: u64,
        events: &[WorldEvent],
    ) -> Result<String, WorldError> {
        let mut root = "0".repeat(64);
        for (idx, event) in events.iter().enumerate() {
            let WorldEventBody::ReceiptAppended(existing_receipt) = &event.body else {
                continue;
            };
            let leaf_hash = receipt_leaf_hash(existing_receipt)?;
            root = advance_receipts_root(root.as_str(), idx as u64 + 1, leaf_hash.as_str());
        }
        let next_leaf_hash = receipt_leaf_hash(receipt)?;
        Ok(advance_receipts_root(
            root.as_str(),
            consensus_height,
            next_leaf_hash.as_str(),
        ))
    }

    fn install_prepared_receipt_ingestion(
        &mut self,
        prepared: PreparedReceiptIngestion,
    ) -> Result<WorldEventId, WorldError> {
        self.install_prepared_tick_consensus_record(prepared.consensus_record)?;
        if let Some(audit) = prepared.authorization_receipt_update {
            self.capability_authorization_receipts
                .insert(audit.receipt_id.clone(), audit);
        }
        if let Some(intent_id) = prepared.authorization_link_removed {
            self.capability_effect_receipt_links.remove(&intent_id);
        }
        if let Some(root) = prepared.capability_authorization_root {
            self.capability_authorization_root = root;
        }
        self.pending_effects = prepared.pending_effects;
        self.inflight_effects = prepared.inflight_effects;
        self.next_event_id = prepared.next_event_id;
        self.next_event_id_era = prepared.next_event_id_era;
        self.journal.events = prepared.journal_events;
        self.runtime_backpressure_stats.journal_events_evicted = self
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(prepared.journal_events_evicted);

        Ok(prepared
            .events
            .last()
            .map(|event| event.id)
            .expect("prepared receipt ingestion contains an event"))
    }
}

fn advance_receipts_root(previous_root: &str, consensus_height: u64, leaf_hash: &str) -> String {
    let payload = format!("receipts-root:v1|{previous_root}|{consensus_height}|{leaf_hash}");
    sha256_hex(payload.as_bytes())
}

fn receipt_leaf_hash(receipt: &EffectReceipt) -> Result<String, WorldError> {
    if let Some(era) = receipt.birth_era {
        let bytes = serde_json::to_vec(&(
            "oasis7.world.effect.receipt-leaf.v2",
            &receipt.world,
            era,
            &receipt.intent_id,
            &receipt.intent_payload_digest,
            &receipt.status,
            &receipt.payload,
            receipt.cost_cents,
        ))?;
        return Ok(sha256_hex(&bytes));
    }
    #[derive(serde::Serialize)]
    struct ReceiptLeaf<'a> {
        intent_id: &'a str,
        status: &'a str,
        payload: &'a JsonValue,
        cost_cents: Option<u64>,
    }

    let leaf = ReceiptLeaf {
        intent_id: &receipt.intent_id,
        status: &receipt.status,
        payload: &receipt.payload,
        cost_cents: receipt.cost_cents,
    };
    let bytes = serde_json::to_vec(&leaf)?;
    Ok(sha256_hex(bytes.as_slice()))
}
