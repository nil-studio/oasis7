#[cfg(test)]
#[path = "../../oasis7/src/world_service/formal_read_fixtures.rs"]
mod formal_read_fixtures;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7::observability::emit_stderr_or_event;
use oasis7_node::{
    Libp2pReplicationNetwork, NodeConfig, NodeConsensusSlashingReceiptSnapshot, NodeNetworkPolicy,
    NodePosConfig, NodeReplicationConfig, NodeRole, NodeRuntime, PosConsensusStatus, PosValidator,
};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::{
    NodeAssetBalance, NodeRewardMintRecord, ReleaseSecurityPolicy, RewardAssetConfig,
};
use runtime_status_util::now_unix_ms;
use serde::Serialize;
use tracing::{Level, error, info};
#[path = "host/agent_claim_api.rs"]
mod agent_claim_api;
#[path = "host/balances_api.rs"]
mod balances_api;
#[path = "host/signer_derivation.rs"]
mod signer_derivation;
use signer_derivation::{
    build_feedback_submit_signer, build_validator_signer_public_keys,
    derive_node_consensus_signer_keypair, derive_node_libp2p_identity_keypair_config,
};
#[path = "host/cli.rs"]
mod cli;
#[path = "host/controlled_bootstrap_cli.rs"]
mod controlled_bootstrap_cli;
#[path = "host/controlled_execution_policy.rs"]
mod controlled_execution_policy;
#[path = "host/controlled_history_cli.rs"]
mod controlled_history_cli;
#[path = "host/controlled_live_config.rs"]
mod controlled_live_config;
#[path = "host/controlled_live_runtime.rs"]
mod controlled_live_runtime;
#[path = "host/dev_cli.rs"]
mod dev_cli;
#[path = "host/development_runtime.rs"]
mod development_runtime;
#[path = "host/devlocal_participant_cli.rs"]
mod devlocal_participant_cli;
#[path = "host/distfs_probe_runtime.rs"]
mod distfs_probe_runtime;
#[path = "host/executable_image/mod.rs"]
mod executable_image;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "host/execution_bridge/mod.rs"]
mod execution_bridge;
#[path = "host/execution_role.rs"]
mod execution_role;
#[path = "host/explorer_p0_api.rs"]
mod explorer_p0_api;
#[path = "host/feedback_submit_api.rs"]
mod feedback_submit_api;
#[cfg(unix)]
#[path = "host/formal_bft_prefix_cli.rs"]
mod formal_bft_prefix_cli;
#[cfg(unix)]
#[path = "host/formal_bft_prefix_http.rs"]
mod formal_bft_prefix_http;
#[cfg(unix)]
#[path = "host/formal_bft_prefix_transport.rs"]
mod formal_bft_prefix_transport;
#[cfg(unix)]
#[path = "host/formal_bft_read_cli.rs"]
mod formal_bft_read_cli;
#[cfg(unix)]
#[path = "host/formal_bft_read_http.rs"]
mod formal_bft_read_http;
#[path = "host/formal_durable_set_cli.rs"]
mod formal_durable_set_cli;
#[path = "host/formal_effect_delivery_store.rs"]
mod formal_effect_delivery_store;
#[path = "host/formal_effect_outbox.rs"]
mod formal_effect_outbox;
#[path = "host/formal_effect_transport.rs"]
mod formal_effect_transport;
#[path = "host/formal_live_config.rs"]
mod formal_live_config;
#[cfg(all(test, unix))]
#[path = "host/formal_live_crash_endpoint.rs"]
mod formal_live_crash_endpoint;
#[cfg(unix)]
#[path = "host/formal_live_effect_delivery.rs"]
mod formal_live_effect_delivery;
#[cfg(unix)]
#[path = "host/formal_live_history.rs"]
mod formal_live_history;
#[cfg(unix)]
#[path = "host/formal_live_reconcile.rs"]
mod formal_live_reconcile;
#[cfg(test)]
#[path = "host/formal_live_recovery_test_support.rs"]
mod formal_live_recovery_test_support;
#[cfg(unix)]
#[path = "host/formal_live_runtime.rs"]
mod formal_live_runtime;
#[cfg(unix)]
#[path = "host/formal_live_service.rs"]
mod formal_live_service;
#[cfg(all(test, unix))]
pub(crate) use formal_bft_read_cli::{
    run_until_shutdown as run_formal_bft_read_cli_until_shutdown, tests::install_handoff_for_test,
};
#[path = "host/gameplay_submit_api.rs"]
mod gameplay_submit_api;
#[path = "host/governance_registry.rs"]
mod governance_registry;
#[path = "host/identity_provision.rs"]
mod identity_provision;
#[path = "host/identity_receipt.rs"]
mod identity_receipt;
#[cfg(test)]
#[path = "host/identity_receipt_tests.rs"]
mod identity_receipt_tests;
#[path = "host/inspect_cli.rs"]
mod inspect_cli;
#[path = "host/local_provider_setup.rs"]
mod local_provider_setup;
#[path = "host/main_token_submit_api.rs"]
mod main_token_submit_api;
#[path = "host/membership_cli.rs"]
mod membership_cli;
#[path = "host/module_release_attestation_submit_api.rs"]
mod module_release_attestation_submit_api;
#[path = "host/native_environment_capture.rs"]
mod native_environment_capture;
#[path = "host/node_keypair_config.rs"]
mod node_keypair_config;
#[path = "host/node_lifecycle.rs"]
mod node_lifecycle;
#[path = "host/node_operations.rs"]
mod node_operations;
#[path = "host/node_points_observation.rs"]
mod node_points_observation;
#[cfg(test)]
#[path = "host/node_points_runtime_closure.rs"]
mod node_points_runtime_closure;
#[path = "host/p2p_status.rs"]
mod p2p_status;
#[path = "host/peer_record_cli.rs"]
mod peer_record_cli;
#[path = "host/preview_world_init.rs"]
mod preview_world_init;
#[path = "host/publication_lifecycle.rs"]
mod publication_lifecycle;
#[path = "host/publication_proof.rs"]
mod publication_proof;
#[path = "host/rebuild_status.rs"]
mod rebuild_status;
#[cfg(test)]
#[path = "host/rebuild_status_tests.rs"]
mod rebuild_status_tests;
#[path = "host/reward_runtime_settlement.rs"]
mod reward_runtime_settlement;
#[path = "host/reward_runtime_worker.rs"]
mod reward_runtime_worker;
#[path = "host/runtime_authority.rs"]
mod runtime_authority;
#[path = "host/runtime_file_io.rs"]
mod runtime_file_io;
#[path = "host/runtime_status_util.rs"]
mod runtime_status_util;
#[path = "host/startup_reconcile.rs"]
mod startup_reconcile;
#[path = "host/status_admission.rs"]
mod status_admission;
#[path = "host/status_payload.rs"]
mod status_payload;
#[path = "host/status_server_support.rs"]
mod status_server_support;
#[path = "host/storage_metrics.rs"]
mod storage_metrics;
#[path = "host/traffic_profile.rs"]
mod traffic_profile;
#[path = "host/traffic_status.rs"]
mod traffic_status;
#[path = "host/transfer_submit_api.rs"]
mod transfer_submit_api;
#[path = "host/wasm_status.rs"]
mod wasm_status;
#[cfg(unix)]
#[path = "host/world_initialization.rs"]
mod world_initialization;
#[path = "host/world_service_api.rs"]
mod world_service_api;
#[path = "host/world_writer_lock.rs"]
mod world_writer_lock;
#[cfg(test)]
use self::cli::{DEFAULT_NODE_ID, DEFAULT_STATUS_BIND, DEFAULT_WORLD_ID, parse_validator_spec};
use balances_api::build_chain_balances_payload;
#[cfg(test)]
use balances_api::build_chain_balances_payload_from_world;
use cli::{
    CliOptions, DEFAULT_REPLICATION_NETWORK_LISTEN, DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_STATE_FILE,
    DEFAULT_REWARD_RUNTIME_REPORT_DIR, DEFAULT_REWARD_RUNTIME_STATE_FILE,
    DEFAULT_REWARD_RUNTIME_STORAGE_METRICS_FILE, parse_host_port, parse_options, print_help,
};
use execution_bridge::NodeRuntimeExecutionDriver;
use execution_role::{node_role_materializes_execution_state, node_role_requires_execution_commit};
use feedback_submit_api::{
    ChainFeedbackSubmitResponse, FeedbackSubmitSigner, build_feedback_create_request,
    extract_http_json_body, parse_feedback_submit_request, write_feedback_submit_error,
};
use p2p_status::{
    applied_runtime_user_mode_label, build_live_node_network_policy_recommendation,
    build_node_network_policy,
};
use reward_runtime_worker::{
    RewardRuntimeWorkerConfig, SharedRewardRuntimeMetrics, init_shared_metrics, poll_worker_error,
    snapshot_metrics, start_reward_runtime_worker, stop_reward_runtime_worker,
};
pub(crate) use runtime_file_io::write_bytes_atomic;
#[cfg(test)]
use status_payload::build_chain_status_payload;
#[cfg(test)]
use status_server_support::ChainPeerHealthStatus;
use status_server_support::{
    ChainBalancesResponse, ChainReplicationDebugStatus, poll_chain_status_server_error,
    start_chain_status_server, stop_chain_status_server, write_json_response,
};
use traffic_profile::apply_traffic_profile_to_node_config;
#[cfg(test)]
use traffic_profile::feedback_p2p_config_for_role;
#[path = "host/replication_network.rs"]
mod replication_network;
use replication_network::{
    attach_default_replication_network, initialize_default_peer_record_publication,
    initialize_default_peer_record_receiver,
};
#[cfg(test)]
use replication_network::{
    build_default_peer_record, build_default_replication_network_config,
    create_default_replication_network,
};
#[cfg(test)]
use traffic_status::ChainTrafficStatus;
use traffic_status::build_chain_traffic_status;
use wasm_status::build_chain_wasm_status;

