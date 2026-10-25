//! Fixed-set BFT execution. This path is isolated from legacy Pos commits and
//! keeps its durable QC/snapshot evidence in a dedicated sidecar.
use super::NodeRuntimeExecutionDriver;
use std::path::Path;

use oasis7_node::{
    BftBlockV1, BftCommitCertificateV1, BftValidatorSetV1, NodeBftBuildContextV1,
    NodeBftExecutionCommitResultV1, NodeBftExecutionConfigV1, NodeBftExecutionContextV1,
    NodeBftExecutionHeadV1, NodeConsensusAction, NodeExecutionCommitContext,
    VerifiedBftCommitCertificateV1, compute_consensus_action_root,
};
use oasis7_proto::distributed_bft::{
    BFT_MAX_ACTIONS_CBOR_BYTES_V1, BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1,
    BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1, BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1,
};
use oasis7_runtime::{
    BlobStore, ChainResourceDerivationContext, LocalCasStore, RuntimeCommittedTickContext,
    World as RuntimeWorld, blake3_hex,
};
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};
use serde::{Deserialize, Serialize};

#[path = "driver_bft_storage.rs"]
mod storage;

const BFT_EXECUTION_TICK_AUTHORITY: &str = "system:bft-execution-v1";
const BFT_EXECUTION_HASH_DOMAIN_V1: &str = "oasis7.bft.execution.v1";

pub(crate) struct BftExecutionState {
    pub(crate) config: NodeBftExecutionConfigV1,
    validator_set: BftValidatorSetV1,
    ledger: storage::BftExecutionLedger,
    cursor_height: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ExecutionHashPayloadV1<'a> {
    domain: &'static str,
    scope: &'a oasis7_node::BftScopeV1,
    height: u64,
    parent_commit_ref: &'a str,
    authority_epoch: u64,
    manifest_hash: &'a str,
    action_root: &'a str,
    event_root: &'a str,
    execution_state_root: &'a str,
    state_root: &'a str,
}

pub(super) struct ExecutedCandidate {
    pub(super) world: RuntimeWorld,
    pub(super) snapshot_bytes: Vec<u8>,
    pub(super) journal_bytes: Vec<u8>,
    pub(super) event_root: String,
    pub(super) execution_block_hash: String,
    pub(super) execution_state_root: String,
    pub(super) state_root: String,
}

impl NodeRuntimeExecutionDriver {
    pub(super) fn configure_bft(
        &mut self,
        config: &NodeBftExecutionConfigV1,
    ) -> Result<Option<NodeBftExecutionHeadV1>, String> {
        let validator_set = config.validate()?;
        if self.bft_execution.is_some() {
            let (head, genesis_world) = {
                let state = self
                    .bft_execution
                    .as_mut()
                    .expect("BFT state presence checked above");
                if state.config != *config || state.validator_set != validator_set {
                    return Err("BFT execution restart changed its pinned configuration".into());
                }
                let genesis_world = state.ledger.restore_genesis_world(
                    &self.execution_store,
                    self.execution_world.release_security_policy().clone(),
                )?;
                let head = state.ledger.head();
                state.cursor_height = 0;
                (head, genesis_world)
            };
            // NodeRuntime calls configure before replaying the complete raw QC
            // history on every stop/start. Keep the already-open ledger and
            // writer lock; only reset the materialized replay cursor to its
            // authenticated genesis snapshot.
            self.execution_world = genesis_world;
            self.state = super::ExecutionBridgeState::default();
            self.simulator_mirror = oasis7::simulator::WorldKernel::new();
            self.bft_mode_required = true;
            return Ok(head);
        }
        validate_driver_is_bft_eligible(self)?;
        if let Some(identity) = &self.resource_identity
            && (identity.chain_id != config.scope.chain_id
                || identity.genesis_ref != config.scope.genesis_hash)
        {
            return Err("BFT scope differs from the pinned execution resource identity".into());
        }
        let allow_initialize = !self.bft_mode_required;
        let (ledger, genesis_world) = storage::BftExecutionLedger::open_or_initialize(
            self.records_dir.as_path(),
            config,
            &self.execution_world,
            &self.execution_store,
            allow_initialize,
        )?;
        validate_world_scope(&genesis_world, config)?;
        self.execution_world = genesis_world;
        self.state = super::ExecutionBridgeState::default();
        self.simulator_mirror = oasis7::simulator::WorldKernel::new();
        let head = ledger.head();
        self.bft_mode_required = true;
        self.bft_execution = Some(BftExecutionState {
            config: config.clone(),
            validator_set,
            ledger,
            cursor_height: 0,
        });
        Ok(head)
    }

