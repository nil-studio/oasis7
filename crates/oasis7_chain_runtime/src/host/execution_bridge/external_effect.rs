use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use oasis7_node::NodeExecutionCommitContext;
use oasis7_runtime::{BlobStore, LocalCasStore, ModuleRegistry, World as RuntimeWorld, blake3_hex};

use super::checkpoint::{
    execution_bridge_record_path, execution_checkpoint_manifest_path,
    execution_checkpoint_root_dir, list_execution_bridge_record_heights,
    load_execution_bridge_record, load_execution_checkpoint_manifest,
    load_latest_execution_checkpoint_manifest,
};
use super::{
    EXECUTION_BRIDGE_DEFAULT_HOT_WINDOW_HEIGHTS,
    EXECUTION_EXTERNAL_EFFECT_CONTRACT_CLOSED_WORLD_V1, EXECUTION_EXTERNAL_EFFECT_SCHEMA_V1,
    ExecutionBridgeRecord, ExecutionCheckpointManifest, ExecutionCommittedActionAnchor,
    ExecutionExternalEffectMaterialization, ExecutionModuleResolutionAnchor, ExecutionReplayPlan,
    ExecutionReplayRecordInput,
};

impl ExecutionExternalEffectMaterialization {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.schema_version < EXECUTION_EXTERNAL_EFFECT_SCHEMA_V1 {
            return Err(format!(
                "execution external effect has invalid schema_version={} at height={}",
                self.schema_version, self.height
            ));
        }
        if self.contract != EXECUTION_EXTERNAL_EFFECT_CONTRACT_CLOSED_WORLD_V1 {
            return Err(format!(
                "execution external effect has unsupported contract={} at height={}",
                self.contract, self.height
            ));
        }
        if self.world_id.trim().is_empty()
            || self.node_id.trim().is_empty()
            || self.node_block_hash.trim().is_empty()
            || self.action_root.trim().is_empty()
            || self.pre_step_execution_state_root.trim().is_empty()
            || self.world_manifest_hash.trim().is_empty()
        {
            return Err(format!(
                "execution external effect missing required fields at height={}",
                self.height
            ));
        }
        if !self.unresolved_inputs.is_empty() {
            return Err(format!(
                "execution external effect unresolved inputs at height={} inputs={:?}",
                self.height, self.unresolved_inputs
            ));
        }
        let expected_active_hash = execution_module_anchor_hash(self.active_modules.as_slice())?;
        if self.active_modules_hash != expected_active_hash {
            return Err(format!(
                "execution external effect active_modules_hash mismatch expected={} actual={} height={}",
                expected_active_hash, self.active_modules_hash, self.height
            ));
        }
        let expected_actions_hash =
            execution_committed_actions_hash(self.committed_actions.as_slice())?;
        if self.committed_actions_hash != expected_actions_hash {
            return Err(format!(
                "execution external effect committed_actions_hash mismatch expected={} actual={} height={}",
                expected_actions_hash, self.committed_actions_hash, self.height
            ));
        }
        Ok(())
    }
}

pub(super) fn validate_execution_external_effect_for_context(
    materialization: &ExecutionExternalEffectMaterialization,
    context: &NodeExecutionCommitContext,
) -> Result<(), String> {
    materialization.validate()?;
    let mismatches = [
        (
            "world_id",
            materialization.world_id.as_str(),
            context.world_id.as_str(),
        ),
        (
            "node_id",
            materialization.node_id.as_str(),
            context.node_id.as_str(),
        ),
        (
            "node_block_hash",
            materialization.node_block_hash.as_str(),
            context.node_block_hash.as_str(),
        ),
        (
            "action_root",
            materialization.action_root.as_str(),
            context.action_root.as_str(),
        ),
    ];
    for (field, actual, expected) in mismatches {
        if actual != expected {
            return Err(format!(
                "execution external effect {field} mismatch at height={}: expected={} actual={}",
                context.height, expected, actual
            ));
        }
    }
    if materialization.height != context.height
        || materialization.slot != context.slot
        || materialization.epoch != context.epoch
        || materialization.committed_at_unix_ms != context.committed_at_unix_ms
    {
        return Err(format!(
            "execution external effect context metadata mismatch at height={}: effect=({}, {}, {}, {}) context=({}, {}, {}, {})",
            context.height,
            materialization.height,
            materialization.slot,
            materialization.epoch,
            materialization.committed_at_unix_ms,
            context.height,
            context.slot,
            context.epoch,
            context.committed_at_unix_ms,
        ));
    }
    let expected_actions = collect_execution_committed_action_anchors(context);
    if materialization.committed_actions != expected_actions {
        return Err(format!(
            "execution external effect committed action anchors mismatch at height={}",
            context.height
        ));
    }
    Ok(())
}

