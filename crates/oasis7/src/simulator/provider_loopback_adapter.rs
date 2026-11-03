use super::cognition_response_identity::{
    CognitionResponseDigestDisposition, classify_cognition_response_digest,
};
use super::{
    Action, AgentQuery, ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ContinuousAgentTurnContextV1, DecisionProvider, DecisionProviderError, DecisionRequest,
    DecisionResponse, FeedbackEnvelope, FeedbackEnvelopeV1, ProviderDecision, ProviderFeedbackAck,
    ProviderLoopbackHttpClient, ProviderLoopbackHttpError,
};
#[cfg(not(target_arch = "wasm32"))]
use super::{
    ProviderOriginalMaterialRefsV1, ProviderOriginalMaterialStore,
    provider_json_admission::ProviderResponseContextLease,
};
use oasis7_wasm_abi::{
    AgentCommandResponse, CapabilityCatalogSnapshot, CapabilityGrantV2, ModuleCommandDeclaration,
    ModuleCommandValidationError, ModuleSchemaDeclarations, validate_module_command_declarations,
};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

/// Host-owned seam between the simulator provider loop and the runtime
/// capability executor.  Provider output is accepted as a typed response,
/// but only this adapter may hand it to `runtime::World`.
#[cfg(not(target_arch = "wasm32"))]
pub trait TrustedModuleCommandExecutor {
    fn execute_trusted_module_command(
        &mut self,
        response: AgentCommandResponse,
    ) -> Result<crate::runtime::CapabilityAuthorizationAuditReceipt, crate::runtime::WorldError>;
}

/// Concrete runtime adapter used by host loops.  The grant, catalog and
/// invocation context are supplied by trusted runtime code and are never
/// selected from provider output.
#[cfg(not(target_arch = "wasm32"))]
pub struct RuntimeTrustedModuleExecutor<'a> {
    world: &'a mut crate::runtime::World,
    grant: CapabilityGrantV2,
    catalog: CapabilityCatalogSnapshot,
    invocation: crate::capability_invocation_context::CapabilityInvocationContext,
    sandbox: &'a mut dyn oasis7_wasm_abi::ModuleSandbox,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> RuntimeTrustedModuleExecutor<'a> {
    pub fn new(
        world: &'a mut crate::runtime::World,
        grant: CapabilityGrantV2,
        catalog: CapabilityCatalogSnapshot,
        invocation: crate::capability_invocation_context::CapabilityInvocationContext,
        sandbox: &'a mut dyn oasis7_wasm_abi::ModuleSandbox,
    ) -> Self {
        Self {
            world,
            grant,
            catalog,
            invocation,
            sandbox,
        }
    }

    /// Execute one already validated provider response through the runtime
    /// authorization boundary.  The response is never converted into a core
    /// simulator `Action`.
    pub fn execute(
        &mut self,
        response: AgentCommandResponse,
    ) -> Result<crate::runtime::CapabilityAuthorizationAuditReceipt, crate::runtime::WorldError>
    {
        let context = super::ProviderCapabilityContext {
            catalog: self.catalog.clone(),
            invocation: self.invocation.clone(),
        };
        context.validate_response(&response).map_err(|error| {
            crate::runtime::WorldError::CapabilityAuthorizationDenied {
                reason: error.to_string(),
            }
        })?;
        self.world.execute_trusted_module_command(
            self.grant.clone(),
            self.catalog.clone(),
            response,
            &mut (),
            self.sandbox,
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl TrustedModuleCommandExecutor for RuntimeTrustedModuleExecutor<'_> {
    fn execute_trusted_module_command(
        &mut self,
        response: AgentCommandResponse,
    ) -> Result<crate::runtime::CapabilityAuthorizationAuditReceipt, crate::runtime::WorldError>
    {
        self.execute(response)
    }
}

const DEFAULT_PROVIDER_LOOPBACK_ADAPTER_PROVIDER_ID: &str = "provider_loopback_http";
const PHASE1_ALLOWED_ACTION_REFS: &[&str] = &[
    "wait",
    "wait_ticks",
    "move_agent",
    "speak_to_nearby",
    "inspect_target",
    "simple_interact",
];

pub struct ProviderLoopbackAdapter {
    provider_id: String,
    client: ProviderLoopbackHttpClient,
    #[cfg(not(target_arch = "wasm32"))]
    original_material_store: Option<Arc<dyn ProviderOriginalMaterialStore>>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_original_material_refs: Option<ProviderOriginalMaterialRefsV1>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_response_context_lease: Option<ProviderResponseContextLease>,
}

impl std::fmt::Debug for ProviderLoopbackAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderLoopbackAdapter")
            .field("provider_id", &self.provider_id)
            .finish_non_exhaustive()
    }
}

