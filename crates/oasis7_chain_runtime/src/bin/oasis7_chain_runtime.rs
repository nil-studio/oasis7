#[cfg(test)]
#[path = "../../../oasis7/src/world_service/formal_read_fixtures.rs"]
mod formal_read_fixtures;

use std::collections::BTreeMap;
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7::observability::{emit_stderr_or_event, init_tracing};
use oasis7_node::{
    Libp2pReplicationNetwork, Libp2pReplicationNetworkConfig, NodeConfig,
    NodeConsensusSlashingReceiptSnapshot, NodeNetworkPolicy, NodePosConfig, NodeReplicationConfig,
    NodeReplicationNetworkHandle, NodeRole, NodeRuntime, PosConsensusStatus, PosValidator,
    derive_libp2p_identity_keypair,
};
use oasis7_proto::distributed_dht::{PeerDiscoverySource, PeerRecord};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::{
    NodeAssetBalance, NodeRewardMintRecord, ReleaseSecurityPolicy, RewardAssetConfig,
};
use runtime_status_util::now_unix_ms;
use serde::Serialize;
use tracing::{Level, error, info};
#[path = "oasis7_chain_runtime/agent_claim_api.rs"]
mod agent_claim_api;
#[path = "oasis7_chain_runtime/balances_api.rs"]
mod balances_api;
#[path = "oasis7_chain_runtime/signer_derivation.rs"]
mod signer_derivation;
use signer_derivation::{
    build_feedback_submit_signer, build_validator_signer_public_keys,
    derive_node_consensus_signer_keypair, derive_node_libp2p_identity_keypair_config,
};
#[path = "oasis7_chain_runtime/cli.rs"]
mod cli;
#[path = "oasis7_chain_runtime/controlled_bootstrap_cli.rs"]
mod controlled_bootstrap_cli;
#[path = "oasis7_chain_runtime/controlled_execution_policy.rs"]
mod controlled_execution_policy;
#[path = "oasis7_chain_runtime/controlled_history_cli.rs"]
mod controlled_history_cli;
#[path = "oasis7_chain_runtime/controlled_live_config.rs"]
mod controlled_live_config;
#[path = "oasis7_chain_runtime/controlled_live_runtime.rs"]
mod controlled_live_runtime;
#[path = "oasis7_chain_runtime/dev_cli.rs"]
mod dev_cli;
#[path = "oasis7_chain_runtime/development_runtime.rs"]
mod development_runtime;
#[path = "oasis7_chain_runtime/distfs_probe_runtime.rs"]
mod distfs_probe_runtime;
#[path = "oasis7_chain_runtime/executable_image.rs"]
mod executable_image;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "oasis7_chain_runtime/execution_bridge/mod.rs"]
mod execution_bridge;
#[path = "oasis7_chain_runtime/execution_role.rs"]
mod execution_role;
#[path = "oasis7_chain_runtime/explorer_p0_api.rs"]
mod explorer_p0_api;
#[path = "oasis7_chain_runtime/feedback_submit_api.rs"]
mod feedback_submit_api;
#[path = "oasis7_chain_runtime/formal_durable_set_cli.rs"]
mod formal_durable_set_cli;
#[path = "oasis7_chain_runtime/formal_effect_delivery_store.rs"]
mod formal_effect_delivery_store;
#[path = "oasis7_chain_runtime/formal_effect_outbox.rs"]
mod formal_effect_outbox;
#[path = "oasis7_chain_runtime/formal_effect_transport.rs"]
mod formal_effect_transport;
#[path = "oasis7_chain_runtime/formal_live_config.rs"]
mod formal_live_config;
#[cfg(all(test, unix))]
#[path = "oasis7_chain_runtime/formal_live_crash_endpoint.rs"]
mod formal_live_crash_endpoint;
#[cfg(unix)]
#[path = "oasis7_chain_runtime/formal_live_effect_delivery.rs"]
mod formal_live_effect_delivery;
#[cfg(unix)]
#[path = "oasis7_chain_runtime/formal_live_history.rs"]
mod formal_live_history;
#[cfg(unix)]
#[path = "oasis7_chain_runtime/formal_live_reconcile.rs"]
mod formal_live_reconcile;
#[cfg(test)]
#[path = "oasis7_chain_runtime/formal_live_recovery_test_support.rs"]
mod formal_live_recovery_test_support;
#[cfg(unix)]
#[path = "oasis7_chain_runtime/formal_live_runtime.rs"]
mod formal_live_runtime;
#[cfg(unix)]
#[path = "oasis7_chain_runtime/formal_live_service.rs"]
mod formal_live_service;
#[path = "oasis7_chain_runtime/gameplay_submit_api.rs"]
mod gameplay_submit_api;
#[path = "oasis7_chain_runtime/governance_registry.rs"]
mod governance_registry;
#[path = "oasis7_chain_runtime/identity_provision.rs"]
mod identity_provision;
#[path = "oasis7_chain_runtime/identity_receipt.rs"]
mod identity_receipt;
#[cfg(test)]
#[path = "oasis7_chain_runtime/identity_receipt_tests.rs"]
mod identity_receipt_tests;
#[path = "oasis7_chain_runtime/inspect_cli.rs"]
mod inspect_cli;
#[path = "oasis7_chain_runtime/local_provider_setup.rs"]
mod local_provider_setup;
#[path = "oasis7_chain_runtime/main_token_submit_api.rs"]
mod main_token_submit_api;
#[path = "oasis7_chain_runtime/membership_cli.rs"]
mod membership_cli;
#[path = "oasis7_chain_runtime/module_release_attestation_submit_api.rs"]
mod module_release_attestation_submit_api;
#[path = "oasis7_chain_runtime/native_environment_capture.rs"]
mod native_environment_capture;
#[path = "oasis7_chain_runtime/node_keypair_config.rs"]
mod node_keypair_config;
#[path = "oasis7_chain_runtime/node_lifecycle.rs"]
mod node_lifecycle;
#[path = "oasis7_chain_runtime/node_operations.rs"]
mod node_operations;
#[path = "oasis7_chain_runtime/p2p_status.rs"]
mod p2p_status;
#[path = "oasis7_chain_runtime/preview_world_init.rs"]
mod preview_world_init;
#[path = "oasis7_chain_runtime/publication_lifecycle.rs"]
mod publication_lifecycle;
#[path = "oasis7_chain_runtime/publication_proof.rs"]
mod publication_proof;
#[path = "oasis7_chain_runtime/rebuild_status.rs"]
mod rebuild_status;
#[cfg(test)]
#[path = "oasis7_chain_runtime/rebuild_status_tests.rs"]
mod rebuild_status_tests;
#[path = "oasis7_chain_runtime/reward_runtime_settlement.rs"]
mod reward_runtime_settlement;
#[path = "oasis7_chain_runtime/reward_runtime_worker.rs"]
mod reward_runtime_worker;
#[path = "oasis7_chain_runtime/runtime_authority.rs"]
mod runtime_authority;
#[path = "oasis7_chain_runtime/runtime_file_io.rs"]
mod runtime_file_io;
#[path = "oasis7_chain_runtime/runtime_status_util.rs"]
mod runtime_status_util;
#[path = "oasis7_chain_runtime/startup_reconcile.rs"]
mod startup_reconcile;
#[path = "oasis7_chain_runtime/status_admission.rs"]
mod status_admission;
#[path = "oasis7_chain_runtime/status_payload.rs"]
mod status_payload;
#[path = "oasis7_chain_runtime/status_server_support.rs"]
mod status_server_support;
#[path = "oasis7_chain_runtime/storage_metrics.rs"]
mod storage_metrics;
#[path = "oasis7_chain_runtime/traffic_profile.rs"]
mod traffic_profile;
#[path = "oasis7_chain_runtime/traffic_status.rs"]
mod traffic_status;
#[path = "oasis7_chain_runtime/transfer_submit_api.rs"]
mod transfer_submit_api;
#[path = "oasis7_chain_runtime/wasm_status.rs"]
mod wasm_status;
#[cfg(unix)]
#[path = "oasis7_chain_runtime/world_initialization.rs"]
mod world_initialization;
#[path = "oasis7_chain_runtime/world_service_api.rs"]
mod world_service_api;
#[path = "oasis7_chain_runtime/world_writer_lock.rs"]
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
#[cfg(test)]
use traffic_profile::feedback_p2p_config_for_role;
use traffic_profile::{
    apply_traffic_profile_to_node_config, apply_traffic_profile_to_replication_network_config,
};
#[cfg(test)]
use traffic_status::ChainTrafficStatus;
use traffic_status::build_chain_traffic_status;
use wasm_status::build_chain_wasm_status;

