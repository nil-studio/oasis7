//! Independently consumable complete legacy profile. Runtime admission remains authoritative.
use crate::world_service_client::ClientWorldProfile;
use oasis7_client_api::{legacy_world_service as contract, world_service::*};
#[derive(Debug, Clone)]
pub struct LegacyClientProfile<P = crate::read_projection::WorldReadProjectionV1>(
    std::marker::PhantomData<P>,
);
impl<P: crate::formal_read::FormalReadProjection + Clone + std::fmt::Debug> ClientWorldProfile
    for LegacyClientProfile<P>
{
    type Projection = P;
    type Chat = oasis7_proto::viewer::AgentChatRequest;
    type Scheduler = contract::SchedulerIntentV1;
    fn derive_correlation(
        world: WorldIdentity,
        payload: &contract::WorldServicePayloadV1,
    ) -> Result<RequestCorrelation, String> {
        contract::derive_correlation(world, payload)
    }
    fn validate_canonical_result(key: &str, value: &serde_json::Value) -> Result<(), String> {
        contract::validate_canonical_result(key, value)
    }
    fn validate_captured_task_context(
        context: &oasis7_client_api::world_service::task_context::CapturedTaskContextV1<
            Self::Chat,
            Self::Scheduler,
        >,
        original: &SignedReadRequest<
            oasis7_client_api::world_service::task_context::TaskContextGetV1,
        >,
    ) -> Result<(), String> {
        context.validate_against(original)
    }
}
pub type LegacyWorldServiceClient =
    crate::world_service_client::RemoteWorldServiceClient<LegacyClientProfile>;
