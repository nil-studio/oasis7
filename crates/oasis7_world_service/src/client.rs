//! Execution-schema adapter for the independently owned Game Client.
use crate::verified_view::VerifiedWorldView;
use crate::wire::*;
use oasis7_client_api::world_service::*;
pub use oasis7_game_client::world_service_client::{
    FormalReadTrustConfigV1, WorldServiceAgentSignerConfig, WorldServiceClientConfig,
};
pub use oasis7_game_client::{WorldServiceClientError, WorldServiceQueryState};
use serde_json::Value;
/// Compatibility projection adapter. Contract, correlation and validation are
/// owned by API/Game Client; server projection is retained for local World callers.
pub type ServiceClientProfile = oasis7_game_client::legacy_profile::LegacyClientProfile<
    crate::projection::WorldServiceProjection,
>;
pub type RemoteWorldServiceClient =
    oasis7_game_client::world_service_client::RemoteWorldServiceClient<ServiceClientProfile>;
pub trait WorldServicePort {
    fn describe(&self) -> Result<DescribeWorldResponse, WorldServiceClientError>;
    fn submit(
        &self,
        request: SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<SubmitObservation<Value>, WorldServiceClientError>;
    fn lookup(
        &self,
        request: LookupIntentRequest,
        original: WorldServicePayloadV1,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError>;
    fn read_view(
        &self,
        request: ReadWorldViewRequest,
    ) -> Result<VerifiedWorldView, WorldServiceClientError>;
    fn read_changes(
        &self,
        request: ReadWorldChangesRequest,
    ) -> Result<ReadWorldChangesResponse<Value>, WorldServiceClientError>;
}

impl WorldServicePort for RemoteWorldServiceClient {
    fn describe(&self) -> Result<DescribeWorldResponse, WorldServiceClientError> {
        oasis7_game_client::world_service_client::WorldServicePort::<ServiceClientProfile>::describe(
            self,
        )
    }
    fn submit(
        &self,
        request: SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<SubmitObservation<Value>, WorldServiceClientError> {
        oasis7_game_client::world_service_client::WorldServicePort::<ServiceClientProfile>::submit(
            self, request,
        )
    }
    fn lookup(
        &self,
        request: LookupIntentRequest,
        original: WorldServicePayloadV1,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError> {
        oasis7_game_client::world_service_client::WorldServicePort::<ServiceClientProfile>::lookup(
            self, request, original,
        )
    }
    fn read_view(
        &self,
        request: ReadWorldViewRequest,
    ) -> Result<VerifiedWorldView, WorldServiceClientError> {
        oasis7_game_client::world_service_client::WorldServicePort::<ServiceClientProfile>::read_view(self,request)
    }
    fn read_changes(
        &self,
        request: ReadWorldChangesRequest,
    ) -> Result<ReadWorldChangesResponse<Value>, WorldServiceClientError> {
        oasis7_game_client::world_service_client::WorldServicePort::<ServiceClientProfile>::read_changes(self,request)
    }
}

#[cfg(test)]
#[path = "client_cooldown_tests.rs"]
mod cooldown_tests;
#[cfg(test)]
#[path = "client_formal_operation_tests.rs"]
mod formal_operation_tests;
