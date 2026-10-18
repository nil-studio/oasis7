use crate::world_service_client::ClientWorldProfile;
use oasis7_client_api::world_service::*;
use serde_json::Value;
#[derive(Debug, Clone)]
pub(crate) struct TestProfile;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct TestProjection;
impl crate::formal_read::FormalReadProjection for TestProjection {
    fn validate_contents(
        response: &ReadWorldViewResponse<Self>,
        request: &ReadWorldViewRequest,
    ) -> Result<(), String> {
        response.validate(request).map_err(|e| e.to_string())
    }
}
impl ClientWorldProfile for TestProfile {
    type Projection = TestProjection;
    type Chat = oasis7_client_api::world_service_wire::UnsupportedWireVariant;
    type Scheduler = oasis7_client_api::world_service_wire::TaskSchedulerIntentV1;
    fn derive_correlation(
        world: WorldIdentity,
        payload: &crate::world_service_client::Payload<Self>,
    ) -> Result<RequestCorrelation, String> {
        oasis7_client_api::world_service_correlation::derive_correlation(world, payload)
    }
    fn validate_canonical_result(_key: &str, _value: &Value) -> Result<(), String> {
        Err("not used by transport fixture".into())
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
