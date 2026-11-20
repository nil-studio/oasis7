//! Versioned cognition wire types and the small amount of host-side state
//! required by the continuous-agent provider contract.
//!
//! The existing [`DecisionRequest`] and [`DecisionResponse`] types remain the
//! provider's inner payload.  This module deliberately adds an outer context
//! instead of teaching the legacy DTOs about cognition identity.  Runtime
//! persistence, action receipts, and continuation scheduling consume these
//! values but remain runtime-owned.

use std::collections::{BTreeMap, VecDeque};

use oasis7_wasm_abi::CapabilitySubject;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use oasis7_agent_api::{
    CognitionError, ContinuousAgentTurnContextV1, Digest32, ResponseArtifactIdentityV1,
    RuntimeBindingV1, h_v1,
};

use super::{DecisionRequest, DecisionResponse, FeedbackEnvelope};

#[cfg(test)]
#[path = "continuous_agent_feedback_recovery.rs"]
mod continuous_agent_feedback_recovery;

pub const CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR: &str = "oasis7.continuous-agent-context";
pub const CONTINUOUS_AGENT_CONTEXT_VERSION: u16 = 1;
pub const COGNITION_REQUEST_DIGEST_DOMAIN: &str = "oasis7.cognition.request.v1";
pub const COGNITION_PROVIDER_INVOCATION_DOMAIN: &str = "oasis7.cognition.provider-invocation.v1";
pub const COGNITION_RESPONSE_ARTIFACT_IDENTITY_DOMAIN: &str =
    "oasis7.cognition.response-artifact-identity.v1";
pub const COGNITION_CAPABILITY_CATALOG_DOMAIN: &str = "oasis7.cognition.capability-catalog.v1";
pub const COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN: &str =
    "oasis7.cognition.capability-invocation-context.v1";
const MAX_FEEDBACK_REPLAY_ENTRIES: usize = 8;

pub use oasis7_agent_harness::FinalityBindingV1;

pub use super::continuous_agent_budget::BudgetContractV1;

/// The additive outer request wrapper.  `transport_attempt` is intentionally
/// retained for observability but excluded from identity bytes.  The legacy
/// inner timeout is likewise a transport budget, not a provider invocation
/// identity; the normalized `budget_contract` remains the policy input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuousAgentRequestContextV1 {
    pub base_decision_request: DecisionRequest,
    pub context_discriminator: String,
    pub context_version: u16,
    pub protocol_version: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub agent_subject: String,
    pub runtime_binding: RuntimeBindingV1,
    pub observation_digest: Digest32,
    pub capability_catalog_digest: Digest32,
    pub capability_invocation_context_digest: Digest32,
    pub memory_snapshot_digest: Digest32,
    pub goal_snapshot_digest: Digest32,
    pub continuation_digest: Digest32,
    pub adapter_protocol_version: String,
    pub budget_contract: BudgetContractV1,
    pub request_digest: Digest32,
}

impl ContinuousAgentRequestContextV1 {
    pub fn validate(&self) -> Result<(), CognitionError> {
        self.validate_structure()?;
        self.base_decision_request
            .validate_contract()
            .map_err(|error| CognitionError::new(error.code, error.message))?;
        let derived_digest = self.request_digest();
        if self.request_digest != derived_digest {
            return Err(CognitionError::new(
                "request_digest_mismatch",
                "declared request digest does not match canonical request inputs",
            ));
        }
        Ok(())
    }

    /// Validate the production-target provider lane. Legacy fixtures may use
    /// zero transport fields, but an actual async provider invocation must
    /// carry both retry and transport attempt lineage explicitly.
    pub fn validate_production_lane(&self) -> Result<(), CognitionError> {
        self.validate()?;
        if self.retry_seq == 0 || self.transport_attempt == 0 {
            return Err(CognitionError::new(
                "missing_retry_lineage",
                "production provider requests require nonzero retry_seq and transport_attempt",
            ));
        }
        self.validate_production_capability_context()?;
        Ok(())
    }

