use oasis7::viewer::ViewerRuntimeLiveServerConfig;
use oasis7::world_service::client::{
    RemoteWorldServiceClient, WorldServiceAgentSignerConfig, WorldServiceClientConfig,
};

pub(super) fn configure(
    mut config: ViewerRuntimeLiveServerConfig,
    isolated_offline: bool,
    service: Option<WorldServiceClientConfig>,
    agent_signer: Option<WorldServiceAgentSignerConfig>,
) -> Result<ViewerRuntimeLiveServerConfig, String> {
    if let Some(signer) = &agent_signer {
        if signer.delegation_generation == 0 {
            return Err("Agent delegation generation must be nonzero".into());
        }
        oasis7::world_service::sign_read_request(
            "agent-configuration",
            (),
            &signer.private_key_hex,
        )?;
    }
    let Some(service) = service else {
        if agent_signer.is_some() {
            return Err("Agent signer requires a world service connection".into());
        }
        if !isolated_offline {
            return Err("formal Viewer requires authenticated world service configuration; isolated offline diagnostics require --allow-debug-scenario".into());
        }
        return Ok(config);
    };
    if config.generated_world_dir.is_some() {
        return Err(
            "world service cannot be combined with a local generated world directory".into(),
        );
    }
    RemoteWorldServiceClient::new(service.clone())
        .map_err(|error| format!("invalid world service configuration: {error}"))?;
    // The expected service identity is the only Agent world identity in this lane.
    // Actual remote identity still must authenticate against it on every read.
    config.world_id = service.expected_world.world_id.clone();
    config.world_service = Some(service);
    config.world_service_agent_signer = agent_signer;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_simulator::WorldScenario;
    use std::time::Duration;

    fn service() -> WorldServiceClientConfig {
        let key = ed25519_dalek::SigningKey::from_bytes(&[9; 32]);
        WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint: "http://127.0.0.1:5999".into(),
            trusted_service_public_key: hex::encode(key.verifying_key().to_bytes()),
            expected_world: oasis7_client_api::world_service::WorldIdentity {
                world_id: "canonical-world".into(),
                genesis_digest: "genesis".into(),
            },
            scope_id: "agent:agent-a".into(),
            read_private_key_hex: hex::encode([7; 32]),
            timeout: Duration::from_secs(1),
            max_response_bytes: 4096,
        }
    }

    #[test]
    fn service_identity_and_separate_agent_signer_are_assembled_without_node_configuration() {
        let signer = WorldServiceAgentSignerConfig {
            private_key_hex: hex::encode([8; 32]),
            delegation_generation: 2,
        };
        let config = configure(
            ViewerRuntimeLiveServerConfig::formal_release_default(),
            false,
            Some(service()),
            Some(signer),
        )
        .unwrap();
        assert_eq!(config.world_id, "canonical-world");
        assert!(config.chain_status_bind.is_none());
        assert!(config.generated_world_dir.is_none());
        assert_ne!(
            config.world_service.as_ref().unwrap().read_private_key_hex,
            config
                .world_service_agent_signer
                .as_ref()
                .unwrap()
                .private_key_hex
        );
        assert_eq!(
            config
                .world_service_agent_signer
                .unwrap()
                .delegation_generation,
            2
        );
    }

    #[test]
    fn formal_missing_invalid_or_local_world_configuration_fails_closed() {
        assert!(
            configure(
                ViewerRuntimeLiveServerConfig::formal_release_default(),
                false,
                None,
                None
            )
            .is_err()
        );
        let mut invalid = service();
        invalid.trusted_service_public_key = "not-a-key".into();
        assert!(
            configure(
                ViewerRuntimeLiveServerConfig::formal_release_default(),
                false,
                Some(invalid),
                None
            )
            .is_err()
        );
        let mut local = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal);
        local.generated_world_dir = Some("local-world".into());
        assert!(configure(local, true, Some(service()), None).is_err());
        assert!(
            configure(
                ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal),
                true,
                None,
                None
            )
            .is_ok()
        );
        assert!(
            configure(
                ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal),
                true,
                None,
                Some(WorldServiceAgentSignerConfig {
                    private_key_hex: hex::encode([8; 32]),
                    delegation_generation: 1
                })
            )
            .is_err()
        );
    }
}