#[cfg(test)]
#[path = "host/execution_bridge_test_stub.rs"]
mod execution_bridge;

const DEFAULT_RECENT_MINT_RECORD_LIMIT: usize = 20;
const STORAGE_METRICS_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
use crate::execution_config::{
    EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER, release_security_policy_for_storage_profile,
};

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    pub runtime_root: PathBuf,
    pub execution_bridge_state_path: PathBuf,
    pub execution_world_dir: PathBuf,
    pub execution_records_dir: PathBuf,
    pub storage_root: PathBuf,
    pub replication_root: PathBuf,
    pub reward_runtime_state_path: PathBuf,
    pub reward_runtime_distfs_probe_state_path: PathBuf,
    pub reward_runtime_report_dir: PathBuf,
    pub reward_runtime_storage_metrics_path: PathBuf,
}

/// Parsed native host configuration. Parse once, inspect paths, then run.
#[derive(Debug, Clone)]
pub struct RuntimeConfig(CliOptions);

impl RuntimeConfig {
    /// Parse the same options accepted by the binary, without process-global args.
    pub fn parse<'a>(args: impl Iterator<Item = &'a str>) -> Result<Self, String> {
        parse_options(args).map(Self)
    }

    /// Resolve all host storage paths using the existing CLI defaults.
    pub fn paths(&self) -> RuntimePaths {
        resolve_runtime_paths(&self.0)
    }
}

