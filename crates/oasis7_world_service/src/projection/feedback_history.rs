//! Scoped replay metadata construction from authoritative Runtime state.
use oasis7_agent_api::FeedbackEnvelopeV1;
pub use oasis7_client_api::world_service_feedback::{
    WorldServiceFeedbackHistory, WorldServiceFeedbackReplayRecord,
};
use oasis7_runtime::World;
pub fn feedback_history_from_world(
    world: &World,
    agent: &str,
) -> Result<WorldServiceFeedbackHistory, String> {
    let mut records = Vec::new();
    for record in world
        .runtime_feedback_outbox()
        .map_err(|e| format!("{e:?}"))?
    {
        if record.agent_subject != agent {
            continue;
        }
        record.validate().map_err(|e| e.to_string())?;
        let feedback: FeedbackEnvelopeV1 =
            serde_json::from_value(record.payload).map_err(|e| e.to_string())?;
        let receipt_lineage = if feedback.status == "committed" {
            Some(
                world
                    .read_runtime_receipt_lineage(
                        feedback
                            .runtime_receipt_id
                            .as_deref()
                            .ok_or("feedback receipt missing")?,
                    )
                    .map_err(|e| format!("{e:?}"))?,
            )
        } else {
            None
        };
        records.push(WorldServiceFeedbackReplayRecord {
            feedback,
            original_envelope_digest: record.envelope_digest,
            delivery_state: record.state,
            receipt_lineage: receipt_lineage
                .as_ref()
                .map(oasis7_agent_api::RuntimeReceiptLineageV1::from),
        });
    }
    let history = WorldServiceFeedbackHistory {
        agent_id: agent.into(),
        records,
    };
    history.metadata(agent)?;
    Ok(history)
}
