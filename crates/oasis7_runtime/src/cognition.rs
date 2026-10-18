//! Runtime-owned validation and delivery primitives for agent cognition.
//!
//! This is deliberately a small boundary type.  It does not invoke a provider,
//! mutate `World`, or grant authority.  Provider adapters and durable journal
//! integration build on this boundary in later slices.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use unicode_normalization::UnicodeNormalization;

use super::cognition_validation::{validate_precondition_shape, validate_preconditions};
use super::util::hash_json;
use super::world::World;

pub const AGENT_DECISION_ENVELOPE_V1_SCHEMA: &str = "agent-decision-envelope.v1";
pub(crate) const MAX_IDENTIFIER_BYTES: usize = 128;
pub(crate) const MAX_PATH_BYTES: usize = 128;
pub(crate) const MAX_EXPECTED_VALUE_BYTES: usize = 512;
pub(crate) const MAX_PRECONDITIONS: usize = 32;
pub(crate) const MAX_PRECONDITION_BYTES: usize = 4096;
const DIGEST_HEX_BYTES: usize = 64;

const WORLD_STATE_HASH_DOMAIN_V1: &str = "oasis7.runtime.world-state.v1";
const RUNTIME_MANIFEST_HASH_DOMAIN_V1: &str = "oasis7.runtime.manifest.v1";
const AUTHORITY_CONTEXT_HASH_DOMAIN_V1: &str = "oasis7.runtime.authority-context.v1";

pub(crate) fn is_canonical_identifier(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && !value.chars().any(char::is_control)
        && value.nfc().collect::<String>() == value
}

/// Typed disposition reason for a cognition commit that cannot be applied.
///
/// The external wire code is kept stable for existing callers, while Runtime
/// owns the classification so consumers do not parse diagnostic text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionCommitRejectReasonV1 {
    StaleBase,
}

impl CognitionCommitRejectReasonV1 {
    /// Stable external rejection code used by feedback and terminal events.
    pub const fn code(self) -> &'static str {
        match self {
            Self::StaleBase => "stale_base",
        }
    }
}

/// Classify a Runtime cognition commit error without exposing error-text
/// parsing to an integration boundary.
///
/// The legacy wrapped spellings remain accepted for persisted/replayed errors;
/// new commit paths emit the canonical bare `stale_base` code.
pub fn classify_cognition_commit_error(
    error: &super::error::WorldError,
) -> Option<CognitionCommitRejectReasonV1> {
    let super::error::WorldError::DistributedValidationFailed { reason } = error else {
        return None;
    };
    matches!(
        reason.as_str(),
        "stale_base"
            | "cognition validation failed: stale_base"
            | "cognition validation failed: cognition_base_binding_stale"
            | "cognition validation failed: cognition_world_head_mismatch"
    )
    .then_some(CognitionCommitRejectReasonV1::StaleBase)
}

#[derive(Debug, Serialize)]
struct WorldStateHashInput<'a> {
    world_id: &'a str,
    branch_id: &'a str,
    finality_epoch: u64,
    finality_block_hash: Option<&'a str>,
    finality_status: &'a str,
    logical_tick: u64,
    state_root: &'a str,
    reorg_epoch: u64,
    runtime_manifest_hash: &'a str,
}

#[derive(Debug, Serialize)]
struct DecisionDigestInput<'a> {
    request_digest: &'a str,
    decision_kind: &'a str,
    action: &'a JsonValue,
}

#[derive(Debug, Serialize)]
struct EnvelopeDigestInput<'a> {
    request_digest: &'a str,
    decision_digest: &'a str,
    action: &'a JsonValue,
    world_id: &'a str,
    agent_id: &'a str,
    agent_session_id: &'a str,
    agent_turn_id: &'a str,
    decision_request_id: &'a str,
    retry_seq: u64,
    branch_id: &'a str,
    finality_epoch: u64,
    finality_block_hash: Option<&'a str>,
    finality_status: &'a str,
    finality_binding_digest: &'a str,
    base_tick: u64,
    base_world_hash: &'a str,
    reorg_epoch: u64,
    runtime_manifest_hash: &'a str,
    capability_snapshot_hash: &'a str,
    authority_context_hash: &'a str,
    issued_at_tick: u64,
    valid_until_tick: u64,
    preconditions: &'a [PreconditionV1],
    origin_intent_ref: &'a Option<JsonValue>,
}

#[derive(Debug, Serialize)]
struct EnvelopeIdempotencyInput<'a> {
    request_digest: &'a str,
    envelope_digest: &'a str,
}

#[derive(Debug, Serialize)]
struct FinalityBindingInput<'a> {
    schema_version: u16,
    branch_id: &'a str,
    finality_epoch: u64,
    finality_block_hash: Option<&'a str>,
    finality_status: &'a str,
    reorg_epoch: u64,
}