/// Run a parsed host configuration, returning failures to the caller.
pub fn run(config: RuntimeConfig) -> Result<(), String> {
    run_chain_runtime(config.0)
}

#[derive(Debug)]
struct StorageMetricsWorker {
    stop_tx: Sender<()>,
    degraded_reason_tx: Sender<Option<String>>,
    join_handle: Option<thread::JoinHandle<()>>,
}

fn start_storage_metrics_worker(
    metrics: storage_metrics::SharedStorageMetrics,
    paths: RuntimePaths,
    profile: StorageProfile,
) -> StorageMetricsWorker {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (degraded_reason_tx, degraded_reason_rx) = mpsc::channel::<Option<String>>();
    let join_handle = thread::spawn(move || {
        let mut current_degraded_reason: Option<String> = None;
        let mut last_refresh = Instant::now()
            .checked_sub(STORAGE_METRICS_REFRESH_INTERVAL)
            .unwrap_or_else(Instant::now);
        loop {
            if stop_rx.try_recv().is_ok() {
                return;
            }
            while let Ok(degraded_reason) = degraded_reason_rx.try_recv() {
                current_degraded_reason = degraded_reason.clone();
                if let Err(err) = storage_metrics::update_shared_storage_metrics_degraded_reason(
                    &metrics,
                    degraded_reason,
                ) {
                    emit_stderr_or_event(
                        Level::WARN,
                        format!("warning: storage metrics degraded reason update failed: {err}")
                            .as_str(),
                        "storage metrics degraded reason update failed",
                    );
                }
            }
            if last_refresh.elapsed() >= STORAGE_METRICS_REFRESH_INTERVAL {
                if let Err(err) = storage_metrics::refresh_shared_storage_metrics(
                    &metrics,
                    &paths,
                    profile,
                    current_degraded_reason.clone(),
                ) {
                    emit_stderr_or_event(
                        Level::WARN,
                        format!("warning: storage metrics refresh failed: {err}").as_str(),
                        "storage metrics refresh failed",
                    );
                }
                last_refresh = Instant::now();
            }
            thread::sleep(Duration::from_millis(300));
        }
    });
    StorageMetricsWorker {
        stop_tx,
        degraded_reason_tx,
        join_handle: Some(join_handle),
    }
}

fn update_storage_metrics_degraded_reason(
    worker: &StorageMetricsWorker,
    degraded_reason: Option<String>,
) {
    let _ = worker.degraded_reason_tx.send(degraded_reason);
}

fn stop_storage_metrics_worker(worker: &mut StorageMetricsWorker) {
    let _ = worker.stop_tx.send(());
    if let Some(join_handle) = worker.join_handle.take() {
        let _ = join_handle.join();
    }
}

/// Dispatch command-line arguments without terminating the calling process.
///
/// The caller owns tracing initialization, argument collection, and exit status.
pub fn run_cli(raw_args: Vec<String>) -> Result<(), String> {
    if let Some(result) = identity_receipt::dispatch(raw_args.iter().map(String::as_str)) {
        if let Err(err) = result {
            error!(error = %err, "identity command failed");
            return Err(err);
        }
        return Ok(());
    }
    #[cfg(unix)]
    if let Some(result) = world_initialization::dispatch(raw_args.iter().map(String::as_str)) {
        if let Err(err) = result {
            error!(error = %err, "world initialization command failed");
            return Err(err);
        }
        return Ok(());
    }
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let options = match parse_options(raw_args.iter().map(|arg| arg.as_str())) {
        Ok(options) => options,
        Err(err) => {
            error!(error = %err, "failed to parse chain runtime options");
            print_help();
            return Err(err);
        }
    };

    if let Err(err) = run_chain_runtime(options) {
        error!(error = %err, "oasis7_chain_runtime failed");
        return Err(err);
    }
    Ok(())
}

