//! Confidential task input over the bounded private transport.
use super::*;
use oasis7_agent_api::game::GameTaskFenceV1;
use oasis7_client_api::world_service::task_context::*;
fn assurance(_: String) -> WorldServiceClientError {
    WorldServiceClientError::Assurance("private task response not verified".into())
}
impl<H: ClientWorldProfile> RemoteWorldServiceClient<H> {
    fn task_scope(&self, fence: &GameTaskFenceV1) -> Result<(), WorldServiceClientError> {
        self.private_scope(
            &WorldIdentity {
                world_id: fence.world_id.clone(),
                genesis_digest: fence.genesis_digest.clone(),
            },
            &fence.agent_id,
        )
    }
    pub fn prepare_task_context(
        &self,
        original: &SignedReadRequest<TaskContextPrepareV1>,
    ) -> Result<PreparedTaskContextV1, WorldServiceClientError> {
        verify_prepare(original).map_err(assurance)?;
        self.task_scope(&original.request.fence)?;
        let response: SignedServiceResponse<PreparedTaskContextV1> = self.private_call_bounded(
            TASK_CONTEXT_PREPARE_PATH,
            original,
            MAX_PRIVATE_CONTEXT_BYTES,
        )?;
        verify_private_reply(
            TASK_CONTEXT_PREPARE_PATH,
            original,
            &response,
            &self.reader.config.trusted_service_public_key,
        )
        .map_err(assurance)?;
        response
            .payload
            .validate_against(original)
            .map_err(assurance)?;
        Ok(response.payload)
    }
    pub fn read_task_context(
        &self,
        original: &SignedReadRequest<TaskContextGetV1>,
    ) -> Result<CapturedTaskContextV1<H::Chat, H::Scheduler>, WorldServiceClientError> {
        verify_get(original).map_err(assurance)?;
        self.task_scope(&original.request.context.fence)?;
        let response: SignedServiceResponse<CapturedTaskContextV1<H::Chat, H::Scheduler>> =
            self.private_call_bounded(TASK_CONTEXT_READ_PATH, original, MAX_PRIVATE_CONTEXT_BYTES)?;
        verify_private_reply(
            TASK_CONTEXT_READ_PATH,
            original,
            &response,
            &self.reader.config.trusted_service_public_key,
        )
        .map_err(assurance)?;
        H::validate_captured_task_context(&response.payload, original).map_err(assurance)?;
        Ok(response.payload)
    }
    pub fn read_task_goal_snapshot(
        &self,
        original: &SignedReadRequest<TaskGoalSnapshotRequestV1>,
    ) -> Result<GoalExecutionSnapshotV1, WorldServiceClientError> {
        verify_snapshot_request(original).map_err(assurance)?;
        self.task_scope(&original.request.fence)?;
        let response: SignedServiceResponse<GoalExecutionSnapshotV1> =
            self.private_call_bounded(TASK_SNAPSHOT_PATH, original, MAX_PRIVATE_CONTEXT_BYTES)?;
        verify_private_reply(
            TASK_SNAPSHOT_PATH,
            original,
            &response,
            &self.reader.config.trusted_service_public_key,
        )
        .map_err(assurance)?;
        response.payload.validate().map_err(assurance)?;
        if GameTaskFenceV1::from_control(&response.payload.control) != original.request.fence {
            return Err(assurance("snapshot fence differs".into()));
        }
        Ok(response.payload)
    }
}

impl<H: ClientWorldProfile> RemoteWorldServiceClient<H> {
    pub fn quote_task_recipe(
        &self,
        original: &SignedReadRequest<
            oasis7_client_api::world_service::task_recipe_quote::TaskRecipeQuoteRequestV1,
        >,
    ) -> Result<
        oasis7_client_api::world_service::task_recipe_quote::TaskRecipeQuoteV1,
        WorldServiceClientError,
    > {
        use oasis7_client_api::world_service::task_recipe_quote::*;
        verify_recipe_quote_request(original).map_err(assurance)?;
        self.task_scope(&original.request.context_proof.request.context.fence)?;
        let response: SignedServiceResponse<TaskRecipeQuoteV1> =
            self.private_call_bounded(TASK_RECIPE_QUOTE_PATH, original, MAX_PRIVATE_CONTEXT_BYTES)?;
        verify_private_reply(
            TASK_RECIPE_QUOTE_PATH,
            original,
            &response,
            &self.reader.config.trusted_service_public_key,
        )
        .map_err(assurance)?;
        response
            .payload
            .validate_against(original)
            .map_err(assurance)?;
        Ok(response.payload)
    }
}

impl<H: ClientWorldProfile> RemoteWorldServiceClient<H> {
    pub fn quote_task_factory(
        &self,
        original: &SignedReadRequest<
            oasis7_client_api::world_service::task_factory_quote::TaskFactoryQuoteRequestV1,
        >,
    ) -> Result<
        oasis7_client_api::world_service::task_factory_quote::TaskFactoryQuoteV1,
        WorldServiceClientError,
    > {
        use oasis7_client_api::world_service::task_factory_quote::*;
        verify_factory_quote_request(original).map_err(assurance)?;
        self.task_scope(&original.request.context_proof.request.context.fence)?;
        let response: SignedServiceResponse<TaskFactoryQuoteV1> = self.private_call_bounded(
            TASK_FACTORY_QUOTE_PATH,
            original,
            MAX_PRIVATE_CONTEXT_BYTES,
        )?;
        verify_private_reply(
            TASK_FACTORY_QUOTE_PATH,
            original,
            &response,
            &self.reader.config.trusted_service_public_key,
        )
        .map_err(assurance)?;
        response
            .payload
            .validate_against(original)
            .map_err(assurance)?;
        Ok(response.payload)
    }
}
