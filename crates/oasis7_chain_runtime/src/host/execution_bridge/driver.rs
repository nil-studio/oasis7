use std::fs;
use std::time::{Duration, Instant};

use crate::execution_config::ExecutionDriverConfig;
use oasis7::simulator::WorldKernel;
use oasis7_node::{
    EXECUTION_MISSING_PREDECESSOR_RECORD_SIGNATURE, NodeExecutionBootstrap,
    NodeExecutionCheckpointBlob, NodeExecutionCheckpointBundle,
    NodeExecutionCheckpointInstallContext, NodeExecutionCommitContext, NodeExecutionCommitResult,
    NodeExecutionHook, compute_consensus_action_root,
};
use oasis7_proto::storage_profile::StorageProfileConfig;
use oasis7_runtime::{
    BlobStore, ChainResourceDerivationContext, LocalCasStore, RuntimeCommittedTickContext,
    World as RuntimeWorld, WorldError, blake3_hex,
};
use oasis7_wasm_abi::ModuleSandbox;
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};

use super::checkpoint::{
    begin_execution_bridge_retention_transaction, execution_bridge_record_path,
    execution_checkpoint_root_dir, list_execution_bridge_record_heights,
    load_execution_bridge_record, load_execution_checkpoint_manifest,
    maybe_persist_execution_checkpoint_for_record, persist_execution_bridge_record,
    promote_interrupted_execution_bridge_retention,
};
use super::driver_observability::{
    CommitObservation, SimulatorMirrorCommitObservation, emit_commit_observation,
};
pub(crate) use super::driver_persistence::load_execution_world;
pub(crate) use super::driver_persistence::{
    execution_world_persistence_files_missing, load_execution_bridge_state,
    load_execution_world_with_policy, persist_execution_bridge_state, persist_execution_world,
    remove_partial_execution_world_persistence_files,
};
use super::execution_hash::{
    ExecutionHashPayload, execution_resource_commit_hash, execution_resource_context_hash,
    execution_resource_created_at_height,
};
use super::external_effect::{
    build_execution_external_effect_materialization_with_pre_step_root,
    execution_world_snapshot_root, persist_execution_external_effect_materialization,
    validate_execution_external_effect_for_context,
};
pub(crate) use super::local_bootstrap::derive_local_execution_bootstrap;
use super::product_validation_intent::{
    ProductValidationIntentMarkerV1, clear_product_validation_intent,
    load_product_validation_intent, persist_product_validation_intent_for_staged_world,
    world_is_staged_for_product_validation_intent,
};
pub(crate) use super::simulator_mirror::simulator_world_dir_from_execution_world_dir;
use super::simulator_mirror::{load_simulator_execution_world, persist_simulator_execution_world};
use super::{
    ExecutionBridgeRecord, ExecutionBridgeState, ExecutionSimulatorMirrorRecord,
    persist_world_head_proof_for_record,
};
use crate::execution_config::{
    EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER, EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER,
};
#[path = "driver_state.rs"]
mod state;
pub(crate) use state::{ExecutionResourceIdentity, NodeRuntimeExecutionDriver};

#[cfg(all(unix, not(target_arch = "wasm32")))]
#[path = "driver_bft.rs"]
mod bft_execution;
#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
#[path = "driver_bft_unsupported.rs"]
mod bft_execution;

#[path = "driver_simulator_actions.rs"]
mod simulator_actions;

#[path = "driver_construction.rs"]
mod construction;
#[path = "driver_formal_retention.rs"]
mod formal_retention;
#[path = "driver_formal_tick.rs"]
mod formal_tick;
#[path = "driver_participant.rs"]
mod participant;

#[path = "driver_hook.rs"]
mod hook;

