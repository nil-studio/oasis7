//! Same response identity/lineage predicates and original JSON preimage,
//! with real caller work for fields, graph copies and the canonical hash.
use super::*;
use oasis7_wasm_abi::{CanonicalCborStreamError as WorkError, CapabilityCanonicalWorkV1};
fn fields<A: CapabilityCanonicalWorkV1>(
    value: &RuntimeCognitionResponseArtifactV1,
    source: &A,
) -> Result<usize, WorkError> {
    let RuntimeCognitionResponseArtifactV1 {
        schema_version: _,
        context_discriminator,
        context_version: _,
        agent_session_id,
        agent_turn_id,
        decision_request_id,
        retry_seq: _,
        transport_attempt: _,
        request_digest,
        response_digest,
        artifact_digest,
    } = value;
    let mut bytes = 0usize;
    for text in [
        context_discriminator,
        agent_session_id,
        agent_turn_id,
        decision_request_id,
        request_digest,
        response_digest,
        artifact_digest,
    ] {
        for part in text.as_bytes().chunks(8192) {
            source.charge_work(1 + 4 * part.len() as u64, part.len() as u64)?;
        }
        bytes = bytes
            .checked_add(text.len())
            .ok_or(WorkError::ResourceLimited("response identity fields"))?;
    }
    source.charge_work(16, 64)?;
    Ok(bytes)
}
fn validate_fields(
    value: &RuntimeCognitionResponseArtifactV1,
) -> Result<(), CognitionRecoveryError> {
    let bounded = |value: &str| !value.trim().is_empty() && value.len() <= 256;
    if value.schema_version != RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION
        || value.context_discriminator != RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR
        || value.context_version != RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION
        || !bounded(&value.agent_session_id)
        || !bounded(&value.agent_turn_id)
        || !bounded(&value.decision_request_id)
        || value.retry_seq == 0
        || value.transport_attempt == 0
        || !canonical_blake3(&value.request_digest)
        || !canonical_blake3(&value.response_digest)
        || !canonical_blake3(&value.artifact_digest)
    {
        return Err(CognitionRecoveryError::new(
            "runtime_cognition_response_artifact_invalid",
        ));
    }
    Ok(())
}
impl RuntimeCognitionResponseArtifactV1 {
    pub fn recompute_artifact_digest_with_work_v1<A: CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<PaidCognitionDigestDataV1<A::Charge>, WorkError> {
        let bytes = fields(self, source)?;
        let graph = bytes
            .checked_mul(4)
            .and_then(|n| n.checked_add(16 * 256 + std::mem::size_of::<JsonValue>()))
            .ok_or(WorkError::ResourceLimited("response identity JSON graph"))?;
        source.charge_work(1 + graph as u64, graph as u64)?;
        let _fee = if source.is_metered() {
            Some(source.reserve(graph)?)
        } else {
            None
        };
        // Keep the original serializer and removal operation, including its JSON
        // number/string representation. No parallel digest preimage is invented.
        let mut payload = serde_json::to_value(self).unwrap_or(JsonValue::Null);
        payload
            .as_object_mut()
            .expect("response artifact identity is an object")
            .remove("artifact_digest");
        cognition_digest_with_work_v1(
            "oasis7.cognition.response-artifact-identity.v1",
            &payload,
            source,
        )
    }
    pub fn validate_with_work_v1<A: CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<Result<(), CognitionRecoveryError>, WorkError> {
        fields(self, source)?;
        if let Err(error) = validate_fields(self) {
            return Ok(Err(error));
        }
        let digest = self.recompute_artifact_digest_with_work_v1(source)?;
        source.charge_work(
            1,
            (self.artifact_digest.len() + digest.as_str().len()) as u64,
        )?;
        if self.artifact_digest != digest.as_str() {
            return Ok(Err(CognitionRecoveryError::new(
                "runtime_cognition_response_artifact_invalid",
            )));
        }
        Ok(Ok(()))
    }
    pub fn validate_for_request_with_work_v1<A: CapabilityCanonicalWorkV1>(
        &self,
        request: &RuntimeCognitionCommitRequestV1,
        source: &A,
    ) -> Result<Result<(), CognitionRecoveryError>, WorkError> {
        if let Err(error) = self.validate_with_work_v1(source)? {
            return Ok(Err(error));
        }
        for text in [
            &self.agent_session_id,
            &request.agent_session_id,
            &self.agent_turn_id,
            &request.agent_turn_id,
            &self.decision_request_id,
            &request.decision_request_id,
            &self.request_digest,
            &request.request_digest,
        ] {
            source.charge_work(1, text.len() as u64)?;
        }
        source.charge_work(4, 32)?;
        if self.agent_session_id != request.agent_session_id
            || self.agent_turn_id != request.agent_turn_id
            || self.decision_request_id != request.decision_request_id
            || self.retry_seq != request.retry_seq
            || self.transport_attempt != request.transport_attempt
            || self.request_digest != request.request_digest
        {
            return Ok(Err(CognitionRecoveryError::new(
                "runtime_cognition_response_lineage_mismatch",
            )));
        }
        Ok(Ok(()))
    }
}
