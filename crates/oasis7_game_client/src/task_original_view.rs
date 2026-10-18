//! Original signed projection reads for native task consumers. The original
//! encoding authenticates the view; only the explicit DTO supplies read values.
use crate::{formal_read::FormalReadProjection, read_projection::WorldReadProjectionV1};
use oasis7_client_api::{world_read_observation::WorldReadObservationV1, world_service::*};
use serde::{Deserialize, Serialize};

/// Unverified original projection plus its narrow read schema. Only a verified
/// view returned by the configured reader authenticates these read values.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OriginalTaskReadProjectionV1(WorldReadProjectionV1);
impl OriginalTaskReadProjectionV1 {
    pub fn observation(&self) -> &WorldReadObservationV1 {
        self.0.observation()
    }
}
impl FormalReadProjection for OriginalTaskReadProjectionV1 {
    fn read_metadata(
        &self,
    ) -> Result<oasis7_client_api::world_service_read::WorldServiceProjectionMetadataV1, String>
    {
        self.0.read_metadata()
    }
    fn validate_contents(
        response: &ReadWorldViewResponse<Self>,
        request: &ReadWorldViewRequest,
    ) -> Result<(), String> {
        // Retain every existing task read check (owner metadata, task fence,
        // feedback identities/lineage and scope) before exposing the new DTO.
        // This mapped JSON value never substitutes for the signed original.
        let mapped =
            serde_json::from_value(serde_json::to_value(response).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        crate::VerifiedTaskWorldView::new(mapped, request).map_err(|error| match error {
            crate::WorldServiceClientError::Assurance(message) => message,
            other => other.to_string(),
        })?;
        response.validate(request).map_err(|e| e.to_string())?;
        response.view.observation().validate_bound_contents(
            &response.version,
            response.logical_tick,
            request,
        )
    }
}
pub type VerifiedOriginalTaskWorldView =
    crate::verified_world_view::VerifiedWorldView<OriginalTaskReadProjectionV1>;

impl crate::task_client::WorldServiceTaskClient {
    /// Read the original `/view` profile with its canonical signature codec.
    /// This preserves ordinary service assurance; it grants no local admission
    /// authority and performs the same scoped task checks as the JSON profile.
    pub fn read_original_view(
        &self,
        request: ReadWorldViewRequest,
    ) -> Result<VerifiedOriginalTaskWorldView, crate::WorldServiceClientError> {
        let base = self.config();
        let reader = crate::remote_world_service::RemoteWorldServiceReader::<
            OriginalTaskReadProjectionV1,
        >::new(crate::remote_world_service::WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint: base.endpoint.clone(),
            trusted_service_public_key: base.trusted_service_public_key.clone(),
            expected_world: base.expected_world.clone(),
            scope_id: base.scope_id.clone(),
            read_private_key_hex: base.read_private_key_hex.clone(),
            timeout: base.timeout,
            max_response_bytes: base.max_response_bytes,
        })?
        .with_query_state(self.query_state());
        reader.read_view(request)
    }
}

#[cfg(test)]
#[path = "task_original_view_tests.rs"]
mod tests;