#[cfg(test)]
#[path = "oasis7_chain_runtime/execution_bridge_test_stub.rs"]
mod execution_bridge;

const DEFAULT_RECENT_MINT_RECORD_LIMIT: usize = 20;
const STORAGE_METRICS_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER: &str = "retention-degraded";
const EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER: &str = "retention-in-progress";

#[derive(Debug, Clone)]
pub(crate) struct RuntimePaths {
    pub(crate) runtime_root: PathBuf,
    pub(crate) execution_bridge_state_path: PathBuf,
    pub(crate) execution_world_dir: PathBuf,
    pub(crate) execution_records_dir: PathBuf,
    pub(crate) storage_root: PathBuf,
    pub(crate) replication_root: PathBuf,
    pub(crate) reward_runtime_state_path: PathBuf,
    pub(crate) reward_runtime_distfs_probe_state_path: PathBuf,
    pub(crate) reward_runtime_report_dir: PathBuf,
    pub(crate) reward_runtime_storage_metrics_path: PathBuf,
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

fn main() {
    init_tracing("oasis7_chain_runtime");
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if let Some(result) = identity_receipt::dispatch(raw_args.iter().map(String::as_str)) {
        if let Err(err) = result {
            error!(error = %err, "identity command failed");
            process::exit(1);
        }
        return;
    }
    #[cfg(unix)]
    if let Some(result) = world_initialization::dispatch(raw_args.iter().map(String::as_str)) {
        if let Err(err) = result {
            error!(error = %err, "world initialization command failed");
            process::exit(1);
        }
        return;
    }
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }

