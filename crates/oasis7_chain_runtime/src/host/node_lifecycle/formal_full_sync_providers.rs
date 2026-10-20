//! Validated, independently pinned full-state-sync source endpoints.

use super::*;
use oasis7::world_service::client::WorldServiceClientConfig;
use std::net::IpAddr;

const MAX_REQUEST_TIMEOUT_MS: u64 = 60_000;
pub(super) const MAX_TOTAL_SYNC_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_SYNC_PROVIDERS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalFullSyncConfigV1 {
    pub api_version: String,
    pub endpoint: String,
    pub trusted_service_public_key: String,
    /// Explicit additional sources. The legacy endpoint/key pair remains the
    /// first provider and old single-provider configs serialize identically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternate_endpoints: Vec<FormalFullSyncEndpointV1>,
    pub world_id: String,
    pub chain_id: String,
    pub genesis_digest: String,
    pub formal_read_trust: FormalReadTrustConfigV1,
    pub grant: Option<FormalNodeReadGrantV1>,
    pub expected_role: FormalNodeReadRoleV1,
    pub poll_interval_ms: u64,
    pub request_timeout_ms: u64,
    pub max_total_sync_bytes: u64,
}

/// One explicitly trusted full-sync provider. Each endpoint has its own
/// service signing-key pin; provider discovery never changes the fixed C0 pin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalFullSyncEndpointV1 {
    pub endpoint: String,
    pub trusted_service_public_key: String,
}

impl FormalFullSyncConfigV1 {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.api_version != super::API_VERSION
            || !(100..=60_000).contains(&self.poll_interval_ms)
            || !(100..=MAX_REQUEST_TIMEOUT_MS).contains(&self.request_timeout_ms)
            || self.max_total_sync_bytes == 0
            || self.max_total_sync_bytes > MAX_TOTAL_SYNC_BYTES
        {
            return Err("invalid formal full-sync configuration or bounds".into());
        }
        let expected_world = WorldIdentity {
            world_id: self.world_id.clone(),
            genesis_digest: self.genesis_digest.clone(),
        };
        expected_world
            .validate()
            .map_err(|error| error.to_string())?;
        let anchor = self.formal_read_trust.verified_anchor()?;
        let trusted_world = &self.formal_read_trust.initial_trust.world;
        if trusted_world.world_id != self.world_id
            || trusted_world.chain_id != self.chain_id
            || trusted_world.genesis_digest != self.genesis_digest
            || anchor.head().record().world.world_id != self.world_id
            || anchor.head().record().world.chain_id != self.chain_id
            || anchor.head().record().world.genesis_digest != self.genesis_digest
        {
            return Err("full-sync World identity differs from the external formal pin".into());
        }
        let providers = self.providers();
        if providers.is_empty() || providers.len() > MAX_SYNC_PROVIDERS {
            return Err("full-sync provider count exceeds its configured bound".into());
        }
        let mut normalized_endpoints = std::collections::BTreeSet::new();
        for provider in &providers {
            let endpoint = validate_provider(provider)?;
            let normalized = endpoint.as_str().trim_end_matches('/').to_owned();
            if !normalized_endpoints.insert(normalized) {
                return Err("full-sync provider endpoints must be unique".into());
            }
        }
        if self.grant.as_ref().is_some_and(|grant| {
            grant.body.role != self.expected_role
                || grant.body.world != expected_world
                || grant.body.chain_id != self.chain_id
        }) {
            return Err("full-sync grant differs from configured World or node role".into());
        }
        Ok(())
    }

    pub(super) fn providers(&self) -> Vec<FormalFullSyncEndpointV1> {
        let mut providers = Vec::with_capacity(1 + self.alternate_endpoints.len());
        providers.push(FormalFullSyncEndpointV1 {
            endpoint: self.endpoint.clone(),
            trusted_service_public_key: self.trusted_service_public_key.clone(),
        });
        providers.extend(self.alternate_endpoints.iter().cloned());
        providers
    }
}

fn validate_provider(provider: &FormalFullSyncEndpointV1) -> Result<reqwest::Url, String> {
    if provider.endpoint.len() > 2048 {
        return Err("full-sync provider endpoint exceeds its size bound".into());
    }
    let endpoint = reqwest::Url::parse(&provider.endpoint).map_err(|error| error.to_string())?;
    if endpoint.host_str().is_none()
        || !endpoint_transport_is_private(&endpoint)
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || !super::valid_hex(&provider.trusted_service_public_key, 32)
    {
        return Err("invalid full-sync service endpoint or trust key".into());
    }
    Ok(endpoint)
}

pub(super) fn build_read_client(
    config: &FormalFullSyncConfigV1,
    provider: &FormalFullSyncEndpointV1,
    node_private_key_hex: &str,
    timeout: Duration,
) -> Result<RemoteWorldServiceClient, String> {
    RemoteWorldServiceClient::new(WorldServiceClientConfig {
        formal_read_trust: Some(config.formal_read_trust.clone()),
        endpoint: provider.endpoint.clone(),
        trusted_service_public_key: provider.trusted_service_public_key.clone(),
        expected_world: WorldIdentity {
            world_id: config.world_id.clone(),
            genesis_digest: config.genesis_digest.clone(),
        },
        scope_id: "public".into(),
        read_private_key_hex: node_private_key_hex.to_owned(),
        timeout,
        max_response_bytes: MAX_RESPONSE_BYTES,
    })
    .map_err(|error| format!("formal full-sync pinned read client: {error}"))
}

fn endpoint_transport_is_private(endpoint: &reqwest::Url) -> bool {
    if endpoint.scheme() == "https" {
        return true;
    }
    if endpoint.scheme() != "http" {
        return false;
    }
    let Some(host) = endpoint.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}
