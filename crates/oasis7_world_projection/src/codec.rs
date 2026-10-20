//! Concrete observation codecs. Parsing alone does not authenticate a view.
use crate::WorldServiceProjection;
use oasis7_client_api::world_service::{FormalBusinessChangesPageV1, ReadWorldViewResponse};

pub type WorldViewResponse = ReadWorldViewResponse<WorldServiceProjection>;
pub type ChangesPage = FormalBusinessChangesPageV1<WorldServiceProjection>;

pub fn projection_to_json(value: &WorldServiceProjection) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}
pub fn projection_to_json_value(
    value: &WorldServiceProjection,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(value)
}
pub fn projection_from_json(bytes: &[u8]) -> Result<WorldServiceProjection, serde_json::Error> {
    serde_json::from_slice(bytes)
}
pub fn projection_from_json_value(
    value: serde_json::Value,
) -> Result<WorldServiceProjection, serde_json::Error> {
    serde_json::from_value(value)
}
pub fn projection_to_cbor(value: &WorldServiceProjection) -> Result<Vec<u8>, serde_cbor::Error> {
    serde_cbor::to_vec(value)
}
pub fn projection_from_cbor(bytes: &[u8]) -> Result<WorldServiceProjection, serde_cbor::Error> {
    serde_cbor::from_slice(bytes)
}
pub fn response_to_json(value: &WorldViewResponse) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}
pub fn response_to_json_value(
    value: &WorldViewResponse,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(value)
}
pub fn response_from_json(bytes: &[u8]) -> Result<WorldViewResponse, serde_json::Error> {
    serde_json::from_slice(bytes)
}
pub fn response_to_cbor(value: &WorldViewResponse) -> Result<Vec<u8>, serde_cbor::Error> {
    serde_cbor::to_vec(value)
}
pub fn response_from_cbor(bytes: &[u8]) -> Result<WorldViewResponse, serde_cbor::Error> {
    serde_cbor::from_slice(bytes)
}
pub fn projection_leaf(
    audience: &oasis7_client_api::formal_projection::ProjectionAudienceV1,
    projection: &WorldServiceProjection,
) -> Result<String, String> {
    oasis7_client_api::formal_projection::projection_leaf(audience, projection)
}

/// Encode the concrete Changes page without changing its wire envelope.
pub fn changes_page_json(value: &ChangesPage) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}
pub fn changes_page_from_json(bytes: &[u8]) -> Result<ChangesPage, serde_json::Error> {
    serde_json::from_slice(bytes)
}

/// Preserve the projection revision domain while owning its concrete encoding.
pub fn projection_revision(value: &WorldServiceProjection) -> Result<String, String> {
    oasis7_client_api::world_service::request_digest("projection", value)
}
