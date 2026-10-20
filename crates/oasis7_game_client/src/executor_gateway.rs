//! Trusted local transport adapter. No owner authority or raw-key API.
use crate::identity_client::{IdentityClient, IdentityClientConfig};
use crate::{WorldServiceClientConfig, WorldServiceClientError as Error};
use ed25519_dalek::SigningKey;
use oasis7_agent_api::game::{GameTaskFenceV1, GameTaskStatusV1};
use oasis7_client_api::world_service::{
    self as ws, entry_executor::*, task_context::*, task_factory_quote::*, task_recipe_quote::*, *,
};
use oasis7_client_api::world_service_wire::*;
use oasis7_client_storage::identity_store::ExecutorControlStore;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "executor_gateway_tests.rs"]
mod tests;

/// A fixed task resource and its own vault. Construct only inside a trusted
/// adapter; the model receives game primitives, never this object or unlocks.
pub struct ExecutorGatewayContext {
    identity: IdentityClient,
    identity_origin: String,
    login_operation: String,
    store: ExecutorControlStore,
    key: SigningKey,
    fence: GameTaskFenceV1,
    trusted_service_key: String,
    scope: String,
    resource_scope: String,
    max_response_bytes: usize,
}

fn configuration(e: impl ToString) -> Error {
    Error::Configuration(e.to_string())
}
fn assurance(e: impl ToString) -> Error {
    Error::Assurance(e.to_string())
}

