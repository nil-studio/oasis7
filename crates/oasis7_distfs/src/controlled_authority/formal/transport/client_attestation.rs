//! A response header may promote native state only when endpoint key custody
//! attests this exact RPC preimage under the independently pinned current floor.
use super::*;
pub(super) fn verify_response_custody(
    response: &Response,
    authority: &VerifiedAuthorityV1,
    role: EndpointRole,
    response_digest: &str,
) -> Result<(), FormalError> {
    let instance = match role {
        EndpointRole::Primary => &authority.definition().primary.instance,
        EndpointRole::Replica => &authority.definition().replica.instance,
    };
    let attestation = response
        .protected_signature
        .as_ref()
        .ok_or(FormalError::FencingUnproven)?;
    let request = fencing::ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance
            .digest()
            .map_err(super::super::super::endpoint::fencing_error)?,
        checkpoint: response.body.checkpoint.clone(),
        challenge: attestation.body.challenge.clone(),
        purpose: fencing::ProtectedEndpointSignaturePurposeV1::RpcResponse,
        preimage: bytes(RESPONSE_DOMAIN, &response_digest)?,
    };
    let signature = fencing::verify_protected_endpoint_signature(
        attestation,
        &request,
        instance,
        &fencing::TrustedFencingPolicyV1 {
            provider_public_key: authority.policy().fencing_provider_public_key.clone(),
            authorized_resource_bindings: authority
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        },
    )
    .map_err(super::super::super::endpoint::fencing_error)?;
    if signature != response.signature
        || attestation.body.resource_binding != response.body.provider.statement.resource_binding
    {
        return Err(FormalError::FencingUnproven);
    }
    Ok(())
}
