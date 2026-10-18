//! Service-side task construction and Runtime action adapters.
use super::*;
use oasis7_agent_api::game::TaskContextIdentityV1;
pub use oasis7_client_api::world_service::task_context::{
    GoalExecutionSnapshotV1, MAX_PRIVATE_CONTEXT_BYTES, PreparedTaskContextV1,
    TASK_CONTEXT_LEASE_TICKS, TASK_CONTEXT_PREPARE_PATH, TASK_CONTEXT_READ_PATH,
    TASK_SNAPSHOT_PATH, TaskContextGetV1, TaskContextPrepareV1, TaskGoalSnapshotRequestV1,
    TaskPublicContextV1, cognition_intent, context_deadline, outcome_intent, public_outcome_intent,
    public_prefix, verify_get, verify_prepare, verify_private_reply, verify_snapshot_request,
};
#[cfg(unix)]
use zeroize::Zeroizing;
pub type CapturedTaskContextV1 =
    oasis7_client_api::world_service::task_context::CapturedTaskContextV1<
        oasis7_proto::viewer::AgentChatRequest,
        SchedulerIntentV1,
    >;
#[cfg(unix)]
pub(crate) fn create(
    original: SignedReadRequest<TaskContextPrepareV1>,
    snapshot: GoalExecutionSnapshotV1,
) -> Result<PreparedTaskContextV1, String> {
    verify_prepare(&original)?;
    snapshot.validate()?;
    let random_id = |prefix: &str| -> Result<String, String> {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|_| "private task randomness unavailable")?;
        Ok(format!("{prefix}-{}", hex::encode(bytes)))
    };
    let mut bytes = Zeroizing::new([0; 32]);
    getrandom::fill(&mut *bytes).map_err(|_| "private task randomness unavailable")?;
    let valid_until_tick = context_deadline(
        snapshot.captured_base_binding.base_tick,
        snapshot
            .provider_authority
            .capability_catalog
            .valid_until_tick,
    )?;
    let mut record = PreparedTaskContextV1 {
        schema_revision: 1,
        identity: TaskContextIdentityV1 {
            schema_revision: 1,
            fence: original.request.fence.clone(),
            valid_until_tick,
            agent_session_id: random_id("task-session")?,
            agent_turn_id: random_id("task-turn")?,
            decision_request_id: random_id("task-request")?,
            retry_seq: 1,
            transport_attempt: 1,
            request_commitment: format!("blake3:{}", "0".repeat(64)),
            context_commitment: format!("blake3:{}", "0".repeat(64)),
            provider_invocation_commitment: format!("blake3:{}", "0".repeat(64)),
        },
        prepare_proof: original,
        snapshot,
        reference_salt: hex::encode(bytes.as_slice()),
        valid_until_tick,
    };
    record.identity.request_commitment = record.commitment("oasis7/private-task-request/v1")?;
    record.identity.context_commitment = record.commitment("oasis7/private-task-context/v1")?;
    record.identity.provider_invocation_commitment =
        record.commitment("oasis7/private-task-invocation/v1")?;
    record.validate()?;
    Ok(record)
}
