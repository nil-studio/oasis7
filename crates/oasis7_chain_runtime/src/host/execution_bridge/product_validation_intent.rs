use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use oasis7_runtime::World as RuntimeWorld;

use super::write_bytes_atomic;
use super::{
    ExecutionExternalEffectMaterialization, external_effect::execution_world_snapshot_root,
};

pub(super) const PRODUCT_VALIDATION_INTENT_SCHEMA_V1: u8 = 1;
const PRODUCT_VALIDATION_INTENT_FILE: &str = "product-validation-intent.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ProductValidationIntentMarkerV1 {
    #[serde(default = "default_schema_version")]
    pub schema_version: u8,
    pub world_id: String,
    pub height: u64,
    #[serde(default)]
    pub action_root: String,
    pub journal_len: usize,
    /// Root of the execution world immediately before this committed tick.
    /// This is captured before the staged pre-call world is published so a
    /// retry cannot mistake the mid-tick continuation for its predecessor.
    #[serde(default)]
    pub pre_step_execution_state_root: String,
    /// Complete pre-step external-effect evidence captured before the staged
    /// world is published. Older markers only carry the root and are rebuilt
    /// from the staged world for replay compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_step_external_effect: Option<ExecutionExternalEffectMaterialization>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controlled_preparation_ref: Option<String>,
    /// Root of the exact staged world described by this marker.  A marker is
    /// published before that world, so a later output can temporarily leave
    /// the previous same-height generation on disk.
    #[serde(default)]
    pub staged_execution_state_root: String,
    /// Root of the immediately previous same-height staged generation.  This
    /// makes marker replacement a small transactional chain instead of
    /// treating a valid earlier generation as corruption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_staged_execution_state_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_staged_journal_len: Option<usize>,
}

fn default_schema_version() -> u8 {
    PRODUCT_VALIDATION_INTENT_SCHEMA_V1
}

/// Classify the durable intent marker against the world currently on disk.
/// A marker is written before the staged world, so a crash in that small
/// window leaves the predecessor world and can be safely discarded. Any
/// other mismatch is fail-closed rather than guessing which generation is
/// authoritative.
pub(super) fn world_is_staged_for_product_validation_intent(
    execution_world: &RuntimeWorld,
    marker: &ProductValidationIntentMarkerV1,
    last_applied_committed_height: u64,
) -> Result<bool, String> {
    if marker.height != last_applied_committed_height.saturating_add(1) {
        return Err(format!(
            "product validation intent marker does not match committed head: marker_height={} last_applied={}",
            marker.height, last_applied_committed_height
        ));
    }
    let execution_state_root = execution_world_snapshot_root(execution_world)?;
    if execution_world.state().time == marker.height
        && execution_world.journal().len() == marker.journal_len
        && (marker.staged_execution_state_root.trim().is_empty()
            || execution_state_root == marker.staged_execution_state_root)
    {
        return Ok(true);
    }
    if execution_world.state().time == marker.height
        && marker
            .previous_staged_journal_len
            .is_some_and(|journal_len| execution_world.journal().len() == journal_len)
        && marker
            .previous_staged_execution_state_root
            .as_deref()
            .is_some_and(|root| !root.trim().is_empty() && root == execution_state_root)
    {
        return Ok(true);
    }
    if !marker.pre_step_execution_state_root.trim().is_empty()
        && execution_world.state().time == marker.height.saturating_sub(1)
        && execution_world_snapshot_root(execution_world)? == marker.pre_step_execution_state_root
    {
        return Ok(false);
    }
    Err(format!(
        "product validation intent staged world is inconsistent: height={} world_time={} journal_len={} marker_journal_len={}",
        marker.height,
        execution_world.state().time,
        execution_world.journal().len(),
        marker.journal_len
    ))
}

pub(super) fn product_validation_intent_path(records_dir: &Path) -> PathBuf {
    records_dir.join(PRODUCT_VALIDATION_INTENT_FILE)
}

