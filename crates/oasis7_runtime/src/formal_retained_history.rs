//! Verified access to the separately rooted, compacted formal runtime history.
//!
//! The index root is carried by the canonical Snapshot and is signed indirectly
//! by the qualified record's `after_state_root`. It is deliberately independent
//! of that snapshot root: retention leaves never contain the current snapshot
//! hash, so an execution can build a new retention root without a hash cycle.
//!
//! This module is a storage/query boundary only. It does not authorize a caller
//! to read an owner-scoped result; the owning service must apply its existing
//! caller and privacy checks before returning a decoded value.

use crate::service_admission::{CanonicalIntentResultV1, FeedbackAckIntentV1};
use crate::world_event::WorldEvent;
use crate::{
    AgentIntentV2, CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationNonceRecord,
    CapabilityInvocationContext,
};
use oasis7_distfs::controlled_authority::formal::business_index::{
    BusinessIndexLeafKindV1, BusinessIndexLeafV1, BusinessIndexNodeReader, BusinessIndexRootV1,
};
use serde::{
    Deserialize, Serialize,
    de::{Error as DeError, SeqAccess, Visitor},
};
use std::fmt;

pub(crate) const FORMAL_RETAINED_HISTORY_SCHEMA_V1: u32 = 1;
const FORMAL_RETAINED_HISTORY_ROOT_DOMAIN: &str = "oasis7.formal.retained-history.root.v1";
const FORMAL_RETAINED_HISTORY_KEY_DOMAIN: &str = "oasis7.formal.retained-history.key.v1";

/// Snapshot field committed by the qualified `after_state_root`.
///
/// Keep this descriptor separate from the business-result tree. A caller must
/// not substitute an untrusted digest for this value and treat it as an anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalRetainedHistoryRootV1 {
    pub schema_version: u32,
    pub index_root: BusinessIndexRootV1,
    pub root_digest: String,
}

impl FormalRetainedHistoryRootV1 {
    pub(crate) fn from_index_root(index_root: BusinessIndexRootV1) -> Result<Self, String> {
        index_root.validate()?;
        let mut value = Self {
            schema_version: FORMAL_RETAINED_HISTORY_SCHEMA_V1,
            index_root,
            root_digest: String::new(),
        };
        value.root_digest = value.compute_digest()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.schema_version != FORMAL_RETAINED_HISTORY_SCHEMA_V1 {
            return Err("unsupported formal retained-history root schema".into());
        }
        self.index_root.validate()?;
        if self.root_digest.len() != 64
            || !self
                .root_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || self.root_digest != self.compute_digest()?
        {
            return Err("formal retained-history root digest mismatch".into());
        }
        Ok(())
    }

    fn compute_digest(&self) -> Result<String, String> {
        let encoded = oasis7_wasm_abi::encode_canonical_cbor(&(
            FORMAL_RETAINED_HISTORY_ROOT_DOMAIN,
            self.schema_version,
            &self.index_root,
        ))
        .map_err(|error| error.to_string())?;
        Ok(blake3::hash(&encoded).to_hex().to_string())
    }
}

#[path = "formal_retained_history_snapshot.rs"]
mod snapshot_parent;
#[path = "formal_retained_history_tick.rs"]
mod tick_archive;
pub use snapshot_parent::VerifiedFormalRetainedHistoryParentV1;
pub(crate) use tick_archive::retained_tick_consensus_leaves;
#[path = "formal_retained_history_task_control.rs"]
mod task_control;
pub(crate) use task_control::{
    GameTaskControlLookupV1, game_task_result_lookup, game_task_result_matches,
};

/// Read-only view of a retained index root. A view can be built from a
/// qualified parent or from a delta that was recomputed against that parent;
/// it does not itself represent a formal anchor or authorize installation.
#[derive(Debug, Clone)]
pub(crate) struct FormalRetainedHistoryIndexViewV1 {
    root: FormalRetainedHistoryRootV1,
}

impl FormalRetainedHistoryIndexViewV1 {
    pub(crate) fn from_validated_root(root: FormalRetainedHistoryRootV1) -> Result<Self, String> {
        root.validate()?;
        Ok(Self { root })
    }

    pub(crate) fn root(&self) -> &FormalRetainedHistoryRootV1 {
        &self.root
    }
}

