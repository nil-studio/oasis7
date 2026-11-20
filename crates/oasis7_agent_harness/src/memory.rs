//! Bounded private-memory and goal projection algorithms.
//!
//! Provider memory intents are proposals. Durable memory writes and correction
//! finalization require a Runtime host callback that proves the receipt
//! readback; receipt DTOs alone are not authority.

use std::collections::BTreeMap;

use oasis7_agent_api::{
    CognitionError, ContinuousAgentTurnContextV1, Digest32, GoalSnapshotV1, MemoryContextEntryV1,
    MemoryContextSnapshotV1, MemoryWriteIntentV1, RuntimeReceiptLineageV1, h_v1,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

use crate::RuntimeAuthority;

const MEMORY_INTENT_DOMAIN: &str = "oasis7.cognition.memory-write-intent.v1";
const MAX_MEMORY_INTENTS: usize = 8;
const MAX_MEMORY_SUMMARY_BYTES: usize = 512;
const MAX_MEMORY_TAGS: usize = 8;
const MAX_MEMORY_TAG_BYTES: usize = 64;
const MAX_MEMORY_PAYLOAD_BYTES: usize = 4096;
const MAX_GOAL_SUMMARY_BYTES: usize = 512;
const MAX_BLOCKED_REASON_BYTES: usize = 256;

fn error(code: &'static str, message: impl Into<String>) -> CognitionError {
    CognitionError::new(code, message)
}

fn normalized_text(
    value: &str,
    max_bytes: usize,
    too_large: &'static str,
    invalid: &'static str,
) -> Result<String, CognitionError> {
    let normalized: String = value.nfc().collect::<String>().trim().to_string();
    if normalized.len() > max_bytes {
        return Err(error(
            too_large,
            "bounded cognition text exceeds its byte limit",
        ));
    }
    if normalized.chars().any(char::is_control) {
        return Err(error(
            invalid,
            "cognition text contains a control character",
        ));
    }
    Ok(normalized)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWritePolicyContextV1 {
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub request_digest: String,
    pub source: String,
    pub provenance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedMemoryWriteIntentV1 {
    pub schema_version: u16,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_reason: Option<String>,
}

impl From<NormalizedMemoryWriteIntentV1> for MemoryWriteIntentV1 {
    fn from(value: NormalizedMemoryWriteIntentV1) -> Self {
        Self {
            schema_version: value.schema_version,
            scope: value.scope,
            summary: value.summary,
            tags: value.tags,
            compatibility_reason: value.compatibility_reason,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteIntentPolicyV1 {
    pub max_intents_per_turn: usize,
    pub max_summary_bytes: usize,
    pub max_tags_per_intent: usize,
    pub max_tag_bytes: usize,
    pub max_payload_bytes: usize,
}

impl Default for MemoryWriteIntentPolicyV1 {
    fn default() -> Self {
        Self {
            max_intents_per_turn: MAX_MEMORY_INTENTS,
            max_summary_bytes: MAX_MEMORY_SUMMARY_BYTES,
            max_tags_per_intent: MAX_MEMORY_TAGS,
            max_tag_bytes: MAX_MEMORY_TAG_BYTES,
            max_payload_bytes: MAX_MEMORY_PAYLOAD_BYTES,
        }
    }
}

impl MemoryWriteIntentPolicyV1 {
    fn check_identity(context: &MemoryWritePolicyContextV1) -> Result<(), CognitionError> {
        if context.agent_id.trim().is_empty()
            || context.agent_session_id.trim().is_empty()
            || context.agent_turn_id.trim().is_empty()
            || context.request_digest.trim().is_empty()
        {
            return Err(error(
                "memory_source_mismatch",
                "memory intent identity is incomplete",
            ));
        }
        Ok(())
    }

    fn check_context(context: &MemoryWritePolicyContextV1) -> Result<(), CognitionError> {
        Self::check_identity(context)?;
        if !matches!(context.source.as_str(), "provider" | "harness" | "builtin") {
            return Err(error(
                "memory_source_mismatch",
                "memory intent source is not a recognized Harness source",
            ));
        }
        if context.source == "provider" && context.provenance != "provider_unverified" {
            return Err(error(
                "memory_source_mismatch",
                "provider cannot self-assign authoritative memory provenance",
            ));
        }
        Ok(())
    }

    fn normalize_inner(
        &self,
        intent: MemoryWriteIntentV1,
        context: &MemoryWritePolicyContextV1,
    ) -> Result<NormalizedMemoryWriteIntentV1, CognitionError> {
        Self::check_context(context)?;
        if intent.schema_version != 1 {
            return Err(error(
                "memory_source_mismatch",
                "unsupported memory intent schema version",
            ));
        }
        if !matches!(intent.scope.as_str(), "turn_private" | "session_private") {
            return Err(error(
                "memory_scope_denied",
                "target lane permits only turn_private and session_private memory",
            ));
        }
        let summary = intent
            .summary
            .map(|value| {
                let value = normalized_text(
                    &value,
                    self.max_summary_bytes,
                    "memory_summary_too_large",
                    "memory_summary_invalid",
                )?;
                if value.is_empty() {
                    return Err(error("memory_summary_invalid", "summary must not be empty"));
                }
                Ok(value)
            })
            .transpose()?;
        if intent.tags.len() > self.max_tags_per_intent {
            return Err(error(
                "memory_tag_count_exceeded",
                "memory intent has too many tags",
            ));
        }
        let mut tags = Vec::with_capacity(intent.tags.len());
        for raw in intent.tags {
            let tag = normalized_text(
                &raw,
                self.max_tag_bytes,
                "memory_tag_invalid",
                "memory_tag_invalid",
            )?;
            if tag.is_empty() {
                return Err(error(
                    "memory_tag_invalid",
                    "tags must not contain empty values",
                ));
            }
            tags.push(tag);
        }
        tags.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        tags.dedup();
        Ok(NormalizedMemoryWriteIntentV1 {
            schema_version: 1,
            scope: intent.scope,
            summary,
            tags,
            compatibility_reason: intent.compatibility_reason,
        })
    }

    pub fn normalize(
        &self,
        intent: impl Into<MemoryWriteIntentV1>,
        context: &MemoryWritePolicyContextV1,
    ) -> Result<NormalizedMemoryWriteIntentV1, CognitionError> {
        let normalized = self.normalize_inner(intent.into(), context)?;
        if self.canonical_intent_bytes(&normalized).len() > self.max_payload_bytes {
            return Err(error(
                "memory_payload_too_large",
                "memory intent exceeds the canonical payload bound",
            ));
        }
        Ok(normalized)
    }

    pub fn normalize_legacy(
        &self,
        intent: impl Into<MemoryWriteIntentV1>,
        context: &MemoryWritePolicyContextV1,
    ) -> Result<NormalizedMemoryWriteIntentV1, CognitionError> {
        let mut intent = intent.into();
        if intent.scope != "short_term" {
            return self.normalize(intent, context);
        }
        intent.scope = "turn_private".to_string();
        let mut normalized = self.normalize_inner(intent, context)?;
        normalized.compatibility_reason = Some("memory_scope_alias_used".to_string());
        if self.canonical_intent_bytes(&normalized).len() > self.max_payload_bytes {
            return Err(error(
                "memory_payload_too_large",
                "memory intent exceeds the canonical payload bound",
            ));
        }
        Ok(normalized)
    }

    pub fn normalize_batch<I: Into<MemoryWriteIntentV1>>(
        &self,
        intents: Vec<I>,
        context: &MemoryWritePolicyContextV1,
    ) -> Result<Vec<NormalizedMemoryWriteIntentV1>, CognitionError> {
        if intents.len() > self.max_intents_per_turn {
            return Err(error(
                "memory_intent_count_exceeded",
                "turn contains too many memory intents",
            ));
        }
        let normalized = intents
            .into_iter()
            .map(|intent| self.normalize(intent.into(), context))
            .collect::<Result<Vec<_>, _>>()?;
        let total = normalized
            .iter()
            .map(|intent| self.canonical_intent_bytes(intent).len())
            .sum::<usize>();
        if total > self.max_payload_bytes {
            return Err(error(
                "memory_payload_too_large",
                "turn memory intents exceed the canonical payload bound",
            ));
        }
        Ok(normalized)
    }

    fn canonical_intent_value(&self, intent: &NormalizedMemoryWriteIntentV1) -> Value {
        json!({
            "schema_version": intent.schema_version,
            "scope": intent.scope,
            "summary_present": intent.summary.is_some(),
            "summary": intent.summary,
            "tags": intent.tags,
            "compatibility_reason": intent.compatibility_reason,
        })
    }

    fn canonical_intent_bytes(&self, intent: &NormalizedMemoryWriteIntentV1) -> Vec<u8> {
        oasis7_wasm_abi::encode_canonical_cbor(&self.canonical_intent_value(intent))
            .expect("memory intent is canonicalizable")
    }

    pub fn intent_digest(
        &self,
        intent: &NormalizedMemoryWriteIntentV1,
        context: &MemoryWritePolicyContextV1,
    ) -> Result<Digest32, CognitionError> {
        Self::check_identity(context)?;
        if !matches!(context.source.as_str(), "provider" | "harness" | "builtin") {
            return Err(error(
                "memory_source_mismatch",
                "memory intent source is not a recognized Harness source",
            ));
        }
        Ok(h_v1(
            MEMORY_INTENT_DOMAIN,
            &json!({
                "agent_id": context.agent_id,
                "agent_session_id": context.agent_session_id,
                "agent_turn_id": context.agent_turn_id,
                "request_digest": context.request_digest,
                "source": context.source,
                "provenance": context.provenance,
                "intent": self.canonical_intent_value(intent),
            }),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryWritePolicyOutcome {
    Committed {
        receipt_id: String,
        provenance: String,
    },
    Rejected,
    Failed,
    Pending,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryWriteStore {
    #[serde(default)]
    revision: u64,
    entries: Vec<Value>,
    committed_by_digest: BTreeMap<String, String>,
    #[serde(default)]
    corrections: Vec<MemoryCorrectionV1>,
    #[serde(default)]
    referenced_context_by_agent: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCorrectionV1 {
    pub correction_id: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub scope: String,
    pub target_memory_id: String,
    pub expected_revision: u64,
    pub replacement_summary: String,
    pub status: String,
    pub reason: String,
    pub memory_revision: u64,
    pub earliest_decision_request_id: Option<String>,
    pub earliest_request_digest: Option<String>,
    #[serde(default)]
    pub active_decision_request_id: Option<String>,
    #[serde(default)]
    pub active_request_digest: Option<String>,
    #[serde(default)]
    pub committed_decision_request_id: Option<String>,
    #[serde(default)]
    pub committed_request_digest: Option<String>,
    pub runtime_receipt_id: Option<String>,
    pub action_id: Option<String>,
}

impl MemoryWriteStore {
    pub fn corrections(&self) -> &[MemoryCorrectionV1] {
        &self.corrections
    }

    pub fn referenced_memory_context(&self, agent_id: &str) -> Option<&Value> {
        self.referenced_context_by_agent.get(agent_id)
    }

    pub fn ignore_corrections_for_decision(
        &mut self,
        agent_id: &str,
        decision_request_id: &str,
        request_digest: &str,
        reason: &str,
    ) {
        for correction in &mut self.corrections {
            if correction.status == "accepted"
                && correction.agent_id == agent_id
                && correction
                    .active_decision_request_id
                    .as_ref()
                    .or(correction.earliest_decision_request_id.as_ref())
                    .map(String::as_str)
                    == Some(decision_request_id)
                && correction
                    .active_request_digest
                    .as_ref()
                    .or(correction.earliest_request_digest.as_ref())
                    .map(String::as_str)
                    == Some(request_digest)
            {
                correction.status = "ignored".into();
                correction.reason = reason.to_string();
            }
        }
    }

    pub fn correct_memory(
        &mut self,
        mut correction: MemoryCorrectionV1,
    ) -> Result<MemoryCorrectionV1, CognitionError> {
        if correction.correction_id.trim().is_empty() || correction.correction_id.len() > 256 {
            return Err(error(
                "memory_correction_identity_invalid",
                "invalid correction identity",
            ));
        }
        correction.replacement_summary = normalized_text(
            &correction.replacement_summary,
            MAX_MEMORY_SUMMARY_BYTES,
            "memory_summary_too_large",
            "memory_summary_invalid",
        )?;
        if correction.replacement_summary.is_empty() {
            return Err(error("memory_summary_invalid", "empty correction summary"));
        }
        if let Some(previous) = self
            .corrections
            .iter()
            .find(|item| item.correction_id == correction.correction_id)
        {
            if previous.agent_id == correction.agent_id
                && previous.agent_session_id == correction.agent_session_id
                && previous.scope == correction.scope
                && previous.target_memory_id == correction.target_memory_id
                && previous.expected_revision == correction.expected_revision
                && previous.replacement_summary == correction.replacement_summary
            {
                return Ok(previous.clone());
            }
            return Err(error(
                "memory_correction_identity_reused",
                "correction identity has different payload",
            ));
        }
        correction.earliest_decision_request_id = None;
        correction.earliest_request_digest = None;
        correction.active_decision_request_id = None;
        correction.active_request_digest = None;
        correction.committed_decision_request_id = None;
        correction.committed_request_digest = None;
        correction.runtime_receipt_id = None;
        correction.action_id = None;
        let target = self.entries.iter_mut().find(|entry| {
            entry.get("intent_digest").and_then(Value::as_str)
                == Some(correction.target_memory_id.as_str())
                && entry.get("agent_id").and_then(Value::as_str)
                    == Some(correction.agent_id.as_str())
                && entry.get("agent_session_id").and_then(Value::as_str)
                    == Some(correction.agent_session_id.as_str())
                && entry.get("scope").and_then(Value::as_str) == Some(correction.scope.as_str())
        });
        if correction.expected_revision != self.revision {
            correction.status = "stale".into();
            correction.reason = "memory_revision_changed".into();
        } else if let Some(target) = target {
            target["summary"] = json!(correction.replacement_summary);
            self.revision = self.revision.saturating_add(1);
            correction.status = "accepted".into();
            correction.reason = "next_retrieval_pending".into();
        } else {
            correction.status = "ignored".into();
            correction.reason = "memory_target_or_scope_unavailable".into();
        }
        correction.memory_revision = self.revision;
        self.corrections.push(correction.clone());
        Ok(correction)
    }

    pub fn bind_corrections_to_decision(
        &mut self,
        turn: &ContinuousAgentTurnContextV1,
    ) -> Result<(), CognitionError> {
        turn.validate_for_agent(&turn.agent_id)?;
        let sources = turn
            .memory_snapshot
            .entries
            .iter()
            .map(|selected| {
                let source = self.entries.iter().find(|entry| {
                    entry.get("intent_digest").and_then(Value::as_str) == Some(selected.id.as_str())
                });
                json!({
                    "memory_id": selected.id,
                    "origin_receipt_id": source.and_then(|entry| entry.get("receipt_id")).cloned(),
                    "correction_refs": self.corrections.iter().filter(|correction| {
                        correction.target_memory_id == selected.id
                            && correction.agent_id == turn.agent_id
                            && correction.agent_session_id == turn.agent_session_id
                            && matches!(correction.status.as_str(), "accepted" | "applied")
                            && correction.replacement_summary == selected.summary
                    }).map(|correction| correction.correction_id.clone()).collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        self.referenced_context_by_agent.insert(
            turn.agent_id.clone(),
            json!({
                "agent_session_id": turn.agent_session_id,
                "decision_request_id": turn.decision_request_id,
                "request_digest": turn.request_digest.to_string(),
                "scope": turn.memory_snapshot.scope,
                "revision": turn.memory_snapshot.revision,
                "entries": turn.memory_snapshot.entries,
                "sources": sources,
                "source": "private_memory_retrieval_context",
                "used_for_decision": false,
                "current_use": "included_in_prepared_decision_request",
                "correction_hint": "correct_by_memory_id_and_revision",
            }),
        );
        for correction in &mut self.corrections {
            if correction.status == "accepted"
                && correction.earliest_decision_request_id.is_none()
                && correction.agent_id == turn.agent_id
                && correction.agent_session_id == turn.agent_session_id
                && correction.scope == turn.memory_snapshot.scope
                && turn.memory_snapshot.revision >= correction.memory_revision
                && turn.memory_snapshot.entries.iter().any(|entry| {
                    entry.id == correction.target_memory_id
                        && entry.summary == correction.replacement_summary
                })
            {
                correction.earliest_decision_request_id = Some(turn.decision_request_id.clone());
                correction.earliest_request_digest = Some(turn.request_digest.to_string());
                correction.active_decision_request_id = Some(turn.decision_request_id.clone());
                correction.active_request_digest = Some(turn.request_digest.to_string());
                correction.reason = "runtime_result_pending".into();
            }
        }
        Ok(())
    }

    pub fn rebind_corrections_after_stale_parent(
        &mut self,
        parent_request_id: &str,
        parent_digest: &str,
        turn: &ContinuousAgentTurnContextV1,
    ) -> Result<(), CognitionError> {
        turn.validate_for_agent(&turn.agent_id)?;
        for correction in &mut self.corrections {
            if correction.status == "accepted"
                && correction.agent_id == turn.agent_id
                && correction.agent_session_id == turn.agent_session_id
                && correction.scope == turn.memory_snapshot.scope
                && turn.memory_snapshot.revision >= correction.memory_revision
                && correction
                    .active_decision_request_id
                    .as_ref()
                    .or(correction.earliest_decision_request_id.as_ref())
                    .map(String::as_str)
                    == Some(parent_request_id)
                && correction
                    .active_request_digest
                    .as_ref()
                    .or(correction.earliest_request_digest.as_ref())
                    .map(String::as_str)
                    == Some(parent_digest)
                && turn.memory_snapshot.entries.iter().any(|entry| {
                    entry.id == correction.target_memory_id
                        && entry.summary == correction.replacement_summary
                })
            {
                correction.active_decision_request_id = Some(turn.decision_request_id.clone());
                correction.active_request_digest = Some(turn.request_digest.to_string());
            }
        }
        Ok(())
    }

    fn apply_verified_lineage(
        &mut self,
        intent: NormalizedMemoryWriteIntentV1,
        digest: Digest32,
        receipt: &RuntimeReceiptLineageV1,
        context: Option<&MemoryWritePolicyContextV1>,
    ) -> Result<(), CognitionError> {
        receipt
            .validate()
            .map_err(|err| error("memory_runtime_receipt_invalid", err.to_string()))?;
        if !receipt.receipt_digest.starts_with("blake3:")
            || !receipt.envelope_digest.starts_with("blake3:")
            || !receipt.request_digest.starts_with("blake3:")
            || !Digest32::from(receipt.receipt_digest.as_str()).is_canonical_blake3()
            || !Digest32::from(receipt.envelope_digest.as_str()).is_canonical_blake3()
            || !Digest32::from(receipt.request_digest.as_str()).is_canonical_blake3()
        {
            return Err(error(
                "memory_runtime_receipt_invalid",
                "Runtime receipt proof digests must be canonical BLAKE3-256 values",
            ));
        }
        if let Some(context) = context
            && (context.agent_id != receipt.agent_id
                || context.agent_session_id != receipt.agent_session_id
                || context.agent_turn_id != receipt.agent_turn_id
                || context.request_digest != receipt.request_digest)
        {
            return Err(error(
                "memory_runtime_context_mismatch",
                "memory context does not match committed receipt",
            ));
        }
        let digest_key = digest.as_str().to_string();
        if let Some(previous) = self.committed_by_digest.get(&digest_key) {
            if previous == &receipt.receipt_id {
                return Ok(());
            }
            return Err(error(
                "memory_digest_mismatch",
                "a committed memory digest was replayed with a different receipt",
            ));
        }
        let mut entry = serde_json::to_value(intent)
            .map_err(|e| error("memory_payload_invalid", e.to_string()))?;
        let object = entry
            .as_object_mut()
            .expect("normalized memory intent is an object");
        object.insert("intent_digest".to_string(), json!(digest_key));
        object.insert("receipt_id".to_string(), json!(receipt.receipt_id));
        object.insert("provenance".to_string(), json!("runtime_authoritative"));
        if let Some(context) = context {
            object.insert("agent_id".to_string(), json!(context.agent_id));
            object.insert(
                "agent_session_id".to_string(),
                json!(context.agent_session_id),
            );
            object.insert("agent_turn_id".to_string(), json!(context.agent_turn_id));
            object.insert("request_digest".to_string(), json!(context.request_digest));
        }
        self.committed_by_digest
            .insert(digest_key, receipt.receipt_id.clone());
        self.entries.push(entry);
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn apply_runtime_receipt<A: RuntimeAuthority>(
        &mut self,
        intent: NormalizedMemoryWriteIntentV1,
        digest: Digest32,
        receipt: &A::Receipt,
        authority: &A,
    ) -> Result<(), CognitionError> {
        self.apply_runtime_receipt_with_context(intent, digest, receipt, None, authority)
    }

    pub fn apply_runtime_receipt_with_context<A: RuntimeAuthority>(
        &mut self,
        intent: NormalizedMemoryWriteIntentV1,
        digest: Digest32,
        receipt: &A::Receipt,
        context: Option<&MemoryWritePolicyContextV1>,
        authority: &A,
    ) -> Result<(), CognitionError> {
        let lineage = authority.verify_memory_receipt(receipt, context)?;
        self.apply_verified_lineage(intent, digest, &lineage, context)
    }

    pub fn finalize_corrections<A: RuntimeAuthority>(
        &mut self,
        receipt: &A::Receipt,
        context: &MemoryWritePolicyContextV1,
        authority: &A,
    ) -> Result<(), CognitionError> {
        let lineage = authority.verify_memory_receipt(receipt, Some(context))?;
        lineage
            .validate()
            .map_err(|err| error("memory_runtime_receipt_invalid", err.to_string()))?;
        if let Some(saved) = self.referenced_context_by_agent.get_mut(&lineage.agent_id)
            && saved.get("agent_session_id").and_then(Value::as_str)
                == Some(lineage.agent_session_id.as_str())
            && saved.get("decision_request_id").and_then(Value::as_str)
                == Some(lineage.decision_request_id.as_str())
            && saved.get("request_digest").and_then(Value::as_str)
                == Some(lineage.request_digest.as_str())
        {
            saved["used_for_decision"] = json!(true);
            saved["current_use"] = json!("committed_decision_context");
        }
        for correction in &mut self.corrections {
            if correction.status == "accepted"
                && correction.agent_id == lineage.agent_id
                && correction.agent_session_id == lineage.agent_session_id
                && correction
                    .active_decision_request_id
                    .as_ref()
                    .or(correction.earliest_decision_request_id.as_ref())
                    .map(String::as_str)
                    == Some(lineage.decision_request_id.as_str())
                && correction
                    .active_request_digest
                    .as_ref()
                    .or(correction.earliest_request_digest.as_ref())
                    .map(String::as_str)
                    == Some(lineage.request_digest.as_str())
            {
                correction.status = "applied".into();
                correction.reason = "corrected_context_committed_decision".into();
                correction.runtime_receipt_id = Some(lineage.receipt_id.clone());
                correction.action_id = Some(lineage.action_id.clone());
                correction.committed_decision_request_id =
                    Some(lineage.decision_request_id.clone());
                correction.committed_request_digest = Some(lineage.request_digest.clone());
            }
        }
        Ok(())
    }

    pub fn entries(&self) -> &[Value] {
        &self.entries
    }

    pub fn context_snapshot(
        &self,
        agent_id: &str,
        agent_session_id: &str,
        scope: &str,
        max_entries: usize,
    ) -> MemoryContextSnapshotV1 {
        let limit = max_entries.min(MAX_MEMORY_INTENTS);
        let mut selected = self
            .entries
            .iter()
            .filter(|entry| {
                entry.get("agent_id").and_then(Value::as_str) == Some(agent_id)
                    && entry.get("agent_session_id").and_then(Value::as_str)
                        == Some(agent_session_id)
                    && entry.get("scope").and_then(Value::as_str) == Some(scope)
            })
            .filter_map(|entry| {
                Some(MemoryContextEntryV1 {
                    id: entry.get("intent_digest")?.as_str()?.to_string(),
                    summary: entry.get("summary")?.as_str()?.to_string(),
                    tags: entry
                        .get("tags")
                        .and_then(Value::as_array)
                        .map(|tags| {
                            tags.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            })
            .collect::<Vec<_>>();
        if selected.len() > limit {
            selected.drain(..selected.len() - limit);
        }
        let mut snapshot = MemoryContextSnapshotV1 {
            revision: self.revision,
            entries: selected,
            scope: scope.to_string(),
            digest: String::new(),
        };
        snapshot.digest = snapshot.computed_digest();
        snapshot
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalSnapshotInputV1 {
    pub revision: u64,
    pub short_term_summary: String,
    pub long_term_summary: String,
    #[serde(default)]
    pub blocked_reason: Option<String>,
    pub provenance: String,
}

pub struct GoalSnapshotProjector;

impl GoalSnapshotProjector {
    pub fn project(
        host: Option<GoalSnapshotInputV1>,
        legacy: Option<GoalSnapshotInputV1>,
    ) -> Result<GoalSnapshotV1, CognitionError> {
        let input = if let Some(input) = host {
            if input.provenance != "harness_projection" {
                return Err(error(
                    "goal_snapshot_invalid",
                    "host goal projection has an invalid provenance",
                ));
            }
            input
        } else if let Some(input) = legacy {
            if input.provenance != "legacy_provider" {
                return Err(error(
                    "goal_snapshot_invalid",
                    "legacy goal projection has an invalid provenance",
                ));
            }
            input
        } else {
            return Ok(GoalSnapshotV1::empty());
        };
        let short_term_summary = normalized_text(
            &input.short_term_summary,
            MAX_GOAL_SUMMARY_BYTES,
            "goal_snapshot_too_large",
            "goal_snapshot_invalid",
        )?;
        let long_term_summary = normalized_text(
            &input.long_term_summary,
            MAX_GOAL_SUMMARY_BYTES,
            "goal_snapshot_too_large",
            "goal_snapshot_invalid",
        )?;
        let blocked_reason = input
            .blocked_reason
            .map(|value| {
                let normalized = normalized_text(
                    &value,
                    MAX_BLOCKED_REASON_BYTES,
                    "goal_snapshot_too_large",
                    "goal_snapshot_invalid",
                )?;
                Ok((!normalized.is_empty()).then_some(normalized))
            })
            .transpose()?
            .flatten();
        let mut snapshot = GoalSnapshotV1 {
            revision: input.revision,
            short_term_summary,
            long_term_summary,
            blocked_reason,
            provenance: input.provenance,
            digest: String::new(),
        };
        snapshot.digest = snapshot.computed_digest();
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> MemoryWritePolicyContextV1 {
        MemoryWritePolicyContextV1 {
            agent_id: "agent-memory-1".into(),
            agent_session_id: "session-memory-1".into(),
            agent_turn_id: "turn-memory-1".into(),
            request_digest:
                "blake3:1111111111111111111111111111111111111111111111111111111111111111".into(),
            source: "provider".into(),
            provenance: "provider_unverified".into(),
        }
    }

    fn intent(scope: &str, summary: Option<&str>, tags: &[&str]) -> MemoryWriteIntentV1 {
        MemoryWriteIntentV1 {
            schema_version: 1,
            scope: scope.into(),
            summary: summary.map(str::to_string),
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
            compatibility_reason: None,
        }
    }

    #[test]
    fn memory_snapshot_digest_matches_existing_literal_golden() {
        let snapshot = MemoryContextSnapshotV1::empty("turn_private");
        assert_eq!(
            snapshot.digest,
            "blake3:e6c8320449505fd5dc2fe87f7081a41be6e2264645190f86fb716006bb93879d"
        );
        assert_eq!(
            serde_json::to_value(snapshot).unwrap()["entries"],
            serde_json::json!([])
        );
    }

    #[test]
    fn goal_snapshot_digest_matches_existing_literal_golden() {
        let snapshot = GoalSnapshotV1::empty();
        assert_eq!(
            snapshot.digest,
            "blake3:31c93293570c36946869bc941435981ad5063734e65342a5eda6b79ec6fc9fe9"
        );
    }

    #[test]
    fn memory_policy_preserves_normalization_bounds_and_scope_fence() {
        let policy = MemoryWriteIntentPolicyV1::default();
        let normalized = policy
            .normalize(
                intent("session_private", Some(" e\u{301} "), &[" z ", "a", "a"]),
                &context(),
            )
            .unwrap();
        assert_eq!(normalized.summary.as_deref(), Some("é"));
        assert_eq!(normalized.tags, ["a", "z"]);
        assert_eq!(
            policy
                .normalize(intent("world_shared", Some("bad"), &[]), &context())
                .unwrap_err()
                .code(),
            "memory_scope_denied"
        );
    }

    #[test]
    fn normalized_memory_intent_digest_matches_literal_fixture() {
        let policy = MemoryWriteIntentPolicyV1::default();
        let normalized = policy
            .normalize(
                intent("session_private", Some(" e\u{301} "), &[" z ", "a", "a"]),
                &context(),
            )
            .unwrap();
        assert_eq!(
            policy
                .intent_digest(&normalized, &context())
                .unwrap()
                .as_str(),
            "blake3:9b9025a8c606a75a074e5e1b1c747d4d42cd9ed037f9d30e708a2b93f4713690"
        );
    }

    #[test]
    fn provider_cannot_self_assign_runtime_memory_provenance() {
        let mut context = context();
        context.provenance = "runtime_authoritative".into();
        assert_eq!(
            MemoryWriteIntentPolicyV1::default()
                .normalize(intent("turn_private", Some("not committed"), &[]), &context)
                .unwrap_err()
                .code(),
            "memory_source_mismatch"
        );
    }
}
