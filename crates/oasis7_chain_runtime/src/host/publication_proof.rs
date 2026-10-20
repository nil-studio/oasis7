use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::publication_lifecycle::{
    PublicationHeadBinding, PublicationLifecycleSnapshot, SEQUENCER_HEAD_PUBLICATION_GRACE_MS,
};
use super::status_payload::ChainConsensusNetworkHeadStatus;
use oasis7_node::NodeSnapshot;

pub(super) const MAX_PUBLICATION_EPISODE_RECORD_SCAN: usize = 255;

#[derive(Clone, Debug, Deserialize)]
pub(super) struct PublicationExecutionRecord {
    pub(super) world_id: String,
    pub(super) height: u64,
    pub(super) node_block_hash: Option<String>,
    #[serde(default)]
    pub(super) prev_node_block_hash: Option<String>,
    pub(super) execution_block_hash: String,
    pub(super) execution_state_root: String,
    pub(super) timestamp_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PublicationProofErrorKind {
    RecordMissing,
    RecordMalformed,
    ContinuityInvalid,
    AncestryInvalid,
    ChronologyInvalid,
    TimestampMismatch,
    BindingInvalid,
    StateBindingInvalid,
    ScanLimitExceeded,
}

#[derive(Clone, Debug)]
pub(super) struct PublicationEpisodeEvaluation {
    pub(super) episode: PublicationExecutionRecord,
    pub(super) episode_binding: PublicationHeadBinding,
    pub(super) retained: bool,
}

pub(super) fn evaluate_publication_episode(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    records_dir: &Path,
    durable_state: Option<&PublicationLifecycleSnapshot>,
    observed_at_unix_ms: i64,
) -> Result<PublicationEpisodeEvaluation, PublicationProofError> {
    let local_height = snapshot.consensus.committed_height;
    let parent_height = local_height.checked_sub(1).ok_or_else(|| {
        PublicationProofError::new(
            PublicationProofErrorKind::ContinuityInvalid,
            "local_height_zero",
        )
    })?;
    let local = load_record(records_dir, local_height)?;
    let parent = load_record(records_dir, parent_height)?;
    validate_record(&local, local_height, snapshot.world_id.as_str())?;
    validate_record(&parent, parent_height, snapshot.world_id.as_str())?;
    validate_edge(&local, &parent)?;
    if Some(local.timestamp_ms) != snapshot.consensus.last_committed_at_ms {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::TimestampMismatch,
            format!("height={local_height}"),
        ));
    }
    validate_boundary_bindings(snapshot, network_head, &local, &parent)?;

    let Some(state) = durable_state else {
        if observed_at_unix_ms
            .saturating_sub(local.timestamp_ms)
            .max(0)
            > SEQUENCER_HEAD_PUBLICATION_GRACE_MS
        {
            return finalize_evaluation(local, false);
        }
        let history =
            scan_contiguous_history(records_dir, snapshot.world_id.as_str(), local, parent, None)?;
        return finalize_evaluation(history[history.len() - 2].clone(), false);
    };
    if state.world_id != snapshot.world_id {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::StateBindingInvalid,
            "world_mismatch",
        ));
    }
    if let Some(episode) = state.episode.as_ref() {
        if episode.height > local.height {
            return Err(PublicationProofError::new(
                PublicationProofErrorKind::StateBindingInvalid,
                format!("episode_height={}", episode.height),
            ));
        }
        let history = scan_contiguous_history(
            records_dir,
            snapshot.world_id.as_str(),
            local,
            parent,
            Some(episode.height),
        )?;
        let retained = history
            .iter()
            .find(|record| record.height == episode.height)
            .ok_or_else(|| {
                PublicationProofError::new(
                    PublicationProofErrorKind::StateBindingInvalid,
                    format!("episode_height={}", episode.height),
                )
            })?;
        if !binding_matches_record(episode, retained) {
            return Err(PublicationProofError::new(
                PublicationProofErrorKind::StateBindingInvalid,
                format!("episode_height={}", episode.height),
            ));
        }
        return finalize_evaluation(retained.clone(), true);
    }
    let catch_up = state.catch_up.as_ref().ok_or_else(|| {
        PublicationProofError::new(
            PublicationProofErrorKind::StateBindingInvalid,
            "state_phase_missing",
        )
    })?;
    if !binding_matches_record(catch_up, &parent) {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::StateBindingInvalid,
            "catch_up_parent_mismatch",
        ));
    }
    finalize_evaluation(local, false)
}

fn finalize_evaluation(
    episode: PublicationExecutionRecord,
    retained: bool,
) -> Result<PublicationEpisodeEvaluation, PublicationProofError> {
    let node_block_hash = nonempty(episode.node_block_hash.as_deref()).ok_or_else(|| {
        PublicationProofError::new(
            PublicationProofErrorKind::BindingInvalid,
            format!("episode_height={}", episode.height),
        )
    })?;
    if episode.execution_block_hash.trim().is_empty()
        || episode.execution_state_root.trim().is_empty()
    {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::BindingInvalid,
            format!("episode_height={}", episode.height),
        ));
    }
    let episode_binding = PublicationHeadBinding {
        height: episode.height,
        node_block_hash: node_block_hash.to_string(),
        execution_block_hash: episode.execution_block_hash.clone(),
        execution_state_root: episode.execution_state_root.clone(),
        timestamp_ms: episode.timestamp_ms,
    };
    Ok(PublicationEpisodeEvaluation {
        episode,
        episode_binding,
        retained,
    })
}

