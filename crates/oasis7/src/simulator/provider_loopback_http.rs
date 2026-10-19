use std::error::Error;
use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder};
use reqwest::{Method, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1, DecisionRequest,
    DecisionResponse, FeedbackEnvelope, FeedbackEnvelopeV1,
};

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
        let mut builder = Client::builder().timeout(Duration::from_millis(timeout_ms.max(1)));
        if transport == LOOPBACK_HTTP_PROVIDER_TRANSPORT {
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
        let url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(|err| ProviderLoopbackHttpError::InvalidBaseUrl(err.to_string()))?;
        let mut request = self.http.request(method, url);
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
