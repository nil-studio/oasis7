//! Pure candidate planning for the profile-v2 retained-history index.
//!
//! This code returns archive CAS writes and exact hot-map prune sets. It never
//! mutates a World, qualifies a candidate, or mints a verified root. The caller
//! must include the new descriptor in the candidate Snapshot and let the
//! ordinary formal execution/qualification path bind that Snapshot root.

use super::World;
use crate::{
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationNonceRecord, WorldError,
    formal_retained_history::{
        AgentChatIntentLookupV1, DistinctReorgEpochSummaryV1, FeedbackPublicationLookupV1,
        FormalRetainedHistoryIndexViewV1, FormalRetainedHistoryRootV1, GameTaskControlLookupV1,
        RetainedAgentIntentV1, RetainedCapabilityInvocationContextV1, RetainedCapabilityNonceV1,
        RetainedHistoryLeafV1, RetainedHistoryReadError, RetainedJournalEventV1,
        RetainedOwnerChatNonceHighWaterV1, RetainedResultRefsV1, RetainedWorldServiceResultV1,
        VerifiedFormalRetainedHistoryParentV1, capability_invocation_context_key,
        chat_result_matches, digest_text, encode_retained_leaf, feedback_result_matches,
        is_correlation_key_digest, is_digest, is_terminal_agent_intent, retained_key_digest,
        retained_leaf,
    },
};
use oasis7_distfs::controlled_authority::formal::business_index::{
    BUSINESS_INDEX_MAX_LEAF_BYTES, BUSINESS_INDEX_MAX_UPDATES, BusinessIndexDeltaV1,
    BusinessIndexLeafV1, BusinessIndexNodeReader,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const HOT_WORLD_SERVICE_RESULTS: usize = 32;
const HOT_WORLD_SERVICE_RESULT_BYTES: usize = 256 * 1024;
const HOT_COMMITTED_CAPABILITY_NONCES: usize = 32;
const HOT_CAPABILITY_CLOSURE_BYTES: usize = 256 * 1024;
const HOT_TERMINAL_AGENT_INTENTS: usize = 32;
const HOT_AGENT_INTENT_BYTES: usize = 256 * 1024;
const HOT_FORMAL_JOURNAL_EVENTS: usize = 256;
const HOT_FORMAL_JOURNAL_CBOR_BYTES: usize = 512 * 1024;
const MAX_JOURNAL_EVENTS_TO_ARCHIVE_PER_PLAN: usize = 256;
const MAX_ARCHIVES_PER_CLASS_PER_PLAN: usize = 128;
const MAX_ESTIMATED_NEW_LEAF_BYTES_PER_PLAN: usize = 16 * 1024 * 1024;
const MAX_RESULT_BYTES_IN_LEAF: usize = BUSINESS_INDEX_MAX_LEAF_BYTES - 16 * 1024;
const MAX_INDEXED_RESULT_REFS: usize = 2;

pub(super) struct AttachedFormalHistory {
    pub(super) parent: Option<VerifiedFormalRetainedHistoryParentV1>,
    view: FormalRetainedHistoryIndexViewV1,
    reader: super::SharedFormalHistoryReader,
    candidate_installed: bool,
}

impl std::fmt::Debug for AttachedFormalHistory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AttachedFormalHistory")
            .field(
                "parent_after_snapshot_root",
                &self
                    .parent
                    .as_ref()
                    .map(|parent| parent.after_snapshot_root()),
            )
            .finish_non_exhaustive()
    }
}

