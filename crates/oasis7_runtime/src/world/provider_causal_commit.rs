//! Provider evidence is proposal data; Runtime derives its intent and all grant authority.
use super::*;
use crate::{Action, AgentDecisionCausalContextV1, RuntimeCognitionCommitRequestV1, RuntimeCognitionResponseArtifactV1, WorldCommitRecordV1, RuntimeReceiptLineageV1};
#[derive(Debug, Clone, Default)]
pub struct ProviderDecisionEvidenceV1 {
    pub expected_consequence: JsonValue,
    pub stakes: JsonValue,
    pub alternative: JsonValue,
    pub evidence_refs: Vec<String>,
    pub correction_refs: Vec<String>,
    pub reason: Option<String>,
    pub dissent: Option<String>,
}
#[derive(Debug)]
pub enum ProviderCognitionCommitError {
    AgencyBinding(WorldError),
    Commit(WorldError),
}
impl World {
    /// Commit a provider proposal with the current Intent and canonical grant
    /// authority selected inside Runtime. Evidence cannot select an override,
    /// owner control, delegation grant, or a different Intent.
    pub fn commit_provider_cognition_action(
        &mut self,
        request: RuntimeCognitionCommitRequestV1,
        action: Action,
        response_artifact: RuntimeCognitionResponseArtifactV1,
        evidence: ProviderDecisionEvidenceV1,
    ) -> Result<(WorldCommitRecordV1, RuntimeReceiptLineageV1), ProviderCognitionCommitError> {
        let already_committed = self.cognition().get("commit_records").and_then(serde_json::Value::as_array).into_iter().flatten().any(|marker| {
            marker.get("status").and_then(serde_json::Value::as_str) == Some("committed")
                && marker.get("decision_request_id").and_then(serde_json::Value::as_str) == Some(request.decision_request_id.as_str())
                && marker.get("request_digest").and_then(serde_json::Value::as_str) == Some(request.request_digest.as_str())
        });
        if !already_committed {
            let intent_id = self.state().agents.get(&request.agent_id).and_then(|cell| cell.intent.as_ref()).filter(|intent| matches!(intent.status.as_str(), "accepted" | "blocked")).map(|intent| intent.intent_id.clone());
            let context = AgentDecisionCausalContextV1 {
                intent_id,
                expected_consequence: evidence.expected_consequence,
                stakes: evidence.stakes,
                alternative: evidence.alternative,
                evidence_refs: evidence.evidence_refs,
                correction_refs: evidence.correction_refs,
                reason: evidence.reason,
                dissent: evidence.dissent,
                ..Default::default()
            };
            self.bind_agent_causal_decision(&request.decision_request_id, &request.agent_id, &action, context).map_err(ProviderCognitionCommitError::AgencyBinding)?;
        }
        self.commit_cognition_action(request, action, response_artifact).map_err(ProviderCognitionCommitError::Commit)
    }
}