    /// The production target cannot discover capabilities from a null or
    /// provider-supplied placeholder. Runtime must bind both snapshots to the
    /// requesting agent and the same world/finality projection carried by the
    /// outer context. The legacy `validate` path remains available for the
    /// fenced compatibility entrypoint only.
    fn validate_production_capability_context(&self) -> Result<(), CognitionError> {
        let catalog = self
            .base_decision_request
            .capability_catalog
            .as_ref()
            .ok_or_else(|| {
                CognitionError::new(
                    "missing_capability_context",
                    "production provider requests require a Runtime capability catalog",
                )
            })?;
        let invocation = self
            .base_decision_request
            .capability_invocation_context
            .as_ref()
            .ok_or_else(|| {
                CognitionError::new(
                    "missing_capability_context",
                    "production provider requests require a Runtime capability invocation context",
                )
            })?;

        catalog.validate().map_err(|error| {
            CognitionError::new(
                "invalid_capability_context",
                format!("Runtime capability catalog is invalid: {error}"),
            )
        })?;
        if self.capability_catalog_digest != h_v1(COGNITION_CAPABILITY_CATALOG_DOMAIN, catalog) {
            return Err(CognitionError::new(
                "capability_context_digest_mismatch",
                "capability catalog digest does not match the Runtime-bound snapshot",
            ));
        }
        if self.capability_invocation_context_digest
            != h_v1(COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN, invocation)
        {
            return Err(CognitionError::new(
                "capability_context_digest_mismatch",
                "capability invocation context digest does not match the Runtime-bound context",
            ));
        }
        if catalog.snapshot_id != invocation.catalog_snapshot_id
            || catalog.subject != invocation.subject
            || catalog.presenter != invocation.presenter
        {
            return Err(CognitionError::new(
                "capability_context_mismatch",
                "capability catalog and invocation context are not bound to the same snapshot",
            ));
        }
        if catalog.world_id != self.runtime_binding.world_id
            || catalog.branch_id != self.runtime_binding.branch_id
            || catalog.finality_epoch != self.runtime_binding.finality_epoch
            || catalog.logical_tick != self.runtime_binding.base_tick
            || catalog.audience.world_id != self.runtime_binding.world_id
            || catalog.audience.branch_id != self.runtime_binding.branch_id
            || catalog.audience.finality_epoch != self.runtime_binding.finality_epoch
        {
            return Err(CognitionError::new(
                "capability_runtime_binding_mismatch",
                "capability catalog is not bound to the request Runtime world/finality projection",
            ));
        }
        match &invocation.subject {
            CapabilitySubject::Agent { agent_id, .. } if agent_id == &self.agent_subject => {}
            _ => {
                return Err(CognitionError::new(
                    "capability_subject_mismatch",
                    "capability invocation subject must be the requesting agent",
                ));
            }
        }
        if invocation.grant_id.trim().is_empty()
            || invocation.catalog_snapshot_id.trim().is_empty()
            || invocation.response_nonce.trim().is_empty()
        {
            return Err(CognitionError::new(
                "invalid_capability_context",
                "capability invocation grant, snapshot, and response nonce are required",
            ));
        }
        Ok(())
    }

    pub fn validate_value(value: &Value) -> Result<(), CognitionError> {
        validate_outer_fields(
            value,
            &[
                "base_decision_request",
                "context_discriminator",
                "context_version",
                "protocol_version",
                "agent_session_id",
                "agent_turn_id",
                "decision_request_id",
                "retry_seq",
                "transport_attempt",
                "agent_subject",
                "runtime_binding",
                "observation_digest",
                "capability_catalog_digest",
                "capability_invocation_context_digest",
                "memory_snapshot_digest",
                "goal_snapshot_digest",
                "continuation_digest",
                "adapter_protocol_version",
                "budget_contract",
                "request_digest",
            ],
        )?;
        let context: Self = serde_json::from_value(value.clone())
            .map_err(|error| CognitionError::new("unknown_context_field", error.to_string()))?;
        context.validate_structure()
    }

    fn validate_structure(&self) -> Result<(), CognitionError> {
        if self.context_discriminator != CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR {
            return Err(CognitionError::new(
                "unsupported_context_discriminator",
                "continuous-agent request discriminator is not recognized",
            ));
        }
        if self.context_version != CONTINUOUS_AGENT_CONTEXT_VERSION {
            return Err(CognitionError::new(
                "unsupported_context_version",
                format!(
                    "unsupported continuous-agent context version {}",
                    self.context_version
                ),
            ));
        }
        if self.agent_session_id.trim().is_empty()
            || self.agent_turn_id.trim().is_empty()
            || self.decision_request_id.trim().is_empty()
            || self.agent_subject.trim().is_empty()
        {
            return Err(CognitionError::new(
                "missing_cognition_identity",
                "session, turn, request, and subject identities are required",
            ));
        }
        if self.base_decision_request.observation.agent_id != self.agent_subject {
            return Err(CognitionError::new(
                "cognition_context_identity_mismatch",
                "outer request subject does not match the inner observation subject",
            ));
        }
        for (name, digest) in [
            ("observation_digest", &self.observation_digest),
            ("capability_catalog_digest", &self.capability_catalog_digest),
            (
                "capability_invocation_context_digest",
                &self.capability_invocation_context_digest,
            ),
            ("memory_snapshot_digest", &self.memory_snapshot_digest),
            ("goal_snapshot_digest", &self.goal_snapshot_digest),
            ("continuation_digest", &self.continuation_digest),
        ] {
            if !digest.is_canonical_blake3() {
                return Err(CognitionError::new(
                    "invalid_context_digest",
                    format!("{name} must be a canonical BLAKE3-256 digest"),
                ));
            }
        }
        self.runtime_binding.validate()?;
        Ok(())
    }

