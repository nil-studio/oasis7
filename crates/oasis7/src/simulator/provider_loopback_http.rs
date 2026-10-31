use std::error::Error;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::IpAddr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use oasis7_proto::memory_budget::MemoryClass;
use reqwest::blocking::{Body, Client, RequestBuilder};
use reqwest::{Method, StatusCode, Url, header::CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1, DecisionRequest,
    DecisionResponse, FeedbackEnvelope, FeedbackEnvelopeV1, ProviderOriginalMaterialRefsV1,
    ProviderOriginalMaterialStore,
};

const PROVIDER_CAPTURE_COPY_BYTES: usize = 64 * 1024;
static NEXT_PROVIDER_SPOOL_ID: AtomicU64 = AtomicU64::new(1);

/// An anonymous, bounded disk spool. The body is never accumulated in an
/// unbounded Vec; the capture owner separately reserves a finite spool quota.
struct ProviderBodySpool {
    file: Option<File>,
    length: u64,
    limit: u64,
}

impl ProviderBodySpool {
    fn new(limit: u64) -> std::io::Result<Self> {
        for _ in 0..32 {
            let sequence = NEXT_PROVIDER_SPOOL_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "oasis7-provider-material-{}-{sequence}.spool",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => {
                    // Keep only the open handle. A crash cannot leave prompt or
                    // response bytes behind in the system temp directory.
                    if let Err(error) = std::fs::remove_file(&path) {
                        drop(file);
                        let _ = std::fs::remove_file(&path);
                        return Err(error);
                    }
                    return Ok(Self {
                        file: Some(file),
                        length: 0,
                        limit,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "provider body spool name space exhausted",
        ))
    }

    fn length(&self) -> u64 {
        self.length
    }

    fn rewind(&mut self) -> std::io::Result<&mut File> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("provider body spool already transferred"))?;
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        Ok(file)
    }

    fn into_file(mut self) -> std::io::Result<File> {
        let mut file = self
            .file
            .take()
            .ok_or_else(|| std::io::Error::other("provider body spool already transferred"))?;
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        Ok(file)
    }
}

fn provider_spool_excerpt(spool: &mut ProviderBodySpool) -> String {
    let mut excerpt = [0u8; 4096];
    match spool.rewind() {
        Ok(file) => match file.read(&mut excerpt) {
            Ok(count) => String::from_utf8_lossy(&excerpt[..count])
                .trim()
                .to_string(),
            Err(_) => String::new(),
        },
        Err(_) => String::new(),
    }
}

fn provider_spool_digest(
    spool: &mut ProviderBodySpool,
    buffer: &mut [u8],
) -> std::io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    let file = spool.rewind()?;
    loop {
        let read = file.read(buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(*hasher.finalize().as_bytes())
}

impl Write for ProviderBodySpool {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("provider body spool already transferred"))?;
        let remaining = self.limit.saturating_sub(self.length);
        if bytes.len() as u64 > remaining {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "provider body exceeds capture limit",
            ));
        }
        let written = file.write(bytes)?;
        self.length = self.length.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("provider body spool already transferred"))?
            .flush()
    }
}

pub use oasis7_client_api::{
    ProviderCompatibilityReport, ProviderCompatibilityStatus, ProviderHealth, ProviderInfo,
    evaluate_provider_compatibility, provider_phase1_required_actions,
    provider_phase1_required_capabilities,
};
pub const LOOPBACK_HTTP_PROVIDER_TRANSPORT: &str =
    oasis7_client_api::provider::PROVIDER_TRANSPORT_LOOPBACK_HTTP;
pub const REMOTE_HTTPS_PROVIDER_TRANSPORT: &str =
    oasis7_client_api::provider::PROVIDER_TRANSPORT_REMOTE_HTTPS;
