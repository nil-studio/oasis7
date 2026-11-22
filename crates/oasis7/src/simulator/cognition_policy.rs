//! Simulator-side policy objects for the continuous-agent Harness.
//!
//! Pure memory write policy, normalized intent, correction, and goal projection
//! types live in `oasis7_agent_harness` (`DES-AGC-001`) and are re-exported
//! here; this module retains only the host/Runtime receipt adapter methods on
//! `MemoryWriteStore`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::continuous_agent_harness::{CognitionError, Digest32, MemoryWriteIntentV1};

pub use oasis7_agent_api::{GoalSnapshotV1, MemoryContextEntryV1, MemoryContextSnapshotV1};

use oasis7_agent_harness::MemoryWriteStore as HarnessMemoryWriteStore;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_agent_harness::RuntimeAuthority as HarnessRuntimeAuthority;
pub use oasis7_agent_harness::{
    GoalSnapshotInputV1, GoalSnapshotProjector, MemoryCorrectionV1, MemoryWriteIntentPolicyV1,
    MemoryWritePolicyContextV1, MemoryWritePolicyOutcome, NormalizedMemoryWriteIntentV1,
};

#[path = "cognition_continuation.rs"]
mod cognition_continuation;
pub use cognition_continuation::*;

fn error(code: &'static str, message: impl Into<String>) -> CognitionError {
    CognitionError::new(code, message)
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

impl From<MemoryWriteIntentV1> for oasis7_agent_api::MemoryWriteIntentV1 {
    fn from(value: MemoryWriteIntentV1) -> Self {
        Self {
            schema_version: value.schema_version,
            scope: value.scope,
            summary: value.summary,
            tags: value.tags,
            compatibility_reason: value.compatibility_reason,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryWriteStore {
    /// Monotonic store revision used to bind a retrieved snapshot to the
    /// committed memory projection that produced it.  Older checkpoints did
    /// not persist a revision, so serde defaults them to the empty revision.
    #[serde(default)]
    revision: u64,
    entries: Vec<Value>,
    committed_by_digest: BTreeMap<String, String>,
    #[serde(default)]
    corrections: Vec<MemoryCorrectionV1>,
    #[serde(default)]
    referenced_context_by_agent: BTreeMap<String, Value>,
}

impl MemoryWriteStore {
    fn with_harness_store<T>(
        &mut self,
        operation: impl FnOnce(&mut HarnessMemoryWriteStore) -> Result<T, CognitionError>,
    ) -> Result<T, CognitionError> {
        let mut core: HarnessMemoryWriteStore = serde_json::from_value(
            serde_json::to_value(&*self)
                .map_err(|err| error("memory_store_adapter_invalid", err.to_string()))?,
        )
        .map_err(|err| error("memory_store_adapter_invalid", err.to_string()))?;
        let result = operation(&mut core);
        *self = serde_json::from_value(
            serde_json::to_value(core)
                .map_err(|err| error("memory_store_adapter_invalid", err.to_string()))?,
        )
        .map_err(|err| error("memory_store_adapter_invalid", err.to_string()))?;
        result
    }

    fn harness_store(&self) -> Result<HarnessMemoryWriteStore, CognitionError> {
        serde_json::from_value(
            serde_json::to_value(self)
                .map_err(|err| error("memory_store_adapter_invalid", err.to_string()))?,
        )
        .map_err(|err| error("memory_store_adapter_invalid", err.to_string()))
    }

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
        self.with_harness_store(|store| {
            store.ignore_corrections_for_decision(
                agent_id,
                decision_request_id,
                request_digest,
                reason,
            );
            Ok(())
        })
        .expect("memory checkpoint schema must round-trip through the canonical store");
    }

    /// The host must authorize the subject/session before calling this method.
    /// Exact revision and target checks prevent an old client overwriting a
    /// newer memory or a different subject's private context.
    pub fn correct_memory(
        &mut self,
        correction: MemoryCorrectionV1,
    ) -> Result<MemoryCorrectionV1, CognitionError> {
        self.with_harness_store(|store| store.correct_memory(correction))
    }

    pub fn bind_corrections_to_decision(
        &mut self,
        turn: &super::continuous_agent_harness::ContinuousAgentTurnContextV1,
    ) -> Result<(), CognitionError> {
        self.with_harness_store(|store| store.bind_corrections_to_decision(turn))
    }

    /// Host-only continuation of an authoritative stale parent. Earliest
    /// preparation provenance remains immutable; only finalization moves.
    pub fn rebind_corrections_after_stale_parent(
        &mut self,
        parent_request_id: &str,
        parent_digest: &str,
        turn: &super::continuous_agent_harness::ContinuousAgentTurnContextV1,
    ) -> Result<(), CognitionError> {
        self.with_harness_store(|store| {
            store.rebind_corrections_after_stale_parent(parent_request_id, parent_digest, turn)
        })
    }

    /// Compatibility-only receipt DTO finalization. Production callers must
    /// finalize corrections through the authority-gated Harness transition.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn finalize_corrections(
        &mut self,
        receipt: &crate::runtime::RuntimeReceiptLineageV1,
    ) -> Result<(), CognitionError> {
        receipt
            .validate()
            .map_err(|err| error("memory_runtime_receipt_invalid", err.to_string()))?;
        if let Some(context) = self.referenced_context_by_agent.get_mut(&receipt.agent_id)
            && context.get("agent_session_id").and_then(Value::as_str)
                == Some(receipt.agent_session_id.as_str())
            && context.get("decision_request_id").and_then(Value::as_str)
                == Some(receipt.decision_request_id.as_str())
            && context.get("request_digest").and_then(Value::as_str)
                == Some(receipt.request_digest.as_str())
        {
            context["used_for_decision"] = json!(true);
            context["current_use"] = json!("committed_decision_context");
        }
        for correction in &mut self.corrections {
            if correction.status == "accepted"
                && correction.agent_id == receipt.agent_id
                && correction.agent_session_id == receipt.agent_session_id
                && correction
                    .active_decision_request_id
                    .as_ref()
                    .or(correction.earliest_decision_request_id.as_ref())
                    .map(String::as_str)
                    == Some(receipt.decision_request_id.as_str())
                && correction
                    .active_request_digest
                    .as_ref()
                    .or(correction.earliest_request_digest.as_ref())
                    .map(String::as_str)
                    == Some(receipt.request_digest.as_str())
            {
                correction.status = "applied".into();
                correction.reason = "corrected_context_committed_decision".into();
                correction.runtime_receipt_id = Some(receipt.receipt_id.clone());
                correction.action_id = Some(receipt.action_id.clone());
                correction.committed_decision_request_id =
                    Some(receipt.decision_request_id.clone());
                correction.committed_request_digest = Some(receipt.request_digest.clone());
            }
        }
        Ok(())
    }

    pub fn entries(&self) -> &[Value] {
        &self.entries
    }

    /// Build a deterministic, bounded snapshot for the next turn of one
    /// Agent session.
    pub fn context_snapshot(
        &self,
        agent_id: &str,
        agent_session_id: &str,
        scope: &str,
        max_entries: usize,
    ) -> MemoryContextSnapshotV1 {
        self.harness_store()
            .expect("memory checkpoint schema must round-trip through the canonical store")
            .context_snapshot(agent_id, agent_session_id, scope, max_entries)
    }

    pub fn apply(
        &mut self,
        intent: NormalizedMemoryWriteIntentV1,
        digest: Digest32,
        outcome: MemoryWritePolicyOutcome,
    ) -> Result<(), CognitionError> {
        let digest_key = digest.as_str().to_string();
        if let Some(receipt) = self.committed_by_digest.get(&digest_key) {
            if matches!(outcome, MemoryWritePolicyOutcome::Committed { ref receipt_id, .. } if receipt_id == receipt)
            {
                return Ok(());
            }
            return Err(error(
                "memory_digest_mismatch",
                "a committed memory digest was replayed with a different receipt",
            ));
        }
        let MemoryWritePolicyOutcome::Committed {
            receipt_id,
            provenance,
        } = outcome
        else {
            return Err(error(
                "memory_no_committed_outcome",
                "authoritative memory requires a committed Runtime receipt",
            ));
        };
        if receipt_id.trim().is_empty() || provenance != "runtime_authoritative" {
            return Err(error(
                "memory_source_mismatch",
                "memory commit provenance is not Runtime-authoritative",
            ));
        }
        let mut entry = serde_json::to_value(intent)
            .map_err(|err| error("memory_payload_invalid", err.to_string()))?;
        let object = entry
            .as_object_mut()
            .expect("normalized memory intent is an object");
        object.insert("intent_digest".to_string(), json!(digest_key));
        object.insert("receipt_id".to_string(), json!(receipt_id));
        object.insert("provenance".to_string(), json!(provenance));
        self.committed_by_digest.insert(
            digest_key,
            object["receipt_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        self.entries.push(entry);
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    fn annotate_context(&mut self, digest: &Digest32, context: &MemoryWritePolicyContextV1) {
        let digest = digest.as_str();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.get("intent_digest").and_then(Value::as_str) == Some(digest))
            && let Some(object) = entry.as_object_mut()
        {
            object.insert("agent_id".to_string(), json!(context.agent_id));
            object.insert(
                "agent_session_id".to_string(),
                json!(context.agent_session_id),
            );
            object.insert("agent_turn_id".to_string(), json!(context.agent_turn_id));
            object.insert("request_digest".to_string(), json!(context.request_digest));
        }
    }

    /// Compatibility-only projection from a receipt DTO.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn apply_runtime_receipt(
        &mut self,
        intent: NormalizedMemoryWriteIntentV1,
        digest: Digest32,
        receipt: &crate::runtime::RuntimeReceiptLineageV1,
    ) -> Result<(), CognitionError> {
        self.apply_runtime_receipt_with_context(intent, digest, receipt, None)
    }

    /// Compatibility-only receipt DTO projection with optional Harness
    /// identity metadata.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn apply_runtime_receipt_with_context(
        &mut self,
        intent: NormalizedMemoryWriteIntentV1,
        digest: Digest32,
        receipt: &crate::runtime::RuntimeReceiptLineageV1,
        context: Option<&MemoryWritePolicyContextV1>,
    ) -> Result<(), CognitionError> {
        receipt.validate().map_err(|runtime_error| {
            error("memory_runtime_receipt_invalid", runtime_error.to_string())
        })?;
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
        self.apply(
            intent,
            digest.clone(),
            MemoryWritePolicyOutcome::Committed {
                receipt_id: receipt.receipt_id.clone(),
                provenance: "runtime_authoritative".to_string(),
            },
        )?;
        if let Some(context) = context {
            self.annotate_context(&digest, context);
        }
        Ok(())
    }

    /// Production receipt projection delegates state changes to the canonical
    /// Harness transition after the supplied host authority verifies the
    /// receipt.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn apply_runtime_receipt_with_authority<A: HarnessRuntimeAuthority>(
        &mut self,
        intent: NormalizedMemoryWriteIntentV1,
        digest: Digest32,
        receipt: &A::Receipt,
        context: &MemoryWritePolicyContextV1,
        authority: &A,
    ) -> Result<(), CognitionError> {
        self.with_harness_store(|store| {
            store.apply_runtime_receipt_with_context(
                intent,
                digest,
                receipt,
                Some(context),
                authority,
            )
        })
    }

    /// Finalize only corrections bound to a receipt accepted by the same
    /// authority boundary as memory writes.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn finalize_corrections_with_authority<A: HarnessRuntimeAuthority>(
        &mut self,
        receipt: &A::Receipt,
        context: &MemoryWritePolicyContextV1,
        authority: &A,
    ) -> Result<(), CognitionError> {
        self.with_harness_store(|store| store.finalize_corrections(receipt, context, authority))
    }
}
