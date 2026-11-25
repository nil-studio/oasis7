//! Native Host `DecisionProvider`, `ProviderLoopbackHttpClient`, and `ProviderLoopbackAdapter`.
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use std::{error::Error, fmt, path::Path, time::Duration};

use oasis7_agent_api::{
    ProviderAgentChatRequest, ProviderAgentChatResponse, ProviderModuleCommand,
};
use oasis7_agent_harness::{
    CognitionResponseDigestDisposition, classify_cognition_response_digest,
    cognition_response_digest,
};
use oasis7_client_api::{ProviderHealth, ProviderInfo};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_simulator::{Action, AgentQuery};
use oasis7_wasm_abi::{
    ModuleCommandDeclaration, ModuleCommandValidationError, ModuleSchemaDeclarations,
    validate_module_command_declarations,
};
use reqwest::blocking::{Body, Client, RequestBuilder};
use reqwest::{Method, StatusCode, Url, header::CONTENT_TYPE};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

pub use oasis7_agent_api::{
    CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR, CONTINUOUS_AGENT_CONTEXT_VERSION,
    CapabilityInvocationContext, ContinuousAgentRequestContextV1, ContinuousAgentTurnContextV1,
    DecisionRequest, DecisionRequestContractError, FeedbackEnvelopeV1,
};
pub use oasis7_client_api::provider::{
    PROVIDER_TRANSPORT_LOOPBACK_HTTP as LOOPBACK_HTTP_PROVIDER_TRANSPORT,
    PROVIDER_TRANSPORT_REMOTE_HTTPS as REMOTE_HTTPS_PROVIDER_TRANSPORT,
};

use crate::provider_json_admission::{
    ProviderResponseContextLease, decode_response_context_with_lease,
};
use crate::provider_material_capture::{
    ProviderOriginalMaterialRefsV1, ProviderOriginalMaterialStore,
};
use crate::simulator_archive::archive::WorldEvent;

pub type ProviderDecision = oasis7_agent_api::ProviderDecision<Action, AgentQuery>;
pub type DecisionResponse = oasis7_agent_api::DecisionResponse<Action, AgentQuery>;
pub type ContinuousAgentResponseContextV1 =
    oasis7_agent_api::ContinuousAgentResponseContextV1<Action, AgentQuery>;
pub type FeedbackEnvelope = oasis7_agent_api::FeedbackEnvelope<WorldEvent>;

const COPY_BUF: usize = 64 * 1024;
const CTX_PATH: &str = "/v1/world-simulator/decision-context";
const JSON_CT: &str = "application/json";
const DEFAULT_ID: &str = "provider_loopback_http";
const PHASE1_REFS: &[&str] = &[
    "wait",
    "wait_ticks",
    "move_agent",
    "speak_to_nearby",
    "inspect_target",
    "simple_interact",
];
static NEXT_SPOOL_ID: AtomicU64 = AtomicU64::new(1);
type DpResult<T> = Result<T, DecisionProviderError>;
type HttpResult<T> = Result<T, ProviderLoopbackHttpError>;
type CapturedResult = (
    ContinuousAgentResponseContextV1,
    ProviderOriginalMaterialRefsV1,
    ProviderResponseContextLease,
);

trait CtxRes<T> {
    fn ctx(self, tag: &str) -> HttpResult<T>;
}
impl<T, E: fmt::Display> CtxRes<T> for Result<T, E> {
    fn ctx(self, tag: &str) -> HttpResult<T> {
        self.map_err(|e| req_err(CTX_PATH, format!("{tag}: {e}")))
    }
}

fn open_exclusive(path: &Path) -> io::Result<Option<File>> {
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    match opts.open(path) {
        Ok(file) => {
            if let Err(err) = std::fs::remove_file(path) {
                drop(file);
                let _ = std::fs::remove_file(path);
                return Err(err);
            }
            Ok(Some(file))
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(None),
        Err(e) => Err(e),
    }
}

struct ProviderBodySpool {
    file: File,
    length: u64,
    limit: u64,
}

impl ProviderBodySpool {
    fn new(limit: u64) -> io::Result<Self> {
        let pid = std::process::id();
        for _ in 0..32 {
            let seq = NEXT_SPOOL_ID.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!("oasis7-provider-{pid}-{seq}.spool"));
            if let Some(file) = open_exclusive(&p)? {
                return Ok(Self {
                    file,
                    length: 0,
                    limit,
                });
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "full"))
    }

    fn rewind(&mut self) -> io::Result<&mut File> {
        self.file.flush()?;
        self.file.seek(SeekFrom::Start(0))?;
        Ok(&mut self.file)
    }

    fn into_file(mut self) -> io::Result<File> {
        self.rewind()?;
        Ok(self.file)
    }

    fn excerpt(&mut self) -> String {
        let mut buf = [0u8; 4096];
        let n = self.rewind().and_then(|f| f.read(&mut buf)).unwrap_or(0);
        String::from_utf8_lossy(&buf[..n]).trim().to_string()
    }

    fn digest(&mut self) -> io::Result<[u8; 32]> {
        let mut h = blake3::Hasher::new();
        io::copy(self.rewind()?, &mut h)?;
        Ok(*h.finalize().as_bytes())
    }
}

