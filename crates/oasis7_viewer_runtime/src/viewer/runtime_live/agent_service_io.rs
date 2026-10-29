//! Immutable transport capsules. Execution never borrows Viewer state.
use crate::world_service::{
    client::{RemoteWorldServiceClient, WorldServicePort},
    verified_view::VerifiedWorldView,
};
use oasis7_client_api::{legacy_world_service::WorldServicePayloadV1, world_service::*};
use serde_json::Value;

pub(super) enum AgentServiceProgress {
    Idle,
    Advanced,
    Blocked(AgentServiceBlocker),
    NeedsIo(Box<AgentServiceIoJob>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AgentServiceBlocker {
    HistoricalEvidenceUnavailable,
    OriginalIntentExpired,
}
impl AgentServiceBlocker {
    pub(super) fn reason(self) -> &'static str {
        match self {
            Self::HistoricalEvidenceUnavailable => {
                "Original canonical history is unavailable; restore its verified historical closure before reconciliation"
            }
            Self::OriginalIntentExpired => {
                "Original canonical intent expired; retained original requires explicit reconciliation"
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct AgentServiceIoToken {
    pub(super) generation: u64,
    pub(super) config_digest: String,
    pub(super) phase_digest: String,
}
pub(super) enum AgentServiceIoOperation {
    PublishLineage(
        super::control_plane::llm_sidecar::lineage_persistence::checkpoint_write::PublicationBatch,
    ),
    Submit(SubmitIntentRequest<WorldServicePayloadV1>),
    Lookup {
        request: LookupIntentRequest,
        original: WorldServicePayloadV1,
    },
    View(ReadWorldViewRequest),
    Feedback {
        client: crate::simulator::ProviderLoopbackHttpClient,
        payload: Value,
    },
    Metadata(crate::simulator::ProviderLoopbackHttpClient),
}
pub(super) struct AgentServiceIoJob {
    pub(super) token: AgentServiceIoToken,
    pub(super) client: Option<RemoteWorldServiceClient>,
    pub(super) operation: AgentServiceIoOperation,
}
pub(super) enum AgentServiceIoResponse {
    LineagePublished,
    Submit(SubmitObservation<Value>),
    Intent(IntentResponse<Value>),
    View(Box<VerifiedWorldView>),
    Feedback,
    Metadata(
        crate::simulator::ProviderInfo,
        crate::simulator::ProviderHealth,
    ),
}
pub(super) struct AgentServiceIoResult {
    pub(super) token: AgentServiceIoToken,
    pub(super) response: Result<AgentServiceIoResponse, String>,
}
impl AgentServiceIoJob {
    /// Only a not-yet-dispatched fresh admission Submit needs current Play
    /// eligibility. Lookup/reconciliation and accepted terminal work continue.
    pub(super) fn requires_fresh_dispatch_eligibility(&self) -> bool {
        if !matches!(
            self.token.phase_digest.as_str(),
            "admission:reserve" | "admission:prefix"
        ) {
            return false;
        }
        matches!(&self.operation,
            AgentServiceIoOperation::Submit(SubmitIntentRequest {
                signed_payload: WorldServicePayloadV1::Scheduler(signed), ..
            }) if matches!(&signed.request.operation,
                crate::world_service::wire::SchedulerOperationV1::ReserveLease(_)
                | crate::world_service::wire::SchedulerOperationV1::ProviderPrefix { .. }))
    }
    pub(super) fn execute(self) -> AgentServiceIoResult {
        let response = match self.operation {
            AgentServiceIoOperation::PublishLineage(batch) => batch
                .lock()
                .map_err(|_| "lineage in-flight batch poisoned".to_string())
                .and_then(|batch| {
                    batch
                        .iter()
                        .try_for_each(|publication| publication.execute())
                })
                .map(|()| AgentServiceIoResponse::LineagePublished),
            AgentServiceIoOperation::Feedback { client, payload } => client
                .submit_feedback_context_payload(&payload)
                .map_err(|e| e.to_string())
                .and_then(|ack| {
                    if ack.ok {
                        Ok(AgentServiceIoResponse::Feedback)
                    } else {
                        Err("provider rejected canonical feedback".into())
                    }
                }),
            AgentServiceIoOperation::Metadata(client) => client
                .provider_info()
                .and_then(|info| client.provider_health().map(|health| (info, health)))
                .map(|(info, health)| AgentServiceIoResponse::Metadata(info, health))
                .map_err(|error| error.to_string()),
            operation => match self.client {
                None => Err("canonical service transport configuration missing".into()),
                Some(client) => match operation {
                    AgentServiceIoOperation::Submit(request) => {
                        client.submit(request).map(AgentServiceIoResponse::Submit)
                    }
                    AgentServiceIoOperation::Lookup { request, original } => client
                        .lookup(request, original)
                        .map(AgentServiceIoResponse::Intent),
                    AgentServiceIoOperation::View(request) => client
                        .read_view(request)
                        .map(|view| AgentServiceIoResponse::View(Box::new(view))),
                    AgentServiceIoOperation::Metadata(_)
                    | AgentServiceIoOperation::PublishLineage(_)
                    | AgentServiceIoOperation::Feedback { .. } => unreachable!(),
                }
                .map_err(|error| error.to_string()),
            },
        };
        AgentServiceIoResult {
            token: self.token,
            response,
        }
    }
}
