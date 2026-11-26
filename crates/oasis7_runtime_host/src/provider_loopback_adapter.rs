//! Compatible native Host decision-provider contract; no shipping cutover.
use std::sync::Arc;
use std::{error::Error, fmt};

use oasis7_agent_api::ProviderModuleCommand;
use oasis7_agent_harness::{
    CognitionResponseDigestDisposition, classify_cognition_response_digest,
    cognition_response_digest,
};
use oasis7_simulator::{Action, AgentQuery};
use oasis7_wasm_abi::{
    AgentCommandResponse, CapabilityCatalogSnapshot, CapabilityGrantV2, ModuleCommandDeclaration,
    ModuleCommandValidationError, ModuleSchemaDeclarations, validate_module_command_declarations,
};
use serde_json::Value;

use crate::decision_provider::ProviderCapabilityContext;
pub use crate::decision_provider::{DecisionProvider, DecisionProviderError};
use crate::provider_json_admission::ProviderResponseContextLease;
use crate::provider_loopback_http::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ContinuousAgentTurnContextV1, DecisionRequest, DecisionResponse, FeedbackEnvelope,
    FeedbackEnvelopeV1, ProviderDecision, ProviderFeedbackAck, ProviderLoopbackHttpClient,
    ProviderLoopbackHttpError,
};
use crate::provider_material_capture::{
    ProviderOriginalMaterialRefsV1, ProviderOriginalMaterialStore,
};
use crate::simulator_archive::archive::WorldEvent;

const DEFAULT_ID: &str = "provider_loopback_http";
const PHASE1_REFS: &[&str] = &[
    "wait",
    "wait_ticks",
    "move_agent",
    "speak_to_nearby",
    "inspect_target",
    "simple_interact",
];
type DpResult<T> = Result<T, DecisionProviderError>;
type HttpResult<T> = Result<T, ProviderLoopbackHttpError>;

fn reject(code: impl Into<String>, msg: impl Into<String>) -> DecisionProviderError {
    DecisionProviderError::new(code, msg, false)
}

fn bail<T>(code: impl Into<String>, msg: impl Into<String>) -> DpResult<T> {
    Err(reject(code, msg))
}

fn dp_chk(cond: bool, code: &str) -> DpResult<()> {
    if cond { Ok(()) } else { bail(code, code) }
}

pub trait TrustedModuleCommandExecutor {
    fn execute_trusted_module_command(
        &mut self,
        response: AgentCommandResponse,
    ) -> Result<oasis7_runtime::CapabilityAuthorizationAuditReceipt, oasis7_runtime::WorldError>;
}

pub struct RuntimeTrustedModuleExecutor<'a> {
    world: &'a mut oasis7_runtime::World,
    grant: CapabilityGrantV2,
    catalog: CapabilityCatalogSnapshot,
    invocation: oasis7_agent_api::CapabilityInvocationContext,
    sandbox: &'a mut dyn oasis7_wasm_abi::ModuleSandbox,
}

