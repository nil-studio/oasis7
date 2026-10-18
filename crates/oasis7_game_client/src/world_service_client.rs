//! Typed World Service intent and proof client. Backend-specific payload schemas remain owning profiles.
use crate::formal_read::FormalReadProjection;
use crate::remote_world_service::RemoteWorldServiceReader;
pub use crate::remote_world_service::{
    FormalReadTrustConfigV1, WorldServiceAgentSignerConfig, WorldServiceClientConfig,
};
use crate::verified_world_view::{ReadEvidencePolicy, VerifiedWorldView};
use crate::{WorldServiceClientError, WorldServiceQueryState, authority};
use oasis7_client_api::world_service::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
pub trait ClientWorldProfile: Clone + std::fmt::Debug {
    type Projection: FormalReadProjection;
    type Chat: Serialize + DeserializeOwned + Clone + PartialEq + std::fmt::Debug;
    type Scheduler: Serialize + DeserializeOwned + Clone + PartialEq + std::fmt::Debug;
    fn derive_correlation(
        world: WorldIdentity,
        payload: &Payload<Self>,
    ) -> Result<RequestCorrelation, String>;
    fn validate_captured_task_context(
        context: &oasis7_client_api::world_service::task_context::CapturedTaskContextV1<
            Self::Chat,
            Self::Scheduler,
        >,
        original: &SignedReadRequest<
            oasis7_client_api::world_service::task_context::TaskContextGetV1,
        >,
    ) -> Result<(), String>;
    fn validate_canonical_result(key: &str, value: &Value) -> Result<(), String>;
}
pub type Payload<H> = oasis7_client_api::world_service_wire::WorldServicePayloadV1<
    <H as ClientWorldProfile>::Chat,
    <H as ClientWorldProfile>::Scheduler,
>;
pub type AuthenticatedLookup<H> = oasis7_client_api::world_service_wire::AuthenticatedLookup<
    <H as ClientWorldProfile>::Chat,
    <H as ClientWorldProfile>::Scheduler,
