//! Legacy Host authority and store interfaces for original provider materials.
//! The immutable material identities and protected-root DTOs now have a
//! canonical WorldService owner. Reexports preserve exact historical types.
#[cfg(not(target_arch = "wasm32"))]
use super::{ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1};
#[cfg(not(target_arch = "wasm32"))]
use oasis7_proto::memory_budget::MemoryBudget;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Read;

pub use oasis7_world_service::provider_material::{
    ProviderAttemptAuthorityBindingV1, ProviderDispositionEvidenceV1, ProviderMaterialAttemptV1,
    ProviderMaterialOwnerBindingV1, ProviderOriginalMaterialRefsV1, ProviderPendingAttemptPhaseV1,
    ProviderPendingAttemptSummaryV1, ProviderPendingRootsCursorV1, ProviderPendingRootsPageV1,
    ProviderPendingStateRootsV1,
};

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
#[cfg(not(target_arch = "wasm32"))]
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

    /// Read-only receipt fence before any finalization side effects. A terminal
    /// attempt must match its durable receipt/action; a captured attempt must
    /// remain unresolved until the later explicit `mark_committed` call.
    fn verify_commit_disposition(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
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