impl Write for ProviderBodySpool {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if b.len() as u64 > self.limit.saturating_sub(self.length) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "limit"));
        }
        let n = self.file.write(b)?;
        self.length = self.length.saturating_add(n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn req_err(p: &str, d: impl Into<String>) -> ProviderLoopbackHttpError {
    let (path, detail) = (p.into(), d.into());
    ProviderLoopbackHttpError::RequestFailed { path, detail }
}

fn dec_err(e: impl fmt::Display) -> ProviderLoopbackHttpError {
    let (path, detail) = (CTX_PATH.into(), e.to_string());
    ProviderLoopbackHttpError::DecodeFailed { path, detail }
}

fn http_chk(c: bool, msg: &str) -> HttpResult<()> {
    c.then_some(()).ok_or_else(|| req_err(CTX_PATH, msg))
}

fn check_status(st: StatusCode, p: &str, body_fn: impl FnOnce() -> String) -> HttpResult<()> {
    if st == StatusCode::UNAUTHORIZED {
        let b = body_fn();
        let (path, detail) = (p.into(), if b.is_empty() { "HTTP 401".into() } else { b });
        return Err(ProviderLoopbackHttpError::Unauthorized { path, detail });
    }
    if !st.is_success() {
        let (path, sc, body) = (p.into(), st.as_u16(), body_fn());
        return Err(ProviderLoopbackHttpError::UnexpectedStatus {
            path,
            status_code: sc,
            body,
        });
    }
    Ok(())
}

fn reject(code: impl Into<String>, msg: impl Into<String>) -> DecisionProviderError {
    DecisionProviderError::new(code, msg, false)
}

fn bail<T>(code: impl Into<String>, msg: impl Into<String>) -> DpResult<T> {
    Err(reject(code, msg))
}

fn dp_chk(cond: bool, code: &str) -> DpResult<()> {
    if cond { Ok(()) } else { bail(code, code) }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderFeedbackAck {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum ProviderLoopbackHttpError {
    InvalidBaseUrl(String),
    RequestFailed {
        path: String,
        detail: String,
    },
    Unauthorized {
        path: String,
        detail: String,
    },
    UnexpectedStatus {
        path: String,
        status_code: u16,
        body: String,
    },
    DecodeFailed {
        path: String,
        detail: String,
    },
}

impl ProviderLoopbackHttpError {
    pub fn retryable(&self) -> bool {
        use ProviderLoopbackHttpError::*;
        match self {
            RequestFailed { .. } => true,
            UnexpectedStatus { status_code: c, .. } => *c == 429 || *c >= 500,
            InvalidBaseUrl(_) | Unauthorized { .. } | DecodeFailed { .. } => false,
        }
    }
}

impl fmt::Display for ProviderLoopbackHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ProviderLoopbackHttpError::*;
        match self {
            InvalidBaseUrl(d) => write!(f, "invalid provider base url: {d}"),
            RequestFailed { path: p, detail: d } => write!(f, "provider {p} failed: {d}"),
            Unauthorized { path: p, detail: d } => write!(f, "provider {p} 401: {d}"),
            UnexpectedStatus {
                path: p,
                status_code: c,
                body: b,
            } => write!(f, "provider {p} HTTP {c}: {b}"),
            DecodeFailed { path: p, detail: d } => write!(f, "provider {p} decode: {d}"),
        }
    }
}

impl Error for ProviderLoopbackHttpError {}

#[derive(Debug)]
pub struct ProviderLoopbackHttpClient {
    base_url: Url,
    auth_token: Option<String>,
    http: Client,
    captured_http: OnceLock<Result<Client, String>>,
    timeout: Duration,
    no_proxy: bool,
}

impl ProviderLoopbackHttpClient {
    pub fn new(url: &str, tok: Option<&str>, ms: u64) -> HttpResult<Self> {
        Self::new_with_transport(url, tok, ms, LOOPBACK_HTTP_PROVIDER_TRANSPORT)
    }

    pub fn new_with_transport(url: &str, tok: Option<&str>, ms: u64, tr: &str) -> HttpResult<Self> {
        let tr = tr.trim();
        let base_url = validate_provider_http_base_url(url, tr)?;
        let timeout = Duration::from_millis(ms.max(1));
        let no_proxy = tr == LOOPBACK_HTTP_PROVIDER_TRANSPORT;
        let mut b = Client::builder().timeout(timeout);
        if no_proxy {
            b = b.no_proxy();
        }
        let http = b.build().map_err(|e| req_err("<client>", e.to_string()))?;
        let auth_token = tok
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from);
        Ok(Self {
            base_url,
            auth_token,
            http,
            captured_http: OnceLock::new(),
            timeout,
            no_proxy,
        })
    }

    pub fn provider_info(&self) -> HttpResult<ProviderInfo> {
        self.get_json("/v1/provider/info")
    }

    pub fn provider_health(&self) -> HttpResult<ProviderHealth> {
        self.get_json("/v1/provider/health")
    }

    pub fn request_decision(&self, req: &DecisionRequest) -> HttpResult<DecisionResponse> {
        self.post_json("/v1/world-simulator/decision", req)
    }

    pub fn request_decision_with_context(
        &self,
        req: &ContinuousAgentRequestContextV1,
    ) -> HttpResult<ContinuousAgentResponseContextV1> {
        self.post_json(CTX_PATH, req)
    }

    pub fn request_decision_with_context_and_material_store(
        &self,
        req: &ContinuousAgentRequestContextV1,
        store: &dyn ProviderOriginalMaterialStore,
    ) -> HttpResult<CapturedResult> {
        let bnd = store.owner_binding();
        bnd.validate().map_err(|d| req_err(CTX_PATH, d))?;
        let same_world = bnd.world_id == req.runtime_binding.world_id;
        http_chk(same_world, "world mismatch")?;
        store.validate_request_owner(req).ctx("owner rejected")?;
        let rl = store.request_body_limit_bytes();
        let sl = store.response_body_limit_bytes();
        http_chk(rl > 0 && sl > 0, "limits must be nonzero")?;
        let _spools = store.reserve_body_spools(rl, sl).ctx("spool admission")?;
        let budget = store.memory_budget();
        let _ws = budget.reserve(COPY_BUF, MemoryClass::Ordinary).ctx("buf")?;
        http_chk(!store.cancelled(), "cancelled before encode")?;

        let mut rq = ProviderBodySpool::new(rl).ctx("req spool")?;
        serde_json::to_writer(&mut rq, req).ctx("req write")?;
        rq.flush().ctx("req flush")?;
        let req_len = rq.length;
        let req_id = rq.digest().ctx("req digest")?;
        let qf = rq.rewind().ctx("rewind")?;
        let att = store.persist_request(req, req_id, qf, req_len).ctx("req")?;
        let inv = req.provider_invocation_key().to_string();
        let ok_att = att.owner == *bnd
            && att.operation_id == inv
            && att.transport_attempt == req.transport_attempt
            && att.request_object_id == req_id;
        http_chk(ok_att, "attempt mismatch")?;
        http_chk(!store.cancelled(), "cancelled before dispatch")?;
        let req_file = rq.into_file().ctx("req transfer")?;
        let rb = self.captured_req(Method::POST, CTX_PATH)?;
        store.mark_dispatched(&att).ctx("dispatch marker")?;
        http_chk(!store.cancelled(), "cancelled before send")?;

        let mut resp = rb
            .header(CONTENT_TYPE, JSON_CT)
            .body(Body::sized(req_file, req_len))
            .send()
            .ctx("send")?;
        let sc = resp.status();
        http_chk(resp.content_length().is_none_or(|n| n <= sl), "too large")?;
        let mut rs = ProviderBodySpool::new(sl).ctx("resp spool")?;
        let mut buf = [0u8; COPY_BUF];
        loop {
            http_chk(!store.cancelled(), "cancelled while reading")?;
            let n = resp.read(&mut buf).ctx("resp read")?;
            if n == 0 {
                break;
            }
            rs.write_all(&buf[..n]).ctx("resp write")?;
        }
        rs.flush().ctx("resp flush")?;
        let resp_len = rs.length;
        let rb_id = rs.digest().ctx("resp digest")?;
        let rf = rs.rewind().ctx("rewind")?;
        let code = sc.as_u16();
        let resp_id = store
            .persist_response_body(&att, code, rb_id, rf, resp_len)
            .ctx("resp")?;
        http_chk(resp_id == rb_id, "response object mismatch")?;
        check_status(sc, CTX_PATH, || rs.excerpt())?;
        http_chk(!store.cancelled(), "cancelled before decode")?;
        let df = rs.rewind().map_err(dec_err)?;
        let cancelled = || store.cancelled();
        let (dec, lease) =
            decode_response_context_with_lease(df, sl, &budget, MemoryClass::Ordinary, cancelled)
                .map_err(dec_err)?;
        let refs = store
            .finish_response(&att, code, resp_id, &dec)
            .ctx("finish")?;
        let ok_refs = refs.owner == att.owner
            && refs.operation_id == att.operation_id
            && refs.transport_attempt == att.transport_attempt
            && refs.attempt_id == att.attempt_id
            && refs.request_object_id == att.request_object_id
            && refs.request_context_object_id == att.request_context_object_id
            && refs.response_object_id == resp_id
            && refs.response_status == code;
        http_chk(ok_refs, "refs mismatch")?;
        Ok((dec, refs, lease))
    }

    pub fn submit_feedback(&self, fb: &FeedbackEnvelope) -> HttpResult<ProviderFeedbackAck> {
        self.post_json("/v1/world-simulator/feedback", fb)
    }

    pub fn submit_feedback_context(
        &self,
        fb: &FeedbackEnvelopeV1,
    ) -> HttpResult<ProviderFeedbackAck> {
        self.post_json("/v1/world-simulator/feedback-context", fb)
    }

    pub fn submit_feedback_context_payload(&self, p: &Value) -> HttpResult<ProviderFeedbackAck> {
        self.post_json("/v1/world-simulator/feedback-context", p)
    }

    pub fn request_agent_chat(
        &self,
        req: &ProviderAgentChatRequest,
    ) -> HttpResult<ProviderAgentChatResponse> {
        self.post_json("/v1/world-simulator/agent-chat", req)
    }

    fn get_json<R: DeserializeOwned>(&self, p: &str) -> HttpResult<R> {
        self.send_json(self.req_with(&self.http, Method::GET, p)?, p)
    }

    fn post_json<Q: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        p: &str,
        body: &Q,
    ) -> HttpResult<R> {
        self.send_json(self.req_with(&self.http, Method::POST, p)?.json(body), p)
    }

    fn captured_req(&self, m: Method, p: &str) -> HttpResult<RequestBuilder> {
        let c = self.captured_http.get_or_init(|| {
            let mut b = Client::builder()
                .timeout(self.timeout)
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never());
            if self.no_proxy {
                b = b.no_proxy();
            }
            b.build().map_err(|e| e.to_string())
        });
        let client = c.as_ref().map_err(|d| req_err("<capture>", d.clone()))?;
        self.req_with(client, m, p)
    }

    fn req_with(&self, c: &Client, m: Method, p: &str) -> HttpResult<RequestBuilder> {
        let url = self
            .base_url
            .join(p.trim_start_matches('/'))
            .map_err(|e| ProviderLoopbackHttpError::InvalidBaseUrl(e.to_string()))?;
        let req = c.request(m, url);
        Ok(if let Some(t) = &self.auth_token {
            req.bearer_auth(t)
        } else {
            req
        })
    }

    fn send_json<R: DeserializeOwned>(&self, req: RequestBuilder, p: &str) -> HttpResult<R> {
        let resp = req.send().map_err(|e| req_err(p, e.to_string()))?;
        let sc = resp.status();
        let body = resp.bytes().map_err(|e| req_err(p, e.to_string()))?;
        check_status(sc, p, || String::from_utf8_lossy(&body).trim().to_string())?;
        serde_json::from_slice(&body).map_err(|e| ProviderLoopbackHttpError::DecodeFailed {
            path: p.into(),
            detail: e.to_string(),
        })
    }
}

