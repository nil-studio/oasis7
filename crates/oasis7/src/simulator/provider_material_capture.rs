//! Native provider transport material capture contracts.
//!
//! This seam carries only content identities across the actor boundary. The
//! owner of the independent pending store supplies the durable implementation
//! and must bind it to a verified new-format world before installing it.

use std::io::Read;

use oasis7_formal_protocol::incremental_state::StateRootV1;
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::any::Any;
use std::fmt;
use std::sync::Arc;

use super::{ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1};

/// Immutable new-format owner scope for a provider pending ledger.
///
/// The native owner creates this only after it has verified the world,
/// genesis, exact parent, authority snapshot, and separate pending-protection
/// root. It is deliberately not derivable from the legacy lineage checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderMaterialOwnerBindingV1 {
    pub world_id: String,
    pub genesis_root: [u8; 32],
    /// Stable identifier for this independently owned pending store.
    pub owner_id: [u8; 32],
}

impl ProviderMaterialOwnerBindingV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.world_id.trim().is_empty() || self.world_id.len() > 256 {
            return Err("provider pending owner world id is empty or too long");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderMaterialAttemptV1 {
    pub owner: ProviderMaterialOwnerBindingV1,
    /// Existing canonical cognition invocation key (`blake3:<hex>`).
    pub operation_id: String,
    pub transport_attempt: u64,
    pub attempt_id: [u8; 32],
    /// Exact authenticated parent and authority for this invocation. These
    /// belong to the attempt, not the long-lived owner scope.
    pub parent_root_set: [u8; 32],
    pub authority_snapshot: [u8; 32],
    pub request_object_id: [u8; 32],
    pub request_context_object_id: [u8; 32],
}

/// References returned only after both the original response body and its
/// typed semantic context have been persisted by the installed owner.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderOriginalMaterialRefsV1 {
    pub owner: ProviderMaterialOwnerBindingV1,
    /// Existing canonical cognition invocation key (`blake3:<hex>`).
    pub operation_id: String,
    pub transport_attempt: u64,
    pub attempt_id: [u8; 32],
    pub parent_root_set: [u8; 32],
    pub authority_snapshot: [u8; 32],
    pub request_object_id: [u8; 32],
    pub request_context_object_id: [u8; 32],
    pub response_object_id: [u8; 32],
    pub response_context_object_id: [u8; 32],
    pub response_status: u16,
}

/// Per-invocation binding returned by the already verified new-format Runtime
/// owner. `RuntimeBindingV1::base_world_hash` is not an exact RootSet and must
/// not be substituted here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderAttemptAuthorityBindingV1 {
    pub parent_root_set: [u8; 32],
    pub authority_snapshot: [u8; 32],
}

/// Evidence must be checked by the installed trusted authority before an
/// unresolved provider attempt can leave the protected set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderDispositionEvidenceV1 {
    pub evidence_object_id: [u8; 32],
    pub disposition_digest: [u8; 32],
}

/// Opaque continuation for a single protected-roots snapshot. Callers can
/// retain and return a cursor produced by the owner, but cannot choose an
/// arbitrary offset that skips unresolved attempts. Owner implementations must
/// downcast to their private cursor type and bind it to the exact root snapshot.
pub struct ProviderPendingRootsCursorV1 {
    state: Box<dyn Any + Send + Sync>,
}

impl ProviderPendingRootsCursorV1 {
    /// Wrap a private owner cursor. The concrete type should remain private to
    /// the owner crate; other implementations cannot fabricate that type.
    #[doc(hidden)]
    pub fn from_owner_state<T: Any + Send + Sync>(state: T) -> Self {
        Self {
            state: Box::new(state),
        }
    }

    /// Access a cursor only when it has the concrete type owned by the caller.
    #[doc(hidden)]
    pub fn owner_state<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.state.downcast_ref::<T>()
    }
}

impl fmt::Debug for ProviderPendingRootsCursorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderPendingRootsCursorV1")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderPendingAttemptPhaseV1 {
    RequestPrepared,
    DispatchedOrUnknown,
    ResponseBodyPlanned,
    ResponseBodyDurable,
    ResponseContextPlanned,
    ResponseCaptured,
    RejectionEvidencePlanned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPendingAttemptSummaryV1 {
    pub attempt: ProviderMaterialAttemptV1,
    pub phase: ProviderPendingAttemptPhaseV1,
    pub response_object_id: Option<[u8; 32]>,
    pub response_context_object_id: Option<[u8; 32]>,
    pub response_status: Option<u16>,
    pub terminal_evidence_object_id: Option<[u8; 32]>,
}

/// Actual roots needed to retain the independent owner's event and index pages.
/// `pending_root` in the page is a cursor binding digest, not a substitute for
/// these Merkle root descriptors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPendingStateRootsV1 {
    pub events: StateRootV1,
    pub pending: StateRootV1,
    pub seen: StateRootV1,
}

