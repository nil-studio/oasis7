//! Retention, replay and GC pins for cognition terminal records.
//!
//! A terminal key is retained as a tombstone for the configured safety
//! horizon.  GC never turns an old record into permission to execute it again.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::cognition::is_canonical_identifier;
use super::cognition_recovery::WorldCommitRecordV1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionError {
    code: &'static str,
}

impl RetentionError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub fn code(&self) -> &str {
        self.code
    }
}

impl fmt::Display for RetentionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}

impl std::error::Error for RetentionError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionRecordV1 {
    pub schema_version: String,
    pub world_id: String,
    pub envelope_idempotency_key: String,
    pub envelope_digest: String,
    /// Exact cognition lineage for the terminal envelope. Optional during
    /// deserialization so legacy snapshots remain recoverable, but replay
    /// refuses any record that cannot prove all three identities.
    #[serde(default)]
    pub agent_session_id: Option<String>,
    #[serde(default)]
    pub agent_turn_id: Option<String>,
    #[serde(default)]
    pub decision_request_id: Option<String>,
    pub status: String,
    pub base_tick: u64,
    pub issued_at_tick: u64,
    #[serde(default)]
    pub terminal_disposition: Option<String>,
    #[serde(default)]
    pub receipt_id: Option<String>,
    #[serde(default)]
    pub receipt_digest: Option<String>,
    pub response_artifact_id: String,
    pub continuation_id: String,
    #[serde(default)]
    pub commit_record_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetentionGcReport {
    pub deleted_count: usize,
    /// Envelope keys whose terminal tombstones became eligible for deletion.
    /// World uses these exact keys to compact the corresponding canonical
    /// commit/response/receipt/journal projections in the same transaction.
    #[serde(default)]
    pub deleted_keys: Vec<String>,
    pub retained_terminal_count: usize,
    pub pinned_reference_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RetentionExecutionProbe {
    pub provider_invocation_count: u64,
    pub effect_count: u64,
    pub receipt_count: u64,
    pub world_receipt_linked_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetentionReplayResult {
    pub receipt_id: Option<String>,
    pub provider_invocation_count: u64,
    pub effect_delta: i64,
    pub world_receipt_linked_delta: u64,
}

/// A bounded replay request.  `schema_version=None` is intentionally retained
/// for compatibility classification and is never an executable success path.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RetentionReplayRequestV1 {
    #[serde(default)]
    pub schema_version: Option<String>,
    #[serde(default)]
    pub world_id: Option<String>,
    #[serde(default)]
    pub agent_session_id: Option<String>,
    #[serde(default)]
    pub agent_turn_id: Option<String>,
    #[serde(default)]
    pub decision_request_id: Option<String>,
    #[serde(default)]
    pub envelope_idempotency_key: Option<String>,
    #[serde(default)]
    pub envelope_digest: Option<String>,
    #[serde(default)]
    pub base_tick: Option<u64>,
    #[serde(default)]
    pub issued_at_tick: Option<u64>,
    #[serde(default)]
    pub gc_floor_tick: Option<u64>,
}

impl RetentionReplayRequestV1 {
    pub fn from_json(value: Value) -> Result<Self, RetentionError> {
        serde_json::from_value(value).map_err(|_| RetentionError::new("invalid_replay_request"))
    }

    fn complete_v1(&self) -> bool {
        self.schema_version.as_deref() == Some("agent-decision-envelope.v1")
            && self
                .world_id
                .as_deref()
                .is_some_and(|value| is_canonical_identifier(value, 128))
            && self
                .agent_session_id
                .as_deref()
                .is_some_and(|value| is_canonical_identifier(value, 128))
            && self
                .agent_turn_id
                .as_deref()
                .is_some_and(|value| is_canonical_identifier(value, 128))
            && self
                .decision_request_id
                .as_deref()
                .is_some_and(|value| is_canonical_identifier(value, 128))
            && self
                .envelope_idempotency_key
                .as_deref()
                .is_some_and(|value| is_canonical_identifier(value, 256))
            && self
                .envelope_digest
                .as_deref()
                .is_some_and(|value| is_canonical_identifier(value, 256))
            && self.base_tick.is_some()
            && self.issued_at_tick.is_some()
            && self.gc_floor_tick.is_some()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CognitionRetentionStore {
    horizon: u64,
    /// Highest GC floor applied to this durable store. Replay requests must
    /// bind to this floor rather than supplying a weaker caller-local value.
    #[serde(default)]
    gc_floor_tick: u64,
    records: BTreeMap<String, RetentionRecordV1>,
    pins: BTreeMap<String, BTreeSet<String>>,
    response_artifacts: BTreeSet<String>,
}

impl CognitionRetentionStore {
    pub fn with_horizon(horizon: u64) -> Self {
        Self {
            horizon,
            ..Self::default()
        }
    }

    pub fn insert(&mut self, record: RetentionRecordV1) {
        if !record.response_artifact_id.is_empty() {
            self.response_artifacts
                .insert(record.response_artifact_id.clone());
        }
        self.records
            .insert(record.envelope_idempotency_key.clone(), record);
    }

    /// Enroll a World-owned commit marker in the same retention index used by
    /// explicit terminal records. This keeps canonical commit artifacts and
    /// their tombstones on one GC horizon instead of allowing the JSON
    /// projection to grow independently of `retention_state`.
    pub fn insert_commit_record(&mut self, marker: &WorldCommitRecordV1) {
        self.insert(RetentionRecordV1 {
            schema_version: "cognition-retention-record.v1".to_string(),
            world_id: marker.world_id.clone(),
            envelope_idempotency_key: marker.envelope_idempotency_key.clone(),
            envelope_digest: marker.envelope_digest.clone(),
            agent_session_id: (!marker.agent_session_id.is_empty())
                .then(|| marker.agent_session_id.clone()),
            agent_turn_id: (!marker.agent_turn_id.is_empty()).then(|| marker.agent_turn_id.clone()),
            decision_request_id: (!marker.decision_request_id.is_empty())
                .then(|| marker.decision_request_id.clone()),
            status: marker.status.clone(),
            base_tick: marker.parent_tick,
            issued_at_tick: marker.parent_tick,
            terminal_disposition: marker.abort_reason.clone(),
            receipt_id: (marker.status == "committed").then(|| marker.receipt_id.clone()),
            receipt_digest: (marker.status == "committed").then(|| marker.receipt_digest.clone()),
            response_artifact_id: marker.response_artifact_digest.clone(),
            continuation_id: String::new(),
            commit_record_id: Some(marker.commit_id.clone()),
        });
    }

    pub fn pin_reference(&mut self, key: &str, reference: &str) {
        self.pins
            .entry(key.to_string())
            .or_default()
            .insert(reference.to_string());
    }

    pub(crate) fn clear_references_with_prefix(&mut self, prefix: &str) {
        self.pins.retain(|_, references| {
            references.retain(|reference| !reference.starts_with(prefix));
            !references.is_empty()
        });
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.records.contains_key(key)
    }

    pub fn contains_response_artifact(&self, artifact_id: &str) -> bool {
        self.response_artifacts.contains(artifact_id)
    }

    pub fn gc_floor_tick(&self) -> u64 {
        self.gc_floor_tick
    }

    pub fn gc(
        &mut self,
        now_tick: u64,
        gc_floor_tick: u64,
    ) -> Result<RetentionGcReport, RetentionError> {
        self.gc_floor_tick = self.gc_floor_tick.max(gc_floor_tick);
        let mut deleted_count = 0usize;
        let mut deleted_keys = Vec::new();
        let mut retained_terminal_count = 0usize;
        let pinned_reference_count = self.pins.values().map(BTreeSet::len).sum();
        let keys: Vec<String> = self.records.keys().cloned().collect();
        for key in keys {
            let Some(record) = self.records.get(&key) else {
                continue;
            };
            let terminal = matches!(
                record.status.as_str(),
                "committed" | "rejected" | "failed" | "cancelled" | "aborted"
            );
            let pinned = self.pins.get(&key).is_some_and(|refs| !refs.is_empty());
            let within_horizon = now_tick.saturating_sub(record.issued_at_tick) <= self.horizon;
            let below_floor =
                record.base_tick < gc_floor_tick && record.issued_at_tick < gc_floor_tick;
            if terminal && (within_horizon || pinned || !below_floor) {
                retained_terminal_count = retained_terminal_count.saturating_add(1);
            } else if pinned || within_horizon || !below_floor {
                // Active records are also retained while they can still be
                // addressed by a checkpoint or a late response.
            } else {
                self.records.remove(&key);
                self.pins.remove(&key);
                deleted_count = deleted_count.saturating_add(1);
                deleted_keys.push(key);
            }
        }
        self.rebuild_artifact_index();
        Ok(RetentionGcReport {
            deleted_count,
            deleted_keys,
            retained_terminal_count,
            pinned_reference_count,
        })
    }

    pub fn classify_replay(&self, request: RetentionReplayRequestV1) -> Result<(), RetentionError> {
        if !request.complete_v1() {
            return Err(RetentionError::new("legacy_no_cognition_proof"));
        }
        if request
            .base_tick
            .zip(request.gc_floor_tick)
            .is_some_and(|(base, floor)| base < floor)
            || request
                .issued_at_tick
                .zip(request.gc_floor_tick)
                .is_some_and(|(issued, floor)| issued < floor)
        {
            return Err(RetentionError::new("expired_idempotency"));
        }
        Ok(())
    }

    pub(crate) fn replay(
        &self,
        key: &str,
        digest: &str,
        probe: &mut RetentionExecutionProbe,
    ) -> Result<RetentionReplayResult, RetentionError> {
        let Some(record) = self.records.get(key) else {
            return Err(RetentionError::new("expired_idempotency"));
        };
        if record.envelope_digest != digest {
            return Err(RetentionError::new("idempotency_conflict"));
        }
        if record.status != "committed" {
            return Err(RetentionError::new("replay_not_committed"));
        }
        // Reading a canonical receipt is the only operation on replay.  Keep
        // probe counters untouched: no provider, effect, debit or link runs.
        Ok(RetentionReplayResult {
            receipt_id: record.receipt_id.clone(),
            provider_invocation_count: probe.provider_invocation_count,
            effect_delta: 0,
            world_receipt_linked_delta: 0,
        })
    }

    /// Replay a committed terminal record only after proving the complete v1
    /// request and binding it to the persisted record and current GC floor.
    /// The legacy key/digest seam remains available internally for focused
    /// store fixtures, but World-facing callers must use this method.
    pub fn replay_v1(
        &self,
        request: RetentionReplayRequestV1,
        probe: &mut RetentionExecutionProbe,
    ) -> Result<RetentionReplayResult, RetentionError> {
        let request_floor = request.gc_floor_tick;
        self.classify_replay(request.clone())?;
        if request_floor != Some(self.gc_floor_tick) {
            return Err(RetentionError::new("expired_idempotency"));
        }
        let key = request
            .envelope_idempotency_key
            .as_deref()
            .ok_or_else(|| RetentionError::new("legacy_no_cognition_proof"))?;
        let digest = request
            .envelope_digest
            .as_deref()
            .ok_or_else(|| RetentionError::new("legacy_no_cognition_proof"))?;
        let record = self
            .records
            .get(key)
            .ok_or_else(|| RetentionError::new("expired_idempotency"))?;
        if record.agent_session_id.is_none()
            || record.agent_turn_id.is_none()
            || record.decision_request_id.is_none()
        {
            return Err(RetentionError::new("legacy_no_cognition_proof"));
        }
        if request.world_id.as_deref() != Some(record.world_id.as_str())
            || request.agent_session_id.as_deref() != record.agent_session_id.as_deref()
            || request.agent_turn_id.as_deref() != record.agent_turn_id.as_deref()
            || request.decision_request_id.as_deref() != record.decision_request_id.as_deref()
            || digest != record.envelope_digest
            || request.base_tick != Some(record.base_tick)
            || request.issued_at_tick != Some(record.issued_at_tick)
        {
            return Err(RetentionError::new("idempotency_conflict"));
        }
        self.replay(key, digest, probe)
    }

    fn rebuild_artifact_index(&mut self) {
        self.response_artifacts = self
            .records
            .values()
            .filter(|record| !record.response_artifact_id.is_empty())
            .map(|record| record.response_artifact_id.clone())
            .collect();
    }
}
