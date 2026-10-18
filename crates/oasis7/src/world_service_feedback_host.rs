//! Application Runner adapters for authenticated feedback history read models.
use crate::simulator::AsyncAgentRunner;
use oasis7_agent_harness::{FeedbackHistoryLineage, FeedbackHistoryProjection};
use oasis7_client_api::world_service_feedback::WorldServiceFeedbackHistory;

fn harness_projections(
    history: &WorldServiceFeedbackHistory,
    agent: &str,
) -> Result<Vec<FeedbackHistoryProjection>, String> {
    history.metadata(agent)?;
    Ok(history
        .records
        .iter()
        .map(|record| FeedbackHistoryProjection {
            feedback: record.feedback.clone(),
            acknowledged: record.delivery_state == "acked",
            lineage: Some(FeedbackHistoryLineage {
                original_envelope_digest: record.original_envelope_digest.clone(),
                delivery_state: record.delivery_state.clone(),
                receipt_lineage: record.receipt_lineage.clone(),
            }),
        })
        .collect())
}
pub(crate) fn restore_preverified(
    history: &WorldServiceFeedbackHistory,
    agent: &str,
    runner: &mut AsyncAgentRunner,
) -> Result<(), String> {
    runner
        .restore_preverified_scoped_feedback_history(&harness_projections(history, agent)?)
        .map_err(|e| e.to_string())
}
pub(crate) fn check_fresh_session(
    history: &WorldServiceFeedbackHistory,
    agent: &str,
    session: &str,
) -> Result<(), String> {
    let mut checker = AsyncAgentRunner::new(1).map_err(|e| e.to_string())?;
    restore_preverified(history, agent, &mut checker)?;
    if checker.feedback_recovery_blocked(agent, session) {
        return Err("Runtime feedback history fences fresh admission".into());
    }
    Ok(())
}
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::runtime::World;
    use oasis7_agent_api::Digest32;
    use oasis7_agent_api::FeedbackEnvelopeV1;
    use oasis7_world_service::projection::WorldServiceProjection;
    fn feedback(agent: &str, session: &str, seq: u64) -> FeedbackEnvelopeV1 {
        FeedbackEnvelopeV1 {
            feedback_id: format!("{agent}-{session}-{seq}"),
            feedback_seq: seq,
            agent_subject: agent.into(),
            agent_session_id: session.into(),
            agent_turn_id: format!("turn-{seq}"),
            decision_request_id: format!("request-{seq}"),
            candidate_action_id: None,
            runtime_receipt_id: None,
            status: "rejected".into(),
            request_digest: Digest32(format!("blake3:{}", "a".repeat(64))),
            reject_reason: Some("stale_base".into()),
            provenance: "runtime_authoritative".into(),
        }
    }
    fn enqueue(world: &mut World, agent: &str, session: &str, seq: u64) {
        let f = feedback(agent, session, seq);
        world.enqueue_runtime_feedback(f.clone()).unwrap();
        world.claim_runtime_feedback(&f.feedback_id).unwrap();
        world
            .retry_runtime_feedback(&f.feedback_id, "private delivery diagnostic")
            .unwrap();
        world.claim_runtime_feedback(&f.feedback_id).unwrap();
        world.ack_runtime_feedback(&f.feedback_id).unwrap();
    }
    #[test]
    fn scoped_feedback_history_uses_actual_world_and_omits_private_other_agent_metadata() {
        let mut world = crate::runtime_host::world::new();
        enqueue(&mut world, "agent-a", "session-a", 1);
        enqueue(&mut world, "agent-a", "session-a", 2);
        enqueue(&mut world, "agent-b", "session-b", 1);
        let projection = WorldServiceProjection::from_world(&world, Some("agent-a")).unwrap();
        let history = projection.feedback_history.unwrap();
        assert_eq!(history.records.len(), 2);
        check_fresh_session(&history, "agent-a", "session-a").unwrap();
        let encoded = serde_json::to_string(&history).unwrap();
        assert!(!encoded.contains("agent-b"));
        assert!(!encoded.contains("private delivery diagnostic"));
        assert!(!encoded.contains("last_error"));
        assert!(!encoded.contains("emitted_events"));
        let actual = world.runtime_feedback_outbox().unwrap();
        assert_eq!(
            history.records[0].original_envelope_digest,
            actual[0].envelope_digest
        );
        let public = WorldServiceProjection::from_world(&world, None).unwrap();
        assert!(public.feedback_history.is_none());
        assert!(
            serde_json::to_value(public)
                .unwrap()
                .get("feedback_history")
                .is_none()
        );
    }
    #[test]
    fn actual_canonical_feedback_gap_fences_only_matching_session() {
        let mut world = crate::runtime_host::world::new();
        enqueue(&mut world, "agent-a", "bad-session", 1);
        enqueue(&mut world, "agent-a", "bad-session", 3);
        let history =
            oasis7_world_service::projection::feedback_history_from_world(&world, "agent-a")
                .unwrap();
        assert!(check_fresh_session(&history, "agent-a", "bad-session").is_err());
        check_fresh_session(&history, "agent-a", "new-session").unwrap();
        assert!(check_fresh_session(&history, "agent-b", "bad-session").is_err());
    }
    #[test]
    fn preverified_metadata_collision_fences_both_sessions_without_outbox_fabrication() {
        let mut world = crate::runtime_host::world::new();
        enqueue(&mut world, "agent-a", "session-a", 1);
        enqueue(&mut world, "agent-a", "session-b", 1);
        let mut history =
            oasis7_world_service::projection::feedback_history_from_world(&world, "agent-a")
                .unwrap();
        history.records[1].feedback.feedback_id = history.records[0].feedback.feedback_id.clone();
        assert!(check_fresh_session(&history, "agent-a", "session-a").is_err());
        assert!(check_fresh_session(&history, "agent-a", "session-b").is_err());
    }
}