    let options = match parse_options(raw_args.iter().map(|arg| arg.as_str())) {
        Ok(options) => options,
        Err(err) => {
            error!(error = %err, "failed to parse chain runtime options");
            print_help();
            process::exit(1);
        }
    };

    if let Err(err) = run_chain_runtime(options) {
        error!(error = %err, "oasis7_chain_runtime failed");
        process::exit(1);
    }
}

fn run_chain_runtime(options: CliOptions) -> Result<(), String> {
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
    governance_registry::ensure_world_governance_validator_registry(
        paths.execution_world_dir.as_path(),
        options.genesis_validator_registry_path.as_deref(),
        options.loaded_network_tier_manifest.as_ref(),
    )?;
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
    config = governance_registry::apply_world_governance_registry_overrides(
        config,
        paths.execution_world_dir.as_path(),
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
        Some(execution_bridge::derive_service_execution_bootstrap(
            &paths.execution_world_dir,
            &paths.execution_records_dir,
            &paths.storage_root,
            &options.world_id,
        )?)
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
        let mut execution_driver = if let Some(baseline) = local_execution_bootstrap {
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
        attach_default_replication_network(runtime, &options, &keypair)?;

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

pub(crate) fn release_security_policy_for_storage_profile(
    storage_profile: StorageProfile,
) -> ReleaseSecurityPolicy {
    if matches!(storage_profile, StorageProfile::ReleaseDefault) {
        ReleaseSecurityPolicy::production_hardened()
    } else {
        ReleaseSecurityPolicy::default()
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

fn attach_default_replication_network(
    runtime: NodeRuntime,
    options: &CliOptions,
    root_keypair: &node_keypair_config::NodeKeypairConfig,
) -> Result<(NodeRuntime, Arc<Libp2pReplicationNetwork>), String> {
    let mut network_config = build_default_replication_network_config(options, root_keypair)?;
    network_config.allow_local_handler_fallback_when_no_peers =
        network_config.bootstrap_peers.is_empty();
    let network = Arc::new(Libp2pReplicationNetwork::new(network_config));
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

fn build_default_replication_network_config(
    options: &CliOptions,
    root_keypair: &node_keypair_config::NodeKeypairConfig,
) -> Result<Libp2pReplicationNetworkConfig, String> {
    let mut config = Libp2pReplicationNetworkConfig::default();
    let libp2p_identity_keypair =
        derive_node_libp2p_identity_keypair_config(options.node_id.as_str(), root_keypair)?;
    config.keypair = Some(
        derive_libp2p_identity_keypair(libp2p_identity_keypair.private_key_hex.as_str())
            .map_err(|err| format!("failed to derive libp2p identity keypair: {err:?}"))?,
    );
    config.peer_record = Some(build_default_peer_record(options));
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

pub(crate) fn apply_network_tier_manifest_defaults(options: &mut CliOptions) {
    let Some(loaded_manifest) = options.loaded_network_tier_manifest.as_ref() else {
        return;
    };
    if options.world_id.trim().is_empty() || options.world_id == cli::DEFAULT_WORLD_ID {
        options.world_id = loaded_manifest.manifest.chain_id.clone();
    }
}

fn build_default_peer_record(options: &CliOptions) -> PeerRecord {
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

#[cfg(test)]
#[path = "oasis7_chain_runtime/execution_bridge_real_tests.rs"]
mod execution_bridge_real_tests;

#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_network_tier_tests.rs"]
mod network_tier_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_observability_readiness_tests.rs"]
mod observability_readiness_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_observability_replication_tests.rs"]
mod observability_replication_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_observability_slashing_tests.rs"]
mod observability_slashing_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_observability_state_sync_tests.rs"]
mod observability_state_sync_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_observability_storage_challenge_tests.rs"]
mod observability_storage_challenge_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_observability_tests.rs"]
mod observability_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_observability_transport_tests.rs"]
mod observability_transport_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_status_payload_chain_proof_tests.rs"]
mod status_payload_chain_proof_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_status_payload_observer_tests.rs"]
mod status_payload_observer_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_status_payload_tests.rs"]
mod status_payload_tests;
#[cfg(test)]
#[path = "oasis7_chain_runtime/oasis7_chain_runtime_tests.rs"]
mod tests;
