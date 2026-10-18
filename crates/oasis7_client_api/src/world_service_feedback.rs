//! Scoped feedback recovery DTOs. These values do not grant execution authority.
use oasis7_agent_api::{FeedbackEnvelopeV1, RuntimeReceiptLineageV1};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldServiceFeedbackReplayRecord {
    pub feedback: FeedbackEnvelopeV1,
    /// Digest of the ORIGINAL validated payload including private projection.
    /// It must not be recomputed from this sanitized envelope.
    pub original_envelope_digest: String,
    pub delivery_state: String,
    pub receipt_lineage: Option<RuntimeReceiptLineageV1>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldServiceFeedbackHistory {
    pub agent_id: String,
    pub records: Vec<WorldServiceFeedbackReplayRecord>,
}
impl WorldServiceFeedbackHistory {
    pub fn metadata(
        &self,
        agent: &str,
    ) -> Result<Vec<(FeedbackEnvelopeV1, String, String)>, String> {
        if self.agent_id != agent || self.records.len() > 16384 {
            return Err("scoped feedback history audience or bound invalid".into());
        }
        self.records
            .iter()
            .map(|record| {
                let f = &record.feedback;
                if f.agent_subject != agent {
                    return Err("scoped feedback history contains another Agent".into());
                }
                if f.status == "committed" {
                    let r = record
                        .receipt_lineage
                        .as_ref()
                        .ok_or("committed feedback receipt missing")?;
                    r.validate().map_err(|e| e.code().to_owned())?;
                    if Some(r.receipt_id.as_str()) != f.runtime_receipt_id.as_deref()
                        || r.agent_id != f.agent_subject
                        || r.agent_session_id != f.agent_session_id
                        || r.agent_turn_id != f.agent_turn_id
                        || r.decision_request_id != f.decision_request_id
                        || r.request_digest != f.request_digest.to_string()
                        || r.feedback_id != f.feedback_id
                        || f.candidate_action_id
                            .is_none_or(|id| r.action_id != format!("action:{id}"))
                    {
                        return Err("scoped feedback receipt lineage mismatch".into());
                    }
                } else if record.receipt_lineage.is_some() {
                    return Err("unexpected feedback receipt lineage".into());
                }
                Ok((
                    f.clone(),
                    record.original_envelope_digest.clone(),
                    record.delivery_state.clone(),
                ))
            })
            .collect()
    }
}