pub(super) fn load_product_validation_intent(
    records_dir: &Path,
) -> Result<Option<ProductValidationIntentMarkerV1>, String> {
    let path = product_validation_intent_path(records_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path.as_path()).map_err(|err| {
        format!(
            "read product validation intent marker {} failed: {}",
            path.display(),
            err
        )
    })?;
    let marker = serde_json::from_slice::<ProductValidationIntentMarkerV1>(bytes.as_slice())
        .map_err(|err| {
            format!(
                "parse product validation intent marker {} failed: {}",
                path.display(),
                err
            )
        })?;
    if marker.schema_version != PRODUCT_VALIDATION_INTENT_SCHEMA_V1
        || marker.world_id.trim().is_empty()
        || marker.height == 0
        || marker.journal_len == 0
    {
        return Err(format!(
            "invalid product validation intent marker {}",
            path.display()
        ));
    }
    if let Some(effect) = marker.pre_step_external_effect.as_ref() {
        effect.validate().map_err(|err| {
            format!(
                "invalid product validation intent pre-step external effect {}: {}",
                path.display(),
                err
            )
        })?;
        if effect.height != marker.height
            || effect.world_id != marker.world_id
            || (!marker.action_root.is_empty() && effect.action_root != marker.action_root)
            || (!marker.pre_step_execution_state_root.is_empty()
                && effect.pre_step_execution_state_root != marker.pre_step_execution_state_root)
        {
            return Err(format!(
                "product validation intent marker pre-step external effect identity mismatch {}",
                path.display()
            ));
        }
    }
    Ok(Some(marker))
}

pub(super) fn build_product_validation_intent_marker(
    records_dir: &Path,
    staged: &RuntimeWorld,
    world_id: &str,
    height: u64,
    action_root: &str,
    pre_step_execution_state_root: &str,
    pre_step_external_effect: ExecutionExternalEffectMaterialization,
) -> Result<ProductValidationIntentMarkerV1, String> {
    let previous_marker = load_product_validation_intent(records_dir)?.filter(|marker| {
        marker.world_id == world_id
            && marker.height == height
            && (marker.action_root.is_empty() || marker.action_root == action_root)
    });
    let staged_execution_state_root = execution_world_snapshot_root(staged)?;
    Ok(ProductValidationIntentMarkerV1 {
        schema_version: PRODUCT_VALIDATION_INTENT_SCHEMA_V1,
        world_id: world_id.to_string(),
        height,
        action_root: action_root.to_string(),
        journal_len: staged.journal().len(),
        pre_step_execution_state_root: pre_step_execution_state_root.to_string(),
        pre_step_external_effect: Some(pre_step_external_effect),
        controlled_preparation_ref: previous_marker
            .as_ref()
            .and_then(|marker| marker.controlled_preparation_ref.clone()),
        staged_execution_state_root,
        previous_staged_execution_state_root: previous_marker.as_ref().and_then(|marker| {
            (!marker.staged_execution_state_root.trim().is_empty())
                .then_some(marker.staged_execution_state_root.clone())
        }),
        previous_staged_journal_len: previous_marker.as_ref().map(|marker| marker.journal_len),
    })
}

pub(super) fn persist_product_validation_intent_for_staged_world(
    records_dir: &Path,
    staged: &RuntimeWorld,
    context: &oasis7_node::NodeExecutionCommitContext,
    pre_step_execution_state_root: &str,
    pre_step_external_effect: ExecutionExternalEffectMaterialization,
    controlled_preparation_ref: Option<String>,
) -> Result<(), String> {
    let mut marker = build_product_validation_intent_marker(
        records_dir,
        staged,
        &context.world_id,
        context.height,
        &context.action_root,
        pre_step_execution_state_root,
        pre_step_external_effect,
    )?;
    if controlled_preparation_ref.is_some() {
        marker.controlled_preparation_ref = controlled_preparation_ref;
    }
    persist_product_validation_intent(records_dir, &marker)
}

pub(super) fn persist_product_validation_intent(
    records_dir: &Path,
    marker: &ProductValidationIntentMarkerV1,
) -> Result<(), String> {
    fs::create_dir_all(records_dir).map_err(|err| {
        format!(
            "create product validation intent records dir {} failed: {}",
            records_dir.display(),
            err
        )
    })?;
    let bytes = serde_json::to_vec_pretty(marker)
        .map_err(|err| format!("serialize product validation intent marker failed: {err}"))?;
    if marker.controlled_preparation_ref.is_some() {
        super::durable_transaction::write_file_durable(
            &product_validation_intent_path(records_dir),
            &bytes,
        )
    } else {
        write_bytes_atomic(&product_validation_intent_path(records_dir), &bytes)
    }
}

pub(super) fn clear_product_validation_intent(records_dir: &Path) -> Result<(), String> {
    let path = product_validation_intent_path(records_dir);
    match fs::remove_file(path.as_path()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "remove product validation intent marker {} failed: {}",
            path.display(),
            err
        )),
    }
}