fn run_chain_runtime(options: CliOptions) -> Result<(), String> {
    if options.initialize_peer_record_publication {
        let mut initialization_options = options;
        apply_network_tier_manifest_defaults(&mut initialization_options);
        return initialize_default_peer_record_publication(&initialization_options);
    }
    if options.initialize_peer_record_receiver {
        let mut initialization_options = options;
        apply_network_tier_manifest_defaults(&mut initialization_options);
        return initialize_default_peer_record_receiver(&initialization_options);
    }
    cli::validate_local_participant_options(&options)?;
    if let Some(config_path) = options.formal_live_config.as_deref() {
        if options.guarded_initial_config.is_some() {
            return Err(
                "--formal-live-config cannot be combined with --guarded-initial-config".into(),
            );
        }
        #[cfg(unix)]
        {
            return formal_live_runtime::run(config_path);
        }
        #[cfg(not(unix))]
        {
            let _ = config_path;
            return Err("formal live runtime currently requires Unix native storage".into());
        }
    }
    if options.guarded_initial_config.is_some() {
        return controlled_live_runtime::run(dev_cli::GuardedWorldOptions {
            config_path: options
                .guarded_initial_config
                .clone()
                .ok_or("guarded configuration missing")?,
        });
    }
    let mut options = options;
    apply_network_tier_manifest_defaults(&mut options);
    cli::validate_local_participant_options(&options)?;
    let mut participant_control = cli::devlocal_participant_config(&options)?;
    let trace_session_id = oasis7::observability::resolve_trace_session_id("oasis7_chain_runtime");
    info!(
        trace_session_id = %trace_session_id,
        node_id = %options.node_id,
        world_id = %options.world_id,
        node_role = ?options.node_role,
        status_bind = %options.status_bind,
        storage_profile = %options.storage_profile.as_str(),
        traffic_profile = %options.traffic_profile.as_str(),
        reward_runtime_enabled = options.reward_runtime_enabled,
        "starting chain runtime"
    );
    let paths = resolve_runtime_paths(&options);
    let _world_writer_lock =
        world_writer_lock::acquire_live_world_writer_lock(paths.execution_world_dir.as_path())?;
    let keypair = node_keypair_config::ensure_node_keypair_in_config(Path::new(
        options.config_path.as_str(),
    ))?;
    let storage_profile_config = StorageProfileConfig::from(options.storage_profile);
    let release_security_policy =
        release_security_policy_for_storage_profile(options.storage_profile);

    let runtime_authority_binding =
        runtime_authority::load_runtime_authority_binding_for_node_with_world_id(
            paths.execution_world_dir.as_path(),
            options.node_id.as_str(),
            options.node_role,
            options.world_id.as_str(),
            options.genesis_validator_registry_path.as_deref(),
            options.deployment_inventory_path.as_deref(),
            options.loaded_network_tier_manifest.as_ref(),
        )?;
    cli::prepare_participant_world(&options, &paths, &keypair, &mut participant_control)?;
    let mut config = NodeConfig::new(
        options.node_id.clone(),
        options.world_id.clone(),
        options.node_role,
    )
    .and_then(|cfg| cfg.with_tick_interval(Duration::from_millis(options.node_tick_ms)))
    .map_err(|err| format!("failed to build node config: {err:?}"))?;
    config = config
        .with_network_policy(build_node_network_policy(&options))
        .map_err(|err| format!("failed to apply network policy: {err:?}"))?;

    let (_validators, _validator_signer_bindings, mut pos_config) =
        if options.loaded_network_tier_manifest.is_some() || !options.development_runtime {
            // Authority and registry preparation above already checked the World
            // binding. Read its effective registry; raw JSON is not a trust source
            // and no remote validator key is provisionally root-derived.
            let verified_world = runtime_authority::load_authority_world(
                paths.execution_world_dir.as_path(),
                options.world_id.as_str(),
            )?;
            let registry = verified_world
            .resolve_governance_effective_finality_signer_registry()
            .map_err(|err| format!("verified World signer registry: {err:?}"))?
            .ok_or(
                "formal/authority-bound startup requires a complete verified World signer registry",
            )?;
            governance_registry::validate_world_finality_signer_completeness(
                &registry,
                &options.node_validators,
            )?;
            let pos = governance_registry::node_pos_config_from_world_finality_registry(
                &registry,
                &config.pos_config,
            );
            if pos.validators.is_empty()
                || pos.validators.iter().any(|validator| {
                    !pos.validator_signer_public_keys
                        .contains_key(&validator.validator_id)
                })
            {
                return Err("verified World registry omits required validator signer keys".into());
            }
            (
                pos.validators.clone(),
                pos.validator_signer_public_keys.clone(),
                pos,
            )
        } else {
            let validators = if options.node_validators.is_empty() {
                config.pos_config.validators.clone()
            } else {
                options.node_validators.clone()
            };
            let bindings = build_validator_signer_public_keys(
                validators.as_slice(),
                &keypair,
                &options.node_validator_signer_public_keys,
            )?;
            let pos = NodePosConfig::ethereum_like(validators.clone())
                .with_validator_signer_public_keys(bindings.clone())
                .map_err(|err| {
                    format!("failed to apply engineering validator signer bindings: {err:?}")
                })?;
            (validators, bindings, pos)
        };
    pos_config.slot_duration_ms = options.pos_slot_duration_ms;
    pos_config.slot_clock_genesis_unix_ms = options.pos_slot_clock_genesis_unix_ms;
    pos_config = pos_config
        .with_ticks_per_slot(options.pos_ticks_per_slot)
        .and_then(|cfg| cfg.with_proposal_tick_phase(options.pos_proposal_tick_phase))
        .and_then(|cfg| cfg.with_max_past_slot_lag(options.pos_max_past_slot_lag))
        .map_err(|err| format!("failed to apply PoS clock options: {err:?}"))?
        .with_adaptive_tick_scheduler_enabled(options.pos_adaptive_tick_scheduler_enabled);
    config = config
        .with_pos_config(pos_config)
        .map_err(|err| format!("failed to apply node pos config: {err:?}"))?;
    config = config
        .with_auto_attest_all_validators(options.node_auto_attest_all_validators)
        .with_allow_local_proposals(matches!(options.node_role, NodeRole::Sequencer));
    let require_execution = node_role_requires_execution_commit(options.node_role);
    let materialize_execution = node_role_materializes_execution_state(options.node_role);
    if options.capture_schedule_recipe_history && !materialize_execution {
        return Err("history capture requires an actual execution driver".into());
    }
    config = config
        .with_require_execution_on_commit(require_execution)
        .with_require_peer_execution_hashes(require_execution);

    if !options.node_gossip_peers.is_empty() && options.node_gossip_bind.is_none() {
        return Err("--node-gossip-peer requires --node-gossip-bind".to_string());
    }
    if let Some(bind_addr) = options.node_gossip_bind {
        config = config.with_gossip_optional(bind_addr, options.node_gossip_peers.clone());
    }
    config = apply_traffic_profile_to_node_config(config, &options)?;
    config = governance_registry::apply_world_governance_registry_overrides_with_context(
        config,
        paths.execution_world_dir.as_path(),
        &release_security_policy,
        participant_control.authority.as_ref(),
    )?;
    if options.local_test_provider_authority_path.is_some() {
        local_provider_setup::ensure_local_test_provider_authority(
            &options,
            paths.execution_world_dir.as_path(),
        )?;
    }
    #[cfg(not(test))]
    let local_execution_bootstrap = options
        .local_test_provider_authority_path
        .as_ref()
        .filter(|_| {
            !(paths
                .execution_world_dir
                .join("world-service-identity.json")
                .exists()
                && paths.execution_records_dir.join("latest.json").exists())
        })
        .map(|_| {
            let world = execution_bridge::load_execution_world_with_policy(
                paths.execution_world_dir.as_path(),
                release_security_policy.clone(),
            )?;
            let finality_block_hash = options
                .local_test_provider_finality_block_hash
                .as_deref()
                .ok_or_else(|| "local test finality marker is missing".to_string())?;
            execution_bridge::derive_local_execution_bootstrap(
                paths.execution_world_dir.as_path(),
                paths.execution_records_dir.as_path(),
                options.world_id.as_str(),
                world.state().time,
                finality_block_hash,
                &release_security_policy,
            )
        })
        .transpose()?;
    #[cfg(not(test))]
    let local_execution_bootstrap = if paths
        .execution_world_dir
        .join("world-service-identity.json")
        .exists()
        && paths.execution_records_dir.join("latest.json").exists()
    {
        Some(
            execution_bridge::derive_service_execution_bootstrap_with_context(
                &paths.execution_world_dir,
                &paths.execution_records_dir,
                &paths.storage_root,
                &options.world_id,
                &execution_bridge::world_service_read::RestoreContext::new(
                    release_security_policy.clone(),
                    participant_control.authority.clone(),
                )?,
            )?,
        )
    } else {
        local_execution_bootstrap
    };
    #[cfg(test)]
    let local_execution_bootstrap: Option<oasis7_node::NodeExecutionBootstrap> = None;
    let effective_validator_signer_bindings =
        config.pos_config.validator_signer_public_keys.clone();
    let replication_remote_writer_allowlist =
        build_replication_remote_writer_allowlist(effective_validator_signer_bindings.values());
    let replication_fetch_requester_allowlist = build_replication_fetch_requester_allowlist(
        effective_validator_signer_bindings.values(),
        &options.replication_fetch_requester_public_keys,
    );
    let allow_any_signed_fetch_requester =
        network_tier_allows_open_observer_fetch(options.loaded_network_tier_manifest.as_ref());
    config = config.with_replication(build_node_replication_config(
        options.node_id.as_str(),
        paths.replication_root.as_path(),
        &keypair,
        &storage_profile_config,
        replication_remote_writer_allowlist.as_slice(),
        replication_fetch_requester_allowlist.as_slice(),
        allow_any_signed_fetch_requester,
    )?);
    let startup_reconcile_bind_guard = reserve_startup_reconcile_bind_guard(&options)?;
    if let Some(report) = startup_reconcile::reconcile_startup_state_from_execution_latest(
        &paths,
        options.world_id.as_str(),
    )? {
        emit_stderr_or_event(
            Level::INFO,
            format!(
                "startup reconciled node pos state from execution latest: previous_committed_height={} reconciled_height={}",
                report.previous_committed_height, report.reconciled_height
            )
            .as_str(),
            "startup reconciled node pos state from execution latest",
        );
    }
    drop(startup_reconcile_bind_guard);

    let publication_lifecycle_observer = publication_lifecycle::PublicationLifecycleObserver::new(
        config.node_id.clone(),
        config.player_id.clone(),
        config.world_id.clone(),
        config.role,
        config.replication.is_some(),
        options.loaded_network_tier_manifest.clone(),
        paths.execution_world_dir.clone(),
        paths.execution_records_dir.clone(),
    );
    let mut runtime =
        NodeRuntime::new(config).with_consensus_progress_observer(publication_lifecycle_observer);
    if let Some(baseline) = local_execution_bootstrap.as_ref() {
        runtime = runtime.with_local_execution_bootstrap(baseline.clone());
    }
    if materialize_execution {
        let mut execution_driver = if participant_control.authority.is_some() {
            NodeRuntimeExecutionDriver::new_with_participant_paths(
                &paths,
                &storage_profile_config,
                participant_control.clone(),
            )
        } else if let Some(baseline) = local_execution_bootstrap {
            #[cfg(not(test))]
            {
                NodeRuntimeExecutionDriver::new_with_local_bootstrap(
                    paths.execution_bridge_state_path.clone(),
                    paths.execution_world_dir.clone(),
                    paths.execution_records_dir.clone(),
                    paths.storage_root.clone(),
                    &storage_profile_config,
                    baseline,
                )
            }
            #[cfg(test)]
            {
                let _ = baseline;
                NodeRuntimeExecutionDriver::new_with_storage_profile(
                    paths.execution_bridge_state_path.clone(),
                    paths.execution_world_dir.clone(),
                    paths.execution_records_dir.clone(),
                    paths.storage_root.clone(),
                    &storage_profile_config,
                )
            }
        } else {
            NodeRuntimeExecutionDriver::new_with_storage_profile(
                paths.execution_bridge_state_path.clone(),
                paths.execution_world_dir.clone(),
                paths.execution_records_dir.clone(),
                paths.storage_root.clone(),
                &storage_profile_config,
            )
        }
        .map_err(|err| format!("failed to initialize execution driver: {err}"))?;
        #[cfg(not(test))]
        let mut provider_bootstrap_authority_paths =
            options.provider_backed_bootstrap_authority_paths.clone();
        #[cfg(not(test))]
        if let Some(local_authority_path) = options.local_test_provider_authority_path.as_ref()
            && !provider_bootstrap_authority_paths.contains(local_authority_path)
        {
            provider_bootstrap_authority_paths.push(local_authority_path.clone());
        }
        #[cfg(not(test))]
        execution_bridge::publish_provider_backed_bootstrap_from_paths(
            &runtime,
            paths.execution_world_dir.as_path(),
            provider_bootstrap_authority_paths.as_slice(),
        )?;
        execution_driver.set_controlled_capture_enabled(options.capture_schedule_recipe_history)?;
        runtime = runtime.with_execution_hook(execution_driver);
    }
    let (mut runtime, replication_network) =
        attach_default_replication_network(runtime, &options, &keypair, &paths)?;

    runtime
        .start()
        .map_err(|err| format!("failed to start node runtime: {err:?}"))?;

    let runtime = Arc::new(Mutex::new(runtime));
    let signer_node_id = options
        .reward_runtime_signer_node_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| options.node_id.clone());
    let signer_keypair = derive_node_consensus_signer_keypair(signer_node_id.as_str(), &keypair)
        .map_err(|err| {
            format!("failed to derive reward runtime signer keypair for {signer_node_id}: {err}")
        })?;
    let mut reward_runtime_node_identity_bindings = effective_validator_signer_bindings;
    if !reward_runtime_node_identity_bindings.contains_key(signer_node_id.as_str()) {
        reward_runtime_node_identity_bindings.insert(
            signer_node_id.clone(),
            signer_keypair.public_key_hex.clone(),
        );
    }
    if !reward_runtime_node_identity_bindings.contains_key(options.node_id.as_str()) {
        let local_keypair =
            derive_node_consensus_signer_keypair(options.node_id.as_str(), &keypair)
                .map_err(|err| format!("failed to derive local node signer keypair: {err}"))?;
        reward_runtime_node_identity_bindings
            .insert(options.node_id.clone(), local_keypair.public_key_hex);
    }
    let reward_runtime_config = RewardRuntimeWorkerConfig {
        enabled: options.reward_runtime_enabled,
        poll_interval: Duration::from_millis(options.node_tick_ms),
        world_id: options.world_id.clone(),
        local_node_id: options.node_id.clone(),
        report_dir: paths.reward_runtime_report_dir.clone(),
        state_path: paths.reward_runtime_state_path.clone(),
        distfs_probe_state_path: paths.reward_runtime_distfs_probe_state_path.clone(),
        storage_root: paths.storage_root.clone(),
        signer_node_id,
        signer_private_key_hex: signer_keypair.private_key_hex,
        reward_runtime_epoch_duration_secs: options.reward_runtime_epoch_duration_secs,
        reward_runtime_auto_redeem: options.reward_runtime_auto_redeem,
        reward_asset_config: RewardAssetConfig {
            points_per_credit: options.reward_points_per_credit,
            ..RewardAssetConfig::default()
        },
        reward_initial_reserve_power_units: options.reward_initial_reserve_power_units,
        reward_runtime_node_identity_bindings,
        reward_distfs_probe_config: options.reward_distfs_probe_config,
    };
    let reward_runtime_metrics = init_shared_metrics(&reward_runtime_config);
    let storage_metrics = storage_metrics::init_shared_storage_metrics(options.storage_profile);
    let mut reward_runtime_worker = start_reward_runtime_worker(
        Arc::clone(&runtime),
        reward_runtime_config,
        Arc::clone(&reward_runtime_metrics),
    )?;
    let mut storage_metrics_worker = start_storage_metrics_worker(
        Arc::clone(&storage_metrics),
        paths.clone(),
        options.storage_profile,
    );
    let feedback_submit_signer = build_feedback_submit_signer(options.node_id.as_str(), &keypair)?;
    let effective_p2p_policy = build_node_network_policy(&options);
    let (status_host, status_port) =
        parse_host_port(options.status_bind.as_str(), "--status-bind")?;
    let mut status_server = start_chain_status_server(
        status_host.as_str(),
        status_port,
        Arc::clone(&runtime),
        Arc::clone(&replication_network),
        options.clone(),
        options.node_id.clone(),
        options.world_id.clone(),
        paths.execution_world_dir.clone(),
        paths.execution_records_dir.clone(),
        paths.storage_root.clone(),
        options.loaded_network_tier_manifest.clone(),
        release_security_policy,
        effective_p2p_policy,
        Arc::clone(&reward_runtime_metrics),
        Arc::clone(&storage_metrics),
        feedback_submit_signer,
        runtime_authority_binding,
    )?;

    runtime_status_util::print_runtime_ready_summary(
        &options,
        &paths,
        status_host.as_str(),
        status_port,
    );

    let mut last_error = String::new();
    let mut current_degraded_reason: Option<String> = None;
    let mut last_storage_metrics_degraded_reason: Option<String> = None;
    loop {
        if let Some(server_err) = poll_chain_status_server_error(&mut status_server)? {
            stop_chain_status_server(&mut status_server);
            stop_reward_runtime_worker(&mut reward_runtime_worker);
            stop_storage_metrics_worker(&mut storage_metrics_worker);
            stop_runtime(&runtime);
            return Err(server_err);
        }
        if let Some(worker) = reward_runtime_worker.as_mut()
            && let Some(worker_err) = poll_worker_error(worker)?
        {
            stop_chain_status_server(&mut status_server);
            stop_reward_runtime_worker(&mut reward_runtime_worker);
            stop_storage_metrics_worker(&mut storage_metrics_worker);
            stop_runtime(&runtime);
            return Err(worker_err);
        }

        if let Ok(snapshot) = runtime.lock().map(|locked| locked.snapshot()) {
            current_degraded_reason = snapshot.last_error.clone();
            if let Some(err) = snapshot.last_error
                && err != last_error
            {
                emit_stderr_or_event(
                    Level::WARN,
                    format!("node runtime reported error: {err}").as_str(),
                    "node runtime reported error",
                );
                last_error = err;
            }
        }
        if current_degraded_reason != last_storage_metrics_degraded_reason {
            last_storage_metrics_degraded_reason = current_degraded_reason.clone();
            update_storage_metrics_degraded_reason(
                &storage_metrics_worker,
                current_degraded_reason.clone(),
            );
        }

        thread::sleep(Duration::from_millis(300));
    }
}

