//! Dynamic validator-set BFT execution, isolated from the V1 ledger/schema.
#[path = "driver_bft_schedule.rs"]
mod schedule;

use super::*;
use std::collections::BTreeMap;
use std::path::Path;

use super::ClosedRuntimeAction;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadBody, decode_consensus_action_payload_envelope,
};
use oasis7_consensus::tendermint_v2::{
    BftHeightProposerScheduleV2, VerifiedBftCommitCertificateV2,
};
use oasis7_node::{
    BftBlockV2, BftCommitCertificateV2, BftScopeV2, BftValidatorTransitionV2,
    NodeBftBuildContextV2, NodeBftExecutionCommitResultV2, NodeBftExecutionConfigV2,
    NodeBftExecutionContextV2, NodeBftExecutionHeadV2, NodeConsensusAction,
    NodeExecutionCommitContext,
};
use oasis7_proto::distributed_bft::v2::{
    BFT_MAX_ACTIONS_CBOR_BYTES_V2, BftValidatorTransitionRefV2,
};
use oasis7_runtime::{
    ChainResourceDerivationContext, LocalCasStore, RuntimeCommittedTickContext,
    World as RuntimeWorld, blake3_hex,
};
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};
use serde::Serialize;

#[path = "driver_bft_v2_execution.rs"]
mod execution;
#[path = "driver_bft_v2_formal.rs"]
mod formal_read;
#[path = "driver_bft_v2_governance.rs"]
mod governance;
#[path = "driver_bft_v2_storage.rs"]
mod storage;
pub(crate) use formal_read::{
    FormalBftHeaderExecutionV3, FormalBftHeaderLedgerExecutionV3, FormalBftHeaderNodeDriverV3,
    FormalBftPrefixExecutionV1, FormalBftReadExecutionV1, PRIVATE_HEADER_PREFIX_MAX_BYTES_V3,
    PrivateHeaderPrefixV3,
};
use governance::{
    has_validator_set_change_action_v2, transition_metadata_for_schedule,
    validate_set_change_actions_have_registry_v2, validate_validator_transition_action_schedule_v2,
};

const BFT_V2_EXECUTION_TICK_AUTHORITY: &str = "system:bft-execution-v2";
const BFT_V2_EXECUTION_HASH_DOMAIN: &str = "oasis7.bft.execution.v2";

pub(super) fn has_engineering_material(records_dir: &Path) -> Result<bool, String> {
    storage::BftV2ExecutionLedger::required_material_exists(records_dir)
}

pub(super) fn has_formal_bft_material(records_dir: &Path) -> Result<bool, String> {
    formal_read::FormalBftReadExecutionV1::has_required_material(records_dir)
}

pub(crate) struct BftExecutionStateV2 {
    pub(crate) config: NodeBftExecutionConfigV2,
    genesis_schedule: BftHeightProposerScheduleV2,
    cursor_schedule: BftHeightProposerScheduleV2,
    ledger: storage::BftV2ExecutionLedger,
    cursor_height: u64,
}

struct ExecutedCandidateV2 {
    world: RuntimeWorld,
    snapshot_bytes: Vec<u8>,
    journal_bytes: Vec<u8>,
    event_root: String,
    execution_block_hash: String,
    execution_state_root: String,
    state_root: String,
    post_manifest_hash: String,
    transition: Option<BftValidatorTransitionV2>,
    transition_ref: Option<BftValidatorTransitionRefV2>,
}

#[derive(Serialize)]
struct ExecutionHashPayloadV2<'a> {
    domain: &'static str,
    scope: &'a BftScopeV2,
    height: u64,
    parent_commit_ref: &'a str,
    authority_epoch: u64,
    active_validator_set_hash: &'a str,
    schedule_policy_hash: &'a str,
    manifest_hash: &'a str,
    action_root: &'a str,
    event_root: &'a str,
    execution_state_root: &'a str,
    state_root: &'a str,
    post_manifest_hash: &'a str,
    transition: &'a Option<BftValidatorTransitionV2>,
    transition_ref: &'a Option<BftValidatorTransitionRefV2>,
}