>;
pub type CanonicalIntentResultV1<H> =
    oasis7_client_api::world_service_wire::CanonicalIntentResultV1<
        <H as ClientWorldProfile>::Chat,
        <H as ClientWorldProfile>::Scheduler,
    >;
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct FormalAuthenticatedLookupV1<C, S> {
    pub schema_version: u32,
    pub lookup: oasis7_client_api::world_service_wire::AuthenticatedLookup<C, S>,
    pub read: SignedReadRequest<ReadWorldViewRequest>,
}
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct FormalAuthenticatedChangesV1 {
    pub schema_version: u32,
    pub cursor: FormalBusinessChangesCursorV1,
    pub max_items: u32,
    pub max_bytes: u64,
    pub read: SignedReadRequest<ReadWorldViewRequest>,
}
#[path = "world_service_client_changes.rs"]
mod changes;
#[path = "world_service_client_operations.rs"]
mod operations;
pub trait WorldServicePort<H: ClientWorldProfile> {
    fn describe(&self) -> Result<DescribeWorldResponse, WorldServiceClientError>;
    fn submit(
        &self,
        request: SubmitIntentRequest<Payload<H>>,
    ) -> Result<SubmitObservation<Value>, WorldServiceClientError>;
    fn lookup(
        &self,
        request: LookupIntentRequest,
        original: Payload<H>,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError>;
    fn read_view(
        &self,
        request: ReadWorldViewRequest,
    ) -> Result<VerifiedWorldView<H::Projection>, WorldServiceClientError>;
    fn read_changes(
        &self,
        request: ReadWorldChangesRequest,
    ) -> Result<ReadWorldChangesResponse<Value>, WorldServiceClientError>;
}

#[derive(Clone)]
pub struct RemoteWorldServiceClient<H: ClientWorldProfile> {
    reader: RemoteWorldServiceReader<H::Projection>,
    marker: std::marker::PhantomData<H>,
}
impl<H: ClientWorldProfile> RemoteWorldServiceClient<H> {
    pub fn new(config: WorldServiceClientConfig) -> Result<Self, WorldServiceClientError> {
        Ok(Self {
            reader: RemoteWorldServiceReader::new(config)?,
            marker: std::marker::PhantomData,
        })
    }
    pub fn new_formal(
        config: WorldServiceClientConfig,
        trust: oasis7_formal_protocol::controlled_authority::formal::InitialActivationTrustV1,
        anchor: oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalReadAnchorV1,
    ) -> Result<Self, WorldServiceClientError> {
        Ok(Self {
            reader: RemoteWorldServiceReader::new_formal(config, trust, anchor)?,
            marker: std::marker::PhantomData,
        })
    }
    pub fn new_with_evidence_policy(
        config: WorldServiceClientConfig,
        policy: ReadEvidencePolicy,
    ) -> Result<Self, WorldServiceClientError> {
        Ok(Self {
            reader: RemoteWorldServiceReader::new_with_evidence_policy(config, policy)?,
            marker: std::marker::PhantomData,
        })
    }
    pub fn with_query_state(mut self, state: WorldServiceQueryState) -> Self {
        self.reader = self.reader.with_query_state(state);
        self
    }
    pub fn config(&self) -> &WorldServiceClientConfig {
        self.reader.config()
    }
    pub fn read_authority_identity(&self) -> Result<String, String> {
        self.reader.read_authority_identity()
    }
    fn call<Q: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        q: &Q,
    ) -> Result<R, WorldServiceClientError> {
        self.reader.transport.call(path, q)
    }
    pub fn read_actor_view(
        &self,
        signed: SignedReadRequest<ReadWorldViewRequest>,
    ) -> Result<VerifiedWorldView<H::Projection>, WorldServiceClientError> {
        self.reader.read_actor_view(signed)
    }
    pub fn read_owner_view(
        &self,
        signed: SignedReadRequest<ReadWorldViewRequest>,
    ) -> Result<VerifiedWorldView<H::Projection>, WorldServiceClientError> {
        self.reader.read_owner_view(signed)
    }
}
impl<H: ClientWorldProfile> WorldServicePort<H> for RemoteWorldServiceClient<H> {
    fn describe(&self) -> Result<DescribeWorldResponse, WorldServiceClientError> {
        self.reader.describe()
    }
    fn submit(
        &self,
        request: SubmitIntentRequest<Payload<H>>,
    ) -> Result<SubmitObservation<Value>, WorldServiceClientError> {
        if self.reader.formal_read.is_some() {
            return self.submit_formal(request);
        }
        self.reader.require_unproved_operation_profile(None)?;
        if request.correlation.key.world != self.reader.config.expected_world {
            return Err(WorldServiceClientError::Assurance(
                "intent world differs from configuration".into(),
            ));
        }
        request
            .validate()
            .map_err(|e| WorldServiceClientError::Assurance(e.to_string()))?;
        let correlation = request.correlation.clone();
        match self.call::<_, IntentResponse<Value>>(SUBMIT_PATH, &request) {
            Ok(response) => {
                if response.formal_profile.is_some() {
                    return Ok(SubmitObservation::OutcomeUnknown(correlation));
                }
                self.reader
                    .check_evidence_scope(response.execution_evidence_scope.as_deref())?;
                response
                    .validate(&correlation)
                    .map_err(|e| WorldServiceClientError::Assurance(e.to_string()))?;
                Ok(SubmitObservation::Response(Box::new(response)))
            }
            // A lost/malformed/untrusted response cannot establish rejection.
            Err(
                WorldServiceClientError::Transport(_)
                | WorldServiceClientError::Assurance(_)
                | WorldServiceClientError::Cooldown { .. },
            ) => Ok(SubmitObservation::OutcomeUnknown(correlation)),
            Err(error) => Err(error),
        }
    }
    fn lookup(
        &self,
        request: LookupIntentRequest,
        original: Payload<H>,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError> {
        if self.reader.formal_read.is_some() {
            return self.lookup_formal(request, original);
        }
        self.reader.require_unproved_operation_profile(None)?;
        request
            .validate()
            .map_err(|e| WorldServiceClientError::Assurance(e.to_string()))?;
        let expected = H::derive_correlation(self.reader.config.expected_world.clone(), &original)
            .map_err(WorldServiceClientError::Assurance)?;
        if expected.key != request.key {
            return Err(WorldServiceClientError::Assurance(
                "lookup key differs from signed payload".into(),
            ));
        }
        let response: IntentResponse<Value> =
            self.call(LOOKUP_PATH, &AuthenticatedLookup::<H> { request, original })?;
        self.reader
            .check_evidence_scope(response.execution_evidence_scope.as_deref())?;
        self.reader
            .require_unproved_operation_profile(response.formal_profile.as_ref())?;
        response
            .validate(&expected)
            .map_err(|e| WorldServiceClientError::Assurance(e.to_string()))?;
        Ok(response)
    }

    fn read_view(
        &self,
        request: ReadWorldViewRequest,
    ) -> Result<VerifiedWorldView<H::Projection>, WorldServiceClientError> {
        self.reader.read_view(request)
    }
    fn read_changes(
        &self,
        request: ReadWorldChangesRequest,
    ) -> Result<ReadWorldChangesResponse<Value>, WorldServiceClientError> {
        self.reader.read_changes(request)
    }
}

impl<H: ClientWorldProfile> std::ops::Deref for RemoteWorldServiceClient<H> {
    type Target = RemoteWorldServiceReader<H::Projection>;
    fn deref(&self) -> &Self::Target {
        &self.reader
    }
}

#[path = "world_service_client_private_goal.rs"]
mod private_goal;
#[path = "world_service_client_private_task.rs"]
mod private_task;

impl<H: ClientWorldProfile> RemoteWorldServiceClient<H> {
    /// Formal membership is read from the independently proved public projection;
    /// engineering membership retains its explicit signed diagnostic capability.
    pub fn read_membership(
        &self,
        request: ReadMembershipRequest,
    ) -> Result<crate::membership::AuthenticatedMembershipObservation, WorldServiceClientError>
    {
        if self.reader.formal_read.is_none() {
            request
                .validate()
                .map_err(|e| WorldServiceClientError::Assurance(e.to_string()))?;
        }
        if request.world != self.reader.config.expected_world
            || self.reader.config.scope_id != "public"
        {
            return Err(WorldServiceClientError::Assurance(
                "membership requires configured public world scope".into(),
            ));
        }
        if self.reader.formal_read.is_some() {
            let view = self.read_view(ReadWorldViewRequest {
                observation_nonce: None,
                contract_version: request.contract_version,
                formal_anchor: None,
                world: request.world.clone(),
                scope_id: "public".into(),
                min_commit: request.min_commit.clone(),
                fixed_commit: request.fixed_commit.clone(),
                deadline_unix_ms: None,
            })?;
            return crate::membership::AuthenticatedMembershipObservation::from_formal_view(
                &view, &request,
            )
            .map_err(WorldServiceClientError::Assurance);
        }
        let description = self.describe()?;
        if !description
            .capabilities
            .contains(&Capability::GovernanceMembership)
        {
            return Err(WorldServiceClientError::Assurance(
                "authenticated governance membership capability is unsupported".into(),
            ));
        }
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| {
            WorldServiceClientError::Configuration(
                "membership observation randomness is unavailable".into(),
            )
        })?;
        let envelope = MembershipReadEnvelope {
            query: request.clone(),
            observation_nonce: hex::encode(nonce),
        };
        let response: ReadMembershipResponse = self
            .reader
            .transport
            .signed_call("/v1/world/governance-membership", envelope)?;
        if response.execution_evidence_scope != description.execution_evidence_scope {
            return Err(WorldServiceClientError::Assurance(
                "membership scope differs from service description".into(),
            ));
        }
        crate::membership::AuthenticatedMembershipObservation::from_signed_transport(
            response,
            &request,
            self.reader.evidence_policy,
        )
        .map_err(WorldServiceClientError::Assurance)
    }
}