#[derive(Debug)]
struct StartupReconcileBindGuard {
    _tcp_listeners: Vec<TcpListener>,
    _udp_sockets: Vec<UdpSocket>,
}

fn reserve_startup_reconcile_bind_guard(
    options: &CliOptions,
) -> Result<StartupReconcileBindGuard, String> {
    let mut tcp_listeners = Vec::new();
    let mut udp_sockets = Vec::new();
    let (status_host, status_port) =
        parse_host_port(options.status_bind.as_str(), "--status-bind")?;
    tcp_listeners.push(
        TcpListener::bind((status_host.as_str(), status_port)).map_err(|err| {
            format!(
                "startup reconcile preflight failed: status bind {} unavailable: {err}",
                options.status_bind
            )
        })?,
    );
    if let Some(gossip_bind) = options.node_gossip_bind {
        udp_sockets.push(UdpSocket::bind(gossip_bind).map_err(|err| {
            format!(
                "startup reconcile preflight failed: gossip bind {} unavailable: {err}",
                gossip_bind
            )
        })?);
    }
    let listen_addrs = if options.replication_network_listen_addrs.is_empty() {
        vec![DEFAULT_REPLICATION_NETWORK_LISTEN.to_string()]
    } else {
        options.replication_network_listen_addrs.clone()
    };
    for listen_addr in listen_addrs {
        startup_reconcile::reserve_replication_listen_addr(
            listen_addr.as_str(),
            &mut tcp_listeners,
            &mut udp_sockets,
        )?;
    }
    Ok(StartupReconcileBindGuard {
        _tcp_listeners: tcp_listeners,
        _udp_sockets: udp_sockets,
    })
}