    pub(super) fn has_bft_material(records_dir: &Path) -> Result<bool, String> {
        storage::has_bft_material(records_dir)
    }

    pub(super) fn bft_history_page(
        &self,
        config: &NodeBftExecutionConfigV1,
        after_height: u64,
        max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV1>, String> {
        let state = self
            .bft_execution
            .as_ref()
            .ok_or("BFT execution is not configured")?;
        if config != &state.config {
            return Err("BFT history query config differs from the pinned config".into());
        }
        if max_entries == 0 || max_entries > BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1 {
            return Err("BFT history page request exceeds the page bound".into());
        }
        state.ledger.history_page(after_height, max_entries)
    }

    pub(super) fn current_bft_context(
        &self,
        scope: &oasis7_node::BftScopeV1,
        height: u64,
        parent_commit_ref: &str,
        authority_epoch: u64,
    ) -> Result<NodeBftExecutionContextV1, String> {
        let state = self
            .bft_execution
            .as_ref()
            .ok_or("BFT execution is not configured")?;
        let expected_parent = state.ledger.expected_parent(height)?;
        if scope != &state.config.scope
            || authority_epoch != state.config.authority_epoch
            || parent_commit_ref != expected_parent
        {
            return Err(
                "requested BFT context differs from the pinned config or exact ledger parent"
                    .into(),
            );
        }
        let expected = NodeBftExecutionContextV1 {
            scope: scope.clone(),
            height,
            parent_commit_ref: parent_commit_ref.to_string(),
            authority_epoch,
            manifest_hash: self
                .execution_world
                .current_manifest_hash()
                .map_err(|error| format!("read current BFT manifest hash: {error:?}"))?,
        };
        validate_expected_context(
            &self.execution_world,
            &state.config,
            &expected,
            expected_parent.as_str(),
        )?;
        Ok(expected)
    }

    pub(super) fn validate_bft_action_payload(&self, payload: &[u8]) -> Result<(), String> {
        if payload.is_empty() || payload.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V1 {
            return Err("BFT action payload is empty or exceeds the protocol bound".into());
        }
        let action =
            NodeConsensusAction::from_payload(1, "system:bft-action-preflight", payload.to_vec())
                .map_err(|error| format!("build BFT action admission probe: {error}"))?;
        let context = NodeExecutionCommitContext {
            world_id: "bft-action-preflight".into(),
            node_id: "bft-action-preflight".into(),
            proposer_id: "bft-action-preflight".into(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "00".repeat(32),
            action_root: "00".repeat(32),
            committed_actions: vec![action],
            committed_at_unix_ms: 0,
        };
        if decode_closed_runtime_actions(&context)?.len() != 1 {
            return Err("BFT action admission requires exactly one closed RuntimeAction".into());
        }
        Ok(())
    }

    pub(super) fn build_bft(&self, context: NodeBftBuildContextV1) -> Result<BftBlockV1, String> {
        let state = self
            .bft_execution
            .as_ref()
            .ok_or("BFT execution is not configured")?;
        let expected_parent = state.ledger.expected_parent(context.execution.height)?;
        validate_expected_context(
            &self.execution_world,
            &state.config,
            &context.execution,
            expected_parent.as_str(),
        )?;
        let actions_cbor = serde_cbor::to_vec(&context.actions)
            .map_err(|error| format!("encode BFT actions: {error}"))?;
        if actions_cbor.is_empty() || actions_cbor.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V1 {
            return Err("BFT actions exceed the bounded execution input size".into());
        }
        let mut block = BftBlockV1 {
            scope: context.execution.scope.clone(),
            height: context.execution.height,
            parent_commit_ref: context.execution.parent_commit_ref.clone(),
            authority_epoch: context.execution.authority_epoch,
            manifest_hash: context.execution.manifest_hash.clone(),
            actions_cbor,
            action_root: String::new(),
            event_root: String::new(),
            execution_block_hash: String::new(),
            execution_state_root: String::new(),
            state_root: String::new(),
        };
        block.action_root = action_root(context.actions.as_slice())?;
        let executed = execute_candidate(
            &self.execution_world,
            &state.config,
            &block,
            expected_parent.as_str(),
        )?;
        block.event_root = executed.event_root;
        block.execution_block_hash = executed.execution_block_hash;
        block.execution_state_root = executed.execution_state_root;
        block.state_root = executed.state_root;
        block.validate()?;
        Ok(block)
    }

    pub(super) fn preview_bft(
        &self,
        expected: &NodeBftExecutionContextV1,
        candidate: &BftBlockV1,
    ) -> Result<(), String> {
        let state = self
            .bft_execution
            .as_ref()
            .ok_or("BFT execution is not configured")?;
        let expected_parent = state.ledger.expected_parent(expected.height)?;
        validate_expected_context(
            &self.execution_world,
            &state.config,
            expected,
            expected_parent.as_str(),
        )?;
        validate_candidate_binding(&state.config, expected, candidate)?;
        let executed = execute_candidate(
            &self.execution_world,
            &state.config,
            candidate,
            expected_parent.as_str(),
        )?;
        compare_executed_candidate(candidate, &executed)
    }

    pub(super) fn commit_bft(
        &mut self,
        token: VerifiedBftCommitCertificateV1,
    ) -> Result<NodeBftExecutionCommitResultV1, String> {
        let (config, validator_set, cursor_height, expected_parent, existing) = {
            let state = self
                .bft_execution
                .as_ref()
                .ok_or("BFT execution is not configured")?;
            let block = token.block();
            (
                state.config.clone(),
                state.validator_set.clone(),
                state.cursor_height,
                state.ledger.expected_parent(block.height)?,
                state.ledger.record(block.height)?,
            )
        };
        let block = token.block();
        if token.validator_set() != &validator_set
            || token.schedule().scope() != &config.scope
            || token.schedule().authority_epoch() != config.authority_epoch
            || token.schedule().validator_set_hash() != config.validator_set_hash
        {
            return Err("verified BFT certificate is outside the pinned execution config".into());
        }
        if token.schedule().height() != block.height
            || token.schedule().parent_commit_ref() != block.parent_commit_ref
            || block.parent_commit_ref != expected_parent
        {
            return Err("BFT certificate does not extend the exact durable predecessor".into());
        }
        validate_candidate_binding(
            &config,
            &NodeBftExecutionContextV1 {
                scope: block.scope.clone(),
                height: block.height,
                parent_commit_ref: block.parent_commit_ref.clone(),
                authority_epoch: block.authority_epoch,
                manifest_hash: block.manifest_hash.clone(),
            },
            block,
        )?;

        if let Some(record) = existing.as_ref() {
            if &record.certificate != token.certificate() || record.block_id != token.block_id() {
                return Err(
                    "durable BFT height already contains different certificate evidence".into(),
                );
            }
            if cursor_height > block.height {
                return Ok(commit_result(block, token.block_id()));
            }
            if cursor_height == block.height {
                validate_live_world_matches_record(
                    &self.execution_world,
                    record,
                    &self.execution_store,
                )?;
                return Ok(commit_result(block, token.block_id()));
            }
            if cursor_height.saturating_add(1) != block.height {
                return Err("BFT certificate replay skips the current execution cursor".into());
            }
        } else if cursor_height.saturating_add(1) != block.height
            || self
                .bft_execution
                .as_ref()
                .expect("configured above")
                .ledger
                .tip_height()
                != cursor_height
        {
            return Err(
                "new BFT certificate is not the exact successor of the durable cursor".into(),
            );
        }

        validate_expected_context(
            &self.execution_world,
            &config,
            &NodeBftExecutionContextV1 {
                scope: block.scope.clone(),
                height: block.height,
                parent_commit_ref: block.parent_commit_ref.clone(),
                authority_epoch: block.authority_epoch,
                manifest_hash: block.manifest_hash.clone(),
            },
            expected_parent.as_str(),
        )?;
        let executed = execute_candidate(
            &self.execution_world,
            &config,
            block,
            expected_parent.as_str(),
        )?;
        compare_executed_candidate(block, &executed)?;

        {
            let state = self
                .bft_execution
                .as_mut()
                .expect("BFT state remains configured");
            if let Some(record) = existing.as_ref() {
                state
                    .ledger
                    .validate_record_material(record, &executed, &self.execution_store)?;
                state.ledger.accept_existing_record(record)?;
            } else {
                state.ledger.commit_candidate(
                    token.certificate().clone(),
                    token.block_id().to_string(),
                    block,
                    &executed,
                    &self.execution_store,
                )?;
            }
        }
        self.install_bft_world(block.height, executed.world)?;
        self.bft_execution
            .as_mut()
            .expect("BFT state remains configured")
            .cursor_height = block.height;
        Ok(commit_result(block, token.block_id()))
    }

    fn install_bft_world(&mut self, height: u64, world: RuntimeWorld) -> Result<(), String> {
        // The BFT height record is already the durable commit point. These
        // ordinary World files are a recoverable materialized view; a failure
        // leaves the old in-memory parent and exact retry installs the record.
        super::persist_execution_world(self.world_dir.as_path(), &world)?;
        self.execution_world = world;
        self.state.last_applied_committed_height = height;
        Ok(())
    }
}

pub(super) fn validate_driver_is_bft_eligible(
    driver: &NodeRuntimeExecutionDriver,
) -> Result<(), String> {
    if !cfg!(unix) {
        return Err("durable BFT execution is unsupported on this platform".into());
    }
    if !cfg!(feature = "wasmtime") {
        return Err("BFT execution requires the deterministic Wasmtime executor".into());
    }
    if !driver
        .execution_world
        .release_security_policy()
        .is_production_hardened()
    {
        return Err("BFT execution requires a production-hardened release policy".into());
    }
    if driver.controlled_capture_enabled
        || driver.formal_retention_enabled
        || driver.formal_retention_delta.is_some()
        || !driver.formal_new_results.is_empty()
        || driver.formal_event_root.is_some()
        || driver.formal_unpruned_journal.is_some()
        || !driver.formal_journal_cursors.is_empty()
        || driver.devlocal_participant.authority.is_some()
        || driver.devlocal_participant.startup.is_some()
        || driver.local_execution_bootstrap.is_some()
    {
        return Err(
            "BFT execution cannot share controlled, formal, or local-bootstrap state".into(),
        );
    }
    if driver.execution_world.state().time != 0
        || !driver.execution_world.journal().is_empty()
        || driver.execution_world.pending_actions_len() != 0
        || driver.execution_world.pending_effects_len() != 0
        || driver
            .execution_world
            .state()
            .retained_history_root
            .is_some()
    {
        return Err("BFT execution requires a closed, empty genesis execution world".into());
    }
    if driver.state.last_applied_committed_height != 0
        || !super::list_execution_bridge_record_heights(driver.records_dir.as_path())?.is_empty()
        || driver.records_dir.join("latest.json").exists()
    {
        return Err("BFT execution cannot adopt legacy execution proof or history".into());
    }
    if driver.pending_product_validation_intent.is_some()
        || driver
            .records_dir
            .join("product-validation-intent.json")
            .exists()
    {
        return Err("BFT execution cannot adopt a legacy pending execution intent".into());
    }
    Ok(())
}

pub(super) fn validate_world_scope(
    world: &RuntimeWorld,
    config: &NodeBftExecutionConfigV1,
) -> Result<(), String> {
    config.scope.validate()?;
    let manifest = world.chain_resource_manifest();
    if manifest.world_id != config.scope.world_id
        || manifest.chain_id != config.scope.chain_id
        || manifest.genesis_ref.as_deref() != Some(config.scope.genesis_hash.as_str())
    {
        return Err(
            "BFT scope differs from the existing authenticated World resource identity".into(),
        );
    }
    Ok(())
}

fn validate_expected_context(
    world: &RuntimeWorld,
    config: &NodeBftExecutionConfigV1,
    expected: &NodeBftExecutionContextV1,
    expected_parent: &str,
) -> Result<(), String> {
    if expected.scope != config.scope || expected.authority_epoch != config.authority_epoch {
        return Err("BFT execution context differs from the fixed scope or epoch".into());
    }
    let expected_height = world
        .state()
        .time
        .checked_add(1)
        .ok_or("BFT execution height overflow")?;
    if expected.height != expected_height {
        return Err(format!(
            "BFT execution height is not the exact next World tick: expected={expected_height} actual={}",
            expected.height
        ));
    }
    if expected.parent_commit_ref != expected_parent {
        return Err("BFT execution parent differs from the exact durable QC parent".into());
    }
    let manifest_hash = world
        .current_manifest_hash()
        .map_err(|error| format!("read current World manifest hash: {error:?}"))?;
    if expected.manifest_hash != manifest_hash {
        return Err("BFT execution manifest hash differs from the current World".into());
    }
    validate_world_scope(world, config)
}

fn validate_candidate_binding(
    config: &NodeBftExecutionConfigV1,
    expected: &NodeBftExecutionContextV1,
    candidate: &BftBlockV1,
) -> Result<(), String> {
    candidate.validate()?;
    if candidate.scope != config.scope
        || candidate.scope != expected.scope
        || candidate.height != expected.height
        || candidate.parent_commit_ref != expected.parent_commit_ref
        || candidate.authority_epoch != config.authority_epoch
        || candidate.authority_epoch != expected.authority_epoch
        || candidate.manifest_hash != expected.manifest_hash
    {
        return Err("BFT block differs from the exact local execution context".into());
    }
    Ok(())
}

fn action_root(actions: &[NodeConsensusAction]) -> Result<String, String> {
    for action in actions {
        action.validate().map_err(|error| error.to_string())?;
    }
    compute_consensus_action_root(actions).map_err(|error| format!("{error:?}"))
}

fn decode_actions(bytes: &[u8]) -> Result<Vec<NodeConsensusAction>, String> {
    if bytes.is_empty() || bytes.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V1 {
        return Err("BFT action bytes are empty or exceed the protocol bound".into());
    }
    let actions: Vec<NodeConsensusAction> = serde_cbor::from_slice(bytes)
        .map_err(|error| format!("decode BFT action bytes: {error}"))?;
    if serde_cbor::to_vec(&actions).map_err(|error| error.to_string())? != bytes {
        return Err("BFT action bytes are not canonical CBOR".into());
    }
    action_root(actions.as_slice())?;
    Ok(actions)
}

type ClosedRuntimeAction = (
    oasis7_runtime::Action,
    Option<oasis7_runtime::CommittedRecipeOrigin>,
    Option<oasis7_runtime::GovernanceAuthorizationV1>,
);

fn decode_closed_runtime_actions(
    context: &NodeExecutionCommitContext,
) -> Result<Vec<ClosedRuntimeAction>, String> {
    let (runtime_actions, simulator_actions, provider_bootstrap, service_intents) =
        super::super::driver_replicated_input::decode_committed_actions(context)?;
    if !simulator_actions.is_empty() || provider_bootstrap.is_some() || !service_intents.is_empty()
    {
        return Err("BFT execution only accepts closed RuntimeAction inputs".into());
    }
    Ok(runtime_actions)
}

fn execute_candidate(
    parent: &RuntimeWorld,
    config: &NodeBftExecutionConfigV1,
    block: &BftBlockV1,
    expected_parent: &str,
) -> Result<ExecutedCandidate, String> {
    block.scope.validate()?;
    if block.height == 0 || block.parent_commit_ref != expected_parent {
        return Err("BFT execution input has a noncanonical height or parent".into());
    }
    let actions = decode_actions(block.actions_cbor.as_slice())?;
    let computed_action_root = action_root(actions.as_slice())?;
    if !block.action_root.is_empty() && computed_action_root != block.action_root {
        return Err("BFT action root differs from canonical committed actions".into());
    }
    let parent_manifest_hash = parent
        .current_manifest_hash()
        .map_err(|error| format!("read parent World manifest hash: {error:?}"))?;
    let expected = NodeBftExecutionContextV1 {
        scope: block.scope.clone(),
        height: block.height,
        parent_commit_ref: block.parent_commit_ref.clone(),
        authority_epoch: block.authority_epoch,
        manifest_hash: block.manifest_hash.clone(),
    };
    validate_expected_context(parent, config, &expected, expected_parent)?;
    if parent_manifest_hash != block.manifest_hash {
        return Err("BFT block manifest hash is not the exact parent manifest".into());
    }
    if parent.pending_actions_len() != 0 || parent.pending_effects_len() != 0 {
        return Err("BFT execution parent has pending local inputs or effects".into());
    }

    let parent_snapshot_bytes = super::super::snapshot_to_cbor(&parent.snapshot())?;
    let parent_journal_bytes = super::super::journal_to_cbor(parent.journal())?;
    let mut next_world = RuntimeWorld::from_snapshot_with_release_security_policy(
        parent.snapshot(),
        parent.journal().clone(),
        parent.release_security_policy().clone(),
    )
    .map_err(|error| format!("restore detached BFT parent World: {error:?}"))?;
    if super::super::snapshot_to_cbor(&next_world.snapshot())? != parent_snapshot_bytes
        || super::super::journal_to_cbor(next_world.journal())? != parent_journal_bytes
    {
        return Err("detached BFT parent restore changed canonical snapshot or journal".into());
    }
    let deterministic_node_hash = execution_input_hash(block)?;
    let commit_context = NodeExecutionCommitContext {
        world_id: config.scope.world_id.clone(),
        node_id: BFT_EXECUTION_TICK_AUTHORITY.to_string(),
        proposer_id: BFT_EXECUTION_TICK_AUTHORITY.to_string(),
        height: block.height,
        slot: block.height.saturating_sub(1),
        epoch: block.authority_epoch,
        node_block_hash: deterministic_node_hash.clone(),
        action_root: block.action_root.clone(),
        committed_actions: actions.clone(),
        committed_at_unix_ms: 0,
    };
    let runtime_actions = decode_closed_runtime_actions(&commit_context)?;
    for (action, origin, authorization) in runtime_actions {
        if let Some(authorization) = authorization {
            next_world.submit_governance_action(action, authorization);
        } else if let Some(origin) = origin {
            next_world
                .submit_recipe_action_with_origin(action, origin)
                .map_err(|error| format!("submit BFT recipe action origin: {error:?}"))?;
        } else {
            next_world.submit_action(action);
        }
    }
    let event_start = next_world.journal().len();
    let tick_context = RuntimeCommittedTickContext {
        height: block.height,
        slot: block.height.saturating_sub(1),
        epoch: block.authority_epoch,
        node_block_hash: deterministic_node_hash,
        action_root: block.action_root.clone(),
        authority_node_id: BFT_EXECUTION_TICK_AUTHORITY.to_string(),
        committed_at_unix_ms: 0,
    };
    let mut executor = WasmExecutor::new_for_deterministic_replay(WasmExecutorConfig::default())
        .map_err(|error| format!("initialize isolated BFT executor: {error}"))?;
    next_world
        .step_with_modules_for_committed_context(&mut executor, &tick_context)
        .map_err(|error| format!("isolated BFT World execution failed: {error:?}"))?;
    if next_world.pending_actions_len() != 0 || next_world.pending_effects_len() != 0 {
        return Err("BFT execution produced pending local actions or external effects".into());
    }

    let manifest = parent.chain_resource_manifest();
    let resource_commit_hash = execution_input_hash(block)?;
    let resource_context = ChainResourceDerivationContext {
        world_id: config.scope.world_id.as_str(),
        chain_id: config.scope.chain_id.as_str(),
        genesis_ref: Some(config.scope.genesis_hash.as_str()),
        created_at_height: manifest.created_at_height,
        manifest_height: block.height,
        commit_block_hash: Some(resource_commit_hash.as_str()),
        tick: next_world.state().time,
    };
    let snapshot = next_world.snapshot_with_chain_resource_context(
        resource_context,
        manifest.world_config_hash.clone(),
        manifest.generation_algorithm_hash.clone(),
    );
    next_world = next_world
        .restore_candidate_snapshot_with_attached_formal_history(
            snapshot,
            next_world.journal().clone(),
        )
        .map_err(|error| format!("install derived BFT chain-resource context: {error:?}"))?;

    let new_events = next_world
        .journal()
        .events
        .get(event_start..)
        .ok_or("BFT execution journal moved backwards")?;
    let event_root = blake3_hex(
        &serde_cbor::to_vec(&new_events)
            .map_err(|error| format!("encode BFT event root: {error}"))?,
    );
    let snapshot_bytes = super::super::snapshot_to_cbor(&next_world.snapshot())?;
    let journal_bytes = super::super::journal_to_cbor(next_world.journal())?;
    let execution_state_root = blake3_hex(snapshot_bytes.as_slice());
    let state_root = next_world
        .execution_state_identity()
        .map_err(|error| format!("compute BFT runtime state root: {error:?}"))?
        .state_root_hash;
    let execution_block_hash = execution_block_hash(
        block,
        event_root.as_str(),
        execution_state_root.as_str(),
        state_root.as_str(),
    )?;
    Ok(ExecutedCandidate {
        world: next_world,
        snapshot_bytes,
        journal_bytes,
        event_root,
        execution_block_hash,
        execution_state_root,
        state_root,
    })
}

fn execution_input_hash(block: &BftBlockV1) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(&(
        BFT_EXECUTION_HASH_DOMAIN_V1,
        &block.scope,
        block.height,
        &block.parent_commit_ref,
        block.authority_epoch,
        &block.manifest_hash,
        &block.actions_cbor,
        &block.action_root,
    ))
    .map_err(|error| format!("encode BFT execution input: {error}"))?;
    Ok(blake3_hex(bytes.as_slice()))
}

fn execution_block_hash(
    block: &BftBlockV1,
    event_root: &str,
    execution_state_root: &str,
    state_root: &str,
) -> Result<String, String> {
    let payload = ExecutionHashPayloadV1 {
        domain: BFT_EXECUTION_HASH_DOMAIN_V1,
        scope: &block.scope,
        height: block.height,
        parent_commit_ref: &block.parent_commit_ref,
        authority_epoch: block.authority_epoch,
        manifest_hash: &block.manifest_hash,
        action_root: &block.action_root,
        event_root,
        execution_state_root,
        state_root,
    };
    let bytes = serde_cbor::to_vec(&payload)
        .map_err(|error| format!("encode BFT execution hash: {error}"))?;
    Ok(blake3_hex(bytes.as_slice()))
}

fn compare_executed_candidate(
    block: &BftBlockV1,
    executed: &ExecutedCandidate,
) -> Result<(), String> {
    let actual_execution_hash = execution_block_hash(
        block,
        executed.event_root.as_str(),
        executed.execution_state_root.as_str(),
        executed.state_root.as_str(),
    )?;
    if block.event_root != executed.event_root
        || block.execution_block_hash != actual_execution_hash
        || block.execution_state_root != executed.execution_state_root
        || block.state_root != executed.state_root
    {
        return Err("BFT candidate execution roots do not match isolated Runtime replay".into());
    }
    Ok(())
}

fn commit_result(block: &BftBlockV1, block_id: &str) -> NodeBftExecutionCommitResultV1 {
    NodeBftExecutionCommitResultV1 {
        height: block.height,
        bft_commit_ref: block_id.to_string(),
        execution_block_hash: block.execution_block_hash.clone(),
        execution_state_root: block.execution_state_root.clone(),
        state_root: block.state_root.clone(),
    }
}

fn validate_live_world_matches_record(
    world: &RuntimeWorld,
    record: &storage::BftExecutionRecordV1,
    store: &LocalCasStore,
) -> Result<(), String> {
    let snapshot = super::super::snapshot_to_cbor(&world.snapshot())?;
    let journal = super::super::journal_to_cbor(world.journal())?;
    if blake3_hex(snapshot.as_slice()) != record.execution_state_root
        || world
            .execution_state_identity()
            .map_err(|error| format!("read current BFT execution identity: {error:?}"))?
            .state_root_hash
            != record.state_root
        || storage::read_cas_bounded(
            store,
            record.snapshot_ref.as_str(),
            storage::MAX_BFT_SNAPSHOT_BYTES,
        )? != snapshot
        || storage::read_cas_bounded(
            store,
            record.journal_ref.as_str(),
            storage::MAX_BFT_JOURNAL_BYTES,
        )? != journal
    {
        return Err("live World differs from the exact committed BFT snapshot and journal".into());
    }
    Ok(())
}

pub(super) const MAX_BFT_HISTORY_BYTES: usize = BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1;
