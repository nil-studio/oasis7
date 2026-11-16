//! Validation of the explicitly normalized JSON projection profile. The exact
//! original JSON payload is authenticated before these read-only checks run.
use crate::WorldServiceClientError as Error;
use oasis7_agent_api::{game::GameTaskFenceV1, world_context::WorldServiceAgentContext};
pub use oasis7_client_api::world_service::task_projection::{
    AuthorizedFeedbackRecord, TaskWorldProjection,
};
use oasis7_client_api::world_service::*;
use serde_json::Value;

/// Service-authenticated scoped observation; not a World, task admission token,
/// durable receipt proof, or permission to execute a local world action.
#[derive(Debug, Clone)]
pub struct VerifiedTaskWorldView {
    response: ReadWorldViewResponse<Value>,
    pub(crate) world: WorldIdentity,
    read_authority: Option<String>,
    projection: TaskWorldProjection,
    task_fence: Option<GameTaskFenceV1>,
}
impl VerifiedTaskWorldView {
    pub(crate) fn new(
        response: ReadWorldViewResponse<Value>,
        request: &ReadWorldViewRequest,
    ) -> Result<Self, Error> {
        let checked = oasis7_client_api::world_service::task_projection::validate_task_projection(
            &response, request,
        )
        .map_err(Error::Assurance)?;
        Ok(Self {
            world: request.world.clone(),
            read_authority: None,
            response,
            projection: checked.projection,
            task_fence: checked.task_fence,
        })
    }
    pub(crate) fn bind_read_authority(mut self, identity: String) -> Self {
        self.read_authority = Some(identity);
        self
    }
    pub(crate) fn read_authority_matches(&self, identity: &str) -> bool {
        self.read_authority.as_deref() == Some(identity)
    }
    pub fn projection(&self) -> &TaskWorldProjection {
        &self.projection
    }
    pub fn raw_projection(&self) -> &Value {
        &self.response.view
    }
    pub fn version(&self) -> &ProjectionVersion {
        &self.response.version
    }
    pub fn continuation(&self) -> &EventCursor {
        &self.response.continuation
    }
    pub fn logical_tick(&self) -> u64 {
        self.response.logical_tick
    }
    pub fn agent_context(&self) -> Option<&WorldServiceAgentContext> {
        self.projection.agent_context.as_ref()
    }
    pub fn task_fence(&self) -> Option<&GameTaskFenceV1> {
        self.task_fence.as_ref()
    }
}

#[cfg(test)]
#[path = "verified_task_view_tests.rs"]
pub(crate) mod tests;