fn h_v1<T: Serialize>(domain: &str, payload: &T) -> String {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(domain, payload))
        .expect("cognition identity payload must be canonicalizable");
    format!("blake3:{}", blake3::hash(&bytes))
}

pub(crate) fn finality_binding_digest_v1(
    branch_id: &str,
    finality_epoch: u64,
    finality_block_hash: Option<&str>,
    finality_status: &str,
    reorg_epoch: u64,
) -> String {
    h_v1(
        "oasis7.runtime.finality-binding.v1",
        &FinalityBindingInput {
            schema_version: 1,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            reorg_epoch,
        },
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "This hash preimage is a stable replay/persistence contract; grouping fields would change call-site/API shape."
)]
pub(crate) fn world_state_binding_digest_v1(
    world_id: &str,
    branch_id: &str,
    finality_epoch: u64,
    finality_block_hash: Option<&str>,
    finality_status: &str,
    logical_tick: u64,
    state_root: &str,
    reorg_epoch: u64,
    runtime_manifest_hash: &str,
) -> String {
    h_v1(
        WORLD_STATE_HASH_DOMAIN_V1,
        &WorldStateHashInput {
            world_id,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            logical_tick,
            state_root,
            reorg_epoch,
            runtime_manifest_hash,
        },
    )
}

pub(crate) use oasis7_agent_api::world_cognition::finality_binding_is_legal;

fn world_has_cognition_binding(world: &World) -> bool {
    world.chain_resource_manifest().world_id != "unbound"
        || world.latest_tick_consensus_record().is_some()
        || !world.state().agents.is_empty()
        || !world.state().agent_intent_ledger.is_empty()
        || !world.capability_grants_v2().is_empty()
        || !world.capability_invocation_contexts().is_empty()
        || !world
            .capability_revocation_state()
            .authority_records
            .is_empty()
}

/// A subject selector used by a bounded runtime precondition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PreconditionSubjectV1 {
    pub kind: String,
    pub id: String,
}

/// A single, fail-closed, all-of MVCC precondition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PreconditionV1 {
    pub schema_version: u32,
    pub subject: PreconditionSubjectV1,
    pub path_or_rule: String,
    pub operator: String,
    pub expected_value_bytes: Vec<u8>,
    pub missing_behavior: String,
}

/// The v1 decision result crossing back into the runtime.
///
/// `action` remains a JSON value at this seam because the simulator and
/// runtime currently expose different legacy Action wire encodings.  Kernel
/// action decoding is intentionally a later, host-authoritative step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentDecisionEnvelopeV1 {
    pub schema_version: String,
    pub world_id: String,
    pub agent_id: String,
    pub branch_id: String,
    pub finality_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finality_block_hash: Option<String>,
    pub finality_status: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub retry_seq: u64,
    pub base_tick: u64,
    pub base_world_hash: String,
    pub reorg_epoch: u64,
    pub runtime_manifest_hash: String,
    pub capability_snapshot_hash: String,
    pub authority_context_hash: String,
    pub observation_digest: String,
    pub context_digest: String,
    pub issued_at_tick: u64,
    pub valid_until_tick: u64,
    pub preconditions: Vec<PreconditionV1>,
    pub decision_kind: String,
    pub action: JsonValue,
    pub request_digest: String,
    pub decision_digest: String,
    pub envelope_digest: String,
    pub provider_invocation_key: String,
    pub envelope_idempotency_key: String,
    #[serde(default)]
    pub origin_intent_ref: Option<JsonValue>,
    pub source: String,
}

impl AgentDecisionEnvelopeV1 {
    /// Derive the runtime-owned provider invocation binding from the
    /// Agent-owned request digest.  The request digest itself remains opaque
    /// at this boundary; only this downstream binding is runtime-defined.
    pub fn derive_provider_invocation_key(&self) -> String {
        Self::provider_invocation_key_for_request(&self.request_digest)
    }

    /// Public opaque request binding shared by capture and response dispatch.
    pub fn provider_invocation_key_for_request(request_digest: &str) -> String {
        h_v1("oasis7.cognition.provider-invocation.v1", &request_digest)
    }

    /// Derive the canonical decision binding for the request and action.
    pub fn derive_decision_digest(&self) -> String {
        h_v1(
            "oasis7.cognition.decision.v1",
            &DecisionDigestInput {
                request_digest: &self.request_digest,
                decision_kind: &self.decision_kind,
                action: &self.action,
            },
        )
    }

    /// Derive the trusted finality tuple binding carried by this envelope.
    pub fn derive_finality_binding_digest(&self) -> String {
        finality_binding_digest_v1(
            &self.branch_id,
            self.finality_epoch,
            self.finality_block_hash.as_deref(),
            &self.finality_status,
            self.reorg_epoch,
        )
    }

