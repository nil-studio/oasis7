use std::fs;
use std::time::{Duration, Instant};

use crate::release_security_policy_for_storage_profile;
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
use oasis7_simulator::{Action as SimulatorAction, ActionSubmitter, WorldEventKind};
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
use crate::{
    EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER, EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER,
};
#[path = "driver_state.rs"]
mod state;
pub(crate) use state::{ExecutionResourceIdentity, NodeRuntimeExecutionDriver};

#[path = "driver_construction.rs"]
mod construction;
#[path = "driver_formal_retention.rs"]
mod formal_retention;
#[path = "driver_formal_tick.rs"]
mod formal_tick;

impl NodeRuntimeExecutionDriver {
    pub(super) fn new_with_storage_profile_and_local_bootstrap(
        state_path: std::path::PathBuf,
        world_dir: std::path::PathBuf,
        records_dir: std::path::PathBuf,
        storage_root: std::path::PathBuf,
        storage_profile: &StorageProfileConfig,
        local_execution_bootstrap: Option<NodeExecutionBootstrap>,
    ) -> Result<Self, String> {
        let checkpoint_install_transaction =
            super::driver_checkpoint_install::load_checkpoint_install_transaction(
                records_dir.as_path(),
            )?;
        promote_interrupted_execution_bridge_retention(records_dir.as_path())?;
        let state = load_execution_bridge_state(state_path.as_path())?;
        let release_security_policy =
            release_security_policy_for_storage_profile(storage_profile.profile);
        remove_partial_execution_world_persistence_files(world_dir.as_path())?;
        let execution_world_bootstrap_required =
            execution_world_persistence_files_missing(world_dir.as_path());
        let execution_world =
            load_execution_world_with_policy(world_dir.as_path(), release_security_policy)?;
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
            storage_profile.execution_hot_head_heights,
            storage_profile.execution_checkpoint_interval,
            storage_profile.execution_checkpoint_keep as usize,
        );
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
            local_execution_bootstrap: None,
            controlled_capture_enabled: false,
            formal_retention_enabled: false,
            formal_retention_delta: None,
            formal_new_results: Vec::new(),
            formal_event_root: None,
            formal_unpruned_journal: None,
            formal_journal_cursors: Vec::new(),
            resource_identity: None,
        }
    }

    fn apply_simulator_actions(
        &mut self,
        context: &NodeExecutionCommitContext,
        simulator_actions: &[(SimulatorAction, ActionSubmitter)],
    ) -> Result<
        (
            Option<ExecutionSimulatorMirrorRecord>,
            SimulatorMirrorCommitObservation,
        ),
        String,
    > {
        let height = context.height;
        if simulator_actions.is_empty() {
            return Ok((None, SimulatorMirrorCommitObservation::default()));
        }

        let mut rejected_action_count = 0_usize;
        for (action, submitter) in simulator_actions {
            match submitter {
                ActionSubmitter::System => {
                    self.simulator_mirror
                        .submit_action_from_system(action.clone());
                }
                ActionSubmitter::Agent { agent_id } => {
                    self.simulator_mirror
                        .submit_action_from_agent(agent_id.clone(), action.clone());
                }
                ActionSubmitter::Player { player_id } => {
                    self.simulator_mirror
                        .submit_action_from_player(player_id.clone(), action.clone());
                }
            }

            let event = self.simulator_mirror.step().ok_or_else(|| {
                format!(
                    "execution driver simulator mirror step produced no event at height={height}"
                )
            })?;
            if matches!(event.kind, WorldEventKind::ActionRejected { .. }) {
                rejected_action_count = rejected_action_count.saturating_add(1);
            }
        }

        let resource_commit_hash = execution_resource_commit_hash(&context.world_id, height);
        let resource_context = ChainResourceDerivationContext {
            world_id: context.world_id.as_str(),
            chain_id: context.world_id.as_str(),
            genesis_ref: None,
            created_at_height: 0,
            manifest_height: context.height,
            commit_block_hash: Some(resource_commit_hash.as_str()),
            tick: self.simulator_mirror.time(),
        };
        let snapshot_value = self
            .simulator_mirror
            .snapshot_with_chain_resource_context(resource_context);
        let journal_value = self.simulator_mirror.journal_snapshot();
        let snapshot_bytes = super::to_cbor(snapshot_value)?;
        let journal_bytes = super::to_cbor(journal_value)?;

        let snapshot_ref = self
            .execution_store
            .put_bytes(snapshot_bytes.as_slice())
            .map_err(|err| {
                format!(
                    "execution driver simulator CAS snapshot put failed: {:?}",
                    err
                )
            })?;
        let journal_ref = self
            .execution_store
            .put_bytes(journal_bytes.as_slice())
            .map_err(|err| {
                format!(
                    "execution driver simulator CAS journal put failed: {:?}",
                    err
                )
            })?;
        let state_root = blake3_hex(snapshot_bytes.as_slice());

        let observation = SimulatorMirrorCommitObservation {
            action_count: simulator_actions.len(),
            rejected_action_count,
            snapshot_bytes: snapshot_bytes.len(),
            journal_bytes: journal_bytes.len(),
        };

        Ok((
            Some(ExecutionSimulatorMirrorRecord {
                action_count: simulator_actions.len(),
                rejected_action_count,
                journal_len: self.simulator_mirror.journal().len(),
                snapshot_ref,
                journal_ref,
                state_root,
            }),
            observation,
        ))
    }
}

