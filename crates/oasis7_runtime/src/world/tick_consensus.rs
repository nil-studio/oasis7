use super::super::state::CommandStateOverlay;
use super::super::util::{hash_json, sha256_hex};
use super::super::{
    BodyOverlay, CausedBy, ProductValidationDeliveryCursor, RuntimeCommittedTickContext,
    TICK_BLOCK_HEADER_SCHEMA_V1, TICK_BLOCK_HEADER_SCHEMA_V2, TickBlock, TickBlockHeader,
    TickCertificate, TickConsensusDriftReport, TickConsensusRecord,
    TickConsensusRejectionAuditEvent, TickConsensusSubmissionRole, TickExecutionDigest, WorldError,
    WorldEvent, WorldEventBody, WorldEventId, WorldStateProjection, WorldTime,
};
use super::World;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[path = "tick_consensus_events.rs"]
mod retained_events;

const TICK_CONSENSUS_EPOCH_LEN: u64 = 360;
pub(super) const TICK_CONSENSUS_GENESIS_PARENT_HASH: &str = "genesis";
const TICK_CERT_SIGNATURE_PREFIX: &str = "sha256:";
const MAX_TICK_CONSENSUS_REJECTION_AUDIT_EVENTS: usize = 2_048;

#[derive(Serialize)]
struct TickEventHashInput<'a> {
    id: WorldEventId,
    caused_by: &'a Option<CausedBy>,
    body: &'a WorldEventBody,
}

#[derive(Serialize)]
pub(super) struct StateRootProjection<'a, T: ?Sized> {
    pub(super) state: &'a T,
    pub(super) manifest_hash: &'a str,
    pub(super) policy_hash: &'a str,
}

impl World {
    pub fn latest_tick_consensus_record(&self) -> Option<&TickConsensusRecord> {
        self.tick_consensus_records.last()
    }

    pub fn first_tick_consensus_drift(&self) -> Option<TickConsensusDriftReport> {
        let boundary = match self.formal_tick_consensus_boundary() {
            Ok(boundary) => boundary,
            Err(err) => {
                return Some(TickConsensusDriftReport {
                    tick: self.state.time,
                    reason: format!("{err:?}"),
                });
            }
        };
        let mut expected_parent = boundary
            .as_ref()
            .map(|r| r.certificate.block_hash.clone())
            .unwrap_or_else(|| TICK_CONSENSUS_GENESIS_PARENT_HASH.to_string());
        let Some(mut expected_height) = self.formal_archived_tick_consensus_count.checked_add(1)
        else {
            return Some(TickConsensusDriftReport {
                tick: self.state.time,
                reason: "Tick height overflow".into(),
            });
        };
        for record in &self.tick_consensus_records {
            if let Err(err) = self.validate_tick_consensus_record_metadata(record) {
                return Some(TickConsensusDriftReport {
                    tick: record.block.header.tick,
                    reason: format!("{err:?}"),
                });
            }
            if record.block.header.parent_hash != expected_parent {
                return Some(TickConsensusDriftReport {
                    tick: record.block.header.tick,
                    reason: format!(
                        "tick consensus parent hash mismatch tick={} expected={} found={}",
                        record.block.header.tick, expected_parent, record.block.header.parent_hash
                    ),
                });
            }
            if record.certificate.consensus_height != expected_height {
                return Some(TickConsensusDriftReport {
                    tick: record.block.header.tick,
                    reason: format!(
                        "tick consensus height mismatch tick={} expected={} found={}",
                        record.block.header.tick,
                        expected_height,
                        record.certificate.consensus_height
                    ),
                });
            }
            let block_hash = record.block.block_hash();
            if record.certificate.block_hash != block_hash {
                return Some(TickConsensusDriftReport {
                    tick: record.block.header.tick,
                    reason: format!(
                        "tick consensus block hash mismatch tick={} expected={} found={}",
                        record.block.header.tick, block_hash, record.certificate.block_hash
                    ),
                });
            }
            let events_hash = match self.hash_tick_events_from_ids(
                record.block.header.tick,
                &record.block.ordered_event_ids,
            ) {
                Ok(value) => value,
                Err(err) => {
                    return Some(TickConsensusDriftReport {
                        tick: record.block.header.tick,
                        reason: format!("{err:?}"),
                    });
                }
            };
            if events_hash != record.block.header.events_hash {
                return Some(TickConsensusDriftReport {
                    tick: record.block.header.tick,
                    reason: format!(
                        "tick events hash mismatch tick={} expected={} found={}",
                        record.block.header.tick, events_hash, record.block.header.events_hash
                    ),
                });
            }
            expected_parent = record.certificate.block_hash.clone();
            expected_height = expected_height.saturating_add(1);
        }
        if let Some(record) = self.tick_consensus_records.last() {
            let current_state_root = match self.current_state_root_hash() {
                Ok(value) => value,
                Err(err) => {
                    return Some(TickConsensusDriftReport {
                        tick: record.block.header.tick,
                        reason: format!("{err:?}"),
                    });
                }
            };
            if record.block.header.state_root != current_state_root {
                return Some(TickConsensusDriftReport {
                    tick: record.block.header.tick,
                    reason: format!(
                        "latest state root mismatch tick={} expected={} found={}",
                        record.block.header.tick,
                        current_state_root,
                        record.block.header.state_root
                    ),
                });
            }
        }
        None
    }