impl NodeRuntimeExecutionDriver {
    pub(super) fn new_with_config_and_local_bootstrap(
        state_path: std::path::PathBuf,
        world_dir: std::path::PathBuf,
        records_dir: std::path::PathBuf,
        storage_root: std::path::PathBuf,
        config: ExecutionDriverConfig,
        local_execution_bootstrap: Option<NodeExecutionBootstrap>,
    ) -> Result<Self, String> {
        // A BFT sidecar is an execution-mode fence. Do not let legacy startup
        // clean partial World files, restore Pos records, or reconcile checkpoint
        // transactions before NodeRuntime supplies the fixed BFT configuration.
        if Self::has_bft_material(records_dir.as_path())? {
            if local_execution_bootstrap.is_some()
                || config.devlocal_participant.authority.is_some()
                || config.devlocal_participant.startup.is_some()
            {
                return Err("BFT execution cannot start with local or DevLocal bootstrap".into());
            }
            let execution_world = oasis7::runtime_host::world::new_production_hardened()
                .with_release_security_policy(config.release_security_policy.clone());
            let execution_sandbox: Box<dyn ModuleSandbox + Send> = Box::new(
                WasmExecutor::new(WasmExecutorConfig::default())
                    .map_err(|error| error.to_string())?,
            );
            let mut driver = Self::new_with_sandbox(
                state_path,
                world_dir,
                records_dir,
                storage_root,
                ExecutionBridgeState::default(),
                execution_world,
                execution_sandbox,
                config.hot_window_heights,
                config.checkpoint_interval_heights,
                config.checkpoint_keep_latest,
            );
            driver.devlocal_participant = config.devlocal_participant;
            driver.bft_mode_required = true;
            return Ok(driver);
        }

        let checkpoint_install_transaction =
            super::driver_checkpoint_install::load_checkpoint_install_transaction(
                records_dir.as_path(),
            )?;
        promote_interrupted_execution_bridge_retention(records_dir.as_path())?;
        let state = load_execution_bridge_state(state_path.as_path())?;
        let release_security_policy = config.release_security_policy;
        let devlocal_participant = config.devlocal_participant;
        if devlocal_participant.authority.is_some()
            && execution_world_persistence_files_missing(world_dir.as_path())
        {
            return Err(
                "local participant administration requires a complete existing execution world"
                    .into(),
            );
        }
        remove_partial_execution_world_persistence_files(world_dir.as_path())?;
        let execution_world_bootstrap_required =
            execution_world_persistence_files_missing(world_dir.as_path());
        let execution_world = super::driver_persistence::load_execution_world_with_context(
            world_dir.as_path(),
            release_security_policy,
            devlocal_participant.authority.as_ref(),
        )?;
        let execution_sandbox: Box<dyn ModuleSandbox + Send> = Box::new(
            WasmExecutor::new(WasmExecutorConfig::default()).map_err(|err| err.to_string())?,
        );
        let durable_product_validation_intent =
            load_product_validation_intent(records_dir.as_path())?;
        let mut driver = Self::new_with_sandbox(
            state_path,
            world_dir,
            records_dir,
            storage_root,
            state,
            execution_world,
            execution_sandbox,
            config.hot_window_heights,
            config.checkpoint_interval_heights,
            config.checkpoint_keep_latest,
        );
        driver.devlocal_participant = devlocal_participant;
        driver.pending_product_validation_intent = durable_product_validation_intent;
        driver.local_execution_bootstrap = local_execution_bootstrap;
        if let Some(baseline) = driver.local_execution_bootstrap.clone() {
            driver.apply_local_execution_bootstrap(&baseline)?;
        }
        if let Some(marker) = driver.pending_product_validation_intent.clone() {
            let authoritative_record_exists =
                execution_bridge_record_path(driver.records_dir.as_path(), marker.height).exists();
            if marker.height <= driver.state.last_applied_committed_height
                || authoritative_record_exists
            {
                clear_product_validation_intent(driver.records_dir.as_path())?;
                driver.pending_product_validation_intent = None;
            } else if !world_is_staged_for_product_validation_intent(
                &driver.execution_world,
                &marker,
                driver.state.last_applied_committed_height,
            )? {
                // The marker exists but the world is still the exact
                // predecessor. This is the crash window between marker-first
                // publication and staged-world publication; clear it and let
                // the next committed call execute from the predecessor.
                clear_product_validation_intent(driver.records_dir.as_path())?;
                driver.pending_product_validation_intent = None;
            }
        }
        super::driver_checkpoint_install::recover_checkpoint_install_transaction(
            &mut driver,
            checkpoint_install_transaction,
        )?;
        remove_partial_execution_world_persistence_files(driver.simulator_world_dir.as_path())?;
        let simulator_world_bootstrap_required =
            execution_world_persistence_files_missing(driver.simulator_world_dir.as_path());
        driver.simulator_mirror =
            load_simulator_execution_world(driver.simulator_world_dir.as_path())?;
        // The bridge state file is a cache of the committed head, not the
        // authority for it. A crash can publish a record before this cache is
        // replaced, or the cache can be lost independently. In either case,
        // bootstrapping the world from disk would replay the next commit on a
        // different generation than the durable execution record. Reconcile
        // from the highest valid durable per-height record whenever one exists.
        // The latest pointer is only a convenience pointer and may lag the
        // numbered records after a crash.
        let has_execution_records =
            !list_execution_bridge_record_heights(driver.records_dir.as_path())?.is_empty()
                || driver.records_dir.join("latest.json").exists();
        let unmaterialized_local_bootstrap =
            driver.is_unmaterialized_local_execution_bootstrap()?;
        if driver.pending_product_validation_intent.is_some() {
            // The world directory is the crash-safe continuation for the
            // pre-call intent. The authoritative height record does not exist
            // yet, so restoring it here would erase the intent.
        } else if (driver.state.last_applied_committed_height > 0 || has_execution_records)
            && !unmaterialized_local_bootstrap
        {
            driver.restore_startup_execution_head()?;
        } else {
            if execution_world_bootstrap_required {
                persist_execution_world(driver.world_dir.as_path(), &driver.execution_world)?;
            }
            if simulator_world_bootstrap_required {
                persist_simulator_execution_world(
                    driver.simulator_world_dir.as_path(),
                    &driver.simulator_mirror,
                    None,
                )?;
            }
        }
        driver.pin_devlocal_participant_resource_identity()?;
        driver.validate_devlocal_participant_startup()?;
        Ok(driver)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
    )]
    pub(crate) fn new_with_sandbox(
        state_path: std::path::PathBuf,
        world_dir: std::path::PathBuf,
        records_dir: std::path::PathBuf,
        storage_root: std::path::PathBuf,
        state: ExecutionBridgeState,
        execution_world: RuntimeWorld,
        execution_sandbox: Box<dyn ModuleSandbox + Send>,
        hot_window_heights: u64,
        checkpoint_interval_heights: u64,
        checkpoint_keep_latest: usize,
    ) -> Self {
        let simulator_world_dir = simulator_world_dir_from_execution_world_dir(world_dir.as_path());
        let retention_reconcile_pending = [
            EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER,
            EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER,
        ]
        .iter()
        .any(|marker| records_dir.join(marker).exists());
        let retention_reconcile_next_height = retention_reconcile_pending
            .then(|| state.last_applied_committed_height.saturating_add(1));
        Self {
            state_path,
            world_dir,
            records_dir,
            simulator_world_dir,
            execution_store: LocalCasStore::new(storage_root),
            state,
            execution_world,
            simulator_mirror: WorldKernel::new(),
            execution_sandbox,
            hot_window_heights,
            checkpoint_interval_heights,
            checkpoint_keep_latest,
            retention_reconcile_pending,
            retention_reconcile_next_height,
            pending_product_validation_intent: None,
            devlocal_participant: crate::execution_config::DevLocalParticipantConfig::default(),
            local_execution_bootstrap: None,
            controlled_capture_enabled: false,
            formal_retention_enabled: false,
            formal_retention_delta: None,
            formal_new_results: Vec::new(),
            formal_event_root: None,
            formal_unpruned_journal: None,
            formal_journal_cursors: Vec::new(),
            resource_identity: None,
            bft_execution: None,
            bft_execution_v2: None,
            bft_mode_required: false,
        }
    }
}

#[path = "driver_retention.rs"]
mod retention;
