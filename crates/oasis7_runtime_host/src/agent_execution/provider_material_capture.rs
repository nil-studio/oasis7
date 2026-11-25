//! Native Host authority and store interfaces for original provider materials.
//!
//! Portable material identities and protected-root DTOs live in
//! `oasis7_formal_protocol::provider_material`; Host traits bind them to
//! in-process request/response streams and memory budgets.
#[cfg(not(target_arch = "wasm32"))]
use oasis7_agent_api::ContinuousAgentRequestContextV1;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_proto::memory_budget::MemoryBudget;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Read;

#[cfg(not(target_arch = "wasm32"))]
type ContinuousAgentResponseContextV1 = oasis7_agent_api::ContinuousAgentResponseContextV1<
    oasis7_simulator::Action,
    oasis7_simulator::AgentQuery,
>;

pub use oasis7_formal_protocol::provider_material::{
    ProviderAttemptAuthorityBindingV1, ProviderDispositionEvidenceV1, ProviderMaterialAttemptV1,
    ProviderMaterialOwnerBindingV1, ProviderOriginalMaterialRefsV1, ProviderPendingAttemptPhaseV1,
    ProviderPendingAttemptSummaryV1, ProviderPendingRootsCursorV1, ProviderPendingRootsPageV1,
    ProviderPendingStateRootsV1,
};

/// Trusted new-format owner seam supplied by the Runtime owner.
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

/// Finite allowance for on-disk request/response spools retained through persistence.
#[cfg(not(target_arch = "wasm32"))]
pub trait ProviderSpoolReservation: Send {}

/// Native owner injection for exact provider request/response body capture.
#[cfg(not(target_arch = "wasm32"))]
pub trait ProviderOriginalMaterialStore: Send + Sync {
    fn owner_binding(&self) -> &ProviderMaterialOwnerBindingV1;

    fn pending_protection_root(&self) -> Result<[u8; 32], String>;

    fn protected_roots_page(
        &self,
        cursor: Option<&ProviderPendingRootsCursorV1>,
        limit: usize,
    ) -> Result<ProviderPendingRootsPageV1, String>;

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

    fn persist_request(
        &self,
        request: &ContinuousAgentRequestContextV1,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<ProviderMaterialAttemptV1, String>;

    fn mark_dispatched(&self, attempt: &ProviderMaterialAttemptV1) -> Result<(), String>;

    fn persist_response_body(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<[u8; 32], String>;

    fn finish_response(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        response_object_id: [u8; 32],
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<ProviderOriginalMaterialRefsV1, String>;

    fn verify_commit_materials(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        request: &ContinuousAgentRequestContextV1,
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<(), String>;

    fn verify_commit_disposition(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String>;

    fn mark_committed(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String>;

    fn mark_rejected(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        evidence: ProviderDispositionEvidenceV1,
    ) -> Result<(), String>;
}
