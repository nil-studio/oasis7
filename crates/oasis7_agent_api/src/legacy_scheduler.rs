//! Pure legacy wake conditions and continuation admission messages.
//! Durable lifecycle state and authority remain in Runtime.

use serde::{Deserialize, Serialize};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use unicode_normalization::UnicodeNormalization;

const WAKE_SCHEMA: &str = "wake-condition.v1";

const WAKE_CONDITIONS_DIGEST_DOMAIN: &str = "oasis7.cognition.wake-conditions.v1";

const MAX_ID_BYTES: usize = 128;
const MAX_PATH_BYTES: usize = 128;
const MAX_ITEM_BYTES: usize = 768;
const MAX_LIST_BYTES: usize = 4096;
const MAX_CONDITIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeConditionError {
    code: &'static str,
}

impl WakeConditionError {
    pub fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub fn code(&self) -> &str {
        self.code
    }
}

impl fmt::Display for WakeConditionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}

impl std::error::Error for WakeConditionError {}

/// A deliberately flat one-of shape.  Keeping optional members explicit
/// makes forbidden-field checks fail closed instead of silently ignoring a
/// future condition kind's payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeConditionV1 {
    pub schema_version: String,
    pub kind: String,
    #[serde(default)]
    pub logical_tick: Option<u64>,
    #[serde(default)]
    pub event_digest: Option<String>,
    #[serde(default)]
    pub receipt_id: Option<String>,
    #[serde(default)]
    pub subject: Option<PreconditionSubjectV1>,
    #[serde(default)]
    pub path_or_rule: Option<String>,
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub expected_value_bytes: Option<Vec<u8>>,
}

impl WakeConditionV1 {
    fn canonical_bytes_unchecked(&self) -> Vec<u8> {
        serde_cbor::to_vec(self).unwrap_or_default()
    }
}

pub struct WakeConditionValidator;

impl WakeConditionValidator {
    pub fn validate(conditions: &[WakeConditionV1]) -> Result<(), WakeConditionError> {
        Self::validate_internal(conditions, true)
    }

    fn validate_internal(
        conditions: &[WakeConditionV1],
        require_canonical_order: bool,
    ) -> Result<(), WakeConditionError> {
        if conditions.is_empty() {
            return Err(WakeConditionError::new("wake_conditions_empty"));
        }
        if conditions.len() > MAX_CONDITIONS {
            return Err(WakeConditionError::new("wake_condition_invalid"));
        }
        let mut seen = BTreeSet::new();
        let mut total = 0usize;
        let mut previous: Option<Vec<u8>> = None;
        for condition in conditions {
            Self::validate_one(condition)?;
            let bytes = Self::canonical_bytes(condition);
            if bytes.len() > MAX_ITEM_BYTES || !seen.insert(bytes.clone()) {
                return Err(WakeConditionError::new("wake_condition_invalid"));
            }
            if require_canonical_order && previous.as_ref().is_some_and(|item| item > &bytes) {
                return Err(WakeConditionError::new("wake_condition_invalid"));
            }
            previous = Some(bytes.clone());
            total = total.saturating_add(bytes.len());
            if total > MAX_LIST_BYTES {
                return Err(WakeConditionError::new("wake_condition_invalid"));
            }
        }
        Ok(())
    }

    pub fn canonicalize(
        mut conditions: Vec<WakeConditionV1>,
    ) -> Result<Vec<WakeConditionV1>, WakeConditionError> {
        Self::validate_internal(&conditions, false)?;
        conditions.sort_by_key(Self::canonical_bytes);
        Ok(conditions)
    }

    pub fn canonical_bytes(condition: &WakeConditionV1) -> Vec<u8> {
        oasis7_wasm_abi::encode_canonical_cbor(condition)
            .unwrap_or_else(|_| condition.canonical_bytes_unchecked())
    }

