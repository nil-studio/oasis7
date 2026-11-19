#[cfg(unix)]
use std::path::Path;
use std::sync::Arc;

use oasis7::development_node_config::{
    DevelopmentPeerRecordPublicationModeV1, DevelopmentPeerRecordReceptionModeV1,
};
use oasis7_node::{
    Libp2pReplicationNetwork, Libp2pReplicationNetworkConfig, NodeReplicationNetworkHandle,
    NodeRuntime, PeerRecordPublicationTransition, PeerRecordReceiverConfig, PeerRecordReceiverMode,
    derive_libp2p_identity_keypair,
};
use oasis7_proto::distributed_dht::{PeerDiscoverySource, PeerRecord};

use super::cli::{CliOptions, DEFAULT_REPLICATION_NETWORK_LISTEN};
use super::node_keypair_config::{self, NodeKeypairConfig};
use super::signer_derivation::derive_node_libp2p_identity_keypair_config;
use super::traffic_profile::apply_traffic_profile_to_replication_network_config;
use super::{RuntimePaths, build_node_network_policy};

pub(super) fn attach_default_replication_network(
    runtime: NodeRuntime,
    options: &CliOptions,
    root_keypair: &NodeKeypairConfig,
    paths: &RuntimePaths,
) -> Result<(NodeRuntime, Arc<Libp2pReplicationNetwork>), String> {
    let network = create_default_replication_network(options, root_keypair, paths)?;
    let handle_network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<oasis7_proto::world_error::WorldError>
            + Send
            + Sync,
    > = network.clone();
    let handle_dht: Arc<
        dyn oasis7_proto::distributed_dht::DistributedDht<oasis7_proto::world_error::WorldError>
            + Send
            + Sync,
    > = network.clone();
    let runtime = runtime
        .with_replication_network(
            NodeReplicationNetworkHandle::new(handle_network)
                .with_dht(handle_dht.clone())
                .with_local_provider_id(network.peer_id().to_string()),
        )
        .with_replica_maintenance_dht(handle_dht)
        // ObserverLight may subscribe to signed consensus commits (the lane
        // policy still forbids observer publication). This lets checkpoint
        // preflight observe validated high peer heads before replication ingest.
        .with_replication_network_consensus_enabled(true);
    Ok((runtime, network))
}

pub(super) fn create_default_replication_network(
    options: &CliOptions,
    root_keypair: &NodeKeypairConfig,
    paths: &RuntimePaths,
) -> Result<Arc<Libp2pReplicationNetwork>, String> {
    let mut network_config =
        build_default_replication_network_config(options, root_keypair, paths)?;
    network_config.allow_local_handler_fallback_when_no_peers =
        network_config.bootstrap_peers.is_empty();
    Libp2pReplicationNetwork::try_new(network_config)
        .map(Arc::new)
        .map_err(|err| format!("failed to open replication network: {err:?}"))
}

pub(super) fn build_default_replication_network_config(
    options: &CliOptions,
    root_keypair: &NodeKeypairConfig,
    paths: &RuntimePaths,
) -> Result<Libp2pReplicationNetworkConfig, String> {
    let mut config = build_default_peer_record_network_config(options, root_keypair, paths)?;
    config.p2p_resource_budget = options.p2p_resource_budget;
    if options.replication_network_listen_addrs.is_empty() {
        config
            .listen_addrs
            .push(DEFAULT_REPLICATION_NETWORK_LISTEN.parse().map_err(|err| {
                format!(
                    "failed to parse default replication listen address {}: {err}",
                    DEFAULT_REPLICATION_NETWORK_LISTEN
                )
            })?);
    } else {
        config.listen_addrs = options
            .replication_network_listen_addrs
            .iter()
            .map(|raw| {
                raw.parse()
                    .map_err(|err| format!("invalid --replication-network-listen {raw}: {err}"))
            })
            .collect::<Result<_, _>>()?;
    }
    let bootstrap_peers = if options.replication_network_bootstrap_peers.is_empty() {
        options
            .loaded_network_tier_manifest
            .as_ref()
            .map(|loaded| loaded.bootstrap_peers.clone())
            .unwrap_or_default()
    } else {
        options.replication_network_bootstrap_peers.clone()
    };
    config.bootstrap_peers = bootstrap_peers
        .iter()
        .map(|raw| {
            raw.parse()
                .map_err(|err| format!("invalid --replication-network-peer {raw}: {err}"))
        })
        .collect::<Result<_, _>>()?;
    if let Some(limit) = options.p2p_max_ipv4_subnet_active_peers {
        config.peer_manager_policy.max_ipv4_subnet_active_peers = Some(limit);
    }
    apply_traffic_profile_to_replication_network_config(&mut config, options.traffic_profile);
    Ok(config)
}

