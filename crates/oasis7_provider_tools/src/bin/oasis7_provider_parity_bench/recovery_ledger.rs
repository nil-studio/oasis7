use std::collections::{BTreeMap, BTreeSet};

use oasis7_agent_api::Digest32;
use serde::Serialize;

pub const RECOVERY_METRIC_SCHEMA_VERSION: &str = "recoverable_error_resolution_rate.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveryEvent {
    pub event_kind: String,
    pub event_seq: u64,
    pub error_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Sample identity is repeated on every event so an extracted resolved
    /// event cannot be detached from the benchmark sample that produced it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_id: Option<String>,
    pub agent_id: String,
    pub agent_session_id: String,
    pub recovery_chain_id: String,
    pub agent_turn_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_request_id: Option<String>,
    /// Canonical request digest captured on the originating error. Keeping
    /// this alongside the error makes a later resolution's origin digest
    /// verifiable from the persisted sample artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_digest: Option<String>,
    /// Logical retry sequence for the request represented by this event.
    /// Resolved events must carry the recovery request's sequence explicitly;
    /// it cannot be inferred from event order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_request_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveryMetricSummary {
    pub numerator: u64,
    pub denominator: u64,
    pub value: Option<f64>,
    pub zero_case: Option<String>,
    pub gate_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TraceValidity {
    Valid,
    InvalidFixture,
    Blocked,
}

impl TraceValidity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::InvalidFixture => "invalid_fixture",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryAssessment {
    pub trace_validity: TraceValidity,
    pub recovery_events: Vec<RecoveryEvent>,
    pub metric: RecoveryMetricSummary,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryLineage {
    pub agent_id: String,
    pub agent_session_id: String,
    pub recovery_chain_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
}

impl RecoveryLineage {
    fn is_complete(&self) -> bool {
        !self.agent_id.trim().is_empty()
            && !self.agent_session_id.trim().is_empty()
            && !self.recovery_chain_id.trim().is_empty()
            && !self.agent_turn_id.trim().is_empty()
            && !self.decision_request_id.trim().is_empty()
            && Digest32::from(self.request_digest.as_str()).is_canonical_blake3()
    }
}

#[derive(Debug, Clone)]
pub struct RecoveryErrorEvidence {
    pub error_code: String,
    pub lineage: RecoveryLineage,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RecoveryActionEvidence {
    pub lineage: RecoveryLineage,
    pub origin: RecoveryLineage,
    pub action_id: u64,
    pub authority_ref: String,
    pub retry_seq: u64,
}

#[derive(Debug, Clone)]
struct PendingError {
    error_id: String,
    lineage: RecoveryLineage,
}

/// Host-side recovery evidence for one benchmark sample.
///
/// The ledger is deliberately event based: a successful goal or a provider
/// transcript never resolves an error. Only a later successful action observed
/// by this host runner can append the Runtime/fixture-host-authorized
/// `recovery_resolved` event.
#[derive(Debug, Clone)]
pub struct RecoveryLedger {
    sample_id: String,
    next_event_seq: u64,
    next_error_index: u64,
    events: Vec<RecoveryEvent>,
    pending: Vec<PendingError>,
}

impl RecoveryLedger {
    pub fn new(sample_id: impl Into<String>) -> Self {
        Self {
            sample_id: sample_id.into(),
            next_event_seq: 1,
            next_error_index: 1,
            events: Vec::new(),
            pending: Vec::new(),
        }
    }