impl ProviderLoopbackAdapter {
    pub fn new(
        base_url: &str,
        auth_token: Option<&str>,
        timeout_ms: u64,
    ) -> Result<Self, ProviderLoopbackHttpError> {
        Self::new_with_transport(base_url, auth_token, timeout_ms, "loopback_http")
    }

    pub fn new_with_transport(
        base_url: &str,
        auth_token: Option<&str>,
        timeout_ms: u64,
        transport: &str,
    ) -> Result<Self, ProviderLoopbackHttpError> {
        let client = ProviderLoopbackHttpClient::new_with_transport(
            base_url, auth_token, timeout_ms, transport,
        )?;
        Ok(Self::with_client(
            DEFAULT_PROVIDER_LOOPBACK_ADAPTER_PROVIDER_ID,
            client,
        ))
    }

    pub fn with_client(provider_id: impl Into<String>, client: ProviderLoopbackHttpClient) -> Self {
        let provider_id = provider_id.into();
        Self {
            provider_id: if provider_id.trim().is_empty() {
                DEFAULT_PROVIDER_LOOPBACK_ADAPTER_PROVIDER_ID.to_string()
            } else {
                provider_id
            },
            client,
            #[cfg(not(target_arch = "wasm32"))]
            original_material_store: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_original_material_refs: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_response_context_lease: None,
        }
    }