pub fn validate_provider_http_base_url(url: &str, tr: &str) -> HttpResult<Url> {
    crate::provider_transport_url::validate_provider_http_base_url(url, tr)
        .map_err(|e| ProviderLoopbackHttpError::InvalidBaseUrl(e.0))
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionProviderError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub upstream_trace: Option<Value>,
}

impl DecisionProviderError {
    pub fn new(code: impl Into<String>, msg: impl Into<String>, retryable: bool) -> Self {
        Self::new_with_upstream_trace(code, msg, retryable, None)
    }

    pub fn new_with_upstream_trace(
        c: impl Into<String>,
        m: impl Into<String>,
        retryable: bool,
        upstream_trace: Option<Value>,
    ) -> Self {
        Self {
            code: c.into(),
            message: m.into(),
            retryable,
            upstream_trace,
        }
    }

    pub fn as_trace_message(&self) -> String {
        format!("{}: {}", self.code, self.message)
    }
}

impl fmt::Display for DecisionProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Error for DecisionProviderError {}

pub trait DecisionProvider {
    fn provider_id(&self) -> &str;
    fn decide(&mut self, request: &DecisionRequest) -> DpResult<DecisionResponse>;

    fn decide_with_continuous_context(
        &mut self,
        req: &DecisionRequest,
        ctx: &ContinuousAgentTurnContextV1,
    ) -> DpResult<ContinuousAgentResponseContextV1> {
        Ok(wrap_continuous_response(self.decide(req)?, ctx))
    }

