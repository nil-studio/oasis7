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
        complete_with_work(self, &NoRetentionWorkV1).unwrap()
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
        let decision = prepare_retention_insert_with_work_v1(record, &NoRetentionWorkV1).unwrap();
        if let Some(artifact) = decision.response_artifact {
            self.response_artifacts.insert(artifact);
        }
        self.records.insert(decision.key, decision.record);
    }

    /// Enroll a World-owned commit marker in the same retention index used by
    /// explicit terminal records. This keeps canonical commit artifacts and
    /// their tombstones on one GC horizon instead of allowing the JSON
    /// projection to grow independently of `retention_state`.
    pub fn insert_commit_record(&mut self, marker: &WorldCommitRecordV1) {
        let record = retention_record_from_commit_with_work_v1(marker, &NoRetentionWorkV1).unwrap();
        self.insert(record);
    }

    pub fn pin_reference(&mut self, key: &str, reference: &str) {
        let pin = prepare_retention_pin_with_work_v1(key, reference, &NoRetentionWorkV1).unwrap();
        self.pins.entry(pin.key).or_default().insert(pin.reference);
    }

    pub(crate) fn clear_references_with_prefix(&mut self, prefix: &str) {
        let changes = prepare_retention_clear_prefix_with_work_v1(self, prefix).unwrap();
        for change in changes {
            if change.remove_set {
                self.pins.remove(&change.key);
            } else if let Some(refs) = self.pins.get_mut(&change.key) {
                for reference in change.references {
                    refs.remove(&reference);
                }
            }
        }
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
        let decision = prepare_retention_gc_with_work_v1(self, now_tick, gc_floor_tick).unwrap();
        self.gc_floor_tick = decision.floor;
        for key in &decision.report.deleted_keys {
            self.records.remove(key);
            self.pins.remove(key);
        }
        self.response_artifacts = decision.artifacts;
        Ok(decision.report)
    }

    pub fn classify_replay(&self, request: RetentionReplayRequestV1) -> Result<(), RetentionError> {
        classify_retention_replay_with_work_v1(&request, &NoRetentionWorkV1).unwrap()
    }

    pub(crate) fn replay(
        &self,
        key: &str,
        digest: &str,
        probe: &mut RetentionExecutionProbe,
    ) -> Result<RetentionReplayResult, RetentionError> {
        replay_retention_key_with_work_v1(self, key, digest, probe).unwrap()
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
        replay_retention_v1_with_work(self, &request, probe).unwrap()
    }
}