    /// Install the explicitly selected new-format pending owner. This is never
    /// inferred from the legacy provider-lineage checkpoint path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_original_material_store(
        mut self,
        store: Arc<dyn ProviderOriginalMaterialStore>,
    ) -> Self {
        self.original_material_store = Some(store);
        self
    }

    pub fn phase1_allowed_action_refs() -> &'static [&'static str] {
        PHASE1_ALLOWED_ACTION_REFS
    }

    fn request_advertises_action_ref(request: &DecisionRequest, action_ref: &str) -> bool {
        request
            .observation
            .action_catalog
            .iter()
            .any(|entry| entry.action_ref == action_ref)
    }

    fn validate_response(
        &self,
        request: &DecisionRequest,
        response: &DecisionResponse,
    ) -> Result<(), DecisionProviderError> {
        if let Some(error) = &response.provider_error {
            return Err(DecisionProviderError::new_with_upstream_trace(
                error.code.clone(),
                error.message.clone(),
                error.retryable,
                response.trace_payload.upstream_trace.clone(),
            ));
        }
        if let ProviderDecision::ModuleCommandResponse {
            response: command_response,
        } = &response.decision
        {
            if response.module_command.is_some() {
                return Err(DecisionProviderError::new(
                    "module_command_ambiguous",
                    "typed module response cannot be accompanied by a legacy module_command",
                    false,
                ));
            }
            let Some(catalog) = request.capability_catalog.as_ref() else {
                return Err(DecisionProviderError::new(
                    "module_command_catalog_unavailable",
                    "typed AgentCommandResponse requires a trusted runtime catalog",
                    false,
                ));
            };
            let Some(context) = request.capability_invocation_context.as_ref() else {
                return Err(DecisionProviderError::new(
                    "module_command_context_unavailable",
                    "typed AgentCommandResponse requires a trusted invocation context",
                    false,
                ));
            };
            command_response.validate().map_err(|error| {
                DecisionProviderError::new(
                    "module_command_response_invalid",
                    error.to_string(),
                    false,
                )
            })?;
            if !command_response.matches_catalog(catalog)
                || command_response.subject != context.subject
                || command_response.presenter != context.presenter
                || command_response.audience != context.audience
                || command_response.catalog_snapshot_id != context.catalog_snapshot_id
                || command_response.response_nonce != context.response_nonce
                || command_response.selected_entry.module_id != context.module_id
                || command_response.selected_entry.module_version != context.module_version
            {
                return Err(DecisionProviderError::new(
                    "module_command_context_mismatch",
                    "typed AgentCommandResponse does not match trusted catalog/context",
                    false,
                ));
            }
            return Ok(());
        }
        if let Some(module_command) = response.module_command.as_ref() {
            validate_module_command_response(request, module_command)?;
            return Err(DecisionProviderError::new(
                "module_command_unroutable",
                "provider returned a valid module command but no trusted module executor is attached",
                false,
            ));
        }
        match &response.decision {
            ProviderDecision::Wait => Ok(()),
            ProviderDecision::WaitTicks { .. } => {
                if !Self::request_advertises_action_ref(request, "wait_ticks") {
                    return Err(DecisionProviderError::new(
                        "action_ref_not_in_catalog",
                        "action_ref `wait_ticks` is not present in request action_catalog",
                        false,
                    ));
                }
                if Self::phase1_allowed_action_refs().contains(&"wait_ticks") {
                    Ok(())
                } else {
                    Err(DecisionProviderError::new(
                        "action_ref_not_allowed",
                        "ProviderLoopbackAdapter phase-1 whitelist does not permit wait_ticks",
                        false,
                    ))
                }
            }
            ProviderDecision::Act { action_ref, action } => {
                if !Self::request_advertises_action_ref(request, action_ref) {
                    return Err(DecisionProviderError::new(
                        "action_ref_not_in_catalog",
                        format!(
                            "action_ref `{action_ref}` is not present in request action_catalog"
                        ),
                        false,
                    ));
                }
                if !Self::phase1_allowed_action_refs()
                    .iter()
                    .any(|allowed| *allowed == action_ref)
                {
                    return Err(DecisionProviderError::new(
                        "action_ref_not_allowed",
                        format!(
                            "action_ref `{action_ref}` is outside ProviderLoopbackAdapter phase-1 whitelist"
                        ),
                        false,
                    ));
                }
                match resolved_action_ref(action) {
                    Some(expected_action_ref) if expected_action_ref == action_ref => Ok(()),
                    Some(expected_action_ref) => Err(DecisionProviderError::new(
                        "action_ref_mismatch",
                        format!(
                            "action_ref `{action_ref}` does not match serialized action kind `{expected_action_ref}`"
                        ),
                        false,
                    )),
                    None => Err(DecisionProviderError::new(
                        "action_kind_not_supported",
                        format!(
                            "action_ref `{action_ref}` is phase-1 allowed but its Action variant is not yet supported by ProviderLoopbackAdapter"
                        ),
                        false,
                    )),
                }
            }
            ProviderDecision::Query { query_ref, query } => {
                if !Self::request_advertises_action_ref(request, query_ref) {
                    return Err(DecisionProviderError::new(
                        "action_ref_not_in_catalog",
                        format!("query_ref `{query_ref}` is not present in request action_catalog"),
                        false,
                    ));
                }
                match resolved_query_ref(query) {
                    expected_query_ref if expected_query_ref == query_ref => Ok(()),
                    expected_query_ref => Err(DecisionProviderError::new(
                        "query_ref_mismatch",
                        format!(
                            "query_ref `{query_ref}` does not match serialized query kind `{expected_query_ref}`"
                        ),
                        false,
                    )),
                }
            }
            ProviderDecision::ModuleCommand { module_command } => {
                validate_module_command_response(request, module_command)?;
                Err(DecisionProviderError::new(
                    "module_command_unroutable",
                    "provider returned a valid module command but no trusted module executor is attached",
                    false,
                ))
            }
            ProviderDecision::ModuleCommandResponse { .. } => {
                // The complete response was checked above against the
                // runtime-produced catalog and host context.
                Ok(())
            }
        }
    }

    /// Route a typed v2 provider response into a trusted host executor.
    ///
    /// This is the explicit bridge seam used by host loops: validation is
    /// completed against the request's runtime-produced catalog and bound
    /// invocation context before the executor is called.  Legacy
    /// `ProviderModuleCommand` values remain intentionally unroutable.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn execute_trusted_module_command<E: TrustedModuleCommandExecutor>(
        &self,
        request: &DecisionRequest,
        response: DecisionResponse,
        executor: &mut E,
    ) -> Result<crate::runtime::CapabilityAuthorizationAuditReceipt, DecisionProviderError> {
        self.validate_response(request, &response)?;
        let ProviderDecision::ModuleCommandResponse { response } = response.decision else {
            return Err(DecisionProviderError::new(
                "module_command_not_typed",
                "only typed ModuleCommandResponse values may cross the trusted module bridge",
                false,
            ));
        };
        executor
            .execute_trusted_module_command(response)
            .map_err(|error| {
                DecisionProviderError::new(
                    "trusted_module_execution_failed",
                    format!("trusted module executor rejected command: {error:?}"),
                    false,
                )
            })
    }

    fn map_http_error(error: ProviderLoopbackHttpError) -> DecisionProviderError {
        match error {
            ProviderLoopbackHttpError::InvalidBaseUrl(detail) => {
                DecisionProviderError::new("provider_config_invalid", detail, false)
            }
            ProviderLoopbackHttpError::RequestFailed { detail, .. } => {
                DecisionProviderError::new("provider_unreachable", detail, true)
            }
            ProviderLoopbackHttpError::Unauthorized { detail, .. } => {
                DecisionProviderError::new("provider_unauthorized", detail, false)
            }
            ProviderLoopbackHttpError::UnexpectedStatus {
                status_code, body, ..
            } => DecisionProviderError::new(
                format!("provider_http_{status_code}"),
                if body.is_empty() {
                    format!("provider bridge returned HTTP {status_code}")
                } else {
                    format!("provider bridge returned HTTP {status_code}: {body}")
                },
                status_code >= 500,
            ),
            ProviderLoopbackHttpError::DecodeFailed { detail, .. } => {
                DecisionProviderError::new("provider_payload_invalid", detail, false)
            }
        }
    }

    fn map_feedback_ack_error(ack: ProviderFeedbackAck) -> Result<(), DecisionProviderError> {
        if ack.ok {
            return Ok(());
        }
        Err(DecisionProviderError::new(
            ack.error_code
                .unwrap_or_else(|| "feedback_rejected".to_string()),
            ack.error
                .unwrap_or_else(|| "provider feedback endpoint rejected payload".to_string()),
            false,
        ))
    }
}