macro_rules! impl_retained_history_read_methods {
    ($target:ty) => {
        impl $target {
            pub(crate) fn read_world_service_result(
                &self,
                correlation_key_digest: &str,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<Option<CanonicalIntentResultV1>, RetainedHistoryReadError> {
                if !is_correlation_key_digest(correlation_key_digest) {
                    return Err(RetainedHistoryReadError::Invalid(
                        "WorldService correlation key digest is malformed".into(),
                    ));
                }
                let key = retained_key_digest("world-service-result", &correlation_key_digest)?;
                let entry = match self.read_leaf(&key, reader)? {
                    None => return Ok(None),
                    Some(RetainedHistoryLeafV1::WorldServiceResult(entry)) => entry,
                    Some(_) => {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained WorldService result leaf kind mismatch".into(),
                        ));
                    }
                };
                if entry.correlation_key_digest != correlation_key_digest {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained WorldService result key mismatch".into(),
                    ));
                }
                let result: CanonicalIntentResultV1 =
                    serde_cbor::from_slice(&entry.canonical_result_cbor)
                        .map_err(|error| RetainedHistoryReadError::Invalid(error.to_string()))?;
                let exact = serde_cbor::to_vec(&result)
                    .map_err(|error| RetainedHistoryReadError::Invalid(error.to_string()))?;
                if exact != entry.canonical_result_cbor
                    || blake3::hash(&exact).to_hex().to_string() != entry.canonical_result_digest
                {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained WorldService result bytes are not canonical".into(),
                    ));
                }
                let recomputed = crate::service_admission::correlation::key_digest(
                    &result.request.correlation.key,
                )
                .map_err(RetainedHistoryReadError::Invalid)?;
                if recomputed != correlation_key_digest {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained WorldService result correlation mismatch".into(),
                    ));
                }
                let as_json = serde_json::to_value(&result)
                    .map_err(|error| RetainedHistoryReadError::Invalid(error.to_string()))?;
                crate::service_admission::correlation::validate_result(
                    correlation_key_digest,
                    &as_json,
                )
                .map_err(RetainedHistoryReadError::Invalid)?;
                Ok(Some(result))
            }

            pub(crate) fn read_journal_event(
                &self,
                event_id_era: u64,
                event_id: u64,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<Option<WorldEvent>, RetainedHistoryReadError> {
                if event_id == 0 {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained journal event id is zero".into(),
                    ));
                }
                let key = retained_key_digest("world-journal-event", &(event_id_era, event_id))?;
                let entry = match self.read_leaf(&key, reader)? {
                    None => return Ok(None),
                    Some(RetainedHistoryLeafV1::JournalEvent(entry)) => entry,
                    Some(_) => {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained journal event leaf kind mismatch".into(),
                        ));
                    }
                };
                if entry.event_id_era != event_id_era || entry.event_id != event_id {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained journal event key mismatch".into(),
                    ));
                }
                entry
                    .decode()
                    .map(Some)
                    .map_err(RetainedHistoryReadError::Invalid)
            }

            /// Authenticated lookup used by owner chat's nonce fence. A missing CAS
            /// node is returned as `Unavailable`; it can never be mistaken for no prior
            /// nonce record.
            pub(crate) fn owner_chat_nonce_high_water(
                &self,
                public_key: &str,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<Option<u64>, RetainedHistoryReadError> {
                if public_key.trim().is_empty() {
                    return Err(RetainedHistoryReadError::Invalid(
                        "owner public key is empty".into(),
                    ));
                }
                let public_key_digest = digest_text("owner-public-key", public_key)
                    .map_err(RetainedHistoryReadError::Invalid)?;
                let key = retained_key_digest("owner-chat-nonce-high-water", &public_key_digest)?;
                let entry = match self.read_leaf(&key, reader)? {
                    None => return Ok(None),
                    Some(RetainedHistoryLeafV1::OwnerChatNonceHighWater(entry)) => entry,
                    Some(_) => {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained owner nonce index leaf kind mismatch".into(),
                        ));
                    }
                };
                if entry.owner_public_key_digest != public_key_digest {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained owner nonce key mismatch".into(),
                    ));
                }
                let witness = self
                    .read_world_service_result(&entry.witness_result_key_digest, reader)?
                    .ok_or_else(|| {
                        RetainedHistoryReadError::Invalid(
                            "retained owner nonce high-water witness is absent".into(),
                        )
                    })?;
                let witnessed_nonce = match &witness.request.signed_payload {
                    crate::service_admission::WorldServicePayloadV1::AgentChat(request) => {
                        if let Some(auth) = request.auth.as_ref()
                            && digest_text("owner-public-key", &auth.public_key)
                                .map_err(RetainedHistoryReadError::Invalid)?
                                == public_key_digest
                        {
                            Some(auth.nonce)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if witnessed_nonce != Some(entry.high_water_nonce) {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained owner nonce high-water witness mismatch".into(),
                    ));
                }
                Ok(Some(entry.high_water_nonce))
            }

            pub(crate) fn agent_chat_intent_results(
                &self,
                lookup: &AgentChatIntentLookupV1,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<Vec<CanonicalIntentResultV1>, RetainedHistoryReadError> {
                let lookup = lookup.canonicalized()?;
                let key = retained_key_digest("agent-chat-intent", &lookup)?;
                let entry = match self.read_leaf(&key, reader)? {
                    None => return Ok(Vec::new()),
                    Some(RetainedHistoryLeafV1::AgentChatIntentIndex(entry)) => entry,
                    Some(_) => {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained AgentChat index leaf kind mismatch".into(),
                        ));
                    }
                };
                validate_result_refs(&entry.result_key_digests)?;
                let mut results = Vec::with_capacity(entry.result_key_digests.len());
                for result_key_digest in entry.result_key_digests {
                    let result = self
                        .read_world_service_result(&result_key_digest, reader)?
                        .ok_or_else(|| {
                            RetainedHistoryReadError::Invalid(
                                "retained chat index points to an absent result".into(),
                            )
                        })?;
                    if !chat_result_matches(&result, &lookup)? {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained chat index points to a nonmatching result".into(),
                        ));
                    }
                    results.push(result);
                }
                Ok(results)
            }

            pub(crate) fn feedback_publication_exists(
                &self,
                lookup: &FeedbackPublicationLookupV1,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<bool, RetainedHistoryReadError> {
                lookup.validate()?;
                let key = retained_key_digest("feedback-publication", lookup)?;
                let entry = match self.read_leaf(&key, reader)? {
                    None => return Ok(false),
                    Some(RetainedHistoryLeafV1::FeedbackPublicationIndex(entry)) => entry,
                    Some(_) => {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained feedback index leaf kind mismatch".into(),
                        ));
                    }
                };
                if entry.result_key_digests.is_empty() || entry.result_key_digests.len() > 1 {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained feedback index cardinality".into(),
                    ));
                }
                let result = self
                    .read_world_service_result(&entry.result_key_digests[0], reader)?
                    .ok_or_else(|| {
                        RetainedHistoryReadError::Invalid(
                            "retained feedback index points to an absent result".into(),
                        )
                    })?;
                if !feedback_result_matches(&result, lookup) {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained feedback index points to a nonmatching result".into(),
                    ));
                }
                Ok(true)
            }

            pub(crate) fn game_task_control_results(
                &self,
                lookup: &GameTaskControlLookupV1,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<Vec<CanonicalIntentResultV1>, RetainedHistoryReadError> {
                let lookup = lookup.canonicalized()?;
                // The old two-reference create/ambiguity index stays readable;
                // complete revision chains use a distinct namespace and leaf schema.
                let old_key = retained_key_digest("game-task-control", &lookup)?;
                let mut results = match self.read_leaf(&old_key, reader)? {
                    None => Vec::new(),
                    Some(RetainedHistoryLeafV1::GameTaskControlIndex(entry)) => {
                        validate_result_refs(&entry.result_key_digests)?;
                        let mut legacy = Vec::new();
                        for reference in entry.result_key_digests {
                            let result = self
                                .read_world_service_result(&reference, reader)?
                                .ok_or_else(|| {
                                    RetainedHistoryReadError::Invalid(
                                        "legacy task index result absent".into(),
                                    )
                                })?;
                            if !game_task_result_matches(&result, &lookup)? {
                                return Err(RetainedHistoryReadError::Invalid(
                                    "legacy task index scope mismatch".into(),
                                ));
                            }
                            legacy.push(result);
                        }
                        legacy
                    }
                    Some(_) => {
                        return Err(RetainedHistoryReadError::Invalid(
                            "legacy task index leaf type mismatch".into(),
                        ));
                    }
                };
                let key = retained_key_digest("game-task-control-chain-v1", &lookup)?;
                let entry = match self.read_leaf(&key, reader)? {
                    None => return Ok(results),
                    Some(RetainedHistoryLeafV1::GameTaskControlChainIndex(entry)) => entry,
                    Some(_) => {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained task chain head type mismatch".into(),
                        ));
                    }
                };
                if entry.count == 0 {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained task chain is empty".into(),
                    ));
                }
                if entry.count > 4096 {
                    return Err(RetainedHistoryReadError::Unavailable(
                        "retained task chain exceeds query budget".into(),
                    ));
                }
                let mut chain_count = 0usize;
                let mut cursor = Some(entry.head);
                let mut seen = std::collections::BTreeSet::new();
                while let Some(reference) = cursor {
                    if chain_count >= entry.count as usize
                        || !seen.insert(reference.clone())
                        || !is_correlation_key_digest(&reference)
                    {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained task chain count/cycle/reference mismatch".into(),
                        ));
                    }
                    let key = retained_key_digest("game-task-control-link", &reference)?;
                    let Some(RetainedHistoryLeafV1::GameTaskControlLink(link)) =
                        self.read_leaf(&key, reader)?
                    else {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained task chain link absent".into(),
                        ));
                    };
                    if link.result_key_digest != reference {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained task chain link identity mismatch".into(),
                        ));
                    }
                    let result = self
                        .read_world_service_result(&reference, reader)?
                        .ok_or_else(|| {
                            RetainedHistoryReadError::Invalid(
                                "retained task chain result absent".into(),
                            )
                        })?;
                    if !game_task_result_matches(&result, &lookup)? {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained task chain result scope mismatch".into(),
                        ));
                    }
                    cursor = link.previous;
                    results.push(result);
                    chain_count += 1;
                }
                if chain_count != entry.count as usize {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained task chain incomplete".into(),
                    ));
                }
                Ok(results)
            }

            pub(crate) fn capability_nonce_record(
                &self,
                nonce_key: &str,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<Option<RetainedCapabilityNonceV1>, RetainedHistoryReadError> {
                if !is_digest(nonce_key) {
                    return Err(RetainedHistoryReadError::Invalid(
                        "capability nonce key is malformed".into(),
                    ));
                }
                let key = retained_key_digest("capability-nonce", &nonce_key)?;
                let entry = match self.read_leaf(&key, reader)? {
                    None => return Ok(None),
                    Some(RetainedHistoryLeafV1::CapabilityNonce(entry)) => entry,
                    Some(_) => {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained capability nonce leaf kind mismatch".into(),
                        ));
                    }
                };
                if entry.nonce_key != nonce_key {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained capability nonce key mismatch".into(),
                    ));
                }
                entry
                    .validate()
                    .map_err(RetainedHistoryReadError::Invalid)?;
                Ok(Some(entry))
            }

            /// Restore the exact invocation context used by a previously committed
            /// command. Preserve the original lookup precedence: the canonical
            /// `(grant_id, response_nonce)` entry wins, followed by the legacy
            /// grant-id alias.
            pub(crate) fn capability_invocation_context_for_nonce(
                &self,
                nonce_key: &str,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<Option<CapabilityInvocationContext>, RetainedHistoryReadError> {
                let Some(entry) = self.capability_nonce_record(nonce_key, reader)? else {
                    return Ok(None);
                };
                let Some(grant_id) = entry.receipt.grant_id.as_deref() else {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained capability nonce lacks grant id".into(),
                    ));
                };
                let Some(response_nonce) = entry.receipt.response_nonce.as_deref() else {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained capability nonce lacks response nonce".into(),
                    ));
                };
                let canonical_key = capability_invocation_context_key(grant_id, response_nonce)
                    .map_err(RetainedHistoryReadError::Invalid)?;
                let selected = entry
                    .invocation_contexts
                    .iter()
                    .find(|stored| stored.map_key == canonical_key)
                    .or_else(|| {
                        entry
                            .invocation_contexts
                            .iter()
                            .find(|stored| stored.map_key == grant_id)
                    });
                Ok(selected.map(|stored| stored.context.clone()))
            }

            pub(crate) fn agent_intent(
                &self,
                intent_id: &str,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<Option<RetainedAgentIntentV1>, RetainedHistoryReadError> {
                if intent_id.trim().is_empty() {
                    return Err(RetainedHistoryReadError::Invalid(
                        "AgentIntent id is empty".into(),
                    ));
                }
                let key = retained_key_digest("agent-intent", &intent_id)?;
                let entry = match self.read_leaf(&key, reader)? {
                    None => return Ok(None),
                    Some(RetainedHistoryLeafV1::AgentIntent(entry)) => entry,
                    Some(_) => {
                        return Err(RetainedHistoryReadError::Invalid(
                            "retained AgentIntent leaf kind mismatch".into(),
                        ));
                    }
                };
                if entry.intent.intent_id != intent_id {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained AgentIntent key mismatch".into(),
                    ));
                }
                entry
                    .validate()
                    .map_err(RetainedHistoryReadError::Invalid)?;
                Ok(Some(entry))
            }

            pub(crate) fn archived_reorg_epoch_summary(
                &self,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<DistinctReorgEpochSummaryV1, RetainedHistoryReadError> {
                let key = retained_key_digest("agent-intent-reorg-epochs", &())?;
                match self.read_leaf(&key, reader)? {
                    Some(RetainedHistoryLeafV1::AgentIntentReorgEpochs(summary)) => Ok(summary),
                    Some(_) => Err(RetainedHistoryReadError::Invalid(
                        "retained AgentIntent epoch summary kind mismatch".into(),
                    )),
                    None => Ok(DistinctReorgEpochSummaryV1::Empty),
                }
            }

            pub(crate) fn read_leaf(
                &self,
                key_digest: &str,
                reader: &impl BusinessIndexNodeReader,
            ) -> Result<Option<RetainedHistoryLeafV1>, RetainedHistoryReadError> {
                let leaf = self
                    .root
                    .index_root
                    .get(key_digest, reader)
                    .map_err(RetainedHistoryReadError::Unavailable)?;
                let Some(leaf) = leaf else {
                    // BusinessIndexRoot::get traverses and authenticates the complete
                    // sparse path before returning absence.
                    return Ok(None);
                };
                if leaf.key_digest != key_digest {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained-history leaf key mismatch".into(),
                    ));
                }
                let decoded: RetainedHistoryLeafV1 = serde_cbor::from_slice(&leaf.canonical_value)
                    .map_err(|error| RetainedHistoryReadError::Invalid(error.to_string()))?;
                if leaf.kind != retained_leaf_kind(&decoded) {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained-history leaf kind does not match its value type".into(),
                    ));
                }
                let encoded =
                    encode_retained_leaf(&decoded).map_err(RetainedHistoryReadError::Invalid)?;
                if encoded != leaf.canonical_value {
                    return Err(RetainedHistoryReadError::Invalid(
                        "retained-history leaf is not canonical".into(),
                    ));
                }
                Ok(Some(decoded))
            }
        }
    };
}