    /// Derive the complete runtime envelope identity from its canonical
    /// fields.  `envelope_digest` and `envelope_idempotency_key` are excluded
    /// from their own inputs by construction.
    pub fn derive_envelope_digest(&self) -> String {
        h_v1(
            "oasis7.cognition.envelope.v1",
            &EnvelopeDigestInput {
                request_digest: &self.request_digest,
                decision_digest: &self.decision_digest,
                action: &self.action,
                world_id: &self.world_id,
                agent_id: &self.agent_id,
                agent_session_id: &self.agent_session_id,
                agent_turn_id: &self.agent_turn_id,
                decision_request_id: &self.decision_request_id,
                retry_seq: self.retry_seq,
                branch_id: &self.branch_id,
                finality_epoch: self.finality_epoch,
                finality_block_hash: self.finality_block_hash.as_deref(),
                finality_status: &self.finality_status,
                finality_binding_digest: &self.derive_finality_binding_digest(),
                base_tick: self.base_tick,
                base_world_hash: &self.base_world_hash,
                reorg_epoch: self.reorg_epoch,
                runtime_manifest_hash: &self.runtime_manifest_hash,
                capability_snapshot_hash: &self.capability_snapshot_hash,
                authority_context_hash: &self.authority_context_hash,
                issued_at_tick: self.issued_at_tick,
                valid_until_tick: self.valid_until_tick,
                preconditions: &self.preconditions,
                origin_intent_ref: &self.origin_intent_ref,
            },
        )
    }

    /// Derive the exactly-once idempotency key from the request and envelope
    /// digests.
    pub fn derive_envelope_idempotency_key(&self) -> String {
        h_v1(
            "oasis7.cognition.envelope-idempotency.v1",
            &EnvelopeIdempotencyInput {
                request_digest: &self.request_digest,
                envelope_digest: &self.envelope_digest,
            },
        )
    }
}

/// A stable runtime validation error.  Codes are part of the narrow P0.2
/// contract; details are intentionally omitted so callers cannot branch on
/// unstable diagnostic prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitionValidationError {
    code: &'static str,
}

impl CognitionValidationError {
    pub(crate) fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub fn code(&self) -> &str {
        self.code
    }
}

impl fmt::Display for CognitionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for CognitionValidationError {}

/// The result returned by validation/submission and bounded mailbox enqueue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCognitionDisposition {
    disposition: &'static str,
    provider_invocation_count: u64,
}

impl AgentCognitionDisposition {
    fn pending() -> Self {
        Self {
            disposition: "pending",
            provider_invocation_count: 0,
        }
    }

    pub fn disposition(&self) -> &str {
        self.disposition
    }

    pub fn provider_invocation_count(&self) -> u64 {
        self.provider_invocation_count
    }
}

/// Runtime MVCC/idempotency boundary for cognition envelopes.
///
/// The validator owns only in-process request admission in this slice.  It
/// never invokes a provider and never writes to `World`; durable persistence is
/// intentionally reserved for the journal/commit slice.
#[derive(Debug, Default)]
pub struct MvccValidator {
    idempotency: BTreeMap<String, String>,
    dispositions: BTreeMap<String, AgentCognitionDisposition>,
}

impl MvccValidator {
    /// Validate an envelope without changing the world.
    pub fn validate(
        world: &World,
        envelope: &AgentDecisionEnvelopeV1,
    ) -> Result<(), CognitionValidationError> {
        Self::validate_shape(envelope)?;
        Self::validate_mvcc(world, envelope)
    }

    fn validate_mvcc(
        world: &World,
        envelope: &AgentDecisionEnvelopeV1,
    ) -> Result<(), CognitionValidationError> {
        // This order is part of the wire contract.  The World is read once
        // per check and is never mutated or synchronously coupled to a
        // provider call.  An unbound bootstrap World remains an explicit
        // compatibility lane; a bound World must pass every comparison below.
        validate_request_binding(envelope)?;
        validate_base_head(world, envelope)?;
        validate_validity_window(world, envelope)?;
        validate_runtime_identity(world, envelope)?;
        validate_capability_context(world, envelope)?;
        validate_origin_intent(world, envelope)?;
        validate_preconditions_against_world(world, envelope)?;
        validate_derived_identity(world, envelope)
    }

    /// Admit an envelope and remember its disposition for same-key retries.
    /// Same key + same digest is a replay; same key + another digest fails
    /// closed.  No world action, effect, or provider call is performed.
    pub fn submit(
        &mut self,
        world: &mut World,
        envelope: AgentDecisionEnvelopeV1,
    ) -> Result<AgentCognitionDisposition, CognitionValidationError> {
        // Keep idempotency lookup between identity and world-head checks.  A
        // previously committed key must replay its disposition even if a
        // later transport copy carries stale observation metadata.
        Self::validate_shape(&envelope)?;
        let key = envelope.envelope_idempotency_key.clone();
        let digest = envelope.envelope_digest.clone();
        if let Some(existing_digest) = self.idempotency.get(&key) {
            if existing_digest != &digest {
                return Err(CognitionValidationError::new("idempotency_conflict"));
            }
            return Ok(self
                .dispositions
                .get(&key)
                .cloned()
                .unwrap_or_else(AgentCognitionDisposition::pending));
        }

        Self::validate_mvcc(world, &envelope)?;
        let disposition = AgentCognitionDisposition::pending();
        self.idempotency.insert(key.clone(), digest);
        self.dispositions.insert(key, disposition.clone());
        Ok(disposition)
    }

