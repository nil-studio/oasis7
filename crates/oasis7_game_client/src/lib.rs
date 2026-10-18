//! Dependency-light bounded World Service HTTP transport.
use oasis7_client_api::world_service::{self as authority, *};
use serde::{Serialize, de::DeserializeOwned};
use std::{io::Read, time::Duration};
mod cooldown;
pub use cooldown::WorldServiceQueryState;
#[derive(Clone)]
pub struct WorldServiceClientConfig {
    pub endpoint: String,
    pub trusted_service_public_key: String,
    pub expected_world: WorldIdentity,
    pub scope_id: String,
    pub read_private_key_hex: String,
    pub timeout: Duration,
    pub max_response_bytes: usize,
}

impl std::fmt::Debug for WorldServiceClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldServiceClientConfig")
            .field("endpoint", &self.endpoint)
            .field("expected_world", &self.expected_world)
            .field("scope_id", &self.scope_id)
            .finish_non_exhaustive()
    }
}
impl WorldServiceClientConfig {
    pub fn read_authority_identity(&self) -> Result<String, String> {
        authority::request_digest(
            "local-read-authority",
            &(
                &self.endpoint,
                &self.trusted_service_public_key,
                &self.expected_world,
                &self.scope_id,
                &self.read_private_key_hex,
            ),
        )
    }

    pub fn from_env() -> Result<Option<Self>, String> {
        let Ok(endpoint) = std::env::var("OASIS7_WORLD_SERVICE_ENDPOINT") else {
            return Ok(None);
        };
        let required = |name: &str| std::env::var(name).map_err(|_| format!("missing {name}"));
        let timeout = match std::env::var("OASIS7_WORLD_SERVICE_TIMEOUT_MS") {
            Ok(value) => value
                .bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| value.parse::<u64>().ok())
                .flatten()
                .filter(|milliseconds| (100..=30_000).contains(milliseconds))
                .map(Duration::from_millis)
                .ok_or("OASIS7_WORLD_SERVICE_TIMEOUT_MS must be an integer from 100 to 30000")?,
            Err(std::env::VarError::NotPresent) => Duration::from_millis(1500),
            Err(_) => return Err("OASIS7_WORLD_SERVICE_TIMEOUT_MS is not valid Unicode".into()),
        };
        Ok(Some(Self {
            endpoint,
            trusted_service_public_key: required("OASIS7_WORLD_SERVICE_PUBLIC_KEY")?,
            expected_world: WorldIdentity {
                world_id: required("OASIS7_WORLD_SERVICE_WORLD_ID")?,
                genesis_digest: required("OASIS7_WORLD_SERVICE_GENESIS_DIGEST")?,
            },
            scope_id: required("OASIS7_WORLD_SERVICE_SCOPE")?,
            read_private_key_hex: required("OASIS7_WORLD_SERVICE_READ_PRIVATE_KEY")?,
            timeout,
            max_response_bytes: 1_048_576,
        }))
    }
}

#[derive(Debug)]
pub enum WorldServiceClientError {
    Configuration(String),
    Transport(String),
    Rejected { status: u16, message: String },
    Assurance(String),
    Cooldown { status: u16, retry_after: Duration },
}
impl std::fmt::Display for WorldServiceClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WorldServiceClientError {}

