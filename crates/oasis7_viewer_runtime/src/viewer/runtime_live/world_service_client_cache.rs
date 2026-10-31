//! One validated, immutable seed per exact configured reader identity.
//! Returned clients retain independent formal verifier state. Query cooldown
//! sharing is installed by the caller under its original scope rules.
use super::*;
use crate::world_service::client::{RemoteWorldServiceClient, WorldServiceClientConfig};
use std::cell::RefCell;

pub(super) struct ClientCache {
    seed: RefCell<Option<RemoteWorldServiceClient>>,
}
impl ClientCache {
    pub(super) fn new(
        config: Option<&WorldServiceClientConfig>,
    ) -> Result<Self, ViewerRuntimeLiveServerError> {
        let seed = config
            .map(|config| RemoteWorldServiceClient::new(config.clone()))
            .transpose()
            .map_err(|error| ViewerRuntimeLiveServerError::Init(error.to_string()))?;
        Ok(Self {
            seed: RefCell::new(seed),
        })
    }
    pub(super) fn get(
        &self,
        config: Option<&WorldServiceClientConfig>,
    ) -> Result<Option<RemoteWorldServiceClient>, ViewerRuntimeLiveServerError> {
        let mut seed = self.seed.borrow_mut();
        let Some(config) = config else {
            *seed = None;
            return Ok(None);
        };
        if seed.as_ref().is_none_or(|client| client.config() != config) {
            // Configuration is immutable in serving production. Local test and
            // reconfiguration callers must validate a complete new identity;
            // a failed replacement cannot return the previous authority.
            let next = RemoteWorldServiceClient::new(config.clone())
                .map_err(|error| ViewerRuntimeLiveServerError::Init(error.to_string()))?;
            *seed = Some(next);
        }
        seed.as_ref()
            .unwrap()
            .independent_clone()
            .map(Some)
            .map_err(|error| ViewerRuntimeLiveServerError::Init(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> WorldServiceClientConfig {
        WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint: "http://127.0.0.1:9".into(),
            trusted_service_public_key: hex::encode([9; 32]),
            expected_world: oasis7_client_api::world_service::WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
            scope_id: "public".into(),
            read_private_key_hex: hex::encode([7; 32]),
            timeout: Duration::from_millis(500),
            max_response_bytes: 4096,
        }
    }
    #[test]
    fn exact_configuration_replacement_never_returns_previous_authority() {
        let original = config();
        let cache = ClientCache::new(Some(&original)).unwrap();
        for field in 0..8 {
            let mut next = original.clone();
            match field {
                0 => next.endpoint = "http://127.0.0.1:10".into(),
                1 => next.trusted_service_public_key = hex::encode([10; 32]),
                2 => next.expected_world.world_id = "other-world".into(),
                3 => next.expected_world.genesis_digest = "other-genesis".into(),
                4 => next.scope_id = "agent:a".into(),
                5 => next.read_private_key_hex = hex::encode([11; 32]),
                6 => next.timeout = Duration::from_millis(501),
                _ => next.max_response_bytes += 1,
            }
            let client = cache.get(Some(&next)).unwrap().unwrap();
            assert!(client.config() == &next);
            assert!(client.config() != &original);
        }
        let mut invalid = original.clone();
        invalid.read_private_key_hex = "invalid".into();
        assert!(cache.get(Some(&invalid)).is_err());
        assert!(cache.get(Some(&original)).unwrap().unwrap().config() == &original);
        assert!(cache.get(None).unwrap().is_none());
    }
}