    /// Return canonical request payload bytes, excluding output identity,
    /// transport attempt, and both legacy timeout-only transport fields.
    pub fn canonical_request_bytes(&self) -> Result<Vec<u8>, CognitionError> {
        self.validate_structure()?;
        super::agent_api_compat::request_context_to_api(self).canonical_request_bytes()
    }

    pub fn request_digest(&self) -> Digest32 {
        h_v1(
            COGNITION_REQUEST_DIGEST_DOMAIN,
            &self
                .canonical_request_bytes()
                .expect("valid cognition request must have canonical bytes"),
        )
    }

    pub fn provider_invocation_key(&self) -> Digest32 {
        h_v1(COGNITION_PROVIDER_INVOCATION_DOMAIN, &self.request_digest())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuousAgentResponseContextV1 {
    pub base_decision_response: DecisionResponse,
    pub context_discriminator: String,
    pub context_version: u16,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub request_digest: Digest32,
    /// Content identity for the provider response/artifact retained at the
    /// replay seam. Runtime may bind this digest to its durable envelope.
    #[serde(default)]
    pub response_digest: Digest32,
}

impl ContinuousAgentResponseContextV1 {
    pub fn validate_value(value: &Value) -> Result<(), CognitionError> {
        validate_outer_fields(
            value,
            &[
                "base_decision_response",
                "context_discriminator",
                "context_version",
                "agent_session_id",
                "agent_turn_id",
                "decision_request_id",
                "retry_seq",
                "transport_attempt",
                "request_digest",
                "response_digest",
            ],
        )?;
        serde_json::from_value::<Self>(value.clone())
            .map_err(|error| CognitionError::new("unknown_context_field", error.to_string()))?;
        Ok(())
    }

    pub fn response_artifact_identity(&self) -> ResponseArtifactIdentityV1 {
        super::agent_api_compat::response_context_to_api(self).response_artifact_identity()
    }

    pub fn response_artifact_identity_payload(&self) -> Result<Value, CognitionError> {
        super::agent_api_compat::response_context_to_api(self).response_artifact_identity_payload()
    }
}

/// Oasis7-owned policy wrapper retained to preserve the existing public
/// `From<NormalizedMemoryWriteIntentV1>` conversion. The wire fields match
/// the portable API DTO; conversion is explicit at the Harness boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteIntentV1 {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackEnvelopeV1 {
    pub feedback_id: String,
    pub feedback_seq: u64,
    pub agent_subject: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_action_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_receipt_id: Option<String>,
    pub status: String,
    pub request_digest: Digest32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
    pub provenance: String,
}

impl FeedbackEnvelopeV1 {
    /// Validate the target feedback object's outer keys. Runtime projection
    /// fields are intentionally rejected: this bridge has no verifier that
    /// can turn a projection into Runtime authority.
    pub fn validate_value(value: &Value) -> Result<(), CognitionError> {
        validate_outer_fields(
            value,
            &[
                "feedback_id",
                "feedback_seq",
                "agent_subject",
                "agent_session_id",
                "agent_turn_id",
                "decision_request_id",
                "candidate_action_id",
                "runtime_receipt_id",
                "status",
                "request_digest",
                "reject_reason",
                "provenance",
            ],
        )?;
        Ok(())
    }

