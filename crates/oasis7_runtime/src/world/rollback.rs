use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use super::super::util::{hash_json, sha256_hex, to_canonical_cbor};
use super::super::{
    Journal, RollbackAuthorityRegistry, RollbackAuthorityRole, RollbackAuthorizationEnvelope,
    RollbackEvent, RollbackNonceOutcome, RollbackOutcomeRecoveryMetadata, RollbackReadiness,
    RollbackReadinessEvidence, RollbackReceiptProjection, RollbackSourceEventIdentity, Snapshot,
    WorldError,
};
use super::World;
#[path = "rollback_v2.rs"]
mod v2;

#[derive(serde::Serialize)]
struct RollbackJournalCommitment<'a> {
    domain: &'static str,
    snapshot_hash: &'a str,
    snapshot_journal_len: usize,
    target_journal_len: usize,
    events: &'a [super::super::WorldEvent],
}

#[cfg(test)]
mod status_aware_attribution_tests {
    use super::*;

    fn world_with_unknown(status: super::super::super::RollbackDispositionStatus) -> World {
        let mut world = World::new();
        world.rollback_nonce_outcomes.insert(
            "nonce-status-aware".to_string(),
            super::super::super::RollbackNonceOutcome {
                canonical_intent_hash: "intent".to_string(),
                rollback_checkpoint_batch_id: "checkpoint".to_string(),
                rollback_checkpoint_snapshot_hash: "snapshot".to_string(),
                rollback_checkpoint_journal_len: 0,
                rollback_ticket: "ticket".to_string(),
                target_journal_commitment: "commitment".to_string(),
                target_state_root: "root".to_string(),
                target_journal_len: 0,
                affected_census_digest: "census".to_string(),
                affected_census_count: 1,
                readiness_blockers: vec!["player_attribution_incomplete".to_string()],
                rollback_event_id: 1,
                target_batch_id: "target".to_string(),
                prior_reorg_epoch: 0,
                committed_reorg_epoch: 1,
                invalidated_batch_ids: Vec::new(),
                dispositions: vec![super::super::super::RollbackEventDisposition {
                    source_batch_id: "batch".to_string(),
                    source_event_id: 7,
                    status,
                    compensation: None,
                    player_id: None,
                    action_id: Some("action-7".to_string()),
                    system_authored: false,
                }],
                receipt: None,
            },
        );
        world
    }

    fn assert_non_compensation_player_resolution(
        status: super::super::super::RollbackDispositionStatus,
    ) {
        let mut world = world_with_unknown(status);
        let source = super::super::super::RollbackSourceEventIdentity {
            source_batch_id: "batch".to_string(),
            source_event_id: 7,
        };
        assert!(
            world
                .resolve_rollback_action_attribution(
                    "nonce-status-aware",
                    &source,
                    "player-7".to_string(),
                    super::super::super::RollbackCompensationCaseRef {
                        owner_id: "must-not-be-used".to_string(),
                        ticket_id: "must-not-be-used".to_string(),
                        state: super::super::super::RollbackCompensationState::PendingAuthorization,
                    },
                )
                .expect("non-compensation attribution resolves")
        );
        assert!(
            !world
                .resolve_rollback_action_attribution(
                    "nonce-status-aware",
                    &source,
                    "player-7".to_string(),
                    super::super::super::RollbackCompensationCaseRef {
                        owner_id: "ignored-on-retry".to_string(),
                        ticket_id: "ignored-on-retry".to_string(),
                        state: super::super::super::RollbackCompensationState::PendingAuthorization,
                    },
                )
                .expect("exact player attribution retry is a no-op")
        );
        let disposition = &world
            .rollback_nonce_outcome("nonce-status-aware")
            .expect("outcome")
            .dispositions[0];
        assert_eq!(disposition.status, status);
        assert_eq!(disposition.player_id.as_deref(), Some("player-7"));
        assert!(disposition.compensation.is_none());
        assert!(!disposition.system_authored);
        let snapshot = world.snapshot();
        let journal = world.journal().clone();
        let mut world = World::from_snapshot(snapshot, journal).expect("restart from snapshot");
        let restored = &world
            .rollback_nonce_outcome("nonce-status-aware")
            .expect("restored outcome")
            .dispositions[0];
        assert_eq!(restored.status, status);
        assert_eq!(restored.player_id.as_deref(), Some("player-7"));
        assert!(restored.compensation.is_none());
        assert!(matches!(
            world.resolve_rollback_action_as_system_authored("nonce-status-aware", &source),
            Err(WorldError::RollbackAttributionResolutionConflict { .. })
        ));
    }

    #[test]
    fn replayed_unknown_accepts_player_without_fabricating_compensation() {
        assert_non_compensation_player_resolution(
            super::super::super::RollbackDispositionStatus::Replayed,
        );
    }

