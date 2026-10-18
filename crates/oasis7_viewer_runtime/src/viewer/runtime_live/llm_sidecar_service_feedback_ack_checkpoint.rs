//! Original signed Agent authority and immutable private acceptance checkpoint validation.
use super::*;
impl PendingFeedbackAck {
    pub(in crate::viewer::runtime_live) fn validate(
        &self,
        pending: &PendingProviderServiceIntent,
    ) -> Result<(), String> {
        let WorldServicePayloadV1::FeedbackAck(signed) = &self.payload else {
            return Err("feedback ACK codec mismatch".into());
        };
        signed.request.validate()?;
        let WorldServicePayloadV1::Cognition(original) = &pending.payload else {
            return Err("feedback ACK original signed cognition missing".into());
        };
        crate::world_service::authority::verify_read_request("cognition", original)?;
        let binding = &pending.cognition.request.request_context.runtime_binding;
        if signed.subject_public_key != original.subject_public_key
            || signed.request.delegation_generation != original.request.delegation_generation
            || self.correlation.key.world != pending.correlation.key.world
            || self.correlation.key.verified_subject != original.subject_public_key
            || pending.correlation.key.verified_subject != original.subject_public_key
            || original.request.request.captured_base_binding.world_id != binding.world_id
            || original.request.request.captured_base_binding.branch_id != binding.branch_id
            || self.correlation.key.world.world_id != binding.world_id
        {
            return Err("feedback ACK checkpoint signer/authority mismatch".into());
        }

        crate::world_service::authority::verify_read_request("feedback_ack", signed)
            .map_err(|error| format!("feedback ACK checkpoint signature invalid: {error}"))?;
        let request = &pending.cognition.request.request_context;
        let feedback: crate::simulator::FeedbackEnvelopeV1 =
            serde_json::from_value(self.original_feedback.clone()).map_err(|e| e.to_string())?;
        let encoded = oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.runtime.feedback-envelope.v1",
            &self.original_feedback,
        ))
        .map_err(|e| e.to_string())?;
        let original_digest = format!("blake3:{}", blake3::hash(&encoded));
        if signed.request.agent_id != request.agent_subject
            || signed.request.agent_session_id != request.agent_session_id
            || signed.request.agent_turn_id != request.agent_turn_id
            || signed.request.decision_request_id != request.decision_request_id
            || signed.request.request_digest != request.request_digest.to_string()
            || feedback.agent_subject != signed.request.agent_id
            || feedback.agent_session_id != signed.request.agent_session_id
            || feedback.agent_turn_id != signed.request.agent_turn_id
            || feedback.decision_request_id != signed.request.decision_request_id
            || feedback.request_digest.to_string() != signed.request.request_digest
            || feedback.feedback_id != signed.request.feedback_id
            || feedback.feedback_seq != signed.request.feedback_seq
            || feedback.runtime_receipt_id != signed.request.runtime_receipt_id
            || original_digest != signed.request.original_envelope_digest
            || crate::world_service::derive_correlation(
                self.correlation.key.world.clone(),
                &self.payload,
            )? != self.correlation
            || (self.issued && !self.delivered)
        {
            return Err("feedback ACK checkpoint original identity mismatch".into());
        }
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn validate_memory(
        &self,
        pending: &PendingProviderServiceIntent,
        memory: &MemoryWriteStore,
    ) -> Result<(), String> {
        self.validate(pending)?;
        let WorldServicePayloadV1::FeedbackAck(signed) = &self.payload else {
            unreachable!()
        };
        self.receipt_lineage.validate().map_err(|e| e.to_string())?;
        if signed.request.private_acceptance_digest
            != private_acceptance_digest(
                pending,
                &self.original_feedback,
                &self.receipt_lineage,
                memory,
            )?
        {
            return Err("durable private feedback acceptance binding mismatch".into());
        }
        Ok(())
    }
}