impl<'a> RuntimeTrustedModuleExecutor<'a> {
    pub fn new(
        world: &'a mut oasis7_runtime::World,
        grant: CapabilityGrantV2,
        catalog: CapabilityCatalogSnapshot,
        invocation: oasis7_agent_api::CapabilityInvocationContext,
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

    pub fn execute(
        &mut self,
        response: AgentCommandResponse,
    ) -> Result<oasis7_runtime::CapabilityAuthorizationAuditReceipt, oasis7_runtime::WorldError>
    {
        let context = ProviderCapabilityContext {
            catalog: self.catalog.clone(),
            invocation: self.invocation.clone(),
        };
        context.validate_response(&response).map_err(|error| {
            oasis7_runtime::WorldError::CapabilityAuthorizationDenied {
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

impl TrustedModuleCommandExecutor for RuntimeTrustedModuleExecutor<'_> {
    fn execute_trusted_module_command(
        &mut self,
        response: AgentCommandResponse,
    ) -> Result<oasis7_runtime::CapabilityAuthorizationAuditReceipt, oasis7_runtime::WorldError>
    {
        self.execute(response)
    }
}

pub struct ProviderLoopbackAdapter {
    provider_id: String,
    client: ProviderLoopbackHttpClient,
    original_material_store: Option<Arc<dyn ProviderOriginalMaterialStore>>,
    pending_original_material_refs: Option<ProviderOriginalMaterialRefsV1>,
    pending_response_context_lease: Option<ProviderResponseContextLease>,
}

impl fmt::Debug for ProviderLoopbackAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderLoopbackAdapter")
            .field("provider_id", &self.provider_id)
            .finish_non_exhaustive()
    }
}

impl ProviderLoopbackAdapter {
    pub fn new(url: &str, tok: Option<&str>, ms: u64) -> HttpResult<Self> {
        Self::new_with_transport(url, tok, ms, "loopback_http")
    }

    pub fn new_with_transport(url: &str, tok: Option<&str>, ms: u64, tr: &str) -> HttpResult<Self> {
        let client = ProviderLoopbackHttpClient::new_with_transport(url, tok, ms, tr)?;
        Ok(Self::with_client(DEFAULT_ID, client))
    }

    pub fn with_client(id: impl Into<String>, client: ProviderLoopbackHttpClient) -> Self {
        let s = id.into();
        let provider_id = if s.trim().is_empty() {
            DEFAULT_ID.into()
        } else {
            s
        };
        let (
            original_material_store,
            pending_original_material_refs,
            pending_response_context_lease,
        ) = (None, None, None);
        Self {
            provider_id,
            client,
            original_material_store,
            pending_original_material_refs,
            pending_response_context_lease,
        }
    }

    pub fn with_original_material_store(
        mut self,
        s: Arc<dyn ProviderOriginalMaterialStore>,
    ) -> Self {
        self.original_material_store = Some(s);
        self
    }

    pub fn phase1_allowed_action_refs() -> &'static [&'static str] {
        PHASE1_REFS
    }

    fn has_ref(req: &DecisionRequest, r: &str) -> bool {
        req.observation
            .action_catalog
            .iter()
            .any(|e| e.action_ref == r)
    }

    fn validate_response(&self, req: &DecisionRequest, resp: &DecisionResponse) -> DpResult<()> {
        if let Some(e) = &resp.provider_error {
            let tr = resp.trace_payload.upstream_trace.clone();
            return Err(DecisionProviderError::new_with_upstream_trace(
                &e.code,
                &e.message,
                e.retryable,
                tr,
            ));
        }
        let is_typed_mc = matches!(
            resp.decision,
            ProviderDecision::ModuleCommandResponse { .. }
        );
        if !is_typed_mc && let Some(mc) = resp.module_command.as_ref() {
            validate_module_cmd(req, mc)?;
            return dp_chk(false, "module_command_unroutable");
        }
        match &resp.decision {
            ProviderDecision::ModuleCommandResponse { response: cmd } => {
                dp_chk(resp.module_command.is_none(), "module_command_ambiguous")?;
                let cat = req
                    .capability_catalog
                    .as_ref()
                    .ok_or_else(|| reject("module_command_catalog_unavailable", "no catalog"))?;
                let ctx = req
                    .capability_invocation_context
                    .as_ref()
                    .ok_or_else(|| reject("module_command_context_unavailable", "no ctx"))?;
                cmd.validate()
                    .map_err(|e| reject("module_command_response_invalid", e.to_string()))?;
                let ok = cmd.matches_catalog(cat)
                    && cmd.subject == ctx.subject
                    && cmd.presenter == ctx.presenter
                    && cmd.audience == ctx.audience
                    && cmd.catalog_snapshot_id == ctx.catalog_snapshot_id
                    && cmd.response_nonce == ctx.response_nonce
                    && cmd.selected_entry.module_id == ctx.module_id
                    && cmd.selected_entry.module_version == ctx.module_version;
                dp_chk(ok, "module_command_context_mismatch")
            }
            ProviderDecision::Wait => Ok(()),
            ProviderDecision::WaitTicks { .. } => dp_chk(
                Self::has_ref(req, "wait_ticks"),
                "action_ref_not_in_catalog",
            ),
            ProviderDecision::Act {
                action_ref: ar,
                action,
            } => {
                dp_chk(Self::has_ref(req, ar), "action_ref_not_in_catalog")?;
                dp_chk(PHASE1_REFS.contains(&ar.as_str()), "action_ref_not_allowed")?;
                let exp = resolved_action_ref(action)
                    .ok_or_else(|| reject("action_kind_not_supported", ar))?;
                dp_chk(exp == ar, "action_ref_mismatch")
            }
            ProviderDecision::Query {
                query_ref: qr,
                query,
            } => {
                dp_chk(Self::has_ref(req, qr), "action_ref_not_in_catalog")?;
                dp_chk(resolved_query_ref(query) == qr, "query_ref_mismatch")
            }
            ProviderDecision::ModuleCommand { module_command } => {
                validate_module_cmd(req, module_command)?;
                dp_chk(false, "module_command_unroutable")
            }
        }
    }

    pub fn execute_trusted_module_command<E: TrustedModuleCommandExecutor>(
        &self,
        request: &DecisionRequest,
        response: DecisionResponse,
        executor: &mut E,
    ) -> Result<oasis7_runtime::CapabilityAuthorizationAuditReceipt, DecisionProviderError> {
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
        use ProviderLoopbackHttpError::*;
        match error {
            InvalidBaseUrl(d) => reject("provider_config_invalid", d),
            RequestFailed { detail, .. } => {
                DecisionProviderError::new("provider_unreachable", detail, true)
            }
            Unauthorized { detail, .. } => reject("provider_unauthorized", detail),
            UnexpectedStatus {
                status_code: c,
                body: b,
                ..
            } => {
                let msg = if b.is_empty() {
                    format!("HTTP {c}")
                } else {
                    format!("HTTP {c}: {b}")
                };
                DecisionProviderError::new(format!("provider_http_{c}"), msg, c >= 500)
            }
            DecodeFailed { detail, .. } => reject("provider_payload_invalid", detail),
        }
    }

    fn ack(res: HttpResult<ProviderFeedbackAck>) -> DpResult<()> {
        let ack = res.map_err(Self::map_http_error)?;
        if ack.ok {
            Ok(())
        } else {
            let c = ack.error_code.unwrap_or_else(|| "feedback_rejected".into());
            bail(c, ack.error.unwrap_or_else(|| "rejected".into()))
        }
    }
}

fn validate_module_cmd(req: &DecisionRequest, cmd: &ProviderModuleCommand) -> DpResult<()> {
    let decl = ModuleCommandDeclaration {
        namespace: cmd.namespace.clone(),
        name: cmd.name.clone(),
        schema_version: cmd.schema_version,
        schema_hash: cmd.schema_hash.clone(),
        max_payload_bytes: cmd.payload.len().max(1) as u64,
    };
    if let Err(err) = validate_module_command_declarations(&ModuleSchemaDeclarations {
        commands: vec![decl],
    }) {
        let c = if matches!(err, ModuleCommandValidationError::ReservedNamespace(_)) {
            "module_command_reserved_namespace"
        } else {
            "module_command_invalid_declaration"
        };
        return bail(c, err.to_string());
    }
    let cat = &req.observation.module_command_catalog;
    let entry = cat
        .iter()
        .find(|e| {
            e.module_id == cmd.module_id
                && e.module_version == cmd.module_version
                && e.namespace == cmd.namespace
                && e.name == cmd.name
                && e.schema_version == cmd.schema_version
        })
        .ok_or_else(|| reject("module_command_identity_mismatch", "not in catalog"))?;
    dp_chk(
        entry.schema_hash == cmd.schema_hash,
        "module_command_schema_hash_mismatch",
    )?;
    let len = cmd.payload.len();
    dp_chk(
        len > 0 && len <= entry.max_payload_bytes as usize,
        "module_command_payload_too_large",
    )
}

impl DecisionProvider for ProviderLoopbackAdapter {
    fn provider_id(&self) -> &str {
        self.provider_id.as_str()
    }

    fn decide(&mut self, req: &DecisionRequest) -> DpResult<DecisionResponse> {
        self.pending_original_material_refs = None;
        self.pending_response_context_lease = None;
        let resp = self
            .client
            .request_decision(req)
            .map_err(Self::map_http_error)?;
        self.validate_response(req, &resp)?;
        Ok(resp)
    }

    fn decide_with_continuous_request_context(
        &mut self,
        req: &DecisionRequest,
        tc: &ContinuousAgentTurnContextV1,
        rc: &ContinuousAgentRequestContextV1,
    ) -> DpResult<ContinuousAgentResponseContextV1> {
        self.pending_original_material_refs = None;
        self.pending_response_context_lease = None;
        rc.validate_production_lane()
            .map_err(|e| reject(e.code(), e.message()))?;
        let same_turn = rc.base_decision_request == *req
            && rc.agent_session_id == tc.agent_session_id
            && rc.agent_turn_id == tc.agent_turn_id
            && rc.decision_request_id == tc.decision_request_id
            && rc.request_digest == tc.request_digest;
        dp_chk(same_turn, "request_context_identity_mismatch")?;
        let resp = if let Some(store) = self.original_material_store.as_deref() {
            let retained =
                |e: ProviderLoopbackHttpError| reject("provider_attempt_retained", e.to_string());
            let (r, refs, lease) = self
                .client
                .request_decision_with_context_and_material_store(rc, store)
                .map_err(retained)?;
            self.pending_original_material_refs = Some(refs);
            self.pending_response_context_lease = Some(lease);
            r
        } else {
            self.client
                .request_decision_with_context(rc)
                .map_err(Self::map_http_error)?
        };
        let same_resp = resp.context_discriminator == CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR
            && resp.context_version == CONTINUOUS_AGENT_CONTEXT_VERSION
            && resp.agent_session_id == rc.agent_session_id
            && resp.agent_turn_id == rc.agent_turn_id
            && resp.decision_request_id == rc.decision_request_id
            && resp.retry_seq == rc.retry_seq
            && resp.transport_attempt == rc.transport_attempt
            && resp.request_digest == rc.request_digest;
        dp_chk(same_resp, "response_identity_mismatch")?;
        match classify_cognition_response_digest(
            &resp.base_decision_response,
            &resp.response_digest,
        ) {
            CognitionResponseDigestDisposition::Current => {}
            CognitionResponseDigestDisposition::Legacy => {
                dp_chk(false, "legacy_response_digest_unsupported")?
            }
            CognitionResponseDigestDisposition::Mismatch => {
                dp_chk(false, "response_digest_mismatch")?
            }
        }
        let id = resp.response_artifact_identity();
        resp.validate_response_artifact_identity(&id)
            .map_err(|e| reject(e.code(), e.message()))?;
        self.validate_response(req, &resp.base_decision_response)?;
        Ok(resp)
    }

    fn take_provider_original_material_refs(&mut self) -> Option<ProviderOriginalMaterialRefsV1> {
        self.pending_original_material_refs.take()
    }

    fn take_provider_response_context_lease(&mut self) -> Option<ProviderResponseContextLease> {
        self.pending_response_context_lease.take()
    }

    fn push_feedback(&mut self, fb: &FeedbackEnvelope) -> DpResult<()> {
        Self::ack(self.client.submit_feedback(fb))
    }

    fn push_continuous_feedback(&mut self, fb: &FeedbackEnvelopeV1) -> DpResult<()> {
        Self::ack(self.client.submit_feedback_context(fb))
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

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_agent_api::{ProviderDiagnostics, ProviderTraceEnvelope};
    use std::{io::Read, io::Write, net::TcpListener};

    fn serve_once(status: &str, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let (addr, st) = (listener.local_addr().unwrap(), status.to_string());
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let _ = s.read(&mut [0u8; 4096]);
                let hdr = format!(
                    "HTTP/1.1 {st}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = s
                    .write_all(hdr.as_bytes())
                    .and_then(|_| s.write_all(body.as_bytes()));
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn loopback_adapter_validates_url_auth_and_catalog_over_local_tcp() {
        assert!(ProviderLoopbackAdapter::new("http://example.com:8080", None, 500).is_err());
        let raw = r#"{"observation":{"agent_id":"a1","world_time":1,"observation":{"self_state":{"location_ref":"l1","pose_hint":"idle"},"mission_context":{"goal_summary":"g"}},"action_catalog":[{"action_ref":"wait","summary":"idle"}],"timeout_budget_ms":1000},"timeout_budget_ms":1000}"#;
        let req: DecisionRequest = serde_json::from_str(raw).unwrap();
        let resp = DecisionResponse {
            decision: ProviderDecision::Wait,
            module_command: None,
            provider_error: None,
            diagnostics: ProviderDiagnostics::default(),
            trace_payload: ProviderTraceEnvelope::default(),
            memory_write_intents: vec![],
        };
        let ok_url = serve_once("200 OK", serde_json::to_string(&resp).unwrap());
        let mut ok = ProviderLoopbackAdapter::new(&ok_url, Some("tok"), 1000).unwrap();
        assert_eq!(ok.decide(&req).unwrap(), resp);

        let bad_url = serve_once("401 Unauthorized", "denied".into());
        let err = ProviderLoopbackAdapter::new(&bad_url, None, 1000)
            .unwrap()
            .decide(&req)
            .unwrap_err();
        assert_eq!(
            (err.code.as_str(), err.retryable),
            ("provider_unauthorized", false)
        );
    }
}