    /// Validate only the wire shape of a precondition.  This helper is public
    /// so Agent/simulator adapters can reject malformed entries before a World
    /// is available; value evaluation remains a Runtime operation.
    pub fn validate_precondition_shape(
        condition: &PreconditionV1,
    ) -> Result<(), CognitionValidationError> {
        validate_precondition_shape(condition)
    }

    fn validate_shape(envelope: &AgentDecisionEnvelopeV1) -> Result<(), CognitionValidationError> {
        if envelope.schema_version != AGENT_DECISION_ENVELOPE_V1_SCHEMA {
            return Err(CognitionValidationError::new("unsupported_schema_version"));
        }
        for value in [
            (&envelope.world_id, "world_id"),
            (&envelope.agent_id, "agent_id"),
            (&envelope.branch_id, "branch_id"),
            (&envelope.agent_session_id, "agent_session_id"),
            (&envelope.agent_turn_id, "agent_turn_id"),
            (&envelope.decision_request_id, "decision_request_id"),
            (&envelope.decision_kind, "decision_kind"),
            (&envelope.source, "source"),
        ] {
            validate_identifier(value.0, value.1)?;
        }
        if envelope.issued_at_tick > envelope.valid_until_tick {
            return Err(CognitionValidationError::new("invalid_validity_window"));
        }
        for value in [
            &envelope.base_world_hash,
            &envelope.runtime_manifest_hash,
            &envelope.capability_snapshot_hash,
            &envelope.authority_context_hash,
            &envelope.observation_digest,
            &envelope.context_digest,
            &envelope.request_digest,
            &envelope.decision_digest,
            &envelope.envelope_digest,
            &envelope.provider_invocation_key,
            &envelope.envelope_idempotency_key,
        ] {
            if value.trim().is_empty() || value.len() > 256 {
                return Err(CognitionValidationError::new("invalid_digest"));
            }
        }
        validate_finality_binding(
            envelope.finality_status.as_str(),
            envelope.finality_block_hash.as_deref(),
        )?;
        if envelope.action.is_null() {
            return Err(CognitionValidationError::new("invalid_action"));
        }
        Ok(())
    }
}

/// A nonblocking bounded queue with one active turn per agent.
#[derive(Debug, Clone)]
pub struct AgentCognitionMailbox {
    capacity: usize,
    per_agent_capacity: usize,
    queue: VecDeque<AgentDecisionEnvelopeV1>,
    active_agents: BTreeSet<String>,
}

impl AgentCognitionMailbox {
    pub fn with_capacity(capacity: usize, per_agent_capacity: usize) -> Self {
        Self {
            capacity,
            per_agent_capacity,
            queue: VecDeque::new(),
            active_agents: BTreeSet::new(),
        }
    }

    /// Try to enqueue without waiting, invoking a provider, or touching a
    /// World.  Full and reentrant queues return immediately with stable codes.
    pub fn try_enqueue(
        &mut self,
        envelope: AgentDecisionEnvelopeV1,
    ) -> Result<AgentCognitionDisposition, CognitionValidationError> {
        MvccValidator::validate_shape(&envelope)?;
        validate_preconditions(envelope.preconditions.as_slice())?;
        if self.per_agent_capacity == 0 || self.active_agents.contains(envelope.agent_id.as_str()) {
            return Err(CognitionValidationError::new("agent_busy"));
        }
        if self.queue.len() >= self.capacity {
            return Err(CognitionValidationError::new("mailbox_full"));
        }
        self.active_agents.insert(envelope.agent_id.clone());
        self.queue.push_back(envelope);
        Ok(AgentCognitionDisposition::pending())
    }