impl ExecutorGatewayContext {
    /// Opens an existing explicitly created executor vault. Missing, corrupt,
    /// wrong-password or cross-resource material fails closed without new keys.
    pub fn open(
        identity: IdentityClientConfig,
        login_operation: String,
        executor_directory: PathBuf,
        private_unlock: &[u8],
        fence: GameTaskFenceV1,
        trusted_service_key: String,
        max_response_bytes: usize,
    ) -> Result<Self, Error> {
        fence.validate().map_err(configuration)?;
        if fence.task_status != GameTaskStatusV1::Active
            || max_response_bytes == 0
            || max_response_bytes > 2 * 1024 * 1024
            || login_operation.trim().is_empty()
            || login_operation.len() > 256
        {
            return Err(configuration("invalid executor gateway resource"));
        }
        independent_directory(&executor_directory, &identity.state_directory)?;
        let trusted: [u8; 32] = hex::decode(&trusted_service_key)
            .map_err(|_| configuration("invalid executor service key"))?
            .try_into()
            .map_err(|_| configuration("invalid executor service key"))?;
        if trusted_service_key != hex::encode(trusted)
            || ed25519_dalek::VerifyingKey::from_bytes(&trusted)
                .map_err(configuration)?
                .is_weak()
        {
            return Err(configuration("invalid executor service trust"));
        }
        let store = ExecutorControlStore::open(&executor_directory, private_unlock)
            .map_err(configuration)?;
        let key = SigningKey::from_bytes(store.executor_seed());
        let public = hex::encode(key.verifying_key().to_bytes());
        let binding = fence
            .executor_binding
            .as_ref()
            .ok_or_else(|| configuration("executor binding unavailable"))?;
        if binding.executor_public_key != public || public == fence.owner_public_key {
            return Err(configuration(
                "executor vault differs from independent canonical executor",
            ));
        }
        let client =
            IdentityClient::new(identity.clone(), private_unlock).map_err(configuration)?;
        if public == client.public_key() {
            return Err(configuration("executor key must differ from login key"));
        }
        // A vault cannot be silently reused for owned A and delegated B, nor
        // across worlds/tasks/owner generations. New explicit task fences for
        // this same resource remain possible after reconciliation.
        let resource = (
            &identity.endpoint,
            &identity.identity_service_id,
            &identity.environment_id,
            &identity.audience,
            &fence.world_id,
            &fence.genesis_digest,
            &fence.agent_id,
            &fence.task_id,
            fence.owner_generation,
            &public,
            &trusted_service_key,
        );
        let binding_digest =
            ws::request_digest("executor-vault-resource/v1", &resource).map_err(configuration)?;
        let saved: String = store
            .put_original(
                "executor-vault-resource/v1",
                "binding",
                "resource",
                &binding_digest,
            )
            .map_err(configuration)?;
        if saved != binding_digest {
            return Err(configuration(
                "executor vault belongs to another task resource",
            ));
        }
        let scope = ws::request_digest("executor-gateway-context/v1", &(&binding_digest, &fence))
            .map_err(configuration)?;
        Ok(Self {
            identity: client,
            identity_origin: identity.endpoint,
            login_operation,
            store,
            key,
            fence,
            trusted_service_key,
            scope,
            resource_scope: binding_digest,
            max_response_bytes,
        })
    }
    pub(crate) fn validate_config(&self, config: &WorldServiceClientConfig) -> Result<(), Error> {
        if config.endpoint != self.identity_origin
            || config.trusted_service_public_key != self.trusted_service_key
            || config.expected_world != self.world()
            || config.scope_id != format!("agent:{}", self.fence.agent_id)
            || !config.read_private_key_hex.is_empty()
            || config.max_response_bytes > self.max_response_bytes
        {
            return Err(configuration(
                "executor transport differs from pinned resource or exposes a raw read key",
            ));
        }
        Ok(())
    }
    fn world(&self) -> WorldIdentity {
        WorldIdentity {
            world_id: self.fence.world_id.clone(),
            genesis_digest: self.fence.genesis_digest.clone(),
        }
    }
    pub(crate) fn authority_identity(&self) -> String {
        self.scope.clone()
    }
    fn public_key(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }
    pub(crate) fn executor_public_key(&self) -> String {
        self.public_key()
    }
    pub(crate) fn resource_fence(&self) -> &GameTaskFenceV1 {
        &self.fence
    }
    pub(crate) fn open_operation_store(
        &self,
        directory: &Path,
    ) -> Result<crate::operation_store::OperationStore, Error> {
        if !directory.is_absolute()
            || directory.join("executor-control.encrypted-v1").exists()
            || directory.join("owner-control.encrypted-v1").exists()
            || directory.join("login-identity.encrypted-v1").exists()
        {
            return Err(configuration(
                "executor operation state requires a separate absolute private directory",
            ));
        }
        let mut material = zeroize::Zeroizing::new(self.key.to_bytes().to_vec());
        material.extend_from_slice(self.resource_scope.as_bytes());
        let key = zeroize::Zeroizing::new(blake3::derive_key(
            "oasis7/executor-operation-state/v1",
            &material,
        ));
        let state =
            crate::operation_store::OperationStore::open(directory, *key).map_err(configuration)?;
        let canonical = directory
            .canonicalize()
            .map_err(|_| configuration("executor state directory unavailable"))?;
        let saved: PathBuf = self
            .store
            .put_original(
                "executor-resource-state/v1",
                &self.resource_scope,
                "directory",
                &canonical,
            )
            .map_err(configuration)?;
        if saved != canonical {
            return Err(configuration(
                "executor resource already has an original operation state directory; reconcile it",
            ));
        }
        Ok(state)
    }
    /// Restricted typed executor signing for trusted local device commands.
    /// The completed envelope is validated by the same fixed route/resource
    /// allowlist used before transport. No arbitrary domain or owner signing.
    pub(crate) fn sign_native_request<Q: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        request: Q,
    ) -> Result<SignedReadRequest<Q>, Error> {
        if ![
            "cognition",
            "scheduler",
            TASK_CONTEXT_PREPARE_PATH,
            TASK_CONTEXT_READ_PATH,
            TASK_SNAPSHOT_PATH,
            TASK_RECIPE_QUOTE_PATH,
            TASK_FACTORY_QUOTE_PATH,
        ]
        .contains(&path)
        {
            return Err(assurance("executor cannot sign owner or arbitrary domains"));
        }
        let signed =
            ws::sign_read_request(path, request, &self.private_key()).map_err(assurance)?;
        match path {
            "cognition" => {
                let typed: SignedReadRequest<CognitionIntentV1> =
                    decode(&serde_json::to_value(&signed).map_err(assurance)?)?;
                self.verify_payload(&WorldServicePayloadV1::Cognition(typed))?;
            }
            "scheduler" => {
                let typed: SignedReadRequest<TaskSchedulerIntentV1> =
                    decode(&serde_json::to_value(&signed).map_err(assurance)?)?;
                self.verify_payload(&WorldServicePayloadV1::Scheduler(typed))?;
            }
            TASK_CONTEXT_PREPARE_PATH
            | TASK_CONTEXT_READ_PATH
            | TASK_SNAPSHOT_PATH
            | TASK_RECIPE_QUOTE_PATH
            | TASK_FACTORY_QUOTE_PATH => {
                self.operation(path, &signed)?;
            }
            _ => return Err(assurance("executor cannot sign owner or arbitrary domains")),
        }
        Ok(signed)
    }
    fn private_key(&self) -> zeroize::Zeroizing<String> {
        zeroize::Zeroizing::new(hex::encode(self.key.to_bytes()))
    }
    /// Only the two typed read domains used by native task discovery. No
    /// arbitrary model-supplied signing or owner-control domain is exposed.
    pub(crate) fn sign_read<Q: Serialize>(
        &self,
        path: &str,
        request: Q,
    ) -> Result<SignedReadRequest<Q>, Error> {
        let value = serde_json::to_value(&request).map_err(assurance)?;
        match path {
            VIEW_JSON_PATH => {
                let r: ReadWorldViewRequest = decode(&value)?;
                self.read_scope(&r)?;
            }
            DESCRIBE_PATH => {
                let r: DescribeWorldRequest = decode(&value)?;
                if r.expected_world != self.world()
                    || r.trust_config_ref != self.trusted_service_key
                {
                    return Err(assurance("executor describe trust mismatch"));
                }
            }
            _ => return Err(assurance("executor gateway cannot sign this operation")),
        }
        ws::sign_read_request(path, request, &self.private_key()).map_err(assurance)
    }
    fn read_scope(&self, r: &ReadWorldViewRequest) -> Result<(), Error> {
        r.validate().map_err(assurance)?;
        if r.world != self.world()
            || r.scope_id != format!("agent:{}", self.fence.agent_id)
            || r.observation_nonce.is_none()
            || r.formal_anchor.is_some()
        {
            return Err(assurance("executor read resource or freshness mismatch"));
        }
        Ok(())
    }
    fn match_fence(&self, f: &GameTaskFenceV1) -> Result<(), Error> {
        if f != &self.fence {
            return Err(assurance(
                "executor operation belongs to another task fence",
            ));
        }
        Ok(())
    }
    fn match_payload_fence(&self, f: &GameTaskFenceV1, historical: bool) -> Result<(), Error> {
        if !historical {
            return self.match_fence(f);
        }
        if f.world_id != self.fence.world_id
            || f.genesis_digest != self.fence.genesis_digest
            || f.agent_id != self.fence.agent_id
            || f.task_id != self.fence.task_id
            || f.owner_generation != self.fence.owner_generation
            || f.owner_binding != self.fence.owner_binding
            || f.owner_public_key != self.fence.owner_public_key
        {
            return Err(assurance(
                "historical executor original belongs to another resource",
            ));
        }
        f.validate().map_err(assurance)
    }
    fn verify_signer<Q: Serialize>(
        &self,
        path: &str,
        signed: &SignedReadRequest<Q>,
    ) -> Result<(), Error> {
        if signed.subject_public_key != self.public_key() {
            return Err(assurance("executor request subject mismatch"));
        }
        ws::private_goal::verify_owner_signature_strict(
            path,
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
        )
        .map_err(assurance)
    }
    fn verify_payload(&self, payload: &WorldServicePayloadV1) -> Result<(), Error> {
        self.verify_payload_kind(payload, false)
    }
    fn verify_payload_kind(
        &self,
        payload: &WorldServicePayloadV1,
        historical: bool,
    ) -> Result<(), Error> {
        let correlation =
            oasis7_client_api::world_service_correlation::derive_correlation(self.world(), payload)
                .map_err(assurance)?;
        if !historical && correlation.key.verified_subject != self.public_key() {
            return Err(assurance("executor intent subject mismatch"));
        }
        match payload {
            WorldServicePayloadV1::Cognition(s)
                if s.request.request.agent_id == self.fence.agent_id
                    && s.request.causal_proposal.is_none() =>
            {
                self.verify_payload_binding(
                    s.request
                        .task_fence
                        .as_ref()
                        .ok_or_else(|| assurance("executor cognition requires task fence"))?,
                    &s.subject_public_key,
                    s.request.delegation_generation,
                    historical,
                )
            }
            WorldServicePayloadV1::Scheduler(s) if s.request.agent_id == self.fence.agent_id => {
                match &s.request.operation {
                    SchedulerOperationV1::TaskProviderPrefix { context }
                    | SchedulerOperationV1::TaskProviderFailure { context, .. } => self
                        .verify_payload_binding(
                            &context.fence,
                            &s.subject_public_key,
                            s.request.delegation_generation,
                            historical,
                        ),
                    _ => Err(assurance(
                        "executor gateway requires task scheduler operation",
                    )),
                }
            }
            _ => Err(assurance(
                "executor gateway denies owner and unscoped intents",
            )),
        }
    }
    fn verify_payload_binding(
        &self,
        fence: &GameTaskFenceV1,
        original_actor: &str,
        generation: u64,
        historical: bool,
    ) -> Result<(), Error> {
        self.match_payload_fence(fence, historical)?;
        let binding = fence
            .executor_binding
            .as_ref()
            .ok_or_else(|| assurance("original executor binding missing"))?;
        if original_actor == fence.owner_public_key
            || binding.executor_public_key != original_actor
            || generation == 0
            || binding.delegation_generation.0 != generation
        {
            return Err(assurance(
                "original executor signer or delegation generation differs from original fence",
            ));
        }
        if !historical && original_actor != self.public_key() {
            return Err(assurance("executor intent subject mismatch"));
        }
        Ok(())
    }
    fn operation<Q: Serialize>(
        &self,
        path: &str,
        request: &Q,
    ) -> Result<EntryExecutorOperationV1, Error> {
        let value = serde_json::to_value(request).map_err(assurance)?;
        let operation = match path {
            VIEW_JSON_PATH => {
                let r: SignedReadRequest<ReadWorldViewRequest> = decode(&value)?;
                self.read_scope(&r.request)?;
                self.verify_signer(path, &r)?;
                EntryExecutorOperationV1::ReadView(r)
            }
            DESCRIBE_PATH => {
                let r: SignedReadRequest<DescribeWorldRequest> = decode(&value)?;
                self.verify_signer(path, &r)?;
                if r.request.expected_world != self.world()
                    || r.request.trust_config_ref != self.trusted_service_key
                {
                    return Err(assurance("executor describe world mismatch"));
                }
                EntryExecutorOperationV1::Describe(r)
            }
            SUBMIT_PATH => {
                let r: SubmitIntentRequest<WorldServicePayloadV1> = decode(&value)?;
                r.validate().map_err(assurance)?;
                self.verify_payload(&r.signed_payload)?;
                if r.correlation
                    != oasis7_client_api::world_service_correlation::derive_correlation(
                        self.world(),
                        &r.signed_payload,
                    )
                    .map_err(assurance)?
                {
                    return Err(assurance("executor intent correlation mismatch"));
                }
                EntryExecutorOperationV1::Submit(r)
            }
            LOOKUP_PATH => {
                let r: AuthenticatedLookup = decode(&value)?;
                r.request.validate().map_err(assurance)?;
                self.verify_payload_kind(&r.original, true)?;
                if r.request.key
                    != oasis7_client_api::world_service_correlation::derive_correlation(
                        self.world(),
                        &r.original,
                    )
                    .map_err(assurance)?
                    .key
                {
                    return Err(assurance("executor lookup key mismatch"));
                }
                EntryExecutorOperationV1::Lookup(r)
            }
            TASK_CONTEXT_PREPARE_PATH => {
                let r: SignedReadRequest<TaskContextPrepareV1> = decode(&value)?;
                verify_prepare(&r).map_err(assurance)?;
                self.verify_signer(path, &r)?;
                self.match_fence(&r.request.fence)?;
                EntryExecutorOperationV1::PrepareContext(r)
            }
            TASK_CONTEXT_READ_PATH => {
                let r: SignedReadRequest<TaskContextGetV1> = decode(&value)?;
                verify_get(&r).map_err(assurance)?;
                self.verify_signer(path, &r)?;
                self.match_fence(&r.request.context.fence)?;
                EntryExecutorOperationV1::ReadContext(r)
            }
            TASK_SNAPSHOT_PATH => {
                let r: SignedReadRequest<TaskGoalSnapshotRequestV1> = decode(&value)?;
                verify_snapshot_request(&r).map_err(assurance)?;
                self.verify_signer(path, &r)?;
                self.match_fence(&r.request.fence)?;
                EntryExecutorOperationV1::GoalSnapshot(r)
            }
            TASK_RECIPE_QUOTE_PATH => {
                let r: SignedReadRequest<TaskRecipeQuoteRequestV1> = decode(&value)?;
                verify_recipe_quote_request(&r).map_err(assurance)?;
                self.verify_signer(path, &r)?;
                self.match_fence(&r.request.context_proof.request.context.fence)?;
                EntryExecutorOperationV1::RecipeQuote(r)
            }
            TASK_FACTORY_QUOTE_PATH => {
                let r: SignedReadRequest<TaskFactoryQuoteRequestV1> = decode(&value)?;
                verify_factory_quote_request(&r).map_err(assurance)?;
                self.verify_signer(path, &r)?;
                self.match_fence(&r.request.context_proof.request.context.fence)?;
                EntryExecutorOperationV1::FactoryQuote(r)
            }
            _ => return Err(assurance("executor gateway route not allowed")),
        };
        Ok(operation)
    }
    pub(crate) fn response_bytes<Q: Serialize>(
        &self,
        path: &str,
        request: &Q,
        maximum: usize,
    ) -> Result<zeroize::Zeroizing<Vec<u8>>, Error> {
        let operation = self.operation(path, request)?;
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| assurance("executor access nonce unavailable"))?;
        let read = ReadWorldViewRequest {
            contract_version: 1,
            observation_nonce: Some(hex::encode(nonce)),
            formal_anchor: None,
            world: self.world(),
            scope_id: format!("agent:{}", self.fence.agent_id),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        };
        let access = EntryExecutorAccessV1 {
            read: self.sign_read(VIEW_JSON_PATH, read)?,
            fence: self.fence.clone(),
        };
        let original = EntryExecutorControlRequestV1 { access, operation };
        let digest =
            ws::request_digest("executor-gateway-transport/v1", &original).map_err(assurance)?;
        let body = zeroize::Zeroizing::new(serde_json::to_vec(&original).map_err(assurance)?);
        if body.len() > 65536 {
            return Err(configuration(
                "executor gateway envelope exceeds public request bound",
            ));
        }
        let original: EntryExecutorControlRequestV1 = self
            .store
            .put_original(&self.scope, &digest, "transport-original", &original)
            .map_err(assurance)?;
        let reply = self
            .identity
            .authenticated_entry_call(
                &self.login_operation,
                "/entry-executor/control",
                &serde_json::to_value(original).map_err(assurance)?,
            )
            .map_err(|_| {
                Error::Transport(
                    "executor gateway outcome unknown; retain original operation".into(),
                )
            })?;
        if let Some(error) = reply.get("upstream_read_error") {
            if let Some(code) = fixed_read_drift(path, error)
                .filter(|_| reply.get("world_response").is_none_or(Value::is_null))
            {
                return Err(Error::Rejected {
                    status: 409,
                    message: code.into(),
                });
            }
            return Err(assurance("unsupported executor upstream observation"));
        }
        let bound = maximum.min(self.max_response_bytes);
        let bytes = zeroize::Zeroizing::new(
            serde_json::to_vec(&reply["world_response"])
                .map_err(|_| assurance("executor gateway result malformed"))?,
        );
        if bytes.len() > bound {
            return Err(assurance("executor gateway response exceeds bound"));
        }
        Ok(bytes)
    }
    pub(crate) fn call<Q: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        request: &Q,
        maximum: usize,
    ) -> Result<SignedServiceResponse<R>, Error> {
        let bound = maximum.min(self.max_response_bytes);
        let bytes = self.response_bytes(path, request, bound)?;
        let value: Value = oasis7_agent_api::game::parse_bounded_json(&bytes, bound)
            .map_err(|_| assurance("executor gateway signed response malformed"))?;
        use serde::de::IntoDeserializer;
        let mut ignored = false;
        let signed: SignedServiceResponse<R> =
            serde_ignored::deserialize(value.into_deserializer(), |_| ignored = true)
                .map_err(|_| assurance("executor gateway signed response malformed"))?;
        if ignored {
            return Err(assurance("unsupported executor gateway response field"));
        }
        // Check the exact original typed domain, never the gateway outer body.
        ws::verify_service_response(
            path,
            &ws::request_digest(path, request).map_err(assurance)?,
            &signed,
            &self.trusted_service_key,
        )
        .map_err(assurance)?;
        ws::private_goal::verify_owner_signature_strict(
            path,
            &(&signed.request_digest, &signed.payload),
            &self.trusted_service_key,
            &signed.signature_hex,
        )
        .map_err(assurance)?;
        Ok(signed)
    }
}