fn validate_module_command_response(
    request: &DecisionRequest,
    command: &super::ProviderModuleCommand,
) -> Result<(), DecisionProviderError> {
    let declaration = ModuleCommandDeclaration {
        namespace: command.namespace.clone(),
        name: command.name.clone(),
        schema_version: command.schema_version,
        schema_hash: command.schema_hash.clone(),
        max_payload_bytes: command.payload.len().max(1) as u64,
    };
    if let Err(error) = validate_module_command_declarations(&ModuleSchemaDeclarations {
        commands: vec![declaration],
    }) {
        let code = match error {
            ModuleCommandValidationError::ReservedNamespace(_) => {
                "module_command_reserved_namespace"
            }
            ModuleCommandValidationError::InvalidNamespace(_)
            | ModuleCommandValidationError::InvalidName(_)
            | ModuleCommandValidationError::InvalidSchemaVersion
            | ModuleCommandValidationError::InvalidSchemaHash(_)
            | ModuleCommandValidationError::InvalidPayloadBound(_) => {
                "module_command_invalid_declaration"
            }
            _ => "module_command_invalid_declaration",
        };
        return Err(DecisionProviderError::new(code, error.to_string(), false));
    }

    let Some(catalog_entry) = request
        .observation
        .module_command_catalog
        .iter()
        .find(|entry| {
            entry.module_id == command.module_id
                && entry.module_version == command.module_version
                && entry.namespace == command.namespace
                && entry.name == command.name
                && entry.schema_version == command.schema_version
        })
    else {
        return Err(DecisionProviderError::new(
            "module_command_identity_mismatch",
            "provider module command does not match an exact catalog identity",
            false,
        ));
    };
    if catalog_entry.schema_hash != command.schema_hash {
        return Err(DecisionProviderError::new(
            "module_command_schema_hash_mismatch",
            "provider module command schema_hash does not match the catalog",
            false,
        ));
    }
    if command.payload.is_empty()
        || command.payload.len() > catalog_entry.max_payload_bytes as usize
    {
        return Err(DecisionProviderError::new(
            "module_command_payload_too_large",
            format!(
                "provider module command payload size {} exceeds catalog bound {}",
                command.payload.len(),
                catalog_entry.max_payload_bytes
            ),
            false,
        ));
    }
    Ok(())
}

impl DecisionProvider for ProviderLoopbackAdapter {
    fn provider_id(&self) -> &str {
        self.provider_id.as_str()
    }