    pub fn record_recoverable_error(&mut self, evidence: RecoveryErrorEvidence) -> String {
        let error_code = evidence.error_code;
        let lineage = evidence.lineage;
        let error_id = format!("{}-error-{}", self.sample_id, self.next_error_index);
        self.next_error_index = self.next_error_index.saturating_add(1);
        let event_seq = self.take_event_seq();
        self.events.push(RecoveryEvent {
            event_kind: "recoverable_error".to_string(),
            event_seq,
            error_id: error_id.clone(),
            error_code: Some(error_code.clone()),
            sample_id: Some(self.sample_id.clone()),
            agent_id: lineage.agent_id.clone(),
            agent_session_id: lineage.agent_session_id.clone(),
            recovery_chain_id: lineage.recovery_chain_id.clone(),
            agent_turn_id: lineage.agent_turn_id.clone(),
            decision_request_id: Some(lineage.decision_request_id.clone()),
            request_digest: Some(lineage.request_digest.clone()),
            retry_seq: None,
            origin_turn_id: None,
            origin_request_digest: None,
            authority: None,
            runtime_outcome: None,
            authority_ref: None,
        });
        if lineage.is_complete() {
            self.pending.push(PendingError {
                error_id: error_id.clone(),
                lineage,
            });
        }
        self.events
            .last()
            .map(|event| event.error_id.clone())
            .expect("recoverable error event was appended")
    }