    pub fn set_tick_consensus_authority_source(
        &mut self,
        source_node_id: &str,
    ) -> Result<(), WorldError> {
        let validated = self.validate_tick_consensus_source_node(source_node_id)?;
        self.tick_consensus_authority_source = validated;
        Ok(())
    }

    pub fn clear_tick_consensus_rejection_audit_events(&mut self) {
        self.tick_consensus_rejection_audit_events.clear();
    }

    pub fn record_tick_consensus_propagation_for_tick(
        &mut self,
        tick: WorldTime,
        source_node_id: &str,
    ) -> Result<(), WorldError> {
        self.record_tick_consensus_submission_for_tick(
            tick,
            source_node_id,
            TickConsensusSubmissionRole::Propagation,
        )
    }

    pub fn record_tick_consensus_authority_for_tick(
        &mut self,
        tick: WorldTime,
        source_node_id: &str,
    ) -> Result<(), WorldError> {
        self.record_tick_consensus_submission_for_tick(
            tick,
            source_node_id,
            TickConsensusSubmissionRole::Authority,
        )
    }

    pub fn verify_tick_consensus_chain(&self) -> Result<(), WorldError> {
        if let Some(report) = self.first_tick_consensus_drift() {
            return Err(WorldError::DistributedValidationFailed {
                reason: report.reason,
            });
        }
        Ok(())
    }

    pub(super) fn record_tick_consensus(&mut self) -> Result<(), WorldError> {
        self.record_tick_consensus_for_tick(self.state.time)
    }

    pub(super) fn record_tick_consensus_for_tick(
        &mut self,
        tick: WorldTime,
    ) -> Result<(), WorldError> {
        let authority_source = self.tick_consensus_authority_source.clone();
        self.record_tick_consensus_submission_for_tick(
            tick,
            authority_source.as_str(),
            TickConsensusSubmissionRole::Authority,
        )
    }

    pub(super) fn record_tick_consensus_for_committed_context(
        &mut self,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        let authority_source = self.tick_consensus_authority_source.clone();
        self.record_tick_consensus_submission_for_tick_with_context(
            context.height,
            authority_source.as_str(),
            TickConsensusSubmissionRole::Authority,
            Some(context),
        )
    }

    fn record_tick_consensus_submission_for_tick(
        &mut self,
        tick: WorldTime,
        source_node_id: &str,
        submission_role: TickConsensusSubmissionRole,
    ) -> Result<(), WorldError> {
        self.record_tick_consensus_submission_for_tick_with_context(
            tick,
            source_node_id,
            submission_role,
            None,
        )
    }

    fn record_tick_consensus_submission_for_tick_with_context(
        &mut self,
        tick: WorldTime,
        source_node_id: &str,
        submission_role: TickConsensusSubmissionRole,
        committed_context: Option<&RuntimeCommittedTickContext>,
    ) -> Result<(), WorldError> {
        let source_node_id = self.validate_tick_consensus_source_node(source_node_id)?;
        let record = self.build_tick_consensus_record_for_submission(
            tick,
            source_node_id.as_str(),
            submission_role,
            committed_context,
        )?;
        self.commit_tick_consensus_record_submission(record)?;
        self.verify_latest_tick_consensus_record()
    }