pub const PROVIDER_PHASE1_ACTION_SET_ALIAS: &str =
    oasis7_client_api::provider::PROVIDER_PHASE1_ACTION_SET_ALIAS;

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn legacy_provider_surface_uses_client_api_contract() {
        assert_eq!(
            LOOPBACK_HTTP_PROVIDER_TRANSPORT,
            oasis7_client_api::provider::PROVIDER_TRANSPORT_LOOPBACK_HTTP
        );
        assert_eq!(
            REMOTE_HTTPS_PROVIDER_TRANSPORT,
            oasis7_client_api::provider::PROVIDER_TRANSPORT_REMOTE_HTTPS
        );
        assert_eq!(
            PROVIDER_PHASE1_ACTION_SET_ALIAS,
            oasis7_client_api::provider::PROVIDER_PHASE1_ACTION_SET_ALIAS
        );
        assert_eq!(
            provider_phase1_required_capabilities(),
            oasis7_client_api::provider_phase1_required_capabilities()
        );
        assert_eq!(
            provider_phase1_required_actions(),
            oasis7_client_api::provider_phase1_required_actions()
        );

        let info = ProviderInfo {
            capabilities: vec!["decision".to_string(), "feedback".to_string()],
            supported_action_sets: vec![PROVIDER_PHASE1_ACTION_SET_ALIAS.to_string()],
            chain_resource_manifest_schema_version: Some(
                "oasis7.world_resource_manifest.v1".to_string(),
            ),
            chain_resource_delta_schema_version: Some("oasis7.world_resource_delta.v1".to_string()),
            ..ProviderInfo::default()
        };
        assert_eq!(
            evaluate_provider_compatibility(&info, None),
            oasis7_client_api::evaluate_provider_compatibility(&info, None)
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderFeedbackAck {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub use oasis7_agent_api::{
    ProviderAgentChatRequest, ProviderAgentChatResponse, provider_agent_chat_log_key,
};

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
    /// Whether retrying the same provider request can plausibly succeed without
    /// changing the authenticated request identity.  Authentication, URL,
    /// and response-shape failures are durable rejections; transport and
    /// server throttling failures remain retryable.
    pub fn retryable(&self) -> bool {
        match self {
            Self::RequestFailed { .. } => true,
            Self::UnexpectedStatus { status_code, .. } => {
                *status_code == 429 || *status_code >= 500
            }
            Self::InvalidBaseUrl(_) | Self::Unauthorized { .. } | Self::DecodeFailed { .. } => {
                false
            }
        }
    }
}

impl fmt::Display for ProviderLoopbackHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBaseUrl(detail) => {
                write!(f, "invalid provider base url: {detail}")
            }
            Self::RequestFailed { path, detail } => {
                write!(f, "provider request {path} failed: {detail}")
            }
            Self::Unauthorized { path, detail } => {
                write!(f, "provider request {path} unauthorized: {detail}")
            }
            Self::UnexpectedStatus {
                path,
                status_code,
                body,
            } => write!(
                f,
                "provider request {path} returned HTTP {status_code}: {body}"
            ),
            Self::DecodeFailed { path, detail } => {
                write!(f, "decode provider response {path} failed: {detail}")
            }
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
    pub fn new(
        base_url: &str,
        auth_token: Option<&str>,
        timeout_ms: u64,
    ) -> Result<Self, ProviderLoopbackHttpError> {
        Self::new_with_transport(
            base_url,
            auth_token,
            timeout_ms,
            LOOPBACK_HTTP_PROVIDER_TRANSPORT,
        )
    }

    pub fn new_with_transport(
        base_url: &str,
        auth_token: Option<&str>,
        timeout_ms: u64,
        transport: &str,
    ) -> Result<Self, ProviderLoopbackHttpError> {
        let transport = transport.trim();
        let base_url = validate_provider_http_base_url(base_url, transport)?;
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let no_proxy = transport == LOOPBACK_HTTP_PROVIDER_TRANSPORT;
        let mut builder = Client::builder().timeout(timeout);
        if no_proxy {
            builder = builder.no_proxy();
        }
        let http = builder
            .build()
            .map_err(|err| ProviderLoopbackHttpError::RequestFailed {
                path: "<client>".to_string(),
                detail: err.to_string(),
            })?;
        Ok(Self {
            base_url,
            auth_token: auth_token
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            http,
            captured_http: OnceLock::new(),
            timeout,
            no_proxy,
        })
    }

    pub fn provider_info(&self) -> Result<ProviderInfo, ProviderLoopbackHttpError> {
        self.get_json("/v1/provider/info")
    }

    pub fn provider_health(&self) -> Result<ProviderHealth, ProviderLoopbackHttpError> {
        self.get_json("/v1/provider/health")
    }

    pub fn request_decision(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderLoopbackHttpError> {
        self.post_json("/v1/world-simulator/decision", request)
    }

    pub fn request_decision_with_context(
        &self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1, ProviderLoopbackHttpError> {
        self.post_json("/v1/world-simulator/decision-context", request)
    }

    /// Native new-format lane: persist the exact request body before dispatch,
    /// send that same bounded spool, persist the exact HTTP entity body before
    /// decoding it, and return only the resulting content references beside
    /// the existing typed response.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn request_decision_with_context_and_material_store(
        &self,
        request: &ContinuousAgentRequestContextV1,
        store: &dyn ProviderOriginalMaterialStore,
    ) -> Result<
        (
            ContinuousAgentResponseContextV1,
            ProviderOriginalMaterialRefsV1,
        ),
        ProviderLoopbackHttpError,
    > {
        let path = "/v1/world-simulator/decision-context";
        let binding = store.owner_binding();
        binding
            .validate()
            .map_err(|detail| ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: detail.into(),
            })?;
        if binding.world_id != request.runtime_binding.world_id {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: "provider request world does not match its installed pending owner".into(),
            });
        }
        store.validate_request_owner(request).map_err(|detail| {
            ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider request owner binding rejected: {detail}"),
            }
        })?;
        let request_limit = store.request_body_limit_bytes();
        let response_limit = store.response_body_limit_bytes();
        if request_limit == 0 || response_limit == 0 {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: "provider capture body limits must be nonzero".into(),
            });
        }
        let _spool_reservation = store
            .reserve_body_spools(request_limit, response_limit)
            .map_err(|detail| ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider body spool admission failed: {detail}"),
            })?;
        let _copy_workspace = store
            .memory_budget()
            .reserve(PROVIDER_CAPTURE_COPY_BYTES, MemoryClass::Ordinary)
            .map_err(|error| ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider body copy admission failed: {error}"),
            })?;
        if store.cancelled() {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: "provider material capture cancelled before request encoding".into(),
            });
        }

        let mut request_spool = ProviderBodySpool::new(request_limit).map_err(|error| {
            ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider request spool creation failed: {error}"),
            }
        })?;
        serde_json::to_writer(&mut request_spool, request).map_err(|error| {
            ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider request exceeded its bounded spool: {error}"),
            }
        })?;
        request_spool
            .flush()
            .map_err(|error| ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider request spool flush failed: {error}"),
            })?;
        let request_length = request_spool.length();
        let mut buffer = [0u8; PROVIDER_CAPTURE_COPY_BYTES];
        let request_object_id =
            provider_spool_digest(&mut request_spool, &mut buffer).map_err(|error| {
                ProviderLoopbackHttpError::RequestFailed {
                    path: path.to_string(),
                    detail: format!("provider request spool digest failed: {error}"),
                }
            })?;
        let attempt = {
            let request_file = request_spool.rewind().map_err(|error| {
                ProviderLoopbackHttpError::RequestFailed {
                    path: path.to_string(),
                    detail: format!("provider request spool rewind failed: {error}"),
                }
            })?;
            store
                .persist_request(request, request_object_id, request_file, request_length)
                .map_err(|detail| ProviderLoopbackHttpError::RequestFailed {
                    path: path.to_string(),
                    detail: format!("provider request persistence failed: {detail}"),
                })?
        };
        if attempt.owner != *binding
            || attempt.operation_id != request.provider_invocation_key().to_string()
            || attempt.transport_attempt != request.transport_attempt
            || attempt.request_object_id != request_object_id
        {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: "provider request attempt identity differs from the captured bytes".into(),
            });
        }
        if store.cancelled() {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: "provider material capture cancelled before dispatch".into(),
            });
        }
        // Resolve all local request construction and spool rewinding before
        // publishing DispatchedOrUnknown. From this point, every error is
        // non-retryable under this transport lane.
        let request_file = request_spool.into_file().map_err(|error| {
            ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider request body rewind failed: {error}"),
            }
        })?;
        let request_builder = self
            .build_captured_request(Method::POST, path)?
            .header(CONTENT_TYPE, "application/json");
        store.mark_dispatched(&attempt).map_err(|detail| {
            ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider dispatch marker persistence failed: {detail}"),
            }
        })?;
        if store.cancelled() {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail:
                    "provider material capture cancelled after dispatch marker and before HTTP send"
                        .into(),
            });
        }

        let body = Body::sized(request_file, request_length);
        let mut response = request_builder.body(body).send().map_err(|error| {
            ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: error.to_string(),
            }
        })?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > response_limit)
        {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: "provider response content length exceeds capture limit".into(),
            });
        }
        let mut response_spool = ProviderBodySpool::new(response_limit).map_err(|error| {
            ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider response spool creation failed: {error}"),
            }
        })?;
        loop {
            if store.cancelled() {
                return Err(ProviderLoopbackHttpError::RequestFailed {
                    path: path.to_string(),
                    detail: "provider material capture cancelled while reading response".into(),
                });
            }
            let read = response.read(&mut buffer).map_err(|error| {
                ProviderLoopbackHttpError::RequestFailed {
                    path: path.to_string(),
                    detail: format!("provider response body read failed: {error}"),
                }
            })?;
            if read == 0 {
                break;
            }
            response_spool.write_all(&buffer[..read]).map_err(|error| {
                ProviderLoopbackHttpError::RequestFailed {
                    path: path.to_string(),
                    detail: format!("provider response exceeded its bounded spool: {error}"),
                }
            })?;
        }
        response_spool
            .flush()
            .map_err(|error| ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider response spool flush failed: {error}"),
            })?;
        let response_length = response_spool.length();
        let response_bytes_id =
            provider_spool_digest(&mut response_spool, &mut buffer).map_err(|error| {
                ProviderLoopbackHttpError::RequestFailed {
                    path: path.to_string(),
                    detail: format!("provider response spool digest failed: {error}"),
                }
            })?;
        let response_object_id = {
            let response_file = response_spool.rewind().map_err(|error| {
                ProviderLoopbackHttpError::RequestFailed {
                    path: path.to_string(),
                    detail: format!("provider response spool rewind failed: {error}"),
                }
            })?;
            store
                .persist_response_body(
                    &attempt,
                    status.as_u16(),
                    response_bytes_id,
                    response_file,
                    response_length,
                )
                .map_err(|detail| ProviderLoopbackHttpError::RequestFailed {
                    path: path.to_string(),
                    detail: format!("provider response persistence failed: {detail}"),
                })?
        };
        if response_object_id != response_bytes_id {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: "provider response object identity differs from captured bytes".into(),
            });
        }

        if status == StatusCode::UNAUTHORIZED {
            let detail = provider_spool_excerpt(&mut response_spool);
            return Err(ProviderLoopbackHttpError::Unauthorized {
                path: path.to_string(),
                detail: if detail.is_empty() {
                    "HTTP 401".into()
                } else {
                    detail
                },
            });
        }
        if !status.is_success() {
            return Err(ProviderLoopbackHttpError::UnexpectedStatus {
                path: path.to_string(),
                status_code: status.as_u16(),
                body: provider_spool_excerpt(&mut response_spool),
            });
        }
        if store.cancelled() {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: "provider material capture cancelled before response decode".into(),
            });
        }
        let decoded: ContinuousAgentResponseContextV1 = {
            let response_file = response_spool.rewind().map_err(|error| {
                ProviderLoopbackHttpError::DecodeFailed {
                    path: path.to_string(),
                    detail: format!("provider response spool rewind failed: {error}"),
                }
            })?;
            serde_json::from_reader(response_file).map_err(|error| {
                ProviderLoopbackHttpError::DecodeFailed {
                    path: path.to_string(),
                    detail: error.to_string(),
                }
            })?
        };
        let refs = store
            .finish_response(&attempt, status.as_u16(), response_object_id, &decoded)
            .map_err(|detail| ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: format!("provider typed response persistence failed: {detail}"),
            })?;
        if refs.owner != attempt.owner
            || refs.operation_id != attempt.operation_id
            || refs.transport_attempt != attempt.transport_attempt
            || refs.attempt_id != attempt.attempt_id
            || refs.request_object_id != attempt.request_object_id
            || refs.request_context_object_id != attempt.request_context_object_id
            || refs.response_object_id != response_object_id
            || refs.response_status != status.as_u16()
        {
            return Err(ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: "provider response references do not match the captured attempt".into(),
            });
        }
        Ok((decoded, refs))
    }

    pub fn submit_feedback(
        &self,
        feedback: &FeedbackEnvelope,
    ) -> Result<ProviderFeedbackAck, ProviderLoopbackHttpError> {
        self.post_json("/v1/world-simulator/feedback", feedback)
    }

    pub fn submit_feedback_context(
        &self,
        feedback: &FeedbackEnvelopeV1,
    ) -> Result<ProviderFeedbackAck, ProviderLoopbackHttpError> {
        self.post_json("/v1/world-simulator/feedback-context", feedback)
    }

    /// Submit the Runtime outbox's canonical transport payload. Runtime
    /// feedback projections (including envelope digest and committed event
    /// summaries) are intentionally carried as JSON extensions; adapting the
    /// payload back to `FeedbackEnvelopeV1` would silently discard them.
    pub fn submit_feedback_context_payload(
        &self,
        payload: &serde_json::Value,
    ) -> Result<ProviderFeedbackAck, ProviderLoopbackHttpError> {
        self.post_json("/v1/world-simulator/feedback-context", payload)
    }

    pub fn request_agent_chat(
        &self,
        request: &ProviderAgentChatRequest,
    ) -> Result<ProviderAgentChatResponse, ProviderLoopbackHttpError> {
        self.post_json("/v1/world-simulator/agent-chat", request)
    }

    fn get_json<Response>(&self, path: &str) -> Result<Response, ProviderLoopbackHttpError>
    where
        Response: DeserializeOwned,
    {
        let request = self.build_request(Method::GET, path)?;
        self.send_json(request, path)
    }

    fn post_json<Request, Response>(
        &self,
        path: &str,
        payload: &Request,
    ) -> Result<Response, ProviderLoopbackHttpError>
    where
        Request: Serialize + ?Sized,
        Response: DeserializeOwned,
    {
        let request = self.build_request(Method::POST, path)?.json(payload);
        self.send_json(request, path)
    }

    fn build_request(
        &self,
        method: Method,
        path: &str,
    ) -> Result<RequestBuilder, ProviderLoopbackHttpError> {
        self.build_request_with_client(&self.http, method, path)
    }

    fn build_captured_request(
        &self,
        method: Method,
        path: &str,
    ) -> Result<RequestBuilder, ProviderLoopbackHttpError> {
        let capture_client = self.captured_http.get_or_init(|| {
            let mut builder = Client::builder()
                .timeout(self.timeout)
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never());
            if self.no_proxy {
                builder = builder.no_proxy();
            }
            builder.build().map_err(|error| error.to_string())
        });
        let capture_client =
            capture_client
                .as_ref()
                .map_err(|detail| ProviderLoopbackHttpError::RequestFailed {
                    path: "<capture-client>".to_string(),
                    detail: detail.clone(),
                })?;
        self.build_request_with_client(capture_client, method, path)
    }

    fn build_request_with_client(
        &self,
        client: &Client,
        method: Method,
        path: &str,
    ) -> Result<RequestBuilder, ProviderLoopbackHttpError> {
        let url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(|err| ProviderLoopbackHttpError::InvalidBaseUrl(err.to_string()))?;
        let mut request = client.request(method, url);
        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        }
        Ok(request)
    }

    fn send_json<Response>(
        &self,
        request: RequestBuilder,
        path: &str,
    ) -> Result<Response, ProviderLoopbackHttpError>
    where
        Response: DeserializeOwned,
    {
        let response = request
            .send()
            .map_err(|err| ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: err.to_string(),
            })?;
        let status = response.status();
        let body = response
            .bytes()
            .map_err(|err| ProviderLoopbackHttpError::RequestFailed {
                path: path.to_string(),
                detail: err.to_string(),
            })?;
        if status == StatusCode::UNAUTHORIZED {
            let detail = String::from_utf8_lossy(body.as_ref()).trim().to_string();
            return Err(ProviderLoopbackHttpError::Unauthorized {
                path: path.to_string(),
                detail: if detail.is_empty() {
                    "HTTP 401".to_string()
                } else {
                    detail
                },
            });
        }
        if !status.is_success() {
            return Err(ProviderLoopbackHttpError::UnexpectedStatus {
                path: path.to_string(),
                status_code: status.as_u16(),
                body: String::from_utf8_lossy(body.as_ref()).trim().to_string(),
            });
        }
        serde_json::from_slice(body.as_ref()).map_err(|err| {
            ProviderLoopbackHttpError::DecodeFailed {
                path: path.to_string(),
                detail: err.to_string(),
            }
        })
    }
}