impl VerifiedFormalRetainedHistoryParentV1 {
    /// Planner reads are rooted in the qualified parent, while general query
    /// methods are exposed only by the explicit index view.
    pub(crate) fn read_leaf(
        &self,
        key_digest: &str,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<Option<RetainedHistoryLeafV1>, RetainedHistoryReadError> {
        self.index_view().read_leaf(key_digest, reader)
    }
}

impl_retained_history_read_methods!(FormalRetainedHistoryIndexViewV1);

/// Cardinality summary for the one remaining historical AgentIntent query.
/// The legacy behavior only needs zero/one/multiple, so archived epochs do not
/// need an unbounded set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "epoch", rename_all = "snake_case")]
pub(crate) enum DistinctReorgEpochSummaryV1 {
    Empty,
    Single(u64),
    Multiple { max_epoch: u64 },
}

impl DistinctReorgEpochSummaryV1 {
    pub(crate) fn merge_hot<'a>(
        current: &Self,
        epochs: impl IntoIterator<Item = &'a Option<u64>>,
    ) -> Self {
        let mut next = current.clone();
        for epoch in epochs.into_iter().flatten() {
            next = match next {
                Self::Empty => Self::Single(*epoch),
                Self::Single(existing) if existing == *epoch => Self::Single(existing),
                Self::Single(existing) => Self::Multiple {
                    max_epoch: existing.max(*epoch),
                },
                Self::Multiple { max_epoch } => Self::Multiple {
                    max_epoch: max_epoch.max(*epoch),
                },
            };
        }
        next
    }