impl NodeExecutionHook for NodeRuntimeExecutionDriver {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        self.on_commit_with_expected(context, None, None)
    }

    fn on_commit_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        expected_execution_block_hash: Option<&str>,
        expected_execution_state_root: Option<&str>,
    ) -> Result<NodeExecutionCommitResult, String> {
        if context.height < self.state.last_applied_committed_height {
            let stale_state_height = self.state.last_applied_committed_height;
            if !self
                .restore_execution_head_from_record(context.world_id.as_str(), context.height)?
            {
                return Err(format!(
                    "execution driver received stale height: context={} state={}",
                    context.height, stale_state_height
                ));
            }
        }
        if context.height == self.state.last_applied_committed_height {
            let record = load_execution_bridge_record(
                execution_bridge_record_path(self.records_dir.as_path(), context.height).as_path(),
            )
            .map_err(|err| {
                format!(
                    "execution driver stale-height restore authoritative record unavailable at height {}: {}",
                    context.height, err
                )
            })?;
            if record.height != context.height || record.world_id != context.world_id {
                return Err(format!(
                    "execution driver stale-height restore world_id mismatch at height {}: context={} record={}",
                    context.height, context.world_id, record.world_id
                ));
            }
            self.validate_equal_height_replay_identity(&record, &context)?;
            let execution_block_hash =
                self.state
                    .last_execution_block_hash
                    .clone()
                    .ok_or_else(|| {
                        "execution driver missing block hash for current height".to_string()
                    })?;
            let execution_state_root =
                self.state
                    .last_execution_state_root
                    .clone()
                    .ok_or_else(|| {
                        "execution driver missing state root for current height".to_string()
                    })?;
            if execution_block_hash != record.execution_block_hash
                || execution_state_root != record.execution_state_root
            {
                return Err(format!(
                    "execution driver stale-height restore state mismatch at height {}",
                    context.height
                ));
            }
            return Ok(NodeExecutionCommitResult {
                execution_height: context.height,
                execution_block_hash: record.execution_block_hash,
                execution_state_root: record.execution_state_root,
            });
        }
        let next_expected_height = self.state.last_applied_committed_height.saturating_add(1);
        if context.height != next_expected_height {
            let predecessor_height = context.height.saturating_sub(1);
            if predecessor_height == 0 {
                return Err(format!(
                    "execution driver received non-contiguous committed height: last_applied={} incoming={}",
                    self.state.last_applied_committed_height, context.height
                ));
            }
            if !self
                .restore_execution_head_from_record(context.world_id.as_str(), predecessor_height)?
            {
                return Err(format!(
                    "{}: last_applied={} incoming={} predecessor={}",
                    EXECUTION_MISSING_PREDECESSOR_RECORD_SIGNATURE,
                    self.state.last_applied_committed_height,
                    context.height,
                    predecessor_height
                ));
            }
            let restored_next_expected_height =
                self.state.last_applied_committed_height.saturating_add(1);
            if context.height != restored_next_expected_height {
                return Err(format!(
                    "execution driver restore failed to close committed height gap: restored_head={} incoming={}",
                    self.state.last_applied_committed_height, context.height
                ));
            }
        }

        let commit_started_at = Instant::now();
        let action_count = context.committed_actions.len();
        let computed_action_root =
            compute_consensus_action_root(context.committed_actions.as_slice())
                .map_err(|err| format!("execution driver compute action root failed: {err:?}"))?;
        if computed_action_root != context.action_root {
            return Err(format!(
                "execution driver action_root mismatch expected={} actual={}",
                computed_action_root, context.action_root
            ));
        }
        // The marker carries pre-step evidence and is published before the
        // staged world; an exact predecessor plus marker is a safe crash
        // window, while a matching staged world resumes without re-invocation.
        let mut pre_step_execution_state_root = None;
        let mut pre_step_external_effect = None;
        let resume_after_product_validation_intent = if let Some(marker) =
            load_product_validation_intent(self.records_dir.as_path())?
        {
            let authoritative_record_exists =
                execution_bridge_record_path(self.records_dir.as_path(), marker.height).exists();
            if marker.height <= self.state.last_applied_committed_height {
                clear_product_validation_intent(self.records_dir.as_path())?;
                false
            } else if authoritative_record_exists && marker.height == context.height {
                // A record can win the race with marker cleanup. Reconcile to
                // that authoritative result instead of resuming the stale
                // pre-call world. Keep the continuation marker durable until
                // the record and its replay identity have both been restored
                // successfully so a transient CAS/read failure remains
                // retryable in-process.
                self.restore_execution_head_from_record(context.world_id.as_str(), context.height)?;
                let record = load_execution_bridge_record(
                    execution_bridge_record_path(self.records_dir.as_path(), context.height)
                        .as_path(),
                )?;
                self.validate_equal_height_replay_identity(&record, &context)?;
                clear_product_validation_intent(self.records_dir.as_path())?;
                return Ok(NodeExecutionCommitResult {
                    execution_height: context.height,
                    execution_block_hash: record.execution_block_hash,
                    execution_state_root: record.execution_state_root,
                });
            } else {
                if marker.height != context.height
                    || marker.world_id != context.world_id
                    || (!marker.action_root.is_empty() && marker.action_root != context.action_root)
                {
                    return Err(format!(
                        "product validation intent marker does not match committed context: marker_height={} context_height={} marker_world={} context_world={}",
                        marker.height, context.height, marker.world_id, context.world_id
                    ));
                }
                if let Some(effect) = marker.pre_step_external_effect.as_ref() {
                    validate_execution_external_effect_for_context(effect, &context)?;
                    pre_step_external_effect = Some(effect.clone());
                }
                if !marker.pre_step_execution_state_root.trim().is_empty() {
                    pre_step_execution_state_root =
                        Some(marker.pre_step_execution_state_root.clone());
                } else if let Some(effect) = pre_step_external_effect.as_ref() {
                    pre_step_execution_state_root =
                        Some(effect.pre_step_execution_state_root.clone());
                }
                let durable_execution_world = load_execution_world_with_policy(
                    self.world_dir.as_path(),
                    self.execution_world.release_security_policy().clone(),
                )?;
                if world_is_staged_for_product_validation_intent(
                    &durable_execution_world,
                    &marker,
                    self.state.last_applied_committed_height,
                )? {
                    self.execution_world = durable_execution_world;
                    self.pending_product_validation_intent = Some(marker);
                    true
                } else {
                    // Marker-first publication completed, but world
                    // publication did not. Reconcile to the durable
                    // predecessor before retrying the tick.
                    self.execution_world = durable_execution_world;
                    clear_product_validation_intent(self.records_dir.as_path())?;
                    if pre_step_execution_state_root.is_none() {
                        pre_step_execution_state_root =
                            Some(execution_world_snapshot_root(&self.execution_world)?);
                    }
                    false
                }
            }
        } else {
            pre_step_execution_state_root =
                Some(execution_world_snapshot_root(&self.execution_world)?);
            false
        };

        let external_effect = match pre_step_external_effect {
            Some(materialization) => materialization,
            None => build_execution_external_effect_materialization_with_pre_step_root(
                &self.execution_world,
                &context,
                pre_step_execution_state_root.as_deref(),
            )?,
        };

        let decode_started_at = Instant::now();
        let (
            decoded_runtime_actions,
            decoded_simulator_actions,
            replicated_provider_backed_bootstrap,
            service_intents,
        ) = super::driver_replicated_input::decode_committed_actions(&context)?;
        let decode_ms = decode_started_at.elapsed();
        let runtime_action_count = decoded_runtime_actions.len();

        if self.controlled_capture_enabled {
            super::durable_transaction::ensure_dir_all_durable(&self.records_dir)?;
        }
        fs::create_dir_all(self.records_dir.as_path()).map_err(|err| {
            format!(
                "create execution records dir {} failed: {}",
                self.records_dir.display(),
                err
            )
        })?;

        // Capture only the explicit supported operation. Legacy/generic actions
        // retain their current record encoding and cannot enter captured history.
        let controlled_preparation = if self.controlled_capture_enabled
            && super::controlled_capture::supported_origin(&context).is_ok()
        {
            if resume_after_product_validation_intent {
                let reference = self
                    .pending_product_validation_intent
                    .as_ref()
                    .and_then(|marker| marker.controlled_preparation_ref.as_deref())
                    .ok_or("supported capture recovery requires original pre-step material")?;
                Some(super::controlled_capture::load_preparation(
                    &self.execution_store,
                    reference,
                    &context,
                )?)
            } else {
                Some(super::controlled_capture::prepare(
                    &self.execution_world,
                    &context,
                )?)
            }
        } else {
            None
        };
        if controlled_preparation.as_ref().is_some_and(|preparation| {
            preparation.release_security_policy != *self.execution_world.release_security_policy()
        }) {
            return Err(
                "capture recovery security policy differs from original preparation".into(),
            );
        }
        let controlled_preparation_ref = controlled_preparation
            .as_ref()
            .map(|preparation| {
                super::controlled_capture::persist_preparation(&self.execution_store, preparation)
            })
            .transpose()?;

        let previous_execution_world = self.execution_world.clone();
        let previous_simulator_mirror = self.simulator_mirror.clone();
        let previous_state = self.state.clone();
        macro_rules! rollback_on_error {
            ($result:expr) => {
                match $result {
                    Ok(value) => value,
                    Err(err) => {
                        self.execution_world = previous_execution_world;
                        self.simulator_mirror = previous_simulator_mirror;
                        self.state = previous_state;
                        return Err(err);
                    }
                }
            };
        }
        let committed_tick_context = RuntimeCommittedTickContext {
            height: context.height,
            slot: context.slot,
            epoch: context.epoch,
            node_block_hash: if self.formal_retention_enabled {
                context.node_block_hash.clone()
            } else {
                String::new()
            },
            action_root: context.action_root.clone(),
            authority_node_id: context.node_id.clone(),
            committed_at_unix_ms: context.committed_at_unix_ms,
        };
        rollback_on_error!(self.begin_formal_tick(
            &committed_tick_context,
            resume_after_product_validation_intent
        ));
        if let Some(authorities) = replicated_provider_backed_bootstrap.as_deref() {
            rollback_on_error!(self.apply_provider_backed_bootstrap(authorities));
        }
        // Newly registered administrative inputs advance commit history without
        // advancing simulation time. Legacy and gameplay blocks keep their
        // existing tick behavior; this preserves provider observation fences.
        let service_admin_only = !service_intents.is_empty()
            && decoded_runtime_actions.is_empty()
            && decoded_simulator_actions.is_empty()
            && service_intents.iter().all(|(_, request)| {
                !matches!(
                    request.signed_payload,
                    oasis7::world_service::WorldServicePayloadV1::GameplayJson(_)
                )
            });
        let service_identity = if service_intents.is_empty() {
            None
        } else {
            Some(rollback_on_error!(super::world_service_read::identity(
                &self.world_dir,
                &context.world_id
            )))
        };
        let service_results = rollback_on_error!(super::world_service_execution::apply_intents(
            &mut self.execution_world,
            &context,
            service_identity.as_ref(),
            service_intents
        ));
        let runtime_step_started_at = Instant::now();
        if !resume_after_product_validation_intent {
            for (action, origin) in decoded_runtime_actions {
                if let Some(origin) = origin {
                    rollback_on_error!(
                        self.execution_world
                            .submit_recipe_action_with_origin(action, origin)
                            .map_err(|err| format!(
                                "submit committed recipe origin failed: {err:?}"
                            ))
                    );
                } else {
                    self.execution_world.submit_action(action);
                }
            }
        }
        if resume_after_product_validation_intent {
            rollback_on_error!(
                self.execution_world
                    .step_with_modules_for_committed_context_after_product_validation_intent(
                        &mut *self.execution_sandbox,
                        &committed_tick_context,
                    )
                    .map_err(|err| {
                        format!(
                            "execution driver world.resume failed at height {}: {:?}",
                            context.height, err
                        )
                    })
            );
        } else if !service_admin_only {
            let intent_world_dir = self.world_dir.clone();
            let intent_records_dir = self.records_dir.clone();
            let intent_height = context.height;
            let intent_pre_step_execution_state_root = pre_step_execution_state_root
                .clone()
                .ok_or_else(|| "missing pre-step execution state root for intent".to_string())?;
            let intent_pre_step_external_effect = external_effect.clone();
            let intent_controlled_preparation_ref = controlled_preparation_ref.clone();
            let intent_commit_context = context.clone();
            let mut publish_product_validation_intent = move |staged: &RuntimeWorld| {
                // Publish the complete intent before the staged world. A
                // crash after this write is recognizable as an exact
                // predecessor marker; a crash after the world write leaves
                // both artifacts available for continuation.
                persist_product_validation_intent_for_staged_world(
                    intent_records_dir.as_path(),
                    staged,
                    &intent_commit_context,
                    intent_pre_step_execution_state_root.as_str(),
                    intent_pre_step_external_effect.clone(),
                    intent_controlled_preparation_ref.clone(),
                )
                .map_err(|err| WorldError::DistributedValidationFailed {
                    reason: format!(
                        "publish product validation intent marker failed at height {}: {}",
                        intent_height, err
                    ),
                })?;
                persist_execution_world(intent_world_dir.as_path(), staged).map_err(|err| {
                    WorldError::DistributedValidationFailed {
                        reason: format!(
                            "publish product validation intent world failed at height {}: {}",
                            intent_height, err
                        ),
                    }
                })
            };
            rollback_on_error!(
                self.execution_world
                    .step_with_modules_for_committed_context_with_product_validation_checkpoint(
                        &mut *self.execution_sandbox,
                        &committed_tick_context,
                        &mut publish_product_validation_intent,
                    )
                    .map_err(|err| {
                        format!(
                            "execution driver world.step failed at height {}: {:?}",
                            context.height, err
                        )
                    })
            );
        }
        // Publish the ordinary cache only after peer-hash/CAS acceptance;
        // otherwise rejects persist mutation and startup can prefer an
        // uncommitted cache. Product-validation intents use the explicit
        // staged-world callback above.
        let runtime_step_ms = runtime_step_started_at.elapsed();
        rollback_on_error!(super::world_service_execution::finalize_intents(
            &mut self.execution_world,
            service_results
        ));
        rollback_on_error!(self.finish_formal_tick(&committed_tick_context, service_admin_only));
        if self.formal_retention_enabled {
            rollback_on_error!(self.capture_formal_retention(&previous_execution_world));
        }
        let simulator_step_started_at = Instant::now();
        let (simulator_mirror, simulator_observation) = rollback_on_error!(
            self.apply_simulator_actions(&context, decoded_simulator_actions.as_slice())
        );
        let simulator_step_ms = simulator_step_started_at.elapsed();

        let runtime_resource_commit_hash =
            execution_resource_commit_hash(&context.world_id, context.height);
        let runtime_resource_context = ChainResourceDerivationContext {
            world_id: context.world_id.as_str(),
            chain_id: self
                .resource_identity
                .as_ref()
                .map_or(context.world_id.as_str(), |t| t.chain_id.as_str()),
            genesis_ref: self
                .resource_identity
                .as_ref()
                .map(|t| t.genesis_ref.as_str()),
            created_at_height: execution_resource_created_at_height(context.height),
            manifest_height: context.height,
            commit_block_hash: Some(runtime_resource_commit_hash.as_str()),
            tick: self.execution_world.state().time,
        };
        let runtime_resource_context_hash = execution_resource_context_hash(&context.world_id);
        let snapshot_value = self.execution_world.snapshot_with_chain_resource_context(
            runtime_resource_context,
            runtime_resource_context_hash.clone(),
            runtime_resource_context_hash.clone(),
        );
        let journal_value = self.execution_world.journal().clone();
        let serialize_started_at = Instant::now();
        let snapshot_bytes = rollback_on_error!(super::to_cbor(snapshot_value));
        let journal_bytes = rollback_on_error!(super::to_cbor(journal_value));
        let serialize_ms = serialize_started_at.elapsed();
        let snapshot_bytes_len = snapshot_bytes.len();
        let journal_bytes_len = journal_bytes.len();

        let cas_put_started_at = Instant::now();
        let snapshot_ref = rollback_on_error!(
            self.execution_store
                .put_bytes(snapshot_bytes.as_slice())
                .map_err(|err| format!("execution driver CAS snapshot put failed: {:?}", err))
        );
        let journal_ref = rollback_on_error!(
            self.execution_store
                .put_bytes(journal_bytes.as_slice())
                .map_err(|err| format!("execution driver CAS journal put failed: {:?}", err))
        );
        let mut cas_put_ms = cas_put_started_at.elapsed();

        let execution_state_root = blake3_hex(snapshot_bytes.as_slice());
        let prev_execution_block_hash = self
            .state
            .last_execution_block_hash
            .clone()
            .unwrap_or_else(|| "genesis".to_string());
        let hash_payload = ExecutionHashPayload {
            world_id: context.world_id.as_str(),
            height: context.height,
            prev_execution_block_hash: prev_execution_block_hash.as_str(),
            execution_state_root: execution_state_root.as_str(),
            journal_len: self.execution_world.journal().len(),
        };
        let execution_block_hash =
            blake3_hex(rollback_on_error!(super::to_cbor(hash_payload)).as_slice());
        if let (Some(expected_block_hash), Some(expected_state_root)) =
            (expected_execution_block_hash, expected_execution_state_root)
            && (execution_block_hash != expected_block_hash
                || execution_state_root != expected_state_root)
        {
            self.execution_world = previous_execution_world;
            self.simulator_mirror = previous_simulator_mirror;
            self.state = previous_state;
            return Err(format!(
                "execution driver peer mismatch at height {}: local_block={} peer_block={} local_state={} peer_state={}",
                context.height,
                execution_block_hash,
                expected_block_hash,
                execution_state_root,
                expected_state_root
            ));
        }
        let mut simulator_persist_ms = Duration::default();
        if simulator_mirror.is_some() {
            let resource_context = ChainResourceDerivationContext {
                world_id: context.world_id.as_str(),
                chain_id: context.world_id.as_str(),
                genesis_ref: None,
                created_at_height: 0,
                manifest_height: context.height,
                commit_block_hash: None,
                tick: self.simulator_mirror.time(),
            };
            let simulator_persist_started_at = Instant::now();
            rollback_on_error!(persist_simulator_execution_world(
                self.simulator_world_dir.as_path(),
                &self.simulator_mirror,
                Some(resource_context),
            ));
            simulator_persist_ms = simulator_persist_started_at.elapsed();
        }
        let external_effect_started_at = Instant::now();
        let external_effect_ref =
            rollback_on_error!(persist_execution_external_effect_materialization(
                &self.execution_store,
                &external_effect,
            ));
        cas_put_ms += external_effect_started_at.elapsed();
        let prev_node_block_hash = self.state.last_node_block_hash.clone();
        let node_block_hash = Some(context.node_block_hash.clone());

        let mut record = ExecutionBridgeRecord::new_v3(
            context.world_id.clone(),
            context.height,
            node_block_hash.clone(),
            prev_node_block_hash,
            context.proposer_id.clone(),
            context.action_root.clone(),
            execution_block_hash.clone(),
            execution_state_root.clone(),
            self.execution_world.journal().len(),
            snapshot_ref,
            journal_ref,
            Some(external_effect_ref),
            simulator_mirror,
            context.committed_at_unix_ms,
        );
        if let Some(preparation) = controlled_preparation.as_ref() {
            // The closed package is self-contained, and the legacy recovery
            // reader still dereferences these real CAS objects separately.
            for bytes in [
                &snapshot_bytes,
                &journal_bytes,
                &rollback_on_error!(super::to_cbor(&external_effect)),
            ] {
                rollback_on_error!(super::controlled_capture::persist_capture_bytes(
                    &self.execution_store,
                    bytes
                ));
            }

            let preceding = rollback_on_error!((|| -> Result<_, String> {
                Ok(if self.state.last_applied_committed_height > 0 {
                    let previous_record = load_execution_bridge_record(
                        execution_bridge_record_path(
                            self.records_dir.as_path(),
                            self.state.last_applied_committed_height,
                        )
                        .as_path(),
                    )?;
                    previous_record.controlled_capture_ref.as_ref().map(|reference| {
                    let package = super::controlled_capture::load_package(&self.execution_store, reference)?;
                    let manifest = super::controlled_capture::decode_role::<super::controlled_capture::CaptureManifest>(&package, oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest)?;
                    Ok::<_, String>((package, manifest))
                }).transpose()?
                } else {
                    None
                })
            })());
            let package = rollback_on_error!(super::controlled_capture::build(
                preparation,
                &record,
                &snapshot_bytes,
                &journal_bytes,
                &external_effect,
                preceding.as_ref().map(|(p, m)| (p, m))
            ));
            record.controlled_capture_ref = Some(rollback_on_error!(
                super::controlled_capture::persist_capture_bytes(
                    &self.execution_store,
                    &rollback_on_error!(super::to_cbor(package))
                )
            ));
        }
        let checkpoint_started_at = Instant::now();
        record.checkpoint_ref = rollback_on_error!(self.persist_local_checkpoint(&record));
        let checkpoint_ms = checkpoint_started_at.elapsed();
        let checkpoint_manifest = rollback_on_error!(
            record
                .checkpoint_ref
                .as_deref()
                .map(|checkpoint_ref| {
                    load_execution_checkpoint_manifest(
                        execution_checkpoint_root_dir(self.records_dir.as_path())
                            .join(checkpoint_ref)
                            .as_path(),
                    )
                })
                .transpose()
        );
        let world_head_proof_started_at = Instant::now();
        rollback_on_error!(persist_world_head_proof_for_record(
            &self.execution_store,
            &mut record,
            checkpoint_manifest.as_ref(),
        ));
        let world_head_proof_ms = world_head_proof_started_at.elapsed();
        let record_persist_started_at = Instant::now();
        rollback_on_error!(begin_execution_bridge_retention_transaction(
            self.records_dir.as_path()
        ));
        rollback_on_error!(persist_execution_bridge_record(
            self.records_dir.as_path(),
            &record
        ));
        let record_persist_ms = record_persist_started_at.elapsed();

        self.state.last_applied_committed_height = context.height;
        self.state.last_execution_block_hash = Some(execution_block_hash);
        self.state.last_execution_state_root = Some(execution_state_root);
        self.state.last_node_block_hash = node_block_hash;

        rollback_on_error!(persist_execution_bridge_state(
            self.state_path.as_path(),
            &self.state
        ));
        let persist_world_started_at = Instant::now();
        if self.local_execution_bootstrap.is_some() {
            // A local bootstrap owns an explicit persisted world boundary.
            // Publish its accepted successor only after the authoritative
            // record and bridge state are durable, while ordinary commits
            // keep the materialized cache unchanged and recover from CAS.
            if let Err(err) =
                persist_execution_world(self.world_dir.as_path(), &self.execution_world)
            {
                // The record and bridge state are authoritative. A cache
                // publish failure remains recoverable through startup head
                // restoration, so do not turn an accepted commit into a
                // retry that could duplicate its durable record.
                oasis7::observability::emit_stderr_or_event(
                    tracing::Level::WARN,
                    format!(
                        "execution driver local bootstrap cache publish failed at height {}: {}",
                        context.height, err
                    )
                    .as_str(),
                    "local bootstrap cache publish deferred to recovery",
                );
            }
        }
        let persist_world_ms = persist_world_started_at.elapsed();
        if self.pending_product_validation_intent.is_some()
            || load_product_validation_intent(self.records_dir.as_path())?.is_some()
        {
            if let Err(err) = clear_product_validation_intent(self.records_dir.as_path()) {
                // The authoritative record and bridge state are already
                // durable. A stale marker is harmless: startup removes it
                // once it observes that its height is settled.
                oasis7::observability::emit_stderr_or_event(
                    tracing::Level::WARN,
                    format!(
                        "execution driver could not clear settled product validation intent at height {}: {}",
                        context.height, err
                    )
                    .as_str(),
                    "settled product validation intent cleanup deferred",
                );
            }
            self.pending_product_validation_intent = None;
        }
        let retention_ms = self.maintain_committed_retention(context.height, &record);
        super::record_execution_bridge_module_tick_routing_metrics(
            self.execution_world.module_tick_routing_metrics_snapshot(),
        );
        emit_commit_observation(CommitObservation {
            world_id: context.world_id.as_str(),
            height: context.height,
            action_count,
            runtime_action_count,
            simulator: simulator_observation,
            snapshot_bytes: snapshot_bytes_len,
            journal_bytes: journal_bytes_len,
            decode_ms,
            runtime_step_ms,
            simulator_step_ms,
            serialize_ms,
            cas_put_ms,
            simulator_persist_ms,
            world_head_proof_ms,
            record_persist_ms,
            persist_world_ms,
            checkpoint_ms,
            retention_ms,
            total_ms: commit_started_at.elapsed(),
        });

        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: self
                .state
                .last_execution_block_hash
                .clone()
                .ok_or_else(|| "execution driver missing execution_block_hash".to_string())?,
            execution_state_root: self
                .state
                .last_execution_state_root
                .clone()
                .ok_or_else(|| "execution driver missing execution_state_root".to_string())?,
        })
    }

    fn restore_to_height(&mut self, world_id: &str, height: u64) -> Result<bool, String> {
        self.restore_execution_head_from_record(world_id, height)
    }

    fn export_checkpoint_bundle(
        &mut self,
        height: u64,
    ) -> Result<Option<NodeExecutionCheckpointBundle>, String> {
        let record_path = execution_bridge_record_path(self.records_dir.as_path(), height);
        if !record_path.exists() {
            return Ok(None);
        }
        let record = load_execution_bridge_record(record_path.as_path())?;
        let Some(checkpoint_ref) = record.checkpoint_ref.as_deref() else {
            return Ok(None);
        };
        let manifest_path =
            execution_checkpoint_root_dir(self.records_dir.as_path()).join(checkpoint_ref);
        let manifest_json = fs::read(manifest_path.as_path()).map_err(|err| {
            format!(
                "read execution checkpoint manifest {} failed: {}",
                manifest_path.display(),
                err
            )
        })?;
        let manifest = load_execution_checkpoint_manifest(manifest_path.as_path())?;
        if manifest.height != height
            || manifest.execution_block_hash != record.execution_block_hash
            || manifest.execution_state_root != record.execution_state_root
        {
            return Err(format!(
                "execution checkpoint manifest mismatch at height {}",
                height
            ));
        }

        let mut blobs = Vec::with_capacity(manifest.pinned_refs.len());
        for content_hash in &manifest.pinned_refs {
            let bytes = self
                .execution_store
                .get_verified(content_hash.as_str())
                .map_err(|err| {
                    format!(
                        "read execution checkpoint blob {} failed at height {}: {:?}",
                        content_hash, height, err
                    )
                })?;
            blobs.push(NodeExecutionCheckpointBlob {
                content_hash: content_hash.clone(),
                bytes,
            });
        }

        Ok(Some(NodeExecutionCheckpointBundle {
            height,
            execution_block_hash: manifest.execution_block_hash,
            execution_state_root: manifest.execution_state_root,
            manifest_json,
            blobs,
        }))
    }

    fn install_checkpoint_bundle(
        &mut self,
        context: NodeExecutionCheckpointInstallContext,
        bundle: NodeExecutionCheckpointBundle,
    ) -> Result<NodeExecutionCommitResult, String> {
        super::driver_checkpoint_install::install_checkpoint_bundle(self, context, bundle)
    }
}

#[path = "driver_retention.rs"]
mod retention;
