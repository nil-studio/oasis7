//! Unwired native Host HTTP provider client and bounded original material capture.
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{error::Error, fmt, path::Path, time::Duration};

pub use oasis7_agent_api::{
    ProviderAgentChatRequest, ProviderAgentChatResponse, provider_agent_chat_log_key,
};
pub use oasis7_client_api::{
    ProviderCompatibilityReport, ProviderCompatibilityStatus, ProviderHealth, ProviderInfo,
    evaluate_provider_compatibility, provider_phase1_required_actions,
    provider_phase1_required_capabilities,
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_simulator::{Action, AgentQuery};
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
    PROVIDER_PHASE1_ACTION_SET_ALIAS,
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
static NEXT_SPOOL_ID: AtomicU64 = AtomicU64::new(1);
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
        let att = store
            .persist_request(req, req_id, qf, req_len)
            .ctx("request persistence failed")?;
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
        http_chk(
            !store.cancelled(),
            "provider material capture cancelled after dispatch marker and before HTTP send",
        )?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_admission_is_bounded_to_loopback_by_default() {
        let public = ProviderLoopbackHttpClient::new("http://example.com:8080", None, 500);
        assert!(public.is_err());
        let local = ProviderLoopbackHttpClient::new("http://127.0.0.1:8765", None, 500);
        assert!(local.is_ok());
        let minimum_timeout = ProviderLoopbackHttpClient::new("http://127.0.0.1:8765", None, 0);
        assert!(minimum_timeout.is_ok());
    }
}