impl NodeRuntimeExecutionDriver {
    pub(crate) fn configure_bft_v2(
        &mut self,
        config: &NodeBftExecutionConfigV2,
    ) -> Result<Option<NodeBftExecutionHeadV2>, String> {
        if has_formal_bft_material(self.records_dir.as_path())? {
            return Err("BFT V2 execution refuses a formal BFT read ledger".into());
        }
        let genesis_schedule = config.validate()?;
        if self.bft_execution.is_some() {
            return Err("V1 and V2 BFT execution modes cannot share one driver".into());
        }
        if let Some(state) = self.bft_execution_v2.as_mut() {
            if state.config != *config {
                return Err("BFT V2 execution restart changed its pinned configuration".into());
            }
            let genesis_world = {
                let genesis_world = state.ledger.restore_genesis_world(
                    &self.execution_store,
                    self.execution_world.release_security_policy().clone(),
                )?;
                state.cursor_height = 0;
                state.cursor_schedule = state.genesis_schedule.clone();
                genesis_world
            };
            self.install_bft_v2_genesis_world(genesis_world);
            self.bft_mode_required = true;
            // The ledger's structural tip is not a verified consensus head.
            // Callers must replay raw QC history from the pinned genesis
            // schedule before exposing a head or starting a worker.
            return Ok(None);
        }
        if super::storage::BftExecutionLedger::required_material_exists(self.records_dir.as_path())?
        {
            return Err("V2 BFT execution refuses an existing V1 execution ledger".into());
        }
        validate_driver_is_bft_eligible(self)?;
        if !self.bft_mode_required {
            validate_world_scope_v2(&self.execution_world, config)?;
        }
        if let Some(identity) = &self.resource_identity
            && (identity.chain_id != config.scope.chain_id
                || identity.genesis_ref != config.scope.genesis_hash)
        {
            return Err("BFT V2 scope differs from pinned execution resource identity".into());
        }
        let allow_initialize = !self.bft_mode_required;
        let (ledger, genesis_world) = storage::BftV2ExecutionLedger::open_or_initialize(
            self.records_dir.as_path(),
            config,
            &genesis_schedule,
            &self.execution_world,
            &self.execution_store,
            allow_initialize,
        )?;
        self.install_bft_v2_genesis_world(genesis_world);
        self.bft_mode_required = true;
        self.bft_execution_v2 = Some(Box::new(BftExecutionStateV2 {
            config: config.clone(),
            genesis_schedule: genesis_schedule.clone(),
            cursor_schedule: genesis_schedule,
            ledger,
            cursor_height: 0,
        }));
        Ok(None)
    }

    fn install_bft_v2_genesis_world(&mut self, genesis_world: RuntimeWorld) {
        self.execution_world = genesis_world;
        self.state = super::super::ExecutionBridgeState::default();
        self.simulator_mirror = oasis7::simulator::WorldKernel::new();
    }

    pub(super) fn has_bft_v2_material(records_dir: &Path) -> Result<bool, String> {
        storage::has_bft_v2_material(records_dir)
    }

