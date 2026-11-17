//! Single operation transport owner used by both signed full-sync exits.
//! Origins and trusted TLS/DNS configuration are frozen before attempt admission.
//! V2 boot and original-owner takeover require their own completed wiring.
use super::operation_ledger::{AttemptPermit, Ledger};
use super::{direct_http, tls_http};
use oasis7_game_client::{
    WorldServiceClientError,
    formal_sync_exchange::{EncodedFullSyncExchange, FullSyncExchange, FullSyncHttpResponse},
};
use rustls::{ClientConfig, pki_types::ServerName};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

struct ActiveOperation {
    ledger: Ledger,
    attempt: AttemptPermit,
}

struct ApprovedEndpoint {
    address: super::dns::Target,
    authority: String,
    tls_name: Option<ServerName<'static>>,
}

pub(super) struct SharedOperationExchange {
    active: Mutex<ActiveOperation>,
    endpoints: BTreeMap<String, ApprovedEndpoint>,
    tls_config: Option<Arc<ClientConfig>>,
}
impl SharedOperationExchange {
    #[cfg(test)]
    pub fn snapshot(&self) -> super::operation_ledger::Snapshot {
        self.active.lock().unwrap().ledger.snapshot().clone()
    }
}

/// Configuration-only factory stage. This performs local DNS configuration
/// reads before the caller persists a new attempt. It cannot issue requests,
/// create a ledger, select an operation identity, or grant takeover permission.
pub(super) struct PreparedTransport {
    binding_digest: String,
    endpoint_order: Vec<String>,
    endpoints: BTreeMap<String, ApprovedEndpoint>,
    tls_config: Option<Arc<ClientConfig>>,
}
impl PreparedTransport {
    /// Production trust is constructed here, never supplied as an opaque
    /// ClientConfig whose custom verifier could silently disable validation.
    pub fn prepare_builtin_trust(endpoints: &[String]) -> Result<Self, String> {
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let root_material: Vec<_> = roots
            .roots
            .iter()
            .map(|root| {
                (
                    root.subject.as_ref().to_vec(),
                    root.subject_public_key_info.as_ref().to_vec(),
                    root.name_constraints
                        .as_ref()
                        .map(|value| value.as_ref().to_vec()),
                )
            })
            .collect();
        let trust_digest = super::super::local_io::digest(&root_material)?;
        let provider = rustls::crypto::ring::default_provider();
        let mut config = ClientConfig::builder_with_provider(Arc::new(provider))
            .with_safe_default_protocol_versions()
            .map_err(|error| error.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Self::prepare_with_policy(endpoints, Some(Arc::new(config)), &trust_digest)
    }

    pub fn binding_digest(&self) -> &str {
        &self.binding_digest
    }

    pub fn matches_endpoints(&self, endpoints: &[String]) -> bool {
        self.endpoint_order == endpoints
    }

    #[cfg(test)]
    pub fn prepare(
        endpoints: &[String],
        tls_config: Option<Arc<ClientConfig>>,
    ) -> Result<Self, String> {
        Self::prepare_with_policy(endpoints, tls_config, "test-only-injected-trust")
    }

    fn prepare_with_policy(
        endpoints: &[String],
        tls_config: Option<Arc<ClientConfig>>,
        trust_digest: &str,
    ) -> Result<Self, String> {
        let mut approved = BTreeMap::new();
        let mut origins = std::collections::BTreeSet::new();
        let mut dns_config = None;
        if endpoints.is_empty() || endpoints.len() > 4 {
            return Err("invalid operation endpoint count".into());
        }
        for endpoint in endpoints {
            let url = reqwest::Url::parse(endpoint).map_err(|e| e.to_string())?;
            let host = url
                .host_str()
                .ok_or("missing operation endpoint host")?
                .trim_matches(['[', ']']);
            let ip = host.parse::<std::net::IpAddr>().ok();
            if !matches!(url.scheme(), "http" | "https")
                || (url.scheme() == "http" && !ip.is_some_and(|ip| ip.is_loopback()))
                || (url.scheme() == "https" && tls_config.is_none())
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || url.path() != "/"
            {
                return Err(
                    "operation adapter requires literal loopback HTTP or trusted HTTPS origin"
                        .into(),
                );
            }
            let port = url.port_or_known_default().ok_or("missing endpoint port")?;
            if port == 0 || !origins.insert((url.scheme().to_owned(), host.to_owned(), port)) {
                return Err("invalid or duplicate operation origin".into());
            }
            if ip.is_none() && dns_config.is_none() {
                dns_config = Some(super::dns::capture_system_config()?);
            }
            approved.insert(
                endpoint.clone(),
                ApprovedEndpoint {
                    address: match ip {
                        Some(ip) => super::dns::Target::Socket(SocketAddr::new(
                            ip,
                            url.port_or_known_default().ok_or("missing endpoint port")?,
                        )),
                        None => super::dns::Target::Name {
                            host: host.to_owned(),
                            port: url.port_or_known_default().ok_or("missing endpoint port")?,
                            config: dns_config.clone().ok_or("DNS config snapshot missing")?,
                        },
                    },
                    authority: match ip {
                        Some(ip) => SocketAddr::new(
                            ip,
                            url.port_or_known_default().ok_or("missing endpoint port")?,
                        )
                        .to_string(),
                        None => format!(
                            "{}:{}",
                            host,
                            url.port_or_known_default().ok_or("missing endpoint port")?
                        ),
                    },
                    tls_name: if url.scheme() == "https" {
                        Some(ServerName::try_from(host.to_owned()).map_err(|e| e.to_string())?)
                    } else {
                        None
                    },
                },
            );
        }
        let binding_digest = super::super::local_io::digest(&(
            "oasis7.full-sync-transport-binding/v1",
            endpoints,
            trust_digest,
            "http1.1;https-name-verification;bounded-dns-v1",
        ))?;
        Ok(Self {
            binding_digest,
            endpoint_order: endpoints.to_vec(),
            endpoints: approved,
            tls_config,
        })
    }

    /// Consume a durably admitted permit only after immutable preparation.
    /// An expired, recovered or poisoned ledger never becomes a live transport.
    pub fn admit(
        self,
        ledger: Ledger,
        attempt: AttemptPermit,
    ) -> Result<SharedOperationExchange, String> {
        ledger.check_attempt(&attempt)?;
        let original: serde_json::Value = serde_json::from_slice(
            ledger
                .snapshot()
                .original_request_json
                .as_deref()
                .ok_or("shared transport requires the retained original operation request")?,
        )
        .map_err(|error| error.to_string())?;
        if original
            .get("transport_binding")
            .and_then(serde_json::Value::as_str)
            != Some(self.binding_digest.as_str())
        {
            return Err("transport policy differs from the original operation request".into());
        }
        if original.get("transfer_policy")
            != Some(
                &serde_json::to_value(&ledger.snapshot().policy)
                    .map_err(|error| error.to_string())?,
            )
        {
            return Err("transfer policy differs from the original operation request".into());
        }
        if original
            .get("config")
            .and_then(|config| config.get("max_total_sync_bytes"))
            .and_then(serde_json::Value::as_u64)
            != Some(ledger.snapshot().material_limit)
        {
            return Err("material limit differs from the original operation request".into());
        }
        Ok(SharedOperationExchange {
            active: Mutex::new(ActiveOperation { ledger, attempt }),
            endpoints: self.endpoints,
            tls_config: self.tls_config,
        })
    }
}
impl FullSyncExchange for SharedOperationExchange {
    fn original_request_digest(&self) -> Result<String, WorldServiceClientError> {
        let guard = self.active.try_lock().map_err(|_| {
            WorldServiceClientError::Transport("operation occupied or poisoned".into())
        })?;
        guard
            .ledger
            .check_attempt(&guard.attempt)
            .map_err(WorldServiceClientError::Transport)?;
        Ok(guard.ledger.snapshot().identity.request_digest.clone())
    }

