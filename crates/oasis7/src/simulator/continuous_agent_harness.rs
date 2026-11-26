//! Versioned cognition wire types and the small amount of host-side state
//! required by the continuous-agent provider contract.
//!
//! The existing [`DecisionRequest`] and [`DecisionResponse`] types remain the
//! provider's inner payload.  This module deliberately adds an outer context
//! instead of teaching the legacy DTOs about cognition identity.  Runtime
//! persistence, action receipts, and continuation scheduling consume these
//! values but remain runtime-owned.

use std::collections::{BTreeMap, VecDeque};

#[cfg(test)]
use serde_json::Value;

pub use oasis7_agent_api::{
    CognitionError, ContinuousAgentTurnContextV1, Digest32, ResponseArtifactIdentityV1,
    RuntimeBindingV1, h_v1,
};

#[cfg(test)]
use super::FeedbackEnvelope;

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

pub use oasis7_agent_api::{
    ContinuousAgentRequestContextV1, FeedbackEnvelopeV1, MemoryWriteIntentV1,
};
pub type ContinuousAgentResponseContextV1 =
    oasis7_agent_api::ContinuousAgentResponseContextV1<super::Action, super::AgentQuery>;

#[cfg(test)]
pub(crate) fn feedback_v1_from_legacy_value(
    value: Value,
) -> Result<FeedbackEnvelopeV1, CognitionError> {
    let legacy: FeedbackEnvelope = serde_json::from_value(value)
        .map_err(|error| CognitionError::new("legacy_feedback_invalid", error.to_string()))?;
    let _ = legacy;
    Err(CognitionError::new(
        "legacy_feedback_ambiguous",
        "legacy feedback lacks a Runtime disposition or committed receipt",
    ))
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

fn feedback_digest(feedback: &FeedbackEnvelopeV1) -> Digest32 {
    oasis7_agent_api::feedback_digest(&super::agent_api_compat::feedback_to_api(feedback))
}

fn validate_feedback_contract(feedback: &FeedbackEnvelopeV1) -> Result<(), CognitionError> {
    super::agent_api_compat::feedback_to_api(feedback).validate()
}