pub(crate) fn resolve_runtime_paths(options: &CliOptions) -> RuntimePaths {
    let root = options.runtime_root.clone().unwrap_or_else(|| {
        Path::new("output")
            .join("chain-runtime")
            .join(options.node_id.as_str())
    });
    RuntimePaths {
        runtime_root: root.clone(),
        execution_bridge_state_path: options
            .execution_bridge_state_path
            .clone()
            .unwrap_or_else(|| root.join("reward-runtime-execution-bridge-state.json")),
        execution_world_dir: options
            .execution_world_dir
            .clone()
            .unwrap_or_else(|| root.join("reward-runtime-execution-world")),
        execution_records_dir: options
            .execution_records_dir
            .clone()
            .unwrap_or_else(|| root.join("reward-runtime-execution-records")),
        storage_root: options
            .storage_root
            .clone()
            .unwrap_or_else(|| root.join("store")),
        replication_root: options.replication_root.clone().unwrap_or_else(|| {
            Path::new("output")
                .join("node-distfs")
                .join(options.node_id.as_str())
        }),
        reward_runtime_state_path: root.join(DEFAULT_REWARD_RUNTIME_STATE_FILE),
        reward_runtime_distfs_probe_state_path: root
            .join(DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_STATE_FILE),
        reward_runtime_report_dir: root.join(DEFAULT_REWARD_RUNTIME_REPORT_DIR),
        reward_runtime_storage_metrics_path: root.join(DEFAULT_REWARD_RUNTIME_STORAGE_METRICS_FILE),
    }
}