#[derive(Clone)]
pub struct WorldServiceTransport {
    config: WorldServiceClientConfig,
    http: reqwest::blocking::Client,
    query_state: WorldServiceQueryState,
}
impl WorldServiceTransport {
    pub fn new(config: WorldServiceClientConfig) -> Result<Self, WorldServiceClientError> {
        config
            .expected_world
            .validate()
            .map_err(|e| WorldServiceClientError::Configuration(e.to_string()))?;
        let endpoint = reqwest::Url::parse(&config.endpoint)
            .map_err(|e| WorldServiceClientError::Configuration(e.to_string()))?;
        if !["http", "https"].contains(&endpoint.scheme())
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || config.scope_id.trim().is_empty()
            || config.timeout.is_zero()
            || config.max_response_bytes == 0
        {
            return Err(WorldServiceClientError::Configuration(
                "invalid service connection".into(),
            ));
        }
        // Decode and validate configured trust/signer before any request.
        let proof = authority::sign_read_request(DESCRIBE_PATH, (), &config.read_private_key_hex)
            .map_err(WorldServiceClientError::Configuration)?;
        authority::verify_read_request(DESCRIBE_PATH, &proof)
            .map_err(WorldServiceClientError::Configuration)?;
        if hex::decode(&config.trusted_service_public_key).map_or(true, |key| key.len() != 32) {
            return Err(WorldServiceClientError::Configuration(
                "invalid service trust key".into(),
            ));
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| WorldServiceClientError::Configuration(e.to_string()))?;
        Ok(Self {
            config,
            http,
            query_state: WorldServiceQueryState::default(),
        })
    }
    pub fn with_query_state(mut self, state: WorldServiceQueryState) -> Self {
        self.query_state = state;
        self
    }
    pub fn config(&self) -> &WorldServiceClientConfig {
        &self.config
    }
    pub fn private_connection(&self) -> Result<reqwest::blocking::Client, WorldServiceClientError> {
        let endpoint = reqwest::Url::parse(&self.config.endpoint).map_err(|_| {
            WorldServiceClientError::Configuration("invalid private endpoint".into())
        })?;
        let host = endpoint
            .host_str()
            .unwrap_or_default()
            .trim_start_matches('[')
            .trim_end_matches(']');
        if endpoint.scheme() != "https"
            && !(endpoint.scheme() == "http"
                && host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback()))
        {
            return Err(WorldServiceClientError::Configuration(
                "private goals require HTTPS or a literal loopback HTTP address".into(),
            ));
        }
        if self.config.trusted_service_public_key.len() != 64
            || !self
                .config
                .trusted_service_public_key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(WorldServiceClientError::Configuration(
                "noncanonical private service trust".into(),
            ));
        }
        let key: [u8; 32] = hex::decode(&self.config.trusted_service_public_key)
            .map_err(|_| {
                WorldServiceClientError::Configuration("invalid private service trust".into())
            })?
            .try_into()
            .map_err(|_| {
                WorldServiceClientError::Configuration("invalid private service trust".into())
            })?;
        if ed25519_dalek::VerifyingKey::from_bytes(&key)
            .map_err(|_| {
                WorldServiceClientError::Configuration("invalid private service trust".into())
            })?
            .is_weak()
        {
            return Err(WorldServiceClientError::Configuration(
                "weak private service trust".into(),
            ));
        }
        reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(self.config.timeout)
            .build()
            .map_err(|_| {
                WorldServiceClientError::Configuration("private transport unavailable".into())
            })
    }
    /// Fetch confidential bytes without proxies or redirects. The caller must
    /// strictly decode and verify the exact original request's response proof.
    pub fn private_response_bytes<Q: Serialize>(
        &self,
        path: &str,
        request: &Q,
        maximum: usize,
    ) -> Result<zeroize::Zeroizing<Vec<u8>>, WorldServiceClientError> {
        let http = self.private_connection()?;
        self.query_state.check()?;
        let mut response = http
            .post(format!(
                "{}{}",
                self.config.endpoint.trim_end_matches('/'),
                path
            ))
            .json(request)
            .send()
            .map_err(|error| {
                let category = if error.is_timeout() {
                    "timed out"
                } else if error.is_connect() {
                    "connection failed"
                } else if error.is_body() {
                    "request body failed"
                } else if error.is_request() {
                    "request failed"
                } else {
                    "send failed"
                };
                WorldServiceClientError::Transport(format!("private transport {category}"))
            })?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(WorldServiceClientError::Rejected {
                status,
                message: quote_error_category(path, status, &mut response)
                    .unwrap_or("private goal request rejected")
                    .into(),
            });
        }
        let bound = self
            .config
            .max_response_bytes
            .min(maximum.min(2 * 1024 * 1024));
        let mut bytes = zeroize::Zeroizing::new(Vec::new());
        response
            .by_ref()
            .take(bound as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                WorldServiceClientError::Transport("private response read failed".into())
            })?;
        if bytes.len() > bound {
            return Err(WorldServiceClientError::Assurance(
                "private response exceeds bound".into(),
            ));
        }
        Ok(bytes)
    }
    /// Strictly decode a confidential envelope. The caller must verify its
    /// typed proof against the exact original request before using its payload.
    pub fn private_signed_response<Q: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        request: &Q,
        maximum: usize,
    ) -> Result<SignedServiceResponse<R>, WorldServiceClientError> {
        let bound = self
            .config
            .max_response_bytes
            .min(maximum.min(2 * 1024 * 1024));
        let bytes = self.private_response_bytes(path, request, maximum)?;
        let value: serde_json::Value = oasis7_agent_api::game::parse_bounded_json(&bytes, bound)
            .map_err(|_| WorldServiceClientError::Assurance("invalid private response".into()))?;
        use serde::de::IntoDeserializer;
        let mut ignored = false;
        let signed = serde_ignored::deserialize(value.into_deserializer(), |_| ignored = true)
            .map_err(|_| WorldServiceClientError::Assurance("invalid private response".into()))?;
        if ignored {
            return Err(WorldServiceClientError::Assurance(
                "unsupported private response field".into(),
            ));
        }
        Ok(signed)
    }
    /// Authenticate the response envelope and bind it to this request/path.
    /// Callers must validate world, scope, evidence policy, correlation and payload
    /// semantics before using the response as authority.
    pub fn call<Q: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        request: &Q,
    ) -> Result<R, WorldServiceClientError> {
        if path != SUBMIT_PATH {
            self.query_state.check()?;
        }
        let digest =
            authority::request_digest(path, request).map_err(WorldServiceClientError::Assurance)?;
        let mut response = self
            .http
            .post(format!(
                "{}{}",
                self.config.endpoint.trim_end_matches('/'),
                path
            ))
            .json(request)
            .send()
            .map_err(|e| WorldServiceClientError::Transport(e.to_string()))?;
        let status = response.status().as_u16();
        if let Some(retry_after) = self.query_state.observe(
            status,
            response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
        )? {
            return Err(WorldServiceClientError::Cooldown {
                status,
                retry_after,
            });
        }
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take((self.config.max_response_bytes as u64).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| WorldServiceClientError::Transport(e.to_string()))?;
        if bytes.len() > self.config.max_response_bytes {
            return Err(WorldServiceClientError::Assurance(
                "service response exceeds bound".into(),
            ));
        }
        if status != 200 {
            // Surface only a reason authenticated for this exact operation and
            // request by the configured service. Untrusted error bodies remain
            // generic and cannot become signed service assertions.
            let message =
                serde_json::from_slice::<SignedServiceResponse<serde_json::Value>>(&bytes)
                    .ok()
                    .filter(|reply| {
                        authority::verify_service_response(
                            path,
                            &digest,
                            reply,
                            &self.config.trusted_service_public_key,
                        )
                        .is_ok()
                    })
                    .and_then(|reply| {
                        reply
                            .payload
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                            .map(|message| message.chars().take(1024).collect::<String>())
                    })
                    .unwrap_or_else(|| "service rejected request".into());
            return Err(WorldServiceClientError::Rejected { status, message });
        }
        let signed: SignedServiceResponse<R> = serde_json::from_slice(&bytes)
            .map_err(|e| WorldServiceClientError::Assurance(e.to_string()))?;
        authority::verify_service_response(
            path,
            &digest,
            &signed,
            &self.config.trusted_service_public_key,
        )
        .map_err(WorldServiceClientError::Assurance)?;
        Ok(signed.payload)
    }
    pub fn signed_call<Q: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        request: Q,
    ) -> Result<R, WorldServiceClientError> {
        let signed = authority::sign_read_request(path, request, &self.config.read_private_key_hex)
            .map_err(WorldServiceClientError::Assurance)?;
        self.call(path, &signed)
    }
}