    fn bind_verified_target(&self, target_json: &[u8]) -> Result<(), WorldServiceClientError> {
        let mut guard = self.active.try_lock().map_err(|_| {
            WorldServiceClientError::Transport("operation occupied or poisoned".into())
        })?;
        let active = &mut *guard;
        active
            .ledger
            .bind_verified_target(&active.attempt, target_json)
            .map_err(WorldServiceClientError::Transport)
    }
    fn begin_publication(
        &self,
        cut: &serde_json::Value,
        roots_digest: &str,
    ) -> Result<(), WorldServiceClientError> {
        let mut guard = self.active.try_lock().map_err(|_| {
            WorldServiceClientError::Transport("operation occupied or poisoned".into())
        })?;
        let active = &mut *guard;
        active
            .ledger
            .begin_publication(&active.attempt, cut, roots_digest)
            .map_err(WorldServiceClientError::Transport)
    }
    fn complete_publication(
        &self,
        cut: &serde_json::Value,
        roots_digest: &str,
    ) -> Result<(), WorldServiceClientError> {
        let mut guard = self.active.try_lock().map_err(|_| {
            WorldServiceClientError::Transport("operation occupied or poisoned".into())
        })?;
        let active = &mut *guard;
        active
            .ledger
            .complete_publication(&active.attempt, cut, roots_digest)
            .map_err(WorldServiceClientError::Transport)
    }
    fn check_active(&self) -> Result<(), WorldServiceClientError> {
        let guard = self.active.try_lock().map_err(|_| {
            WorldServiceClientError::Transport("operation occupied or poisoned".into())
        })?;
        guard
            .ledger
            .check_attempt(&guard.attempt)
            .map_err(WorldServiceClientError::Transport)
    }
    fn finish_attempt(&self) -> Result<(), WorldServiceClientError> {
        let mut guard = self.active.try_lock().map_err(|_| {
            WorldServiceClientError::Transport("operation occupied or poisoned".into())
        })?;
        let active = &mut *guard;
        active
            .ledger
            .finish_attempt(&active.attempt)
            .map_err(WorldServiceClientError::Transport)?;
        if std::time::Instant::now() >= active.attempt.deadline() {
            return Err(WorldServiceClientError::Transport("attempt_expired".into()));
        }
        Ok(())
    }
    fn charge_proof(&self, bytes: u64) -> Result<(), WorldServiceClientError> {
        let mut guard = self.active.try_lock().map_err(|_| {
            WorldServiceClientError::Transport("operation occupied or poisoned".into())
        })?;
        let active = &mut *guard;
        active
            .ledger
            .charge_proof(&active.attempt, bytes)
            .map_err(WorldServiceClientError::Transport)
    }
    fn charge_material(&self, digest: &str, bytes: u64) -> Result<(), WorldServiceClientError> {
        let mut guard = self.active.try_lock().map_err(|_| {
            WorldServiceClientError::Transport("operation occupied or poisoned".into())
        })?;
        let active = &mut *guard;
        active
            .ledger
            .charge_material(&active.attempt, digest, bytes)
            .map_err(WorldServiceClientError::Transport)
    }
    fn exchange(
        &self,
        request: EncodedFullSyncExchange<'_>,
    ) -> Result<FullSyncHttpResponse, WorldServiceClientError> {
        if !matches!(
            request.path,
            oasis7_client_api::world_service::VIEW_PATH
                | oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_MANIFEST_PATH
                | oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH
        ) {
            return Err(WorldServiceClientError::Configuration(
                "unsupported full-sync operation route".into(),
            ));
        }
        let address = self.endpoints.get(request.endpoint).ok_or_else(|| {
            WorldServiceClientError::Configuration(
                "operation endpoint differs from admitted providers".into(),
            )
        })?;
        // Do not wait indefinitely behind an old owner or start parallel I/O.
        let mut guard = self.active.try_lock().map_err(|_| {
            WorldServiceClientError::Transport(
                "operation request already occupied or poisoned".into(),
            )
        })?;
        let active = &mut *guard;
        let response = if let Some(name) = &address.tls_name {
            tls_http::post(
                tls_http::Request {
                    address: address.address.clone(),
                    server_name: name.clone(),
                    tls_config: self.tls_config.clone().ok_or_else(|| {
                        WorldServiceClientError::Configuration("TLS policy missing".into())
                    })?,
                    authority: &address.authority,
                    path: request.path,
                    body: request.signed_body,
                    response_bound: request.response_bound,
                    request_timeout: request.request_timeout,
                },
                &mut active.ledger,
                &active.attempt,
                active.attempt.deadline(),
            )
        } else {
            direct_http::post_with_headers(
                direct_http::Request {
                    address: match &address.address {
                        super::dns::Target::Socket(socket) => *socket,
                        _ => {
                            return Err(WorldServiceClientError::Configuration(
                                "plaintext DNS is forbidden".into(),
                            ));
                        }
                    },
                    path: request.path,
                    body: request.signed_body,
                    response_bound: request.response_bound,
                    request_timeout: request.request_timeout,
                },
                &mut active.ledger,
                &active.attempt,
                active.attempt.deadline(),
            )
        }
        .map_err(WorldServiceClientError::Transport)?;
        Ok(response)
    }
}