    #[test]
    fn preserved_unknown_accepts_player_without_fabricating_compensation() {
        assert_non_compensation_player_resolution(
            super::super::super::RollbackDispositionStatus::PreservedAtTarget,
        );
    }

    #[test]
    fn rejected_player_and_system_terminal_retries_report_no_change() {
        let source = super::super::super::RollbackSourceEventIdentity {
            source_batch_id: "batch".to_string(),
            source_event_id: 7,
        };
        let case = || super::super::super::RollbackCompensationCaseRef {
            owner_id: "support".to_string(),
            ticket_id: "ticket-7".to_string(),
            state: super::super::super::RollbackCompensationState::PendingAuthorization,
        };
        let mut player_world =
            world_with_unknown(super::super::super::RollbackDispositionStatus::RejectedFork);
        assert!(
            player_world
                .resolve_rollback_action_attribution(
                    "nonce-status-aware",
                    &source,
                    "player-7".to_string(),
                    case(),
                )
                .expect("first player resolution changes state")
        );
        assert!(
            !player_world
                .resolve_rollback_action_attribution(
                    "nonce-status-aware",
                    &source,
                    "player-7".to_string(),
                    case(),
                )
                .expect("compensation-required retry is unchanged")
        );
        assert!(matches!(
            player_world.resolve_rollback_action_attribution(
                "nonce-status-aware",
                &source,
                "player-8".to_string(),
                case(),
            ),
            Err(WorldError::RollbackAttributionResolutionConflict { .. })
        ));

        let mut system_world =
            world_with_unknown(super::super::super::RollbackDispositionStatus::RejectedFork);
        assert!(
            system_world
                .resolve_rollback_action_as_system_authored("nonce-status-aware", &source)
                .expect("first system resolution changes state")
        );
        assert!(
            !system_world
                .resolve_rollback_action_as_system_authored("nonce-status-aware", &source)
                .expect("system retry is unchanged")
        );
    }
}

#[derive(serde::Serialize)]
struct RollbackAffectedCensusCommitment<'a> {
    domain: &'static str,
    events: &'a [RollbackSourceEventIdentity],
}

pub fn rollback_affected_census_digest(
    affected_events: &[RollbackSourceEventIdentity],
) -> Result<String, WorldError> {
    let unique = affected_events.iter().cloned().collect::<BTreeSet<_>>();
    if unique.len() != affected_events.len() {
        return Err(WorldError::DistributedValidationFailed {
            reason: "rollback affected census requires unique source identities".to_string(),
        });
    }
    let canonical = unique.into_iter().collect::<Vec<_>>();
    Ok(sha256_hex(&to_canonical_cbor(
        &RollbackAffectedCensusCommitment {
            domain: "oasis7:rollback-affected-census:v1",
            events: canonical.as_slice(),
        },
    )?))
}

fn validate_receipt_projection(
    outcome: &RollbackNonceOutcome,
    affected_events: &[RollbackSourceEventIdentity],
    receipt: &RollbackReceiptProjection,
) -> Result<(), WorldError> {
    let census_digest = rollback_affected_census_digest(affected_events)?;
    let required = [
        receipt.receipt_id.as_str(),
        receipt.canonical_intent_digest.as_str(),
        receipt.rollback_checkpoint_batch_id.as_str(),
        receipt.rollback_checkpoint_snapshot_hash.as_str(),
        receipt.replay_target_batch_id.as_str(),
        receipt.replay_target_state_root.as_str(),
        receipt.replay_target_journal_commitment.as_str(),
        receipt.affected_census_digest.as_str(),
    ];
    if required.iter().any(|value| value.trim().is_empty())
        || receipt.canonical_intent_digest != outcome.canonical_intent_hash
        || receipt.rollback_checkpoint_snapshot_hash != outcome.rollback_checkpoint_snapshot_hash
        || receipt.rollback_checkpoint_journal_len != outcome.rollback_checkpoint_journal_len
        || receipt.replay_target_journal_len != outcome.target_journal_len
        || receipt.replay_target_state_root != outcome.target_state_root
        || receipt.replay_target_journal_commitment != outcome.target_journal_commitment
        || receipt.affected_census_digest != census_digest
        || receipt.affected_census_count != affected_events.len()
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: "rollback receipt projection does not match immutable outcome evidence"
                .to_string(),
        });
    }
    Ok(())
}