pub(super) fn execution_module_anchor_hash(
    anchors: &[ExecutionModuleResolutionAnchor],
) -> Result<String, String> {
    Ok(blake3_hex(super::to_cbor(anchors)?.as_slice()))
}

pub(super) fn execution_committed_actions_hash(
    actions: &[ExecutionCommittedActionAnchor],
) -> Result<String, String> {
    Ok(blake3_hex(super::to_cbor(actions)?.as_slice()))
}

fn collect_execution_module_resolution_anchors(
    execution_world: &RuntimeWorld,
) -> Result<Vec<ExecutionModuleResolutionAnchor>, String> {
    let module_registry = execution_world.module_registry();
    let state = execution_world.state();
    let mut anchors = Vec::new();
    let mut module_ids_with_instances = BTreeSet::new();
    for instance in state.module_instances.values() {
        module_ids_with_instances.insert(instance.module_id.clone());
        let key = ModuleRegistry::record_key(
            instance.module_id.as_str(),
            instance.module_version.as_str(),
        );
        let record = module_registry.records.get(&key).ok_or_else(|| {
            format!(
                "execution external effect missing module record {} for instance {}",
                key, instance.instance_id
            )
        })?;
        anchors.push(ExecutionModuleResolutionAnchor {
            instance_id: instance.instance_id.clone(),
            module_id: instance.module_id.clone(),
            module_version: record.manifest.version.clone(),
            wasm_hash: record.manifest.wasm_hash.clone(),
            install_target: instance.install_target.clone(),
        });
    }

    let mut module_ids_without_instances: Vec<String> =
        module_registry.active.keys().cloned().collect();
    module_ids_without_instances.sort();
    for module_id in module_ids_without_instances {
        if module_ids_with_instances.contains(&module_id) {
            continue;
        }
        let version = module_registry.active.get(&module_id).ok_or_else(|| {
            format!(
                "execution external effect missing active module version for {}",
                module_id
            )
        })?;
        let key = ModuleRegistry::record_key(module_id.as_str(), version.as_str());
        let record = module_registry
            .records
            .get(&key)
            .ok_or_else(|| format!("execution external effect missing module record {}", key))?;
        anchors.push(ExecutionModuleResolutionAnchor {
            instance_id: module_id.clone(),
            module_id: module_id.clone(),
            module_version: version.clone(),
            wasm_hash: record.manifest.wasm_hash.clone(),
            install_target: state
                .installed_module_targets
                .get(&module_id)
                .cloned()
                .unwrap_or_default(),
        });
    }

    anchors.sort_by(|left, right| left.instance_id.cmp(&right.instance_id));
    Ok(anchors)
}

fn collect_execution_committed_action_anchors(
    context: &NodeExecutionCommitContext,
) -> Vec<ExecutionCommittedActionAnchor> {
    let mut anchors: Vec<_> = context
        .committed_actions
        .iter()
        .map(|action| ExecutionCommittedActionAnchor {
            action_id: action.action_id,
            submitter_player_id: action.submitter_player_id.clone(),
            payload_hash: action.payload_hash.clone(),
        })
        .collect();
    anchors.sort_by_key(|left| left.action_id);
    anchors
}

pub(super) fn execution_world_snapshot_root(
    execution_world: &RuntimeWorld,
) -> Result<String, String> {
    let pre_step_snapshot = execution_world.snapshot();
    Ok(blake3_hex(super::to_cbor(pre_step_snapshot)?.as_slice()))
}