    #[cfg(test)]
    pub(crate) fn from_hot<'a>(epochs: impl IntoIterator<Item = &'a Option<u64>>) -> Self {
        Self::merge_hot(&Self::Empty, epochs)
    }

    pub(crate) fn has_multiple(&self) -> bool {
        matches!(self, Self::Multiple { .. })
    }

    pub(crate) fn single_epoch(&self) -> Option<u64> {
        match self {
            Self::Single(epoch) => Some(*epoch),
            Self::Empty | Self::Multiple { .. } => None,
        }
    }

    pub(crate) fn max_epoch(&self) -> Option<u64> {
        match self {
            Self::Empty => None,
            Self::Single(epoch) => Some(*epoch),
            Self::Multiple { max_epoch } => Some(*max_epoch),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetainedHistoryReadError {
    /// The authenticated node path or a referenced CAS node cannot be read.
    /// Callers must not treat this as a missing result.
    Unavailable(String),
    /// The root authenticated bytes with a malformed schema or mismatch.
    Invalid(String),
}

impl fmt::Display for RetainedHistoryReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(f, "retained history unavailable: {reason}"),
            Self::Invalid(reason) => write!(f, "invalid retained history: {reason}"),
        }
    }
}

impl std::error::Error for RetainedHistoryReadError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentChatIntentLookupV1 {
    pub agent_id: String,
    pub intent_id: String,
    pub owner_public_key_digest: String,
    pub player_id: String,
    pub world_id: String,
    pub branch_id: String,
    pub identity_generation: u64,
    pub reorg_epoch: u64,
    pub authority_scope: String,
}