fn build_default_peer_record_network_config(
    options: &CliOptions,
    root_keypair: &NodeKeypairConfig,
    paths: &RuntimePaths,
) -> Result<Libp2pReplicationNetworkConfig, String> {
    let mut config = Libp2pReplicationNetworkConfig::default();
    let libp2p_identity_keypair =
        derive_node_libp2p_identity_keypair_config(options.node_id.as_str(), root_keypair)?;
    config.keypair = Some(
        derive_libp2p_identity_keypair(libp2p_identity_keypair.private_key_hex.as_str())
            .map_err(|err| format!("failed to derive libp2p identity keypair: {err:?}"))?,
    );
    config.peer_record = Some(build_default_peer_record(options));
    if options.p2p_peer_record_publication == DevelopmentPeerRecordPublicationModeV1::V2Durable {
        config.peer_record_publication_directory = Some(
            paths
                .runtime_root
                .join("p2p")
                .join("peer-record-publication"),
        );
    }
    config.peer_record_receiver = PeerRecordReceiverConfig {
        mode: match options.p2p_peer_record_reception {
            DevelopmentPeerRecordReceptionModeV1::LegacyCompatible => {
                PeerRecordReceiverMode::Legacy
            }
            DevelopmentPeerRecordReceptionModeV1::V2DurableCompatible => {
                PeerRecordReceiverMode::DurableCompatible
            }
            DevelopmentPeerRecordReceptionModeV1::V2Only => PeerRecordReceiverMode::V2Only,
        },
        directory: Some(paths.runtime_root.join("p2p").join("peer-record-receiver")),
    };
    config.peer_record_publication_transition = match (
        options.withdraw_peer_record_publication,
        options.resume_peer_record_publication,
    ) {
        (false, false) => None,
        (true, false) => Some(PeerRecordPublicationTransition::Withdraw),
        (false, true) => Some(PeerRecordPublicationTransition::Resume),
        (true, true) => {
            return Err(
                "--withdraw-peer-record-publication and --resume-peer-record-publication are mutually exclusive"
                    .into(),
            );
        }
    };
    Ok(config)
}

pub(super) fn initialize_default_peer_record_receiver(options: &CliOptions) -> Result<(), String> {
    if options.p2p_peer_record_reception == DevelopmentPeerRecordReceptionModeV1::LegacyCompatible {
        return Err(
            "--initialize-peer-record-receiver requires --p2p-peer-record-reception v2_durable_compatible or v2_only"
                .into(),
        );
    }
    if options.formal_live_config.is_some() || options.guarded_initial_config.is_some() {
        return Err(
            "--initialize-peer-record-receiver applies only to the development host runtime".into(),
        );
    }
    #[cfg(not(unix))]
    {
        let _ = options;
        return Err("durable peer-record reception requires Unix filesystem guarantees".into());
    }
    #[cfg(unix)]
    {
        let root_keypair = if options.upgrade_peer_record_receiver {
            let config_path = Path::new(options.config_path.as_str());
            let config_path = if config_path.is_absolute() {
                config_path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|error| error.to_string())?
                    .join(config_path)
            };
            node_keypair_config::read_existing_legacy_keypair(&config_path)?
        } else {
            node_keypair_config::ensure_node_keypair_in_config(Path::new(
                options.config_path.as_str(),
            ))?
        };
        let paths = super::resolve_runtime_paths(options);
        let mut network_config =
            build_default_peer_record_network_config(options, &root_keypair, &paths)?;
        network_config.peer_record_publication_directory = None;
        if options.upgrade_peer_record_receiver {
            Libp2pReplicationNetwork::upgrade_peer_record_receiver(&network_config)
                .map_err(|err| format!("failed to upgrade durable peer-record receiver: {err:?}"))
        } else {
            Libp2pReplicationNetwork::initialize_peer_record_receiver(&network_config).map_err(
                |err| format!("failed to initialize durable peer-record receiver: {err:?}"),
            )
        }
    }
}