fn validate_rollback_dispositions(
    affected_events: Option<&[RollbackSourceEventIdentity]>,
    dispositions: &[super::super::RollbackEventDisposition],
) -> Result<(), WorldError> {
    let mut found = BTreeSet::new();
    for disposition in dispositions {
        let identity = RollbackSourceEventIdentity {
            source_batch_id: disposition.source_batch_id.clone(),
            source_event_id: disposition.source_event_id,
        };
        if identity.source_batch_id.trim().is_empty() || !found.insert(identity) {
            return Err(WorldError::DistributedValidationFailed {
                reason: "rollback dispositions require unique source batch/event identities"
                    .to_string(),
            });
        }
        match disposition.status {
            super::super::RollbackDispositionStatus::CompensationRequired => {
                let case = disposition.compensation.as_ref().ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "compensation_required disposition lacks case reference"
                            .to_string(),
                    }
                })?;
                if case.owner_id.trim().is_empty() || case.ticket_id.trim().is_empty() {
                    return Err(WorldError::DistributedValidationFailed {
                        reason: "compensation case requires owner and ticket".to_string(),
                    });
                }
            }
            _ if disposition.compensation.is_some() => {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "only compensation_required may reference compensation".to_string(),
                });
            }
            _ => {}
        }
    }
    if let Some(affected) = affected_events {
        let expected = affected.iter().cloned().collect::<BTreeSet<_>>();
        if expected.len() != affected.len() || expected != found {
            return Err(WorldError::DistributedValidationFailed {
                reason: "rollback disposition coverage does not exactly match affected events"
                    .to_string(),
            });
        }
    }
    Ok(())
}

pub fn rollback_journal_commitment(
    snapshot: &Snapshot,
    journal: &Journal,
    target_journal_len: usize,
) -> Result<String, WorldError> {
    if target_journal_len > journal.len() || snapshot.journal_len > target_journal_len {
        return Err(WorldError::RollbackReplayTargetInvalid {
            snapshot_journal_len: snapshot.journal_len,
            target_journal_len,
            supplied_journal_len: journal.len(),
        });
    }
    let snapshot_hash = hash_json(snapshot)?;
    Ok(sha256_hex(&to_canonical_cbor(
        &RollbackJournalCommitment {
            domain: "oasis7:rollback-journal-commitment:v1",
            snapshot_hash: snapshot_hash.as_str(),
            snapshot_journal_len: snapshot.journal_len,
            target_journal_len,
            events: &journal.events[..target_journal_len],
        },
    )?))
}

impl World {
    pub fn rollback_nonce_outcome(&self, nonce: &str) -> Option<RollbackNonceOutcome> {
        self.rollback_nonce_outcomes.get(nonce).cloned()
    }

