//! Fresh, externally pinned WorldService reads for operation result collection.
//!
//! The ordinary observer status file is intentionally not consulted. This
//! module reconstructs the existing client from the node's pinned observer
//! configuration and private read identity, then returns only a verified view.
use super::{actions, formal_observer, local_io as io, model::NodeConfig, types::LocalInstance};
use oasis7::world_service::{
    client::{RemoteWorldServiceClient, WorldServiceClientConfig, WorldServicePort},
    verified_view::VerifiedWorldView,
};
use oasis7_client_api::world_service::{
    ReadWorldViewRequest, WORLD_SERVICE_CONTRACT_VERSION, WorldIdentity,
};
use std::{path::Path, time::Duration};

pub(crate) struct VerifiedOperationReadV1 {
    pub view: VerifiedWorldView,
    pub node_id: String,
    pub world_id: String,
    pub chain_id: String,
    pub instance: LocalInstance,
}

/// Read a fresh formal public projection for this exact local node instance.
/// Compatibility observers have no independently pinned formal read and
/// return `None`; malformed or unavailable formal inputs return an error.
pub(crate) fn read_verified_operation_view(
    config_path: &Path,
    expected_instance: &LocalInstance,
) -> Result<Option<VerifiedOperationReadV1>, String> {
    let config = NodeConfig::load(config_path)?;
    config.verify_identity()?;
    let Some(observer) = config.identity.chain.formal_observer() else {
        return Ok(None);
    };
    formal_observer::validate(observer)?;

    let (reader, _) =
        super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &config.identity.data_dir.join("keys"),
        )?;
    if reader.public_key_hex != config.identity.root_public_key {
        return Err("formal operation reader identity differs from the exact node binding".into());
    }
    let world = WorldIdentity {
        world_id: observer.world_id.clone(),
        genesis_digest: observer.genesis_digest.clone(),
    };
    let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
        formal_read_trust: Some(observer.formal_read_trust.clone()),
        endpoint: observer.endpoint.clone(),
        trusted_service_public_key: observer.trusted_service_public_key.clone(),
        expected_world: world.clone(),
        scope_id: "public".into(),
        read_private_key_hex: reader.private_key_hex,
        timeout: Duration::from_millis(observer.request_timeout_ms),
        max_response_bytes: observer.max_response_bytes,
    })
    .map_err(|error| format!("pinned formal operation read client: {error}"))?;
    let view = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world,
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .map_err(|error| format!("pinned formal operation read: {error}"))?;
    let commit = view
        .formal_commit()
        .ok_or("formal operation read did not carry a verified qualified commit")?;
    let record = commit.record();
    if record.world.world_id != observer.world_id
        || record.world.chain_id != observer.chain_id
        || record.world.genesis_digest != observer.genesis_digest
        || commit.finalizing_authority_id().is_empty()
    {
        return Err("verified formal read differs from the node's pinned World identity".into());
    }

    let binding = actions::binding(config_path)?;
    if binding.data_dir != config.identity.data_dir
        || binding.world_id != observer.world_id
        || binding.chain_id != observer.chain_id
        || binding.current.as_ref() != Some(expected_instance)
    {
        return Err("operation result read no longer matches the exact local node instance".into());
    }
    io::reject_symlinks(config_path)?;
    Ok(Some(VerifiedOperationReadV1 {
        view,
        node_id: config.identity.node_id,
        world_id: observer.world_id.clone(),
        chain_id: observer.chain_id.clone(),
        instance: expected_instance.clone(),
    }))
}