    pub fn conditions_digest(conditions: &[WakeConditionV1]) -> String {
        // The list is explicitly set-like in the v1 wake contract.  Always
        // sort here as well as in `canonicalize`, so callers cannot
        // accidentally create a different identity by hashing a non-canonical
        // permutation.
        let mut canonical = conditions.to_vec();
        canonical.sort_by_key(Self::canonical_bytes);
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&canonical).unwrap_or_default();
        h_v1(WAKE_CONDITIONS_DIGEST_DOMAIN, &bytes)
    }

    pub fn evaluate(
        conditions: &[WakeConditionV1],
        context: &WakeEvaluationContext,
    ) -> Result<WakeEvaluation, WakeConditionError> {
        let canonical = Self::canonicalize(conditions.to_vec())?;
        let mut expired = false;
        let mut all_met = true;
        for condition in &canonical {
            let (met, reference_expired) = context.evaluate_one(condition);
            expired |= reference_expired;
            all_met &= met;
        }
        let conditions_digest = Self::conditions_digest(canonical.as_slice());
        let (status, reason) = if expired {
            ("expired", "wake_condition_expired")
        } else if all_met {
            ("ready", "condition_met")
        } else {
            ("pending", "condition_not_met")
        };
        let evaluation_digest = h_v1(
            "oasis7.cognition.wake-evaluation.v1",
            &(
                conditions_digest.as_str(),
                status,
                reason,
                context.logical_tick,
                context.reorg_epoch,
                context.evaluation_head_digest.as_deref(),
            ),
        );
        Ok(WakeEvaluation {
            status: status.to_string(),
            reason: reason.to_string(),
            evaluation_tick: context.logical_tick,
            conditions_digest,
            evaluation_digest,
        })
    }

    pub fn next_wake_tick(
        conditions: &[WakeConditionV1],
    ) -> Result<Option<u64>, WakeConditionError> {
        Self::next_wake_tick_at(conditions, 0)
    }

    /// Derive the schedule tick from the current committed Runtime tick.
    /// Event-, receipt- and state-head-driven conditions remain untimed; a
    /// tick condition cannot schedule work in the past.
    pub fn next_wake_tick_at(
        conditions: &[WakeConditionV1],
        current_tick: u64,
    ) -> Result<Option<u64>, WakeConditionError> {
        Self::validate(conditions)?;
        Ok(conditions
            .iter()
            .filter_map(|condition| condition.logical_tick)
            .map(|tick| tick.max(current_tick))
            .max())
    }

    fn validate_one(condition: &WakeConditionV1) -> Result<(), WakeConditionError> {
        if condition.schema_version != WAKE_SCHEMA {
            return Err(WakeConditionError::new("wake_condition_invalid"));
        }
        let present = [
            condition.logical_tick.is_some(),
            condition.event_digest.is_some(),
            condition.receipt_id.is_some(),
            condition.subject.is_some(),
            condition.path_or_rule.is_some(),
            condition.operator.is_some(),
            condition.expected_value_bytes.is_some(),
        ];
        let exact = |expected: [bool; 7]| present == expected;
        match condition.kind.as_str() {
            "at_or_after_tick" if exact([true, false, false, false, false, false, false]) => Ok(()),
            "world_event_committed" if exact([false, true, false, false, false, false, false]) => {
                Self::canonical_digest(condition.event_digest.as_deref())
            }
            "receipt_linked" if exact([false, false, true, false, false, false, false]) => {
                Self::canonical_digest(condition.receipt_id.as_deref())
            }
            "state_predicate" if exact([false, false, false, true, true, true, true]) => {
                let subject = condition.subject.as_ref().expect("presence checked");
                let path = condition.path_or_rule.as_deref().expect("presence checked");
                if !is_canonical_identifier(&subject.id, MAX_ID_BYTES) {
                    return Err(WakeConditionError::new("wake_condition_invalid"));
                }
                let (expected_subject, numeric) = match path {
                    "world.logical_tick" | "world.reorg_epoch" => ("world", true),
                    "world.state_root" | "world.runtime_manifest_hash" => ("world", false),
                    "agent.status"
                    | "agent.position"
                    | "agent.inventory_digest"
                    | "agent.capability_snapshot_hash" => ("agent", false),
                    path if is_supported_resource_path(path) => ("agent", true),
                    "intent.status" => ("intent", false),
                    _ => return Err(WakeConditionError::new("wake_condition_invalid")),
                };
                if subject.kind != expected_subject
                    || path.len() > MAX_PATH_BYTES
                    || !matches!(
                        condition.operator.as_deref(),
                        Some("eq" | "neq" | "lt" | "lte" | "gt" | "gte")
                    )
                    || (!numeric && !matches!(condition.operator.as_deref(), Some("eq" | "neq")))
                {
                    return Err(WakeConditionError::new("wake_condition_invalid"));
                }
                let expected = condition
                    .expected_value_bytes
                    .as_ref()
                    .expect("presence checked");
                let Ok(value) = serde_cbor::from_slice::<serde_cbor::Value>(expected) else {
                    return Err(WakeConditionError::new("wake_condition_invalid"));
                };
                let Ok(canonical) = oasis7_wasm_abi::encode_canonical_cbor(&value) else {
                    return Err(WakeConditionError::new("wake_condition_invalid"));
                };
                if expected.is_empty()
                    || expected.len() > 512
                    || canonical.as_slice() != expected.as_slice()
                    || !valid_predicate_value(path, &value)
                {
                    return Err(WakeConditionError::new("wake_condition_invalid"));
                }
                Ok(())
            }
            _ => Err(WakeConditionError::new("wake_condition_invalid")),
        }
    }

    fn canonical_digest(value: Option<&str>) -> Result<(), WakeConditionError> {
        let Some(value) = value else {
            return Err(WakeConditionError::new("wake_condition_invalid"));
        };
        if valid_blake3_digest(value) {
            Ok(())
        } else {
            Err(WakeConditionError::new("wake_condition_invalid"))
        }
    }
}