/// Work-only companion to the original retention rules. A callback can charge
/// processing/layout; it cannot override a status, lineage or GC decision.
pub trait RetentionWorkV1 {
    type Error;
    fn work(&self, steps: u64, bytes: usize) -> Result<(), Self::Error>;
    fn owned(&self, bytes: usize) -> Result<(), Self::Error>;
}
pub trait RetentionFactsV1: RetentionWorkV1 {
    fn horizon(&self) -> Result<u64, Self::Error>;
    fn floor(&self) -> Result<u64, Self::Error>;
    fn with_record<T>(
        &self,
        key: &str,
        visit: impl FnOnce(Option<&RetentionRecordV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
    fn visit_records(
        &self,
        visit: impl FnMut(&str, &RetentionRecordV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn with_pins<T>(
        &self,
        key: &str,
        visit: impl FnOnce(Option<&BTreeSet<String>>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
    fn visit_pins(
        &self,
        visit: impl FnMut(&str, &BTreeSet<String>) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn visit_response_artifacts(
        &self,
        visit: impl FnMut(&str) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn response_artifact_present(&self, key: &str) -> Result<bool, Self::Error>;
}
pub(crate) struct NoRetentionWorkV1;
impl RetentionWorkV1 for NoRetentionWorkV1 {
    type Error = std::convert::Infallible;
    fn work(&self, _: u64, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn owned(&self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
}
pub fn retention_text_with_work_v1<S: RetentionWorkV1>(
    s: &S,
    value: &str,
) -> Result<String, S::Error> {
    s.owned(std::mem::size_of::<String>() + value.len())?;
    let mut bytes = Vec::with_capacity(value.len());
    assert_eq!(
        bytes.capacity(),
        value.len(),
        "retention text exact capacity"
    );
    for part in value.as_bytes().chunks(8192) {
        s.work(1, part.len())?;
        bytes.extend_from_slice(part)
    }
    Ok(String::from_utf8(bytes).expect("original retention UTF-8"))
}
fn optional_text<S: RetentionWorkV1>(
    s: &S,
    value: Option<&str>,
) -> Result<Option<String>, S::Error> {
    value.map(|v| retention_text_with_work_v1(s, v)).transpose()
}
fn compare<S: RetentionWorkV1>(s: &S, a: &str, b: &str) -> Result<bool, S::Error> {
    s.work(1, a.len() + b.len())?;
    Ok(a == b)
}
fn tree_admit<S: RetentionWorkV1, T>(
    s: &S,
    count: usize,
    max_key: usize,
    key: &str,
) -> Result<(), S::Error> {
    let levels = (count + 1).ilog2() as usize + 2;
    s.work(
        (levels * 11) as u64,
        (max_key + key.len()).saturating_mul(levels * 11),
    )?;
    s.owned(
        (11 * std::mem::size_of::<T>() + 13 * std::mem::size_of::<usize>() + 128)
            .saturating_mul(levels),
    )
}
fn push<S: RetentionWorkV1, T>(s: &S, values: &mut Vec<T>, value: T) -> Result<(), S::Error> {
    s.work(1, 0)?;
    if values.len() == values.capacity() {
        let capacity = values.capacity().saturating_mul(2).max(4);
        s.owned(capacity.saturating_mul(std::mem::size_of::<T>()))?;
        values.reserve_exact(capacity - values.len());
        assert_eq!(
            values.capacity(),
            capacity,
            "retention vector exact capacity"
        )
    }
    values.push(value);
    Ok(())
}
pub struct RetentionInsertDecisionV1 {
    pub key: String,
    pub record: RetentionRecordV1,
    pub response_artifact: Option<String>,
}
pub fn prepare_retention_insert_with_work_v1<S: RetentionWorkV1>(
    record: RetentionRecordV1,
    s: &S,
) -> Result<RetentionInsertDecisionV1, S::Error> {
    s.owned(std::mem::size_of::<RetentionInsertDecisionV1>())?;
    s.work(1, 0)?;
    let response_artifact = if record.response_artifact_id.is_empty() {
        None
    } else {
        Some(retention_text_with_work_v1(
            s,
            &record.response_artifact_id,
        )?)
    };
    let key = retention_text_with_work_v1(s, &record.envelope_idempotency_key)?;
    Ok(RetentionInsertDecisionV1 {
        key,
        record,
        response_artifact,
    })
}
pub fn retention_record_from_commit_with_work_v1<S: RetentionWorkV1>(
    marker: &WorldCommitRecordV1,
    s: &S,
) -> Result<RetentionRecordV1, S::Error> {
    s.owned(std::mem::size_of::<RetentionRecordV1>())?;
    Ok(RetentionRecordV1 {
        schema_version: retention_text_with_work_v1(s, "cognition-retention-record.v1")?,
        world_id: retention_text_with_work_v1(s, &marker.world_id)?,
        envelope_idempotency_key: retention_text_with_work_v1(s, &marker.envelope_idempotency_key)?,
        envelope_digest: retention_text_with_work_v1(s, &marker.envelope_digest)?,
        agent_session_id: if marker.agent_session_id.is_empty() {
            None
        } else {
            Some(retention_text_with_work_v1(s, &marker.agent_session_id)?)
        },
        agent_turn_id: if marker.agent_turn_id.is_empty() {
            None
        } else {
            Some(retention_text_with_work_v1(s, &marker.agent_turn_id)?)
        },
        decision_request_id: if marker.decision_request_id.is_empty() {
            None
        } else {
            Some(retention_text_with_work_v1(s, &marker.decision_request_id)?)
        },
        status: retention_text_with_work_v1(s, &marker.status)?,
        base_tick: marker.parent_tick,
        issued_at_tick: marker.parent_tick,
        terminal_disposition: optional_text(s, marker.abort_reason.as_deref())?,
        receipt_id: if compare(s, &marker.status, "committed")? {
            Some(retention_text_with_work_v1(s, &marker.receipt_id)?)
        } else {
            None
        },
        receipt_digest: if compare(s, &marker.status, "committed")? {
            Some(retention_text_with_work_v1(s, &marker.receipt_digest)?)
        } else {
            None
        },
        response_artifact_id: retention_text_with_work_v1(s, &marker.response_artifact_digest)?,
        continuation_id: retention_text_with_work_v1(s, "")?,
        commit_record_id: Some(retention_text_with_work_v1(s, &marker.commit_id)?),
    })
}
pub struct RetentionPinDecisionV1 {
    pub key: String,
    pub reference: String,
}
pub fn prepare_retention_pin_with_work_v1<S: RetentionWorkV1>(
    key: &str,
    reference: &str,
    s: &S,
) -> Result<RetentionPinDecisionV1, S::Error> {
    s.owned(std::mem::size_of::<RetentionPinDecisionV1>())?;
    Ok(RetentionPinDecisionV1 {
        key: retention_text_with_work_v1(s, key)?,
        reference: retention_text_with_work_v1(s, reference)?,
    })
}
pub struct RetentionClearPinDecisionV1 {
    pub key: String,
    pub references: Vec<String>,
    pub remove_set: bool,
}
pub fn prepare_retention_clear_prefix_with_work_v1<S: RetentionFactsV1>(
    s: &S,
    prefix: &str,
) -> Result<Vec<RetentionClearPinDecisionV1>, S::Error> {
    s.owned(std::mem::size_of::<Vec<RetentionClearPinDecisionV1>>())?;
    let mut changes = Vec::new();
    s.visit_pins(|key, references| {
        s.work(1, 0)?;
        let mut removed = Vec::new();
        s.owned(std::mem::size_of::<Vec<String>>())?;
        for reference in references {
            s.work(1, reference.len().min(prefix.len()))?;
            if reference.starts_with(prefix) {
                let reference = retention_text_with_work_v1(s, reference)?;
                push(s, &mut removed, reference)?
            }
        }
        if !removed.is_empty() || references.is_empty() {
            let change = RetentionClearPinDecisionV1 {
                key: retention_text_with_work_v1(s, key)?,
                remove_set: removed.len() == references.len(),
                references: removed,
            };
            push(s, &mut changes, change)?
        }
        Ok(())
    })?;
    Ok(changes)
}
pub struct RetentionGcDecisionV1 {
    pub floor: u64,
    pub report: RetentionGcReport,
    pub artifacts: BTreeSet<String>,
}
pub fn prepare_retention_gc_with_work_v1<S: RetentionFactsV1>(
    s: &S,
    now_tick: u64,
    gc_floor_tick: u64,
) -> Result<RetentionGcDecisionV1, S::Error> {
    s.owned(std::mem::size_of::<RetentionGcDecisionV1>())?;
    let floor = s.floor()?.max(gc_floor_tick);
    let horizon = s.horizon()?;
    let mut pinned_reference_count = 0usize;
    s.visit_pins(|_, refs| {
        s.work(1, 0)?;
        pinned_reference_count += refs.len();
        Ok(())
    })?;
    let mut deleted_keys = Vec::new();
    let mut retained_terminal_count = 0usize;
    let mut artifacts = BTreeSet::new();
    let mut max_artifact = 0;
    s.visit_records(|key, record| {
        s.work(1, record.status.len().saturating_mul(5))?;
        let terminal = matches!(
            record.status.as_str(),
            "committed" | "rejected" | "failed" | "cancelled" | "aborted"
        );
        let pinned = s.with_pins(key, |refs| Ok(refs.is_some_and(|refs| !refs.is_empty())))?;
        let within_horizon = now_tick.saturating_sub(record.issued_at_tick) <= horizon;
        let below_floor = record.base_tick < gc_floor_tick && record.issued_at_tick < gc_floor_tick;
        let retained = if terminal && (within_horizon || pinned || !below_floor) {
            retained_terminal_count = retained_terminal_count.saturating_add(1);
            true
        } else {
            pinned || within_horizon || !below_floor
        };
        if retained {
            if !record.response_artifact_id.is_empty() {
                tree_admit::<_, String>(
                    s,
                    artifacts.len(),
                    max_artifact,
                    &record.response_artifact_id,
                )?;
                max_artifact = max_artifact.max(record.response_artifact_id.len());
                artifacts.insert(retention_text_with_work_v1(
                    s,
                    &record.response_artifact_id,
                )?);
            }
        } else {
            let key = retention_text_with_work_v1(s, key)?;
            push(s, &mut deleted_keys, key)?
        }
        Ok(())
    })?;
    let deleted_count = deleted_keys.len();
    Ok(RetentionGcDecisionV1 {
        floor,
        report: RetentionGcReport {
            deleted_count,
            deleted_keys,
            retained_terminal_count,
            pinned_reference_count,
        },
        artifacts,
    })
}
impl RetentionWorkV1 for CognitionRetentionStore {
    type Error = std::convert::Infallible;
    fn work(&self, _: u64, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn owned(&self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
}
impl RetentionFactsV1 for CognitionRetentionStore {
    fn horizon(&self) -> Result<u64, Self::Error> {
        Ok(self.horizon)
    }
    fn floor(&self) -> Result<u64, Self::Error> {
        Ok(self.gc_floor_tick)
    }
    fn with_record<T>(
        &self,
        key: &str,
        visit: impl FnOnce(Option<&RetentionRecordV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        visit(self.records.get(key))
    }
    fn visit_records(
        &self,
        mut visit: impl FnMut(&str, &RetentionRecordV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        for (key, record) in &self.records {
            visit(key, record)?
        }
        Ok(())
    }
    fn with_pins<T>(
        &self,
        key: &str,
        visit: impl FnOnce(Option<&BTreeSet<String>>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        visit(self.pins.get(key))
    }
    fn visit_pins(
        &self,
        mut visit: impl FnMut(&str, &BTreeSet<String>) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        for (key, refs) in &self.pins {
            visit(key, refs)?
        }
        Ok(())
    }
    fn visit_response_artifacts(
        &self,
        mut visit: impl FnMut(&str) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        for id in &self.response_artifacts {
            visit(id)?
        }
        Ok(())
    }
    fn response_artifact_present(&self, key: &str) -> Result<bool, Self::Error> {
        Ok(self.response_artifacts.contains(key))
    }
}

fn canonical_with_work<S: RetentionWorkV1>(
    s: &S,
    value: Option<&str>,
    max: usize,
) -> Result<bool, S::Error> {
    let Some(value) = value else { return Ok(false) };
    for chunk in value.as_bytes().chunks(8192) {
        s.work(1, chunk.len())?
    }
    s.owned(max.saturating_mul(8).saturating_add(4096))?;
    Ok(is_canonical_identifier(value, max))
}
fn complete_with_work<S: RetentionWorkV1>(
    request: &RetentionReplayRequestV1,
    s: &S,
) -> Result<bool, S::Error> {
    if !compare(
        s,
        request.schema_version.as_deref().unwrap_or(""),
        "agent-decision-envelope.v1",
    )? {
        return Ok(false);
    }
    for value in [
        &request.world_id,
        &request.agent_session_id,
        &request.agent_turn_id,
        &request.decision_request_id,
    ] {
        if !canonical_with_work(s, value.as_deref(), 128)? {
            return Ok(false);
        }
    }
    for value in [&request.envelope_idempotency_key, &request.envelope_digest] {
        if !canonical_with_work(s, value.as_deref(), 256)? {
            return Ok(false);
        }
    }
    s.work(3, 0)?;
    Ok(request.base_tick.is_some()
        && request.issued_at_tick.is_some()
        && request.gc_floor_tick.is_some())
}
pub fn classify_retention_replay_with_work_v1<S: RetentionWorkV1>(
    request: &RetentionReplayRequestV1,
    s: &S,
) -> Result<Result<(), RetentionError>, S::Error> {
    if !complete_with_work(request, s)? {
        return Ok(Err(RetentionError::new("legacy_no_cognition_proof")));
    }
    s.work(2, 0)?;
    if request
        .base_tick
        .zip(request.gc_floor_tick)
        .is_some_and(|(base, floor)| base < floor)
        || request
            .issued_at_tick
            .zip(request.gc_floor_tick)
            .is_some_and(|(issued, floor)| issued < floor)
    {
        return Ok(Err(RetentionError::new("expired_idempotency")));
    }
    Ok(Ok(()))
}
fn replay_record_with_work<S: RetentionWorkV1>(
    record: Option<&RetentionRecordV1>,
    digest: &str,
    probe: &RetentionExecutionProbe,
    s: &S,
) -> Result<Result<RetentionReplayResult, RetentionError>, S::Error> {
    let Some(record) = record else {
        return Ok(Err(RetentionError::new("expired_idempotency")));
    };
    if !compare(s, &record.envelope_digest, digest)? {
        return Ok(Err(RetentionError::new("idempotency_conflict")));
    }
    if !compare(s, &record.status, "committed")? {
        return Ok(Err(RetentionError::new("replay_not_committed")));
    }
    s.owned(std::mem::size_of::<RetentionReplayResult>())?;
    Ok(Ok(RetentionReplayResult {
        receipt_id: optional_text(s, record.receipt_id.as_deref())?,
        provider_invocation_count: probe.provider_invocation_count,
        effect_delta: 0,
        world_receipt_linked_delta: 0,
    }))
}
pub fn replay_retention_key_with_work_v1<S: RetentionFactsV1>(
    s: &S,
    key: &str,
    digest: &str,
    probe: &RetentionExecutionProbe,
) -> Result<Result<RetentionReplayResult, RetentionError>, S::Error> {
    s.with_record(key, |record| {
        replay_record_with_work(record, digest, probe, s)
    })
}
pub fn replay_retention_v1_with_work<S: RetentionFactsV1>(
    s: &S,
    request: &RetentionReplayRequestV1,
    probe: &RetentionExecutionProbe,
) -> Result<Result<RetentionReplayResult, RetentionError>, S::Error> {
    if let Err(error) = classify_retention_replay_with_work_v1(request, s)? {
        return Ok(Err(error));
    }
    s.work(1, 0)?;
    if request.gc_floor_tick != Some(s.floor()?) {
        return Ok(Err(RetentionError::new("expired_idempotency")));
    }
    let Some(key) = request.envelope_idempotency_key.as_deref() else {
        return Ok(Err(RetentionError::new("legacy_no_cognition_proof")));
    };
    let Some(digest) = request.envelope_digest.as_deref() else {
        return Ok(Err(RetentionError::new("legacy_no_cognition_proof")));
    };
    s.with_record(key, |record| {
        let Some(record) = record else {
            return Ok(Err(RetentionError::new("expired_idempotency")));
        };
        s.work(3, 0)?;
        if record.agent_session_id.is_none()
            || record.agent_turn_id.is_none()
            || record.decision_request_id.is_none()
        {
            return Ok(Err(RetentionError::new("legacy_no_cognition_proof")));
        }
        let mut matches = true;
        for (a, b) in [
            (request.world_id.as_deref(), Some(record.world_id.as_str())),
            (
                request.agent_session_id.as_deref(),
                record.agent_session_id.as_deref(),
            ),
            (
                request.agent_turn_id.as_deref(),
                record.agent_turn_id.as_deref(),
            ),
            (
                request.decision_request_id.as_deref(),
                record.decision_request_id.as_deref(),
            ),
        ] {
            s.work(1, 0)?;
            matches = match (a, b) {
                (Some(a), Some(b)) => compare(s, a, b)?,
                (None, None) => true,
                _ => false,
            };
            if !matches {
                break;
            }
        }
        if !matches
            || !compare(s, digest, &record.envelope_digest)?
            || request.base_tick != Some(record.base_tick)
            || request.issued_at_tick != Some(record.issued_at_tick)
        {
            return Ok(Err(RetentionError::new("idempotency_conflict")));
        }
        replay_retention_key_with_work_v1(s, key, digest, probe)
    })
}

#[cfg(test)]
mod shared_retention_rule_tests {
    use super::*;
    fn record(key: &str, artifact: &str, tick: u64) -> RetentionRecordV1 {
        RetentionRecordV1 {
            schema_version: "cognition-retention-record.v1".into(),
            world_id: "world".into(),
            envelope_idempotency_key: key.into(),
            envelope_digest: "digest".into(),
            agent_session_id: Some("session".into()),
            agent_turn_id: Some("turn".into()),
            decision_request_id: Some("request".into()),
            status: "committed".into(),
            base_tick: tick,
            issued_at_tick: tick,
            terminal_disposition: None,
            receipt_id: Some("receipt".into()),
            receipt_digest: Some("receipt-digest".into()),
            response_artifact_id: artifact.into(),
            continuation_id: String::new(),
            commit_record_id: None,
        }
    }
    #[test]
    fn gc_uses_supplied_floor_and_rebuilds_artifacts_after_overwrite() {
        let mut store = CognitionRetentionStore::with_horizon(0);
        store.insert(record("key", "old", 5));
        store.insert(record("key", "new", 5));
        assert!(store.contains_response_artifact("old"));
        store.gc_floor_tick = 100;
        let report = store.gc(20, 3).unwrap();
        assert_eq!(store.gc_floor_tick(), 100);
        assert_eq!(report.retained_terminal_count, 1);
        assert!(store.contains_key("key"));
        assert!(!store.contains_response_artifact("old"));
        assert!(store.contains_response_artifact("new"));
        let report = store.gc(20, 6).unwrap();
        assert_eq!(report.deleted_keys, vec!["key"]);
        assert!(!store.contains_response_artifact("new"));
    }
    #[test]
    fn clear_prefix_preserves_unrelated_pins_and_gc_counts_orphans() {
        let mut store = CognitionRetentionStore::with_horizon(0);
        store.insert(record("key", "artifact", 1));
        store.pin_reference("key", "continuation-event:a");
        store.pin_reference("key", "receipt:a");
        store.pin_reference("orphan", "receipt:orphan");
        store.pins.insert("empty".into(), BTreeSet::new());
        store.clear_references_with_prefix("continuation-event:");
        assert!(!store.pins.contains_key("empty"));
        assert_eq!(store.pins["key"], BTreeSet::from(["receipt:a".to_string()]));
        let report = store.gc(20, 10).unwrap();
        assert_eq!(report.pinned_reference_count, 2);
        assert_eq!(report.retained_terminal_count, 1);
        assert!(report.deleted_keys.is_empty());
    }
    #[test]
    fn replay_refusal_order_and_execution_probe_are_preserved() {
        let mut store = CognitionRetentionStore::with_horizon(0);
        let mut item = record("key", "artifact", 1);
        item.status = "aborted".into();
        store.insert(item);
        let mut probe = RetentionExecutionProbe {
            provider_invocation_count: 7,
            effect_count: 9,
            receipt_count: 11,
            world_receipt_linked_count: 13,
        };
        let original = probe.clone();
        assert_eq!(
            store.replay("key", "wrong", &mut probe).unwrap_err().code(),
            "idempotency_conflict"
        );
        assert_eq!(
            store
                .replay("key", "digest", &mut probe)
                .unwrap_err()
                .code(),
            "replay_not_committed"
        );
        assert_eq!(probe, original);
    }
}