fn validate_boundary_bindings(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    local: &PublicationExecutionRecord,
    parent: &PublicationExecutionRecord,
) -> Result<(), PublicationProofError> {
    let valid = local.node_block_hash.as_deref() == snapshot.consensus.last_block_hash.as_deref()
        && Some(local.execution_block_hash.as_str())
            == snapshot.consensus.last_execution_block_hash.as_deref()
        && Some(local.execution_state_root.as_str())
            == snapshot.consensus.last_execution_state_root.as_deref()
        && local.prev_node_block_hash.as_deref() == network_head.block_hash.as_deref()
        && parent.node_block_hash.as_deref() == network_head.block_hash.as_deref()
        && Some(parent.execution_block_hash.as_str())
            == network_head.execution_block_hash.as_deref()
        && Some(parent.execution_state_root.as_str())
            == network_head.execution_state_root.as_deref();
    if !valid {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::BindingInvalid,
            format!("local_height={}", local.height),
        ));
    }
    Ok(())
}

fn binding_matches_record(
    binding: &PublicationHeadBinding,
    record: &PublicationExecutionRecord,
) -> bool {
    binding.height == record.height
        && Some(binding.node_block_hash.as_str()) == record.node_block_hash.as_deref()
        && binding.execution_block_hash == record.execution_block_hash
        && binding.execution_state_root == record.execution_state_root
        && binding.timestamp_ms == record.timestamp_ms
}

#[derive(Clone, Debug)]
pub(super) struct PublicationProofError {
    pub(super) kind: PublicationProofErrorKind,
    pub(super) detail: String,
}

impl PublicationProofError {
    fn new(kind: PublicationProofErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

pub(super) fn load_record(
    records_dir: &Path,
    height: u64,
) -> Result<PublicationExecutionRecord, PublicationProofError> {
    let bytes = std::fs::read(record_path(records_dir, height)).map_err(|error| {
        let kind = if error.kind() == ErrorKind::NotFound {
            PublicationProofErrorKind::RecordMissing
        } else {
            PublicationProofErrorKind::RecordMalformed
        };
        PublicationProofError::new(kind, format!("height={height}"))
    })?;
    serde_json::from_slice(&bytes).map_err(|_| {
        PublicationProofError::new(
            PublicationProofErrorKind::RecordMalformed,
            format!("height={height}"),
        )
    })
}

pub(super) fn validate_record(
    record: &PublicationExecutionRecord,
    expected_height: u64,
    expected_world_id: &str,
) -> Result<(), PublicationProofError> {
    if record.height != expected_height || record.world_id != expected_world_id {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::ContinuityInvalid,
            format!("expected_height={expected_height}"),
        ));
    }
    if nonempty(record.node_block_hash.as_deref()).is_none()
        || nonempty(record.prev_node_block_hash.as_deref()).is_none()
    {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::AncestryInvalid,
            format!("height={expected_height}"),
        ));
    }
    Ok(())
}

pub(super) fn validate_edge(
    child: &PublicationExecutionRecord,
    parent: &PublicationExecutionRecord,
) -> Result<(), PublicationProofError> {
    if child.height.checked_sub(1) != Some(parent.height) || child.world_id != parent.world_id {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::ContinuityInvalid,
            format!("child_height={}", child.height),
        ));
    }
    if child.prev_node_block_hash.as_deref() != parent.node_block_hash.as_deref() {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::AncestryInvalid,
            format!("child_height={}", child.height),
        ));
    }
    if child.timestamp_ms < parent.timestamp_ms {
        return Err(PublicationProofError::new(
            PublicationProofErrorKind::ChronologyInvalid,
            format!("child_height={}", child.height),
        ));
    }
    Ok(())
}

pub(super) fn scan_contiguous_history(
    records_dir: &Path,
    world_id: &str,
    local: PublicationExecutionRecord,
    parent: PublicationExecutionRecord,
    target_height: Option<u64>,
) -> Result<Vec<PublicationExecutionRecord>, PublicationProofError> {
    validate_record(&local, local.height, world_id)?;
    validate_record(&parent, parent.height, world_id)?;
    validate_edge(&local, &parent)?;
    let mut records = vec![local, parent];
    loop {
        let oldest = records.last().expect("nonempty publication history");
        if target_height.is_some_and(|target| oldest.height <= target) || oldest.height == 0 {
            return Ok(records);
        }
        if records.len() == MAX_PUBLICATION_EPISODE_RECORD_SCAN {
            if record_path(records_dir, oldest.height - 1).exists() {
                return Err(PublicationProofError::new(
                    PublicationProofErrorKind::ScanLimitExceeded,
                    format!("records={}", records.len() + 1),
                ));
            }
            return Ok(records);
        }
        let previous_height = oldest.height - 1;
        let previous = load_record(records_dir, previous_height)?;
        validate_record(&previous, previous_height, world_id)?;
        validate_edge(oldest, &previous)?;
        records.push(previous);
    }
}

pub(super) fn record_path(records_dir: &Path, height: u64) -> PathBuf {
    records_dir.join(format!("{height:020}.json"))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}