fn valid_blake3_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_predicate_value(path: &str, value: &serde_cbor::Value) -> bool {
    match path {
        "world.logical_tick" | "world.reorg_epoch" => matches!(
            value,
            serde_cbor::Value::Integer(value) if *value >= 0 && *value <= u64::MAX as i128
        ),
        path if is_supported_resource_path(path) => matches!(
            value,
            serde_cbor::Value::Integer(value)
                if *value >= i64::MIN as i128 && *value <= i64::MAX as i128
        ),
        "agent.position" => match value {
            serde_cbor::Value::Array(values) if values.len() == 2 => values.iter().all(|value| {
                matches!(
                    value,
                    serde_cbor::Value::Integer(value)
                        if *value >= i64::MIN as i128 && *value <= i64::MAX as i128
                )
            }),
            _ => false,
        },
        "world.state_root"
        | "world.runtime_manifest_hash"
        | "agent.inventory_digest"
        | "agent.capability_snapshot_hash" => {
            matches!(value, serde_cbor::Value::Text(value) if !value.is_empty() && value.len() <= MAX_ID_BYTES)
        }
        "agent.status" => matches!(
            value,
            serde_cbor::Value::Text(value)
                if matches!(value.as_str(), "idle" | "executing" | "blocked" | "waiting" | "unavailable")
        ),
        "intent.status" => matches!(
            value,
            serde_cbor::Value::Text(value)
                if matches!(value.as_str(), "proposed" | "submitted" | "accepted" | "blocked" | "completed" | "rejected" | "expired" | "cancelled" | "superseded")
        ),
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WakeEvaluation {
    pub status: String,
    pub reason: String,
    pub evaluation_tick: u64,
    pub conditions_digest: String,
    pub evaluation_digest: String,
}

#[derive(Debug, Clone, Default)]
pub struct WakeEvaluationContext {
    logical_tick: u64,
    reorg_epoch: u64,
    evaluation_head_digest: Option<String>,
    event_digests: BTreeSet<String>,
    receipt_ids: BTreeSet<String>,
    gc_references: BTreeSet<String>,
    predicate_values: BTreeMap<(String, String, String), Vec<u8>>,
}

impl WakeEvaluationContext {
    pub fn at(logical_tick: u64) -> Self {
        Self {
            logical_tick,
            ..Self::default()
        }
    }

    pub fn with_reorg_epoch(mut self, reorg_epoch: u64) -> Self {
        self.reorg_epoch = reorg_epoch;
        self
    }

    pub fn with_evaluation_head(mut self, digest: &str) -> Self {
        self.evaluation_head_digest = Some(digest.to_string());
        self
    }

    pub fn with_event(mut self, digest: &str) -> Self {
        self.event_digests.insert(digest.to_string());
        self
    }

    pub fn with_receipt(mut self, receipt_id: &str) -> Self {
        self.receipt_ids.insert(receipt_id.to_string());
        self
    }

    pub fn with_predicate_value(mut self, path: &str, value: &[u8]) -> Self {
        self.predicate_values.insert(
            (String::new(), String::new(), path.to_string()),
            value.to_vec(),
        );
        self
    }

    pub fn with_subject_predicate_value(
        mut self,
        subject: &PreconditionSubjectV1,
        path: &str,
        value: &[u8],
    ) -> Self {
        self.predicate_values.insert(
            (subject.kind.clone(), subject.id.clone(), path.to_string()),
            value.to_vec(),
        );
        self
    }

    pub fn with_predicate_u64(mut self, path: &str, value: u64) -> Self {
        self.predicate_values.insert(
            (String::new(), String::new(), path.to_string()),
            serde_cbor::to_vec(&value).expect("u64 must be CBOR encodable"),
        );
        self
    }

    pub fn with_predicate_i64(mut self, path: &str, value: i64) -> Self {
        self.predicate_values.insert(
            (String::new(), String::new(), path.to_string()),
            serde_cbor::to_vec(&value).expect("i64 must be CBOR encodable"),
        );
        self
    }

    pub fn with_predicate_text(mut self, path: &str, value: &str) -> Self {
        self.predicate_values.insert(
            (String::new(), String::new(), path.to_string()),
            serde_cbor::to_vec(&value).expect("text must be CBOR encodable"),
        );
        self
    }

    pub fn with_gc_references(mut self, references: &[&str]) -> Self {
        self.gc_references
            .extend(references.iter().map(|reference| (*reference).to_string()));
        self
    }

    fn evaluate_one(&self, condition: &WakeConditionV1) -> (bool, bool) {
        match condition.kind.as_str() {
            "at_or_after_tick" => (
                self.logical_tick >= condition.logical_tick.unwrap_or(u64::MAX),
                false,
            ),
            "world_event_committed" => {
                let digest = condition.event_digest.as_deref().unwrap_or_default();
                (
                    self.event_digests.contains(digest),
                    self.gc_references.contains(digest),
                )
            }
            "receipt_linked" => {
                let receipt = condition.receipt_id.as_deref().unwrap_or_default();
                (
                    self.receipt_ids.contains(receipt),
                    self.gc_references.contains(receipt),
                )
            }
            "state_predicate" => {
                let path = condition.path_or_rule.as_deref().unwrap_or_default();
                let empty_subject = PreconditionSubjectV1::default();
                let subject = condition.subject.as_ref().unwrap_or(&empty_subject);
                let Some(actual) = self
                    .predicate_values
                    .get(&(subject.kind.clone(), subject.id.clone(), path.to_string()))
                    .or_else(|| {
                        self.predicate_values
                            .get(&(String::new(), String::new(), path.to_string()))
                    })
                else {
                    return (false, false);
                };
                let expected = condition
                    .expected_value_bytes
                    .as_deref()
                    .unwrap_or_default();
                let Some(ordering) = canonical_value_ordering(actual, expected) else {
                    return (false, false);
                };
                let met = match condition.operator.as_deref() {
                    Some("eq") => ordering == std::cmp::Ordering::Equal,
                    Some("neq") => ordering != std::cmp::Ordering::Equal,
                    Some("lt") => ordering == std::cmp::Ordering::Less,
                    Some("lte") => ordering != std::cmp::Ordering::Greater,
                    Some("gt") => ordering == std::cmp::Ordering::Greater,
                    Some("gte") => ordering != std::cmp::Ordering::Less,
                    _ => false,
                };
                (met, false)
            }
            _ => (false, false),
        }
    }
}

fn canonical_value_ordering(left: &[u8], right: &[u8]) -> Option<std::cmp::Ordering> {
    let left = serde_cbor::from_slice::<serde_cbor::Value>(left).ok()?;
    let right = serde_cbor::from_slice::<serde_cbor::Value>(right).ok()?;
    Some(match (&left, &right) {
        (serde_cbor::Value::Integer(left), serde_cbor::Value::Integer(right)) => left.cmp(right),
        (serde_cbor::Value::Float(left), serde_cbor::Value::Float(right)) => {
            left.partial_cmp(right)?
        }
        _ => left.cmp(&right),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationBudgetV1 {
    pub unit: String,
    pub value: u64,
}

/// Runtime admission input for a continuation.  Deliberately excludes the
/// continuation identity, wake identity, wake sequence, status and status
/// digest: those fields are allocated and bound by `World`.  The paired
/// fields mirror the simulator proposal schema; branch/finality and the
/// derived wake tick are runtime bindings and are not part of the proposal
/// digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionContinuationProposalV1 {
    #[serde(default)]
    pub schema_version: u16,
    pub continuation_proposal_id: String,
    pub world_id: String,
    #[serde(default)]
    pub branch_id: String,
    #[serde(default)]
    pub finality_epoch: u64,
    #[serde(default)]
    pub finality_block_hash: Option<String>,
    #[serde(default)]
    pub finality_status: String,
    #[serde(default)]
    pub reorg_epoch: u64,
    #[serde(default)]
    pub runtime_manifest_hash: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub origin_turn_id: String,
    pub origin_request_digest: String,
    #[serde(default)]
    pub action_or_plan_kind: String,
    pub proposal_digest: String,
    #[serde(default)]
    pub action_or_envelope_digest: Option<String>,
    #[serde(default)]
    pub baseline_observation_digest: String,
    #[serde(default)]
    pub goal_digest: String,
    #[serde(default)]
    pub policy_digest: String,
    #[serde(default)]
    pub policy_revision: u64,
    #[serde(default)]
    pub precondition_summary: String,
    pub wake_conditions: Vec<WakeConditionV1>,
    #[serde(default)]
    pub next_wake_tick: Option<u64>,
    pub remaining_budget: ContinuationBudgetV1,
    #[serde(default)]
    pub valid_until_tick: Option<u64>,
    pub precondition_digest: String,
    #[serde(default)]
    pub source: String,
}

/// The context digests that an agent/provider must re-present when handing a
/// ready wake back to Runtime. Runtime does not invent a goal or policy; it
/// verifies this typed set against the originally admitted proposal digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionContextDigestsV1 {
    pub baseline_observation_digest: String,
    pub goal_digest: String,
    pub policy_digest: String,
    pub precondition_digest: String,
}

impl CognitionContextDigestsV1 {
    pub fn from_proposal(proposal: &CognitionContinuationProposalV1) -> Self {
        Self {
            baseline_observation_digest: proposal.baseline_observation_digest.clone(),
            goal_digest: proposal.goal_digest.clone(),
            policy_digest: proposal.policy_digest.clone(),
            precondition_digest: proposal.precondition_digest.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), WakeConditionError> {
        let bounded = |value: &str| is_canonical_identifier(value, MAX_ID_BYTES);
        if !bounded(&self.baseline_observation_digest)
            || !bounded(&self.goal_digest)
            || !bounded(&self.policy_digest)
            || !bounded(&self.precondition_digest)
        {
            return Err(WakeConditionError::new("cognition_context_invalid"));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct ContinuationProposalDigestInput<'a> {
    schema_version: u16,
    continuation_proposal_id: &'a str,
    world_id: &'a str,
    agent_id: &'a str,
    agent_session_id: &'a str,
    agent_turn_id: &'a str,
    decision_request_id: &'a str,
    origin_turn_id: &'a str,
    origin_request_digest: &'a str,
    action_or_plan_kind: &'a str,
    action_or_envelope_digest: Option<&'a str>,
    remaining_budget: &'a ContinuationBudgetV1,
    baseline_observation_digest: &'a str,
    goal_digest: &'a str,
    policy_digest: &'a str,
    policy_revision: u64,
    precondition_summary: &'a str,
    precondition_digest: &'a str,
    wake_conditions: Vec<WakeConditionV1>,
    valid_until_tick: Option<u64>,
    source: &'a str,
}

/// Runtime-facing identity for the next logical request emitted after a
/// selected continuation wake.  The resumed request deliberately has a new
/// session/turn/request and request digest (the wake's continuation digest is
/// part of the next Harness request context), while the continuation proposal
/// retains the original `origin_request_digest` as its causal lineage.
///
/// Runtime allocates no Agent identity here: these values are supplied by the
/// Harness, validated against the proposal and recorded before provider I/O.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionContinuationResumeRequestV1 {
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
    pub context_digest: String,
}

impl CognitionContinuationResumeRequestV1 {
    pub fn validate(&self) -> Result<(), WakeConditionError> {
        let bounded = |value: &str| is_canonical_identifier(value, MAX_ID_BYTES);
        if !bounded(&self.agent_session_id)
            || !bounded(&self.agent_turn_id)
            || !bounded(&self.decision_request_id)
            || !bounded(&self.request_digest)
            || !bounded(&self.context_digest)
            || !valid_blake3_digest(&self.request_digest)
            || !valid_blake3_digest(&self.context_digest)
        {
            return Err(WakeConditionError::new("cognition_resume_identity_invalid"));
        }
        Ok(())
    }
}

impl CognitionContinuationProposalV1 {
    /// Recompute the canonical paired-schema digest. Runtime-derived
    /// branch/finality and wake-tick fields are intentionally excluded.
    pub fn proposal_digest(&self) -> String {
        let payload = ContinuationProposalDigestInput {
            schema_version: self.schema_version,
            continuation_proposal_id: &self.continuation_proposal_id,
            world_id: &self.world_id,
            agent_id: &self.agent_id,
            agent_session_id: &self.agent_session_id,
            agent_turn_id: &self.agent_turn_id,
            decision_request_id: &self.decision_request_id,
            origin_turn_id: &self.origin_turn_id,
            origin_request_digest: &self.origin_request_digest,
            action_or_plan_kind: &self.action_or_plan_kind,
            action_or_envelope_digest: self.action_or_envelope_digest.as_deref(),
            remaining_budget: &self.remaining_budget,
            baseline_observation_digest: &self.baseline_observation_digest,
            goal_digest: &self.goal_digest,
            policy_digest: &self.policy_digest,
            policy_revision: self.policy_revision,
            precondition_summary: &self.precondition_summary,
            precondition_digest: &self.precondition_digest,
            wake_conditions: self.wake_conditions.clone(),
            valid_until_tick: self.valid_until_tick,
            source: &self.source,
        };
        h_v1("oasis7.cognition.continuation-proposal.v1", &payload)
    }

    pub fn validate(&self) -> Result<(), WakeConditionError> {
        let bounded = |value: &str| is_canonical_identifier(value, MAX_ID_BYTES);
        if self.schema_version != 1
            || !bounded(&self.continuation_proposal_id)
            || !bounded(&self.world_id)
            || !bounded(&self.agent_id)
            || !bounded(&self.agent_session_id)
            || !bounded(&self.agent_turn_id)
            || !bounded(&self.decision_request_id)
            || !bounded(&self.origin_turn_id)
            || !bounded(&self.origin_request_digest)
            || !bounded(&self.action_or_plan_kind)
            || !bounded(&self.baseline_observation_digest)
            || !bounded(&self.goal_digest)
            || !bounded(&self.policy_digest)
            || !bounded(&self.precondition_summary)
            || !bounded(&self.precondition_digest)
            || !bounded(&self.source)
            || !bounded(&self.proposal_digest)
            || self
                .action_or_envelope_digest
                .as_deref()
                .is_some_and(|value| !bounded(value))
            || self.remaining_budget.value == 0
            || !matches!(self.remaining_budget.unit.as_str(), "steps" | "ticks")
            || !crate::world_cognition::finality_binding_is_legal(
                &self.finality_status,
                self.finality_block_hash.as_deref(),
            )
            || WakeConditionValidator::validate(self.wake_conditions.as_slice()).is_err()
            || self.proposal_digest != self.proposal_digest()
        {
            return Err(WakeConditionError::new("continuation_proposal_invalid"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationStatusV1 {
    Scheduled,
    Pending,
    Waking,
    Consumed,
    Completed,
    Cancelled,
    Invalidated,
    Expired,
    Rejected,
}

/// A typed handoff is the only Runtime-owned completion path for a leased
/// wake. A replan carries the complete next proposal so the World can admit
/// it under the same transaction; no adapter may mark a wake consumed merely
/// by acknowledging a lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::large_enum_variant,
    reason = "Continuation proposals are persisted as this stable enum shape; boxing would change replay serialization and API."
)]
pub enum CognitionWakeDispositionV1 {
    Terminal {
        status: ContinuationStatusV1,
        reason: String,
    },
    Replan {
        proposal: CognitionContinuationProposalV1,
        budget_spent: u64,
    },
}

/// A subject selector used by a bounded runtime precondition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PreconditionSubjectV1 {
    pub kind: String,
    pub id: String,
}

fn is_canonical_identifier(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && !value.chars().any(char::is_control)
        && value.nfc().collect::<String>() == value
}
fn is_supported_resource_path(path: &str) -> bool {
    matches!(path, "agent.resource.electricity" | "agent.resource.data")
}
pub fn digest<T: Serialize>(domain: &str, payload: &T) -> String {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(domain, payload))
        .expect("cognition identity payload must be canonicalizable");
    format!("blake3:{}", blake3::hash(&bytes))
}
use digest as h_v1;