fn fixed_read_drift<'a>(path: &str, error: &'a Value) -> Option<&'a str> {
    let code = error.get("code")?.as_str()?;
    if error.get("status") != Some(&Value::from(409)) || error.as_object()?.len() != 2 {
        return None;
    }
    match path {
        TASK_RECIPE_QUOTE_PATH | TASK_FACTORY_QUOTE_PATH
            if [TASK_QUOTE_BASE_DRIFT, TASK_QUOTE_DELIVERY_DRIFT].contains(&code) =>
        {
            Some(code)
        }
        TASK_CONTEXT_READ_PATH if code == TASK_CONTEXT_READ_DELIVERY_DRIFT => Some(code),
        _ => None,
    }
}

fn decode<T: Serialize + DeserializeOwned>(value: &Value) -> Result<T, Error> {
    let bytes = serde_json::to_vec(value).map_err(assurance)?;
    let result: T = oasis7_agent_api::game::parse_bounded_json(&bytes, 2 * 1024 * 1024)
        .map_err(|_| assurance("executor original malformed"))?;
    if serde_json::to_value(&result).map_err(assurance)? != *value {
        return Err(assurance("executor original representation changed"));
    }
    Ok(result)
}
fn independent_directory(directory: &Path, identity: &Path) -> Result<(), Error> {
    if !directory.is_absolute()
        || directory.components().any(|c| {
            matches!(
                c,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
        || directory == identity
        || directory.join("owner-control.encrypted-v1").exists()
        || directory
            .canonicalize()
            .ok()
            .is_some_and(|p| identity.canonicalize().ok().as_ref() == Some(&p))
    {
        return Err(configuration(
            "executor requires an independent private vault directory",
        ));
    }
    Ok(())
}