pub(super) fn build_execution_external_effect_materialization_with_pre_step_root(
    execution_world: &RuntimeWorld,
    context: &NodeExecutionCommitContext,
    pre_step_execution_state_root: Option<&str>,
) -> Result<ExecutionExternalEffectMaterialization, String> {
    let pre_step_execution_state_root = match pre_step_execution_state_root {
        Some(root) if !root.trim().is_empty() => root.to_string(),
        _ => execution_world_snapshot_root(execution_world)?,
    };
    let world_manifest_hash = execution_world
        .current_manifest_hash()
        .map_err(|err| format!("execution external effect manifest hash failed: {:?}", err))?;
    let active_modules = collect_execution_module_resolution_anchors(execution_world)?;
    let committed_actions = collect_execution_committed_action_anchors(context);
    let materialization = ExecutionExternalEffectMaterialization {
        schema_version: EXECUTION_EXTERNAL_EFFECT_SCHEMA_V1,
        contract: EXECUTION_EXTERNAL_EFFECT_CONTRACT_CLOSED_WORLD_V1.to_string(),
        world_id: context.world_id.clone(),
        node_id: context.node_id.clone(),
        height: context.height,
        slot: context.slot,
        epoch: context.epoch,
        node_block_hash: context.node_block_hash.clone(),
        action_root: context.action_root.clone(),
        committed_at_unix_ms: context.committed_at_unix_ms,
        pre_step_execution_state_root,
        world_manifest_hash,
        active_modules_hash: execution_module_anchor_hash(active_modules.as_slice())?,
        committed_actions_hash: execution_committed_actions_hash(committed_actions.as_slice())?,
        active_modules,
        committed_actions,
        unresolved_inputs: Vec::new(),
    };
    materialization.validate()?;
    Ok(materialization)
}

pub(super) fn persist_execution_external_effect_materialization(
    execution_store: &LocalCasStore,
    materialization: &ExecutionExternalEffectMaterialization,
) -> Result<String, String> {
    materialization.validate()?;
    let bytes = super::to_cbor(materialization)?;
    execution_store
        .put_bytes(bytes.as_slice())
        .map_err(|err| format!("execution external effect CAS put failed: {:?}", err))
}

pub(super) fn load_execution_external_effect_materialization(
    execution_store: &LocalCasStore,
    external_effect_ref: &str,
) -> Result<ExecutionExternalEffectMaterialization, String> {
    let bytes = execution_store.get(external_effect_ref).map_err(|err| {
        format!(
            "execution external effect CAS get failed ref={} err={:?}",
            external_effect_ref, err
        )
    })?;
    let materialization =
        serde_cbor::from_slice::<ExecutionExternalEffectMaterialization>(bytes.as_slice())
            .map_err(|err| {
                format!(
                    "parse execution external effect failed ref={} err={}",
                    external_effect_ref, err
                )
            })?;
    materialization.validate()?;
    Ok(materialization)
}

fn load_execution_replay_record_input(
    execution_store: &LocalCasStore,
    record: ExecutionBridgeRecord,
) -> Result<ExecutionReplayRecordInput, String> {
    let external_effect = match record.external_effect_ref.as_deref() {
        Some(external_effect_ref) => {
            let external_effect = load_execution_external_effect_materialization(
                execution_store,
                external_effect_ref,
            )?;
            if external_effect.world_id != record.world_id {
                return Err(format!(
                    "execution replay input world_id mismatch height={} expected={} actual={}",
                    record.height, record.world_id, external_effect.world_id
                ));
            }
            if external_effect.height != record.height {
                return Err(format!(
                    "execution replay input height mismatch expected={} actual={}",
                    record.height, external_effect.height
                ));
            }
            if let Some(node_block_hash) = record.node_block_hash.as_deref()
                && external_effect.node_block_hash != node_block_hash
            {
                return Err(format!(
                    "execution replay input node_block_hash mismatch height={} expected={} actual={}",
                    record.height, node_block_hash, external_effect.node_block_hash
                ));
            }
            Some(external_effect)
        }
        None => None,
    };
    Ok(ExecutionReplayRecordInput {
        record,
        external_effect,
    })
}