    pub(crate) fn bft_v2_history_page(
        &self,
        config: &NodeBftExecutionConfigV2,
        after_height: u64,
        max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV2>, String> {
        let state = self
            .bft_execution_v2
            .as_ref()
            .ok_or("BFT V2 execution is not configured")?;
        if config != &state.config {
            return Err("BFT V2 history query differs from pinned config".into());
        }
        state.ledger.history_page(after_height, max_entries)
    }

    pub(crate) fn current_bft_v2_context(
        &self,
        requested_schedule: &BftHeightProposerScheduleV2,
    ) -> Result<NodeBftExecutionContextV2, String> {
        let state = self
            .bft_execution_v2
            .as_ref()
            .ok_or("BFT V2 execution is not configured")?;
        if requested_schedule != &state.cursor_schedule {
            return Err(
                "requested BFT V2 schedule differs from independently replayed cursor".into(),
            );
        }
        let expected_parent = state.ledger.expected_parent(requested_schedule.height())?;
        let expected = NodeBftExecutionContextV2::from_schedule(requested_schedule);
        validate_expected_context_v2(
            &self.execution_world,
            &state.config,
            requested_schedule,
            &expected,
            expected_parent.as_str(),
        )?;
        Ok(expected)
    }

    pub(crate) fn validate_bft_v2_action_payload(&self, payload: &[u8]) -> Result<(), String> {
        if payload.is_empty() || payload.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V2 {
            return Err("BFT V2 action payload is empty or exceeds protocol bound".into());
        }
        let action = NodeConsensusAction::from_payload(
            1,
            "system:bft-v2-action-preflight",
            payload.to_vec(),
        )
        .map_err(|error| format!("build BFT V2 admission probe: {error}"))?;
        let context = NodeExecutionCommitContext {
            world_id: "bft-v2-action-preflight".into(),
            node_id: BFT_V2_EXECUTION_TICK_AUTHORITY.into(),
            proposer_id: BFT_V2_EXECUTION_TICK_AUTHORITY.into(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "00".repeat(32),
            action_root: "00".repeat(32),
            committed_actions: vec![action.clone()],
            committed_at_unix_ms: 0,
        };
        let decoded = super::decode_closed_runtime_actions(&context)?;
        if decoded.len() != 1 {
            return Err("BFT V2 admission requires exactly one closed RuntimeAction".into());
        }
        let Some(schedule) = self
            .bft_execution_v2
            .as_ref()
            .map(|state| state.cursor_schedule.clone())
        else {
            if decoded[0].2.is_some() {
                return Err(
                    "BFT V2 governance action admission requires a configured verified parent"
                        .into(),
                );
            }
            return Ok(());
        };

        // Keep single-payload ingress on the same current-parent verification
        // path as a batch/proposal. A structurally valid proof is not enough:
        // its signatures, policy, nonce, position, and transition schedule
        // must authenticate against the driver's actual V2 cursor and World.
        self.validate_bft_v2_action_batch(&schedule, std::slice::from_ref(&action))
    }

    pub(crate) fn validate_bft_v2_action_batch(
        &self,
        requested_schedule: &BftHeightProposerScheduleV2,
        actions: &[NodeConsensusAction],
    ) -> Result<(), String> {
        let state = self
            .bft_execution_v2
            .as_ref()
            .ok_or("BFT V2 execution is not configured")?;
        if requested_schedule != &state.cursor_schedule {
            return Err("BFT V2 action batch uses a stale or foreign schedule".into());
        }
        let actions_cbor = serde_cbor::to_vec(&actions)
            .map_err(|error| format!("encode BFT V2 action batch: {error}"))?;
        if actions_cbor.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V2 {
            return Err("BFT V2 action batch exceeds the protocol bound".into());
        }
        let context = make_commit_context(
            &state.config,
            requested_schedule.height(),
            "00".repeat(32),
            actions,
        )?;
        let decoded = super::decode_closed_runtime_actions(&context)?;
        validate_validator_transition_action_schedule_v2(requested_schedule, &decoded)?;
        let mut parent = self.execution_world.clone();
        parent
            .attach_bft_governance_parent_v2(requested_schedule)
            .map_err(|error| format!("attach BFT V2 governance admission parent: {error:?}"))?;
        validate_parent_governance_actions(&parent, &context)?;
        validate_set_change_actions_have_registry_v2(&parent, &decoded)?;
        Ok(())
    }

    pub(crate) fn build_bft_v2(
        &self,
        context: NodeBftBuildContextV2,
    ) -> Result<BftBlockV2, String> {
        let state = self
            .bft_execution_v2
            .as_ref()
            .ok_or("BFT V2 execution is not configured")?;
        let expected_parent = state.ledger.expected_parent(context.execution.height)?;
        validate_expected_context_v2(
            &self.execution_world,
            &state.config,
            &state.cursor_schedule,
            &context.execution,
            expected_parent.as_str(),
        )?;
        let actions_cbor = serde_cbor::to_vec(&context.actions)
            .map_err(|error| format!("encode BFT V2 actions: {error}"))?;
        if actions_cbor.is_empty() || actions_cbor.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V2 {
            return Err("BFT V2 actions exceed bounded execution input size".into());
        }
        let admission_context = make_commit_context(
            &state.config,
            context.execution.height,
            "00".repeat(32),
            &context.actions,
        )?;
        let decoded = super::decode_closed_runtime_actions(&admission_context)?;
        validate_validator_transition_action_schedule_v2(&state.cursor_schedule, &decoded)?;
        let has_set_change_action = has_validator_set_change_action_v2(&decoded);
        let mut admission_parent = self.execution_world.clone();
        admission_parent
            .attach_bft_governance_parent_v2(&state.cursor_schedule)
            .map_err(|error| format!("attach BFT V2 candidate admission parent: {error:?}"))?;
        validate_parent_governance_actions(&admission_parent, &admission_context)?;
        validate_set_change_actions_have_registry_v2(&admission_parent, &decoded)?;
        let action_root = super::action_root(context.actions.as_slice())?;
        let (transition, transition_ref) = transition_metadata_for_schedule(
            &state.cursor_schedule,
            &context.actions,
            has_set_change_action,
            None,
        )?;
        let mut block = BftBlockV2 {
            protocol_version: oasis7_proto::distributed_bft::v2::BFT_PROTOCOL_VERSION_V2,
            scope: context.execution.scope.clone(),
            height: context.execution.height,
            parent_commit_ref: context.execution.parent_commit_ref.clone(),
            authority_epoch: context.execution.authority_epoch,
            active_validator_set_hash: state.cursor_schedule.active_set().set_hash().to_string(),
            schedule_policy_hash: state.cursor_schedule.policy_hash().to_string(),
            manifest_hash: context.execution.manifest_hash.clone(),
            post_manifest_hash: context.execution.manifest_hash.clone(),
            actions_cbor,
            action_root,
            event_root: "00".repeat(32),
            execution_block_hash: "00".repeat(32),
            execution_state_root: "00".repeat(32),
            state_root: "00".repeat(32),
            transition,
            transition_ref,
        };
        let executed = execute_candidate_v2(
            &self.execution_world,
            &state.config,
            &state.cursor_schedule,
            &block,
            expected_parent.as_str(),
        )?;
        block.event_root = executed.event_root.clone();
        block.execution_block_hash = executed.execution_block_hash.clone();
        block.execution_state_root = executed.execution_state_root.clone();
        block.state_root = executed.state_root.clone();
        block.post_manifest_hash = executed.post_manifest_hash.clone();
        block.transition = executed.transition;
        block.transition_ref = executed.transition_ref;
        block.validate()?;
        Ok(block)
    }

    pub(crate) fn preview_bft_v2(
        &self,
        expected: &NodeBftExecutionContextV2,
        candidate: &BftBlockV2,
    ) -> Result<(), String> {
        let state = self
            .bft_execution_v2
            .as_ref()
            .ok_or("BFT V2 execution is not configured")?;
        let expected_parent = state.ledger.expected_parent(expected.height)?;
        validate_expected_context_v2(
            &self.execution_world,
            &state.config,
            &state.cursor_schedule,
            expected,
            expected_parent.as_str(),
        )?;
        validate_candidate_binding_v2(&state.config, &state.cursor_schedule, expected, candidate)?;
        let executed = execute_candidate_v2(
            &self.execution_world,
            &state.config,
            &state.cursor_schedule,
            candidate,
            expected_parent.as_str(),
        )?;
        compare_executed_candidate_v2(candidate, &executed)
    }

    pub(crate) fn commit_bft_v2(
        &mut self,
        token: VerifiedBftCommitCertificateV2,
    ) -> Result<NodeBftExecutionCommitResultV2, String> {
        let (config, cursor_schedule, cursor_height, expected_parent, existing) = {
            let state = self
                .bft_execution_v2
                .as_ref()
                .ok_or("BFT V2 execution is not configured")?;
            let block = token.block();
            (
                state.config.clone(),
                state.cursor_schedule.clone(),
                state.cursor_height,
                state.ledger.expected_parent(block.height)?,
                state.ledger.record(block.height)?,
            )
        };
        let block = token.block();
        if token.next_height_schedule().height() != block.height.saturating_add(1)
            || token.next_height_schedule().parent_commit_ref() != token.block_id()
        {
            return Err("verified BFT V2 successor schedule does not follow its block".into());
        }
        if token.schedule().scope() != &config.scope
            || token.schedule().authority_epoch() != config.authority_epoch
            || token.block_id() != block.block_id()?.as_str()
        {
            return Err("verified BFT V2 certificate is outside pinned config".into());
        }
        if block.height != token.schedule().height() || block.parent_commit_ref != expected_parent {
            return Err("BFT V2 certificate does not extend exact durable predecessor".into());
        }
        let expected = NodeBftExecutionContextV2::from_schedule(token.schedule());
        validate_candidate_binding_v2(&config, token.schedule(), &expected, block)?;

        if let Some(record) = existing.as_ref() {
            if &record.certificate != token.certificate() || record.block_id != token.block_id() {
                return Err("durable BFT V2 height contains different certificate evidence".into());
            }
            if cursor_height > block.height {
                let digest = self
                    .bft_execution_v2
                    .as_ref()
                    .expect("BFT V2 state remains configured")
                    .ledger
                    .durable_install_digest(block.height)?;
                return Ok(commit_result_v2(block, token.block_id(), digest));
            }
            if cursor_height == block.height {
                validate_live_world_matches_record_v2(
                    &self.execution_world,
                    record,
                    &self.execution_store,
                )?;
                let digest = self
                    .bft_execution_v2
                    .as_ref()
                    .expect("BFT V2 state remains configured")
                    .ledger
                    .durable_install_digest(block.height)?;
                return Ok(commit_result_v2(block, token.block_id(), digest));
            }
            if token.schedule() != &cursor_schedule {
                return Err(
                    "verified BFT V2 certificate is not from the exact replay cursor".into(),
                );
            }
        } else if token.schedule() != &cursor_schedule {
            return Err("verified BFT V2 certificate is not from the exact replay cursor".into());
        }
        if cursor_height.checked_add(1) != Some(block.height) {
            return Err("BFT V2 certificate replay skips execution cursor".into());
        }
        if existing.is_none()
            && self
                .bft_execution_v2
                .as_ref()
                .expect("configured above")
                .ledger
                .tip_height()
                != cursor_height
        {
            return Err("new BFT V2 certificate does not follow durable ledger tip".into());
        }
        let executed = execute_candidate_v2(
            &self.execution_world,
            &config,
            token.schedule(),
            block,
            expected_parent.as_str(),
        )?;
        compare_executed_candidate_v2(block, &executed)?;

        {
            let state = self
                .bft_execution_v2
                .as_mut()
                .expect("BFT V2 state remains configured");
            if let Some(record) = existing.as_ref() {
                state.ledger.validate_record_material(
                    record,
                    executed.snapshot_bytes.as_slice(),
                    executed.journal_bytes.as_slice(),
                    &self.execution_store,
                )?;
                state.ledger.accept_existing_record(record)?;
            } else {
                state.ledger.commit_candidate(
                    storage::BftV2CommitCandidate {
                        certificate: token.certificate().clone(),
                        block_id: token.block_id().to_string(),
                        block,
                        snapshot_bytes: executed.snapshot_bytes.as_slice(),
                        journal_bytes: executed.journal_bytes.as_slice(),
                        execution_block_hash: executed.execution_block_hash.as_str(),
                        execution_state_root: executed.execution_state_root.as_str(),
                        state_root: executed.state_root.as_str(),
                        event_root: executed.event_root.as_str(),
                    },
                    &self.execution_store,
                )?;
            }
        }
        self.install_bft_v2_world(block.height, executed.world)?;
        let state = self
            .bft_execution_v2
            .as_mut()
            .expect("BFT V2 state remains configured");
        state.cursor_height = block.height;
        state.cursor_schedule = token.next_height_schedule().clone();
        let driver_install_digest = state.ledger.durable_install_digest(block.height)?;
        Ok(commit_result_v2(
            block,
            token.block_id(),
            driver_install_digest,
        ))
    }

    fn install_bft_v2_world(&mut self, height: u64, world: RuntimeWorld) -> Result<(), String> {
        super::super::persist_execution_world(self.world_dir.as_path(), &world)?;
        self.execution_world = world;
        self.state.last_applied_committed_height = height;
        Ok(())
    }
}

pub(super) fn validate_world_scope_v2(
    world: &RuntimeWorld,
    config: &NodeBftExecutionConfigV2,
) -> Result<(), String> {
    config.scope.validate()?;
    validate_world_scope_for_scope(world, &config.scope)
}

fn validate_world_scope_for_scope(world: &RuntimeWorld, scope: &BftScopeV2) -> Result<(), String> {
    scope.validate()?;
    let manifest = world.chain_resource_manifest();
    if manifest.world_id != scope.world_id
        || manifest.chain_id != scope.chain_id
        || manifest.genesis_ref.as_deref() != Some(scope.genesis_hash.as_str())
    {
        return Err("BFT V2 scope differs from authenticated World resource identity".into());
    }
    Ok(())
}

fn validate_expected_context_v2(
    world: &RuntimeWorld,
    config: &NodeBftExecutionConfigV2,
    schedule: &BftHeightProposerScheduleV2,
    expected: &NodeBftExecutionContextV2,
    expected_parent: &str,
) -> Result<(), String> {
    if expected.scope != config.scope || expected.authority_epoch != config.authority_epoch {
        return Err("BFT V2 context differs from pinned config".into());
    }
    validate_expected_world_context_v2(
        world,
        &config.scope,
        config.authority_epoch,
        schedule,
        expected,
        expected_parent,
    )
}

fn validate_expected_world_context_v2(
    world: &RuntimeWorld,
    scope: &BftScopeV2,
    authority_epoch: u64,
    schedule: &BftHeightProposerScheduleV2,
    expected: &NodeBftExecutionContextV2,
    expected_parent: &str,
) -> Result<(), String> {
    let identity = world
        .execution_state_identity()
        .map_err(|error| format!("read BFT V2 parent state root: {error:?}"))?;
    let manifest = world
        .current_manifest_hash()
        .map_err(|error| format!("read BFT V2 parent manifest: {error:?}"))?;
    let next_tick = world
        .state()
        .time
        .checked_add(1)
        .ok_or("BFT V2 execution height overflow")?;
    if expected.scope != *scope
        || expected.scope != *schedule.scope()
        || expected.authority_epoch != authority_epoch
        || expected.authority_epoch != schedule.authority_epoch()
        || expected.height != schedule.height()
        || expected.height != next_tick
        || expected.parent_tick != world.state().time
        || expected.parent_tick != schedule.parent_tick()
        || expected.parent_state_root != identity.state_root_hash
        || expected.parent_state_root != schedule.parent_state_root()
        || expected.manifest_hash != manifest
        || expected.manifest_hash != schedule.parent_manifest_hash()
        || expected.parent_commit_ref != expected_parent
        || expected.parent_commit_ref != schedule.parent_commit_ref()
        || expected.active_validator_set_hash != schedule.active_set().set_hash()
        || expected.schedule_policy_hash != schedule.policy_hash()
        || schedule.policy().hash()? != schedule.policy_hash()
    {
        return Err("BFT V2 context differs from exact World, schedule or ledger parent".into());
    }
    if world.pending_actions_len() != 0 || world.pending_effects_len() != 0 {
        return Err("BFT V2 parent World has pending local actions or effects".into());
    }
    validate_world_scope_for_scope(world, scope)
}

fn validate_candidate_binding_v2(
    config: &NodeBftExecutionConfigV2,
    schedule: &BftHeightProposerScheduleV2,
    expected: &NodeBftExecutionContextV2,
    candidate: &BftBlockV2,
) -> Result<(), String> {
    candidate.validate()?;
    if candidate.protocol_version != oasis7_proto::distributed_bft::v2::BFT_PROTOCOL_VERSION_V2
        || candidate.scope != config.scope
        || candidate.scope != expected.scope
        || candidate.scope != *schedule.scope()
        || candidate.height != expected.height
        || candidate.height != schedule.height()
        || candidate.parent_commit_ref != expected.parent_commit_ref
        || candidate.parent_commit_ref != schedule.parent_commit_ref()
        || candidate.authority_epoch != config.authority_epoch
        || candidate.authority_epoch != expected.authority_epoch
        || candidate.active_validator_set_hash != schedule.active_set().set_hash()
        || candidate.schedule_policy_hash != schedule.policy_hash()
        || candidate.manifest_hash != expected.manifest_hash
        || candidate.manifest_hash != schedule.parent_manifest_hash()
    {
        return Err("BFT V2 block differs from exact local schedule context".into());
    }
    let expected_transition_ref = schedule.pending_transition();
    match (expected_transition_ref, candidate.transition_ref.as_ref()) {
        (Some(expected), Some(actual)) if expected == actual => {}
        (None, None) => {}
        _ => {
            return Err(
                "BFT V2 block has a missing or foreign pending transition reference".into(),
            );
        }
    }
    Ok(())
}

fn execute_candidate_v2(
    parent: &RuntimeWorld,
    config: &NodeBftExecutionConfigV2,
    schedule: &BftHeightProposerScheduleV2,
    block: &BftBlockV2,
    expected_parent: &str,
) -> Result<ExecutedCandidateV2, String> {
    let input_hash = execution_input_hash_v2(block)?;
    let executed = execution::execute_world_transition(execution::WorldTransitionRequest {
        parent,
        scope: &config.scope,
        authority_epoch: config.authority_epoch,
        schedule,
        block,
        expected_parent,
        input_hash: &input_hash,
        tick_authority: BFT_V2_EXECUTION_TICK_AUTHORITY,
        formal_handoff: None,
        formal_prefix: None,
    })?;
    let execution_block_hash = execution_block_hash_v2(
        block,
        &executed.event_root,
        &executed.execution_state_root,
        &executed.state_root,
        &executed.post_manifest_hash,
        &executed.transition,
        &executed.transition_ref,
    )?;
    Ok(ExecutedCandidateV2 {
        world: executed.world,
        snapshot_bytes: executed.snapshot_bytes,
        journal_bytes: executed.journal_bytes,
        event_root: executed.event_root,
        execution_block_hash,
        execution_state_root: executed.execution_state_root,
        state_root: executed.state_root,
        post_manifest_hash: executed.post_manifest_hash,
        transition: executed.transition,
        transition_ref: executed.transition_ref,
    })
}

fn decode_actions_v2(bytes: &[u8]) -> Result<Vec<NodeConsensusAction>, String> {
    if bytes.is_empty() || bytes.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V2 {
        return Err("BFT V2 action bytes are empty or exceed the protocol bound".into());
    }
    let actions: Vec<NodeConsensusAction> = serde_cbor::from_slice(bytes)
        .map_err(|error| format!("decode BFT V2 action bytes: {error}"))?;
    if serde_cbor::to_vec(&actions).map_err(|error| error.to_string())? != bytes {
        return Err("BFT V2 action bytes are not canonical CBOR".into());
    }
    super::action_root(actions.as_slice())?;
    Ok(actions)
}

fn governance_action_digest(actions: &[NodeConsensusAction]) -> Result<String, String> {
    let mut governance_actions = Vec::new();
    for action in actions {
        let envelope = decode_consensus_action_payload_envelope(action.payload_cbor.as_slice())
            .map_err(|error| format!("decode governance action digest input: {error}"))?;
        let is_validator_admission = matches!(
            envelope.body,
            ConsensusActionPayloadBody::RuntimeAction {
                action: oasis7_runtime::Action::SubmitGovernanceValidatorAdmission { .. }
                    | oasis7_runtime::Action::ApproveGovernanceValidatorAdmission { .. }
                    | oasis7_runtime::Action::ActivateGovernanceValidatorAdmission { .. }
                    | oasis7_runtime::Action::RevokeGovernanceValidatorAdmission { .. }
            }
        );
        if is_validator_admission {
            governance_actions.push(action);
        }
    }
    let bytes = serde_cbor::to_vec(&("oasis7.bft.v2.governance-actions", governance_actions))
        .map_err(|error| format!("encode BFT V2 governance action digest: {error}"))?;
    Ok(blake3_hex(bytes.as_slice()))
}

fn validate_parent_governance_actions(
    parent: &RuntimeWorld,
    context: &NodeExecutionCommitContext,
) -> Result<(), String> {
    let decoded = super::decode_closed_runtime_actions(context)?;
    let mut nonce_highwater = BTreeMap::<String, u64>::new();
    for (action, _, authorization) in decoded {
        if let Some(authorization) = authorization {
            let previous = nonce_highwater
                .entry(authorization.controller_account_id.clone())
                .or_insert(0);
            if authorization.nonce <= *previous {
                return Err(
                    "BFT V2 governance action batch has a repeated or descending nonce".into(),
                );
            }
            *previous = authorization.nonce;
            parent
                .verify_governance_action_for_next_commit(&action, &authorization)
                .map_err(|error| {
                    format!("BFT V2 parent governance authorization failed: {error:?}")
                })?;
        }
    }
    Ok(())
}

fn make_commit_context(
    config: &NodeBftExecutionConfigV2,
    height: u64,
    node_block_hash: String,
    actions: &[NodeConsensusAction],
) -> Result<NodeExecutionCommitContext, String> {
    make_commit_context_for_binding(
        &config.scope,
        config.authority_epoch,
        BFT_V2_EXECUTION_TICK_AUTHORITY,
        height,
        node_block_hash,
        actions,
    )
}

fn make_commit_context_for_binding(
    scope: &BftScopeV2,
    authority_epoch: u64,
    tick_authority: &str,
    height: u64,
    node_block_hash: String,
    actions: &[NodeConsensusAction],
) -> Result<NodeExecutionCommitContext, String> {
    Ok(NodeExecutionCommitContext {
        world_id: scope.world_id.clone(),
        node_id: tick_authority.to_owned(),
        proposer_id: tick_authority.to_owned(),
        height,
        slot: height.saturating_sub(1),
        epoch: authority_epoch,
        node_block_hash,
        action_root: super::action_root(actions)?,
        committed_actions: actions.to_vec(),
        committed_at_unix_ms: 0,
    })
}

fn execution_input_hash_v2(block: &BftBlockV2) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(&(
        BFT_V2_EXECUTION_HASH_DOMAIN,
        &block.scope,
        block.height,
        &block.parent_commit_ref,
        block.authority_epoch,
        &block.active_validator_set_hash,
        &block.schedule_policy_hash,
        &block.manifest_hash,
        &block.actions_cbor,
        &block.action_root,
    ))
    .map_err(|error| format!("encode BFT V2 execution input: {error}"))?;
    Ok(blake3_hex(bytes.as_slice()))
}

fn execution_block_hash_v2(
    block: &BftBlockV2,
    event_root: &str,
    execution_state_root: &str,
    state_root: &str,
    post_manifest_hash: &str,
    transition: &Option<BftValidatorTransitionV2>,
    transition_ref: &Option<BftValidatorTransitionRefV2>,
) -> Result<String, String> {
    let payload = ExecutionHashPayloadV2 {
        domain: BFT_V2_EXECUTION_HASH_DOMAIN,
        scope: &block.scope,
        height: block.height,
        parent_commit_ref: &block.parent_commit_ref,
        authority_epoch: block.authority_epoch,
        active_validator_set_hash: &block.active_validator_set_hash,
        schedule_policy_hash: &block.schedule_policy_hash,
        manifest_hash: &block.manifest_hash,
        action_root: &block.action_root,
        event_root,
        execution_state_root,
        state_root,
        post_manifest_hash,
        transition,
        transition_ref,
    };
    let bytes = serde_cbor::to_vec(&payload)
        .map_err(|error| format!("encode BFT V2 execution hash: {error}"))?;
    Ok(blake3_hex(bytes.as_slice()))
}

fn compare_executed_candidate_v2(
    block: &BftBlockV2,
    executed: &ExecutedCandidateV2,
) -> Result<(), String> {
    let actual_execution_hash = execution_block_hash_v2(
        block,
        executed.event_root.as_str(),
        executed.execution_state_root.as_str(),
        executed.state_root.as_str(),
        executed.post_manifest_hash.as_str(),
        &executed.transition,
        &executed.transition_ref,
    )?;
    if block.event_root != executed.event_root
        || block.execution_block_hash != actual_execution_hash
        || block.execution_state_root != executed.execution_state_root
        || block.state_root != executed.state_root
        || block.post_manifest_hash != executed.post_manifest_hash
        || block.transition != executed.transition
        || block.transition_ref != executed.transition_ref
    {
        return Err("BFT V2 candidate differs from isolated Runtime replay".into());
    }
    Ok(())
}

fn commit_result_v2(
    block: &BftBlockV2,
    block_id: &str,
    driver_install_digest: String,
) -> NodeBftExecutionCommitResultV2 {
    NodeBftExecutionCommitResultV2 {
        height: block.height,
        bft_commit_ref: block_id.to_string(),
        execution_block_hash: block.execution_block_hash.clone(),
        execution_state_root: block.execution_state_root.clone(),
        state_root: block.state_root.clone(),
        post_manifest_hash: block.post_manifest_hash.clone(),
        driver_install_digest,
    }
}

fn validate_live_world_matches_record_v2(
    world: &RuntimeWorld,
    record: &storage::BftV2ExecutionRecord,
    store: &LocalCasStore,
) -> Result<(), String> {
    let snapshot = super::super::super::snapshot_to_cbor(&world.snapshot())?;
    let journal = super::super::super::journal_to_cbor(world.journal())?;
    if blake3_hex(snapshot.as_slice()) != record.execution_state_root
        || world
            .execution_state_identity()
            .map_err(|error| format!("read BFT V2 current state root: {error:?}"))?
            .state_root_hash
            != record.state_root
        || world
            .current_manifest_hash()
            .map_err(|error| format!("read BFT V2 current manifest: {error:?}"))?
            != record.post_manifest_hash
        || super::storage::read_cas_bounded(
            store,
            record.snapshot_ref.as_str(),
            super::storage::MAX_BFT_SNAPSHOT_BYTES,
        )? != snapshot
        || super::storage::read_cas_bounded(
            store,
            record.journal_ref.as_str(),
            super::storage::MAX_BFT_JOURNAL_BYTES,
        )? != journal
    {
        return Err("live World differs from exact committed BFT V2 material".into());
    }
    Ok(())
}
