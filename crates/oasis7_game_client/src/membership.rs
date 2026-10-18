//! Membership observations with private authentication constructors.
use crate::{
    authority,
    verified_world_view::{ReadEvidencePolicy, VerifiedWorldView},
};
use oasis7_client_api::world_service::*;
#[derive(Debug, Clone)]
pub struct AuthenticatedMembershipObservation {
    response: ReadMembershipResponse,
}
impl AuthenticatedMembershipObservation {
    /// Authenticate an engineering membership assertion for the exact signed
    /// observation challenge using the configured service trust key.
    pub fn verify_service_assertion(
        response: SignedServiceResponse<ReadMembershipResponse>,
        request: &SignedReadRequest<MembershipReadEnvelope>,
        trusted_service_public_key: &str,
        policy: ReadEvidencePolicy,
    ) -> Result<Self, String> {
        const PATH: &str = "/v1/world/governance-membership";
        request.request.validate().map_err(|e| e.to_string())?;
        authority::verify_read_request(PATH, request)?;
        authority::verify_service_response(
            PATH,
            &authority::request_digest(PATH, request)?,
            &response,
            trusted_service_public_key,
        )?;
        Self::from_signed_transport(response.payload, &request.request.query, policy)
    }
    /// Read membership solely from an already qualified public projection.
    pub fn read_verified_view<P: crate::formal_read::FormalReadProjection>(
        view: &VerifiedWorldView<P>,
        request: &ReadMembershipRequest,
    ) -> Result<Self, String> {
        Self::from_formal_view(view, request)
    }
    /// Formal observations come exclusively from a privately verified public
    /// projection. A signed membership endpoint alone cannot create this value.
    pub(crate) fn from_formal_view<P: crate::formal_read::FormalReadProjection>(
        view: &VerifiedWorldView<P>,
        request: &ReadMembershipRequest,
    ) -> Result<Self, String> {
        if view.formal_commit().is_none() || view.version().visibility_scope != "public" {
            return Err("formal membership requires independently proved public projection".into());
        }
        let projection = view.projection().read_metadata()?;
        let body = projection
            .formal_commit_header
            .as_ref()
            .and_then(|header| header.governance_membership.as_ref())
            .ok_or("formal governance body unavailable")?;
        if body.schema_version != 1
            || authority::request_digest(
                "governance-membership-registry-v1",
                &body.effective_registry,
            )? != body.effective_registry_digest
        {
            return Err("formal governance schema/registry commitment mismatch".into());
        }
        let response = ReadMembershipResponse {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            execution_evidence_scope: None,
            commit: view.version().commit.clone(),
            state_root_hash: body.state_root_hash.clone(),
            governance_epoch: body.governance_epoch,
            history_profile: "formal_controlled_single_authority_v1".into(),
            candidates: body.candidates.clone(),
            selected_candidates: body.selected_candidates.clone(),
            effective_registry: body.effective_registry.clone(),
            effective_registry_digest: body.effective_registry_digest.clone(),
            supports_safe_runtime_update: false,
            applied_consensus_membership: None,
        };
        // Exact minimum ancestry was already certified by the formal view
        // verifier; prototype binding equality cannot reject legal governance.
        if request
            .min_commit
            .as_ref()
            .is_some_and(|minimum| !view.formal_minimum_matches(minimum))
        {
            return Err("formal membership minimum was not certified by this exact view".into());
        }
        let mut shape = request.clone();
        shape.min_commit = None;
        response.validate(&shape).map_err(|e| e.to_string())?;
        Ok(Self { response })
    }
    /// Only the signed transport can construct this service assertion.
    pub(crate) fn from_signed_transport(
        response: ReadMembershipResponse,
        request: &ReadMembershipRequest,
        policy: ReadEvidencePolicy,
    ) -> Result<Self, String> {
        if !policy.accepts(response.execution_evidence_scope.as_deref()) {
            return Err("membership execution scope is not explicitly allowed".into());
        }
        let engineering_scope = response.execution_evidence_scope.as_deref()
            == Some(crate::verified_world_view::CONTROLLED_PREREQUISITE_READ_SCOPE);
        let engineering_profile = response.history_profile
            == crate::verified_world_view::CONTROLLED_PREREQUISITE_READ_SCOPE;
        // Formal membership has no authenticated profile contract in this
        // implementation. Admit one defined diagnostic pair, never arbitrary
        // unscoped strings by exclusion of known engineering labels.
        if !engineering_scope || !engineering_profile {
            return Err("membership history scope/profile pair is unsupported".into());
        }
        response
            .validate(request)
            .map_err(|error| error.to_string())?;
        if response.supports_safe_runtime_update || response.applied_consensus_membership.is_some()
        {
            return Err("membership runtime installation evidence is unsupported".into());
        }
        if authority::request_digest(
            "governance-membership-registry-v1",
            &response.effective_registry,
        )? != response.effective_registry_digest
        {
            return Err("membership registry digest mismatch".into());
        }
        Ok(Self { response })
    }
    pub fn response(&self) -> &ReadMembershipResponse {
        &self.response
    }
}
