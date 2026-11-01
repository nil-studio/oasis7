//! Two distinct mutation queues; no ACK is a canonical business result.
use super::*;
use oasis7_client_api::world_service::{
    task_cleanup_v2::*, task_mutation_admission_v2::*, task_stop_v2::*,
};
fn admit(
    service: &FormalLiveService,
    kind: TaskMutationKindV2,
    world: &WorldIdentity,
    original_digest: &str,
    raw: &[u8],
    until: u64,
) -> Result<TaskMutationAdmissionV2, String> {
    let _admission = service
        .admission_gate
        .lock()
        .map_err(|_| "Node admission gate poisoned")?;
    if !service.runtime_status().ready || world != &service.world {
        return Err("qualified Node/world unavailable".into());
    }
    let now = service.stop_record_checked_now()?;
    if now > until || (kind == TaskMutationKindV2::GuardedStop && now == until) {
        return Err("task mutation expired before queue admission".into());
    }
    let id = mutation_action_id_v2(kind, world, original_digest)?;
    service
        .runtime
        .lock()
        .map_err(|_| "Node runtime lock poisoned")?
        .submit_consensus_action_payload(id, raw.to_vec())
        .map_err(|e| format!("task mutation queue rejected: {e:?}"))?;
    Ok(TaskMutationAdmissionV2 {
        api_revision: 2,
        kind,
        world: world.clone(),
        original_digest: original_digest.into(),
        action_id: oasis7_agent_api::game::DecimalU64(id),
        admitted_at_ms: oasis7_agent_api::game::DecimalU64(now),
        outcome: TaskMutationQueuedOutcomeV2::QueuedVolatile,
    })
}
pub(super) fn handle_stop(service: &FormalLiveService, body: &[u8]) -> HttpReply {
    let q: TaskStopSubmitV2 = match parse_strict::<TaskStopSubmitV2>(body) {
        Ok(q) if body.len() <= 64 * 1024 => q,
        Ok(_) => {
            return raw_error(
                TASK_STOP_SUBMIT_V2,
                body,
                "invalid_request",
                "Stop body exceeds bound",
                &service.signer,
            );
        }
        Err(e) => {
            return raw_error(
                TASK_STOP_SUBMIT_V2,
                body,
                "invalid_request",
                &e,
                &service.signer,
            );
        }
    };
    let result = (|| {
        q.verify_owner_signature()?;
        let raw = correlation::encode_guarded_stop_consensus_v2(&q)?;
        admit(
            service,
            TaskMutationKindV2::GuardedStop,
            &q.expected_world,
            &q.original_digest()?,
            &raw,
            q.original.request.execute_until_ms.0,
        )
    })();
    match result {
        Ok(ack) => signed_reply(200, TASK_STOP_SUBMIT_V2, &q, &ack, &service.signer),
        Err(e) => raw_error(
            TASK_STOP_SUBMIT_V2,
            body,
            "mutation_not_queued",
            &e,
            &service.signer,
        ),
    }
}
pub(super) fn handle_close(service: &FormalLiveService, body: &[u8]) -> HttpReply {
    let q: TaskCleanupSubmitV2 = match parse_strict::<TaskCleanupSubmitV2>(body) {
        Ok(q) if body.len() <= 64 * 1024 => q,
        Ok(_) => {
            return raw_error(
                TASK_CLEANUP_SUBMIT_V2,
                body,
                "invalid_request",
                "Close body exceeds bound",
                &service.signer,
            );
        }
        Err(e) => {
            return raw_error(
                TASK_CLEANUP_SUBMIT_V2,
                body,
                "invalid_request",
                &e,
                &service.signer,
            );
        }
    };
    let result = (|| {
        q.verify_owner_signature()?;
        let raw = correlation::encode_cleanup_consensus_v2(&q)?;
        admit(
            service,
            TaskMutationKindV2::RevokeAndClose,
            &q.expected_world,
            &q.original_digest()?,
            &raw,
            q.original.request.cleanup_until_ms.0,
        )
    })();
    match result {
        Ok(ack) => signed_reply(200, TASK_CLEANUP_SUBMIT_V2, &q, &ack, &service.signer),
        Err(e) => raw_error(
            TASK_CLEANUP_SUBMIT_V2,
            body,
            "mutation_not_queued",
            &e,
            &service.signer,
        ),
    }
}
