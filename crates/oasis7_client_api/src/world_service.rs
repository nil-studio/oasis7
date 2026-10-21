//! Pure WorldService contracts; no transport, world execution or proof verification.
//!
//! The deterministic-world-execution PRD distinguishes acceptance from a committed
//! receipt and assigns governing rules at the committed execution block. A client
//! compatibility declaration cannot select those rules. These DTOs describe that
//! boundary; `validate` checks structure, never authenticity or finality.
//! Generic payloads retain their existing signing authority. Envelope metadata is
//! not signed by wrapping a payload, and cannot grant new retry or world authority.

use serde::{Deserialize, Serialize};
pub mod entry_control;
pub mod entry_executor;
pub mod participant_discovery;

pub const WORLD_SERVICE_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractError(pub &'static str);

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ContractError {}

fn required(value: &str) -> Result<(), ContractError> {
    if value.trim().is_empty() {
        Err(ContractError("empty required identifier"))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldIdentity {
    pub world_id: String,
    pub genesis_digest: String,
}
impl WorldIdentity {
    pub fn validate(&self) -> Result<(), ContractError> {
        required(&self.world_id)?;
        required(&self.genesis_digest)
    }
}

/// Opaque authority references are retained rather than inferred from topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionBinding {
    pub provider_world_id: String,
    pub branch_id: String,
    pub finality_ref: String,
    pub reorg_generation: u64,
    pub governing_manifest_ref: String,
    pub authority_generation: u64,
    pub permission_generation: u64,
}
impl ExecutionBinding {
    pub fn validate(&self) -> Result<(), ContractError> {
        for value in [
            &self.provider_world_id,
            &self.branch_id,
            &self.finality_ref,
            &self.governing_manifest_ref,
        ] {
            required(value)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitRef {
    pub world: WorldIdentity,
    pub binding: ExecutionBinding,
    pub position: u64,
    pub execution_block_hash: String,
    pub state_root_ref: String,
}
impl CommitRef {
    pub fn validate(&self) -> Result<(), ContractError> {
        self.world.validate()?;
        self.binding.validate()?;
        required(&self.execution_block_hash)?;
        required(&self.state_root_ref)
    }
    /// Numerical comparison is valid only in the same explicitly bound history.
    pub fn satisfies_minimum(&self, minimum: &Self) -> Result<bool, ContractError> {
        self.validate()?;
        minimum.validate()?;
        if self.world != minimum.world || self.binding != minimum.binding {
            return Err(ContractError("incompatible world or execution binding"));
        }
        if self.position == minimum.position && self != minimum {
            return Err(ContractError("conflicting commit references"));
        }
        Ok(self.position >= minimum.position)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventCursor {
    pub stream_id: String,
    pub scope_id: String,
    pub era: u64,
    pub sequence: u64,
    pub commit: CommitRef,
}
impl EventCursor {
    pub fn validate(&self) -> Result<(), ContractError> {
        required(&self.stream_id)?;
        required(&self.scope_id)?;
        self.commit.validate()
    }
    pub fn validate_continuation(&self, previous: &Self) -> Result<(), ContractError> {
        self.validate()?;
        previous.validate()?;
        if self.stream_id != previous.stream_id
            || self.scope_id != previous.scope_id
            || self.era != previous.era
        {
            return Err(ContractError("cursor requires resynchronization"));
        }
        if self.sequence < previous.sequence || !self.commit.satisfies_minimum(&previous.commit)? {
            return Err(ContractError("cursor moved backwards"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionVersion {
    pub commit: CommitRef,
    pub projection_revision: String,
    pub visibility_scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestKey {
    pub world: WorldIdentity,
    /// Must be resolved by the authenticating adapter; a bare player ID is insufficient.
    pub verified_subject: String,
    pub operation_domain: String,
    /// The existing nonce domain must be preserved, including cross-operation scope.
    pub nonce_scope: String,
    pub request_id_or_nonce: String,
}
impl RequestKey {
    pub fn validate(&self) -> Result<(), ContractError> {
        self.world.validate()?;
        for value in [
            &self.verified_subject,
            &self.operation_domain,
            &self.nonce_scope,
            &self.request_id_or_nonce,
        ] {
            required(value)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestCorrelation {
    pub key: RequestKey,
    /// Not part of the key: different content at the same key is a conflict.
    pub payload_digest: String,
}
impl RequestCorrelation {
    pub fn validate(&self) -> Result<(), ContractError> {
        self.key.validate()?;
        required(&self.payload_digest)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    StableLookup,
    IdempotentRetry,
    CommittedView,
    MinimumCommit,
    FixedHistoricalView,
    BoundedChanges,
    GovernanceMembership,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribeWorldRequest {
    pub contract_version: u32,
    pub expected_world: WorldIdentity,
    pub trust_config_ref: String,
}
impl DescribeWorldRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.contract_version)?;
        self.expected_world.validate()?;
        required(&self.trust_config_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldAvailability {
    pub readable: bool,
    pub writable: bool,
    pub recovering: bool,
    pub reason: Option<WorldServiceErrorKind>,
    pub retry_after_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribeWorldResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_profile: Option<FormalReadProfileV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_evidence_scope: Option<String>,
    pub contract_version: u32,
    pub world: WorldIdentity,
    pub binding: ExecutionBinding,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    pub availability: WorldAvailability,
    pub current_view: Option<ProjectionVersion>,
}
impl DescribeWorldResponse {
    pub fn validate(&self, expected: &WorldIdentity) -> Result<(), ContractError> {
        validate_version(self.contract_version)?;
        self.world.validate()?;
        expected.validate()?;
        self.binding.validate()?;
        if &self.world != expected {
            return Err(ContractError("unexpected world"));
        }
        if let Some(view) = &self.current_view {
            validate_projection(view)?;
            if view.commit.world != self.world || view.commit.binding != self.binding {
                return Err(ContractError("view identity or binding mismatch"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitIntentRequest<T> {
    pub contract_version: u32,
    pub correlation: RequestCorrelation,
    pub deadline_unix_ms: Option<u64>,
    /// An existing signed payload, unchanged; this envelope adds no signature.
    pub signed_payload: T,
}
impl<T> SubmitIntentRequest<T> {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.contract_version)?;
        self.correlation.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupIntentRequest {
    pub contract_version: u32,
    pub key: RequestKey,
}
impl LookupIntentRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.contract_version)?;
        self.key.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionDurability {
    Volatile,
    Durable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IntentOutcome<T> {
    Received { durability: AdmissionDurability },
    Pending,
    Committed { commit: Box<CommitRef>, receipt: T },
    Rejected { reason: WorldServiceErrorKind },
    Expired,
    Unknown,
    HistoryUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentResponse<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_profile: Option<FormalReadProfileV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_evidence_scope: Option<String>,
    pub contract_version: u32,
    pub correlation: RequestCorrelation,
    pub outcome: IntentOutcome<T>,
}
impl<T> IntentResponse<T> {
    pub fn validate(&self, expected: &RequestCorrelation) -> Result<(), ContractError> {
        validate_version(self.contract_version)?;
        self.correlation.validate()?;
        expected.validate()?;
        if &self.correlation != expected {
            return Err(ContractError("request correlation conflict"));
        }
        if let IntentOutcome::Committed { commit, .. } = &self.outcome {
            commit.validate()?;
            if commit.world != expected.key.world {
                return Err(ContractError("receipt world mismatch"));
            }
        }
        Ok(())
    }
}

/// A local observation, never an authoritative intent lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitObservation<T> {
    Response(Box<IntentResponse<T>>),
    OutcomeUnknown(RequestCorrelation),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadWorldViewRequest {
    /// Fresh authenticated observation challenge for the formal read profile.
    /// Omitted legacy requests retain their original signing bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_nonce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_anchor: Option<FormalReadAnchorRefV1>,
    pub contract_version: u32,
    pub world: WorldIdentity,
    pub scope_id: String,
    pub min_commit: Option<CommitRef>,
    pub fixed_commit: Option<CommitRef>,
    pub deadline_unix_ms: Option<u64>,
}
impl ReadWorldViewRequest {
    /// Structural validation only. Formal ancestry and minimum bindings must
    /// additionally be established by an independently anchored proof consumer.
    pub fn validate_formal_shape(&self) -> Result<(), ContractError> {
        if self.observation_nonce.as_ref().is_none_or(|nonce| {
            nonce.len() != 64
                || !nonce
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            return Err(ContractError(
                "formal read requires bounded observation nonce",
            ));
        }
        if self.formal_anchor.is_none() {
            return Err(ContractError("formal read requires caller anchor"));
        }
        let mut shape = self.clone();
        shape.min_commit = None;
        shape.validate()?;
        if let Some(minimum) = &self.min_commit {
            minimum.validate()?;
            if minimum.world != self.world {
                return Err(ContractError("read world mismatch"));
            }
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.observation_nonce.as_ref().is_some_and(|nonce| {
            nonce.len() != 64
                || !nonce
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            return Err(ContractError("invalid read observation nonce"));
        }
        validate_version(self.contract_version)?;
        self.world.validate()?;
        required(&self.scope_id)?;
        if let Some(anchor) = &self.formal_anchor {
            anchor.validate()?;
        }
        for commit in [&self.min_commit, &self.fixed_commit].into_iter().flatten() {
            commit.validate()?;
            if commit.world != self.world {
                return Err(ContractError("read world mismatch"));
            }
        }
        if let (Some(fixed), Some(minimum)) = (&self.fixed_commit, &self.min_commit)
            && !fixed.satisfies_minimum(minimum)?
        {
            return Err(ContractError("fixed commit below minimum"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadWorldViewResponse<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_profile: Option<FormalReadProfileV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_proof: Option<FormalReadSidecarV1>,
    pub contract_version: u32,
    /// Signed engineering evidence claim; absent retains the ordinary wire contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_evidence_scope: Option<String>,
    pub version: ProjectionVersion,
    /// Logical world time is never a commit position or event sequence.
    pub logical_tick: u64,
    pub continuation: EventCursor,
    pub view: T,
}
impl<T> ReadWorldViewResponse<T> {
    pub fn validate(&self, request: &ReadWorldViewRequest) -> Result<(), ContractError> {
        request.validate()?;
        validate_version(self.contract_version)?;
        validate_projection(&self.version)?;
        self.continuation.validate()?;
        if self.version.commit.world != request.world
            || self.version.visibility_scope != request.scope_id
            || self.continuation.scope_id != request.scope_id
            || self.continuation.commit != self.version.commit
        {
            return Err(ContractError("view and continuation mismatch"));
        }
        if let Some(minimum) = &request.min_commit
            && !self.version.commit.satisfies_minimum(minimum)?
        {
            return Err(ContractError("read not caught up"));
        }
        if let Some(fixed) = &request.fixed_commit
            && &self.version.commit != fixed
        {
            return Err(ContractError("fixed view mismatch"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadWorldChangesRequest {
    pub contract_version: u32,
    pub cursor: EventCursor,
    pub max_items: u32,
    pub max_bytes: u64,
}
impl ReadWorldChangesRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.contract_version)?;
        self.cursor.validate()?;
        if self.max_items == 0 || self.max_bytes == 0 {
            return Err(ContractError("zero changes limit"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldChange<T> {
    pub cursor: EventCursor,
    pub change: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadWorldChangesResponse<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_profile: Option<FormalReadProfileV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_evidence_scope: Option<String>,
    pub contract_version: u32,
    pub changes: Vec<WorldChange<T>>,
    pub next_cursor: EventCursor,
}
impl<T> ReadWorldChangesResponse<T> {
    /// The transport separately enforces encoded byte bounds and authorization.
    pub fn validate(&self, request: &ReadWorldChangesRequest) -> Result<(), ContractError> {
        request.validate()?;
        validate_version(self.contract_version)?;
        if self.changes.len() > request.max_items as usize {
            return Err(ContractError("changes item limit exceeded"));
        }
        let mut previous = &request.cursor;
        for event in &self.changes {
            event.cursor.validate_continuation(previous)?;
            if event.cursor.sequence <= previous.sequence {
                return Err(ContractError("unordered changes"));
            }
            previous = &event.cursor;
        }
        self.next_cursor.validate_continuation(previous)?;
        if self.changes.is_empty() && self.next_cursor != request.cursor {
            return Err(ContractError("empty batch advanced cursor"));
        }
        if !self.changes.is_empty() && &self.next_cursor != previous {
            return Err(ContractError("next cursor differs from last change"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldServiceErrorKind {
    UnsupportedContract,
    UnsupportedCapability,
    Unauthorized,
    Conflict,
    Unavailable,
    AssuranceUnavailable,
    ReadNotCaughtUp,
    HistoryUnavailable,
    CursorExpired,
    ResyncRequired,
}

fn validate_version(version: u32) -> Result<(), ContractError> {
    if version != WORLD_SERVICE_CONTRACT_VERSION {
        Err(ContractError("unsupported contract version"))
    } else {
        Ok(())
    }
}
fn validate_projection(version: &ProjectionVersion) -> Result<(), ContractError> {
    version.commit.validate()?;
    required(&version.projection_revision)?;
    required(&version.visibility_scope)
}

#[path = "world_service_membership.rs"]
mod membership;
pub use membership::*;
#[path = "world_service_formal_governance.rs"]
mod formal_governance;
pub use formal_governance::*;
#[path = "world_service_formal.rs"]
mod formal;
pub use formal::*;
#[path = "world_service_formal_result.rs"]
mod formal_result;
pub use formal_result::*;

/// Authentication codecs shared by remote clients and runtime adapters.
mod authority;
pub use authority::*;

pub const DESCRIBE_PATH: &str = "/v1/world/describe";
pub const SUBMIT_PATH: &str = "/v1/world/submit";
pub const LOOKUP_PATH: &str = "/v1/world/lookup";
pub const VIEW_PATH: &str = "/v1/world/view";
pub const CHANGES_PATH: &str = "/v1/world/changes";

/// The signature binds the operation domain and every request field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedReadRequest<T> {
    pub request: T,
    pub subject_public_key: String,
    pub signature_hex: String,
}

/// Verified against configured service trust, not a caller-supplied key.
/// This authenticates a service assertion; consensus proofs remain separate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedServiceResponse<T> {
    pub request_digest: String,
    pub payload: T,
    pub signature_hex: String,
}

pub mod private_goal;

pub mod private_goal_read;

pub mod task_context;

pub mod client_state;

/// Explicit normalized-JSON projection profile; independent signature domain.
pub const VIEW_JSON_PATH: &str = "/v1/world/view-json";

pub mod task_factory_quote;
pub mod task_recipe_quote;