    pub fn try_dequeue(&mut self) -> Option<AgentDecisionEnvelopeV1> {
        let envelope = self.queue.pop_front()?;
        self.active_agents.remove(envelope.agent_id.as_str());
        Some(envelope)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

fn validate_runtime_identity(
    world: &World,
    envelope: &AgentDecisionEnvelopeV1,
) -> Result<(), CognitionValidationError> {
    let manifest = world.chain_resource_manifest();
    if manifest.world_id != "unbound" && manifest.world_id != envelope.world_id {
        return Err(CognitionValidationError::new("invalid_identity"));
    }
    if !matches!(
        envelope.finality_status.as_str(),
        "pending" | "verified" | "reorged" | "suspended"
    ) {
        return Err(CognitionValidationError::new("recovery_pending"));
    }

    let authorities = &world.capability_revocation_state().authority_records;
    if let Some(first) = authorities.values().next() {
        // A capability authority record is the existing durable branch and
        // finality source.  More than one disagreeing record is an upstream
        // contradiction, not a reason to choose an arbitrary record.
        let consistent = authorities.values().all(|record| {
            record.world_id == first.world_id
                && record.branch_id == first.branch_id
                && record.finality_epoch == first.finality_epoch
                && record.finality_block_hash == first.finality_block_hash
                && record.finality_status == first.finality_status
        });
        if !consistent {
            return Err(CognitionValidationError::new("recovery_pending"));
        }
        if first.world_id != envelope.world_id || first.branch_id != envelope.branch_id {
            return Err(CognitionValidationError::new("reorg_invalidated"));
        }
        // Governance stores `finalized`; the cognition wire uses `verified`
        // for the same finalized anchor.  No other status is interchangeable.
        if !((first.finality_status == "finalized" && envelope.finality_status == "verified")
            || first.finality_status == envelope.finality_status)
        {
            return Err(CognitionValidationError::new("reorg_invalidated"));
        }
        if first.finality_epoch != envelope.finality_epoch
            || envelope
                .finality_block_hash
                .as_deref()
                .is_some_and(|hash| hash != first.finality_block_hash)
        {
            return Err(CognitionValidationError::new("reorg_invalidated"));
        }
    } else if let Some(record) = world.latest_tick_consensus_record() {
        // Tick consensus has no branch field, so it can anchor the epoch and
        // block only.  Branch identity remains supplied by a capability
        // authority record; accepting a guessed branch here would weaken the
        // reorg boundary.
        if record.block.header.epoch != envelope.finality_epoch
            || envelope
                .finality_block_hash
                .as_deref()
                .is_some_and(|hash| hash != record.certificate.block_hash)
        {
            return Err(CognitionValidationError::new("reorg_invalidated"));
        }
    }

    let intent_reorg_epochs = world
        .read_retained_reorg_epoch_summary()
        .map_err(|_| CognitionValidationError::new("recovery_pending"))?;
    if intent_reorg_epochs.has_multiple() {
        return Err(CognitionValidationError::new("recovery_pending"));
    }
    if let Some(reorg_epoch) = intent_reorg_epochs.single_epoch()
        && reorg_epoch != envelope.reorg_epoch
    {
        return Err(CognitionValidationError::new("reorg_invalidated"));
    }

    // An empty World is the explicitly supported bootstrap compatibility lane.
    // Once any versioned world/agent/finality state is present, identity is
    // checked against that state rather than inferred from envelope text.
    let bound = world_has_cognition_binding(world);
    if bound
        && !world.state().agents.is_empty()
        && !world.state().agents.contains_key(&envelope.agent_id)
    {
        return Err(CognitionValidationError::new("invalid_identity"));
    }
    if !bound
        && (envelope.finality_epoch > envelope.base_tick
            || envelope.reorg_epoch > envelope.base_tick)
    {
        return Err(CognitionValidationError::new("reorg_invalidated"));
    }
    Ok(())
}

fn validate_finality_binding(
    status: &str,
    block_hash: Option<&str>,
) -> Result<(), CognitionValidationError> {
    finality_binding_is_legal(status, block_hash)
        .then_some(())
        .ok_or_else(|| CognitionValidationError::new("recovery_pending"))
}

fn validate_request_binding(
    envelope: &AgentDecisionEnvelopeV1,
) -> Result<(), CognitionValidationError> {
    if !valid_digest(&envelope.request_digest) {
        return Err(CognitionValidationError::new("invalid_digest"));
    }
    // Runtime treats request_digest as an opaque, Agent-owned verified binding.
    // It does not re-hash prompt, memory, goal, or response fields here.
    if !is_canonical_identifier(&envelope.agent_session_id, MAX_IDENTIFIER_BYTES)
        || !is_canonical_identifier(&envelope.agent_turn_id, MAX_IDENTIFIER_BYTES)
        || !is_canonical_identifier(&envelope.decision_request_id, MAX_IDENTIFIER_BYTES)
    {
        return Err(CognitionValidationError::new("invalid_identity"));
    }
    Ok(())
}

fn validate_derived_identity(
    world: &World,
    envelope: &AgentDecisionEnvelopeV1,
) -> Result<(), CognitionValidationError> {
    if !world_has_cognition_binding(world) {
        return Ok(());
    }
    let provider_key = envelope.derive_provider_invocation_key();
    if envelope.provider_invocation_key != provider_key {
        return Err(CognitionValidationError::new("invalid_digest"));
    }
    let decision_digest = envelope.derive_decision_digest();
    if envelope.decision_digest != decision_digest {
        return Err(CognitionValidationError::new("invalid_digest"));
    }
    let envelope_digest = envelope.derive_envelope_digest();
    if envelope.envelope_digest != envelope_digest {
        return Err(CognitionValidationError::new("invalid_digest"));
    }
    let idempotency_key = envelope.derive_envelope_idempotency_key();
    if envelope.envelope_idempotency_key != idempotency_key {
        return Err(CognitionValidationError::new("invalid_digest"));
    }
    Ok(())
}

fn validate_base_head(
    world: &World,
    envelope: &AgentDecisionEnvelopeV1,
) -> Result<(), CognitionValidationError> {
    if !valid_digest(&envelope.base_world_hash) {
        return Err(CognitionValidationError::new("stale_base"));
    }
    // A candidate cannot refer to a parent after the envelope's own validity
    // horizon.  This catches malformed/future parent positions without a
    // sentinel value or a process-local epoch ceiling.
    if envelope.base_tick > envelope.valid_until_tick {
        return Err(CognitionValidationError::new("stale_base"));
    }

    let bound = world_has_cognition_binding(world);
    if !bound {
        return Ok(());
    }
    if envelope.base_tick != world.state().time {
        return Err(CognitionValidationError::new("stale_base"));
    }
    // Classify an old-fork response before recomputing its visible root.
    let intent_reorg_epochs = world
        .read_retained_reorg_epoch_summary()
        .map_err(|_| CognitionValidationError::new("recovery_pending"))?;
    if intent_reorg_epochs.has_multiple() {
        return Err(CognitionValidationError::new("recovery_pending"));
    }
    if let Some(authoritative_reorg_epoch) = intent_reorg_epochs.single_epoch()
        && authoritative_reorg_epoch != envelope.reorg_epoch
    {
        return Err(CognitionValidationError::new("reorg_invalidated"));
    }
    if let Some(authoritative_reorg_epoch) = world
        .cognition()
        .get("runtime_binding")
        .and_then(JsonValue::as_object)
        .and_then(|binding| binding.get("reorg_epoch"))
        .and_then(JsonValue::as_u64)
        && authoritative_reorg_epoch != envelope.reorg_epoch
    {
        return Err(CognitionValidationError::new("reorg_invalidated"));
    }
    let state_root = world
        .current_state_root_hash()
        .map_err(|_| CognitionValidationError::new("recovery_pending"))?;
    let runtime_manifest_hash = world
        .current_manifest_hash()
        .map_err(|_| CognitionValidationError::new("recovery_pending"))?;
    let expected_manifest = h_v1(RUNTIME_MANIFEST_HASH_DOMAIN_V1, &runtime_manifest_hash);
    if envelope.runtime_manifest_hash != expected_manifest {
        return Err(CognitionValidationError::new("stale_capability_snapshot"));
    }
    let expected = world_state_binding_digest_v1(
        envelope.world_id.as_str(),
        envelope.branch_id.as_str(),
        envelope.finality_epoch,
        envelope.finality_block_hash.as_deref(),
        envelope.finality_status.as_str(),
        world.state().time,
        state_root.as_str(),
        envelope.reorg_epoch,
        runtime_manifest_hash.as_str(),
    );
    if envelope.base_world_hash != expected {
        return Err(CognitionValidationError::new("stale_base"));
    }
    Ok(())
}

fn validate_validity_window(
    world: &World,
    envelope: &AgentDecisionEnvelopeV1,
) -> Result<(), CognitionValidationError> {
    if world.state().time > envelope.valid_until_tick {
        return Err(CognitionValidationError::new("expired"));
    }
    Ok(())
}

fn validate_capability_context(
    world: &World,
    envelope: &AgentDecisionEnvelopeV1,
) -> Result<(), CognitionValidationError> {
    if !valid_digest(&envelope.capability_snapshot_hash)
        || !valid_digest(&envelope.authority_context_hash)
    {
        return Err(CognitionValidationError::new("stale_capability_snapshot"));
    }

    let state = world.capability_revocation_state();
    let bound = !state.authority_records.is_empty()
        || !world.capability_grants_v2().is_empty()
        || !world.capability_invocation_contexts().is_empty();
    if !bound {
        return Ok(());
    }

    let root = world.capability_authorization_root();
    let root_binding = h_v1(AUTHORITY_CONTEXT_HASH_DOMAIN_V1, &root);
    if envelope.capability_snapshot_hash != root
        && envelope.capability_snapshot_hash != h_v1(RUNTIME_MANIFEST_HASH_DOMAIN_V1, &root)
    {
        return Err(CognitionValidationError::new("stale_capability_snapshot"));
    }
    let authority_hash =
        hash_json(state).map_err(|_| CognitionValidationError::new("recovery_pending"))?;
    if envelope.authority_context_hash != authority_hash
        && envelope.authority_context_hash != root_binding
        && envelope.authority_context_hash
            != h_v1(AUTHORITY_CONTEXT_HASH_DOMAIN_V1, &authority_hash)
    {
        return Err(CognitionValidationError::new("authority_denied"));
    }
    Ok(())
}

fn validate_origin_intent(
    world: &World,
    envelope: &AgentDecisionEnvelopeV1,
) -> Result<(), CognitionValidationError> {
    let Some(reference) = envelope.origin_intent_ref.as_ref() else {
        return Ok(());
    };
    let Some(object) = reference.as_object() else {
        return Err(CognitionValidationError::new("intent_conflict"));
    };
    let Some(intent_id) = object.get("intent_id").and_then(JsonValue::as_str) else {
        return Err(CognitionValidationError::new("intent_conflict"));
    };
    let Some(intent) = world.state().agent_intent_ledger.get(intent_id) else {
        return Err(CognitionValidationError::new("intent_conflict"));
    };
    if intent.agent_id != envelope.agent_id
        || intent
            .world_id
            .as_deref()
            .is_some_and(|id| id != envelope.world_id)
        || intent
            .reorg_epoch
            .is_some_and(|epoch| epoch != envelope.reorg_epoch)
    {
        return Err(CognitionValidationError::new("intent_conflict"));
    }
    if let Some(request_digest) = object.get("request_digest").and_then(JsonValue::as_str)
        && (intent.request_digest.is_empty() || request_digest != intent.request_digest)
    {
        return Err(CognitionValidationError::new("intent_conflict"));
    }
    if let Some(world_id) = object.get("world_id").and_then(JsonValue::as_str)
        && world_id != envelope.world_id
    {
        return Err(CognitionValidationError::new("intent_conflict"));
    }
    if let Some(reorg_epoch) = object.get("reorg_epoch").and_then(JsonValue::as_u64)
        && reorg_epoch != envelope.reorg_epoch
    {
        return Err(CognitionValidationError::new("intent_conflict"));
    }
    if let Some(scope) = object.get("authority_scope").and_then(JsonValue::as_str)
        && intent.authority_scope.as_deref() != Some(scope)
    {
        return Err(CognitionValidationError::new("authority_denied"));
    }
    Ok(())
}

fn validate_preconditions_against_world(
    world: &World,
    envelope: &AgentDecisionEnvelopeV1,
) -> Result<(), CognitionValidationError> {
    validate_preconditions(envelope.preconditions.as_slice())?;
    let mut previous: Option<Vec<u8>> = None;
    for condition in &envelope.preconditions {
        let canonical = oasis7_wasm_abi::encode_canonical_cbor(condition)
            .map_err(|_| CognitionValidationError::new("precondition_failed"))?;
        if previous.as_ref().is_some_and(|item| item > &canonical) {
            return Err(CognitionValidationError::new("precondition_failed"));
        }
        previous = Some(canonical);

        let namespace_matches = match condition.path_or_rule.as_str() {
            path if path.starts_with("world.") => {
                condition.subject.kind == "world" && condition.subject.id == envelope.world_id
            }
            path if path.starts_with("agent.") => {
                condition.subject.kind == "agent" && condition.subject.id == envelope.agent_id
            }
            "intent.status" => {
                condition.subject.kind == "intent"
                    && envelope
                        .origin_intent_ref
                        .as_ref()
                        .and_then(|value| value.get("intent_id").and_then(JsonValue::as_str))
                        == Some(condition.subject.id.as_str())
            }
            _ => false,
        };
        if !namespace_matches {
            return Err(CognitionValidationError::new("precondition_failed"));
        }
        let actual = resolve_precondition_value(world, envelope, condition)?;
        compare_precondition(condition, actual)?;
    }
    Ok(())
}

fn resolve_precondition_value(
    world: &World,
    envelope: &AgentDecisionEnvelopeV1,
    condition: &PreconditionV1,
) -> Result<JsonValue, CognitionValidationError> {
    match condition.path_or_rule.as_str() {
        "world.logical_tick" => Ok(JsonValue::from(world.state().time)),
        "world.reorg_epoch" => Ok(JsonValue::from(
            world
                .read_retained_reorg_epoch_summary()
                .map_err(|_| CognitionValidationError::new("recovery_pending"))?
                .max_epoch()
                .unwrap_or(0),
        )),
        "world.state_root" => {
            Ok(JsonValue::String(world.current_state_root_hash().map_err(
                |_| CognitionValidationError::new("recovery_pending"),
            )?))
        }
        "world.runtime_manifest_hash" => {
            Ok(JsonValue::String(world.current_manifest_hash().map_err(
                |_| CognitionValidationError::new("recovery_pending"),
            )?))
        }
        "agent.status" => {
            let cell = world
                .state()
                .agents
                .get(&envelope.agent_id)
                .ok_or_else(|| CognitionValidationError::new("precondition_failed"))?;
            let status = cell
                .activity
                .as_ref()
                .map(|activity| serde_json::to_value(activity.status))
                .transpose()
                .map_err(|_| CognitionValidationError::new("precondition_failed"))?
                .unwrap_or_else(|| JsonValue::String("idle".to_string()));
            Ok(status)
        }
        "agent.position" => {
            let cell = world
                .state()
                .agents
                .get(&envelope.agent_id)
                .ok_or_else(|| CognitionValidationError::new("precondition_failed"))?;
            Ok(JsonValue::Array(vec![
                JsonValue::from(cell.state.pos.x_cm),
                JsonValue::from(cell.state.pos.y_cm),
            ]))
        }
        "agent.inventory_digest" => {
            let cell = world
                .state()
                .agents
                .get(&envelope.agent_id)
                .ok_or_else(|| CognitionValidationError::new("precondition_failed"))?;
            let digest = oasis7_wasm_abi::canonical_hash(&cell.state.body_state.cargo_entries)
                .map_err(|_| CognitionValidationError::new("precondition_failed"))?;
            Ok(JsonValue::String(digest))
        }
        "agent.capability_snapshot_hash" => Ok(JsonValue::String(
            world.capability_authorization_root().to_string(),
        )),
        "intent.status" => world
            .state()
            .agent_intent_ledger
            .get(&condition.subject.id)
            .map(|intent| JsonValue::String(intent.status.clone()))
            .ok_or_else(|| CognitionValidationError::new("precondition_failed")),
        path if path.starts_with("agent.resource.") => {
            let resource = path
                .strip_prefix("agent.resource.")
                .ok_or_else(|| CognitionValidationError::new("precondition_failed"))?;
            let cell = world
                .state()
                .agents
                .get(&envelope.agent_id)
                .ok_or_else(|| CognitionValidationError::new("precondition_failed"))?;
            let amount = match resource {
                "electricity" => cell
                    .state
                    .resources
                    .get(oasis7_agent_api::world_action::ResourceKind::Electricity),
                "data" => cell
                    .state
                    .resources
                    .get(oasis7_agent_api::world_action::ResourceKind::Data),
                _ => return Err(CognitionValidationError::new("precondition_failed")),
            };
            Ok(JsonValue::from(amount))
        }
        _ => Err(CognitionValidationError::new("precondition_failed")),
    }
}

fn compare_precondition(
    condition: &PreconditionV1,
    actual: JsonValue,
) -> Result<(), CognitionValidationError> {
    let is_unsigned = matches!(
        condition.path_or_rule.as_str(),
        "world.logical_tick" | "world.reorg_epoch"
    );
    let is_numeric = is_unsigned || is_supported_resource_path(condition.path_or_rule.as_str());
    if is_numeric {
        let left = if is_unsigned {
            actual.as_u64().map(i128::from)
        } else {
            actual.as_i64().map(i128::from)
        };
        let right = if is_unsigned {
            serde_cbor::from_slice::<u64>(condition.expected_value_bytes.as_slice())
                .ok()
                .map(i128::from)
        } else {
            serde_cbor::from_slice::<i64>(condition.expected_value_bytes.as_slice())
                .ok()
                .map(i128::from)
        };
        let (Some(left), Some(right)) = (left, right) else {
            return Err(CognitionValidationError::new("precondition_failed"));
        };
        let matches = match condition.operator.as_str() {
            "eq" => left == right,
            "neq" => left != right,
            "lt" => left < right,
            "lte" => left <= right,
            "gt" => left > right,
            "gte" => left >= right,
            _ => false,
        };
        if matches {
            return Ok(());
        }
    } else {
        let expected: JsonValue = serde_cbor::from_slice(condition.expected_value_bytes.as_slice())
            .map_err(|_| CognitionValidationError::new("precondition_failed"))?;
        let matches = match condition.operator.as_str() {
            "eq" => actual == expected,
            "neq" => actual != expected,
            _ => false,
        };
        if matches {
            return Ok(());
        }
    }
    Err(CognitionValidationError::new("precondition_failed"))
}

fn valid_digest(value: &str) -> bool {
    let hex = value
        .strip_prefix("blake3:")
        .or_else(|| value.strip_prefix("sha256:"))
        .unwrap_or(value);
    hex.len() == DIGEST_HEX_BYTES
        && hex
            .bytes()
            .all(|character| character.is_ascii_digit() || (b'a'..=b'f').contains(&character))
}

fn validate_identifier(value: &str, _field: &str) -> Result<(), CognitionValidationError> {
    if !is_canonical_identifier(value, MAX_IDENTIFIER_BYTES) {
        return Err(CognitionValidationError::new("invalid_identity"));
    }
    Ok(())
}

pub(crate) fn is_supported_resource_path(path: &str) -> bool {
    matches!(path, "agent.resource.electricity" | "agent.resource.data")
}
