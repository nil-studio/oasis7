use std::collections::BTreeSet;
use std::fmt;

use oasis7_wasm_abi::encode_canonical_cbor;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::provider::{DecisionRequest, DecisionResponse};

pub const CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR: &str = "oasis7.continuous-agent-context";
pub const CONTINUOUS_AGENT_CONTEXT_VERSION: u16 = 1;
pub const COGNITION_REQUEST_DIGEST_DOMAIN: &str = "oasis7.cognition.request.v1";
pub const COGNITION_PROVIDER_INVOCATION_DOMAIN: &str = "oasis7.cognition.provider-invocation.v1";
pub const COGNITION_RESPONSE_ARTIFACT_IDENTITY_DOMAIN: &str =
    "oasis7.cognition.response-artifact-identity.v1";
pub const COGNITION_CAPABILITY_CATALOG_DOMAIN: &str = "oasis7.cognition.capability-catalog.v1";
pub const COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN: &str =
    "oasis7.cognition.capability-invocation-context.v1";
const CONTINUATION_PROPOSAL_DOMAIN: &str = "oasis7.cognition.continuation-proposal.v1";
const COGNITION_FEEDBACK_DIGEST_DOMAIN: &str = "oasis7.cognition.feedback.v1";
const MAX_WAKE_CONDITIONS: usize = 16;
const MAX_WAKE_ITEM_BYTES: usize = 768;
const MAX_WAKE_LIST_BYTES: usize = 4096;

/// A wire digest in the established `blake3:<lowercase-hex>` representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Digest32(pub String);

impl Digest32 {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn expect(self, _message: &str) -> Self {
        self
    }

    pub fn is_canonical_blake3(&self) -> bool {
        valid_blake3_digest(self.as_str())
    }
}

impl fmt::Display for Digest32 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