pub use oasis7_client_storage::private_key_file;
#[cfg(unix)]
pub use oasis7_client_storage::private_storage;

mod task_client;
pub use task_client::WorldServiceTaskClient;

pub use oasis7_client_storage::operation_store;

mod verified_task_view;
pub use verified_task_view::{
    AuthorizedFeedbackRecord, TaskWorldProjection, VerifiedTaskWorldView,
};

pub mod commands;
pub mod device_client;

#[cfg(test)]
mod tests;

mod feedback;
#[cfg(test)]
mod transport_error_tests;

// Private error bodies are never retained. Only route-specific fixed read categories
// can cross the transport boundary; all other errors keep the generic safe tag.
fn quote_error_category(path: &str, status: u16, reader: &mut impl Read) -> Option<&'static str> {
    use oasis7_client_api::world_service::task_context::{
        TASK_CONTEXT_READ_DELIVERY_DRIFT, TASK_CONTEXT_READ_PATH,
    };
    use oasis7_client_api::world_service::{
        task_factory_quote::*, task_recipe_quote::TASK_RECIPE_QUOTE_PATH,
    };
    if status != 409
        || ![
            TASK_FACTORY_QUOTE_PATH,
            TASK_RECIPE_QUOTE_PATH,
            TASK_CONTEXT_READ_PATH,
        ]
        .contains(&path)
    {
        return None;
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ErrorCode {
        error: String,
    }
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    reader.take(257).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 256 {
        return None;
    }
    let error: ErrorCode = serde_json::from_slice(&bytes).ok()?;
    match error.error.as_str() {
        TASK_QUOTE_BASE_DRIFT if path != TASK_CONTEXT_READ_PATH => Some(TASK_QUOTE_BASE_DRIFT),
        TASK_QUOTE_DELIVERY_DRIFT if path != TASK_CONTEXT_READ_PATH => {
            Some(TASK_QUOTE_DELIVERY_DRIFT)
        }
        TASK_CONTEXT_READ_DELIVERY_DRIFT if path == TASK_CONTEXT_READ_PATH => {
            Some(TASK_CONTEXT_READ_DELIVERY_DRIFT)
        }
        _ => None,
    }
}

pub mod formal_read;

pub mod verified_world_view;

pub mod remote_world_service;

pub mod formal_business;
pub mod world_service_client;

pub mod membership;

#[cfg(test)]
mod world_service_test_profile;

pub mod read_projection;

pub mod task_original_view;