    /// Legacy feedback has no runtime disposition/receipt and therefore cannot
    /// be promoted to authoritative cognition feedback.
    pub fn from_legacy_value(value: Value) -> Result<Self, CognitionError> {
        let legacy: FeedbackEnvelope = serde_json::from_value(value)
            .map_err(|error| CognitionError::new("legacy_feedback_invalid", error.to_string()))?;
        let _ = legacy;
        Err(CognitionError::new(
            "legacy_feedback_ambiguous",
            "legacy feedback lacks a Runtime disposition or committed receipt",
        ))
    }
}

#[derive(Debug, Clone)]
struct ActiveCognitionRequest {
    session_id: String,
    turn_id: String,
    request_id: String,
    request_digest: Digest32,
}

/// In-memory host-side correlation and single-flight guard for P0.1.
/// Runtime owns durable recovery and receipt truth; this store only prevents
/// concurrent cognition contamination before a request reaches the provider.
#[derive(Debug, Clone, Default)]
pub struct AgentCognitionStore {
    active_by_agent: BTreeMap<String, ActiveCognitionRequest>,
    digest_by_request_id: BTreeMap<String, Digest32>,
    subject_by_request_id: BTreeMap<String, String>,
    /// Feedback replay state is partitioned by `(agent_subject, session)`;
    /// no Agent can observe another Agent's feedback history.
    feedback_partitions: BTreeMap<(String, String), FeedbackPartition>,
    /// A durable Runtime history that is incomplete or not fully acknowledged
    /// fences only the matching Agent/session from starting a fresh request.
    feedback_recovery_blocked: BTreeMap<(String, String), String>,
}

#[derive(Debug, Clone, Default)]
struct FeedbackPartition {
    next_seq: u64,
    digest_by_id: BTreeMap<String, Digest32>,
    digest_by_seq: BTreeMap<u64, (String, Digest32)>,
    replay_order: VecDeque<String>,
    held: BTreeMap<u64, FeedbackEnvelopeV1>,
}

impl AgentCognitionStore {
    /// Admit the reduced compatibility lane with the same per-Agent
    /// single-flight semantics. Production-target callers should prefer
    /// `begin_request`, which also verifies the trusted outer context.
    pub fn begin_turn(
        &mut self,
        turn: &ContinuousAgentTurnContextV1,
    ) -> Result<(), CognitionError> {
        turn.validate_for_agent(turn.agent_id.as_str())?;
        if let Some(reason) = self
            .feedback_recovery_blocked
            .get(&(turn.agent_id.clone(), turn.agent_session_id.clone()))
        {
            return Err(CognitionError::new(
                "feedback_recovery_blocked",
                reason.clone(),
            ));
        }
        if let Some(active) = self.active_by_agent.get(&turn.agent_id) {
            if active.session_id == turn.agent_session_id
                && active.turn_id == turn.agent_turn_id
                && active.request_id == turn.decision_request_id
                && active.request_digest == turn.request_digest
            {
                return Ok(());
            }
            return Err(CognitionError::new(
                "agent_busy",
                "an Agent already has an in-flight cognition request",
            ));
        }
        if self
            .digest_by_request_id
            .contains_key(&turn.decision_request_id)
        {
            if self
                .subject_by_request_id
                .get(&turn.decision_request_id)
                .is_some_and(|subject| subject != &turn.agent_id)
            {
                return Err(CognitionError::new(
                    "request_identity_collision",
                    "decision_request_id was reused by another Agent subject",
                ));
            }
            return Err(CognitionError::new(
                "request_replay",
                "decision_request_id was already admitted and is no longer active",
            ));
        }
        self.digest_by_request_id.insert(
            turn.decision_request_id.clone(),
            turn.request_digest.clone(),
        );
        self.subject_by_request_id
            .insert(turn.decision_request_id.clone(), turn.agent_id.clone());
        self.active_by_agent.insert(
            turn.agent_id.clone(),
            ActiveCognitionRequest {
                session_id: turn.agent_session_id.clone(),
                turn_id: turn.agent_turn_id.clone(),
                request_id: turn.decision_request_id.clone(),
                request_digest: turn.request_digest.clone(),
            },
        );
        Ok(())
    }

