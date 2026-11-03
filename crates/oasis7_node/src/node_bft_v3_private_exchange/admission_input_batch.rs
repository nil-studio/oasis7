//! Explicit transport-v2 authority interfaces. Subject PoP is not read authority.
pub use super::batch_wire::{
    BatchAuthorizationV2 as NodeBftAdmissionInputBatchAuthorizationV2,
    BatchOperationV2 as NodeBftAdmissionInputBatchOperationV2,
    BatchReadV2 as NodeBftAdmissionInputBatchReadRequestV2,
    VerifiedBatchSubjectV2 as NodeBftVerifiedAdmissionInputBatchSubjectV2,
};
use super::*;
pub struct NodeBftAdmissionInputBatchScopeV2<'a> {
    pub view: NodeBftAdmissionInputReadViewV1<'a>,
    pub subject: &'a NodeBftVerifiedAdmissionInputBatchSubjectV2,
}
pub struct NodeBftAdmissionInputPartScopeV2<'a> {
    pub view: NodeBftAdmissionInputReadViewV1<'a>,
    pub subject: &'a NodeBftVerifiedAdmissionInputBatchSubjectV2,
    pub part_index: u32,
}
pub trait NodeBftAdmissionInputBatchCredentialsV2: Send + Sync {
    /// Obtain the independently issued exact original grant and sign this v2
    /// phase/offset/count. A V1 signature is never an implementation fallback.
    fn authorize(
        &self,
        request: &NodeBftAdmissionInputBatchReadRequestV2,
        original_deadline: Instant,
    ) -> Result<NodeBftAdmissionInputBatchAuthorizationV2, String>;
}
pub trait NodeBftAdmissionInputBatchReadGateV2: Send + Sync {
    fn validate_identity(
        &self,
        authorization: &NodeBftAdmissionInputBatchAuthorizationV2,
    ) -> Result<(), String>;
    /// Fresh fixed issuer/witness/revocation/provider checks both before/after
    /// exactly one actual whole response callback. This is not a child frame.
    fn authorize_batch(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        scope: &NodeBftAdmissionInputBatchScopeV2<'_>,
        response: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String>;
    /// Repeat actual fresh checks for this part of the verified signed range.
    /// The callback returns (), not fabricated child signatures or wire bytes.
    fn authorize_part(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        scope: &NodeBftAdmissionInputPartScopeV2<'_>,
        inspect: &mut dyn FnMut(Instant) -> Result<(), String>,
    ) -> Result<(), String>;
}