    fn build_tick_consensus_record_for_submission(
        &self,
        tick: WorldTime,
        source_node_id: &str,
        submission_role: TickConsensusSubmissionRole,
        committed_context: Option<&RuntimeCommittedTickContext>,
    ) -> Result<TickConsensusRecord, WorldError> {
        let tick_events: Vec<WorldEvent> = self
            .journal
            .events
            .iter()
            .filter(|event| event.time == tick)
            .cloned()
            .collect();
        let state_root = self.current_state_root_hash()?;
        self.build_tick_consensus_record_from_events(
            tick,
            source_node_id,
            submission_role,
            committed_context,
            &tick_events,
            state_root,
        )
    }

    pub(super) fn build_tick_consensus_record_for_prepared_events(
        &self,
        tick: WorldTime,
        tick_events: &[WorldEvent],
        state_root: String,
    ) -> Result<TickConsensusRecord, WorldError> {
        let source_node_id = self
            .validate_tick_consensus_source_node(self.tick_consensus_authority_source.as_str())?;
        self.build_tick_consensus_record_from_events(
            tick,
            source_node_id.as_str(),
            TickConsensusSubmissionRole::Authority,
            None,
            tick_events,
            state_root,
        )
    }

    pub(super) fn build_tick_consensus_record_from_events(
        &self,
        tick: WorldTime,
        source_node_id: &str,
        submission_role: TickConsensusSubmissionRole,
        committed_context: Option<&RuntimeCommittedTickContext>,
        tick_events: &[WorldEvent],
        state_root: String,
    ) -> Result<TickConsensusRecord, WorldError> {
        let ordered_event_ids: Vec<WorldEventId> =
            tick_events.iter().map(|event| event.id).collect();
        let ordered_action_ids = Self::extract_ordered_action_ids(tick_events);
        let events_hash = self.hash_tick_events(tick_events)?;
        let parent_hash = self.parent_hash_for_tick(tick)?;
        let executor_version = env!("CARGO_PKG_VERSION").to_string();
        let randomness_seed = Self::derive_tick_randomness_seed(parent_hash.as_str(), tick);
        let consensus_height = self.consensus_height_for_tick(tick)?;
        let header = TickBlockHeader {
            schema_version: committed_context
                .map(|_| TICK_BLOCK_HEADER_SCHEMA_V2)
                .unwrap_or(TICK_BLOCK_HEADER_SCHEMA_V1),
            epoch: tick / TICK_CONSENSUS_EPOCH_LEN,
            tick,
            parent_hash,
            events_hash: events_hash.clone(),
            state_root: state_root.clone(),
            executor_version,
            randomness_seed,
            chain_height: committed_context.map(|context| context.height),
            chain_slot: committed_context.map(|context| context.slot),
            chain_epoch: committed_context.map(|context| context.epoch),
            node_block_hash: committed_context.and_then(|context| {
                let node_block_hash = context.node_block_hash.trim();
                (!node_block_hash.is_empty()).then(|| node_block_hash.to_string())
            }),
            action_root: committed_context.map(|context| context.action_root.clone()),
            committed_at_unix_ms: committed_context.map(|context| context.committed_at_unix_ms),
        };
        let execution_digest = TickExecutionDigest {
            action_batch_hash: committed_context
                .map(|context| Ok(context.action_root.clone()))
                .unwrap_or_else(|| hash_json(&ordered_action_ids))?,
            domain_events_hash: Self::hash_tick_domain_events(tick_events)?,
            state_projection_hash: state_root,
        };
        let block = TickBlock {
            header,
            ordered_action_ids,
            ordered_event_ids: ordered_event_ids.clone(),
            event_count: ordered_event_ids.len() as u32,
            execution_digest,
        };
        let block_hash = block.block_hash();
        let mut signatures = BTreeMap::new();
        signatures.insert(
            source_node_id.to_string(),
            format!(
                "{}{}",
                TICK_CERT_SIGNATURE_PREFIX,
                sha256_hex(format!("tickcert:v1|{}|{}", source_node_id, block_hash).as_bytes())
            ),
        );
        let candidate = TickConsensusRecord {
            block,
            certificate: TickCertificate {
                block_hash,
                consensus_height,
                threshold: 1,
                authority_source: source_node_id.to_string(),
                submission_role,
                signatures,
            },
        };
        self.validate_formal_tick_immutability(&candidate)?;
        Ok(candidate)
    }