    pub fn begin_request(
        &mut self,
        request: ContinuousAgentRequestContextV1,
    ) -> Result<(), CognitionError> {
        request.validate()?;
        if let Some(reason) = self.feedback_recovery_blocked.get(&(
            request.agent_subject.clone(),
            request.agent_session_id.clone(),
        )) {
            return Err(CognitionError::new(
                "feedback_recovery_blocked",
                reason.clone(),
            ));
        }
        let digest = request.request_digest();
        if let Some(previous) = self.digest_by_request_id.get(&request.decision_request_id) {
            if previous != &digest {
                return Err(CognitionError::new(
                    "request_identity_collision",
                    "decision_request_id was reused with different canonical inputs",
                ));
            }
            if self
                .subject_by_request_id
                .get(&request.decision_request_id)
                .is_some_and(|subject| subject != &request.agent_subject)
            {
                return Err(CognitionError::new(
                    "request_identity_collision",
                    "decision_request_id was reused by another Agent subject",
                ));
            }
            if self.active_by_agent.values().any(|active| {
                active.request_id == request.decision_request_id && active.request_digest == digest
            }) {
                return Ok(());
            }
            return Err(CognitionError::new(
                "request_replay",
                "decision_request_id was already admitted and is no longer active",
            ));
        }
        if self.active_by_agent.contains_key(&request.agent_subject) {
            return Err(CognitionError::new(
                "agent_busy",
                "an Agent already has an in-flight cognition request",
            ));
        }
        self.digest_by_request_id
            .insert(request.decision_request_id.clone(), digest.clone());
        self.subject_by_request_id.insert(
            request.decision_request_id.clone(),
            request.agent_subject.clone(),
        );
        self.active_by_agent.insert(
            request.agent_subject,
            ActiveCognitionRequest {
                session_id: request.agent_session_id,
                turn_id: request.agent_turn_id,
                request_id: request.decision_request_id,
                request_digest: digest,
            },
        );
        Ok(())
    }

    pub fn accept_feedback(&mut self, feedback: FeedbackEnvelopeV1) -> Result<(), CognitionError> {
        if !self.active_by_agent.contains_key(&feedback.agent_subject)
            && self
                .active_by_agent
                .keys()
                .any(|agent| agent != &feedback.agent_subject)
        {
            return Err(CognitionError::new(
                "cross_agent_feedback",
                "feedback does not belong to the active Agent subject",
            ));
        }
        validate_feedback_contract(&feedback)?;
        let partition_key = (
            feedback.agent_subject.clone(),
            feedback.agent_session_id.clone(),
        );
        let feedback_digest_value = feedback_digest(&feedback);
        let Some(active) = self.active_by_agent.get(&feedback.agent_subject) else {
            if let Some(partition) = self.feedback_partitions.get(&partition_key) {
                if let Some(previous) = partition.digest_by_id.get(&feedback.feedback_id) {
                    return Self::replay_or_reject_feedback(previous, feedback_digest_value);
                }
                if let Some((previous_id, previous_digest)) =
                    partition.digest_by_seq.get(&feedback.feedback_seq)
                    && (previous_id != &feedback.feedback_id
                        || previous_digest != &feedback_digest_value)
                {
                    return Err(CognitionError::new(
                        "feedback_identity_collision",
                        "feedback_seq was reused with a different envelope",
                    ));
                }
            }
            return Err(CognitionError::new(
                "unknown_feedback",
                "feedback does not match an active cognition request",
            ));
        };
        let partition = self.feedback_partitions.entry(partition_key).or_default();
        if let Some(previous) = partition.digest_by_id.get(&feedback.feedback_id) {
            return Self::replay_or_reject_feedback(previous, feedback_digest_value);
        }
        if active.session_id != feedback.agent_session_id
            || active.turn_id != feedback.agent_turn_id
            || active.request_id != feedback.decision_request_id
        {
            return Err(CognitionError::new(
                "feedback_correlation_mismatch",
                "feedback cognition lineage does not match the active request",
            ));
        }
        if active.request_digest != feedback.request_digest {
            return Err(CognitionError::new(
                "feedback_digest_mismatch",
                "feedback request digest does not match the active request",
            ));
        }
        let expected = partition.next_seq.saturating_add(1);
        if feedback.feedback_seq > expected {
            if let Some(previous) = partition.held.get(&feedback.feedback_seq) {
                if feedback_digest(previous) != feedback_digest_value {
                    return Err(CognitionError::new(
                        "feedback_identity_collision",
                        "feedback_seq was reused with a different held envelope",
                    ));
                }
                return Err(CognitionError::new(
                    "feedback_sequence_gap",
                    format!("expected feedback sequence {expected}"),
                ));
            }
            if let Some(previous) = partition
                .held
                .values()
                .find(|previous| previous.feedback_id == feedback.feedback_id)
            {
                if feedback_digest(previous) != feedback_digest_value {
                    return Err(CognitionError::new(
                        "feedback_id_conflict",
                        "feedback_id was reused with a different held envelope",
                    ));
                }
                return Err(CognitionError::new(
                    "feedback_sequence_gap",
                    format!("expected feedback sequence {expected}"),
                ));
            }
            if partition.held.len() >= MAX_FEEDBACK_REPLAY_ENTRIES {
                return Err(CognitionError::new(
                    "feedback_sequence_overflow",
                    "feedback sequence gap exceeds the bounded replay window",
                ));
            }
            partition.held.insert(feedback.feedback_seq, feedback);
            return Err(CognitionError::new(
                "feedback_sequence_gap",
                format!("expected feedback sequence {expected}"),
            ));
        }
        if feedback.feedback_seq < expected {
            if let Some((previous_id, previous)) =
                partition.digest_by_seq.get(&feedback.feedback_seq)
            {
                if previous_id == &feedback.feedback_id {
                    return Self::replay_or_reject_feedback(previous, feedback_digest_value);
                }
                return Err(CognitionError::new(
                    "feedback_identity_collision",
                    "feedback_seq was reused with a different envelope",
                ));
            }
            return Err(CognitionError::new(
                "feedback_sequence_gap",
                format!(
                    "feedback sequence {} is behind {expected}",
                    feedback.feedback_seq
                ),
            ));
        }
        Self::remember_feedback(
            partition,
            feedback.feedback_id.clone(),
            feedback_digest_value,
        );
        partition.next_seq = feedback.feedback_seq;
        if matches!(
            feedback.status.as_str(),
            "committed" | "rejected" | "failed"
        ) {
            self.active_by_agent.remove(&feedback.agent_subject);
        }
        while let Some(next_feedback) = partition.held.remove(&partition.next_seq.saturating_add(1))
        {
            let next_digest = feedback_digest(&next_feedback);
            Self::remember_feedback(partition, next_feedback.feedback_id.clone(), next_digest);
            partition.next_seq = next_feedback.feedback_seq;
            if matches!(
                next_feedback.status.as_str(),
                "committed" | "rejected" | "failed"
            ) {
                self.active_by_agent.remove(&next_feedback.agent_subject);
            }
        }
        Ok(())
    }