impl From<String> for Digest32 {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Digest32 {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// Stable, domain-separated BLAKE3-256 over canonical CBOR `[domain,payload]`.
pub fn h_v1<T: Serialize>(domain: &str, payload: &T) -> Digest32 {
    let bytes = encode_canonical_cbor(&(domain, payload))
        .expect("cognition payload must be encodable as canonical CBOR");
    Digest32(format!("blake3:{}", blake3::hash(bytes.as_slice())))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitionError {
    code: String,
    message: String,
}

impl CognitionError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CognitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CognitionError {}

/// Runtime-owned branch/finality projection carried by cognition identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBindingV1 {
    pub world_id: String,
    pub branch_id: String,
    pub finality_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finality_block_hash: Option<Digest32>,
    pub finality_status: String,
    pub base_tick: u64,
    pub base_world_hash: Digest32,
    pub reorg_epoch: u64,
    pub runtime_manifest_hash: Digest32,
}

impl RuntimeBindingV1 {
    pub fn validate(&self) -> Result<(), CognitionError> {
        if self.branch_id.trim().is_empty() || self.branch_id.len() > 128 {
            return Err(CognitionError::new(
                "recovery_pending",
                "finality binding branch identity is invalid",
            ));
        }
        if !matches!(
            self.finality_status.as_str(),
            "pending" | "verified" | "reorged" | "suspended"
        ) {
            return Err(CognitionError::new(
                "recovery_pending",
                "finality binding status is not in the v1 registry",
            ));
        }
        match self.finality_block_hash.as_ref() {
            Some(hash) if !hash.is_canonical_blake3() => Err(CognitionError::new(
                "recovery_pending",
                "finality block hash is not a canonical BLAKE3-256 digest",
            )),
            None if self.finality_status == "verified" => Err(CognitionError::new(
                "recovery_pending",
                "verified finality requires a block hash",
            )),
            _ if self.base_world_hash.is_canonical_blake3()
                && self.runtime_manifest_hash.is_canonical_blake3() =>
            {
                Ok(())
            }
            _ => Err(CognitionError::new(
                "invalid_runtime_binding_digest",
                "Runtime binding hashes must be canonical BLAKE3-256 values",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetContractV1 {
    pub max_latency_ms: u64,
    pub max_repair_attempts: u32,
    pub max_model_calls: u32,
    pub max_tool_calls: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteIntentV1 {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackEnvelopeV1 {
    pub feedback_id: String,
    pub feedback_seq: u64,
    pub agent_subject: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_action_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_receipt_id: Option<String>,
    pub status: String,
    pub request_digest: Digest32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
    pub provenance: String,
}

impl FeedbackEnvelopeV1 {
    /// Validate shape and provenance. This cannot establish a durable receipt
    /// or authorize a world effect.
    pub fn validate(&self) -> Result<(), CognitionError> {
        if self.feedback_id.trim().is_empty()
            || self.feedback_seq == 0
            || self.agent_subject.trim().is_empty()
            || self.agent_session_id.trim().is_empty()
            || self.agent_turn_id.trim().is_empty()
            || self.decision_request_id.trim().is_empty()
            || !self.request_digest.is_canonical_blake3()
            || self.provenance != "runtime_authoritative"
            || !matches!(
                self.status.as_str(),
                "pending" | "committed" | "rejected" | "failed"
            )
        {
            return Err(CognitionError::new(
                "feedback_contract_invalid",
                "feedback requires canonical identity, Runtime provenance, sequence, and status",
            ));
        }
        if self.status == "committed"
            && (self.candidate_action_id.is_none()
                || self
                    .runtime_receipt_id
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty()))
        {
            return Err(CognitionError::new(
                "feedback_contract_invalid",
                "committed feedback requires action and Runtime receipt identity",
            ));
        }
        Ok(())
    }

    pub fn validate_value(value: &Value) -> Result<(), CognitionError> {
        let Some(object) = value.as_object() else {
            return Err(CognitionError::new(
                "unknown_context_field",
                "feedback must be a JSON object",
            ));
        };
        const ALLOWED: &[&str] = &[
            "feedback_id",
            "feedback_seq",
            "agent_subject",
            "agent_session_id",
            "agent_turn_id",
            "decision_request_id",
            "candidate_action_id",
            "runtime_receipt_id",
            "status",
            "request_digest",
            "reject_reason",
            "provenance",
        ];
        if let Some(field) = object.keys().find(|key| !ALLOWED.contains(&key.as_str())) {
            return Err(CognitionError::new(
                "unknown_context_field",
                format!("unknown feedback field `{field}`"),
            ));
        }
        Ok(())
    }
}

pub fn feedback_digest(feedback: &FeedbackEnvelopeV1) -> Digest32 {
    h_v1(COGNITION_FEEDBACK_DIGEST_DOMAIN, feedback)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseArtifactIdentityV1 {
    pub schema_version: u16,
    pub context_discriminator: String,
    pub context_version: u16,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub request_digest: Digest32,
    pub response_digest: Digest32,
    pub artifact_digest: Digest32,
}

impl ResponseArtifactIdentityV1 {
    fn canonical_payload(&self) -> Value {
        let mut payload = serde_json::to_value(self).expect("response identity is serializable");
        payload
            .as_object_mut()
            .expect("response identity is an object")
            .remove("artifact_digest");
        payload
    }

    pub fn validate(&self) -> Result<(), CognitionError> {
        if self.schema_version != 1
            || self.context_discriminator != CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR
            || self.context_version != CONTINUOUS_AGENT_CONTEXT_VERSION
            || self.agent_session_id.trim().is_empty()
            || self.agent_turn_id.trim().is_empty()
            || self.decision_request_id.trim().is_empty()
            || !self.request_digest.is_canonical_blake3()
            || !self.response_digest.is_canonical_blake3()
            || !self.artifact_digest.is_canonical_blake3()
        {
            return Err(CognitionError::new(
                "response_artifact_identity_invalid",
                "response artifact identity is incomplete or not canonical",
            ));
        }
        if self.artifact_digest
            != h_v1(
                COGNITION_RESPONSE_ARTIFACT_IDENTITY_DOMAIN,
                &self.canonical_payload(),
            )
        {
            return Err(CognitionError::new(
                "response_artifact_identity_mismatch",
                "response artifact identity digest does not match its lineage",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuousAgentRequestContextV1 {
    pub base_decision_request: DecisionRequest,
    pub context_discriminator: String,
    pub context_version: u16,
    pub protocol_version: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub agent_subject: String,
    pub runtime_binding: RuntimeBindingV1,
    pub observation_digest: Digest32,
    pub capability_catalog_digest: Digest32,
    pub capability_invocation_context_digest: Digest32,
    pub memory_snapshot_digest: Digest32,
    pub goal_snapshot_digest: Digest32,
    pub continuation_digest: Digest32,
    pub adapter_protocol_version: String,
    pub budget_contract: BudgetContractV1,
    pub request_digest: Digest32,
}

impl ContinuousAgentRequestContextV1 {
    pub fn validate(&self) -> Result<(), CognitionError> {
        self.validate_structure()?;
        self.base_decision_request
            .validate_contract()
            .map_err(|error| CognitionError::new(error.code, error.message))?;
        if self.request_digest != self.request_digest() {
            return Err(CognitionError::new(
                "request_digest_mismatch",
                "declared request digest does not match canonical request inputs",
            ));
        }
        Ok(())
    }

    pub fn validate_production_lane(&self) -> Result<(), CognitionError> {
        self.validate()?;
        if self.retry_seq == 0 || self.transport_attempt == 0 {
            return Err(CognitionError::new(
                "missing_retry_lineage",
                "production provider requests require nonzero retry_seq and transport_attempt",
            ));
        }
        self.validate_production_capability_context()
    }

    fn validate_production_capability_context(&self) -> Result<(), CognitionError> {
        let catalog = self
            .base_decision_request
            .capability_catalog
            .as_ref()
            .ok_or_else(|| {
                CognitionError::new(
                    "missing_capability_context",
                    "production provider requests require a Runtime capability catalog",
                )
            })?;
        let invocation = self
            .base_decision_request
            .capability_invocation_context
            .as_ref()
            .ok_or_else(|| {
                CognitionError::new(
                    "missing_capability_context",
                    "production provider requests require a Runtime capability invocation context",
                )
            })?;
        catalog.validate().map_err(|error| {
            CognitionError::new(
                "invalid_capability_context",
                format!("Runtime capability catalog is invalid: {error}"),
            )
        })?;
        if self.capability_catalog_digest != h_v1(COGNITION_CAPABILITY_CATALOG_DOMAIN, catalog) {
            return Err(CognitionError::new(
                "capability_context_digest_mismatch",
                "capability catalog digest does not match the Runtime-bound snapshot",
            ));
        }
        if self.capability_invocation_context_digest
            != h_v1(COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN, invocation)
        {
            return Err(CognitionError::new(
                "capability_context_digest_mismatch",
                "capability invocation context digest does not match the Runtime-bound context",
            ));
        }
        if catalog.snapshot_id != invocation.catalog_snapshot_id
            || catalog.subject != invocation.subject
            || catalog.presenter != invocation.presenter
        {
            return Err(CognitionError::new(
                "capability_context_mismatch",
                "capability catalog and invocation context are not bound to the same snapshot",
            ));
        }
        if catalog.world_id != self.runtime_binding.world_id
            || catalog.branch_id != self.runtime_binding.branch_id
            || catalog.finality_epoch != self.runtime_binding.finality_epoch
            || catalog.logical_tick != self.runtime_binding.base_tick
            || catalog.audience.world_id != self.runtime_binding.world_id
            || catalog.audience.branch_id != self.runtime_binding.branch_id
            || catalog.audience.finality_epoch != self.runtime_binding.finality_epoch
        {
            return Err(CognitionError::new(
                "capability_runtime_binding_mismatch",
                "capability catalog is not bound to the request Runtime world/finality projection",
            ));
        }
        match &invocation.subject {
            oasis7_wasm_abi::CapabilitySubject::Agent { agent_id, .. }
                if agent_id == &self.agent_subject => {}
            _ => {
                return Err(CognitionError::new(
                    "capability_subject_mismatch",
                    "capability invocation subject must be the requesting agent",
                ));
            }
        }
        if invocation.grant_id.trim().is_empty()
            || invocation.catalog_snapshot_id.trim().is_empty()
            || invocation.response_nonce.trim().is_empty()
        {
            return Err(CognitionError::new(
                "invalid_capability_context",
                "capability invocation grant, snapshot, and response nonce are required",
            ));
        }
        Ok(())
    }

    pub fn validate_value(value: &Value) -> Result<(), CognitionError> {
        let context: Self = serde_json::from_value(value.clone())
            .map_err(|error| CognitionError::new("unknown_context_field", error.to_string()))?;
        context.validate_structure()
    }

    fn validate_structure(&self) -> Result<(), CognitionError> {
        if self.context_discriminator != CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR {
            return Err(CognitionError::new(
                "unsupported_context_discriminator",
                "continuous-agent request discriminator is not recognized",
            ));
        }
        if self.context_version != CONTINUOUS_AGENT_CONTEXT_VERSION {
            return Err(CognitionError::new(
                "unsupported_context_version",
                format!(
                    "unsupported continuous-agent context version {}",
                    self.context_version
                ),
            ));
        }
        if self.agent_session_id.trim().is_empty()
            || self.agent_turn_id.trim().is_empty()
            || self.decision_request_id.trim().is_empty()
            || self.agent_subject.trim().is_empty()
        {
            return Err(CognitionError::new(
                "missing_cognition_identity",
                "session, turn, request, and subject identities are required",
            ));
        }
        if self.base_decision_request.observation.agent_id != self.agent_subject {
            return Err(CognitionError::new(
                "cognition_context_identity_mismatch",
                "outer request subject does not match the inner observation subject",
            ));
        }
        for (name, digest) in [
            ("observation_digest", &self.observation_digest),
            ("capability_catalog_digest", &self.capability_catalog_digest),
            (
                "capability_invocation_context_digest",
                &self.capability_invocation_context_digest,
            ),
            ("memory_snapshot_digest", &self.memory_snapshot_digest),
            ("goal_snapshot_digest", &self.goal_snapshot_digest),
            ("continuation_digest", &self.continuation_digest),
        ] {
            if !digest.is_canonical_blake3() {
                return Err(CognitionError::new(
                    "invalid_context_digest",
                    format!("{name} must be a canonical BLAKE3-256 digest"),
                ));
            }
        }
        self.runtime_binding.validate()
    }

    /// Canonical request payload bytes exclude output identity, transport
    /// attempt, and both legacy timeout-only transport fields. Optional
    /// finality remains an explicit null exactly as in the V1 contract.
    pub fn canonical_request_bytes(&self) -> Result<Vec<u8>, CognitionError> {
        self.validate_structure()?;
        let mut value = serde_json::to_value(self)
            .map_err(|error| CognitionError::new("canonical_encoding_failed", error.to_string()))?;
        let object = value.as_object_mut().ok_or_else(|| {
            CognitionError::new(
                "canonical_encoding_failed",
                "request context is not an object",
            )
        })?;
        object.remove("request_digest");
        object.remove("transport_attempt");
        if let Some(base) = object
            .get_mut("base_decision_request")
            .and_then(Value::as_object_mut)
        {
            base.remove("timeout_budget_ms");
            if let Some(observation) = base.get_mut("observation").and_then(Value::as_object_mut) {
                observation.remove("timeout_budget_ms");
            }
        }
        if let Some(runtime_binding) = object
            .get_mut("runtime_binding")
            .and_then(Value::as_object_mut)
        {
            runtime_binding
                .entry("finality_block_hash")
                .or_insert(Value::Null);
        }
        encode_canonical_cbor(&value)
            .map_err(|error| CognitionError::new("canonical_encoding_failed", error.to_string()))
    }

    pub fn request_digest(&self) -> Digest32 {
        h_v1(
            COGNITION_REQUEST_DIGEST_DOMAIN,
            &self
                .canonical_request_bytes()
                .expect("valid cognition request must have canonical bytes"),
        )
    }

    pub fn provider_invocation_key(&self) -> Digest32 {
        h_v1(COGNITION_PROVIDER_INVOCATION_DOMAIN, &self.request_digest())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuousAgentResponseContextV1<A = Value, Q = Value> {
    pub base_decision_response: DecisionResponse<A, Q>,
    pub context_discriminator: String,
    pub context_version: u16,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub request_digest: Digest32,
    #[serde(default)]
    pub response_digest: Digest32,
}

impl<A, Q> ContinuousAgentResponseContextV1<A, Q>
where
    A: Serialize,
    Q: Serialize,
{
    pub fn response_artifact_identity(&self) -> ResponseArtifactIdentityV1 {
        let mut identity = ResponseArtifactIdentityV1 {
            schema_version: 1,
            context_discriminator: self.context_discriminator.clone(),
            context_version: self.context_version,
            agent_session_id: self.agent_session_id.clone(),
            agent_turn_id: self.agent_turn_id.clone(),
            decision_request_id: self.decision_request_id.clone(),
            retry_seq: self.retry_seq,
            transport_attempt: self.transport_attempt,
            request_digest: self.request_digest.clone(),
            response_digest: self.response_digest.clone(),
            artifact_digest: Digest32::default(),
        };
        identity.artifact_digest = h_v1(
            COGNITION_RESPONSE_ARTIFACT_IDENTITY_DOMAIN,
            &identity.canonical_payload(),
        );
        identity
    }

    pub fn response_artifact_identity_payload(&self) -> Result<Value, CognitionError> {
        serde_json::to_value(self.response_artifact_identity()).map_err(|error| {
            CognitionError::new(
                "response_artifact_identity_encoding_failed",
                error.to_string(),
            )
        })
    }
}

impl<A, Q> ContinuousAgentResponseContextV1<A, Q>
where
    A: for<'de> Deserialize<'de>,
    Q: for<'de> Deserialize<'de>,
{
    pub fn validate_value(value: &Value) -> Result<(), CognitionError> {
        const ALLOWED: &[&str] = &[
            "base_decision_response",
            "context_discriminator",
            "context_version",
            "agent_session_id",
            "agent_turn_id",
            "decision_request_id",
            "retry_seq",
            "transport_attempt",
            "request_digest",
            "response_digest",
        ];
        let Some(object) = value.as_object() else {
            return Err(CognitionError::new(
                "unknown_context_field",
                "continuous-agent response context must be a JSON object",
            ));
        };
        if let Some(field) = object
            .keys()
            .find(|field| !ALLOWED.contains(&field.as_str()))
        {
            return Err(CognitionError::new(
                "unknown_context_field",
                format!("unknown continuous-agent outer field `{field}`"),
            ));
        }
        serde_json::from_value::<Self>(value.clone())
            .map_err(|error| CognitionError::new("unknown_context_field", error.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryContextEntryV1 {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryContextSnapshotV1 {
    pub revision: u64,
    pub entries: Vec<MemoryContextEntryV1>,
    pub scope: String,
    pub digest: String,
}

impl MemoryContextSnapshotV1 {
    pub fn empty(scope: impl Into<String>) -> Self {
        let mut snapshot = Self {
            revision: 0,
            entries: Vec::new(),
            scope: scope.into(),
            digest: String::new(),
        };
        snapshot.digest = snapshot.computed_digest();
        snapshot
    }

    pub fn from_value(value: Value) -> Result<Self, CognitionError> {
        let snapshot: Self = serde_json::from_value(value)
            .map_err(|error| CognitionError::new("memory_snapshot_invalid", error.to_string()))?;
        if snapshot.scope.trim().is_empty() {
            return Err(CognitionError::new(
                "memory_snapshot_invalid",
                "memory scope is required",
            ));
        }
        if snapshot.digest != snapshot.computed_digest() {
            return Err(CognitionError::new(
                "memory_snapshot_digest_mismatch",
                "memory snapshot digest does not match canonical entries",
            ));
        }
        Ok(snapshot)
    }

    pub fn computed_digest(&self) -> String {
        let mut value = serde_json::to_value(self).expect("memory snapshot is serializable");
        value
            .as_object_mut()
            .expect("memory snapshot is an object")
            .remove("digest");
        h_v1("oasis7.cognition.memory-context.v1", &value).0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalSnapshotV1 {
    pub revision: u64,
    pub short_term_summary: String,
    pub long_term_summary: String,
    #[serde(default)]
    pub blocked_reason: Option<String>,
    pub provenance: String,
    pub digest: String,
}

impl GoalSnapshotV1 {
    pub fn empty() -> Self {
        let mut snapshot = Self {
            revision: 0,
            short_term_summary: String::new(),
            long_term_summary: String::new(),
            blocked_reason: None,
            provenance: "harness_projection".to_string(),
            digest: String::new(),
        };
        snapshot.digest = snapshot.computed_digest();
        snapshot
    }

    pub fn from_value(value: Value) -> Result<Self, CognitionError> {
        let snapshot: Self = serde_json::from_value(value)
            .map_err(|error| CognitionError::new("goal_snapshot_invalid", error.to_string()))?;
        if snapshot.digest != snapshot.computed_digest() {
            return Err(CognitionError::new(
                "goal_snapshot_digest_mismatch",
                "goal snapshot digest does not match canonical projection",
            ));
        }
        Ok(snapshot)
    }

    pub fn computed_digest(&self) -> String {
        let mut value = serde_json::to_value(self).expect("goal snapshot is serializable");
        value
            .as_object_mut()
            .expect("goal snapshot is an object")
            .remove("digest");
        h_v1("oasis7.cognition.goal-snapshot.v1", &value).0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContinuousAgentTurnContextV1 {
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: Digest32,
    pub memory_snapshot: MemoryContextSnapshotV1,
    pub goal_snapshot: GoalSnapshotV1,
    #[serde(default)]
    pub continuation: Option<ContinuationProposalV1>,
}

impl ContinuousAgentTurnContextV1 {
    pub fn validate_for_agent(&self, agent_id: &str) -> Result<(), CognitionError> {
        if self.agent_id != agent_id
            || self.agent_session_id.trim().is_empty()
            || self.agent_turn_id.trim().is_empty()
            || self.decision_request_id.trim().is_empty()
            || self.request_digest.as_str().trim().is_empty()
        {
            return Err(CognitionError::new(
                "cognition_context_identity_mismatch",
                "turn context identity does not match the actor",
            ));
        }
        if self.memory_snapshot.digest != self.memory_snapshot.computed_digest()
            || self.goal_snapshot.digest != self.goal_snapshot.computed_digest()
        {
            return Err(CognitionError::new(
                "cognition_context_digest_mismatch",
                "host cognition context contains an invalid projection digest",
            ));
        }
        if let Some(continuation) = &self.continuation {
            continuation.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationBudgetV1 {
    pub unit: String,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeConditionSubjectV1 {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeConditionV1 {
    pub schema_version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_tick: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<WakeConditionSubjectV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_or_rule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_value_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationProposalV1 {
    pub schema_version: u16,
    pub continuation_proposal_id: String,
    pub world_id: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub origin_turn_id: String,
    pub origin_request_digest: String,
    pub action_or_plan_kind: String,
    #[serde(default)]
    pub action_or_envelope_digest: Option<String>,
    pub remaining_budget: ContinuationBudgetV1,
    pub baseline_observation_digest: String,
    pub goal_digest: String,
    pub policy_digest: String,
    pub policy_revision: u64,
    pub precondition_summary: String,
    pub precondition_digest: String,
    pub wake_conditions: Vec<WakeConditionV1>,
    #[serde(default)]
    pub valid_until_tick: Option<u64>,
    pub source: String,
    pub proposal_digest: String,
}

impl ContinuationProposalV1 {
    /// Complete simulator-owned admission DTO. Runtime still makes the
    /// authoritative admission decision and assigns durable continuation and
    /// wake identities.
    pub fn runtime_admission_payload(&self) -> Result<Value, CognitionError> {
        self.validate()?;
        serde_json::to_value(self).map_err(|error| {
            CognitionError::new("continuation_admission_encoding_failed", error.to_string())
        })
    }

    pub fn runtime_admission_bytes(&self) -> Result<Vec<u8>, CognitionError> {
        let payload = self.runtime_admission_payload()?;
        encode_canonical_cbor(&payload).map_err(|error| {
            CognitionError::new("continuation_admission_encoding_failed", error.to_string())
        })
    }

    pub fn runtime_admission_digest(&self) -> Result<Digest32, CognitionError> {
        self.proposal_digest()
    }

    pub fn proposal_digest(&self) -> Result<Digest32, CognitionError> {
        let mut value = serde_json::to_value(self).map_err(|error| {
            CognitionError::new("continuation_canonical_encoding_failed", error.to_string())
        })?;
        let object = value
            .as_object_mut()
            .expect("continuation proposal is an object");
        object.remove("proposal_digest");
        if let Some(wakes) = object
            .get_mut("wake_conditions")
            .and_then(Value::as_array_mut)
        {
            for wake in wakes {
                if let Some(wake) = wake.as_object_mut() {
                    for field in [
                        "logical_tick",
                        "event_digest",
                        "receipt_id",
                        "subject",
                        "path_or_rule",
                        "operator",
                        "expected_value_bytes",
                    ] {
                        wake.entry(field).or_insert(Value::Null);
                    }
                }
            }
        }
        Ok(h_v1(CONTINUATION_PROPOSAL_DOMAIN, &value))
    }

    pub fn validate(&self) -> Result<(), CognitionError> {
        if self.schema_version != 1 {
            return Err(CognitionError::new(
                "continuation_schema_invalid",
                "unsupported proposal version",
            ));
        }
        for (name, value) in [
            ("continuation_proposal_id", &self.continuation_proposal_id),
            ("world_id", &self.world_id),
            ("agent_id", &self.agent_id),
            ("agent_session_id", &self.agent_session_id),
            ("agent_turn_id", &self.agent_turn_id),
            ("decision_request_id", &self.decision_request_id),
            ("origin_turn_id", &self.origin_turn_id),
            ("origin_request_digest", &self.origin_request_digest),
            ("action_or_plan_kind", &self.action_or_plan_kind),
            (
                "baseline_observation_digest",
                &self.baseline_observation_digest,
            ),
            ("goal_digest", &self.goal_digest),
            ("policy_digest", &self.policy_digest),
            ("precondition_digest", &self.precondition_digest),
            ("source", &self.source),
            ("proposal_digest", &self.proposal_digest),
        ] {
            if value.trim().is_empty() {
                return Err(CognitionError::new(
                    "continuation_binding_invalid",
                    format!("{name} is required"),
                ));
            }
        }
        if !matches!(self.remaining_budget.unit.as_str(), "steps" | "ticks")
            || self.remaining_budget.value == 0
        {
            return Err(CognitionError::new(
                "continuation_budget_invalid",
                "continuation budget must be a positive steps or ticks value",
            ));
        }
        validate_wake_conditions(&self.wake_conditions)?;
        if self.proposal_digest.as_str() != self.proposal_digest()?.as_str() {
            return Err(CognitionError::new(
                "continuation_digest_mismatch",
                "continuation proposal digest does not match canonical fields",
            ));
        }
        Ok(())
    }
}

fn validate_wake_conditions(conditions: &[WakeConditionV1]) -> Result<(), CognitionError> {
    if conditions.is_empty() {
        return Err(CognitionError::new(
            "wake_conditions_empty",
            "continuation requires a bounded non-empty wake condition list",
        ));
    }
    if conditions.len() > MAX_WAKE_CONDITIONS {
        return Err(CognitionError::new(
            "continuation_wake_invalid",
            "continuation wake condition list exceeds its bound",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    let mut previous: Option<Vec<u8>> = None;
    for condition in conditions {
        let valid = match condition.kind.as_str() {
            "at_or_after_tick" => {
                condition.logical_tick.is_some()
                    && condition.event_digest.is_none()
                    && condition.receipt_id.is_none()
                    && condition.subject.is_none()
                    && condition.path_or_rule.is_none()
                    && condition.operator.is_none()
                    && condition.expected_value_bytes.is_none()
            }
            "world_event_committed" => {
                condition
                    .event_digest
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
                    && condition.logical_tick.is_none()
                    && condition.receipt_id.is_none()
                    && condition.subject.is_none()
                    && condition.path_or_rule.is_none()
                    && condition.operator.is_none()
                    && condition.expected_value_bytes.is_none()
            }
            "receipt_linked" => {
                condition
                    .receipt_id
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
                    && condition.logical_tick.is_none()
                    && condition.event_digest.is_none()
                    && condition.subject.is_none()
                    && condition.path_or_rule.is_none()
                    && condition.operator.is_none()
                    && condition.expected_value_bytes.is_none()
            }
            "state_predicate" => {
                condition.logical_tick.is_none()
                    && condition.event_digest.is_none()
                    && condition.receipt_id.is_none()
                    && condition.subject.is_some()
                    && condition
                        .path_or_rule
                        .as_ref()
                        .is_some_and(|value| !value.is_empty())
                    && condition
                        .operator
                        .as_ref()
                        .is_some_and(|value| !value.is_empty())
                    && condition
                        .expected_value_bytes
                        .as_ref()
                        .is_some_and(|value| value.len() <= 512)
            }
            _ => false,
        };
        if condition.schema_version != "wake-condition.v1" || !valid {
            return Err(CognitionError::new(
                "continuation_wake_invalid",
                "invalid wake condition",
            ));
        }
        let bytes = encode_canonical_cbor(condition).expect("wake condition is canonicalizable");
        if bytes.len() > MAX_WAKE_ITEM_BYTES || !seen.insert(bytes.clone()) {
            return Err(CognitionError::new(
                "continuation_wake_invalid",
                "duplicate or oversized wake condition",
            ));
        }
        if previous.as_ref().is_some_and(|prior| prior > &bytes) {
            return Err(CognitionError::new(
                "continuation_wake_invalid",
                "wake conditions must be sorted by canonical bytes",
            ));
        }
        total += bytes.len();
        if total > MAX_WAKE_LIST_BYTES {
            return Err(CognitionError::new(
                "continuation_wake_invalid",
                "continuation wake condition list is oversized",
            ));
        }
        previous = Some(bytes);
    }
    Ok(())
}

fn default_schema_version() -> u16 {
    1
}

fn valid_blake3_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