    pub fn record_rollback_outcome_recovery_metadata(
        &mut self,
        nonce: &str,
        metadata: RollbackOutcomeRecoveryMetadata,
    ) -> Result<(), WorldError> {
        if !metadata.invalidated_batch_ids.is_empty() && metadata.dispositions.is_empty() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "rollback disposition coverage is incomplete".to_string(),
            });
        }
        validate_rollback_dispositions(None, &metadata.dispositions)?;
        let outcome = self.rollback_nonce_outcomes.get_mut(nonce).ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: format!("rollback outcome for nonce {nonce} is not committed"),
            }
        })?;
        if outcome.receipt.is_some() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "committed rollback receipt is immutable".to_string(),
            });
        }
        outcome.target_batch_id = metadata.target_batch_id;
        outcome.prior_reorg_epoch = metadata.prior_reorg_epoch;
        outcome.committed_reorg_epoch = metadata.committed_reorg_epoch;
        outcome.invalidated_batch_ids = metadata.invalidated_batch_ids;
        outcome.dispositions = metadata.dispositions;
        Ok(())
    }

    pub fn complete_rollback_outcome(
        &mut self,
        nonce: &str,
        metadata: RollbackOutcomeRecoveryMetadata,
        affected_events: &[RollbackSourceEventIdentity],
        receipt: RollbackReceiptProjection,
    ) -> Result<(), WorldError> {
        validate_rollback_dispositions(Some(affected_events), &metadata.dispositions)?;
        let census_digest = rollback_affected_census_digest(affected_events)?;
        let outcome = self.rollback_nonce_outcomes.get_mut(nonce).ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: format!("rollback outcome for nonce {nonce} is not committed"),
            }
        })?;
        if receipt.replay_target_batch_id != metadata.target_batch_id {
            return Err(WorldError::DistributedValidationFailed {
                reason: "rollback receipt target batch does not match recovery metadata"
                    .to_string(),
            });
        }
        validate_receipt_projection(outcome, affected_events, &receipt)?;
        if let Some(existing) = outcome.receipt.as_ref() {
            if existing == &receipt
                && outcome.target_batch_id == metadata.target_batch_id
                && outcome.prior_reorg_epoch == metadata.prior_reorg_epoch
                && outcome.committed_reorg_epoch == metadata.committed_reorg_epoch
                && outcome.invalidated_batch_ids == metadata.invalidated_batch_ids
                && outcome.dispositions == metadata.dispositions
            {
                return Ok(());
            }
            return Err(WorldError::DistributedValidationFailed {
                reason: "committed rollback receipt is immutable".to_string(),
            });
        }
        outcome.target_batch_id = metadata.target_batch_id;
        outcome.rollback_checkpoint_batch_id = receipt.rollback_checkpoint_batch_id.clone();
        outcome.prior_reorg_epoch = metadata.prior_reorg_epoch;
        outcome.committed_reorg_epoch = metadata.committed_reorg_epoch;
        outcome.invalidated_batch_ids = metadata.invalidated_batch_ids;
        outcome.dispositions = metadata.dispositions;
        outcome.affected_census_digest = census_digest;
        outcome.affected_census_count = affected_events.len();
        outcome.readiness_blockers = receipt.readiness_blockers.clone();
        outcome.receipt = Some(receipt);
        Ok(())
    }

    pub fn rollback_receipt_projection(&self, nonce: &str) -> Option<RollbackReceiptProjection> {
        self.rollback_nonce_outcomes
            .get(nonce)
            .and_then(|outcome| outcome.receipt.clone())
    }

    pub fn transition_rollback_compensation_case(
        &mut self,
        nonce: &str,
        source: &RollbackSourceEventIdentity,
        next: super::super::RollbackCompensationState,
    ) -> Result<(), WorldError> {
        let outcome = self.rollback_nonce_outcomes.get_mut(nonce).ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: format!("rollback outcome for nonce {nonce} is not committed"),
            }
        })?;
        let disposition = outcome
            .dispositions
            .iter_mut()
            .find(|entry| {
                entry.source_batch_id == source.source_batch_id
                    && entry.source_event_id == source.source_event_id
            })
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: "rollback compensation source is not in the affected census".to_string(),
            })?;
        let case = disposition.compensation.as_mut().ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: "rollback disposition has no accountable compensation case".to_string(),
            }
        })?;
        use super::super::RollbackCompensationState as State;
        let allowed = case.state == next
            || matches!(
                (case.state, next),
                (
                    State::PendingAuthorization,
                    State::Authorized | State::Rejected
                ) | (State::Authorized, State::InProgress | State::Rejected)
                    | (State::InProgress, State::Completed)
            );
        if !allowed {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "invalid rollback compensation transition: current={:?} next={next:?}",
                    case.state
                ),
            });
        }
        case.state = next;
        Ok(())
    }

    pub fn resolve_rollback_action_attribution(
        &mut self,
        nonce: &str,
        source: &RollbackSourceEventIdentity,
        player_id: String,
        compensation: super::super::RollbackCompensationCaseRef,
    ) -> Result<bool, WorldError> {
        if player_id.trim().is_empty() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "rollback attribution requires a player id".to_string(),
            });
        }
        let outcome = self.rollback_nonce_outcomes.get_mut(nonce).ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: format!("rollback outcome for nonce {nonce} is not committed"),
            }
        })?;
        let disposition = outcome
            .dispositions
            .iter_mut()
            .find(|entry| {
                entry.source_batch_id == source.source_batch_id
                    && entry.source_event_id == source.source_event_id
            })
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: "rollback attribution source is not in the affected census".to_string(),
            })?;
        if disposition.system_authored {
            return Err(WorldError::RollbackAttributionResolutionConflict {
                source_batch_id: source.source_batch_id.clone(),
                source_event_id: source.source_event_id,
            });
        }
        if let Some(existing_player_id) = disposition.player_id.as_deref() {
            if existing_player_id == player_id {
                return Ok(false);
            }
            return Err(WorldError::RollbackAttributionResolutionConflict {
                source_batch_id: source.source_batch_id.clone(),
                source_event_id: source.source_event_id,
            });
        }
        match disposition.status {
            super::super::RollbackDispositionStatus::RejectedFork => {
                disposition.player_id = Some(player_id);
                disposition.system_authored = false;
                disposition.status = super::super::RollbackDispositionStatus::CompensationRequired;
                disposition.compensation = Some(compensation);
            }
            super::super::RollbackDispositionStatus::Replayed
            | super::super::RollbackDispositionStatus::PreservedAtTarget => {
                disposition.player_id = Some(player_id);
                disposition.system_authored = false;
                disposition.compensation = None;
            }
            super::super::RollbackDispositionStatus::CompensationRequired => {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "rollback action attribution is immutable once resolved".to_string(),
                });
            }
        }
        Ok(true)
    }

    pub fn resolve_rollback_action_as_system_authored(
        &mut self,
        nonce: &str,
        source: &RollbackSourceEventIdentity,
    ) -> Result<bool, WorldError> {
        let outcome = self.rollback_nonce_outcomes.get_mut(nonce).ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: format!("rollback outcome for nonce {nonce} is not committed"),
            }
        })?;
        let disposition = outcome
            .dispositions
            .iter_mut()
            .find(|entry| {
                entry.source_batch_id == source.source_batch_id
                    && entry.source_event_id == source.source_event_id
            })
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: "rollback system-action source is not in the affected census".to_string(),
            })?;
        if disposition.system_authored {
            return Ok(false);
        }
        if disposition.player_id.is_some() || disposition.compensation.is_some() {
            return Err(WorldError::RollbackAttributionResolutionConflict {
                source_batch_id: source.source_batch_id.clone(),
                source_event_id: source.source_event_id,
            });
        }
        if disposition.action_id.is_none() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "only unresolved non-player action evidence may be system-authored"
                    .to_string(),
            });
        }
        disposition.system_authored = true;
        Ok(true)
    }

    pub fn validate_rollback_receipt_projection(
        &self,
        nonce: &str,
        affected_events: &[RollbackSourceEventIdentity],
    ) -> Result<(), WorldError> {
        let outcome = self.rollback_nonce_outcomes.get(nonce).ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: format!("rollback outcome for nonce {nonce} is not committed"),
            }
        })?;
        let receipt =
            outcome
                .receipt
                .as_ref()
                .ok_or_else(|| WorldError::DistributedValidationFailed {
                    reason: "rollback outcome has no immutable receipt projection".to_string(),
                })?;
        validate_rollback_dispositions(Some(affected_events), &outcome.dispositions)?;
        validate_receipt_projection(outcome, affected_events, receipt)
    }

    pub fn rollback_readiness_without_evidence(&self, nonce: &str) -> RollbackReadiness {
        let mut reasons = vec![
            "target_root_evidence_missing".to_string(),
            "epoch_evidence_missing".to_string(),
            "drift_evidence_missing".to_string(),
            "consensus_chain_evidence_missing".to_string(),
            "receipt_retrievability_evidence_missing".to_string(),
        ];
        match self.rollback_nonce_outcomes.get(nonce) {
            None => reasons.push("rollback_outcome_missing".to_string()),
            Some(outcome) => reasons.extend(outcome.readiness_blockers.clone()),
        }
        RollbackReadiness::Blocked { reasons }
    }

    pub fn rollback_readiness(
        &self,
        nonce: &str,
        affected_events: &[RollbackSourceEventIdentity],
        evidence: &RollbackReadinessEvidence,
    ) -> RollbackReadiness {
        let Some(outcome) = self.rollback_nonce_outcomes.get(nonce) else {
            return RollbackReadiness::Blocked {
                reasons: vec!["rollback_outcome_missing".to_string()],
            };
        };
        let mut reasons = Vec::new();
        for (passed, reason) in [
            (evidence.target_root_matches, "target_root_mismatch"),
            (evidence.epoch_matches, "reorg_epoch_mismatch"),
            (evidence.drift_free, "consensus_drift_present"),
            (evidence.consensus_chain_valid, "consensus_chain_invalid"),
            (evidence.receipt_retrievable, "receipt_not_retrievable"),
            (outcome.receipt.is_some(), "immutable_receipt_missing"),
        ] {
            if !passed {
                reasons.push(reason.to_string());
            }
        }
        if let Err(error) =
            validate_rollback_dispositions(Some(affected_events), &outcome.dispositions)
        {
            reasons.push(format!("disposition_coverage_invalid:{error:?}"));
        }
        if reasons.is_empty() {
            RollbackReadiness::Ready
        } else {
            RollbackReadiness::Blocked { reasons }
        }
    }

    pub fn rollback_to_snapshot(
        &mut self,
        snapshot: Snapshot,
        mut journal: Journal,
        reason: impl Into<String>,
        target_batch_id: Option<&str>,
        approval: RollbackAuthorizationEnvelope,
        now_ms: u64,
    ) -> Result<(), WorldError> {
        let prior_journal_len = journal.len();
        journal.events.truncate(snapshot.journal_len);
        self.rollback_to_snapshot_candidate(
            snapshot,
            journal,
            prior_journal_len,
            reason,
            target_batch_id,
            approval,
            None,
            now_ms,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Rollback candidate inputs bind snapshot, journal, authorization, and reconciliation state explicitly."
    )]
    fn rollback_to_snapshot_candidate(
        &mut self,
        snapshot: Snapshot,
        journal: Journal,
        prior_journal_len: usize,
        reason: impl Into<String>,
        target_batch_id: Option<&str>,
        approval: RollbackAuthorizationEnvelope,
        canonical_v2_payload: Option<&[u8]>,
        now_ms: u64,
    ) -> Result<(), WorldError> {
        let reason = reason.into();
        if snapshot.journal_len > journal.len() {
            return Err(WorldError::JournalMismatch);
        }
        let canonical_payload = match canonical_v2_payload {
            Some(payload)
                if approval.intent.schema_version == 2
                    && payload.starts_with(b"oasis7:governed-rollback-replay:v2\0") =>
            {
                payload.to_vec()
            }
            Some(_) => {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "rollback v2 canonical payload domain mismatch".to_string(),
                });
            }
            None => approval.intent.canonical_signing_payload()?,
        };
        let supplied_intent_hash = sha256_hex(&canonical_payload);
        let committed_retry =
            if let Some(committed) = self.rollback_nonce_outcomes.get(&approval.intent.nonce) {
                if committed.canonical_intent_hash != supplied_intent_hash {
                    return Err(WorldError::RollbackNonceConflict {
                        nonce: approval.intent.nonce.clone(),
                        committed_intent_hash: committed.canonical_intent_hash.clone(),
                        supplied_intent_hash,
                    });
                }
                true
            } else {
                false
            };
        let verified = self.verify_rollback_authorization(
            &snapshot,
            &reason,
            target_batch_id,
            &approval,
            canonical_payload.as_slice(),
            now_ms,
            committed_retry,
        )?;

        if approval.intent.target_journal_len < snapshot.journal_len
            || approval.intent.target_journal_len != journal.len()
        {
            return Err(WorldError::RollbackReplayTargetInvalid {
                snapshot_journal_len: snapshot.journal_len,
                target_journal_len: approval.intent.target_journal_len,
                supplied_journal_len: journal.len(),
            });
        }

        self.validate_rollback_replay_journal(&snapshot, &journal)?;
        let actual_journal_commitment =
            rollback_journal_commitment(&snapshot, &journal, approval.intent.target_journal_len)?;
        if approval.intent.schema_version >= 2 {
            let expected_journal_commitment = approval
                .intent
                .target_journal_commitment
                .as_deref()
                .unwrap_or_default();
            if expected_journal_commitment != actual_journal_commitment {
                return Err(WorldError::RollbackJournalCommitmentMismatch {
                    expected: expected_journal_commitment.to_string(),
                    actual: actual_journal_commitment,
                });
            }
        }

        let signer = self.receipt_signer.clone();
        let authority_registry = self.rollback_authority_registry.clone();
        let mut consumed_nonces = self.consumed_rollback_nonces.clone();
        consumed_nonces.insert(approval.intent.nonce.clone());
        let mut world = Self::from_snapshot(snapshot.clone(), journal)?;
        world.source_compiler = self.source_compiler.clone();
        world.receipt_signer = signer;
        world.rollback_authority_registry = authority_registry;
        world.consumed_rollback_nonces = consumed_nonces;
        world.rollback_nonce_outcomes = self.rollback_nonce_outcomes.clone();

        let actual_target_state_root = world.current_state_root_hash()?;
        if actual_target_state_root != approval.intent.expected_target_state_root {
            return Err(WorldError::RollbackTargetStateRootMismatch {
                expected: approval.intent.expected_target_state_root.clone(),
                actual: actual_target_state_root,
            });
        }
        if committed_retry {
            return Ok(());
        }

        let snapshot_hash = hash_json(&snapshot)?;
        let outcome_nonce = approval.intent.nonce.clone();
        let outcome_ticket = approval.intent.rollback_ticket.clone();
        let outcome_commitment = approval
            .intent
            .target_journal_commitment
            .clone()
            .unwrap_or_else(|| actual_journal_commitment.clone());
        let outcome_state_root = approval.intent.expected_target_state_root.clone();
        let event = RollbackEvent {
            snapshot_hash,
            snapshot_journal_len: snapshot.journal_len,
            prior_journal_len,
            reason,
            rollback_ticket: approval.intent.rollback_ticket,
            on_call_approver: verified.0.clone(),
            governance_approver: verified.1.clone(),
            on_call_authority_id: verified.0,
            governance_authority_id: verified.1,
            authorization_nonce: approval.intent.nonce,
        };
        world.append_event(super::super::WorldEventBody::RollbackApplied(event), None)?;
        let rollback_event_id = world
            .journal
            .events
            .last()
            .map(|event| event.id)
            .unwrap_or(0);
        world.rollback_nonce_outcomes.insert(
            outcome_nonce,
            RollbackNonceOutcome {
                canonical_intent_hash: supplied_intent_hash,
                rollback_checkpoint_batch_id: String::new(),
                rollback_checkpoint_snapshot_hash: approval.intent.snapshot_hash.clone(),
                rollback_checkpoint_journal_len: approval.intent.snapshot_journal_len,
                rollback_ticket: outcome_ticket,
                target_journal_commitment: outcome_commitment,
                target_state_root: outcome_state_root,
                target_journal_len: approval.intent.target_journal_len,
                affected_census_digest: String::new(),
                affected_census_count: 0,
                readiness_blockers: vec!["explicit_readiness_evidence_missing".to_string()],
                rollback_event_id,
                target_batch_id: approval.intent.target_batch_id.unwrap_or_default(),
                prior_reorg_epoch: 0,
                committed_reorg_epoch: 0,
                invalidated_batch_ids: Vec::new(),
                dispositions: Vec::new(),
                receipt: None,
            },
        );
        *self = world;
        Ok(())
    }

    pub fn rollback_to_snapshot_with_reconciliation(
        &mut self,
        snapshot: Snapshot,
        journal: Journal,
        reason: impl Into<String>,
        target_batch_id: Option<&str>,
        approval: RollbackAuthorizationEnvelope,
        now_ms: u64,
    ) -> Result<(), WorldError> {
        self.rollback_to_snapshot_with_reconciliation_payload(
            snapshot,
            journal,
            reason,
            target_batch_id,
            approval,
            None,
            now_ms,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Rollback v2 preserves the stable authorization and canonical-payload API."
    )]
    pub(crate) fn rollback_to_snapshot_with_reconciliation_v2(
        &mut self,
        snapshot: Snapshot,
        journal: Journal,
        reason: impl Into<String>,
        target_batch_id: Option<&str>,
        approval: RollbackAuthorizationEnvelope,
        canonical_v2_payload: &[u8],
        now_ms: u64,
    ) -> Result<(), WorldError> {
        self.rollback_to_snapshot_with_reconciliation_payload(
            snapshot,
            journal,
            reason,
            target_batch_id,
            approval,
            Some(canonical_v2_payload),
            now_ms,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Rollback reconciliation payload keeps each durable recovery input explicit."
    )]
    fn rollback_to_snapshot_with_reconciliation_payload(
        &mut self,
        snapshot: Snapshot,
        journal: Journal,
        reason: impl Into<String>,
        target_batch_id: Option<&str>,
        approval: RollbackAuthorizationEnvelope,
        canonical_v2_payload: Option<&[u8]>,
        now_ms: u64,
    ) -> Result<(), WorldError> {
        let mut candidate = self.clone();
        let prior_journal_len = journal.len();
        let reconciliation_tick = snapshot.state.time;
        candidate
            .rollback_to_snapshot_candidate(
                snapshot,
                journal,
                prior_journal_len,
                reason,
                target_batch_id,
                approval,
                canonical_v2_payload,
                now_ms,
            )
            .map_err(|error| match error {
                WorldError::DistributedValidationFailed { reason }
                    if reason.contains("tick consensus") =>
                {
                    WorldError::RollbackReconciliationFailed {
                        tick: reconciliation_tick,
                        reason,
                    }
                }
                other => other,
            })?;

        if let Some(drift) = candidate.first_tick_consensus_drift() {
            return Err(WorldError::RollbackReconciliationFailed {
                tick: drift.tick,
                reason: drift.reason,
            });
        }
        *self = candidate;
        Ok(())
    }

    pub fn set_rollback_authority_registry(
        &mut self,
        registry: RollbackAuthorityRegistry,
    ) -> Result<(), WorldError> {
        if registry.is_empty() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "rollback authority registry must not be empty".to_string(),
            });
        }
        self.rollback_authority_registry = registry;
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Authorization verification receives the complete signed rollback context explicitly."
    )]
    fn verify_rollback_authorization(
        &self,
        snapshot: &Snapshot,
        reason: &str,
        target_batch_id: Option<&str>,
        envelope: &RollbackAuthorizationEnvelope,
        canonical_payload: &[u8],
        now_ms: u64,
        allow_consumed_nonce: bool,
    ) -> Result<(String, String), WorldError> {
        let reject = |reason: String| WorldError::DistributedValidationFailed { reason };
        if self.rollback_authority_registry.is_empty() {
            return Err(reject(
                "rollback authority registry is not configured".to_string(),
            ));
        }
        let intent = &envelope.intent;
        let expected_hash = hash_json(snapshot)?;
        if intent.rollback_ticket.trim().is_empty()
            || intent.reason != reason
            || intent.snapshot_hash != expected_hash
            || intent.snapshot_journal_len != snapshot.journal_len
            || intent.expected_target_state_root.trim().is_empty()
            || intent.target_batch_id.as_deref() != target_batch_id
            || intent.nonce.trim().is_empty()
        {
            return Err(reject(
                "rollback authorization does not match the exact operation".to_string(),
            ));
        }
        self.verify_rollback_authorization_envelope(
            envelope,
            canonical_payload,
            now_ms,
            allow_consumed_nonce,
        )
    }

    fn authenticate_committed_rollback_retry(
        &self,
        envelope: &RollbackAuthorizationEnvelope,
        canonical_payload: &[u8],
        now_ms: u64,
    ) -> Result<bool, WorldError> {
        let Some(committed) = self.rollback_nonce_outcomes.get(&envelope.intent.nonce) else {
            return Ok(false);
        };
        let supplied_intent_hash = sha256_hex(canonical_payload);
        if committed.canonical_intent_hash != supplied_intent_hash {
            return Err(WorldError::RollbackNonceConflict {
                nonce: envelope.intent.nonce.clone(),
                committed_intent_hash: committed.canonical_intent_hash.clone(),
                supplied_intent_hash,
            });
        }
        self.verify_rollback_authorization_envelope(envelope, canonical_payload, now_ms, true)?;
        Ok(true)
    }

    fn verify_rollback_authorization_envelope(
        &self,
        envelope: &RollbackAuthorizationEnvelope,
        canonical_payload: &[u8],
        now_ms: u64,
        allow_consumed_nonce: bool,
    ) -> Result<(String, String), WorldError> {
        let reject = |reason: String| WorldError::DistributedValidationFailed { reason };
        if self.rollback_authority_registry.is_empty() {
            return Err(reject(
                "rollback authority registry is not configured".to_string(),
            ));
        }
        let intent = &envelope.intent;
        if intent.issued_at_ms > now_ms
            || intent.expires_at_ms < now_ms
            || intent.expires_at_ms <= intent.issued_at_ms
        {
            return Err(reject(
                "rollback authorization is not currently valid".to_string(),
            ));
        }
        if !allow_consumed_nonce && self.consumed_rollback_nonces.contains(&intent.nonce) {
            return Err(reject(
                "rollback authorization nonce was already consumed".to_string(),
            ));
        }
        if envelope.signatures.len() != 2 {
            return Err(reject(
                "rollback authorization requires exactly two signatures".to_string(),
            ));
        }
        let mut approved = BTreeMap::new();
        let mut approved_keys = BTreeSet::new();
        for approval in &envelope.signatures {
            if approval.signature_scheme != "ed25519" {
                return Err(reject(
                    "rollback authorization requires Ed25519 signatures".to_string(),
                ));
            }
            let record = self
                .rollback_authority_registry
                .get(&approval.authority_id)
                .ok_or_else(|| {
                    reject(format!(
                        "unknown rollback authority {}",
                        approval.authority_id
                    ))
                })?;
            if !record.active || record.role != approval.role {
                return Err(reject(format!(
                    "rollback authority {} is inactive or has the wrong role",
                    approval.authority_id
                )));
            }
            let public_key: [u8; 32] = hex::decode(record.public_key_hex.trim())
                .map_err(|_| reject("invalid rollback authority public key".to_string()))?
                .try_into()
                .map_err(|_| reject("invalid rollback authority public key length".to_string()))?;
            if !approved_keys.insert(public_key)
                || approved
                    .insert(approval.role, approval.authority_id.clone())
                    .is_some()
            {
                return Err(reject(
                    "rollback approvals must use distinct authorities, keys, and roles".to_string(),
                ));
            }
            let signature: [u8; 64] = hex::decode(approval.signature_hex.trim())
                .map_err(|_| reject("invalid rollback approval signature".to_string()))?
                .try_into()
                .map_err(|_| reject("invalid rollback approval signature length".to_string()))?;
            VerifyingKey::from_bytes(&public_key)
                .map_err(|_| reject("invalid rollback authority public key".to_string()))?
                .verify(canonical_payload, &Signature::from_bytes(&signature))
                .map_err(|_| {
                    reject("rollback approval signature verification failed".to_string())
                })?;
        }
        let on_call = approved
            .remove(&RollbackAuthorityRole::OnCall)
            .ok_or_else(|| {
                reject("rollback authorization is missing on-call approval".to_string())
            })?;
        let governance = approved
            .remove(&RollbackAuthorityRole::Governance)
            .ok_or_else(|| {
                reject("rollback authorization is missing governance approval".to_string())
            })?;
        Ok((on_call, governance))
    }

    fn validate_rollback_replay_journal(
        &self,
        snapshot: &Snapshot,
        journal: &Journal,
    ) -> Result<(), WorldError> {
        if snapshot.journal_len > journal.len() {
            return Err(WorldError::JournalMismatch);
        }
        if snapshot.journal_len > 0 {
            let boundary = &journal.events[snapshot.journal_len - 1];
            if boundary.id != snapshot.last_event_id {
                return Err(WorldError::RollbackReplayJournalInvalid {
                    index: snapshot.journal_len - 1,
                    expected_event_id: snapshot.last_event_id,
                    found_event_id: boundary.id,
                });
            }
        }
        let mut expected_event_id = snapshot.last_event_id.saturating_add(1);
        for (index, event) in journal.events.iter().enumerate().skip(snapshot.journal_len) {
            if event.id != expected_event_id {
                return Err(WorldError::RollbackReplayJournalInvalid {
                    index,
                    expected_event_id,
                    found_event_id: event.id,
                });
            }
            expected_event_id = expected_event_id.saturating_add(1);
        }
        Ok(())
    }
}
