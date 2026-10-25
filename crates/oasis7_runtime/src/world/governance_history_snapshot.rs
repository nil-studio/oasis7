//! Immutable authorization projection of an exact qualified Snapshot.
//! This type cannot publish a World or replace complete checkpoint recovery.
use super::{World, formal_retention::AttachedFormalHistory};
use crate::{Journal, VerifiedFormalRetainedHistoryParentV1};
use oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader;
use std::sync::Arc;

/// Failure provenance of bounded governance-history verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceHistoryVerificationError {
    Invalid(String),
    Capacity(String),
    Budget(String),
}
impl From<String> for GovernanceHistoryVerificationError {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}
impl From<&str> for GovernanceHistoryVerificationError {
    fn from(message: &str) -> Self {
        Self::Invalid(message.into())
    }
}
impl std::fmt::Display for GovernanceHistoryVerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) | Self::Capacity(s) | Self::Budget(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for GovernanceHistoryVerificationError {}

pub struct VerifiedGovernanceHistorySnapshot {
    // Never exposed: unrelated World components deliberately remain unloaded.
    projection: World,
}

impl VerifiedGovernanceHistorySnapshot {
    pub fn from_verified_snapshot_bytes(
        snapshot_bytes: &[u8],
        journal: Journal,
        parent: VerifiedFormalRetainedHistoryParentV1,
        reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, String> {
        if snapshot_bytes.is_empty() || snapshot_bytes.len() > 256 * 1024 * 1024 {
            return Err("governance history snapshot byte bound".into());
        }
        if blake3::hash(snapshot_bytes).to_hex().as_str() != parent.after_snapshot_root() {
            return Err("governance history snapshot bytes differ from qualified parent".into());
        }
        let snapshot = oasis7_world_state::codec::snapshot_from_cbor(snapshot_bytes)
            .map_err(|e| e.to_string())?;
        // Keep the canonical typed-Snapshot contract of full World restoration.
        // Reuse the immutable projection thereafter, rather than rebuilding a full
        // World and serializing both preceding/current Worlds at every step.
        let canonical =
            oasis7_world_state::codec::snapshot_cbor(&snapshot).map_err(|e| e.to_string())?;
        if blake3::hash(&canonical).to_hex().as_str() != parent.after_snapshot_root()
            || snapshot.state.retained_history_root.as_ref() != Some(parent.root())
            || parent
                .logical_tick()
                .is_some_and(|t| t != snapshot.state.time)
        {
            return Err("governance history snapshot differs from qualified parent".into());
        }
        drop(canonical);
        if snapshot.journal_len != journal.len() {
            return Err("governance history requires an exact complete checkpoint Journal".into());
        }
        super::persistence::validate_journal_event_sequence(&journal)
            .map_err(|e| format!("governance history journal: {e:?}"))?;
        if snapshot.journal_len > 0
            && journal.events[snapshot.journal_len - 1].id != snapshot.last_event_id
        {
            return Err("governance history journal last event mismatch".into());
        }
        if !snapshot.journal_commitment.is_empty()
            && journal.commitment().map_err(|e| format!("{e:?}"))? != snapshot.journal_commitment
        {
            return Err("governance history journal commitment mismatch".into());
        }
        World::validate_governance_execution_policy(&snapshot.governance_execution_policy)
            .map_err(|e| format!("governance history policy: {e:?}"))?;
        // Full restoration initializes identity bindings and compatibility state.
        // Require that initialization to leave the qualified state unchanged:
        // in particular a missing signer binding must not become an apparently
        // unbound identity accepted by the projection's admission reducer.
        if snapshot.next_governance_identity_penalty_id == 0 {
            return Err("governance history allocator requires checkpoint normalization".into());
        }
        let original_state = snapshot.state.clone();
        let mut projection = World::new_with_state(snapshot.state);
        if projection.state != original_state {
            return Err("governance history state requires checkpoint normalization".into());
        }
        drop(original_state);
        // Preserve all inputs to the existing shared admission/action reducers.
        projection.governance_execution_policy = snapshot.governance_execution_policy;
        projection.governance_finality_epoch_snapshots =
            snapshot.governance_finality_epoch_snapshots;
        projection.governance_emergency_brake_until_tick =
            snapshot.governance_emergency_brake_until_tick;
        projection.governance_identity_penalties = snapshot.governance_identity_penalties;
        projection.next_governance_identity_penalty_id =
            snapshot.next_governance_identity_penalty_id;
        projection.next_event_id = if snapshot.journal_len > 0 && snapshot.last_event_id == u64::MAX
        {
            1
        } else {
            snapshot.last_event_id.saturating_add(1).max(1)
        };
        projection.next_event_id_era = snapshot.event_id_era;
        projection.journal = journal;
        projection.formal_history_reader = Some(
            AttachedFormalHistory::for_verified_governance_projection(parent, reader),
        );
        Ok(Self { projection })
    }

    pub fn verify_successor(
        &self,
        next: &Self,
        check_budget: impl FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        self.verify_successor_typed(next, check_budget)
            .map_err(|e| e.to_string())
    }

    pub fn verify_successor_typed(
        &self,
        next: &Self,
        mut check_budget: impl FnMut() -> Result<(), String>,
    ) -> Result<(), GovernanceHistoryVerificationError> {
        check_budget().map_err(GovernanceHistoryVerificationError::Budget)?;
        let parent = self
            .projection
            .formal_history_reader
            .as_ref()
            .and_then(|h| h.parent.as_ref())
            .ok_or("governance history qualified projection parent missing")?;
        let successor = next
            .projection
            .formal_history_reader
            .as_ref()
            .and_then(|h| h.parent.as_ref())
            .ok_or("governance history qualified successor projection missing")?;
        self.projection
            .verify_governance_projection_successor_typed(
                &next.projection,
                parent,
                successor,
                check_budget,
            )
    }
}
