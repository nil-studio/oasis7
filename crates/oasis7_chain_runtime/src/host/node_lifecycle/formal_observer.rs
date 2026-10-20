//! Public projection observer. Local process/key ownership never supplies world
//! trust, latest-head freshness, validator membership or full-state sync proof.
use super::LocalInstance;
use oasis7::world_service::{
    client::{
        FormalReadTrustConfigV1, RemoteWorldServiceClient, WorldServiceClientConfig,
        WorldServiceClientError, WorldServicePort,
    },
    projection::WorldServiceProjection,
    verified_view::VerifiedWorldView,
};
use oasis7_client_api::world_service::{CommitRef, ReadWorldViewRequest, WorldIdentity};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(super) const PROFILE: &str = "formal_public_observer_v1";
pub(super) const STATUS_FILE: &str = "formal-observer.json";
// Keep the same cap as node_lifecycle::local_io metadata writes. Check the
// pretty-printed form because that is the representation the atomic writer
// persists.
const MAX_STATUS_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalObserverConfigV1 {
    pub api_version: String,
    pub endpoint: String,
    pub trusted_service_public_key: String,
    pub world_id: String,
    pub chain_id: String,
    pub genesis_digest: String,
    pub formal_read_trust: FormalReadTrustConfigV1,
    pub poll_interval_ms: u64,
    pub request_timeout_ms: u64,
    pub max_response_bytes: usize,
}
impl PartialEq for FormalObserverConfigV1 {
    fn eq(&self, other: &Self) -> bool {
        self.api_version == other.api_version
            && self.endpoint == other.endpoint
            && self.trusted_service_public_key == other.trusted_service_public_key
            && self.world_id == other.world_id
            && self.chain_id == other.chain_id
            && self.genesis_digest == other.genesis_digest
            && self.formal_read_trust.initial_trust == other.formal_read_trust.initial_trust
            && self.formal_read_trust.checkpoint == other.formal_read_trust.checkpoint
            && self.formal_read_trust.checkpoint_digest == other.formal_read_trust.checkpoint_digest
            && self.poll_interval_ms == other.poll_interval_ms
            && self.request_timeout_ms == other.request_timeout_ms
            && self.max_response_bytes == other.max_response_bytes
    }
}
impl Eq for FormalObserverConfigV1 {}
impl FormalObserverConfigV1 {
    fn client_config(&self, reader_private_key_hex: String) -> WorldServiceClientConfig {
        WorldServiceClientConfig {
            formal_read_trust: Some(self.formal_read_trust.clone()),
            endpoint: self.endpoint.clone(),
            trusted_service_public_key: self.trusted_service_public_key.clone(),
            expected_world: WorldIdentity {
                world_id: self.world_id.clone(),
                genesis_digest: self.genesis_digest.clone(),
            },
            scope_id: "public".into(),
            read_private_key_hex: reader_private_key_hex,
            timeout: Duration::from_millis(self.request_timeout_ms),
            max_response_bytes: self.max_response_bytes,
        }
    }
}
pub(super) fn validate(config: &FormalObserverConfigV1) -> Result<(), String> {
    if config.api_version != "oasis7.formal-public-observer/v1"
        || !(100..=60_000).contains(&config.poll_interval_ms)
        || !(100..=60_000).contains(&config.request_timeout_ms)
        || config.max_response_bytes == 0
        || config.max_response_bytes > 16 * 1024 * 1024
    {
        return Err("invalid formal public observer contract or bounds".into());
    }
    let world = &config.formal_read_trust.initial_trust.world;
    if config.world_id != world.world_id
        || config.chain_id != world.chain_id
        || config.genesis_digest != world.genesis_digest
    {
        return Err("formal observer world/chain/genesis differs from external trust".into());
    }
    config.formal_read_trust.verified_anchor()?;
    let endpoint = reqwest::Url::parse(&config.endpoint).map_err(|error| error.to_string())?;
    if !["http", "https"].contains(&endpoint.scheme())
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || hex::decode(&config.trusted_service_public_key).map_or(true, |key| key.len() != 32)
    {
        return Err("invalid formal public observer service endpoint or trust key".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalObserverStatusV1 {
    pub api_version: String,
    pub runtime_profile: String,
    pub incarnation: String,
    pub generation: u64,
    pub config_digest: String,
    pub read_state: String,
    pub read_ready: bool,
    pub can_propose: bool,
    pub state_sync_scope: String,
    pub full_state_sync_readiness: String,
    pub source_freshness: String,
    pub last_verified_commit: Option<CommitRef>,
    pub logical_tick: Option<u64>,
    pub public_projection: Option<WorldServiceProjection>,
    pub last_read_observed_unix_ms: Option<u128>,
    pub error: Option<String>,
}
fn status(instance: &LocalInstance) -> FormalObserverStatusV1 {
    FormalObserverStatusV1 {
        api_version: "oasis7.formal-public-observer-status/v1".into(),
        runtime_profile: PROFILE.into(),
        incarnation: instance.incarnation.clone(),
        generation: instance.generation,
        config_digest: instance.config_digest.clone(),
        read_state: "starting".into(),
        read_ready: false,
        can_propose: false,
        state_sync_scope: "public_projection_only".into(),
        full_state_sync_readiness: "unknown".into(),
        source_freshness: "unknown".into(),
        last_verified_commit: None,
        logical_tick: None,
        public_projection: None,
        last_read_observed_unix_ms: None,
        error: None,
    }
}
pub(super) struct Session {
    config: FormalObserverConfigV1,
    client: RemoteWorldServiceClient,
    observed: FormalObserverStatusV1,
    current_view: Option<VerifiedWorldView>,
}
impl Session {
    pub(super) fn new(
        config: &FormalObserverConfigV1,
        reader: String,
        instance: &LocalInstance,
    ) -> Result<Self, String> {
        validate(config)?;
        if instance.generation == 0
            || instance.incarnation.is_empty()
            || instance.config_digest.is_empty()
        {
            return Err("formal observer requires exact admitted local instance".into());
        }
        Ok(Self {
            config: config.clone(),
            client: RemoteWorldServiceClient::new(config.client_config(reader))
                .map_err(|error| error.to_string())?,
            observed: status(instance),
            current_view: None,
        })
    }
    pub(super) fn open(
        config: &FormalObserverConfigV1,
        data: &Path,
        instance: &LocalInstance,
    ) -> Result<Self, String> {
        let (reader, _) =
            super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
                &data.join("keys"),
            )?;
        Self::new(config, reader.private_key_hex, instance)
    }
    pub(super) fn poll(&mut self) {
        let result = self.client.read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: WorldIdentity {
                world_id: self.config.world_id.clone(),
                genesis_digest: self.config.genesis_digest.clone(),
            },
            scope_id: "public".into(),
            min_commit: self
                .current_view
                .as_ref()
                .map(|view| view.version().commit.clone()),
            fixed_commit: None,
            deadline_unix_ms: None,
        });
        let history_unavailable = matches!(
            &result,
            Err(WorldServiceClientError::Assurance(message))
                if message.contains("HistoryUnavailable")
        ) || matches!(
            &result,
            Err(WorldServiceClientError::Rejected { status: 410, .. })
        );
        match result {
            Ok(view) if view.formal_commit().is_some() => {
                let mut observed = self.observed.clone();
                observed.read_state = "qualified_public_read".into();
                observed.read_ready = true;
                observed.last_verified_commit = Some(view.version().commit.clone());
                observed.logical_tick = Some(view.logical_tick());
                observed.public_projection = Some(view.projection().clone());
                observed.last_read_observed_unix_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|time| time.as_millis());
                observed.error = None;
                match serde_json::to_vec_pretty(&observed) {
                    Ok(bytes) if bytes.len() <= MAX_STATUS_BYTES => {
                        self.observed = observed;
                        self.current_view = Some(view);
                    }
                    Ok(_) => self.fail(
                        "local_status_too_large",
                        "verified public projection exceeds the bounded local status file",
                    ),
                    Err(error) => self.fail(
                        "local_status_unavailable",
                        &format!("verified public projection cannot be encoded: {error}"),
                    ),
                }
            }
            result => {
                let error = result
                    .err()
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "formal profile produced no qualified proof".into());
                let state = if history_unavailable {
                    "history_unavailable"
                } else {
                    "read_unavailable"
                };
                self.fail(state, &error);
            }
        }
    }
    fn fail(&mut self, state: &str, error: &str) {
        self.observed.read_ready = false;
        self.observed.read_state = state.into();
        self.observed.error = Some(error.chars().take(512).collect());
    }
    pub(super) fn observed(&self) -> &FormalObserverStatusV1 {
        &self.observed
    }
    pub(super) fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.config.poll_interval_ms)
    }
    #[cfg(test)]
    pub(super) fn can_propose(&self) -> bool {
        false
    }
    #[cfg(test)]
    pub(super) fn current_view(&self) -> Option<&VerifiedWorldView> {
        self.current_view.as_ref()
    }
}
#[cfg(test)]
#[path = "formal_observer_tests.rs"]
mod tests;