    #[allow(dead_code)]
    pub fn record_action_committed(&mut self, evidence: RecoveryActionEvidence) -> Option<String> {
        if !evidence.lineage.is_complete() || !evidence.origin.is_complete() {
            return None;
        }
        if evidence.retry_seq < 2 {
            return None;
        }
        if evidence.lineage.agent_turn_id == evidence.origin.agent_turn_id
            || evidence.lineage.decision_request_id == evidence.origin.decision_request_id
        {
            return None;
        }
        let matches = self
            .pending
            .iter()
            .enumerate()
            .filter(|(_, pending)| {
                pending.lineage == evidence.origin
                    && pending.lineage.agent_id == evidence.lineage.agent_id
                    && pending.lineage.agent_session_id == evidence.lineage.agent_session_id
                    && pending.lineage.recovery_chain_id == evidence.lineage.recovery_chain_id
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return None;
        }
        if !authority_ref_matches_action_id(&evidence.authority_ref, evidence.action_id) {
            return None;
        }
        let origin = self.pending.remove(matches[0]);
        let event_seq = self.take_event_seq();
        self.events.push(RecoveryEvent {
            event_kind: "recovery_resolved".to_string(),
            event_seq,
            error_id: origin.error_id.clone(),
            error_code: None,
            sample_id: Some(self.sample_id.clone()),
            agent_id: evidence.lineage.agent_id,
            agent_session_id: evidence.lineage.agent_session_id,
            recovery_chain_id: evidence.lineage.recovery_chain_id,
            agent_turn_id: evidence.lineage.agent_turn_id,
            decision_request_id: Some(evidence.lineage.decision_request_id),
            request_digest: Some(evidence.lineage.request_digest),
            retry_seq: Some(evidence.retry_seq),
            origin_turn_id: Some(evidence.origin.agent_turn_id),
            origin_request_digest: Some(evidence.origin.request_digest),
            authority: Some("runtime_or_fixture_host".to_string()),
            runtime_outcome: Some("action_committed".to_string()),
            authority_ref: Some(evidence.authority_ref),
        });
        Some(origin.error_id)
    }

    pub fn assess(&self) -> RecoveryAssessment {
        assess_recovery_events(self.events.clone())
    }

    fn take_event_seq(&mut self) -> u64 {
        let event_seq = self.next_event_seq;
        self.next_event_seq = self.next_event_seq.saturating_add(1);
        event_seq
    }
}

#[allow(dead_code)]
fn authority_ref_matches_action_id(authority_ref: &str, action_id: u64) -> bool {
    let action_id = action_id.to_string();
    [
        format!("://{action_id}"),
        format!("/{action_id}"),
        format!("-{action_id}"),
        format!("#{action_id}"),
    ]
    .iter()
    .any(|suffix| authority_ref.ends_with(suffix))
}

pub fn assess_recovery_events(events: Vec<RecoveryEvent>) -> RecoveryAssessment {
    let mut errors = Vec::new();
    let mut error_by_id = BTreeMap::new();
    let mut resolved_ids = BTreeSet::new();
    let mut previous_seq = 0_u64;
    let mut denominator = 0_u64;
    let mut sample_id: Option<&str> = None;

    for (index, event) in events.iter().enumerate() {
        if event.event_seq <= previous_seq {
            errors.push(format!(
                "event_seq is not strictly increasing at event index {}",
                index
            ));
        }
        previous_seq = event.event_seq;
        match event
            .sample_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Some(event_sample_id) => {
                if let Some(expected_sample_id) = sample_id {
                    if event_sample_id != expected_sample_id {
                        errors.push(format!(
                            "recovery event {} sample_id does not match sample {}",
                            event.error_id, expected_sample_id
                        ));
                    }
                } else {
                    sample_id = Some(event_sample_id);
                }
            }
            None => errors.push(format!(
                "{} {} sample_id is missing",
                event.event_kind, event.error_id
            )),
        }
        if event.event_kind == "recoverable_error" {
            denominator = denominator.saturating_add(1);
            if event.error_id.trim().is_empty()
                || event.error_code.as_deref().unwrap_or("").trim().is_empty()
                || event.agent_id.trim().is_empty()
                || event.agent_session_id.trim().is_empty()
                || event.recovery_chain_id.trim().is_empty()
                || event.agent_turn_id.trim().is_empty()
                || event
                    .decision_request_id
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
            {
                errors.push(format!(
                    "recoverable_error at event index {} is incomplete",
                    index
                ));
            }
            if error_by_id.insert(event.error_id.clone(), event).is_some() {
                errors.push(format!("duplicate recoverable error_id {}", event.error_id));
            }
            if event.origin_turn_id.is_some()
                || event.origin_request_digest.is_some()
                || event.authority.is_some()
                || event.runtime_outcome.is_some()
                || event.authority_ref.is_some()
            {
                errors.push(format!(
                    "recoverable_error {} contains resolution-only fields",
                    event.error_id
                ));
            }
            if !event
                .request_digest
                .as_deref()
                .is_some_and(|digest| Digest32::from(digest).is_canonical_blake3())
            {
                errors.push(format!(
                    "recoverable_error {} request_digest is not canonical",
                    event.error_id
                ));
            }
        } else if event.event_kind == "recovery_resolved" {
            if event.error_id.trim().is_empty()
                || event.agent_id.trim().is_empty()
                || event.agent_session_id.trim().is_empty()
                || event.recovery_chain_id.trim().is_empty()
                || event.agent_turn_id.trim().is_empty()
                || event
                    .origin_turn_id
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                || event
                    .origin_request_digest
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                || event.authority.as_deref() != Some("runtime_or_fixture_host")
                || !matches!(
                    event.runtime_outcome.as_deref(),
                    Some("action_committed") | Some("next_turn_admitted")
                )
                || event
                    .authority_ref
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
            {
                errors.push(format!(
                    "recovery_resolved {} is incomplete or unauthorized",
                    event.error_id
                ));
            }
            if event.error_code.is_some() {
                errors.push(format!(
                    "recovery_resolved {} contains invalid origin-only fields",
                    event.error_id
                ));
            }
            if event
                .decision_request_id
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
                || event
                    .request_digest
                    .as_deref()
                    .is_none_or(|digest| !Digest32::from(digest).is_canonical_blake3())
                || event.retry_seq.is_none()
            {
                errors.push(format!(
                    "recovery_resolved {} is missing complete request identity",
                    event.error_id
                ));
            }
            if event.retry_seq.is_some_and(|retry_seq| retry_seq < 2) {
                errors.push(format!(
                    "recovery_resolved {} retry_seq must be at least 2",
                    event.error_id
                ));
            }
            if !event
                .origin_request_digest
                .as_deref()
                .is_some_and(|digest| Digest32::from(digest).is_canonical_blake3())
            {
                errors.push(format!(
                    "recovery_resolved {} origin_request_digest is not canonical",
                    event.error_id
                ));
            }
            if !resolved_ids.insert(event.error_id.clone()) {
                errors.push(format!("duplicate recovery_resolved {}", event.error_id));
            }
            let Some(error) = error_by_id.get(&event.error_id) else {
                errors.push(format!(
                    "recovery_resolved {} has no recoverable_error",
                    event.error_id
                ));
                continue;
            };
            if event.origin_request_digest.as_deref() != error.request_digest.as_deref() {
                errors.push(format!(
                    "recovery_resolved {} origin_request_digest does not match originating request_digest",
                    event.error_id
                ));
            }
            if event.event_seq <= error.event_seq
                || event.agent_id != error.agent_id
                || event.agent_session_id != error.agent_session_id
                || event.recovery_chain_id != error.recovery_chain_id
                || event.origin_turn_id.as_deref() != Some(error.agent_turn_id.as_str())
                || event.sample_id != error.sample_id
                || event.decision_request_id == error.decision_request_id
                || event.request_digest == error.request_digest
            {
                errors.push(format!(
                    "recovery_resolved {} does not match its ordered error chain",
                    event.error_id
                ));
            }
        } else {
            errors.push(format!("unknown recovery event kind {}", event.event_kind));
        }
    }