    fn decide_with_continuous_request_context(
        &mut self,
        req: &DecisionRequest,
        tc: &ContinuousAgentTurnContextV1,
        rc: &ContinuousAgentRequestContextV1,
    ) -> DpResult<ContinuousAgentResponseContextV1> {
        validate_request_context_pair(req, tc, rc)?;
        let mut r = self.decide_with_continuous_context(req, tc)?;
        r.context_discriminator = CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into();
        r.context_version = CONTINUOUS_AGENT_CONTEXT_VERSION;
        r.agent_session_id = rc.agent_session_id.clone();
        r.agent_turn_id = rc.agent_turn_id.clone();
        r.decision_request_id = rc.decision_request_id.clone();
        r.retry_seq = rc.retry_seq;
        r.transport_attempt = rc.transport_attempt;
        r.request_digest = rc.request_digest.clone();
        Ok(r)
    }

    fn take_provider_original_material_refs(&mut self) -> Option<ProviderOriginalMaterialRefsV1> {
        None
    }

    fn take_provider_response_context_lease(&mut self) -> Option<ProviderResponseContextLease> {
        None
    }

    fn push_feedback(&mut self, _fb: &FeedbackEnvelope) -> DpResult<()> {
        Ok(())
    }

    fn push_continuous_feedback(&mut self, _fb: &FeedbackEnvelopeV1) -> DpResult<()> {
        Ok(())
    }