    pub(super) fn validate_tick_consensus_candidate_for_prepared_publication(
        &self,
        candidate: &TickConsensusRecord,
        prepared_tick_events: &[WorldEvent],
        prepared_state_root: &str,
    ) -> Result<(), WorldError> {
        self.validate_formal_tick_immutability(candidate)?;
        // This candidate is an internal, authority-only publication assembled
        // from the validated configured authority source.  It is never an
        // externally submitted candidate, so a failed check aborts the
        // prepared transaction instead of emitting the mutable rejection
        // audit used by the propagation/submission path.  The source, role,
        // signature, prepared journal, and projected root are all checked
        // before the first canonical install below.
        self.validate_tick_consensus_record_metadata(candidate)?;
        let source = candidate.certificate.authority_source.trim();
        if candidate.certificate.submission_role == TickConsensusSubmissionRole::Authority
            && source != self.tick_consensus_authority_source
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "authority submission source mismatch: expected={} found={source}",
                    self.tick_consensus_authority_source
                ),
            });
        }

        let expected_event_ids: Vec<WorldEventId> =
            prepared_tick_events.iter().map(|event| event.id).collect();
        if candidate.block.ordered_event_ids != expected_event_ids {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "prepared tick event ids mismatch tick={}",
                    candidate.block.header.tick
                ),
            });
        }
        let expected_action_ids = Self::extract_ordered_action_ids(prepared_tick_events);
        if candidate.block.ordered_action_ids != expected_action_ids {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "prepared tick action ids mismatch tick={}",
                    candidate.block.header.tick
                ),
            });
        }
        let expected_action_batch_hash = hash_json(&expected_action_ids)?;
        if candidate.block.execution_digest.action_batch_hash != expected_action_batch_hash {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "prepared tick action batch hash mismatch tick={}",
                    candidate.block.header.tick
                ),
            });
        }
        let expected_events_hash = self.hash_tick_events(prepared_tick_events)?;
        if candidate.block.header.events_hash != expected_events_hash {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "prepared tick events hash mismatch tick={}",
                    candidate.block.header.tick
                ),
            });
        }
        if candidate.block.header.state_root != prepared_state_root
            || candidate.block.execution_digest.state_projection_hash != prepared_state_root
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "prepared tick state root mismatch tick={}",
                    candidate.block.header.tick
                ),
            });
        }
        let expected_domain_events_hash = Self::hash_tick_domain_events(prepared_tick_events)?;
        if candidate.block.execution_digest.domain_events_hash != expected_domain_events_hash {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "prepared tick domain events hash mismatch tick={}",
                    candidate.block.header.tick
                ),
            });
        }
        if candidate.block.event_count != expected_event_ids.len() as u32
            || candidate.certificate.block_hash != candidate.block.block_hash()
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "prepared tick block integrity mismatch tick={}",
                    candidate.block.header.tick
                ),
            });
        }

        if candidate.certificate.consensus_height <= self.formal_archived_tick_consensus_count {
            return Err(WorldError::DistributedValidationFailed {
                reason: "cannot submit an archived Tick slot".into(),
            });
        }
        let Some(existing) = self
            .tick_consensus_records
            .iter()
            .find(|record| record.block.header.tick == candidate.block.header.tick)
        else {
            return Ok(());
        };
        if existing.certificate.submission_role == TickConsensusSubmissionRole::Authority {
            if candidate.certificate.submission_role != TickConsensusSubmissionRole::Authority {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!(
                        "non-authoritative submission rejected at tick {} because authoritative commitment already exists",
                        candidate.block.header.tick
                    ),
                });
            }
            if existing.certificate.authority_source != candidate.certificate.authority_source {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!(
                        "conflicting authority sources at tick {}: existing={} attempted={}",
                        candidate.block.header.tick,
                        existing.certificate.authority_source,
                        candidate.certificate.authority_source
                    ),
                });
            }
        } else if candidate.certificate.submission_role == TickConsensusSubmissionRole::Propagation
            && existing.certificate.authority_source != candidate.certificate.authority_source
            && existing.certificate.block_hash != candidate.certificate.block_hash
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "propagation conflict at tick {} requires authoritative adjudication",
                    candidate.block.header.tick
                ),
            });
        }
        Ok(())
    }

    pub(super) fn install_prepared_tick_consensus_record(
        &mut self,
        candidate: TickConsensusRecord,
    ) -> Result<(), WorldError> {
        self.validate_formal_tick_immutability(&candidate)?;
        if let Some(index) = self
            .tick_consensus_records
            .iter()
            .position(|record| record.block.header.tick == candidate.block.header.tick)
        {
            self.tick_consensus_records[index] = candidate;
        } else {
            self.tick_consensus_records.push(candidate);
        }
        Ok(())
    }

    fn commit_tick_consensus_record_submission(
        &mut self,
        candidate: TickConsensusRecord,
    ) -> Result<(), WorldError> {
        self.validate_formal_tick_immutability(&candidate)?;
        let tick = candidate.block.header.tick;
        let existing_index = self
            .tick_consensus_records
            .iter()
            .position(|record| record.block.header.tick == tick);
        if let Some(index) = existing_index {
            let existing = self.tick_consensus_records[index].clone();
            self.validate_tick_consensus_candidate_against_policy(&candidate, Some(&existing))?;
            self.validate_tick_consensus_candidate_against_existing(&candidate, &existing)?;
            self.tick_consensus_records[index] = candidate;
            return Ok(());
        }
        self.validate_tick_consensus_candidate_against_policy(&candidate, None)?;
        self.tick_consensus_records.push(candidate);
        Ok(())
    }

    fn validate_tick_consensus_candidate_against_policy(
        &mut self,
        candidate: &TickConsensusRecord,
        existing: Option<&TickConsensusRecord>,
    ) -> Result<(), WorldError> {
        let source = candidate.certificate.authority_source.trim();
        if source.is_empty() {
            return self.reject_tick_consensus_submission(
                candidate,
                existing,
                "tick consensus authority_source cannot be empty".to_string(),
            );
        }
        if self.node_identity_public_key(source).is_none() {
            return self.reject_tick_consensus_submission(
                candidate,
                existing,
                format!("tick consensus source node is not trusted: {source}"),
            );
        }
        if !candidate.certificate.signatures.contains_key(source) {
            return self.reject_tick_consensus_submission(
                candidate,
                existing,
                format!("tick consensus signatures missing source signer: {source}"),
            );
        }
        if candidate.certificate.submission_role == TickConsensusSubmissionRole::Authority
            && source != self.tick_consensus_authority_source
        {
            return self.reject_tick_consensus_submission(
                candidate,
                existing,
                format!(
                    "authority submission source mismatch: expected={} found={source}",
                    self.tick_consensus_authority_source
                ),
            );
        }
        Ok(())
    }

    fn validate_tick_consensus_candidate_against_existing(
        &mut self,
        candidate: &TickConsensusRecord,
        existing: &TickConsensusRecord,
    ) -> Result<(), WorldError> {
        let existing_role = existing.certificate.submission_role;
        let candidate_role = candidate.certificate.submission_role;
        if existing_role == TickConsensusSubmissionRole::Authority {
            if candidate_role != TickConsensusSubmissionRole::Authority {
                return self.reject_tick_consensus_submission(
                    candidate,
                    Some(existing),
                    format!(
                        "non-authoritative submission rejected at tick {} because authoritative commitment already exists",
                        candidate.block.header.tick
                    ),
                );
            }
            if existing.certificate.authority_source != candidate.certificate.authority_source {
                return self.reject_tick_consensus_submission(
                    candidate,
                    Some(existing),
                    format!(
                        "conflicting authority sources at tick {}: existing={} attempted={}",
                        candidate.block.header.tick,
                        existing.certificate.authority_source,
                        candidate.certificate.authority_source
                    ),
                );
            }
            return Ok(());
        }

        if candidate_role == TickConsensusSubmissionRole::Propagation
            && existing.certificate.authority_source != candidate.certificate.authority_source
            && existing.certificate.block_hash != candidate.certificate.block_hash
        {
            return self.reject_tick_consensus_submission(
                candidate,
                Some(existing),
                format!(
                    "propagation conflict at tick {} requires authoritative adjudication",
                    candidate.block.header.tick
                ),
            );
        }
        Ok(())
    }

    fn reject_tick_consensus_submission(
        &mut self,
        candidate: &TickConsensusRecord,
        existing: Option<&TickConsensusRecord>,
        reason: String,
    ) -> Result<(), WorldError> {
        self.push_tick_consensus_rejection_audit_event(TickConsensusRejectionAuditEvent {
            recorded_at_tick: self.state.time,
            tick: candidate.block.header.tick,
            consensus_height: candidate.certificate.consensus_height,
            attempted_source: candidate.certificate.authority_source.clone(),
            attempted_role: candidate.certificate.submission_role,
            existing_source: existing.map(|record| record.certificate.authority_source.clone()),
            existing_role: existing.map(|record| record.certificate.submission_role),
            reason: reason.clone(),
        });
        Err(WorldError::DistributedValidationFailed { reason })
    }

    fn push_tick_consensus_rejection_audit_event(
        &mut self,
        event: TickConsensusRejectionAuditEvent,
    ) {
        if self.tick_consensus_rejection_audit_events.len()
            >= MAX_TICK_CONSENSUS_REJECTION_AUDIT_EVENTS
        {
            self.tick_consensus_rejection_audit_events.remove(0);
        }
        self.tick_consensus_rejection_audit_events.push(event);
    }

    fn validate_tick_consensus_source_node(
        &self,
        source_node_id: &str,
    ) -> Result<String, WorldError> {
        let source_node_id = source_node_id.trim();
        if source_node_id.is_empty() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "tick consensus source_node_id cannot be empty".to_string(),
            });
        }
        if self.node_identity_public_key(source_node_id).is_none() {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("tick consensus source node is not trusted: {source_node_id}"),
            });
        }
        Ok(source_node_id.to_string())
    }

    fn validate_tick_consensus_record_metadata(
        &self,
        record: &TickConsensusRecord,
    ) -> Result<(), WorldError> {
        let source = record.certificate.authority_source.trim();
        if source.is_empty() {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus authority_source is empty at tick {}",
                    record.block.header.tick
                ),
            });
        }
        if self.node_identity_public_key(source).is_none() {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus authority_source is untrusted at tick {}: {}",
                    record.block.header.tick, source
                ),
            });
        }
        if !record.certificate.signatures.contains_key(source) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus source signature missing at tick {} source={}",
                    record.block.header.tick, source
                ),
            });
        }
        Ok(())
    }

    fn verify_latest_tick_consensus_record(&self) -> Result<(), WorldError> {
        let Some(record) = self.tick_consensus_records.last() else {
            return Ok(());
        };
        self.validate_tick_consensus_record_metadata(record)?;
        let events_hash = self.hash_tick_events_from_ids(
            record.block.header.tick,
            record.block.ordered_event_ids.as_slice(),
        )?;
        if events_hash != record.block.header.events_hash {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "latest tick events hash mismatch tick={} expected={} found={}",
                    record.block.header.tick, events_hash, record.block.header.events_hash
                ),
            });
        }
        let block_hash = record.block.block_hash();
        if block_hash != record.certificate.block_hash {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "latest tick block hash mismatch tick={} expected={} found={}",
                    record.block.header.tick, block_hash, record.certificate.block_hash
                ),
            });
        }
        let current_state_root = self.current_state_root_hash()?;
        if record.block.header.tick == self.state.time
            && current_state_root != record.block.header.state_root
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "latest tick state root mismatch tick={} expected={} found={}",
                    record.block.header.tick, current_state_root, record.block.header.state_root
                ),
            });
        }
        Ok(())
    }

    fn hash_tick_events(&self, events: &[WorldEvent]) -> Result<String, WorldError> {
        self.hash_tick_event_refs(events.iter())
    }

    fn hash_tick_event_refs<'a>(
        &self,
        events: impl IntoIterator<Item = &'a WorldEvent>,
    ) -> Result<String, WorldError> {
        let inputs: Vec<TickEventHashInput<'_>> = events
            .into_iter()
            .map(|event| TickEventHashInput {
                id: event.id,
                caused_by: &event.caused_by,
                body: &event.body,
            })
            .collect();
        hash_json(&inputs)
    }

    fn hash_tick_domain_events(events: &[WorldEvent]) -> Result<String, WorldError> {
        let inputs: Vec<TickEventHashInput<'_>> = events
            .iter()
            .filter(|event| matches!(event.body, WorldEventBody::Domain(_)))
            .map(|event| TickEventHashInput {
                id: event.id,
                caused_by: &event.caused_by,
                body: &event.body,
            })
            .collect();
        hash_json(&inputs)
    }

    fn extract_ordered_action_ids(events: &[WorldEvent]) -> Vec<u64> {
        let mut seen = BTreeSet::new();
        let mut ordered = Vec::new();
        for event in events {
            let Some(CausedBy::Action(action_id)) = &event.caused_by else {
                continue;
            };
            if seen.insert(*action_id) {
                ordered.push(*action_id);
            }
        }
        ordered
    }

    pub(super) fn state_root_hash_with_body_overlay(
        &self,
        body_overlay: &BodyOverlay,
    ) -> Result<String, WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        let policy_hash = hash_json(&self.policies)?;
        let state_projection =
            WorldStateProjection::borrowed(&self.state).with_body_overlay(body_overlay.clone());
        let projection = StateRootProjection {
            state: &state_projection,
            manifest_hash: manifest_hash.as_str(),
            policy_hash: policy_hash.as_str(),
        };
        hash_json(&projection)
    }

    pub(super) fn state_root_hash_with_product_validation_delivery_cursor(
        &self,
        cursor: &ProductValidationDeliveryCursor,
    ) -> Result<String, WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        let policy_hash = hash_json(&self.policies)?;
        let state_projection = WorldStateProjection::borrowed(&self.state)
            .with_product_validation_delivery_cursor(cursor);
        hash_json(&StateRootProjection {
            state: &state_projection,
            manifest_hash: manifest_hash.as_str(),
            policy_hash: policy_hash.as_str(),
        })
    }

    pub(super) fn state_root_hash_with_command_overlay(
        &self,
        command_overlay: CommandStateOverlay<'_>,
    ) -> Result<String, WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        let policy_hash = hash_json(&self.policies)?;
        let state_projection =
            WorldStateProjection::borrowed(&self.state).with_command_overlay(command_overlay);
        let projection = StateRootProjection {
            state: &state_projection,
            manifest_hash: manifest_hash.as_str(),
            policy_hash: policy_hash.as_str(),
        };
        hash_json(&projection)
    }

    pub(super) fn state_root_hash_with_command_and_module_visual_overlay(
        &self,
        command_overlay: CommandStateOverlay<'_>,
        module_visual_entities: &BTreeMap<
            String,
            oasis7_agent_api::world_entity::ModuleVisualEntity,
        >,
    ) -> Result<String, WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        let policy_hash = hash_json(&self.policies)?;
        let state_projection = WorldStateProjection::borrowed(&self.state)
            .with_command_overlay(command_overlay)
            .with_module_visual_entities_overlay(module_visual_entities);
        let projection = StateRootProjection {
            state: &state_projection,
            manifest_hash: manifest_hash.as_str(),
            policy_hash: policy_hash.as_str(),
        };
        hash_json(&projection)
    }

    pub(super) fn state_root_hash_with_module_visual_overlay(
        &self,
        module_visual_entities: &BTreeMap<
            String,
            oasis7_agent_api::world_entity::ModuleVisualEntity,
        >,
    ) -> Result<String, WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        let policy_hash = hash_json(&self.policies)?;
        let state_projection = WorldStateProjection::borrowed(&self.state)
            .with_module_visual_entities_overlay(module_visual_entities);
        let projection = StateRootProjection {
            state: &state_projection,
            manifest_hash: manifest_hash.as_str(),
            policy_hash: policy_hash.as_str(),
        };
        hash_json(&projection)
    }

    pub(super) fn state_root_hash_with_module_instance_overlay(
        &self,
        prepared: &super::super::state::module_instance_transition::PreparedModuleInstance,
    ) -> Result<String, WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        self.state_root_hash_with_module_instance_and_manifest_hash(prepared, &manifest_hash)
    }

    pub(super) fn state_root_hash_with_module_instance_and_manifest_hash(
        &self,
        prepared: &super::super::state::module_instance_transition::PreparedModuleInstance,
        manifest_hash: &str,
    ) -> Result<String, WorldError> {
        let policy_hash = hash_json(&self.policies)?;
        let agents = prepared.routed_agents();
        let module_states = std::collections::BTreeMap::new();
        let state_projection = WorldStateProjection::borrowed(&self.state)
            .with_command_overlay(CommandStateOverlay {
                module_states: &module_states,
                resources: &prepared.resources,
                agents: &agents,
            })
            .with_module_instance_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &state_projection,
            manifest_hash,
            policy_hash: policy_hash.as_str(),
        })
    }

    pub(super) fn state_root_hash_with_module_release_overlay(
        &self,
        instance: Option<&super::super::state::module_instance_transition::PreparedModuleInstance>,
        release: &super::super::state::module_release_transition::PreparedModuleRelease,
        manifest_hash: &str,
    ) -> Result<String, WorldError> {
        let policy_hash = hash_json(&self.policies)?;
        let agents = release.routed_agents();
        let empty_resources = std::collections::BTreeMap::new();
        let module_states = std::collections::BTreeMap::new();
        let mut projection = WorldStateProjection::borrowed(&self.state)
            .with_command_overlay(CommandStateOverlay {
                module_states: &module_states,
                resources: instance
                    .map(|delta| &delta.resources)
                    .unwrap_or(&empty_resources),
                agents: &agents,
            })
            .with_module_release_overlay(release);
        if let Some(instance) = instance {
            projection = projection.with_module_instance_overlay(instance);
        }
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash,
            policy_hash: &policy_hash,
        })
    }

    pub(super) fn state_root_hash_with_agent_intent_overlay(
        &self,
        prepared: &super::agent_intent_publication::PreparedAgentIntent,
    ) -> Result<String, WorldError> {
        let policy_hash = hash_json(&self.policies)?;
        let agents = prepared.routed_agents();
        let empty_resources = std::collections::BTreeMap::new();
        let empty_module_states = std::collections::BTreeMap::new();
        let projection = WorldStateProjection::borrowed(&self.state)
            .with_command_overlay(CommandStateOverlay {
                module_states: &empty_module_states,
                resources: &empty_resources,
                agents: &agents,
            })
            .with_agent_intent_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &policy_hash,
        })
    }

    pub(super) fn state_root_hash_with_economy_data_overlay(
        &self,
        prepared: &super::economy_data_publication::PreparedEconomyDataEvent,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_economy_data_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_economic_contract_overlay(
        &self,
        prepared: &super::economic_contract_publication::PreparedEconomicContractEvent,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_economic_contract_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_alliance_war_overlay(
        &self,
        prepared: &super::alliance_war_publication::PreparedAllianceWarEvent,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_alliance_war_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_governance_meta_overlay(
        &self,
        prepared: &super::governance_meta_publication::PreparedGovernanceMetaEvent,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_governance_meta_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_power_redemption_overlay(
        &self,
        prepared: &super::power_redemption_publication::PreparedPowerRedemptionEvent,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_power_redemption_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_node_points_settlement_overlay(
        &self,
        prepared: &super::node_points_settlement_publication::PreparedNodePointsSettlement,
    ) -> Result<String, WorldError> {
        let projection = WorldStateProjection::borrowed(&self.state)
            .with_node_points_settlement_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_main_token_monetary_overlay(
        &self,
        prepared: &super::main_token_monetary_publication::PreparedMainTokenMonetaryEvent,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_main_token_monetary_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_main_token_governance_monetary_overlay(
        &self,
        prepared: &super::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent,
    ) -> Result<String, WorldError> {
        let projection = WorldStateProjection::borrowed(&self.state)
            .with_main_token_governance_monetary_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_main_token_restricted_claim_overlay(
        &self,
        prepared: &super::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent,
    ) -> Result<String, WorldError> {
        let projection = WorldStateProjection::borrowed(&self.state)
            .with_main_token_restricted_claim_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_starter_oc_claim_overlay(
        &self,
        prepared: &super::starter_oc_claim_publication::PreparedStarterOcClaimed,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_starter_oc_claim_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_agent_claim_light_lifecycle_overlay(
        &self,
        prepared: &super::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle,
    ) -> Result<String, WorldError> {
        let projection = WorldStateProjection::borrowed(&self.state)
            .with_agent_claim_light_lifecycle_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }
    pub(super) fn state_root_hash_with_agent_claim_economic_overlay(
        &self,
        prepared: &super::agent_claim_economic_publication::PreparedAgentClaimEconomic,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_agent_claim_economic_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_agent_claim_terminal_overlay(
        &self,
        prepared: &super::agent_claim_terminal_publication::PreparedAgentClaimTerminal,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_agent_claim_terminal_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_industry_overlay(
        &self,
        prepared: &super::super::state::industry_transition::PreparedIndustryEvent,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_industry_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_module_marketplace_overlay(
        &self,
        market: &super::super::state::module_marketplace_transition::PreparedModuleMarketplace,
    ) -> Result<String, WorldError> {
        let policy_hash = hash_json(&self.policies)?;
        let manifest_hash = self.current_manifest_hash()?;
        let agents = market.routed_agents();
        let module_states = std::collections::BTreeMap::new();
        let projection = WorldStateProjection::borrowed(&self.state)
            .with_command_overlay(CommandStateOverlay {
                module_states: &module_states,
                resources: &market.resources,
                agents: &agents,
            })
            .with_module_marketplace_overlay(market);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &manifest_hash,
            policy_hash: &policy_hash,
        })
    }

    fn derive_tick_randomness_seed(parent_hash: &str, tick: WorldTime) -> String {
        sha256_hex(format!("tick-rand:v1|{parent_hash}|{tick}").as_bytes())
    }
}