    fn decide(
        &mut self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, DecisionProviderError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.pending_original_material_refs = None;
            self.pending_response_context_lease = None;
        }
        let response = self
            .client
            .request_decision(request)
            .map_err(Self::map_http_error)?;
        self.validate_response(request, &response)?;
        Ok(response)
    }

    fn decide_with_continuous_request_context(
        &mut self,
        request: &DecisionRequest,
        turn_context: &ContinuousAgentTurnContextV1,
        request_context: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1, DecisionProviderError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.pending_original_material_refs = None;
            self.pending_response_context_lease = None;
        }
        request_context
            .validate_production_lane()
            .map_err(|error| DecisionProviderError::new(error.code(), error.message(), false))?;
        if request_context.base_decision_request != *request
            || request_context.agent_session_id != turn_context.agent_session_id
            || request_context.agent_turn_id != turn_context.agent_turn_id
            || request_context.decision_request_id != turn_context.decision_request_id
            || request_context.request_digest != turn_context.request_digest
        {
            return Err(DecisionProviderError::new(
                "request_context_identity_mismatch",
                "loopback request and outer context do not describe one turn",
                false,
            ));
        }
        #[cfg(not(target_arch = "wasm32"))]
        let response = if let Some(store) = self.original_material_store.as_deref() {
            let (response, refs, response_context_lease) = self
                .client
                .request_decision_with_context_and_material_store(request_context, store)
                .map_err(|error| {
                    DecisionProviderError::new(
                        "provider_attempt_retained",
                        format!(
                            "captured provider attempt is retained under its original identity: {error}"
                        ),
                        false,
                    )
                })?;
            // Keep the references even if the subsequent semantic validation
            // fails: the owner must retain the raw response as pending rather
            // than treating an invalid response as if no external effect ran.
            self.pending_original_material_refs = Some(refs);
            self.pending_response_context_lease = Some(response_context_lease);
            response
        } else {
            self.client
                .request_decision_with_context(request_context)
                .map_err(Self::map_http_error)?
        };
        #[cfg(target_arch = "wasm32")]
        let response = self
            .client
            .request_decision_with_context(request_context)
            .map_err(Self::map_http_error)?;
        if response.context_discriminator != super::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR
            || response.context_version != super::CONTINUOUS_AGENT_CONTEXT_VERSION
            || response.agent_session_id != request_context.agent_session_id
            || response.agent_turn_id != request_context.agent_turn_id
            || response.decision_request_id != request_context.decision_request_id
            || response.retry_seq != request_context.retry_seq
            || response.transport_attempt != request_context.transport_attempt
            || response.request_digest != request_context.request_digest
        {
            return Err(DecisionProviderError::new(
                "response_identity_mismatch",
                "loopback response dropped outer request lineage",
                false,
            ));
        }
        match classify_cognition_response_digest(
            &response.base_decision_response,
            &response.response_digest,
        ) {
            CognitionResponseDigestDisposition::Current => {}
            CognitionResponseDigestDisposition::Legacy => {
                return Err(DecisionProviderError::new(
                    "legacy_response_digest_unsupported",
                    "legacy full-response digest requires an explicit compatibility migration",
                    false,
                ));
            }
            CognitionResponseDigestDisposition::Mismatch => {
                return Err(DecisionProviderError::new(
                    "response_digest_mismatch",
                    "loopback response digest does not match its content",
                    false,
                ));
            }
        }
        let artifact_identity = response.response_artifact_identity();
        response
            .validate_response_artifact_identity(&artifact_identity)
            .map_err(|error| DecisionProviderError::new(error.code(), error.message(), false))?;
        self.validate_response(request, &response.base_decision_response)?;
        Ok(response)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn take_provider_original_material_refs(&mut self) -> Option<ProviderOriginalMaterialRefsV1> {
        self.pending_original_material_refs.take()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn take_provider_response_context_lease(&mut self) -> Option<ProviderResponseContextLease> {
        self.pending_response_context_lease.take()
    }

    fn push_feedback(&mut self, feedback: &FeedbackEnvelope) -> Result<(), DecisionProviderError> {
        let ack = self
            .client
            .submit_feedback(feedback)
            .map_err(Self::map_http_error)?;
        Self::map_feedback_ack_error(ack)
    }

    fn push_continuous_feedback(
        &mut self,
        feedback: &FeedbackEnvelopeV1,
    ) -> Result<(), DecisionProviderError> {
        let ack = self
            .client
            .submit_feedback_context(feedback)
            .map_err(Self::map_http_error)?;
        Self::map_feedback_ack_error(ack)
    }
}

fn resolved_action_ref(action: &Action) -> Option<&'static str> {
    match action {
        Action::MoveAgent { .. } => Some("move_agent"),
        Action::SpeakToNearby { .. } => Some("speak_to_nearby"),
        Action::InspectTarget { .. } => Some("inspect_target"),
        Action::SimpleInteract { .. } => Some("simple_interact"),
        _ => None,
    }
}

fn resolved_query_ref(query: &AgentQuery) -> &'static str {
    match query {
        AgentQuery::EvaluateMicroDepotQuote(_) => "evaluate_micro_depot_quote",
        AgentQuery::QuoteMicroDepotInstall(_) => "quote_micro_depot_install",
    }
}
