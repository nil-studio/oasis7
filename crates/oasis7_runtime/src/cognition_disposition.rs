//! Only an authenticated, actually evaluated stale commit can mint this token.
use crate::{RuntimeCognitionCommitRequestV1, RuntimeFeedbackRequestV1, World};

pub enum AuthenticatedCognitionDisposition {
    Committed(serde_json::Value),
    RejectedStale(ValidatedStaleCognitionRejection),
}
/// Not a receipt, action or public request. Private fields prevent adapters
/// from manufacturing an approved rejection from arbitrary diagnostic text.
pub struct ValidatedStaleCognitionRejection {
    request: Box<RuntimeFeedbackRequestV1>,
    envelope_digest: String,
    idempotency_key: String,
}
impl ValidatedStaleCognitionRejection {
    pub(crate) fn from_evaluated_request(
        request: &RuntimeCognitionCommitRequestV1,
        action: &crate::Action,
    ) -> Result<Self, String> {
        let (envelope_digest, idempotency_key) =
            World::stale_cognition_rejection_identity(request, action)
                .map_err(|_| "canonical rejected cognition identity invalid")?;
        Ok(Self {
            request: Box::new(RuntimeFeedbackRequestV1 {
                feedback_id: None,
                agent_subject: request.agent_id.clone(),
                agent_session_id: request.agent_session_id.clone(),
                agent_turn_id: request.agent_turn_id.clone(),
                decision_request_id: request.decision_request_id.clone(),
                candidate_action_id: None,
                runtime_receipt_id: None,
                status: "rejected".into(),
                request_digest: request.request_digest.clone(),
                reject_reason: Some(
                    crate::CognitionCommitRejectReasonV1::StaleBase
                        .code()
                        .into(),
                ),
            }),
            envelope_digest,
            idempotency_key,
        })
    }
    pub(crate) fn into_parts(self) -> (RuntimeFeedbackRequestV1, String, String) {
        (*self.request, self.envelope_digest, self.idempotency_key)
    }
    pub fn rejection_text(&self) -> String {
        format!(
            "{:?}",
            crate::WorldError::DistributedValidationFailed {
                reason: crate::CognitionCommitRejectReasonV1::StaleBase
                    .code()
                    .into(),
            }
        )
    }
    /// Publish to an untouched pre-intent World, never the failed candidate.
    /// The canonical adapter must commit this outbox and its rejected result
    /// atomically. Allocator errors abort execution rather than claiming proof.
    pub fn publish(self, world: &mut World) -> Result<(), String> {
        world.publish_authenticated_stale_cognition_rejection(self)
    }
}