pub(super) fn prune_default_peer_record_receiver(options: &CliOptions) -> Result<(), String> {
    if !options.prune_peer_record_receiver {
        return Err("prune requested without --prune-peer-record-receiver".into());
    }
    if options.formal_live_config.is_some() || options.guarded_initial_config.is_some() {
        return Err(
            "--prune-peer-record-receiver applies only to the development host runtime".into(),
        );
    }
    let trusted_now_ms = options
        .peer_record_trusted_time_unix_ms
        .filter(|t| *t > 0)
        .ok_or_else(|| {
            "--prune-peer-record-receiver requires a valid positive --peer-record-trusted-time-unix-ms"
                .to_string()
        })?;
    #[cfg(not(unix))]
    {
        let _ = (options, trusted_now_ms);
        return Err("durable peer-record reception requires Unix filesystem guarantees".into());
    }
    #[cfg(unix)]
    {
        let config_path = Path::new(options.config_path.as_str());
        let config_path = if config_path.is_absolute() {
            config_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|error| error.to_string())?
                .join(config_path)
        };
        let root_keypair = node_keypair_config::read_existing_legacy_keypair(&config_path)?;
        let paths = super::resolve_runtime_paths(options);
        let mut network_config =
            build_default_peer_record_network_config(options, &root_keypair, &paths)?;
        network_config.peer_record_publication_directory = None;
        let retained_peers: std::collections::BTreeSet<String> =
            options.peer_record_retained_peers.iter().cloned().collect();
        let pruned = Libp2pReplicationNetwork::prune_peer_record_receiver(
            &network_config,
            trusted_now_ms,
            &retained_peers,
        )
        .map_err(|err| format!("failed to prune durable peer-record receiver: {err:?}"))?;
        tracing::info!(pruned = pruned, "pruned expired receiver floors");
        Ok(())
    }
}

pub(super) fn initialize_default_peer_record_publication(
    options: &CliOptions,
) -> Result<(), String> {
    if options.p2p_peer_record_publication != DevelopmentPeerRecordPublicationModeV1::V2Durable {
        return Err(
            "--initialize-peer-record-publication requires --p2p-peer-record-publication v2_durable"
                .into(),
        );
    }
    if options.formal_live_config.is_some() || options.guarded_initial_config.is_some() {
        return Err(
            "--initialize-peer-record-publication applies only to the development host runtime"
                .into(),
        );
    }
    #[cfg(not(unix))]
    {
        let _ = options;
        return Err("durable peer-record publication requires Unix filesystem guarantees".into());
    }
    #[cfg(unix)]
    {
        let root_keypair = node_keypair_config::ensure_node_keypair_in_config(Path::new(
            options.config_path.as_str(),
        ))?;
        let paths = super::resolve_runtime_paths(options);
        let network_config =
            build_default_peer_record_network_config(options, &root_keypair, &paths)?;
        Libp2pReplicationNetwork::initialize_peer_record_publication(&network_config)
            .map_err(|err| format!("failed to initialize durable peer-record publication: {err:?}"))
    }
}

pub(super) fn build_default_peer_record(options: &CliOptions) -> PeerRecord {
    let network_policy = build_node_network_policy(options);
    PeerRecord {
        peer_id: String::new(),
        node_id: options.node_id.clone(),
        world_id: options.world_id.clone(),
        network_id: options.world_id.clone(),
        node_role: network_policy.node_role_claim.as_str().to_string(),
        deployment_mode: network_policy.deployment_mode,
        reachability_class: network_policy.advertised_reachability_class(),
        direct_addrs: Vec::new(),
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: vec![
            PeerDiscoverySource::StaticBootstrap,
            PeerDiscoverySource::Dht,
        ],
        capability_lanes: network_policy.node_role_claim.default_capability_lanes(),
        source_operator: options.p2p_source_operator.clone(),
        source_asn: options.p2p_source_asn.clone(),
        published_at_ms: 0,
        ttl_ms: 60 * 60 * 1000,
    }
}