fn find_nearest_execution_checkpoint_manifest(
    execution_records_dir: &Path,
    target_height: u64,
) -> Result<Option<ExecutionCheckpointManifest>, String> {
    if target_height == 0 {
        return Ok(None);
    }
    if let Some(latest) = load_latest_execution_checkpoint_manifest(execution_records_dir)?
        && latest.height <= target_height
    {
        return Ok(Some(latest));
    }
    let checkpoint_root = execution_checkpoint_root_dir(execution_records_dir);
    if !checkpoint_root.exists() {
        return Ok(None);
    }
    let mut best_height = None;
    for entry in fs::read_dir(checkpoint_root.as_path()).map_err(|err| {
        format!(
            "read execution checkpoint root {} failed: {}",
            checkpoint_root.display(),
            err
        )
    })? {
        let entry = entry.map_err(|err| {
            format!(
                "read execution checkpoint dir entry under {} failed: {}",
                checkpoint_root.display(),
                err
            )
        })?;
        let file_type = entry.file_type().map_err(|err| {
            format!(
                "read execution checkpoint dir entry type {} failed: {}",
                entry.path().display(),
                err
            )
        })?;
        if !file_type.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        let Ok(height) = name.parse::<u64>() else {
            continue;
        };
        if height <= target_height && best_height.map(|best| height > best).unwrap_or(true) {
            best_height = Some(height);
        }
    }
    let Some(best_height) = best_height else {
        return Ok(None);
    };
    load_execution_checkpoint_manifest(
        execution_checkpoint_manifest_path(execution_records_dir, best_height).as_path(),
    )
    .map(Some)
}

fn infer_retained_hot_window_start_height(
    execution_records_dir: &Path,
) -> Result<Option<u64>, String> {
    for height in list_execution_bridge_record_heights(execution_records_dir)? {
        let record = load_execution_bridge_record(
            execution_bridge_record_path(execution_records_dir, height).as_path(),
        )?;
        if record.latest_state_ref.is_some()
            || record.snapshot_ref.is_some()
            || record.journal_ref.is_some()
        {
            return Ok(Some(height));
        }
    }
    Ok(None)
}

pub(super) fn build_execution_replay_plan(
    execution_records_dir: &Path,
    execution_store: &LocalCasStore,
    target_height: u64,
) -> Result<ExecutionReplayPlan, String> {
    if target_height == 0 {
        return Ok(ExecutionReplayPlan {
            target_height,
            start_height: 0,
            checkpoint: None,
            records: Vec::new(),
        });
    }
    let checkpoint =
        find_nearest_execution_checkpoint_manifest(execution_records_dir, target_height)?;
    let latest_height = list_execution_bridge_record_heights(execution_records_dir)?
        .last()
        .copied()
        .unwrap_or(0);
    if checkpoint.is_none() && latest_height > 0 {
        let hot_window_start_height =
            infer_retained_hot_window_start_height(execution_records_dir)?.unwrap_or_else(|| {
                latest_height
                    .saturating_sub(EXECUTION_BRIDGE_DEFAULT_HOT_WINDOW_HEIGHTS.saturating_sub(1))
            });
        if target_height < hot_window_start_height {
            return Err(format!(
                "execution replay plan target height {} is outside retained hot window {}..{} and no checkpoint is available",
                target_height, hot_window_start_height, latest_height
            ));
        }
    }
    let start_height = checkpoint
        .as_ref()
        .map(|manifest| manifest.height.saturating_add(1))
        .unwrap_or(1);
    let mut records = Vec::new();
    if start_height <= target_height {
        for height in start_height..=target_height {
            let path = execution_bridge_record_path(execution_records_dir, height);
            if !path.exists() {
                return Err(format!(
                    "execution replay plan missing commit record for height {} at {}",
                    height,
                    path.display()
                ));
            }
            let record = load_execution_bridge_record(path.as_path())?;
            if record.height != height {
                return Err(format!(
                    "execution replay plan height mismatch expected={} actual={} path={}",
                    height,
                    record.height,
                    path.display()
                ));
            }
            records.push(load_execution_replay_record_input(execution_store, record)?);
        }
    }
    Ok(ExecutionReplayPlan {
        target_height,
        start_height,
        checkpoint,
        records,
    })
}