    fn on_world_event(&mut self, _ev: &WorldEvent) -> DpResult<()> {
        Ok(())
    }
}

pub fn wrap_continuous_response(
    resp: DecisionResponse,
    ctx: &ContinuousAgentTurnContextV1,
) -> ContinuousAgentResponseContextV1 {
    let response_digest = cognition_response_digest(&resp);
    ContinuousAgentResponseContextV1 {
        base_decision_response: resp,
        context_discriminator: CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into(),
        context_version: CONTINUOUS_AGENT_CONTEXT_VERSION,
        agent_session_id: ctx.agent_session_id.clone(),
        agent_turn_id: ctx.agent_turn_id.clone(),
        decision_request_id: ctx.decision_request_id.clone(),
        retry_seq: 0,
        transport_attempt: 0,
        request_digest: ctx.request_digest.clone(),
        response_digest,
    }
}

pub fn validate_request_context_pair(
    req: &DecisionRequest,
    tc: &ContinuousAgentTurnContextV1,
    rc: &ContinuousAgentRequestContextV1,
) -> DpResult<()> {
    rc.validate_production_lane()
        .map_err(|e| reject(e.code(), e.message()))?;
    tc.validate_for_agent(&rc.agent_subject)
        .map_err(|e| reject(e.code(), e.message()))?;
    let ok = rc.base_decision_request == *req
        && rc.agent_session_id == tc.agent_session_id
        && rc.agent_turn_id == tc.agent_turn_id
        && rc.decision_request_id == tc.decision_request_id
        && rc.request_digest == tc.request_digest
        && req.observation.agent_id == rc.agent_subject;
    dp_chk(ok, "request_context_identity_mismatch")
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