    fn replay_or_reject_feedback(
        previous: &Digest32,
        current: Digest32,
    ) -> Result<(), CognitionError> {
        if previous == &current {
            return Ok(());
        }
        Err(CognitionError::new(
            "feedback_id_conflict",
            "feedback_id was reused with a different envelope",
        ))
    }

    fn remember_feedback(partition: &mut FeedbackPartition, feedback_id: String, digest: Digest32) {
        let sequence = partition.next_seq.saturating_add(1);
        partition
            .digest_by_seq
            .insert(sequence, (feedback_id.clone(), digest.clone()));
        partition.digest_by_id.insert(feedback_id.clone(), digest);
        partition.replay_order.push_back(feedback_id);
        while partition.replay_order.len() > MAX_FEEDBACK_REPLAY_ENTRIES {
            let Some(expired_id) = partition.replay_order.pop_front() else {
                break;
            };
            partition.digest_by_id.remove(&expired_id);
            if let Some(sequence) = partition
                .digest_by_seq
                .iter()
                .find_map(|(sequence, (id, _))| (id == &expired_id).then_some(*sequence))
            {
                partition.digest_by_seq.remove(&sequence);
            }
        }
    }

    pub fn clear_agent(&mut self, agent_subject: &str) {
        self.active_by_agent.remove(agent_subject);
    }

    pub fn contains_feedback(
        &self,
        agent_subject: &str,
        agent_session_id: &str,
        feedback_id: &str,
    ) -> bool {
        self.feedback_partitions
            .iter()
            .any(|((subject, session), partition)| {
                subject == agent_subject
                    && session == agent_session_id
                    && partition.digest_by_id.contains_key(feedback_id)
            })
    }
}

fn default_schema_version() -> u16 {
    1
}

fn validate_outer_fields(value: &Value, allowed: &[&str]) -> Result<(), CognitionError> {
    let Some(object) = value.as_object() else {
        return Err(CognitionError::new(
            "unknown_context_field",
            "continuous-agent outer context must be a JSON object",
        ));
    };
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.iter().any(|allowed| allowed == field))
    {
        return Err(CognitionError::new(
            "unknown_context_field",
            format!("unknown continuous-agent outer field `{field}`"),
        ));
    }
    Ok(())
}

fn feedback_digest(feedback: &FeedbackEnvelopeV1) -> Digest32 {
    oasis7_agent_api::feedback_digest(&super::agent_api_compat::feedback_to_api(feedback))
}

fn validate_feedback_contract(feedback: &FeedbackEnvelopeV1) -> Result<(), CognitionError> {
    super::agent_api_compat::feedback_to_api(feedback).validate()
}
