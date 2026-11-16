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
/// Work-only observations selected by the original metadata kernel. These
/// carry original DTOs and allocation sizes, never a caller business verdict.
pub enum FeedbackHistoryWorkV1<'a> {
    Feedback(&'a FeedbackEnvelopeV1),
    Lineage(&'a RuntimeReceiptLineageV1),
    Text(&'a str),
    Slots(usize),
    ActionId(u64),
}
type FeedbackMetadataRowsV1 = Vec<(FeedbackEnvelopeV1, String, String)>;

impl WorldServiceFeedbackHistory {
    pub fn metadata(
        &self,
        agent: &str,
    ) -> Result<Vec<(FeedbackEnvelopeV1, String, String)>, String> {
        match self.metadata_with_work_v1(agent, &mut |_| Ok::<_, std::convert::Infallible>(())) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }
    pub fn metadata_with_work_v1<E>(
        &self,
        agent: &str,
        work: &mut impl FnMut(FeedbackHistoryWorkV1<'_>) -> Result<(), E>,
    ) -> Result<Result<FeedbackMetadataRowsV1, String>, E> {
        fn note<E>(
            s: &str,
            work: &mut impl FnMut(FeedbackHistoryWorkV1<'_>) -> Result<(), E>,
        ) -> Result<String, E> {
            work(FeedbackHistoryWorkV1::Text(s))?;
            Ok(s.to_owned())
        }
        work(FeedbackHistoryWorkV1::Text(&self.agent_id))?;
        if self.agent_id != agent || self.records.len() > 16384 {
            return Ok(Err(note(
                "scoped feedback history audience or bound invalid",
                work,
            )?));
        }
        let mut output = Vec::new();
        for record in &self.records {
            let f = &record.feedback;
            work(FeedbackHistoryWorkV1::Feedback(f))?;
            if f.agent_subject != agent {
                return Ok(Err(note(
                    "scoped feedback history contains another Agent",
                    work,
                )?));
            }
            if f.status == "committed" {
                let Some(r) = record.receipt_lineage.as_ref() else {
                    return Ok(Err(note("committed feedback receipt missing", work)?));
                };
                work(FeedbackHistoryWorkV1::Lineage(r))?;
                if let Err(e) = r.validate() {
                    return Ok(Err(note(e.code(), work)?));
                }
                let first_bad = Some(r.receipt_id.as_str()) != f.runtime_receipt_id.as_deref()
                    || r.agent_id != f.agent_subject
                    || r.agent_session_id != f.agent_session_id
                    || r.agent_turn_id != f.agent_turn_id
                    || r.decision_request_id != f.decision_request_id
                    || r.request_digest != f.request_digest.to_string()
                    || r.feedback_id != f.feedback_id;
                let bad = if first_bad {
                    true
                } else {
                    match f.candidate_action_id {
                        None => true,
                        Some(id) => {
                            work(FeedbackHistoryWorkV1::ActionId(id))?;
                            r.action_id != format!("action:{id}")
                        }
                    }
                };
                if bad {
                    return Ok(Err(note("scoped feedback receipt lineage mismatch", work)?));
                }
            } else if record.receipt_lineage.is_some() {
                return Ok(Err(note("unexpected feedback receipt lineage", work)?));
            }
            if output.len() == output.capacity() {
                let capacity = output.capacity().saturating_mul(2).max(4);
                work(FeedbackHistoryWorkV1::Slots(capacity))?;
                output.reserve_exact(capacity - output.len());
            }
            work(FeedbackHistoryWorkV1::Text(
                &record.original_envelope_digest,
            ))?;
            work(FeedbackHistoryWorkV1::Text(&record.delivery_state))?;
            output.push((
                f.clone(),
                record.original_envelope_digest.clone(),
                record.delivery_state.clone(),
            ));
        }
        Ok(Ok(output))
    }
}
