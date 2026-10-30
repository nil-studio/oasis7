//! Run publication on the existing single IO lane, preserving any earlier remote result.
use super::*;
use crate::viewer::runtime_live::agent_service_io::{AgentServiceIoOperation, AgentServiceIoToken};
use crate::viewer::runtime_live::control_plane::llm_sidecar::lineage_persistence::checkpoint_write::PublicationBatch;
pub(super) enum Failure {
    Executor,
    Publication,
}
pub(super) fn publish(
    jobs: &mpsc::SyncSender<AgentServiceIoJob>,
    results: &mpsc::Receiver<AgentServiceIoResult>,
    returned: &mut Option<Result<AgentServiceIoResult, mpsc::TryRecvError>>,
    batch: PublicationBatch,
    ticket: u64,
) -> Result<(), Failure> {
    let token = AgentServiceIoToken {
        generation: ticket,
        config_digest: String::new(),
        phase_digest: "lineage-publication".into(),
    };
    // Called only after dropping Viewer guard. A transport already in flight
    // completes first on this same capacity-1 lane; it is not cancelled.
    jobs.send(AgentServiceIoJob {
        token: token.clone(),
        client: None,
        operation: AgentServiceIoOperation::PublishLineage(batch),
    })
    .map_err(|_| Failure::Executor)?;
    loop {
        let result = results.recv().map_err(|_| Failure::Executor)?;
        if result.token == token {
            return match result.response {
                Ok(AgentServiceIoResponse::LineagePublished) => Ok(()),
                _ => Err(Failure::Publication),
            };
        }
        if returned.is_some() {
            return Err(Failure::Executor);
        }
        // The one previously accepted transport result still needs its exact
        // normal apply path after the publication ACK. Never discard or apply
        // it under another phase or current configuration.
        *returned = Some(Ok(result));
    }
}