impl Clone for AttachedFormalHistory {
    fn clone(&self) -> Self {
        Self {
            parent: self.parent.clone(),
            view: self.view.clone(),
            reader: self.reader.clone(),
            candidate_installed: self.candidate_installed,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormalWorldRetentionPlanV1 {
    pub base_root: FormalRetainedHistoryRootV1,
    pub proposed_root: FormalRetainedHistoryRootV1,
    /// `None` means the authenticated root is unchanged. Any `Some` delta must
    /// be persisted to CAS and committed together with its proposed root.
    pub delta: Option<BusinessIndexDeltaV1>,
    pub world_service_result_keys_to_prune: Vec<String>,
    pub capability_nonce_keys_to_prune: Vec<String>,
    pub capability_receipt_ids_to_prune: Vec<String>,
    pub capability_invocation_context_keys_to_prune: Vec<String>,
    pub agent_intent_ids_to_prune: Vec<String>,
    /// Exact `(event id era, event id)` prefix moved out of the hot journal.
    pub journal_events_to_prune: Vec<(u64, u64)>,
    pub tick_consensus_archive: super::FormalTickConsensusArchivePlanV1,
}

impl From<RetainedHistoryReadError> for WorldError {
    fn from(error: RetainedHistoryReadError) -> Self {
        Self::DistributedValidationFailed {
            reason: error.to_string(),
        }
    }
}

impl World {
    /// Initialize the independent retained-history index on a fresh staged
    /// World before its first profile-v2 Snapshot is externally approved.
    /// This does not create a verified parent token or migrate existing history.
    pub fn initialize_empty_formal_history(&mut self) -> Result<(), String> {
        if self.state.retained_history_root.is_none()
            && (!self
                .capability_revocation_state
                .world_service_results
                .is_empty()
                || !self.capability_nonce_records.is_empty()
                || !self.capability_authorization_receipts.is_empty()
                || !self.capability_invocation_contexts.is_empty()
                || !self.state.agent_intent_ledger.is_empty())
        {
            return Err(
                "cannot initialize formal history while archivable runtime history exists".into(),
            );
        }
        if self
            .formal_history_reader
            .as_ref()
            .is_some_and(|reader| reader.parent.is_some() || reader.candidate_installed)
        {
            return Err("cannot initialize formal history after a reader is attached".into());
        }

        let empty = FormalRetainedHistoryRootV1::from_index_root(
            oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexRootV1::empty()?,
        )?;
        if let Some(existing) = &self.state.retained_history_root {
            existing.validate()?;
            if existing != &empty {
                return Err(
                    "existing formal history descriptor is not the canonical empty root".into(),
                );
            }
            self.refresh_capability_authorization_root()
                .map_err(|error| format!("{error:?}"))?;
            self.install_staged_empty_history_reader(empty)?;
            return Ok(());
        }
        if !self.tick_consensus_records.is_empty() {
            return Err("formal history initialization must precede tick publication; an existing published World requires an explicit migration".into());
        }
        self.state.retained_history_root = Some(empty.clone());
        self.refresh_capability_authorization_root()
            .map_err(|error| format!("{error:?}"))?;
        self.install_staged_empty_history_reader(empty)?;
        Ok(())
    }

    fn install_staged_empty_history_reader(
        &mut self,
        empty: FormalRetainedHistoryRootV1,
    ) -> Result<(), String> {
        self.formal_history_reader = Some(AttachedFormalHistory {
            parent: None,
            view: FormalRetainedHistoryIndexViewV1::from_validated_root(empty)?,
            reader: super::SharedFormalHistoryReader::new(Arc::new(StagedEmptyHistoryReader)),
            candidate_installed: false,
        });
        Ok(())
    }

    /// Attach process-local CAS access only when this World serializes to the
    /// exact Snapshot authenticated by the qualified parent token.
    pub fn attach_verified_formal_history(
        &mut self,
        parent: VerifiedFormalRetainedHistoryParentV1,
        reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<(), String> {
        let snapshot_bytes =
            serde_cbor::to_vec(&self.snapshot()).map_err(|error| error.to_string())?;
        let current_snapshot_root = blake3::hash(&snapshot_bytes).to_hex().to_string();
        if current_snapshot_root != parent.after_snapshot_root() {
            return Err(
                "World snapshot does not match the verified retained-history parent".into(),
            );
        }
        if self.state.retained_history_root.as_ref() != Some(parent.root()) {
            return Err("World retained-history descriptor differs from verified parent".into());
        }
        self.formal_history_reader = Some(AttachedFormalHistory {
            view: parent.index_view(),
            parent: Some(parent),
            reader: super::SharedFormalHistoryReader::new(reader),
            candidate_installed: false,
        });
        Ok(())
    }

    /// Carry authenticated archive access into a World rebuilt from a candidate
    /// Snapshot. The source must still hold qualified-parent or verified-delta
    /// evidence, and the target descriptor must equal that exact read view.
    /// This only copies process-local access; it does not qualify the Snapshot.
    pub fn inherit_formal_history_reader(&mut self, source: &World) -> Result<(), String> {
        let attached = source
            .formal_history_reader
            .as_ref()
            .ok_or("source World has no authenticated formal-history reader")?;
        if attached.parent.is_none() {
            return Err(
                "staged empty history cannot supply authenticated candidate lineage".into(),
            );
        }
        if source.state.retained_history_root.as_ref() != Some(attached.view.root()) {
            return Err("source World descriptor differs from its authenticated read view".into());
        }
        if self.state.retained_history_root.as_ref() != Some(attached.view.root()) {
            return Err(
                "target World descriptor differs from source authenticated read view".into(),
            );
        }
        self.formal_history_reader = Some(attached.clone());
        Ok(())
    }

    pub(super) fn attached_formal_history(
        &self,
    ) -> Result<
        (
            &FormalRetainedHistoryIndexViewV1,
            &super::SharedFormalHistoryReader,
        ),
        RetainedHistoryReadError,
    > {
        let attached = self.formal_history_reader.as_ref().ok_or_else(|| {
            RetainedHistoryReadError::Unavailable(
                "verified formal history reader is not attached".into(),
            )
        })?;
        if self.state.retained_history_root.as_ref() != Some(attached.view.root()) {
            return Err(RetainedHistoryReadError::Unavailable(
                "World retained-history descriptor changed since reader attachment".into(),
            ));
        }
        Ok((&attached.view, &attached.reader))
    }

    pub fn read_retained_world_service_result(
        &self,
        correlation_key_digest: &str,
    ) -> Result<
        Option<crate::service_admission::CanonicalIntentResultV1>,
        RetainedHistoryReadError,
    > {
        if self.state.retained_history_root.is_none() {
            return Ok(None);
        }
        let (parent, reader) = self.attached_formal_history()?;
        parent.read_world_service_result(correlation_key_digest, reader)
    }

    /// Return the exact `(event id era, event id)` cursor for each currently
    /// resident journal event. The trailing cursor is anchored by the World's
    /// allocator; earlier eras are derived by walking the contiguous hot tail
    /// backward across MAX-to-1 rollovers. Saturated era rollover is ambiguous
    /// and rejected instead of inventing an era.
    pub fn formal_journal_event_cursors(&self) -> Result<Vec<(u64, u64)>, String> {
        let events = &self.journal.events;
        if events.is_empty() {
            return Ok(Vec::new());
        }
        // The event allocator saturates the era counter. Once the current era
        // is MAX, a later MAX-to-1 wrap can reuse a cursor that may already be
        // archived, and a short hot tail cannot prove whether it happened.
        if self.next_event_id_era == u64::MAX {
            return Err("formal journal event-id era is saturated and ambiguous".into());
        }
        if events.iter().any(|event| event.id == 0) {
            return Err("formal journal contains event id zero".into());
        }
        for pair in events.windows(2) {
            let expected = if pair[0].id == u64::MAX {
                1
            } else {
                pair[0].id + 1
            };
            if pair[1].id != expected {
                return Err("formal journal event ids are not contiguous".into());
            }
        }

        let last_event_id = events.last().expect("nonempty journal").id;
        let expected_next_event_id = if last_event_id == u64::MAX {
            1
        } else {
            last_event_id + 1
        };
        if self.next_event_id != expected_next_event_id {
            return Err("formal journal tail differs from the event id allocator".into());
        }

        let mut event_id_era = if last_event_id == u64::MAX {
            if self.next_event_id_era == 0 || self.next_event_id_era == u64::MAX {
                return Err("formal journal tail era is missing or saturated".into());
            }
            self.next_event_id_era - 1
        } else {
            self.next_event_id_era
        };
        let has_rollover = events
            .windows(2)
            .any(|pair| pair[0].id == u64::MAX && pair[1].id == 1);
        if has_rollover && event_id_era == u64::MAX {
            return Err("formal journal contains an ambiguous saturated-era rollover".into());
        }

        let mut cursors = Vec::with_capacity(events.len());
        cursors.push((event_id_era, last_event_id));
        for pair in events.windows(2).rev() {
            if pair[0].id == u64::MAX {
                event_id_era = event_id_era
                    .checked_sub(1)
                    .ok_or("formal journal rollover has no preceding era")?;
            }
            cursors.push((event_id_era, pair[0].id));
        }
        cursors.reverse();
        Ok(cursors)
    }

    /// Read an event from the authenticated cold journal index. An absent
    /// descriptor/reader is an error, and authenticated absence remains
    /// distinct from missing or corrupt CAS nodes.
    pub fn read_retained_journal_event(
        &self,
        event_id_era: u64,
        event_id: u64,
    ) -> Result<Option<crate::WorldEvent>, RetainedHistoryReadError> {
        let (view, reader) = self.attached_formal_history()?;
        view.read_journal_event(event_id_era, event_id, reader)
    }

    /// Verify an exact removed journal prefix against the candidate's current
    /// authenticated archive view. The caller still validates that this list
    /// is the prefix removed from its captured full journal.
    pub fn verify_formal_archived_journal_prefix(
        &self,
        events: &[((u64, u64), crate::WorldEvent)],
    ) -> Result<(), RetainedHistoryReadError> {
        let (view, reader) = self.attached_formal_history()?;
        for (index, ((event_id_era, event_id), event)) in events.iter().enumerate() {
            if *event_id == 0 || event.id != *event_id {
                return Err(RetainedHistoryReadError::Invalid(
                    "formal archived journal cursor/event mismatch".into(),
                ));
            }
            if let Some((previous_cursor, previous_event)) = index
                .checked_sub(1)
                .and_then(|previous| events.get(previous))
            {
                let expected_id = if previous_cursor.1 == u64::MAX {
                    1
                } else {
                    previous_cursor.1 + 1
                };
                let expected_era = if previous_cursor.1 == u64::MAX {
                    previous_cursor.0.checked_add(1).ok_or_else(|| {
                        RetainedHistoryReadError::Invalid(
                            "formal archived journal era overflow".into(),
                        )
                    })?
                } else {
                    previous_cursor.0
                };
                if previous_event.id != previous_cursor.1
                    || (*event_id_era, *event_id) != (expected_era, expected_id)
                {
                    return Err(RetainedHistoryReadError::Invalid(
                        "formal archived journal prefix is not contiguous".into(),
                    ));
                }
            }
            let archived = view.read_journal_event(*event_id_era, *event_id, reader)?;
            if archived.as_ref() != Some(event) {
                return Err(RetainedHistoryReadError::Invalid(
                    "formal archived journal event is absent or differs from the exact prefix"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn read_retained_owner_chat_nonce_high_water(
        &self,
        public_key: &str,
    ) -> Result<Option<u64>, RetainedHistoryReadError> {
        let (parent, reader) = self.attached_formal_history()?;
        parent.owner_chat_nonce_high_water(public_key, reader)
    }

    pub(crate) fn read_retained_agent_chat_intent_results(
        &self,
        lookup: &AgentChatIntentLookupV1,
    ) -> Result<
        Vec<crate::service_admission::CanonicalIntentResultV1>,
        RetainedHistoryReadError,
    > {
        let (parent, reader) = self.attached_formal_history()?;
        parent.agent_chat_intent_results(lookup, reader)
    }

    pub(crate) fn read_retained_feedback_publication_exists(
        &self,
        lookup: &FeedbackPublicationLookupV1,
    ) -> Result<bool, RetainedHistoryReadError> {
        let (parent, reader) = self.attached_formal_history()?;
        parent.feedback_publication_exists(lookup, reader)
    }

    pub(crate) fn read_retained_game_task_control_results(
        &self,
        lookup: &GameTaskControlLookupV1,
    ) -> Result<
        Vec<crate::service_admission::CanonicalIntentResultV1>,
        RetainedHistoryReadError,
    > {
        let (parent, reader) = self.attached_formal_history()?;
        parent.game_task_control_results(lookup, reader)
    }

    pub(crate) fn read_retained_capability_nonce_record(
        &self,
        nonce_key: &str,
    ) -> Result<Option<RetainedCapabilityNonceV1>, RetainedHistoryReadError> {
        let (parent, reader) = self.attached_formal_history()?;
        parent.capability_nonce_record(nonce_key, reader)
    }

    pub(crate) fn read_retained_capability_invocation_context(
        &self,
        nonce_key: &str,
    ) -> Result<Option<crate::CapabilityInvocationContext>, RetainedHistoryReadError> {
        let (parent, reader) = self.attached_formal_history()?;
        parent.capability_invocation_context_for_nonce(nonce_key, reader)
    }

    pub(crate) fn read_retained_agent_intent(
        &self,
        intent_id: &str,
    ) -> Result<Option<RetainedAgentIntentV1>, RetainedHistoryReadError> {
        let (parent, reader) = self.attached_formal_history()?;
        parent.agent_intent(intent_id, reader)
    }

    pub(crate) fn read_retained_reorg_epoch_summary(
        &self,
    ) -> Result<DistinctReorgEpochSummaryV1, RetainedHistoryReadError> {
        let archived = if self.state.retained_history_root.is_some() {
            let (view, reader) = self.attached_formal_history()?;
            view.archived_reorg_epoch_summary(reader)?
        } else {
            DistinctReorgEpochSummaryV1::Empty
        };
        Ok(DistinctReorgEpochSummaryV1::merge_hot(
            &archived,
            self.state
                .agent_intent_ledger
                .values()
                .map(|intent| &intent.reorg_epoch),
        ))
    }

    pub fn prepare_formal_retention_candidate(&self) -> Result<FormalWorldRetentionPlanV1, String> {
        let attached = self
            .formal_history_reader
            .as_ref()
            .ok_or_else(|| "verified formal history reader is not attached".to_string())?;
        if attached.candidate_installed {
            return Err("retention cannot be planned again before candidate qualification".into());
        }
        let parent = attached
            .parent
            .as_ref()
            .ok_or("staged empty history is not a qualified retention parent")?;
        if self.state.retained_history_root.as_ref() != Some(parent.root()) {
            return Err("World retained-history descriptor changed since reader attachment".into());
        }
        prepare_world_retention_candidate(self, parent, &attached.reader)
    }

    /// Apply a deterministic archive/prune plan to an execution candidate.
    /// The plan is regenerated from this exact World, and the sparse delta is
    /// verified against its qualified base before any hot record is removed.
    /// The installed root is a candidate view only; it does not mint a new
    /// qualified parent token or claim formal acceptance.
    pub fn apply_formal_retention_plan(
        &mut self,
        plan: &FormalWorldRetentionPlanV1,
    ) -> Result<(), String> {
        let mut staged = self.clone();
        staged.apply_formal_retention_plan_staged(plan)?;
        *self = staged;
        Ok(())
    }

    fn apply_formal_retention_plan_staged(
        &mut self,
        plan: &FormalWorldRetentionPlanV1,
    ) -> Result<(), String> {
        let tick_before_prune = self.tick_consensus_records.last().cloned();
        let tick_events = tick_before_prune.as_ref().map(|record| {
            self.journal
                .events
                .iter()
                .filter(|event| event.time == record.block.header.tick)
                .cloned()
                .collect::<Vec<_>>()
        });
        let (parent, base_reader) = {
            let attached = self
                .formal_history_reader
                .as_ref()
                .ok_or("verified formal history reader is not attached")?;
            if attached.candidate_installed {
                return Err("formal retention plan already applied to this candidate".into());
            }
            let parent = attached
                .parent
                .as_ref()
                .ok_or("staged empty history is not a qualified retention parent")?;
            if self.state.retained_history_root.as_ref() != Some(parent.root())
                || attached.view.root() != parent.root()
            {
                return Err("World is not on its qualified retained-history parent root".into());
            }
            (parent.clone(), attached.reader.clone())
        };

        let expected = prepare_world_retention_candidate(self, &parent, &base_reader)?;
        if expected != *plan {
            return Err("formal retention plan does not match the current World candidate".into());
        }
        plan.proposed_root.validate()?;
        if plan.base_root != *parent.root() {
            return Err("formal retention plan base root differs from qualified parent".into());
        }
        if let Some(delta) = &plan.delta {
            let base_digest = parent.root().index_root.canonical_result_root()?;
            let verified = delta.verify(&base_digest, &base_reader)?;
            if verified != plan.proposed_root.index_root {
                return Err("formal retention delta does not produce proposed root".into());
            }
        } else if plan.proposed_root != *parent.root() {
            return Err("formal retention plan changed root without a delta".into());
        }
        let proposed_view =
            FormalRetainedHistoryIndexViewV1::from_validated_root(plan.proposed_root.clone())?;
        let node_writes = plan.delta.as_ref().map_or_else(BTreeMap::new, |delta| {
            delta
                .node_writes
                .iter()
                .map(|write| (write.digest.clone(), write.bytes.clone()))
                .collect()
        });
        let reader =
            super::SharedFormalHistoryReader::new(Arc::new(CandidateBusinessIndexReader {
                base: base_reader.clone(),
                node_writes,
            }));

        let current_cursors = self.formal_journal_event_cursors()?;
        let journal_prune_count = plan.journal_events_to_prune.len();
        if journal_prune_count > self.journal.events.len()
            || plan.journal_events_to_prune != current_cursors[..journal_prune_count]
        {
            return Err("formal retention plan does not name the exact journal prefix".into());
        }
        for (cursor, event) in plan
            .journal_events_to_prune
            .iter()
            .zip(self.journal.events.iter())
        {
            let archived = proposed_view
                .read_journal_event(cursor.0, cursor.1, &reader)
                .map_err(|error| error.to_string())?;
            if archived.as_ref() != Some(event) {
                return Err(
                    "formal retention plan does not archive the exact journal prefix".into(),
                );
            }
        }

        if plan.world_service_result_keys_to_prune.iter().any(|key| {
            !self
                .capability_revocation_state
                .world_service_results
                .contains_key(key)
        }) || plan
            .capability_nonce_keys_to_prune
            .iter()
            .any(|key| !self.capability_nonce_records.contains_key(key))
            || plan
                .capability_receipt_ids_to_prune
                .iter()
                .any(|key| !self.capability_authorization_receipts.contains_key(key))
            || plan
                .capability_invocation_context_keys_to_prune
                .iter()
                .any(|key| !self.capability_invocation_contexts.contains_key(key))
            || plan
                .agent_intent_ids_to_prune
                .iter()
                .any(|key| !self.state.agent_intent_ledger.contains_key(key))
        {
            return Err("formal retention plan references a missing hot record".into());
        }

        for key in &plan.world_service_result_keys_to_prune {
            self.capability_revocation_state
                .world_service_results
                .remove(key);
        }
        for key in &plan.capability_nonce_keys_to_prune {
            self.capability_nonce_records.remove(key);
        }
        for key in &plan.capability_receipt_ids_to_prune {
            self.capability_authorization_receipts.remove(key);
        }
        for key in &plan.capability_invocation_context_keys_to_prune {
            self.capability_invocation_contexts.remove(key);
        }
        for key in &plan.agent_intent_ids_to_prune {
            self.state.agent_intent_ledger.remove(key);
        }
        self.journal.events.drain(..journal_prune_count);
        self.state.retained_history_root = Some(plan.proposed_root.clone());
        self.refresh_capability_authorization_root()
            .map_err(|error| format!("{error:?}"))?;

        self.formal_history_reader = Some(AttachedFormalHistory {
            parent: Some(parent),
            view: proposed_view,
            reader,
            candidate_installed: true,
        });
        if let (Some(record), Some(events)) = (tick_before_prune, tick_events) {
            self.refresh_candidate_tick_after_retention(&record, &events)?;
        }
        self.apply_formal_tick_consensus_archive(&plan.tick_consensus_archive)?;
        Ok(())
    }
}

/// Only explicit local initialization installs this reader. The canonical
/// empty index proves absence without CAS; it supplies no qualified parent.
struct StagedEmptyHistoryReader;
impl BusinessIndexNodeReader for StagedEmptyHistoryReader {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Err("staged empty history unexpectedly requested a CAS node".into())
    }
}

#[derive(Clone)]
struct CandidateBusinessIndexReader {
    base: super::SharedFormalHistoryReader,
    node_writes: BTreeMap<String, Vec<u8>>,
}

impl BusinessIndexNodeReader for CandidateBusinessIndexReader {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        match self.node_writes.get(digest) {
            Some(bytes) => Ok(Some(bytes.clone())),
            None => self.base.read_node(digest),
        }
    }
}

#[derive(Debug, Clone)]
struct WorldServiceCandidate {
    map_key: String,
    key_digest: String,
    result: crate::service_admission::CanonicalIntentResultV1,
    canonical_result_cbor: Vec<u8>,
    estimated_bytes: usize,
    hot_entry_bytes: usize,
}

#[derive(Debug, Clone)]
struct CapabilityNonceCandidate {
    nonce_key: String,
    value: RetainedCapabilityNonceV1,
    invocation_context_keys: Vec<String>,
    sort_key: (u64, u64, String),
    estimated_bytes: usize,
}

#[derive(Debug, Clone)]
struct AgentIntentCandidate {
    intent_id: String,
    value: RetainedAgentIntentV1,
    sort_key: (u64, u64, String),
    estimated_bytes: usize,
    hot_entry_bytes: usize,
}

#[cfg(test)]
#[path = "formal_retention_tests.rs"]
mod tests;

fn materialize_changed_updates(
    parent: &VerifiedFormalRetainedHistoryParentV1,
    reader: &impl BusinessIndexNodeReader,
    staged: BTreeMap<String, RetainedHistoryLeafV1>,
) -> Result<Vec<BusinessIndexLeafV1>, String> {
    let mut updates = Vec::new();
    for (key_digest, value) in staged {
        let previous = parent
            .read_leaf(&key_digest, reader)
            .map_err(|error| error.to_string())?;
        if previous.as_ref() == Some(&value) {
            continue;
        }
        updates.push(retained_leaf(key_digest, &value)?);
    }
    updates.sort_by(|left, right| left.key_digest.cmp(&right.key_digest));
    Ok(updates)
}

#[path = "formal_retention_planning.rs"]
mod planning;
#[path = "formal_retention_staging.rs"]
mod staging;
pub(crate) use planning::prepare_world_retention_candidate;
use staging::*;
