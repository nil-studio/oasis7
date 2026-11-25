use super::super::continuous_agent_harness::ResponseArtifactIdentityV1;
use super::{AsyncAgentRunnerError, ContinuousAgentTurnContextV1, Digest32, FeedbackEnvelopeV1};

use crate::runtime::RuntimeReceiptLineageV1;

/// Opaque proof returned by a Runtime-owned world readback verifier. The
/// simulator can carry and inspect identity, but cannot mint this token from
/// provider feedback or a caller-provided receipt alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeReceiptReadbackHandleV1 {
    receipt_id: String,
    receipt_digest: Digest32,
    response_artifact_digest: Digest32,
    verified_lineage: RuntimeReceiptLineageV1,
}

impl RuntimeReceiptReadbackHandleV1 {
    pub fn receipt_id(&self) -> &str {
        self.receipt_id.as_str()
    }

    pub fn receipt_digest(&self) -> &Digest32 {
        &self.receipt_digest
    }

    pub fn response_artifact_digest(&self) -> &Digest32 {
        &self.response_artifact_digest
    }

    pub(crate) fn verified_lineage(&self) -> &RuntimeReceiptLineageV1 {
        &self.verified_lineage
    }

    /// Runtime's concrete World adapter constructs this only after it has
    /// read back and verified both durable identities. Keep the constructor
    /// crate-private: provider callers and external verifier implementations
    /// cannot mint a readback capability from a DTO.
    pub(crate) fn from_durable_world_readback(
        receipt: &RuntimeReceiptLineageV1,
        response_identity: &ResponseArtifactIdentityV1,
    ) -> Self {
        Self {
            receipt_id: receipt.receipt_id.clone(),
            receipt_digest: Digest32::from(receipt.receipt_digest.clone()),
            response_artifact_digest: response_identity.artifact_digest.clone(),
            verified_lineage: receipt.clone(),
        }
    }
}

/// Integration seam for Runtime's strict World-readback proof. A provider or
/// simulator fixture cannot authorize this method by echoing receipt fields;
/// the implementation must be backed by Runtime's durable commit/readback.
/// The verifier is borrowed for one synchronous call and is never retained or
/// transferred to another thread, so it intentionally has no `Send` or `Sync`
/// supertrait requirement.
pub trait RuntimeReceiptReadbackVerifier {
    fn verify_world_readback(
        &self,
        context: &ContinuousAgentTurnContextV1,
        feedback: &FeedbackEnvelopeV1,
        receipt: &RuntimeReceiptLineageV1,
        response_identity: &ResponseArtifactIdentityV1,
    ) -> Result<RuntimeReceiptReadbackHandleV1, AsyncAgentRunnerError>;
}

pub(super) fn validate_feedback(
    context: &ContinuousAgentTurnContextV1,
    agent_id: &str,
    feedback: &FeedbackEnvelopeV1,
) -> Result<(), AsyncAgentRunnerError> {
    if feedback.agent_subject != agent_id
        || feedback.agent_subject != context.agent_id
        || feedback.agent_session_id != context.agent_session_id
        || feedback.agent_turn_id != context.agent_turn_id
        || feedback.decision_request_id != context.decision_request_id
        || feedback.request_digest != context.request_digest
        || feedback.provenance != "runtime_authoritative"
        || feedback.feedback_seq == 0
        || !feedback.request_digest.is_canonical_blake3()
    {
        return Err(AsyncAgentRunnerError::Cognition(
            "Runtime feedback does not correlate to the actor turn".to_string(),
        ));
    }
    if feedback.feedback_id.trim().is_empty()
        || !matches!(
            feedback.status.as_str(),
            "pending" | "committed" | "rejected" | "failed"
        )
    {
        return Err(AsyncAgentRunnerError::Cognition(
            "Runtime feedback identity or status is invalid".to_string(),
        ));
    }
    if feedback.status == "committed"
        && feedback
            .runtime_receipt_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(AsyncAgentRunnerError::Cognition(
            "committed feedback requires a Runtime receipt".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_runtime_receipt_lineage(
    context: &ContinuousAgentTurnContextV1,
    feedback: &FeedbackEnvelopeV1,
    receipt: &RuntimeReceiptLineageV1,
) -> Result<(), AsyncAgentRunnerError> {
    receipt
        .validate()
        .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
    if !receipt.receipt_digest.starts_with("blake3:")
        || !receipt.envelope_digest.starts_with("blake3:")
        || !receipt.request_digest.starts_with("blake3:")
        || !Digest32::from(receipt.receipt_digest.clone()).is_canonical_blake3()
        || !Digest32::from(receipt.envelope_digest.clone()).is_canonical_blake3()
        || !Digest32::from(receipt.request_digest.clone()).is_canonical_blake3()
    {
        return Err(AsyncAgentRunnerError::Cognition(
            "Runtime receipt proof digests must be canonical BLAKE3-256 values".to_string(),
        ));
    }
    if receipt.agent_id != context.agent_id
        || receipt.agent_session_id != context.agent_session_id
        || receipt.agent_turn_id != context.agent_turn_id
        || receipt.decision_request_id != context.decision_request_id
        || receipt.request_digest != context.request_digest.to_string()
        || receipt.feedback_id != feedback.feedback_id
        || feedback.runtime_receipt_id.as_deref() != Some(receipt.receipt_id.as_str())
    {
        return Err(AsyncAgentRunnerError::Cognition(
            "Runtime receipt does not correlate to the actor turn and feedback".to_string(),
        ));
    }
    let Some(candidate_action_id) = feedback.candidate_action_id else {
        return Err(AsyncAgentRunnerError::Cognition(
            "committed feedback requires a Runtime action lineage".to_string(),
        ));
    };
    // Runtime's durable action identity is serialized as `action:<id>`;
    // compatibility fixtures historically carried the numeric suffix alone.
    // Accept exactly either representation while rejecting every other value.
    let numeric_action_id = candidate_action_id.to_string();
    let runtime_action_id = format!("action:{numeric_action_id}");
    if receipt.action_id != numeric_action_id && receipt.action_id != runtime_action_id {
        return Err(AsyncAgentRunnerError::Cognition(
            "Runtime receipt action lineage does not match feedback".to_string(),
        ));
    }
    Ok(())
}