#[derive(Debug)]
pub struct ProviderPendingRootsPageV1 {
    pub owner: Arc<ReservedValue<ProviderMaterialOwnerBindingV1>>,
    pub pending_root: [u8; 32],
    /// Charged page of event-record and original-material content IDs. The
    /// cursor bounds this output; callers retain the carrier while using the
    /// roots.
    pub object_ids: Arc<ReservedValue<Vec<[u8; 32]>>>,
    /// Charged unresolved identities and current phase, for recovery to resume
    /// only by same-identity inspection. This is not retry authorization.
    pub attempts: Arc<ReservedValue<Vec<ProviderPendingAttemptSummaryV1>>>,
    /// Charged actual Merkle roots backing the independent pending ledger.
    pub state_roots: Arc<ReservedValue<ProviderPendingStateRootsV1>>,
    /// A continuation exists only when this page has not reached the end of
    /// the bound snapshot. Exhausting these pages proves this store-local root
    /// enumeration is complete; it does not register the owner with a global
    /// World or GC registry.
    pub next_cursor: Option<Arc<ReservedValue<ProviderPendingRootsCursorV1>>>,
}

/// Trusted new-format owner seam. Implementations verify the exact parent and
/// authority for each request and authenticate terminal disposition evidence;
/// they are supplied by the Runtime owner, never reconstructed from a legacy
/// lineage checkpoint or from provider-supplied fields.
#[cfg(not(target_arch = "wasm32"))]
pub trait ProviderMaterialAuthority: Send + Sync {
    fn verify_owner_scope(&self, owner: &ProviderMaterialOwnerBindingV1) -> Result<(), String>;
    fn request_binding(
        &self,
        owner: &ProviderMaterialOwnerBindingV1,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<ProviderAttemptAuthorityBindingV1, String>;
    fn verify_commit(
        &self,
        owner: &ProviderMaterialOwnerBindingV1,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String>;
    fn verify_rejection(
        &self,
        owner: &ProviderMaterialOwnerBindingV1,
        attempt: &ProviderMaterialAttemptV1,
        evidence: ProviderDispositionEvidenceV1,
    ) -> Result<(), String>;
}

/// A finite allowance for on-disk request/response spools. Implementations
/// retain this guard until the HTTP exchange and persistence complete.
pub trait ProviderSpoolReservation: Send {}

/// Native owner injection for exact provider request/response body capture.
///
/// Implementations must durably record a `DispatchedOrUnknown` attempt before
/// `mark_dispatched` returns, store original body bytes under their original
/// flat content ID, and retain pending material until a matching Runtime
/// receipt or an explicit rejection closes the attempt. This trait does not
/// certify a World head, Runtime authority, or an accepted cognition action.
#[cfg(not(target_arch = "wasm32"))]
pub trait ProviderOriginalMaterialStore: Send + Sync {
    /// Verified owner binding and independent pending protection root.
    fn owner_binding(&self) -> &ProviderMaterialOwnerBindingV1;

    /// Current durable protected-root digest. It changes as unresolved
    /// attempts are appended or disposed and is intentionally not part of the
    /// immutable owner scope.
    fn pending_protection_root(&self) -> Result<[u8; 32], String>;

    /// Bounded page over unresolved request/response material. A cursor is
    /// bound to one exact protected-root digest; callers must restart if the
    /// root changes and follow every returned cursor before treating the
    /// store-local enumeration as complete. This is not a global GC registry.
    fn protected_roots_page(
        &self,
        cursor: Option<&ProviderPendingRootsCursorV1>,
        limit: usize,
    ) -> Result<ProviderPendingRootsPageV1, String>;

    /// Check the captured operation against the verified owner binding before
    /// any request bytes are prepared for dispatch.
    fn validate_request_owner(
        &self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<(), String>;

    fn memory_budget(&self) -> MemoryBudget;
    fn request_body_limit_bytes(&self) -> u64;
    fn response_body_limit_bytes(&self) -> u64;

    fn reserve_body_spools(
        &self,
        request_limit: u64,
        response_limit: u64,
    ) -> Result<Box<dyn ProviderSpoolReservation>, String>;

    fn cancelled(&self) -> bool;

    /// Persist the typed request context and exact outgoing JSON body as the
    /// durable intent before returning the identity used by later phases.
    fn persist_request(
        &self,
        request: &ContinuousAgentRequestContextV1,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<ProviderMaterialAttemptV1, String>;

    /// Durably mark this attempt before handing the request to HTTP.
    fn mark_dispatched(&self, attempt: &ProviderMaterialAttemptV1) -> Result<(), String>;

    /// Store the exact decoded HTTP entity body before any typed parsing.
    fn persist_response_body(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<[u8; 32], String>;

    /// Persist the typed context as a separate semantic object only after the
    /// original response body is durable.
    fn finish_response(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        response_object_id: [u8; 32],
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<ProviderOriginalMaterialRefsV1, String>;

    /// Verify the captured request/response identity before consuming it in a
    /// Runtime provider-action commit.
    fn verify_commit_materials(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        request: &ContinuousAgentRequestContextV1,
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<(), String>;

    /// Append the Runtime receipt identity after the action commit succeeds.
    fn mark_committed(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String>;

    /// Close only with authority-verified evidence of a terminal rejection.
    /// A transport error or missing typed response is not such evidence.
    fn mark_rejected(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        evidence: ProviderDispositionEvidenceV1,
    ) -> Result<(), String>;
}
