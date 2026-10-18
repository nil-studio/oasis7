//! Typed native-task client. World admission remains a remote service action.
use crate::{
    WorldServiceClientConfig, WorldServiceClientError as Error, WorldServiceQueryState,
    WorldServiceTransport,
};
use oasis7_agent_api::game::GameTaskFenceV1;
use oasis7_client_api::world_service::task_context::*;
use oasis7_client_api::world_service::*;
use oasis7_client_api::world_service_correlation::derive_correlation;
use oasis7_client_api::world_service_wire::*;
use serde_json::Value;

#[derive(Clone)]
pub struct WorldServiceTaskClient {
    transport: WorldServiceTransport,
}
fn assurance(error: impl ToString) -> Error {
    Error::Assurance(error.to_string())
}
fn ordinary(scope: Option<&str>) -> Result<(), Error> {
    if scope.is_some() {
        return Err(assurance("task client requires ordinary service evidence"));
    }
    Ok(())
}
impl WorldServiceTaskClient {
    pub fn new(config: WorldServiceClientConfig) -> Result<Self, Error> {
        Ok(Self {
            transport: WorldServiceTransport::new(config)?,
        })
    }
    pub fn with_query_state(mut self, state: WorldServiceQueryState) -> Self {
        self.transport = self.transport.with_query_state(state);
        self
    }
    pub(crate) fn query_state(&self) -> WorldServiceQueryState {
        self.transport.query_state.clone()
    }
    pub fn config(&self) -> &WorldServiceClientConfig {
        self.transport.config()
    }
    pub fn read_authority_identity(&self) -> Result<String, String> {
        self.config().read_authority_identity()
    }
    /// Read the explicit normalized JSON profile. This service assertion never
    /// supplies independent consensus or task admission proof.
    pub fn read_view(
        &self,
        request: ReadWorldViewRequest,
    ) -> Result<crate::VerifiedTaskWorldView, Error> {
        request.validate().map_err(assurance)?;
        if request.world != self.config().expected_world
            || request.scope_id != self.config().scope_id
        {
            return Err(assurance("read scope/world differs from configuration"));
        }
        let response: ReadWorldViewResponse<Value> = self
            .transport
            .signed_call(VIEW_JSON_PATH, request.clone())?;
        Ok(crate::VerifiedTaskWorldView::new(response, &request)?
            .bind_read_authority(self.read_authority_identity().map_err(assurance)?))
    }
    pub fn describe(&self) -> Result<DescribeWorldResponse, Error> {
        let response: DescribeWorldResponse = self.transport.signed_call(
            DESCRIBE_PATH,
            DescribeWorldRequest {
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                expected_world: self.config().expected_world.clone(),
                trust_config_ref: self.config().trusted_service_public_key.clone(),
            },
        )?;
        ordinary(response.execution_evidence_scope.as_deref())?;
        response
            .validate(&self.config().expected_world)
            .map_err(assurance)?;
        Ok(response)
    }
    pub fn submit(
        &self,
        request: SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<SubmitObservation<Value>, Error> {
        request.validate().map_err(assurance)?;
        let expected = derive_correlation(
            self.config().expected_world.clone(),
            &request.signed_payload,
        )
        .map_err(assurance)?;
        if expected != request.correlation {
            return Err(assurance("intent correlation differs from signed payload"));
        }
        match self
            .transport
            .call::<_, IntentResponse<Value>>(SUBMIT_PATH, &request)
        {
            Ok(response) => {
                ordinary(response.execution_evidence_scope.as_deref())?;
                response.validate(&expected).map_err(assurance)?;
                Ok(SubmitObservation::Response(Box::new(response)))
            }
            Err(Error::Transport(_) | Error::Assurance(_) | Error::Cooldown { .. }) => {
                Ok(SubmitObservation::OutcomeUnknown(expected))
            }
            Err(error) => Err(error),
        }
    }
    pub fn lookup(
        &self,
        request: LookupIntentRequest,
        original: WorldServicePayloadV1,
    ) -> Result<IntentResponse<Value>, Error> {
        request.validate().map_err(assurance)?;
        let expected = derive_correlation(self.config().expected_world.clone(), &original)
            .map_err(assurance)?;
        if expected.key != request.key {
            return Err(assurance("lookup key differs from signed payload"));
        }
        let response: IntentResponse<Value> = self
            .transport
            .call(LOOKUP_PATH, &AuthenticatedLookup { request, original })?;
        ordinary(response.execution_evidence_scope.as_deref())?;
        response.validate(&expected).map_err(assurance)?;
        Ok(response)
    }
    pub fn read_changes(
        &self,
        request: ReadWorldChangesRequest,
    ) -> Result<ReadWorldChangesResponse<Value>, Error> {
        request.validate().map_err(assurance)?;
        if request.cursor.commit.world != self.config().expected_world
            || request.cursor.scope_id != self.config().scope_id
        {
            return Err(assurance("changes scope/world differs from configuration"));
        }
        let response: ReadWorldChangesResponse<Value> =
            self.transport.signed_call(CHANGES_PATH, request.clone())?;
        ordinary(response.execution_evidence_scope.as_deref())?;
        response.validate(&request).map_err(assurance)?;
        if serde_json::to_vec(&response).map_err(assurance)?.len() as u64 > request.max_bytes {
            return Err(assurance("changes byte limit exceeded"));
        }
        Ok(response)
    }
    fn task_scope(&self, fence: &GameTaskFenceV1) -> Result<(), Error> {
        self.private_scope(
            &WorldIdentity {
                world_id: fence.world_id.clone(),
                genesis_digest: fence.genesis_digest.clone(),
            },
            &fence.agent_id,
        )
    }
    fn private_scope(&self, world: &WorldIdentity, agent: &str) -> Result<(), Error> {
        if world != &self.config().expected_world
            || self.config().scope_id != format!("agent:{agent}")
        {
            return Err(Error::Configuration(
                "private goal world or agent differs from configuration".into(),
            ));
        }
        Ok(())
    }
    pub fn prepare_task_context(
        &self,
        original: &SignedReadRequest<TaskContextPrepareV1>,
    ) -> Result<PreparedTaskContextV1, Error> {
        verify_prepare(original).map_err(assurance)?;
        self.task_scope(&original.request.fence)?;
        let response: SignedServiceResponse<PreparedTaskContextV1> =
            self.transport.private_signed_response(
                TASK_CONTEXT_PREPARE_PATH,
                original,
                MAX_PRIVATE_CONTEXT_BYTES,
            )?;
        verify_private_reply(
            TASK_CONTEXT_PREPARE_PATH,
            original,
            &response,
            &self.config().trusted_service_public_key,
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
    ) -> Result<CapturedTaskContextV1, Error> {
        verify_get(original).map_err(assurance)?;
        self.task_scope(&original.request.context.fence)?;
        let response: SignedServiceResponse<CapturedTaskContextV1> = self
            .transport
            .private_signed_response(TASK_CONTEXT_READ_PATH, original, MAX_PRIVATE_CONTEXT_BYTES)?;
        verify_private_reply(
            TASK_CONTEXT_READ_PATH,
            original,
            &response,
            &self.config().trusted_service_public_key,
        )
        .map_err(assurance)?;
        response
            .payload
            .validate_against(original)
            .map_err(assurance)?;
        Ok(response.payload)
    }
    pub fn read_task_goal_snapshot(
        &self,
        original: &SignedReadRequest<TaskGoalSnapshotRequestV1>,
    ) -> Result<GoalExecutionSnapshotV1, Error> {
        verify_snapshot_request(original).map_err(assurance)?;
        self.task_scope(&original.request.fence)?;
        let response: SignedServiceResponse<GoalExecutionSnapshotV1> = self
            .transport
            .private_signed_response(TASK_SNAPSHOT_PATH, original, MAX_PRIVATE_CONTEXT_BYTES)?;
        verify_private_reply(
            TASK_SNAPSHOT_PATH,
            original,
            &response,
            &self.config().trusted_service_public_key,
        )
        .map_err(assurance)?;
        response.payload.validate().map_err(assurance)?;
        if GameTaskFenceV1::from_control(&response.payload.control) != original.request.fence {
            return Err(assurance("snapshot fence differs"));
        }
        Ok(response.payload)
    }
}

use oasis7_client_api::world_service::private_goal::{PreparedGoalReceiptV1, SignedGoalPrepareV1};
use oasis7_client_api::world_service::private_goal_read::*;
impl WorldServiceTaskClient {
    /// Retain and verify the exact original proof. Does not sign or submit a public intent.
    pub fn prepare_goal(
        &self,
        original: &SignedGoalPrepareV1,
    ) -> Result<PreparedGoalReceiptV1, Error> {
        original
            .verify()
            .map_err(|_| Error::Assurance("invalid private goal proof".into()))?;
        self.private_scope(
            &WorldIdentity {
                world_id: original.request.world_id.clone(),
                genesis_digest: original.request.genesis_digest.clone(),
            },
            &original.request.agent_id,
        )?;
        let response =
            self.transport
                .private_signed_response(GOAL_PREPARE_PATH, original, 64 * 1024)?;
        verify_prepare_response(
            original,
            &response,
            &self.config().trusted_service_public_key,
        )
        .map_err(|_| Error::Assurance("private prepare response not verified".into()))?;
        Ok(response.payload)
    }
    /// Owner-authorized full goal subset. This does not provide full TaskSnapshot/Context.
    pub fn read_current_goal(
        &self,
        request: &SignedReadRequest<CurrentGoalRequest>,
    ) -> Result<CurrentGoalResponse, Error> {
        verify_current_request(request)
            .map_err(|_| Error::Assurance("invalid current goal proof".into()))?;
        self.private_scope(&request.request.world, &request.request.agent_id)?;
        let response =
            self.transport
                .private_signed_response(GOAL_CURRENT_PATH, request, 64 * 1024)?;
        verify_current_response(
            request,
            &response,
            &self.config().trusted_service_public_key,
        )
        .map_err(|_| Error::Assurance("current private goal response not verified".into()))?;
        Ok(response.payload)
    }
}

#[cfg(test)]
#[path = "task_client_tests.rs"]
mod tests;
impl WorldServiceTaskClient {
    pub fn quote_task_recipe(
        &self,
        original: &SignedReadRequest<
            oasis7_client_api::world_service::task_recipe_quote::TaskRecipeQuoteRequestV1,
        >,
    ) -> Result<oasis7_client_api::world_service::task_recipe_quote::TaskRecipeQuoteV1, Error> {
        use oasis7_client_api::world_service::task_recipe_quote::*;
        verify_recipe_quote_request(original).map_err(assurance)?;
        self.task_scope(&original.request.context_proof.request.context.fence)?;
        let response: SignedServiceResponse<TaskRecipeQuoteV1> = self
            .transport
            .private_signed_response(TASK_RECIPE_QUOTE_PATH, original, MAX_PRIVATE_CONTEXT_BYTES)?;
        verify_private_reply(
            TASK_RECIPE_QUOTE_PATH,
            original,
            &response,
            &self.config().trusted_service_public_key,
        )
        .map_err(assurance)?;
        response
            .payload
            .validate_against(original)
            .map_err(assurance)?;
        Ok(response.payload)
    }
}

impl WorldServiceTaskClient {
    pub fn quote_task_factory_outcome(
        &self,
        original: &SignedReadRequest<
            oasis7_client_api::world_service::task_factory_quote::TaskFactoryQuoteRequestV1,
        >,
    ) -> Result<
        oasis7_client_api::world_service::task_factory_quote::TaskFactoryQuoteResponseV1,
        Error,
    > {
        use oasis7_client_api::world_service::task_factory_quote::*;
        verify_factory_quote_request(original).map_err(assurance)?;
        self.task_scope(&original.request.context_proof.request.context.fence)?;
        let response: SignedServiceResponse<TaskFactoryQuoteResponseV1> =
            self.transport.private_signed_response(
                TASK_FACTORY_QUOTE_PATH,
                original,
                MAX_PRIVATE_CONTEXT_BYTES,
            )?;
        verify_private_reply(
            TASK_FACTORY_QUOTE_PATH,
            original,
            &response,
            &self.config().trusted_service_public_key,
        )
        .map_err(assurance)?;
        response
            .payload
            .validate_against(original)
            .map_err(assurance)?;
        Ok(response.payload)
    }
}

impl WorldServiceTaskClient {
    /// Compatibility API remains fail-closed for callers without denial handling.
    pub fn quote_task_factory(
        &self,
        original: &SignedReadRequest<
            oasis7_client_api::world_service::task_factory_quote::TaskFactoryQuoteRequestV1,
        >,
    ) -> Result<oasis7_client_api::world_service::task_factory_quote::TaskFactoryQuoteV1, Error>
    {
        use oasis7_client_api::world_service::task_factory_quote::TaskFactoryQuoteResponseV1;
        match self.quote_task_factory_outcome(original)? {
            TaskFactoryQuoteResponseV1::Quote(quote) => Ok(*quote),
            TaskFactoryQuoteResponseV1::Denied(_) => {
                Err(assurance("factory selection denied; no action submitted"))
            }
        }
    }
}