fn stop_runtime(runtime: &Arc<Mutex<NodeRuntime>>) {
    let mut locked = match runtime.lock() {
        Ok(locked) => locked,
        Err(_) => {
            emit_stderr_or_event(
                Level::ERROR,
                "failed to stop node runtime: lock poisoned",
                "failed to stop node runtime: lock poisoned",
            );
            return;
        }
    };
    if let Err(err) = locked.stop() {
        emit_stderr_or_event(
            Level::ERROR,
            format!("failed to stop node runtime: {err:?}").as_str(),
            "failed to stop node runtime",
        );
    }
}

fn build_node_replication_config(
    node_id: &str,
    replication_root: &Path,
    keypair: &node_keypair_config::NodeKeypairConfig,
    storage_profile: &StorageProfileConfig,
    remote_writer_allowlist: &[String],
    fetch_requester_allowlist: &[String],
    allow_any_signed_fetch_requester: bool,
) -> Result<NodeReplicationConfig, String> {
    let signer_keypair = derive_node_consensus_signer_keypair(node_id, keypair)?;
    NodeReplicationConfig::new(replication_root.to_path_buf())
        .and_then(|cfg| {
            cfg.with_max_hot_commit_messages(storage_profile.replication_max_hot_commit_messages)
        })
        .and_then(|cfg| {
            cfg.with_signing_keypair(
                signer_keypair.private_key_hex,
                signer_keypair.public_key_hex,
            )
        })
        .and_then(|cfg| {
            if remote_writer_allowlist.is_empty() {
                Ok(cfg)
            } else {
                cfg.with_remote_writer_allowlist(remote_writer_allowlist.to_vec())
            }
        })
        .and_then(|cfg| {
            if fetch_requester_allowlist.is_empty() {
                Ok(cfg)
            } else {
                cfg.with_fetch_requester_allowlist(fetch_requester_allowlist.to_vec())
            }
        })
        .map(|cfg| cfg.with_any_signed_fetch_requester_allowed(allow_any_signed_fetch_requester))
        .map_err(|err| format!("failed to build node replication config: {err:?}"))
}

