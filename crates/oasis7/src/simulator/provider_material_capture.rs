//! Native provider transport material capture contracts.
//!
//! This seam carries only content identities across the actor boundary. The
//! owner of the independent pending store supplies the durable implementation
//! and must bind it to a verified new-format world before installing it.

use std::io::Read;

use oasis7_proto::memory_budget::MemoryBudget;

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
    pub parent_root_set: [u8; 32],
    pub authority_snapshot: [u8; 32],
    pub pending_protection_root: [u8; 32],
}

impl ProviderMaterialOwnerBindingV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.world_id.trim().is_empty() {
            return Err("provider pending owner world id is empty");
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
    pub request_object_id: [u8; 32],
    pub request_context_object_id: [u8; 32],
    pub response_object_id: [u8; 32],
    pub response_context_object_id: [u8; 32],
    pub response_status: u16,
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
}