impl AgentChatIntentLookupV1 {
    fn canonicalized(&self) -> Result<Self, RetainedHistoryReadError> {
        if [
            &self.agent_id,
            &self.intent_id,
            &self.owner_public_key_digest,
            &self.player_id,
            &self.world_id,
            &self.branch_id,
            &self.authority_scope,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
            || self.identity_generation == 0
            || !is_digest(&self.owner_public_key_digest)
        {
            return Err(RetainedHistoryReadError::Invalid(
                "AgentChat retained lookup identity".into(),
            ));
        }
        Ok(self.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeedbackPublicationLookupV1 {
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
    pub feedback_id: String,
    pub runtime_receipt_id: Option<String>,
}

impl From<&FeedbackAckIntentV1> for FeedbackPublicationLookupV1 {
    fn from(value: &FeedbackAckIntentV1) -> Self {
        Self {
            agent_id: value.agent_id.clone(),
            agent_session_id: value.agent_session_id.clone(),
            agent_turn_id: value.agent_turn_id.clone(),
            decision_request_id: value.decision_request_id.clone(),
            request_digest: value.request_digest.clone(),
            feedback_id: value.feedback_id.clone(),
            runtime_receipt_id: value.runtime_receipt_id.clone(),
        }
    }
}

impl FeedbackPublicationLookupV1 {
    fn validate(&self) -> Result<(), RetainedHistoryReadError> {
        if [
            &self.agent_id,
            &self.agent_session_id,
            &self.agent_turn_id,
            &self.decision_request_id,
            &self.feedback_id,
        ]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 256)
            || !is_correlation_key_digest(&self.request_digest)
            || self
                .runtime_receipt_id
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 256)
        {
            return Err(RetainedHistoryReadError::Invalid(
                "feedback publication retained lookup identity".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedCapabilityNonceV1 {
    pub nonce_key: String,
    pub record: CapabilityAuthorizationNonceRecord,
    pub receipt: CapabilityAuthorizationAuditReceipt,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invocation_contexts: Vec<RetainedCapabilityInvocationContextV1>,
}

impl RetainedCapabilityNonceV1 {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let receipt_id = self
            .record
            .committed_receipt_id
            .as_deref()
            .ok_or("retained capability nonce lacks committed receipt id")?;
        if self.nonce_key.trim().is_empty()
            || !is_digest(&self.nonce_key)
            || self.record.state != "committed"
            || self.receipt.receipt_id != receipt_id
            || !matches!(self.receipt.decision.as_str(), "accepted" | "denied")
            || self
                .receipt
                .world_head_after
                .is_none_or(|head| head <= self.receipt.world_head_before)
            || self.receipt.authorization_nonce_key_hash.as_deref() != Some(self.nonce_key.as_str())
            || self.record.request_hash != self.receipt.canonical_request_hash
            || self.record.outcome_hash != self.receipt.canonical_result_hash
        {
            return Err("retained capability nonce/receipt mismatch".into());
        }
        let (Some(grant_id), Some(response_nonce)) = (
            self.receipt.grant_id.as_deref(),
            self.receipt.response_nonce.as_deref(),
        ) else {
            if self.invocation_contexts.is_empty() {
                return Ok(());
            }
            return Err("retained capability nonce context has no receipt identity".into());
        };
        let canonical_context_key = capability_invocation_context_key(grant_id, response_nonce)?;
        let receipt_requires_context = self.receipt.presenter.is_some()
            && self.receipt.module_id.is_some()
            && self.receipt.module_version.is_some()
            && self.receipt.catalog_snapshot_id.is_some();
        if receipt_requires_context && self.invocation_contexts.is_empty() {
            return Err("retained capability nonce omitted its invocation context".into());
        }
        let mut seen_context_keys = std::collections::BTreeSet::new();
        for stored in &self.invocation_contexts {
            if !seen_context_keys.insert(stored.map_key.as_str())
                || stored.context.grant_id != grant_id
                || stored.context.response_nonce != response_nonce
                || (stored.map_key != canonical_context_key && stored.map_key != grant_id)
                || !invocation_context_matches_receipt(&stored.context, &self.receipt)?
            {
                return Err("retained capability invocation context mismatch".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedCapabilityInvocationContextV1 {
    pub map_key: String,
    pub context: CapabilityInvocationContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedAgentIntentV1 {
    pub intent: AgentIntentV2,
    pub replay_identity_digest: String,
}

impl RetainedAgentIntentV1 {
    pub(crate) fn new(intent: AgentIntentV2) -> Result<Self, String> {
        let replay_identity_digest = agent_intent_replay_identity_digest(&intent)?;
        let value = Self {
            intent,
            replay_identity_digest,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if !is_terminal_agent_intent(&self.intent.status)
            || self.intent.intent_id.trim().is_empty()
            || self.replay_identity_digest != agent_intent_replay_identity_digest(&self.intent)?
        {
            return Err("retained AgentIntent terminal replay identity mismatch".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedWorldServiceResultV1 {
    pub correlation_key_digest: String,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    pub canonical_result_cbor: Vec<u8>,
    pub canonical_result_digest: String,
}

#[path = "formal_retained_history_journal.rs"]
mod journal_history;
pub(crate) use journal_history::{
    RetainedJournalEventV1, RetainedJournalTickLocatorV1, retained_tick_journal_leaf,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedOwnerChatNonceHighWaterV1 {
    pub owner_public_key_digest: String,
    pub high_water_nonce: u64,
    pub witness_result_key_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedResultRefsV1 {
    /// Two authenticated references suffice to preserve the existing
    /// zero/one/ambiguous result semantics without a growing hot-world list.
    pub result_key_digests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedTaskChainV1 {
    pub head: String,
    pub count: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedTaskChainLinkV1 {
    pub result_key_digest: String,
    pub previous: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::large_enum_variant,
    reason = "Preserve the typed retained-leaf schema and APIs; archive IO owns only one decoded leaf at a time."
)]
pub(crate) enum RetainedHistoryLeafV1 {
    WorldServiceResult(RetainedWorldServiceResultV1),
    JournalEvent(RetainedJournalEventV1),
    JournalTickLocator(RetainedJournalTickLocatorV1),
    TickConsensusRecord(tick_archive::RetainedTickConsensusRecordV1),
    TickConsensusHeight(tick_archive::RetainedTickConsensusHeightV1),
    OwnerChatNonceHighWater(RetainedOwnerChatNonceHighWaterV1),
    AgentChatIntentIndex(RetainedResultRefsV1),
    FeedbackPublicationIndex(RetainedResultRefsV1),
    GameTaskControlIndex(RetainedResultRefsV1),
    GameTaskControlChainIndex(RetainedTaskChainV1),
    GameTaskControlLink(RetainedTaskChainLinkV1),
    CapabilityNonce(RetainedCapabilityNonceV1),
    AgentIntent(RetainedAgentIntentV1),
    AgentIntentReorgEpochs(DistinctReorgEpochSummaryV1),
}

#[derive(Serialize)]
struct CapabilityInvocationContextKeyV1<'a> {
    grant_id: &'a str,
    response_nonce: &'a str,
}

pub(crate) fn capability_invocation_context_key(
    grant_id: &str,
    response_nonce: &str,
) -> Result<String, String> {
    if grant_id.trim().is_empty() || response_nonce.trim().is_empty() {
        return Err("retained capability invocation context identity is empty".into());
    }
    oasis7_wasm_abi::canonical_hash(&CapabilityInvocationContextKeyV1 {
        grant_id,
        response_nonce,
    })
    .map_err(|error| error.to_string())
}

pub(crate) fn invocation_context_matches_receipt(
    context: &CapabilityInvocationContext,
    receipt: &CapabilityAuthorizationAuditReceipt,
) -> Result<bool, String> {
    Ok(
        serde_json::to_value(&context.subject).map_err(|error| error.to_string())?
            == receipt.subject
            && serde_json::to_value(&context.presenter).map_err(|error| error.to_string())?
                == receipt.presenter.clone().unwrap_or_default()
            && serde_json::to_value(&context.audience).map_err(|error| error.to_string())?
                == receipt.audience
            && Some(context.module_id.as_str()) == receipt.module_id.as_deref()
            && Some(context.module_version.as_str()) == receipt.module_version.as_deref()
            && Some(context.catalog_snapshot_id.as_str()) == receipt.catalog_snapshot_id.as_deref(),
    )
}

pub(crate) fn retained_key_digest<T: Serialize>(
    namespace: &str,
    key: &T,
) -> Result<String, RetainedHistoryReadError> {
    let encoded = oasis7_wasm_abi::encode_canonical_cbor(&(
        FORMAL_RETAINED_HISTORY_KEY_DOMAIN,
        namespace,
        key,
    ))
    .map_err(|error| RetainedHistoryReadError::Invalid(error.to_string()))?;
    Ok(blake3::hash(&encoded).to_hex().to_string())
}

pub(crate) fn encode_retained_leaf(value: &RetainedHistoryLeafV1) -> Result<Vec<u8>, String> {
    oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|error| error.to_string())
}

pub(crate) fn retained_leaf(
    key_digest: String,
    value: &RetainedHistoryLeafV1,
) -> Result<BusinessIndexLeafV1, String> {
    Ok(BusinessIndexLeafV1 {
        key_digest,
        kind: retained_leaf_kind(value),
        canonical_value: encode_retained_leaf(value)?,
    })
}

fn retained_leaf_kind(value: &RetainedHistoryLeafV1) -> BusinessIndexLeafKindV1 {
    match value {
        RetainedHistoryLeafV1::WorldServiceResult(_)
        | RetainedHistoryLeafV1::JournalEvent(_)
        | RetainedHistoryLeafV1::JournalTickLocator(_)
        | RetainedHistoryLeafV1::TickConsensusRecord(_)
        | RetainedHistoryLeafV1::TickConsensusHeight(_)
        | RetainedHistoryLeafV1::CapabilityNonce(_)
        | RetainedHistoryLeafV1::AgentIntent(_) => BusinessIndexLeafKindV1::Outcome,
        RetainedHistoryLeafV1::OwnerChatNonceHighWater(_)
        | RetainedHistoryLeafV1::AgentChatIntentIndex(_)
        | RetainedHistoryLeafV1::FeedbackPublicationIndex(_)
        | RetainedHistoryLeafV1::GameTaskControlIndex(_)
        | RetainedHistoryLeafV1::GameTaskControlChainIndex(_)
        | RetainedHistoryLeafV1::GameTaskControlLink(_)
        | RetainedHistoryLeafV1::AgentIntentReorgEpochs(_) => {
            BusinessIndexLeafKindV1::CommitMetadata
        }
    }
}

pub(crate) fn digest_text(namespace: &str, value: &str) -> Result<String, String> {
    let encoded = oasis7_wasm_abi::encode_canonical_cbor(&(namespace, value))
        .map_err(|error| error.to_string())?;
    Ok(blake3::hash(&encoded).to_hex().to_string())
}

pub(crate) fn agent_intent_replay_identity_digest(
    intent: &AgentIntentV2,
) -> Result<String, String> {
    let encoded = oasis7_wasm_abi::encode_canonical_cbor(&(
        "oasis7.agent-intent.retained-replay-identity.v1",
        &intent.actor_id,
        &intent.agent_id,
        &intent.intent_id,
        &intent.kind,
        &intent.source,
        &intent.request_digest,
        intent.intent_tick,
        &intent.world_id,
        intent.reorg_epoch,
        &intent.authority_scope,
    ))
    .map_err(|error| error.to_string())?;
    Ok(blake3::hash(&encoded).to_hex().to_string())
}

pub(crate) fn is_terminal_agent_intent(status: &str) -> bool {
    matches!(
        status,
        "completed" | "rejected" | "expired" | "cancelled" | "superseded"
    )
}

pub(crate) fn validate_result_refs(refs: &[String]) -> Result<(), RetainedHistoryReadError> {
    if refs.is_empty()
        || refs.len() > 2
        || refs.windows(2).any(|pair| pair[0] >= pair[1])
        || refs.iter().any(|value| !is_correlation_key_digest(value))
    {
        return Err(RetainedHistoryReadError::Invalid(
            "retained result index references are malformed".into(),
        ));
    }
    Ok(())
}

pub(crate) fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn is_correlation_key_digest(value: &str) -> bool {
    value.strip_prefix("blake3:").is_some_and(|digest| {
        is_digest(digest) && digest.bytes().all(|byte| !byte.is_ascii_uppercase())
    })
}

pub(crate) fn chat_result_matches(
    result: &CanonicalIntentResultV1,
    lookup: &AgentChatIntentLookupV1,
) -> Result<bool, RetainedHistoryReadError> {
    if result.rejected.is_some()
        || result
            .receipt
            .get("intent_id")
            .and_then(serde_json::Value::as_str)
            != Some(lookup.intent_id.as_str())
    {
        return Ok(false);
    }
    let crate::service_admission::WorldServicePayloadV1::AgentChat(request) =
        &result.request.signed_payload
    else {
        return Ok(false);
    };
    let Some(auth) = request.auth.as_ref() else {
        return Ok(false);
    };
    let Some(authority) = request.canonical_authority.as_ref() else {
        return Ok(false);
    };
    let public_key_digest = digest_text("owner-public-key", &auth.public_key)
        .map_err(RetainedHistoryReadError::Invalid)?;
    Ok(request.agent_id == lookup.agent_id
        && public_key_digest == lookup.owner_public_key_digest
        && auth.player_id == lookup.player_id
        && request.world_id.as_deref() == Some(lookup.world_id.as_str())
        && request.reorg_epoch == Some(lookup.reorg_epoch)
        && request.authority_scope.as_deref() == Some(lookup.authority_scope.as_str())
        && authority.branch_id == lookup.branch_id
        && authority.agent_identity_generation == lookup.identity_generation)
}

pub(crate) fn feedback_result_matches(
    result: &CanonicalIntentResultV1,
    lookup: &FeedbackPublicationLookupV1,
) -> bool {
    result.rejected.is_none()
        && matches!(&result.request.signed_payload,
            crate::service_admission::WorldServicePayloadV1::Cognition(signed)
                if signed.request.request.agent_id == lookup.agent_id
                    && signed.request.request.agent_session_id == lookup.agent_session_id
                    && signed.request.request.agent_turn_id == lookup.agent_turn_id
                    && signed.request.request.decision_request_id == lookup.decision_request_id
                    && signed.request.request.request_digest == lookup.request_digest)
        && result
            .receipt
            .get("lineage")
            .and_then(|value| value.get("feedback_id"))
            .and_then(serde_json::Value::as_str)
            == Some(lookup.feedback_id.as_str())
        && result
            .receipt
            .get("lineage")
            .and_then(|value| value.get("receipt_id"))
            .and_then(serde_json::Value::as_str)
            == lookup.runtime_receipt_id.as_deref()
}

fn serialize_bytes<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_bytes(value)
}

fn deserialize_bytes<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BytesVisitor;
    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = Vec<u8>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a byte string")
        }
        fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(value.to_vec())
        }
        fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(value)
        }
        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut value = Vec::new();
            while let Some(byte) = sequence.next_element()? {
                value.push(byte);
            }
            Ok(value)
        }
    }
    deserializer.deserialize_bytes(BytesVisitor)
}

#[cfg(test)]
#[path = "formal_retained_history_tests.rs"]
mod tests;