fn build_replication_remote_writer_allowlist<'a>(
    validator_signer_public_keys: impl IntoIterator<Item = &'a String>,
) -> Vec<String> {
    normalize_replication_allowlist(validator_signer_public_keys.into_iter().cloned().collect())
}

fn build_replication_fetch_requester_allowlist<'a>(
    validator_signer_public_keys: impl IntoIterator<Item = &'a String>,
    explicit_fetch_requester_public_keys: &[String],
) -> Vec<String> {
    let mut allowlist: Vec<String> = validator_signer_public_keys.into_iter().cloned().collect();
    allowlist.extend(explicit_fetch_requester_public_keys.iter().cloned());
    normalize_replication_allowlist(allowlist)
}

fn normalize_replication_allowlist(mut allowlist: Vec<String>) -> Vec<String> {
    allowlist.sort();
    allowlist.dedup();
    allowlist
}

fn network_tier_allows_open_observer_fetch(
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> bool {
    let Some(loaded) = loaded_network_tier_manifest else {
        return false;
    };
    loaded.manifest.tier == "public_testnet"
        && loaded.manifest.validator_policy.allow_observer_nodes
}

pub(crate) fn apply_network_tier_manifest_defaults(options: &mut CliOptions) {
    let Some(loaded_manifest) = options.loaded_network_tier_manifest.as_ref() else {
        return;
    };
    if options.world_id.trim().is_empty() || options.world_id == cli::DEFAULT_WORLD_ID {
        options.world_id = loaded_manifest.manifest.chain_id.clone();
    }
}

#[cfg(test)]
#[path = "host/execution_bridge_real_tests.rs"]
mod execution_bridge_real_tests;

#[cfg(test)]
#[path = "host/oasis7_chain_runtime_network_tier_tests.rs"]
mod network_tier_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_observability_readiness_tests.rs"]
mod observability_readiness_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_observability_replication_tests.rs"]
mod observability_replication_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_observability_slashing_tests.rs"]
mod observability_slashing_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_observability_state_sync_tests.rs"]
mod observability_state_sync_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_observability_storage_challenge_tests.rs"]
mod observability_storage_challenge_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_observability_tests.rs"]
mod observability_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_observability_transport_tests.rs"]
mod observability_transport_tests;
#[cfg(test)]
#[path = "host/peer_record_publication_tests.rs"]
mod peer_record_publication_tests;
#[cfg(test)]
#[path = "host/peer_record_receiver_tests.rs"]
mod peer_record_receiver_tests;
#[cfg(test)]
#[path = "host/peer_record_transition_tests.rs"]
mod peer_record_transition_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_status_payload_chain_proof_tests.rs"]
mod status_payload_chain_proof_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_status_payload_observer_tests.rs"]
mod status_payload_observer_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_status_payload_tests.rs"]
mod status_payload_tests;
#[cfg(test)]
#[path = "host/oasis7_chain_runtime_tests.rs"]
mod tests;

#[cfg(test)]
use crate::execution_config::EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER;