    let numerator = resolved_ids
        .iter()
        .filter(|error_id| error_by_id.contains_key(*error_id))
        .count() as u64;
    let trace_validity = if errors.is_empty() {
        TraceValidity::Valid
    } else {
        TraceValidity::Blocked
    };
    let metric = if errors.is_empty() {
        metric_summary(numerator, denominator)
    } else {
        RecoveryMetricSummary {
            numerator: 0,
            denominator,
            value: None,
            zero_case: None,
            gate_status: "blocked".to_string(),
        }
    };
    RecoveryAssessment {
        trace_validity,
        recovery_events: events,
        metric,
        errors,
    }
}

pub fn metric_summary(numerator: u64, denominator: u64) -> RecoveryMetricSummary {
    if denominator == 0 {
        RecoveryMetricSummary {
            numerator: 0,
            denominator: 0,
            value: None,
            zero_case: Some("not_applicable".to_string()),
            gate_status: "not_evaluable".to_string(),
        }
    } else {
        RecoveryMetricSummary {
            numerator,
            denominator,
            value: Some(numerator as f64 / denominator as f64),
            zero_case: None,
            gate_status: "evaluable".to_string(),
        }
    }
}

/// Apply the scenario completion rules after recovery evidence is assessed.
pub fn scenario_goal_completed(
    scenario_id: &str,
    action_kind_counts: &BTreeMap<String, u64>,
    error_counts: &BTreeMap<String, u64>,
    invalid_action_count: u64,
    recovery_metric: &RecoveryMetricSummary,
) -> bool {
    match scenario_id {
        "P0-001" => action_kind_counts.get("move_agent").copied().unwrap_or(0) >= 3,
        "P0-002" => {
            action_kind_counts
                .get("inspect_target")
                .copied()
                .unwrap_or(0)
                >= 1
        }
        "P0-003" => {
            action_kind_counts
                .get("speak_to_nearby")
                .copied()
                .unwrap_or(0)
                >= 2
        }
        "P0-004" => {
            action_kind_counts
                .get("simple_interact")
                .copied()
                .unwrap_or(0)
                >= 1
                && invalid_action_count == 0
        }
        "P0-005" => {
            !error_counts.is_empty()
                && invalid_action_count == 0
                && recovery_metric.denominator > 0
                && recovery_metric.gate_status == "evaluable"
                && recovery_metric.numerator == recovery_metric.denominator
        }
        _ => action_kind_counts.values().copied().sum::<u64>() > 0,
    }
}