pub fn validate_provider_loopback_http_base_url(
    base_url: &str,
) -> Result<Url, ProviderLoopbackHttpError> {
    validate_provider_http_base_url(base_url, LOOPBACK_HTTP_PROVIDER_TRANSPORT)
}

pub fn validate_provider_http_base_url(
    base_url: &str,
    transport: &str,
) -> Result<Url, ProviderLoopbackHttpError> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err(ProviderLoopbackHttpError::InvalidBaseUrl(
            "base url cannot be empty".to_string(),
        ));
    }
    let url = Url::parse(trimmed)
        .map_err(|err| ProviderLoopbackHttpError::InvalidBaseUrl(err.to_string()))?;
    let Some(host) = url.host_str() else {
        return Err(ProviderLoopbackHttpError::InvalidBaseUrl(
            "host is required".to_string(),
        ));
    };
    match transport.trim() {
        LOOPBACK_HTTP_PROVIDER_TRANSPORT => {
            if url.scheme() != "http" {
                return Err(ProviderLoopbackHttpError::InvalidBaseUrl(
                    "scheme must be http for localhost provider".to_string(),
                ));
            }
            if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
                return Err(ProviderLoopbackHttpError::InvalidBaseUrl(
                    "host must be loopback (127.0.0.1 / localhost / ::1)".to_string(),
                ));
            }
        }
        REMOTE_HTTPS_PROVIDER_TRANSPORT => {
            if url.scheme() != "https" {
                return Err(ProviderLoopbackHttpError::InvalidBaseUrl(
                    "scheme must be https for remote provider transport".to_string(),
                ));
            }
            if !is_public_remote_https_host(host) {
                return Err(ProviderLoopbackHttpError::InvalidBaseUrl(
                    "host must be a public remote https host".to_string(),
                ));
            }
        }
        _ => {
            return Err(ProviderLoopbackHttpError::InvalidBaseUrl(format!(
                "unsupported provider transport `{transport}`"
            )));
        }
    }
    Ok(url)
}

fn is_public_remote_https_host(host: &str) -> bool {
    if matches!(host.trim(), "127.0.0.1" | "localhost" | "::1") {
        return false;
    }
    let Ok(ip) = host.parse::<IpAddr>() else {
        return true;
    };
    match ip {
        IpAddr::V4(ipv4) => !ipv4.is_private() && !ipv4.is_loopback() && !ipv4.is_link_local(),
        IpAddr::V6(ipv6) => !ipv6.is_loopback() && !ipv6.is_unicast_link_local(),
    }
}
