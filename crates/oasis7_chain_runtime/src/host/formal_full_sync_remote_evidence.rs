//! Bounded relay for raw issuer/witness evidence. The relay is not an authority:
//! the caller independently verifies every signature, nonce, floor and lease.
use super::*;
pub(crate) use oasis7_distfs::controlled_authority::formal::full_sync_issuer::FullSyncReadEvidenceQueryV3;
use std::io::Read;

#[path = "formal_full_sync_remote_lease.rs"]
mod lease;

const MAX_RESPONSE_BYTES: u64 = 68 * 1024 * 1024;

pub(crate) struct RemoteFullSyncReadEvidenceSourceV3 {
    endpoint: reqwest::Url,
    client: reqwest::blocking::Client,
    lease: Option<lease::ProviderLeaseClientV3>,
}
impl RemoteFullSyncReadEvidenceSourceV3 {
    pub(crate) fn new(endpoint: &str) -> Result<Self, String> {
        Self::with_transport(endpoint, None)
    }

    pub(crate) fn new_with_mtls(
        endpoint: &str,
        ca_pem: &[u8],
        identity_pem: &[u8],
    ) -> Result<Self, String> {
        let roots = reqwest::Certificate::from_pem_bundle(ca_pem)
            .map_err(|_| "invalid issuer mTLS server CA PEM")?;
        if roots.is_empty() {
            return Err("issuer mTLS requires at least one pinned server CA".into());
        }
        let identity = reqwest::Identity::from_pem(identity_pem)
            .map_err(|_| "invalid issuer mTLS client identity PEM")?;
        Self::with_transport(endpoint, Some((roots, identity)))
    }

    fn with_transport(
        endpoint: &str,
        mtls: Option<(Vec<reqwest::Certificate>, reqwest::Identity)>,
    ) -> Result<Self, String> {
        let endpoint =
            reqwest::Url::parse(endpoint).map_err(|_| "invalid full-sync evidence endpoint")?;
        let loopback = endpoint.host_str().is_some_and(|host| {
            // URL hosts retain IPv6 brackets, but IpAddr parses the address
            // itself. Strip only a complete bracket pair after URL validation.
            let host = host
                .strip_prefix('[')
                .and_then(|host| host.strip_suffix(']'))
                .unwrap_or(host);
            host.parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        });
        if !(endpoint.scheme() == "https" || (endpoint.scheme() == "http" && loopback))
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || endpoint.host_str().is_none()
        {
            return Err("full-sync evidence endpoint requires HTTPS or explicit loopback HTTP without URL credentials".into());
        }
        let builder = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy();
        let builder = match mtls {
            Some((roots, identity)) => {
                if endpoint.scheme() != "https" {
                    return Err("issuer mTLS requires an HTTPS evidence endpoint".into());
                }
                builder.https_only(true).tls_certs_only(roots).identity(identity)
            }
            None => builder,
        };
        let client = builder
            .build()
            .map_err(|_| "cannot build full-sync evidence client")?;
        Ok(Self {
            endpoint,
            client,
            lease: None,
        })
    }
    pub(crate) fn with_provider_lease_requests(
        mut self,
        v2: VerifiedFullSyncIssuerPolicyV2,
        v3: VerifiedFullSyncIssuerPolicyV3,
        provider: ed25519_dalek::SigningKey,
    ) -> Result<Self, String> {
        self.lease = Some(lease::ProviderLeaseClientV3::new(v2, v3, provider)?);
        Ok(self)
    }
}
impl FullSyncIssuerReadEvidenceSource for RemoteFullSyncReadEvidenceSourceV3 {
    fn fetch(
        &self,
        request: &FullSyncIssuerReadEvidenceRequestV3,
    ) -> FormalReadResult<FullSyncIssuerReadEvidenceV3> {
        match &self.lease {
            Some(lease) => lease.fetch(self, request),
            None => self.fetch_raw(request, None),
        }
    }
}
impl RemoteFullSyncReadEvidenceSourceV3 {
    fn fetch_raw(
        &self,
        request: &FullSyncIssuerReadEvidenceRequestV3,
        provider_lease_request:Option<oasis7_distfs::controlled_authority::formal::full_sync_issuer::SignedFullSyncProviderLeaseRequestV3>,
    ) -> FormalReadResult<FullSyncIssuerReadEvidenceV3> {
        let remaining = request
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                FormalReadError::HistoryUnavailable("issuer evidence deadline exhausted".into())
            })?;
        let query = FullSyncReadEvidenceQueryV3 {
            schema_version: 3,
            observation_nonce: hex::encode(request.observation_nonce),
            minimum_anchor: request.minimum_anchor.clone(),
            provider_public_key: request.provider_public_key.clone(),
            provider_incarnation: request.provider_incarnation.clone(),
            target: request.target.clone(),
            target_generation: request.target_generation,
            provider_lease_request,
        };
        let response = self
            .client
            .post(self.endpoint.clone())
            .timeout(remaining)
            .json(&query)
            .send()
            .map_err(|_| {
                FormalReadError::HistoryUnavailable("issuer evidence transport unavailable".into())
            })?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|n| n > MAX_RESPONSE_BYTES)
        {
            return Err(FormalReadError::HistoryUnavailable(
                "issuer evidence response rejected".into(),
            ));
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                FormalReadError::HistoryUnavailable("issuer evidence body unavailable".into())
            })?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES || Instant::now() >= request.deadline {
            return Err(FormalReadError::HistoryUnavailable(
                "issuer evidence response exceeded original bound".into(),
            ));
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            FormalReadError::Integrity(
                "issuer evidence response is not a complete typed bundle".into(),
            )
        })
    }
}
